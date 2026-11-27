//! When a torrent that is still downloading can start playing.
//!
//! B46. The `core::torrent_stream` module was written assuming GumballZ Get
//! would choose the piece order. When wired up, it turned out `librqbit`
//! **already does that**: `ManagedTorrent::stream(file_id)` returns a `FileStream`
//! that prioritizes the pieces under the read cursor on its own.
//!
//! So the piece order stays with the engine — two schedulers fighting over the
//! same pieces would be worse than one. What is left, and what the engine does
//! not answer, is the UI's question: **can you press play yet?** And it is
//! `can_start_playback` that decides that, and that is what this module exposes.

use crate::core::torrent_stream::{can_start_playback, request_order, StreamPlan};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlaybackReadiness {
    pub ready: bool,
    /// Pieces missing from the playhead onward, in order of urgency.
    pub missing_head: Vec<u32>,
    pub have_count: usize,
    pub total_pieces: usize,
}

/// Answers whether there is enough on disk from the playhead to play.
///
/// Works both for the initial play and for seeking: the playhead is the piece
/// the current position requires, and jumping to the middle of an incomplete
/// torrent is the case where "50% downloaded" misleads most.
///
/// It takes the bitfield instead of reading from the engine because whoever has
/// the live session is the downloader; passing the state in here keeps the
/// decision testable and independent of a running torrent.
#[tauri::command]
pub fn torrent_playback_readiness(
    total_pieces: u32,
    playhead_piece: u32,
    have: Vec<bool>,
) -> PlaybackReadiness {
    let plan = StreamPlan {
        total_pieces,
        playhead_piece,
    };
    let ready = can_start_playback(&plan, &have);
    let missing_head = request_order(&plan, &have);
    PlaybackReadiness {
        ready,
        have_count: have.iter().filter(|h| **h).count(),
        total_pieces: total_pieces as usize,
        missing_head,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torrent_vazio_nao_esta_pronto() {
        // The case that matters: pressing play with nothing downloaded opens a
        // truncated file and the player shows an error, which is worse than the
        // disabled button.
        let r = torrent_playback_readiness(100, 0, vec![false; 100]);
        assert!(!r.ready);
        assert_eq!(r.have_count, 0);
        assert!(!r.missing_head.is_empty(), "has to say what is missing");
    }

    #[test]
    fn com_inicio_e_ultima_peca_da_para_tocar() {
        // The last piece comes along because in many MP4s the `moov` (the
        // container's index) sits at the end: without it the player does not even
        // know the duration.
        let mut have = vec![false; 100];
        for h in have.iter_mut().take(20) {
            *h = true;
        }
        have[99] = true;
        let r = torrent_playback_readiness(100, 0, have);
        assert!(r.ready);
        assert_eq!(r.have_count, 21);
    }

    #[test]
    fn so_o_inicio_nao_basta_sem_a_ultima_peca() {
        // I wrote this test expecting it to pass with only the start. The opposite
        // passed, and the module was right: without the end of the container the
        // player opens and fails.
        let mut have = vec![false; 100];
        for h in have.iter_mut().take(20) {
            *h = true;
        }
        let r = torrent_playback_readiness(100, 0, have);
        assert!(!r.ready, "missing the last piece, with the container index");
    }

    #[test]
    fn baixar_o_fim_primeiro_nao_ajuda_a_comecar() {
        // A common torrent downloads the rarest pieces first, which tends to be
        // anywhere but the start. That is exactly why "50% downloaded" does not
        // mean "can watch".
        let mut have = vec![false; 100];
        for h in have.iter_mut().skip(50) {
            *h = true;
        }
        let r = torrent_playback_readiness(100, 0, have);
        assert!(!r.ready, "metade baixada, mas a metade errada");
        assert_eq!(r.have_count, 50);
    }
}
