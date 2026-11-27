//! ONNX Runtime as a managed binary, following the same pattern as PDFium.
//!
//! The `ort` crate is built with the `load-dynamic` feature: nothing native goes into
//! the GumballZ Get build. The ONNX Runtime library (`libonnxruntime.dylib`,
//! `onnxruntime.dll`, `libonnxruntime.so`) is downloaded on demand from the official
//! `microsoft/onnxruntime` releases and stored in the app data directory.
//! Anyone who does not want to download it can point to a file they already have — same as PDFium.
//!
//! The version is pinned and matches the API the `ort` expects (`api-27`, i.e.
//! ONNX Runtime 1.27.x). A library older than that is refused by `ort` at load
//! time, so the download comes with a verified sha256: a truncated or
//! swapped file fails here, not inside `dlopen`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{anyhow, Context};
use serde::Serialize;

/// ONNX Runtime version required by `ort` 2.0.0-rc.13 (`api-27`).
pub const RUNTIME_VERSION: &str = "1.27.1";

const RELEASE_BASE: &str = "https://github.com/microsoft/onnxruntime/releases/download";

/// An official release package. `sha256` and `bytes` were checked against the
/// digest published by the GitHub API on 2026-09-09.
struct Asset {
    id: &'static str,
    label: &'static str,
    file: &'static str,
    sha256: &'static str,
    bytes: u64,
    os: &'static str,
}

const ASSETS: &[Asset] = &[
    Asset {
        id: "osx-arm64",
        label: "macOS ARM64 (Apple Silicon)",
        file: "onnxruntime-osx-arm64-1.27.1.tgz",
        sha256: "e42b77a7281cc6e55141bf44fcfbac2c782b823a491bbb6ac33c781dd991f8a6",
        bytes: 31_959_937,
        os: "macos",
    },
    Asset {
        id: "linux-x64",
        label: "Linux x64",
        file: "onnxruntime-linux-x64-1.27.1.tgz",
        sha256: "25b1ef1fea1acd210d63f8f24dc870ad6e077795ce1f54876252c6d3803c15af",
        bytes: 8_828_892,
        os: "linux",
    },
    Asset {
        id: "linux-aarch64",
        label: "Linux ARM64",
        file: "onnxruntime-linux-aarch64-1.27.1.tgz",
        sha256: "33c67e33d1e25b816878366ea276589a024f71f000e7ff955c4b33224d639edd",
        bytes: 7_812_402,
        os: "linux",
    },
    Asset {
        id: "win-x64",
        label: "Windows x64",
        file: "onnxruntime-win-x64-1.27.1.zip",
        sha256: "2e00414a63fdef0914cd5a5ede6c707844878e0c08e1b6693842f0451b2df2a1",
        bytes: 77_242_362,
        os: "windows",
    },
    Asset {
        id: "win-arm64",
        label: "Windows ARM64",
        file: "onnxruntime-win-arm64-1.27.1.zip",
        sha256: "6e22c2061ba6400b42a59663d700c8694e4e8fe654cf452c4700c24237407ae1",
        bytes: 78_590_093,
        os: "windows",
    },
];

/// The name under which the lib is saved on disk. It is the name `ort` looks for
/// when it receives no path, so it serves both `init_from` and anyone who
/// prefers to tweak the environment variable.
pub fn lib_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

fn current_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Recommended variant for this OS + architecture, or `None` when
/// Microsoft publishes no official build (macOS Intel, since 1.26).
pub fn auto_variant_id() -> Option<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Some("win-x64");
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        return Some("win-arm64");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Some("osx-arm64");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Some("linux-x64");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Some("linux-aarch64");
    }
    #[allow(unreachable_code)]
    None
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeVariant {
    pub id: String,
    pub label: String,
    pub bytes: u64,
    pub recommended: bool,
}

/// Variantes que fazem sentido oferecer neste SO.
pub fn list_variants() -> Vec<RuntimeVariant> {
    let os = current_os();
    let auto = auto_variant_id();
    ASSETS
        .iter()
        .filter(|a| a.os == os)
        .map(|a| RuntimeVariant {
            id: a.id.to_string(),
            label: a.label.to_string(),
            bytes: a.bytes,
            recommended: Some(a.id) == auto,
        })
        .collect()
}

fn asset_by_id(id: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.id == id)
}

fn pick_asset(variant: Option<&str>) -> anyhow::Result<&'static Asset> {
    let wanted = variant
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "auto");
    if let Some(id) = wanted {
        return asset_by_id(id)
            .ok_or_else(|| anyhow!("unknown ONNX Runtime variant: {id}"));
    }
    let auto = auto_variant_id().ok_or_else(|| {
        anyhow!(
            "Microsoft does not publish ready-made ONNX Runtime for this system; \
             install from a local file (the `onnxruntime` pip package, for example)"
        )
    })?;
    asset_by_id(auto).ok_or_else(|| anyhow!("variant {auto} vanished from the catalog"))
}

pub fn target_dir() -> Option<PathBuf> {
    crate::core::paths::app_data_dir().map(|d| d.join("onnxruntime"))
}

pub fn target_path() -> Option<PathBuf> {
    target_dir().map(|d| d.join(lib_filename()))
}

pub fn version_marker_path() -> Option<PathBuf> {
    target_dir().map(|d| d.join("onnxruntime.version"))
}

/// The lib in use and where it came from: `"env"` when the user pointed at
/// `ORT_DYLIB_PATH`, `"managed"` when it is the one GumballZ Get downloaded.
pub fn resolve_with_source() -> Option<(PathBuf, &'static str)> {
    if let Ok(raw) = std::env::var("ORT_DYLIB_PATH") {
        let p = PathBuf::from(raw.trim());
        if p.is_file() {
            return Some((p, "env"));
        }
    }
    let managed = target_path()?;
    if managed.is_file() {
        return Some((managed, "managed"));
    }
    None
}

pub fn resolve_path() -> Option<PathBuf> {
    resolve_with_source().map(|(p, _)| p)
}

pub fn is_installed() -> bool {
    resolve_with_source().is_some()
}

pub fn read_version_marker() -> Option<String> {
    let p = version_marker_path()?;
    let s = std::fs::read_to_string(&p).ok()?;
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub installed: bool,
    pub path: Option<String>,
    /// "env" | "managed"
    pub source: Option<String>,
    pub version: Option<String>,
    pub target_version: String,
    pub lib_filename: String,
    /// `false` when there is no official build for this OS/architecture.
    pub can_download: bool,
    pub variants: Vec<RuntimeVariant>,
}

pub fn status() -> RuntimeStatus {
    let found = resolve_with_source();
    RuntimeStatus {
        installed: found.is_some(),
        path: found.as_ref().map(|(p, _)| p.to_string_lossy().to_string()),
        source: found.as_ref().map(|(_, s)| (*s).to_string()),
        version: read_version_marker(),
        target_version: RUNTIME_VERSION.to_string(),
        lib_filename: lib_filename().to_string(),
        can_download: auto_variant_id().is_some(),
        variants: list_variants(),
    }
}

/// Error message that says what to do, instead of just complaining.
fn missing_runtime_error() -> anyhow::Error {
    let where_to = target_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "<app data folder>".into());
    if auto_variant_id().is_some() {
        anyhow!(
            "ONNX Runtime {} is not installed yet. Install it from the Models screen \
             (the download goes to {}) or point to a {} you already have.",
            RUNTIME_VERSION,
            where_to,
            lib_filename()
        )
    } else {
        anyhow!(
            "there is no official ONNX Runtime build for this system. \
             Point to a {} you already have (the `onnxruntime` pip package brings one) — \
             it is copied to {}.",
            lib_filename(),
            where_to
        )
    }
}

/// Path the `ort` is already using, once loading has succeeded.
static READY: OnceLock<PathBuf> = OnceLock::new();

/// Points `ort` at the resolved lib. Idempotent: `ort` keeps the handle
/// in its own `OnceLock`, so a second call changes nothing — that is why
/// we keep the winning path and return it.
///
/// A missing lib becomes an actionable error, never a panic: `ort` only panics
/// if someone touches its API without going through here.
pub fn init() -> anyhow::Result<PathBuf> {
    if let Some(p) = READY.get() {
        return Ok(p.clone());
    }
    let path = resolve_path().ok_or_else(missing_runtime_error)?;
    ort::init_from(&path)
        .map_err(|e| anyhow!("could not load {}: {}", path.display(), e))?
        .with_name("gumballz-get")
        .commit();
    let _ = READY.set(path.clone());
    Ok(path)
}

fn sha256_of(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("opening {} to verify the sha256", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// File name that counts as an ONNX Runtime library on this OS.
/// Left public so the pure selection test needs no network.
pub fn is_runtime_lib(name: &str) -> bool {
    if name.starts_with('.') {
        return false;
    }
    if cfg!(target_os = "windows") {
        name.ends_with(".dll")
    } else if cfg!(target_os = "macos") {
        name.starts_with("libonnxruntime") && name.ends_with(".dylib")
    } else {
        name.starts_with("libonnxruntime.so")
    }
}

/// Only what is inside `lib/` matters; the macOS `.dSYM` carries a
/// file with the same name and is not a loadable library.
fn is_lib_entry(path: &Path) -> bool {
    let mut has_lib = false;
    for comp in path.components() {
        let s = comp.as_os_str().to_string_lossy();
        if s == "dSYM" || s.ends_with(".dSYM") {
            return false;
        }
        if s == "lib" {
            has_lib = true;
        }
    }
    if !has_lib {
        return false;
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .map(is_runtime_lib)
        .unwrap_or(false)
}

fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> anyhow::Result<PathBuf> {
    let dest = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    if dest.exists() {
        let _ = std::fs::remove_file(&dest);
    }
    std::fs::rename(&tmp, &dest).with_context(|| format!("moving to {}", dest.display()))?;
    Ok(dest)
}

/// Pulls the `lib/` libraries out of the package and returns the name of the biggest — which is
/// always the real `libonnxruntime`, not an execution-provider shim.
fn extract_libs(archive: &Path, is_zip: bool, dir: &Path) -> anyhow::Result<String> {
    let mut biggest: Option<(String, u64)> = None;
    let mut note = |name: &str, len: u64| {
        if biggest.as_ref().map(|(_, b)| len > *b).unwrap_or(true) {
            biggest = Some((name.to_string(), len));
        }
    };

    if is_zip {
        let file = std::fs::File::open(archive)
            .with_context(|| format!("opening {}", archive.display()))?;
        let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if !entry.is_file() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            if !is_lib_entry(&path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            write_atomic(dir, &name, &buf)?;
            note(&name, buf.len() as u64);
        }
    } else {
        let file = std::fs::File::open(archive)
            .with_context(|| format!("opening {}", archive.display()))?;
        let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries()? {
            let mut entry = entry?;
            // Symlink (`libonnxruntime.dylib` -> versioned) has no content.
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry.path()?.to_path_buf();
            if !is_lib_entry(&path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            write_atomic(dir, &name, &buf)?;
            note(&name, buf.len() as u64);
        }
    }

    let (name, _) = biggest
        .ok_or_else(|| anyhow!("the package brings no ONNX Runtime library in lib/"))?;
    Ok(name)
}

/// Makes sure the canonical name (`libonnxruntime.dylib` etc.) exists, pointing
/// at the versioned file that came in the package.
fn make_canonical(dir: &Path, extracted: &str) -> anyhow::Result<PathBuf> {
    let canonical = dir.join(lib_filename());
    if extracted == lib_filename() {
        return Ok(canonical);
    }
    let src = dir.join(extracted);
    let tmp = dir.join(format!(".{}.tmp", lib_filename()));
    std::fs::copy(&src, &tmp)
        .with_context(|| format!("copying {} → {}", src.display(), tmp.display()))?;
    if canonical.exists() {
        let _ = std::fs::remove_file(&canonical);
    }
    std::fs::rename(&tmp, &canonical)
        .with_context(|| format!("moving to {}", canonical.display()))?;
    Ok(canonical)
}

/// Downloads (verifying the sha256), extracts, and leaves the lib ready for `init()`.
pub async fn ensure_runtime(
    variant: Option<String>,
    progress: &crate::core::tools::ProgressFn,
) -> anyhow::Result<PathBuf> {
    if let Some(p) = resolve_path() {
        return Ok(p);
    }
    install_runtime(variant, progress).await
}

/// Installs even if one already exists — it is the Models screen's "update".
pub async fn install_runtime(
    variant: Option<String>,
    progress: &crate::core::tools::ProgressFn,
) -> anyhow::Result<PathBuf> {
    let asset = pick_asset(variant.as_deref())?;
    let dir = target_dir().ok_or_else(|| anyhow!("could not find the app data directory"))?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let url = format!("{}/v{}/{}", RELEASE_BASE, RUNTIME_VERSION, asset.file);
    let tmp = crate::core::tools::temp_dir().join(asset.file);
    let client = crate::core::tools::client()?;
    crate::core::tools::download_to(&client, &url, &tmp, progress, "onnxruntime").await?;

    let asset_file = asset.file.to_string();
    let expected = asset.sha256.to_string();
    let is_zip = asset_file.ends_with(".zip");
    let dir_for_task = dir.clone();
    let tmp_for_task = tmp.clone();
    let p = progress.clone();
    let out = tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
        crate::core::tools::report(
            &p,
            "onnxruntime",
            "verify",
            0,
            None,
            Some("verifying sha256".into()),
        );
        let got = sha256_of(&tmp_for_task)?;
        if got != expected {
            let _ = std::fs::remove_file(&tmp_for_task);
            return Err(anyhow!(
                "the downloaded ONNX Runtime package does not match: expected sha256 {expected}, got {got}"
            ));
        }
        crate::core::tools::report(
            &p,
            "onnxruntime",
            "extract",
            0,
            None,
            Some("extracting".into()),
        );
        let extracted = extract_libs(&tmp_for_task, is_zip, &dir_for_task)?;
        let canonical = make_canonical(&dir_for_task, &extracted)?;
        let _ = std::fs::remove_file(&tmp_for_task);
        Ok(canonical)
    })
    .await
    .map_err(|e| anyhow!("extraction task failed: {e}"))??;

    if let Some(marker) = version_marker_path() {
        let _ = std::fs::write(&marker, format!("{} ({})", RUNTIME_VERSION, asset.file));
    }
    strip_quarantine(&out).await;
    crate::core::tools::report(progress, "onnxruntime", "done", 1, Some(1), None);
    Ok(out)
}

/// Installs from a file the user already has on disk.
pub fn install_from_path(source: &Path) -> anyhow::Result<PathBuf> {
    if !source.is_file() {
        return Err(anyhow!("{} is not a file", source.display()));
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !is_runtime_lib(name) {
        return Err(anyhow!(
            "{} does not look like this system's ONNX Runtime library (expected something like {})",
            name,
            lib_filename()
        ));
    }
    let dir = target_dir().ok_or_else(|| anyhow!("could not find the app data directory"))?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let bytes = std::fs::read(source).with_context(|| format!("reading {}", source.display()))?;
    let dest = write_atomic(&dir, lib_filename(), &bytes)?;
    if let Some(marker) = version_marker_path() {
        let _ = std::fs::write(&marker, format!("local ({name})"));
    }
    Ok(dest)
}

/// Deletes the managed lib (the one pointed at by env var is not ours to touch).
pub fn remove_managed() -> anyhow::Result<()> {
    let dir = target_dir().ok_or_else(|| anyhow!("could not find the app data directory"))?;
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("deleting {}", dir.display()))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn strip_quarantine(path: &Path) {
    let p = path.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        crate::core::process::std_command("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(&p)
            .output()
    })
    .await;
}

#[cfg(not(target_os = "macos"))]
async fn strip_quarantine(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_asset_has_sha256_and_size() {
        assert!(!ASSETS.is_empty());
        for a in ASSETS {
            assert_eq!(a.sha256.len(), 64, "sha256 of {} has the wrong size", a.id);
            assert!(
                a.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "sha256 of {} is not hex",
                a.id
            );
            assert!(a.bytes > 1_000_000, "{} looks too small", a.id);
            assert!(
                a.file.contains(RUNTIME_VERSION),
                "{} is not the pinned version",
                a.file
            );
            assert!(a.file.ends_with(".tgz") || a.file.ends_with(".zip"));
        }
    }

    #[test]
    fn variant_ids_are_unique() {
        let mut ids: Vec<&str> = ASSETS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate variant id");
    }

    #[test]
    fn variant_list_is_only_for_this_system() {
        let os = current_os();
        for v in list_variants() {
            let a = asset_by_id(&v.id).expect("listed variant must exist in the catalog");
            assert_eq!(a.os, os);
        }
    }

    #[test]
    fn automatic_variant_is_in_catalog_and_is_the_recommended_one() {
        if let Some(auto) = auto_variant_id() {
            let a = asset_by_id(auto).expect("automatic variant outside the catalog");
            assert_eq!(a.os, current_os());
            let recs: Vec<String> = list_variants()
                .into_iter()
                .filter(|v| v.recommended)
                .map(|v| v.id)
                .collect();
            assert_eq!(recs, vec![auto.to_string()]);
        }
    }

    #[test]
    fn unknown_variant_errors_instead_of_falling_back_to_automatic() {
        let e = pick_asset(Some("plan9-risc"));
        assert!(e.is_err());
    }

    #[test]
    fn auto_and_empty_fall_back_to_the_system_variant() {
        if auto_variant_id().is_some() {
            assert_eq!(pick_asset(None).map(|a| a.id).ok(), auto_variant_id());
            assert_eq!(
                pick_asset(Some("auto")).map(|a| a.id).ok(),
                auto_variant_id()
            );
            assert_eq!(pick_asset(Some("  ")).map(|a| a.id).ok(), auto_variant_id());
        }
    }

    #[test]
    fn canonical_name_counts_as_a_library() {
        assert!(is_runtime_lib(lib_filename()));
        assert!(!is_runtime_lib("LICENSE"));
        assert!(!is_runtime_lib("VERSION_NUMBER"));
    }

    #[test]
    fn only_the_lib_inside_lib_gets_in_and_dsym_stays_out() {
        let base = format!("onnxruntime-osx-arm64-{}", RUNTIME_VERSION);
        let versioned = if cfg!(target_os = "windows") {
            "onnxruntime.dll".to_string()
        } else if cfg!(target_os = "macos") {
            format!("libonnxruntime.{}.dylib", RUNTIME_VERSION)
        } else {
            format!("libonnxruntime.so.{}", RUNTIME_VERSION)
        };
        assert!(is_lib_entry(&PathBuf::from(format!(
            "{base}/lib/{versioned}"
        ))));
        assert!(!is_lib_entry(&PathBuf::from(format!(
            "{base}/lib/libonnxruntime.1.27.1.dylib.dSYM/Contents/Resources/DWARF/{versioned}"
        ))));
        assert!(!is_lib_entry(&PathBuf::from(format!("{base}/{versioned}"))));
        assert!(!is_lib_entry(&PathBuf::from(format!("{base}/lib/LICENSE"))));
    }

    #[test]
    fn missing_runtime_message_says_what_to_do() {
        let msg = missing_runtime_error().to_string();
        assert!(msg.contains(lib_filename()) || msg.contains("Models"));
        assert!(msg.len() > 40, "message too short to be actionable");
    }

    /// Needs network: downloads the official package and checks the pinned sha256.
    #[test]
    #[ignore = "network: downloads the official ONNX Runtime and installs it in the data directory"]
    fn downloads_and_installs_the_real_runtime() {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let p = crate::core::tools::noop_progress();
        let path = rt
            .block_on(install_runtime(None, &p))
            .expect("ONNX Runtime installation");
        assert!(path.is_file());
        assert!(is_installed());
        init().expect("load the freshly installed ONNX Runtime");
    }
}
