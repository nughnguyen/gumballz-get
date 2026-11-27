use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const FIREFOX_STORE_URL: &str = "https://addons.mozilla.org/firefox/addon/gumballz-get/";
const GITHUB_REPO: &str = "nughnguyen/gumballz-get";
const CHROME_EXTENSION_ASSET_PREFIX: &str = "gumballz-get-chrome-extension-v";
/// Where the bridge records which browser paired, so Settings can show it as
/// installed. Written best-effort by `local_bridge::pair`.
const PAIRED_BROWSERS_FILE: &str = "browser-extension/paired.json";

#[derive(Debug, Clone, Serialize)]
pub struct BrowserExtensionStatus {
    pub browser: String,
    pub supported: bool,
    pub bundled_version: Option<String>,
    pub installable: bool,
    pub install_hint_key: String,
    pub store_url: Option<String>,
    /// True once the extension paired with the app over the local bridge
    /// (the only signal the app has that it is actually installed).
    pub connected: bool,
}

fn read_manifest_version(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("version")?.as_str().map(|s| s.to_string())
}

fn read_bundled_manifest_version(app: &AppHandle, browser: &str) -> Option<String> {
    let resource = format!("browser-extension/{}/manifest.json", browser);
    let path = app
        .path()
        .resolve(resource, tauri::path::BaseDirectory::Resource)
        .ok()?;
    read_manifest_version(&path)
}

/// Browsers that paired with the app over the local bridge
/// (see `local_bridge::pair`). Missing file / bad JSON means none.
fn read_paired_browsers(app: &AppHandle) -> Vec<String> {
    let Ok(dir) = app.path().app_data_dir() else {
        return Vec::new();
    };
    let raw = std::fs::read_to_string(dir.join(PAIRED_BROWSERS_FILE)).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    v.get("browsers")
        .and_then(|b| b.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn extension_export_dir(app: &AppHandle, browser: &str) -> Option<PathBuf> {
    let base = app.path().app_data_dir().ok()?;
    Some(base.join("browser-extension").join(browser))
}

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} not found", src.display()),
        ));
    }
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        copy_recursive(&from, &to)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_extension_status(
    app: AppHandle,
) -> Result<Vec<BrowserExtensionStatus>, String> {
    // `installable` used to be a hardcoded `true`. The Windows portable build
    // is the bare `gumballz-get.exe` with no `resources/` folder beside it, so the
    // export copied nothing and still reported a path — the user opened an
    // empty folder and had no idea why (issue #310). Ask the bundle whether the
    // extension is actually there; when it is not, the UI falls back to the
    // store link.
    let chrome_v = read_bundled_manifest_version(&app, "chrome");
    let firefox_v = read_bundled_manifest_version(&app, "firefox");
    let paired = read_paired_browsers(&app);
    Ok(vec![
        BrowserExtensionStatus {
            browser: "chrome".into(),
            supported: true,
            installable: chrome_v.is_some(),
            bundled_version: chrome_v,
            install_hint_key: "chrome".into(),
            store_url: None,
            connected: paired.iter().any(|b| b == "chrome"),
        },
        BrowserExtensionStatus {
            browser: "firefox".into(),
            supported: true,
            installable: firefox_v.is_some(),
            bundled_version: firefox_v,
            install_hint_key: "firefox".into(),
            store_url: Some(FIREFOX_STORE_URL.into()),
            connected: paired.iter().any(|b| b == "firefox"),
        },
        BrowserExtensionStatus {
            browser: "safari".into(),
            supported: false,
            bundled_version: None,
            installable: false,
            install_hint_key: "safari".into(),
            store_url: None,
            connected: false,
        },
    ])
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionExportResult {
    pub path: String,
    pub version: Option<String>,
    pub browser: String,
}

fn bundled_resource_dir(app: &AppHandle, browser: &str) -> Option<PathBuf> {
    let resource = format!("browser-extension/{}", browser);
    let src = app
        .path()
        .resolve(resource, tauri::path::BaseDirectory::Resource)
        .ok()?;
    if src.exists() {
        Some(src)
    } else {
        None
    }
}

fn export_bundled(app: &AppHandle, browser: &str) -> Result<ExtensionExportResult, String> {
    let src = bundled_resource_dir(app, browser).ok_or_else(|| {
        format!(
            "This build does not ship the {} extension files (the Windows portable .exe carries no resources folder).",
            browser
        )
    })?;
    let dst = extension_export_dir(app, browser)
        .ok_or_else(|| "could not derive extension dir".to_string())?;
    if dst.exists() {
        let _ = std::fs::remove_dir_all(&dst);
    }
    std::fs::create_dir_all(&dst).map_err(|e| format!("mkdir: {}", e))?;
    copy_recursive(&src, &dst).map_err(|e| format!("copy extension: {}", e))?;

    let version = read_bundled_manifest_version(app, browser);

    Ok(ExtensionExportResult {
        path: dst.to_string_lossy().into_owned(),
        version,
        browser: browser.to_string(),
    })
}

#[tauri::command]
pub async fn browser_extension_export(
    app: AppHandle,
    browser: String,
) -> Result<ExtensionExportResult, String> {
    if browser != "chrome" && browser != "firefox" {
        return Err(format!("browser '{}' not supported for export", browser));
    }
    export_bundled(&app, &browser).map_err(|e| {
        if browser == "firefox" {
            format!("{} Install it from the store instead.", e)
        } else {
            format!(
                "{} Use 'Cài tiện ích' to download it from the latest release instead.",
                e
            )
        }
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareInstallResult {
    pub folder_path: String,
    pub source: String,
    pub version: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// One-click install prep: make sure the unpacked extension folder exists,
/// then the UI opens the browser's extensions page and the user does the
/// last 2 clicks (Developer mode -> Load unpacked) by hand — Chrome/Cốc Cốc
/// block silent installs, so there is no automatic step after this one.
///
/// (a) bundled resources when the installer build ships them; (b) for
/// Chrome only, the release zip from the latest GitHub release when the
/// portable build carries no resources folder; (c) anything else is an
/// error naming what failed. Firefox has no release zip — without bundled
/// files it must come from the store.
#[tauri::command]
pub async fn browser_extension_prepare_install(
    app: AppHandle,
    browser: String,
) -> Result<PrepareInstallResult, String> {
    if browser != "chrome" && browser != "firefox" {
        return Err(format!("browser '{}' not supported for export", browser));
    }
    if bundled_resource_dir(&app, &browser).is_some() {
        let res = export_bundled(&app, &browser)?;
        return Ok(PrepareInstallResult {
            folder_path: res.path,
            source: "bundled".to_string(),
            version: res.version,
        });
    }
    if browser != "chrome" {
        return Err(
            "This build does not ship the firefox extension files (the Windows portable .exe carries no resources folder), and Firefox is not in the release assets. Install it from the store instead."
                .to_string(),
        );
    }
    download_chrome_extension(&app).await
}

async fn download_chrome_extension(app: &AppHandle) -> Result<PrepareInstallResult, String> {
    let client = crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    .map_err(|e| e.to_string())?;

    let api_url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let release: GitHubRelease = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch latest release: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Invalid release response: {}", e))?;

    let asset = release
        .assets
        .iter()
        .find(|a| {
            a.name.starts_with(CHROME_EXTENSION_ASSET_PREFIX) && a.name.ends_with(".zip")
        })
        .ok_or_else(|| {
            format!(
                "Latest release {} has no Chrome extension zip",
                release.tag_name
            )
        })?;

    let zip_bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download extension zip: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read extension zip: {}", e))?;

    let dst = extension_export_dir(app, "chrome")
        .ok_or_else(|| "could not derive extension dir".to_string())?;
    if dst.exists() {
        let _ = std::fs::remove_dir_all(&dst);
    }
    std::fs::create_dir_all(&dst).map_err(|e| format!("mkdir: {}", e))?;

    // Same traversal guard as the plugin zip installer: mangled names stay
    // inside the destination dir.
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid ZIP: {}", e))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let mangled = file.mangled_name();
        let outpath = dst.join(&mangled);
        if !outpath.starts_with(&dst) {
            return Err("Malicious zip path traversal detected".to_string());
        }
        if file.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    // The release zip packs the extension at its root, so manifest.json
    // must land directly in the folder Chrome loads unpacked.
    let manifest = dst.join("manifest.json");
    if !manifest.exists() {
        return Err(
            "Downloaded zip has no manifest.json at its root — not a Chrome extension package"
                .to_string(),
        );
    }
    let version = read_manifest_version(&manifest);

    Ok(PrepareInstallResult {
        folder_path: dst.to_string_lossy().into_owned(),
        source: "downloaded".to_string(),
        version,
    })
}

#[tauri::command]
pub async fn browser_extension_open_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if !path.exists() {
        return Err(format!("path not found: {}", path.display()));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("explorer: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open: {}", e))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("xdg-open: {}", e))?;
    }
    Ok(())
}
