//! Binaries the user pointed at, instead of the ones GumballZ Get manages.
//!
//! Asked for in issue #222: anyone who already has yt-dlp, FFmpeg or PDFium installed does
//! not want a second copy inside the app directory. Until now only PDFium
//! accepted a custom file, and even then it **copied** it — which solves
//! half the problem and creates the other half, because the copy ages on its own.
//!
//! Here the path is stored and used in place. If the user updates their yt-dlp
//! through the package manager, GumballZ Get starts using the new version without
//! doing anything.
//!
//! Its own file, not a field in `settings.json`, for two reasons: it does not
//! force a migration on everyone for the sake of a few, and a broken path
//! here cannot stop the settings from loading.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const OVERRIDES_FILE: &str = "binary-overrides.json";

/// Accepted names. Closed on purpose: accepting any string would let the
/// file become a dump of mistyped keys that never have any effect.
pub const SUPPORTED: [&str; 3] = ["yt-dlp", "FFmpeg", "PDFium"];

pub fn is_supported(name: &str) -> bool {
    SUPPORTED.contains(&name)
}

fn file_path() -> Option<PathBuf> {
    super::paths::app_data_dir().map(|d| d.join(OVERRIDES_FILE))
}

/// Reads the map from disk. An unreadable file counts as empty: losing the override
/// is an annoyance, not opening the app is a problem.
pub fn load() -> BTreeMap<String, String> {
    let Some(path) = file_path() else {
        return BTreeMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    match serde_json::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("[overrides] {} unreadable, ignoring: {}", path.display(), e);
            BTreeMap::new()
        }
    }
}

fn save(map: &BTreeMap<String, String>) -> std::io::Result<()> {
    let Some(path) = file_path() else {
        return Err(std::io::Error::other("no data directory"));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(map).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)
}

/// Path the user pointed at for this tool, if it still exists.
///
/// The existence check happens here, not only when saving: the
/// binary may have been removed since, and in that case the right thing is to fall back to the
/// managed one silently, not fail the download.
pub fn get(name: &str) -> Option<PathBuf> {
    let raw = load().get(name)?.clone();
    let path = PathBuf::from(raw);
    if path.exists() {
        Some(path)
    } else {
        tracing::warn!(
            "[overrides] {} points to {} which no longer exists — using the managed one",
            name,
            path.display()
        );
        None
    }
}

/// Reason why a path does not work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    NotSupported,
    NotFound,
    NotAFile,
}

impl Rejection {
    pub fn message(&self, name: &str) -> String {
        match self {
            Rejection::NotSupported => {
                format!("{name} does not accept a custom path")
            }
            Rejection::NotFound => "The chosen file does not exist".to_string(),
            Rejection::NotAFile => "The chosen path is a folder, not a file".to_string(),
        }
    }
}

/// Validates without touching the config disk. Separate from `set` so it can be
/// tested without global state.
pub fn validate(name: &str, path: &Path) -> Result<(), Rejection> {
    if !is_supported(name) {
        return Err(Rejection::NotSupported);
    }
    if !path.exists() {
        return Err(Rejection::NotFound);
    }
    if !path.is_file() {
        return Err(Rejection::NotAFile);
    }
    Ok(())
}

pub fn set(name: &str, path: &Path) -> Result<(), String> {
    validate(name, path).map_err(|r| r.message(name))?;
    let mut map = load();
    map.insert(name.to_string(), path.to_string_lossy().to_string());
    save(&map).map_err(|e| e.to_string())
}

pub fn clear(name: &str) -> Result<(), String> {
    let mut map = load();
    if map.remove(name).is_none() {
        return Ok(());
    }
    save(&map).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempfile(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("gumballz-get-ovr-{tag}-{}", std::process::id()));
        std::fs::write(&p, b"stub").expect("create file");
        p
    }

    #[test]
    fn only_the_three_known_tools_are_accepted() {
        assert!(is_supported("yt-dlp"));
        assert!(is_supported("FFmpeg"));
        assert!(is_supported("PDFium"));
        // A differently spelled name must not become a dead entry in the file.
        assert!(!is_supported("ffmpeg"));
        assert!(!is_supported("aria2c"));
    }

    #[test]
    fn nonexistent_path_is_refused_at_selection_time() {
        // The point: failing here, with the file picker open, is far better
        // than failing in the middle of a download.
        let r = validate("yt-dlp", Path::new("/nao/existe/yt-dlp"));
        assert_eq!(r, Err(Rejection::NotFound));
    }

    #[test]
    fn a_folder_does_not_work_as_a_binary() {
        let dir = std::env::temp_dir();
        assert_eq!(validate("FFmpeg", &dir), Err(Rejection::NotAFile));
    }

    #[test]
    fn unknown_tool_is_refused_before_looking_at_the_disk() {
        let f = tempfile("desconhecida");
        assert_eq!(validate("aria2c", &f), Err(Rejection::NotSupported));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn a_real_file_passes() {
        let f = tempfile("valido");
        assert_eq!(validate("yt-dlp", &f), Ok(()));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn the_message_says_what_happened_not_just_that_it_failed() {
        // Project copy rule: an error says what happened.
        assert!(Rejection::NotFound
            .message("yt-dlp")
            .contains("does not exist"));
        assert!(Rejection::NotAFile.message("yt-dlp").contains("folder"));
    }
}
