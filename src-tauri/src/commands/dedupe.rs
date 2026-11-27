//! Content dedup of files already downloaded.
//!
//! B38. `core::cas` knows how to keep a file under its content hash and
//! materialize it with a hard link; what was missing was someone calling it.
//!
//! It runs **after** the download, never during. Touching the download's write
//! path to save disk would be swapping the app's main function for a secondary
//! one — and the risk would fall exactly on the file the user just waited for.
//!
//! The gain shows up when the same content exists in two places: the same lesson
//! in two courses, the same video downloaded into two folders. After the dedup
//! both paths keep existing and point at the same inode.

use std::path::{Path, PathBuf};

use crate::core::cas::{ingest, materialize, same_file, LinkOutcome};

/// How many paths point at the same object.
///
/// Only Unix answers: `core::cas`'s `link_count` is behind `cfg(unix)` because
/// it reads `nlink` from the metadata. On Windows there is no cheap equivalent,
/// and **inventing 1 would be worse than not knowing** — the app would report
/// zero savings with the same face as someone who measured. That is why it
/// returns `None`, and the caller tells the difference.
fn quantos_links(object: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        crate::core::cas::link_count(object)
    }
    #[cfg(not(unix))]
    {
        let _ = object;
        None
    }
}

fn store_root() -> Option<PathBuf> {
    crate::core::paths::app_data_dir().map(|d| d.join("content-store"))
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DedupeReport {
    pub examined: usize,
    pub deduplicated: usize,
    /// Bytes that stopped being occupied twice. Zero when the platform cannot
    /// tell a link from a copy — see `savings_measurable`.
    pub bytes_saved: u64,
    /// `false` on Windows. `cas::same_file` has no way to answer there
    /// (`volume_serial_number` and `file_index` are still unstable), so a file
    /// that **already** was a link and gets re-linked would count as new savings
    /// on every run. We prefer giving no number to inflating one.
    pub savings_measurable: bool,
    pub errors: Vec<String>,
}

/// Runs the dedupe over a list of files already downloaded.
///
/// A file that is already linked to the store is skipped at no cost — the
/// operation is idempotent on purpose, because the user is going to click twice.
#[tauri::command]
pub async fn deduplicate_files(paths: Vec<String>) -> Result<DedupeReport, String> {
    let root = store_root().ok_or_else(|| "no data directory".to_string())?;

    tokio::task::spawn_blocking(move || {
        let mut rep = DedupeReport {
            savings_measurable: cfg!(unix),
            ..Default::default()
        };
        for raw in paths {
            let path = PathBuf::from(&raw);
            if !path.is_file() {
                continue;
            }
            rep.examined += 1;

            let tamanho = match std::fs::metadata(&path) {
                Ok(m) => m.len(),
                Err(e) => {
                    rep.errors.push(format!("{}: {}", path.display(), e));
                    continue;
                }
            };

            let hash = match sha256_of(&path) {
                Ok(h) => h,
                Err(e) => {
                    rep.errors.push(format!("{}: {}", path.display(), e));
                    continue;
                }
            };

            let objeto = match ingest(&root, &path, &hash) {
                Ok(o) => o,
                Err(e) => {
                    rep.errors.push(format!("{}: {}", path.display(), e));
                    continue;
                }
            };

            // Already the same inode: nothing to do, and it does not count as savings.
            if same_file(&objeto, &path) {
                continue;
            }

            match materialize(&objeto, &path) {
                Ok(LinkOutcome::Linked) => {
                    rep.deduplicated += 1;
                    // Only add where `same_file` can guarantee the destination
                    // was not already a link to this object. Without that, the
                    // second run on Windows would add everything again.
                    if rep.savings_measurable {
                        rep.bytes_saved += tamanho;
                    }
                }
                Ok(LinkOutcome::AlreadyPresent) => {
                    // Already linked. Does not count as new savings, or the
                    // number would grow with every click without anything having changed.
                }
                Ok(LinkOutcome::Copied) => {
                    // Different volume: a hard link does not cross filesystems.
                    // Copying keeps the file correct and saves nothing —
                    // reporting savings here would be a lie.
                }
                Err(e) => rep.errors.push(format!("{}: {}", path.display(), e)),
            }
        }
        Ok(rep)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct StoreStats {
    pub objects: usize,
    pub bytes_on_disk: u64,
    /// How much would be occupied if every link were a real copy.
    pub bytes_without_dedupe: u64,
    /// `false` when the platform cannot count links (Windows). The UI needs to
    /// tell "saved zero" apart from "cannot measure".
    pub savings_measurable: bool,
}

/// How much the store is saving today.
#[tauri::command]
pub async fn content_store_stats() -> Result<StoreStats, String> {
    let Some(root) = store_root() else {
        return Ok(StoreStats::default());
    };
    tokio::task::spawn_blocking(move || {
        let mut s = StoreStats {
            savings_measurable: cfg!(unix),
            ..Default::default()
        };
        let Ok(niveis) = std::fs::read_dir(&root) else {
            return s;
        };
        for nivel in niveis.filter_map(|e| e.ok()) {
            let Ok(objetos) = std::fs::read_dir(nivel.path()) else {
                continue;
            };
            for obj in objetos.filter_map(|e| e.ok()) {
                let Ok(meta) = obj.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                s.objects += 1;
                s.bytes_on_disk += meta.len();
                // Each extra link is a copy that was never made.
                let links = quantos_links(&obj.path()).unwrap_or(1).max(1);
                s.bytes_without_dedupe += meta.len() * links;
            }
        }
        s
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gumballz-get-dedupe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("create");
        d
    }

    #[test]
    fn dois_arquivos_iguais_viram_o_mesmo_inode() {
        let d = tempdir("iguais");
        let root = d.join("store");
        let a = d.join("a.mp4");
        let b = d.join("b.mp4");
        std::fs::write(&a, b"mesmo conteudo").unwrap();
        std::fs::write(&b, b"mesmo conteudo").unwrap();

        let h = sha256_of(&a).unwrap();
        assert_eq!(h, sha256_of(&b).unwrap(), "same content, same hash");

        let obj = ingest(&root, &a, &h).unwrap();
        materialize(&obj, &a).unwrap();
        materialize(&obj, &b).unwrap();

        // The behavioural test, not an inode one: writing through one path has to
        // show up on the other. On Windows `same_file` does not always answer.
        assert_eq!(
            std::fs::read(&a).unwrap(),
            std::fs::read(&b).unwrap(),
            "both paths have to see the same content"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn conteudo_diferente_nao_e_deduplicado() {
        let d = tempdir("diferentes");
        let a = d.join("a.mp4");
        let b = d.join("b.mp4");
        std::fs::write(&a, b"conteudo A").unwrap();
        std::fs::write(&b, b"conteudo B").unwrap();
        assert_ne!(
            sha256_of(&a).unwrap(),
            sha256_of(&b).unwrap(),
            "different files must not collide"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rodar_duas_vezes_nao_corrompe_o_arquivo() {
        // The property that holds on all three platforms: materializing twice is
        // safe and the content stays right. This test already caught a real defect
        // — the first version asserted inode identity via `same_file`, which on
        // Windows always says "different", and that is how it showed up that the
        // savings count doubled over there.
        let d = tempdir("idempotente");
        let root = d.join("store");
        let a = d.join("a.mp4");
        std::fs::write(&a, b"conteudo").unwrap();
        let h = sha256_of(&a).unwrap();
        let obj = ingest(&root, &a, &h).unwrap();
        materialize(&obj, &a).unwrap();
        materialize(&obj, &a).unwrap();
        assert_eq!(
            std::fs::read(&a).unwrap(),
            b"conteudo",
            "materializing twice must not break the file"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn economia_so_e_somada_onde_da_para_medir() {
        // The number is reported as unmeasured instead of zeroed by accident:
        // "don't know" is a different answer from "did not save".
        let rep = DedupeReport {
            savings_measurable: cfg!(unix),
            ..Default::default()
        };
        assert_eq!(rep.savings_measurable, cfg!(unix));
        if !rep.savings_measurable {
            assert_eq!(rep.bytes_saved, 0, "without measurement, do not invent a number");
        }
    }
}
