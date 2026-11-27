//! Reading a user state file without silent loss.
//!
//! The pattern this module replaces is
//! `read_to_string().ok().and_then(parse).unwrap_or_default()`: when the file
//! exists but does not deserialize, the caller gets "empty", and the next write
//! overwrites the original. The user loses everything without seeing a single
//! line of log. That is how `settings.json` zeroed preferences, and the same
//! pattern was replicated in the cookie registry, the YouTube templates and the
//! plugin settings.
//!
//! Here the unreadable file is **quarantined** before any default is returned:
//! it is renamed to `<name>.corrupt-<epoch_ms>`, which stops the next write from
//! destroying it and leaves a recoverable copy.

use std::path::{Path, PathBuf};

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Quarantine path for `path`. Public for the tests and for whoever needs to
/// point the user at the preserved file.
pub fn quarantine_path(path: &Path, stamp_ms: u128) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("state.json");
    path.with_file_name(format!("{}.corrupt-{}", name, stamp_ms))
}

/// Renames the unreadable file to the quarantine path. Returns the destination
/// when it worked; `None` when even that was not possible (then the log is all
/// that is left, and the caller carries on with the default).
pub fn quarantine(path: &Path, label: &str) -> Option<PathBuf> {
    let dest = quarantine_path(path, now_ms());
    match std::fs::rename(path, &dest) {
        Ok(()) => {
            tracing::error!(
                "[state] {} unreadable at {} — preserved as {}. The app carried on with the default values; \
                 the original file was NOT overwritten.",
                label,
                path.display(),
                dest.display()
            );
            Some(dest)
        }
        Err(e) => {
            tracing::error!(
                "[state] {} unreadable at {} and could not be preserved ({}). \
                 The app carried on with the default values.",
                label,
                path.display(),
                e
            );
            None
        }
    }
}

/// Reads and deserializes a state file.
///
/// - missing file → `None`, no noise (it is the normal case on first run)
/// - unreadable or corrupted file → quarantine + error log + `None`
///
/// Never returns a default silently: the `None` here always means "there was
/// no usable state", and the reason is in the log when it is not the first run.
pub fn load_or_quarantine<T: serde::de::DeserializeOwned>(path: &Path, label: &str) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("[state] could not read {}: {}", path.display(), e);
            return None;
        }
    };
    // An empty file (a crash in the middle of a non-atomic write) counts as
    // missing, not as corrupted — there is nothing to preserve.
    if content.trim().is_empty() {
        tracing::warn!("[state] {} empty at {}", label, path.display());
        return None;
    }
    match serde_json::from_str::<T>(&content) {
        Ok(value) => Some(value),
        Err(e) => {
            // The serde error names the field and the position, never the content —
            // safe to log even for a cookie file.
            tracing::error!("[state] {} does not deserialize: {}", label, e);
            quarantine(path, label);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Estado {
        contas: Vec<String>,
    }

    fn tmp_dir(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gumballz-get-state-{}-{}", nome, now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn arquivo_ausente_devolve_none_sem_quarentena() {
        let dir = tmp_dir("ausente");
        let path = dir.join("estado.json");
        let lido: Option<Estado> = load_or_quarantine(&path, "estado");
        assert!(lido.is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arquivo_valido_e_lido() {
        let dir = tmp_dir("valido");
        let path = dir.join("estado.json");
        std::fs::write(&path, r#"{"contas":["a","b"]}"#).unwrap();
        let lido: Option<Estado> = load_or_quarantine(&path, "estado");
        assert_eq!(
            lido,
            Some(Estado {
                contas: vec!["a".into(), "b".into()]
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arquivo_corrompido_vai_para_quarentena_e_o_original_some_do_lugar() {
        // The point of the test: after this, a caller's save() writes to a clean
        // path and does NOT destroy the content the user had.
        let dir = tmp_dir("corrompido");
        let path = dir.join("estado.json");
        std::fs::write(&path, "{ isto nao e json").unwrap();

        let lido: Option<Estado> = load_or_quarantine(&path, "estado");
        assert!(lido.is_none());
        assert!(
            !path.exists(),
            "the corrupted file has to leave the write path"
        );

        let preservados: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("estado.json.corrupt-"))
            .collect();
        assert_eq!(
            preservados.len(),
            1,
            "expected exactly one quarantine, found {preservados:?}"
        );

        let conteudo = std::fs::read_to_string(dir.join(&preservados[0])).unwrap();
        assert_eq!(
            conteudo, "{ isto nao e json",
            "the quarantine has to be byte-for-byte"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arquivo_vazio_nao_gera_quarentena() {
        let dir = tmp_dir("vazio");
        let path = dir.join("estado.json");
        std::fs::write(&path, "   \n").unwrap();
        let lido: Option<Estado> = load_or_quarantine(&path, "estado");
        assert!(lido.is_none());
        assert!(path.exists(), "an empty file has nothing to preserve");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_valido_de_forma_errada_tambem_e_quarentena() {
        // Regression: `serde_json::from_str::<T>` failing on schema (not on
        // syntax) fell into the same `unwrap_or_default()` and zeroed the state.
        let dir = tmp_dir("schema");
        let path = dir.join("estado.json");
        std::fs::write(&path, r#"{"contas": "nao e uma lista"}"#).unwrap();
        let lido: Option<Estado> = load_or_quarantine(&path, "estado");
        assert!(lido.is_none());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quarentenas_repetidas_nao_se_sobrescrevem() {
        let dir = tmp_dir("repetida");
        let a = quarantine_path(&dir.join("estado.json"), 1);
        let b = quarantine_path(&dir.join("estado.json"), 2);
        assert_ne!(a, b);
        assert!(a.to_string_lossy().ends_with("estado.json.corrupt-1"));
    }
}
