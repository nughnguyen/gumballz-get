//! Piece order for watching a torrent while it downloads.
//!
//! A common torrent downloads the rarest pieces first, which is great for the
//! health of the swarm and terrible for watching: the file only opens when it
//! finishes. For streaming the order is different, and the trade-off is explicit
//! — worse for the swarm, usable for whoever is waiting.
//!
//! The decision of **which piece to ask for now** is pure and testable. Talking
//! to `librqbit` and writing to disk is I/O.
//!
//! Origin: rqbit, plus qBittorrent's first-and-last-piece.

use serde::Serialize;

/// How many pieces ahead of the read position we keep guaranteed.
///
/// Eight: enough for the player not to stop at every seek, without stalling the
/// whole download waiting for too large a window.
pub const READAHEAD_PIECES: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Without this the player cannot even open the file.
    Critical,
    /// Inside the read window.
    High,
    /// The rest, in order.
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamPlan {
    pub total_pieces: u32,
    /// The piece the current playback position requires.
    pub playhead_piece: u32,
}

/// Priority of a piece, given the plan.
///
/// The first and last pieces are critical because the container keeps the header
/// at the start and, in unoptimized MP4, the index (`moov`) at the end. Without
/// both, the player cannot even find out the duration, let alone seek.
pub fn priority_of(plan: &StreamPlan, piece: u32) -> Priority {
    if plan.total_pieces == 0 {
        return Priority::Normal;
    }
    let last = plan.total_pieces - 1;

    if piece == 0 || piece == last {
        return Priority::Critical;
    }
    if piece >= plan.playhead_piece && piece < plan.playhead_piece + READAHEAD_PIECES {
        return Priority::High;
    }
    Priority::Normal
}

/// The order in which to request the pieces that are still missing.
///
/// Sequential from the playhead, with the critical ones first. What was left
/// behind goes to the end: playback already passed through there, and it only
/// matters to whoever goes back.
pub fn request_order(plan: &StreamPlan, have: &[bool]) -> Vec<u32> {
    if plan.total_pieces == 0 {
        return Vec::new();
    }
    let missing = |p: &u32| have.get(*p as usize).copied() != Some(true);

    let last = plan.total_pieces - 1;
    let mut order: Vec<u32> = Vec::new();
    let mut queued = std::collections::HashSet::new();
    let push = |order: &mut Vec<u32>, queued: &mut std::collections::HashSet<u32>, p: u32| {
        if missing(&p) && queued.insert(p) {
            order.push(p);
        }
    };

    for critica in [0, last] {
        push(&mut order, &mut queued, critica);
    }
    for p in plan.playhead_piece..plan.total_pieces {
        push(&mut order, &mut queued, p);
    }
    for p in 0..plan.playhead_piece {
        push(&mut order, &mut queued, p);
    }
    order
}

/// Can playback start?
///
/// Requires both critical pieces plus the read window. Releasing earlier makes
/// the player open and stall right after, which is worse than waiting — the user
/// reads it as a broken app, not as a download in progress.
pub fn can_start_playback(plan: &StreamPlan, have: &[bool]) -> bool {
    if plan.total_pieces == 0 {
        return false;
    }
    let last = plan.total_pieces - 1;
    let tem = |p: u32| have.get(p as usize).copied() == Some(true);

    if !tem(0) || !tem(last) {
        return false;
    }
    let fim_janela = (plan.playhead_piece + READAHEAD_PIECES).min(plan.total_pieces);
    (plan.playhead_piece..fim_janela).all(tem)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(total: u32, playhead: u32) -> StreamPlan {
        StreamPlan {
            total_pieces: total,
            playhead_piece: playhead,
        }
    }

    #[test]
    fn primeira_e_ultima_peca_sao_criticas() {
        // Header at the start, moov index at the end. Without both, the player
        // cannot even find out the duration.
        let p = plan(100, 50);
        assert_eq!(priority_of(&p, 0), Priority::Critical);
        assert_eq!(priority_of(&p, 99), Priority::Critical);
    }

    #[test]
    fn a_janela_a_frente_do_playhead_e_alta() {
        let p = plan(100, 50);
        assert_eq!(priority_of(&p, 50), Priority::High);
        assert_eq!(priority_of(&p, 57), Priority::High);
        assert_eq!(priority_of(&p, 58), Priority::Normal, "outside the window");
        assert_eq!(priority_of(&p, 49), Priority::Normal, "already passed");
    }

    #[test]
    fn pede_as_criticas_antes_de_qualquer_coisa() {
        let p = plan(10, 5);
        let have = vec![false; 10];
        let ordem = request_order(&p, &have);
        assert_eq!(&ordem[..2], &[0, 9], "critical first: {ordem:?}");
        assert_eq!(ordem[2], 5, "then sequential from the playhead");
    }

    #[test]
    fn nao_pede_de_novo_o_que_ja_tem() {
        let p = plan(6, 2);
        let mut have = vec![false; 6];
        have[0] = true;
        have[5] = true;
        have[3] = true;
        let ordem = request_order(&p, &have);
        assert_eq!(ordem, vec![2, 4, 1], "{ordem:?}");
    }

    #[test]
    fn o_que_ficou_para_tras_vai_para_o_fim() {
        // Playback already passed through there; it only matters to whoever goes back.
        let p = plan(8, 5);
        let have = vec![false; 8];
        let ordem = request_order(&p, &have);
        let pos_de = |x: u32| ordem.iter().position(|&y| y == x).unwrap();
        assert!(pos_de(6) < pos_de(1), "ahead before behind: {ordem:?}");
        assert!(pos_de(6) < pos_de(4));
    }

    #[test]
    fn nao_libera_a_reproducao_cedo_demais() {
        // Opening and stalling right after is worse than waiting: the user reads
        // it as a broken app, not as a download in progress.
        let p = plan(100, 0);
        let mut have = vec![false; 100];
        have[0] = true;
        assert!(
            !can_start_playback(&p, &have),
            "the first piece alone is not enough"
        );

        have[99] = true;
        assert!(!can_start_playback(&p, &have), "the read window is missing");

        for i in 0..READAHEAD_PIECES {
            have[i as usize] = true;
        }
        assert!(can_start_playback(&p, &have));
    }

    #[test]
    fn seek_para_o_meio_exige_a_janela_de_la() {
        // The point of seeking in a partial file: having the start helps in no
        // way if the user jumped to the middle.
        let mut have = vec![false; 100];
        have[0] = true;
        have[99] = true;
        for h in have.iter_mut().take(20) {
            *h = true;
        }
        assert!(can_start_playback(&plan(100, 0), &have));
        assert!(
            !can_start_playback(&plan(100, 60), &have),
            "without the middle window"
        );

        for h in have.iter_mut().take(68).skip(60) {
            *h = true;
        }
        assert!(can_start_playback(&plan(100, 60), &have));
    }

    #[test]
    fn torrent_de_uma_peca_so_nao_quebra() {
        let p = plan(1, 0);
        assert_eq!(priority_of(&p, 0), Priority::Critical);
        assert_eq!(request_order(&p, &[false]), vec![0]);
        assert!(can_start_playback(&p, &[true]));
    }

    #[test]
    fn plano_vazio_nao_libera_nem_estoura() {
        let p = plan(0, 0);
        assert!(request_order(&p, &[]).is_empty());
        assert!(!can_start_playback(&p, &[]));
        assert_eq!(priority_of(&p, 0), Priority::Normal);
    }

    #[test]
    fn have_menor_que_o_total_nao_estoura_indice() {
        // Partial state coming from librqbit must not bring down the calculation.
        let p = plan(10, 3);
        let have = vec![true; 2];
        assert!(!can_start_playback(&p, &have));
        assert!(!request_order(&p, &have).is_empty());
    }
}
