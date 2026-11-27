//! Keeps what each URL was last time, so we know when it changed.
//!
//! B39. `core::media_diff` compares two snapshots and says what changed; what
//! was missing was someone keeping the previous snapshot. Without that,
//! re-downloading a re-edited video overwrites silently and the user finds out
//! while watching.
//!
//! A file of its own, keyed by URL. It does not go in the download history
//! because the question is a different one: the history says what you
//! downloaded, this says what the source was at that moment.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::core::media_diff::{diff, summarize, MediaSnapshot};

const SNAPSHOTS_FILE: &str = "media-snapshots.json";

/// How many snapshots to keep. One per already-downloaded URL piles up without
/// limit for a user of years; this ceiling cuts the oldest ones in insertion order.
const MAX_SNAPSHOTS: usize = 2000;

fn file_path() -> Option<PathBuf> {
    crate::core::paths::app_data_dir().map(|d| d.join(SNAPSHOTS_FILE))
}

fn load() -> BTreeMap<String, MediaSnapshot> {
    let Some(path) = file_path() else {
        return BTreeMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        tracing::warn!("[media-diff] {} ilegivel, ignorando: {}", path.display(), e);
        BTreeMap::new()
    })
}

fn save(map: &BTreeMap<String, MediaSnapshot>) {
    let Some(path) = file_path() else { return };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(json) = serde_json::to_string(map) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Cuts the oldest snapshots when it passes the ceiling.
///
/// Split out so it can be tested: a ceiling that is never exercised is a disk
/// leak waiting for its moment.
pub fn podar(
    mut map: BTreeMap<String, MediaSnapshot>,
    teto: usize,
) -> BTreeMap<String, MediaSnapshot> {
    while map.len() > teto {
        let Some(primeira) = map.keys().next().cloned() else {
            break;
        };
        map.remove(&primeira);
    }
    map
}

/// Compares the new snapshot with the saved one and returns the summary, if
/// something changed.
///
/// Called **before** downloading: the only moment when saying "this is not the
/// same video anymore" is still good for something.
#[tauri::command]
pub fn check_media_changed(url: String, current: MediaSnapshot) -> Option<String> {
    let mapa = load();
    let anterior = mapa.get(&url)?;
    let mudancas = diff(anterior, &current);
    summarize(&mudancas)
}

/// Saves the current snapshot of this URL.
#[tauri::command]
pub fn record_media_snapshot(url: String, snapshot: MediaSnapshot) {
    let mut mapa = load();
    mapa.insert(url, snapshot);
    let mapa = podar(mapa, MAX_SNAPSHOTS);
    save(&mapa);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retrato(dur: f64, sha: &str, titulo: &str) -> MediaSnapshot {
        MediaSnapshot {
            duration_secs: Some(dur),
            chapters: Vec::new(),
            sha256: Some(sha.to_string()),
            title: Some(titulo.to_string()),
        }
    }

    #[test]
    fn video_igual_nao_gera_ruido() {
        // The common case: re-downloading the same video. Warning here would be a
        // false alarm, and a false alarm trains the user to ignore the real warning.
        let a = retrato(120.0, "abc", "Aula 1");
        let mudancas = diff(&a, &a);
        assert!(summarize(&mudancas).is_none());
    }

    #[test]
    fn video_reeditado_e_detectado() {
        let antes = retrato(600.0, "abc", "Aula 1");
        let depois = retrato(540.0, "def", "Aula 1");
        let resumo = summarize(&diff(&antes, &depois)).expect("mudou");
        assert!(!resumo.is_empty());
    }

    #[test]
    fn o_teto_corta_os_mais_antigos() {
        let mut mapa = BTreeMap::new();
        for i in 0..10 {
            mapa.insert(format!("url-{i:02}"), retrato(1.0, "h", "x"));
        }
        let podado = podar(mapa, 4);
        assert_eq!(podado.len(), 4, "the ceiling has to hold");
        assert!(
            !podado.contains_key("url-00"),
            "the oldest has to go first"
        );
        assert!(podado.contains_key("url-09"), "the newest has to stay");
    }

    #[test]
    fn abaixo_do_teto_nada_e_perdido() {
        let mut mapa = BTreeMap::new();
        mapa.insert("a".to_string(), retrato(1.0, "h", "x"));
        assert_eq!(podar(mapa, 100).len(), 1);
    }
}
