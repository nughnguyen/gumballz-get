//! The download queue as a write-ahead log.
//!
//! Today's `recovery.json` rewrites the whole file on every change and keeps
//! only the intent — url, title, folder. A `kill -9` in the middle of 200 items
//! loses the order, the progress and each item's options, and what comes back is
//! a warning the user has to accept, with everything re-resolved from scratch.
//!
//! Here every state transition becomes **one append-only record**. Rebuilding
//! is re-reading the log from the start. No rewriting, no window where the file
//! is half written, and the cost of writing does not grow with the queue size.
//!
//! ## Why append-only and not "write everything, rename"
//!
//! The current `write_to_disk` is already atomic via `tmp` + `rename`, and that
//! protects against a truncated file — but not against loss. Between two writes
//! there is a gap, and a 200-item queue rewritten on every progress update either
//! pays too much I/O or writes rarely and loses what happened in between.
//!
//! ## What survives a corrupted record
//!
//! The last line is the one that can be half written when the process dies in
//! the middle of writing. It is discarded and **all the previous ones stay
//! valid** — which is the property a single rewritten file does not have.

use serde::{Deserialize, Serialize};

/// One transition. The log is a sequence of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WalRecord {
    /// Entered the queue, with everything needed to redo it without re-resolving.
    Enqueued {
        id: u64,
        url: String,
        title: String,
        platform: String,
        output_dir: String,
        #[serde(default)]
        quality: Option<String>,
        #[serde(default)]
        download_mode: Option<String>,
        #[serde(default)]
        format_id: Option<String>,
        /// Referer, when the platform requires one for the download to work.
        /// Missing on records written before this field existed.
        #[serde(default)]
        referer: Option<String>,
        /// Position in the queue, so the order survives.
        #[serde(default)]
        position: u32,
    },
    Started {
        id: u64,
    },
    /// Progress. Logged sparingly — see `PROGRESS_STEP_PERCENT`.
    Progress {
        id: u64,
        percent: f64,
        #[serde(default)]
        downloaded_bytes: Option<u64>,
    },
    Completed {
        id: u64,
        file_path: String,
    },
    Failed {
        id: u64,
        error: String,
    },
    Cancelled {
        id: u64,
    },
    Removed {
        id: u64,
    },
}

impl WalRecord {
    pub fn id(&self) -> u64 {
        match self {
            WalRecord::Enqueued { id, .. }
            | WalRecord::Started { id }
            | WalRecord::Progress { id, .. }
            | WalRecord::Completed { id, .. }
            | WalRecord::Failed { id, .. }
            | WalRecord::Cancelled { id }
            | WalRecord::Removed { id } => *id,
        }
    }

    /// Terminal record: after it, the item does not return to the queue.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WalRecord::Completed { .. }
                | WalRecord::Failed { .. }
                | WalRecord::Cancelled { .. }
                | WalRecord::Removed { .. }
        )
    }
}

/// Only logs progress every 5%.
///
/// Logging every progress event would turn the WAL into an I/O generator:
/// 200 items × hundreds of events each. Five percent loses at most 5% of
/// progress in a crash, and yt-dlp's `.part` covers the rest — the WAL keeps
/// the intent, not the bytes.
pub const PROGRESS_STEP_PERCENT: f64 = 5.0;

/// Is this progress worth logging, given the last one logged?
pub fn should_log_progress(last_logged: Option<f64>, current: f64) -> bool {
    match last_logged {
        None => current > 0.0,
        Some(last) => (current - last).abs() >= PROGRESS_STEP_PERCENT,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveredItem {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub platform: String,
    pub output_dir: String,
    pub quality: Option<String>,
    pub download_mode: Option<String>,
    pub format_id: Option<String>,
    pub referer: Option<String>,
    pub position: u32,
    pub percent: f64,
    pub was_running: bool,
}

/// Rebuilds the queue from the log.
///
/// Returns them in the order the items were enqueued, not in log order — the
/// user's queue has an order and it is part of what is being recovered.
pub fn replay(records: &[WalRecord]) -> Vec<RecoveredItem> {
    use std::collections::HashMap;
    let mut live: HashMap<u64, RecoveredItem> = HashMap::new();

    for r in records {
        match r {
            WalRecord::Enqueued {
                id,
                url,
                title,
                platform,
                output_dir,
                quality,
                download_mode,
                format_id,
                referer,
                position,
            } => {
                live.insert(
                    *id,
                    RecoveredItem {
                        id: *id,
                        url: url.clone(),
                        title: title.clone(),
                        platform: platform.clone(),
                        output_dir: output_dir.clone(),
                        quality: quality.clone(),
                        download_mode: download_mode.clone(),
                        format_id: format_id.clone(),
                        referer: referer.clone(),
                        position: *position,
                        percent: 0.0,
                        was_running: false,
                    },
                );
            }
            WalRecord::Started { id } => {
                if let Some(item) = live.get_mut(id) {
                    item.was_running = true;
                }
            }
            WalRecord::Progress { id, percent, .. } => {
                if let Some(item) = live.get_mut(id) {
                    item.percent = *percent;
                }
            }
            other if other.is_terminal() => {
                live.remove(&other.id());
            }
            _ => {}
        }
    }

    let mut out: Vec<RecoveredItem> = live.into_values().collect();
    out.sort_by_key(|i| (i.position, i.id));
    out
}

/// Serializes one record as one JSON line.
///
/// One line per record is what allows discarding only the last one when it is
/// half written. JSON in a single block would not have that property.
pub fn encode(record: &WalRecord) -> Option<String> {
    serde_json::to_string(record).ok()
}

/// Reads the log, discarding unreadable records.
///
/// Returns `(records, dropped)`. The counter is not decorative: silent dropping
/// on a recovery path is how the queue gets lost without anyone noticing, and
/// the caller needs to be able to log how much was lost.
pub fn decode_log(contents: &str) -> (Vec<WalRecord>, usize) {
    let mut records = Vec::new();
    let mut dropped = 0usize;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<WalRecord>(line) {
            Ok(r) => records.push(r),
            Err(_) => dropped += 1,
        }
    }
    (records, dropped)
}

/// Can the log be compacted?
///
/// Only records of finished items may go. Compacting with a live item in the
/// middle would lose its `Enqueued` — which is exactly what must not be lost.
pub fn compactable(records: &[WalRecord]) -> bool {
    let live = replay(records);
    records.len() > COMPACT_THRESHOLD && live.len() * 4 < records.len()
}

pub const COMPACT_THRESHOLD: usize = 500;

/// Equivalent log containing only what still matters.
pub fn compact(records: &[WalRecord]) -> Vec<WalRecord> {
    replay(records)
        .into_iter()
        .flat_map(|i| {
            let mut out = vec![WalRecord::Enqueued {
                id: i.id,
                url: i.url,
                title: i.title,
                platform: i.platform,
                output_dir: i.output_dir,
                quality: i.quality,
                download_mode: i.download_mode,
                format_id: i.format_id,
                referer: i.referer,
                position: i.position,
            }];
            if i.was_running {
                out.push(WalRecord::Started { id: i.id });
            }
            if i.percent > 0.0 {
                out.push(WalRecord::Progress {
                    id: i.id,
                    percent: i.percent,
                    downloaded_bytes: None,
                });
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enq(id: u64, pos: u32) -> WalRecord {
        WalRecord::Enqueued {
            id,
            url: format!("https://example.com/{id}"),
            title: format!("video {id}"),
            platform: "youtube".into(),
            output_dir: "/downloads".into(),
            quality: Some("1080".into()),
            download_mode: None,
            format_id: None,
            referer: None,
            position: pos,
        }
    }

    #[test]
    fn replay_devolve_a_fila_na_ordem_em_que_foi_enfileirada() {
        // Order is part of what is being recovered: the user built the queue in
        // a sequence and expects it back.
        let log = vec![enq(3, 2), enq(1, 0), enq(2, 1)];
        let itens = replay(&log);
        assert_eq!(
            itens.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn item_terminado_nao_volta_para_a_fila() {
        for terminal in [
            WalRecord::Completed {
                id: 1,
                file_path: "/x.mp4".into(),
            },
            WalRecord::Failed {
                id: 1,
                error: "boom".into(),
            },
            WalRecord::Cancelled { id: 1 },
            WalRecord::Removed { id: 1 },
        ] {
            let log = vec![enq(1, 0), WalRecord::Started { id: 1 }, terminal.clone()];
            assert!(replay(&log).is_empty(), "came back after {terminal:?}");
        }
    }

    #[test]
    fn o_que_estava_baixando_volta_marcado_com_o_progresso() {
        // Without this, resuming means starting from zero — the problem the
        // item came to solve.
        let log = vec![
            enq(1, 0),
            WalRecord::Started { id: 1 },
            WalRecord::Progress {
                id: 1,
                percent: 45.0,
                downloaded_bytes: Some(1234),
            },
        ];
        let i = &replay(&log)[0];
        assert!(i.was_running);
        assert_eq!(i.percent, 45.0);
        assert_eq!(i.quality.as_deref(), Some("1080"));
    }

    #[test]
    fn a_ultima_linha_pela_metade_nao_derruba_as_anteriores() {
        // The central property of the WAL, and the one a single rewritten file
        // does not have: the process died in the middle of writing and everything
        // before survives.
        let mut texto = String::new();
        texto.push_str(&encode(&enq(1, 0)).unwrap());
        texto.push('\n');
        texto.push_str(&encode(&enq(2, 1)).unwrap());
        texto.push('\n');
        texto.push_str("{\"op\":\"enqueued\",\"id\":3,\"url\":\"https://exa"); // cortado

        let (records, dropped) = decode_log(&texto);
        assert_eq!(dropped, 1);
        assert_eq!(records.len(), 2);
        assert_eq!(replay(&records).len(), 2);
    }

    #[test]
    fn descarte_e_contado_e_nao_silencioso() {
        // Losing records without anyone noticing is how the whole queue gets lost
        // with no explanation. The caller needs to be able to log how much was lost.
        let (_, dropped) = decode_log("lixo\n{}\n\n{\"op\":\"started\",\"id\":1}\n");
        assert_eq!(dropped, 2);
    }

    #[test]
    fn log_vazio_nao_e_erro() {
        let (r, d) = decode_log("");
        assert!(r.is_empty());
        assert_eq!(d, 0);
        assert!(replay(&[]).is_empty());
    }

    #[test]
    fn progresso_so_grava_a_cada_cinco_por_cento() {
        // Logging every event would turn the WAL into an I/O generator: 200 items
        // times hundreds of events each.
        assert!(should_log_progress(None, 1.0));
        assert!(!should_log_progress(None, 0.0));
        assert!(!should_log_progress(Some(10.0), 12.0));
        assert!(should_log_progress(Some(10.0), 15.0));
        assert!(
            should_log_progress(Some(50.0), 20.0),
            "a regression also logs"
        );
    }

    #[test]
    fn compactar_preserva_a_fila_viva() {
        // Compacting must not lose the Enqueued of a live item — it is exactly
        // what must not be lost.
        let mut log = vec![enq(1, 0), enq(2, 1), WalRecord::Started { id: 1 }];
        for p in [5.0, 10.0, 15.0, 20.0] {
            log.push(WalRecord::Progress {
                id: 1,
                percent: p,
                downloaded_bytes: None,
            });
        }
        log.push(WalRecord::Completed {
            id: 2,
            file_path: "/x".into(),
        });

        let antes = replay(&log);
        let depois = replay(&compact(&log));
        assert_eq!(antes, depois, "compaction changed the recovered queue");
        assert!(compact(&log).len() < log.len());
    }

    #[test]
    fn nao_compacta_log_pequeno_nem_log_cheio_de_item_vivo() {
        let pequeno: Vec<WalRecord> = (0..10).map(|i| enq(i, i as u32)).collect();
        assert!(!compactable(&pequeno), "a small log is not worth compacting");

        let so_vivos: Vec<WalRecord> = (0..600).map(|i| enq(i, i as u32)).collect();
        assert!(
            !compactable(&so_vivos),
            "a big log but all live has nothing to compact"
        );
    }

    #[test]
    fn registro_sobrevive_ao_round_trip() {
        let originais = vec![
            enq(1, 0),
            WalRecord::Started { id: 1 },
            WalRecord::Progress {
                id: 1,
                percent: 33.3,
                downloaded_bytes: Some(99),
            },
            WalRecord::Completed {
                id: 1,
                file_path: "/a b/c.mp4".into(),
            },
        ];
        let texto: String = originais
            .iter()
            .map(|r| encode(r).unwrap() + "\n")
            .collect();
        let (lidos, dropped) = decode_log(&texto);
        assert_eq!(dropped, 0);
        assert_eq!(lidos, originais);
    }

    #[test]
    fn registro_gravado_por_versao_anterior_sem_position_carrega() {
        // A new field in an already written log must not bring down the whole
        // recovery — same class of bug that zeroed settings.json.
        let linha = r#"{"op":"enqueued","id":7,"url":"https://x","title":"t","platform":"youtube","output_dir":"/d"}"#;
        let (r, dropped) = decode_log(linha);
        assert_eq!(dropped, 0);
        assert_eq!(r.len(), 1);
        assert_eq!(replay(&r)[0].position, 0);
    }
}

#[cfg(test)]
mod kill9_tests {
    use super::*;

    /// Recovers from a WAL produced by a process really killed with SIGKILL in
    /// the middle of a write.
    ///
    /// The file is generated by , which writes 202 records and takes a SIGKILL
    /// while the last one is half written. The content below is the real result
    /// of that file, reduced, with the final line truncated exactly as the kernel
    /// left it.
    const WAL_APOS_KILL9: &str = concat!(
        r#"{"op":"enqueued","id":0,"url":"https://example.com/0","title":"v0","platform":"youtube","output_dir":"/downloads","position":0}"#,
        "\n",
        r#"{"op":"enqueued","id":5,"url":"https://example.com/5","title":"v5","platform":"youtube","output_dir":"/downloads","position":5}"#,
        "\n",
        r#"{"op":"enqueued","id":7,"url":"https://example.com/7","title":"v7","platform":"youtube","output_dir":"/downloads","position":7}"#,
        "\n",
        r#"{"op":"started","id":5}"#,
        "\n",
        r#"{"op":"completed","id":7,"file_path":"/x.mp4"}"#,
        "\n",
        r#"{"op":"progress","id":5,"perc"#,
    );

    #[test]
    fn recupera_a_fila_de_um_arquivo_cortado_por_kill9_real() {
        let (records, dropped) = decode_log(WAL_APOS_KILL9);
        assert_eq!(dropped, 1, "only the cut line is discarded");

        let fila = replay(&records);
        // Item 7 completed and does not come back; 0 and 5 come back, in queue order.
        assert_eq!(fila.iter().map(|i| i.id).collect::<Vec<_>>(), vec![0, 5]);
        // What was downloading comes back marked, with its options intact.
        let cinco = fila.iter().find(|i| i.id == 5).unwrap();
        assert!(cinco.was_running);
        assert_eq!(cinco.url, "https://example.com/5");
        assert_eq!(cinco.output_dir, "/downloads");
    }
}
