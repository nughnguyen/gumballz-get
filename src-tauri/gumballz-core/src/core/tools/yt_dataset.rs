//! Archive built from searches, not from a channel or playlist.
//!
//! `yt_archive` knows how to enumerate a URL; to gather videos from several searches
//! (e.g. all the guides for a game, in several languages) the queue needs to be
//! seeded with already-filtered results. `seed` writes those items into the same
//! `.gumballz-get-archive.json` state, with no network, and `yt_archive::run` carries on from there
//! with resume, recorded failures and a disk-space cap.

use std::path::Path;

use serde::Serialize;

use super::yt_archive::{self, ArchiveItem, ArchiveState, ItemStatus};

#[derive(Debug, Clone, Serialize)]
pub struct SeedResult {
    pub state_path: String,
    /// New items that entered the queue just now.
    pub added: usize,
    /// Queue size after seeding.
    pub total: usize,
}

/// Reads NDJSON (one line per video, as `gumballz-get search --json` prints)
/// and returns pending items. A line without an `id` or that is not JSON is ignored.
pub fn parse_seed(text: &str) -> Vec<ArchiveItem> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter_map(|json| {
            let id = json.get("id")?.as_str()?.trim().to_string();
            if id.is_empty() || !seen.insert(id.clone()) {
                return None;
            }
            let text_of = |k: &str| {
                json.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let url = Some(text_of("url"))
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", id));
            Some(ArchiveItem {
                title: text_of("title"),
                url,
                duration_seconds: json.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
                status: ItemStatus::Pending,
                error: None,
                file: None,
                attempts: 0,
                id,
            })
        })
        .collect()
}

/// Adds the items to the folder's queue without losing what was already downloaded or failed.
pub fn seed(dest: &Path, title: &str, items: Vec<ArchiveItem>) -> anyhow::Result<SeedResult> {
    let mut st = yt_archive::load_state(dest).unwrap_or_else(|| ArchiveState::new("", title));
    let added = st.merge(title, items, false);
    let path = yt_archive::save_state(dest, &mut st)?;
    Ok(SeedResult {
        state_path: path.to_string_lossy().to_string(),
        added,
        total: st.items.len(),
    })
}

/// Adds up the size of the files under `dir` (without following links). Used as a disk-space cap
/// for the archive: the measure is the actual disk, not what the state thinks was downloaded.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = super::super::temp_dir().join(format!("{}-{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp folder");
        dir
    }

    const HITS: &str = r#"{"id":"a1","title":"A","url":"https://youtu.be/a1","duration":600.0}
{"id":"b2","title":"B"}
lixo
{"id":"a1","title":"A de novo"}
{"id":"  ","title":"vazio"}"#;

    #[test]
    fn parse_seed_keeps_first_of_each_id() {
        let items = parse_seed(HITS);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "A");
        assert_eq!(items[0].duration_seconds, 600.0);
        assert_eq!(items[1].url, "https://www.youtube.com/watch?v=b2");
        assert!(items.iter().all(|i| i.status == ItemStatus::Pending));
    }

    #[test]
    fn seeding_twice_adds_only_new_ids_and_keeps_status() {
        let dir = temp("yt-dataset-seed");
        let r = seed(&dir, "Mir4", parse_seed(HITS)).expect("primeira semeadura");
        assert_eq!((r.added, r.total), (2, 2));

        let mut st = yt_archive::load_state(&dir).expect("estado gravado");
        st.mark("a1", ItemStatus::Ok, Some("a1.mp4".into()), None);
        yt_archive::save_state(&dir, &mut st).expect("grava");

        let more = parse_seed("{\"id\":\"a1\"}\n{\"id\":\"c3\",\"title\":\"C\"}");
        let r2 = seed(&dir, "Mir4", more).expect("segunda semeadura");
        assert_eq!((r2.added, r2.total), (1, 3));
        let st = yt_archive::load_state(&dir).expect("estado");
        let a1 = st.items.iter().find(|i| i.id == "a1").expect("a1");
        assert_eq!(
            a1.status,
            ItemStatus::Ok,
            "what already downloaded does not go back to the queue"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_size_sums_nested_files() {
        let dir = temp("yt-dataset-size");
        std::fs::create_dir_all(dir.join("canal")).expect("subfolder");
        std::fs::write(dir.join("a.bin"), vec![0u8; 100]).expect("a");
        std::fs::write(dir.join("canal").join("b.bin"), vec![0u8; 50]).expect("b");
        assert_eq!(dir_size(&dir), 150);
        assert_eq!(dir_size(&dir.join("não-existe")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
