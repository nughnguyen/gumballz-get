use std::sync::Arc;

use futures::StreamExt;
use gumballz_plugin_sdk::{PluginHost, PluginManifest, PluginRegistry, RegistryEntry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::Emitter;
use tokio::io::AsyncWriteExt as _;

use crate::plugin_host::PluginHostImpl;
use crate::plugin_loader::{validate_plugin_id, PluginLoadError, PluginManager};

/// Hard cap for a plugin zip download when the registry publishes no size.
/// A registry `size` replaces this cap (the download must not exceed it).
const MAX_PLUGIN_ZIP_BYTES: u64 = 500 * 1024 * 1024;
/// Hard cap for the total uncompressed bytes of an extracted plugin zip
/// (zip-bomb guard). A plugin package is far smaller than this in practice.
const MAX_PLUGIN_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;

fn emit_plugins_changed(app: &tauri::AppHandle) {
    let _ = app.emit("plugins-changed", ());
}

fn build_plugin_host(
    app: &tauri::AppHandle,
    plugins_dir: std::path::PathBuf,
) -> Arc<dyn PluginHost> {
    Arc::new(PluginHostImpl::new(app.clone(), plugins_dir))
}

#[derive(Debug, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub loaded: bool,
    pub icon: Option<String>,
    pub nav: Vec<PluginNavInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_error: Option<PluginLoadError>,
}

#[derive(Debug, Serialize)]
pub struct PluginNavInfo {
    pub route: String,
    pub label: std::collections::HashMap<String, String>,
    pub icon_svg: Option<String>,
    pub group: String,
    pub order: u32,
}

#[tauri::command]
pub fn list_plugins(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
) -> Result<Vec<PluginInfo>, String> {
    let manager = state.blocking_read();
    let installed = manager.installed_plugins();

    let infos: Vec<PluginInfo> = installed
        .iter()
        .map(|entry| {
            let loaded = manager.get(&entry.id);
            match loaded {
                Some(lp) => PluginInfo {
                    id: entry.id.clone(),
                    name: lp.manifest.name.clone(),
                    version: lp.manifest.version.clone(),
                    description: lp.manifest.description.clone(),
                    author: lp.manifest.author.clone(),
                    enabled: entry.enabled,
                    loaded: true,
                    icon: lp.manifest.icon.clone(),
                    nav: lp
                        .manifest
                        .nav
                        .iter()
                        .map(|n| PluginNavInfo {
                            route: n.route.clone(),
                            label: n.label.clone(),
                            icon_svg: n.icon_svg.clone(),
                            group: match n.group {
                                gumballz_plugin_sdk::NavGroup::Primary => "primary".into(),
                                gumballz_plugin_sdk::NavGroup::Secondary => "secondary".into(),
                            },
                            order: n.order,
                        })
                        .collect(),
                    load_error: None,
                },
                None => {
                    let load_error = manager.load_error(&entry.id).cloned();
                    let manifest_path = manager.plugins_dir().join(&entry.id).join("plugin.json");
                    let manifest: Option<gumballz_plugin_sdk::PluginManifest> =
                        std::fs::read_to_string(&manifest_path)
                            .ok()
                            .and_then(|s| serde_json::from_str(&s).ok());
                    match manifest {
                        Some(m) => PluginInfo {
                            id: entry.id.clone(),
                            name: m.name,
                            version: m.version,
                            description: m.description,
                            author: m.author,
                            enabled: entry.enabled,
                            loaded: false,
                            icon: m.icon,
                            nav: m
                                .nav
                                .iter()
                                .map(|n| PluginNavInfo {
                                    route: n.route.clone(),
                                    label: n.label.clone(),
                                    icon_svg: n.icon_svg.clone(),
                                    group: match n.group {
                                        gumballz_plugin_sdk::NavGroup::Primary => "primary".into(),
                                        gumballz_plugin_sdk::NavGroup::Secondary => {
                                            "secondary".into()
                                        }
                                    },
                                    order: n.order,
                                })
                                .collect(),
                            load_error,
                        },
                        None => PluginInfo {
                            id: entry.id.clone(),
                            name: entry.id.clone(),
                            version: entry.version.clone(),
                            description: String::new(),
                            author: String::new(),
                            enabled: entry.enabled,
                            loaded: false,
                            icon: None,
                            nav: Vec::new(),
                            load_error,
                        },
                    }
                }
            }
        })
        .collect();

    Ok(infos)
}

#[tauri::command]
pub fn get_plugin_frontend_path(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
) -> Result<String, String> {
    let manager = state.blocking_read();
    let frontend_dir = manager
        .plugin_dir(&plugin_id)
        .map_err(|e| e.to_string())?
        .join("frontend");
    if !frontend_dir.exists() {
        return Err(format!("Plugin '{}' has no frontend", plugin_id));
    }
    Ok(frontend_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn set_plugin_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut manager = state.blocking_write();
        manager
            .set_enabled(&plugin_id, enabled)
            .map_err(|e| e.to_string())?;
        if enabled && !manager.is_loaded(&plugin_id) {
            let plugins_dir = manager.plugins_dir().to_path_buf();
            let host = build_plugin_host(&app, plugins_dir);
            let _ = manager.load_one(&plugin_id, host);
        }
    }
    emit_plugins_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn uninstall_plugin(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
) -> Result<(), String> {
    {
        let mut manager = state.blocking_write();
        manager.unregister(&plugin_id).map_err(|e| e.to_string())?;
    }
    emit_plugins_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_loaded_plugin_manifests(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
) -> Result<Vec<PluginManifest>, String> {
    let manager = state.blocking_read();
    Ok(manager.loaded_manifests().into_iter().cloned().collect())
}

#[tauri::command]
pub fn get_plugin_i18n(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
    locale: String,
) -> Result<serde_json::Value, String> {
    let manager = state.blocking_read();
    let i18n_dir = manager
        .plugin_dir(&plugin_id)
        .map_err(|e| e.to_string())?
        .join("i18n");
    let locale_file = i18n_dir.join(format!("{}.json", locale));
    if !locale_file.exists() {
        let fallback = i18n_dir.join("en.json");
        if !fallback.exists() {
            return Ok(serde_json::json!({}));
        }
        let content = std::fs::read_to_string(&fallback).map_err(|e| e.to_string())?;
        return serde_json::from_str(&content).map_err(|e| e.to_string());
    }
    let content = std::fs::read_to_string(&locale_file).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn plugin_command(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
    command: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let manager = state.read().await;
    let is_builtin = manager.is_builtin(&plugin_id);
    manager
        .handle_command(&plugin_id, &command, args)
        .await
        .map_err(|e| {
            let lower = e.to_lowercase();
            let unknown_command = lower.contains("unknown command")
                || (lower.contains("command") && lower.contains("not found"));
            if unknown_command && !lower.contains("not loaded") && !is_builtin {
                format!(
                    "ERR_PLUGIN_OUTDATED: The installed '{}' plugin is outdated and doesn't support '{}'. Update it in the Marketplace, then restart GumballZ Get. ({})",
                    plugin_id, command, e
                )
            } else {
                e
            }
        })
}

/// The marketplace registry lives in the main repo (`plugins.json` at the
/// repo root), so the official tools need no separate registry repo. The
/// jsDelivr mirror is the fallback when raw.githubusercontent.com is
/// unreachable.
const REGISTRY_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/nughnguyen/gumballz-get/main/plugins.json",
    "https://cdn.jsdelivr.net/gh/nughnguyen/gumballz-get@main/plugins.json",
];

#[derive(Debug, Serialize)]
pub struct MarketplaceEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub repo: String,
    pub homepage: Option<String>,
    pub tags: Vec<String>,
    pub official: bool,
    pub capabilities: Vec<String>,
    pub installed: bool,
    pub installed_version: Option<String>,
}

#[tauri::command]
pub async fn fetch_marketplace_registry(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
) -> Result<Vec<MarketplaceEntry>, String> {
    let client = crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    .map_err(|e| e.to_string())?;

    let mut registry: Option<PluginRegistry> = None;
    let mut last_err = String::from("Failed to fetch registry");

    for url in REGISTRY_URLS {
        let body = match client.get(*url).send().await {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.text().await {
                    Ok(body) => body,
                    Err(e) => {
                        last_err = format!("Failed to read registry: {}", e);
                        tracing::warn!("registry fetch via {} failed: {}", url, last_err);
                        continue;
                    }
                },
                Err(e) => {
                    last_err = format!("Registry request failed: {}", e);
                    tracing::warn!("registry fetch via {} failed: {}", url, last_err);
                    continue;
                }
            },
            Err(e) => {
                last_err = format!("Failed to fetch registry: {}", e);
                tracing::warn!("registry fetch via {} failed: {}", url, last_err);
                continue;
            }
        };

        match serde_json::from_str::<PluginRegistry>(&body) {
            Ok(parsed) => {
                registry = Some(parsed);
                break;
            }
            Err(e) => {
                last_err = format!("Invalid registry: {}", e);
                tracing::warn!("registry fetch via {} failed: {}", url, last_err);
            }
        }
    }

    let registry = registry.ok_or(last_err)?;

    let installed = {
        let manager = state.read().await;
        manager.installed_plugins().to_vec()
    };

    let entries = registry
        .plugins
        .into_iter()
        .map(|entry| {
            let inst = installed.iter().find(|i| i.id == entry.id);
            MarketplaceEntry {
                id: entry.id,
                name: entry.name,
                description: entry.description,
                author: entry.author,
                repo: entry.repo,
                homepage: entry.homepage,
                tags: entry.tags,
                official: entry.official,
                capabilities: entry.capabilities,
                installed: inst.is_some(),
                installed_version: inst.map(|i| i.version.clone()),
            }
        })
        .collect();

    Ok(entries)
}

fn platform_artifact_suffix() -> &'static str {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        "windows-x86_64"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "linux-x86_64"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "macos-aarch64"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "macos-x86_64"
    } else {
        "unknown"
    }
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
    #[serde(default)]
    size: Option<u64>,
}

/// Atomically move a fully-populated staging dir into place as
/// `<plugins>/<plugin_id>/` (the previous install, if any, is first moved
/// aside to `<plugins>/<plugin_id>.old` and restored if the move fails, and
/// removed once the new install is in place). Only then is the entry
/// registered in `installed.json`. On ANY failure the staging dir is deleted
/// and the previous install is left untouched (or restored). Shared by the
/// zip installers — the swap logic lives here exactly once.
async fn swap_staged_into_place(
    staging: &std::path::Path,
    plugin_id: &str,
    version: String,
    repo: Option<String>,
    source_release: Option<String>,
    state: &Arc<tokio::sync::RwLock<PluginManager>>,
) -> Result<String, String> {
    let (plugins_dir, dest_dir) = {
        let manager = state.read().await;
        let dest = manager.plugin_dir(plugin_id).map_err(|e| e.to_string())?;
        (manager.plugins_dir().to_path_buf(), dest)
    };
    let cleanup = |staging: &std::path::Path| {
        let _ = std::fs::remove_dir_all(staging);
    };
    // Atomic-ish swap: move the old install aside, move staging in, restore
    // the old install if the second rename fails.
    let backup_dir = plugins_dir.join(format!("{}.old", plugin_id));
    let mut moved_aside = false;
    if dest_dir.exists() {
        let _ = std::fs::remove_dir_all(&backup_dir);
        if let Err(e) = std::fs::rename(&dest_dir, &backup_dir) {
            cleanup(staging);
            return Err(format!("Failed to replace the old installation: {}", e));
        }
        moved_aside = true;
    }
    if let Err(e) = std::fs::rename(staging, &dest_dir) {
        if moved_aside {
            let _ = std::fs::rename(&backup_dir, &dest_dir);
        }
        cleanup(staging);
        return Err(format!("Failed to install the plugin: {}", e));
    }
    let _ = std::fs::remove_dir_all(&backup_dir);

    let mut manager = state.write().await;
    if let Err(e) = manager.register_installed(gumballz_plugin_sdk::InstalledPlugin {
        id: plugin_id.to_string(),
        version: version.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        enabled: true,
        repo,
        source_release,
    }) {
        // The files are in place but the registry write failed: roll the
        // install back so `installed.json` and the disk never disagree.
        let _ = std::fs::remove_dir_all(&dest_dir);
        return Err(format!("Failed to write the installed plugin list: {}", e));
    }

    Ok(version)
}

/// Extract `zip_path` into a fresh staging dir under the plugins dir,
/// validate the extracted `plugin.json` (it must parse and its `id` must
/// equal `plugin_id`), then atomically move the staging dir into place via
/// `swap_staged_into_place`. On ANY failure the staging dir is deleted and
/// the previous install is left untouched. Shared by the registry download
/// path and the install-from-file path — the extraction logic lives here
/// exactly once.
async fn install_from_zip_file(
    zip_path: &std::path::Path,
    plugin_id: &str,
    repo: Option<String>,
    source_release: Option<String>,
    state: &Arc<tokio::sync::RwLock<PluginManager>>,
) -> Result<String, String> {
    let plugins_dir = {
        let manager = state.read().await;
        manager.plugins_dir().to_path_buf()
    };
    std::fs::create_dir_all(&plugins_dir).map_err(|e| e.to_string())?;

    let staging = plugins_dir.join(format!(
        ".tmp-{}-{}",
        plugin_id,
        uuid::Uuid::new_v4().as_simple()
    ));
    let cleanup = |staging: &std::path::Path| {
        let _ = std::fs::remove_dir_all(staging);
    };
    if let Err(e) = std::fs::create_dir_all(&staging) {
        return Err(format!("Failed to create the temporary directory: {}", e));
    }

    let manifest = match extract_and_validate_zip(zip_path, &staging, plugin_id) {
        Ok(m) => m,
        Err(e) => {
            cleanup(&staging);
            return Err(e);
        }
    };
    let version = manifest.version.clone();

    swap_staged_into_place(&staging, plugin_id, version, repo, source_release, state).await
}

/// Open `zip_path`, extract it into `staging` (Zip-Slip + symlink + size
/// guards), then validate the extracted `plugin.json`: it must parse and its
/// `id` must equal `plugin_id`. Returns the parsed manifest.
fn extract_and_validate_zip(
    zip_path: &std::path::Path,
    staging: &std::path::Path,
    plugin_id: &str,
) -> Result<PluginManifest, String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Failed to open the .zip file: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Invalid .zip file: {}", e))?;

    let mut extracted_bytes: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // Reject symlinks: a symlink inside the zip could point the
        // extraction outside the staging dir.
        if let Some(mode) = entry.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err("The .zip file contains a symlink, rejected".into());
            }
        }
        extracted_bytes = extracted_bytes.saturating_add(entry.size());
        if extracted_bytes > MAX_PLUGIN_EXTRACTED_BYTES {
            return Err("The extracted .zip exceeds the allowed limit".into());
        }
        let mangled = entry.mangled_name();
        let outpath = staging.join(&mangled);
        if !outpath.starts_with(staging) {
            return Err("The .zip file contains an invalid path, rejected".into());
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    let manifest_path = staging.join("plugin.json");
    let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|_| {
        "Plugin package is missing plugin.json at the root of the .zip file".to_string()
    })?;
    let manifest: PluginManifest = serde_json::from_str(&manifest_str)
        .map_err(|e| format!("Invalid plugin.json: {}", e))?;
    if let Err(e) = validate_plugin_id(&manifest.id) {
        return Err(format!("plugin.json has an invalid id: {}", e));
    }
    if manifest.id != plugin_id {
        return Err(format!(
            "plugin.json has id '{}' but installing '{}'",
            manifest.id, plugin_id
        ));
    }
    Ok(manifest)
}

/// Read the `id` from the root `plugin.json` inside `zip_path` without
/// extracting anything. Used by install-from-file to learn which plugin the
/// user picked before running the shared install pipeline.
fn read_plugin_id_from_zip(zip_path: &std::path::Path) -> Result<String, String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Failed to open the .zip file: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Invalid .zip file: {}", e))?;
    let mut entry = archive
        .by_name("plugin.json")
        .map_err(|_| "Plugin package is missing plugin.json at the root of the .zip file".to_string())?;
    let mut content = String::new();
    use std::io::Read as _;
    entry
        .read_to_string(&mut content)
        .map_err(|e| format!("Failed to read plugin.json: {}", e))?;
    let manifest: PluginManifest = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid plugin.json: {}", e))?;
    validate_plugin_id(&manifest.id)
        .map_err(|e| format!("plugin.json has an invalid id: {}", e))?;
    Ok(manifest.id)
}

/// Stream `url` to `dest_path`, failing closed if the download exceeds
/// `size_cap` bytes. Returns the SHA-256 hex of the downloaded bytes.
async fn stream_download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest_path: &std::path::Path,
    size_cap: u64,
) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("NetworkUnreachable|Failed to download the plugin: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Failed to download the plugin: {}", e))?;
    if let Some(declared) = resp.content_length() {
        if declared > size_cap {
            return Err(format!(
                "The download exceeds the allowed limit ({} byte)",
                size_cap
            ));
        }
    }
    let mut file = tokio::fs::File::create(dest_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("NetworkUnreachable|Download error: {}", e))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > size_cap {
            return Err(format!(
                "The download exceeds the allowed limit ({} byte)",
                size_cap
            ));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Normalize a registry checksum to lowercase hex, failing closed on
/// anything that is not 64 hex chars — it can never match a real download,
/// so refuse upfront instead of downloading and then comparing against
/// garbage.
fn normalize_expected_sha256(expected_sha256: Option<String>) -> Result<Option<String>, String> {
    match expected_sha256.as_deref() {
        Some(raw) => {
            let normalized = raw.trim().trim_start_matches("sha256:").to_lowercase();
            let is_hex =
                normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit());
            if !is_hex {
                return Err("Invalid SHA-256 checksum in the registry".into());
            }
            Ok(Some(normalized))
        }
        None => Ok(None),
    }
}

/// Download the plugin `.zip` at `url` to a temp file (streaming, hashed,
/// capped at `size_cap`), verify its SHA-256 when `expected_sha256` is
/// known, then install it through the shared zip pipeline. The temp file
/// is always removed. `repo` / `source_release` are recorded on the
/// installed entry for update checks. Shared by the direct-URL path and
/// the repo-release path — the download-then-install tail lives here
/// exactly once.
async fn download_and_install_zip(
    state: &Arc<tokio::sync::RwLock<PluginManager>>,
    client: &reqwest::Client,
    url: &str,
    plugin_id: &str,
    expected_sha256: Option<String>,
    size_cap: u64,
    repo: Option<String>,
    source_release: Option<String>,
) -> Result<String, String> {
    let expected_sha256 = normalize_expected_sha256(expected_sha256)?;
    let plugins_dir = {
        let manager = state.read().await;
        manager.plugins_dir().to_path_buf()
    };
    std::fs::create_dir_all(&plugins_dir).map_err(|e| e.to_string())?;
    let download_path = plugins_dir.join(format!(
        ".download-{}-{}.zip",
        plugin_id,
        uuid::Uuid::new_v4().as_simple()
    ));

    let actual_sha256 = match stream_download_to_file(client, url, &download_path, size_cap).await
    {
        Ok(hash) => hash,
        Err(e) => {
            let _ = std::fs::remove_file(&download_path);
            return Err(e);
        }
    };

    // SHA-256: when the registry publishes one, the download must match it
    // exactly; when it publishes none, there is nothing to compare against
    // and the zip + manifest validation + the loader's ABI/rustc handshake
    // at `load_one` are the checks.
    if let Some(expected) = &expected_sha256 {
        if actual_sha256 != *expected {
            let _ = std::fs::remove_file(&download_path);
            return Err(
                "The downloaded file's SHA-256 checksum does not match the registry. Installation aborted."
                    .into(),
            );
        }
    }

    let version = match install_from_zip_file(
        &download_path,
        plugin_id,
        repo,
        source_release,
        state,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&download_path);
            return Err(e);
        }
    };
    let _ = std::fs::remove_file(&download_path);
    Ok(version)
}

/// Download a plugin `.zip` from a direct URL the registry publishes for
/// this platform (the official plugins), with no GitHub API call and no
/// asset matching. Falls back to the zip + manifest validation when the
/// registry publishes no checksum.
async fn install_plugin_zip_from_url(
    state: &Arc<tokio::sync::RwLock<PluginManager>>,
    plugin_id: String,
    url: String,
    expected_sha256: Option<String>,
    expected_size: Option<u64>,
    repo: Option<String>,
    source_release: Option<String>,
) -> Result<String, String> {
    validate_plugin_id(&plugin_id).map_err(|e| e.to_string())?;
    let client = crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    .map_err(|e| e.to_string())?;
    let size_cap = expected_size.unwrap_or(MAX_PLUGIN_ZIP_BYTES);
    download_and_install_zip(
        state,
        &client,
        &url,
        &plugin_id,
        expected_sha256,
        size_cap,
        repo,
        source_release,
    )
    .await
}

pub async fn install_plugin_zip_from_repo(
    state: &Arc<tokio::sync::RwLock<PluginManager>>,
    plugin_id: String,
    repo: String,
    expected_sha256: Option<String>,
    expected_size: Option<u64>,
    release_tag: Option<String>,
) -> Result<String, String> {
    validate_plugin_id(&plugin_id).map_err(|e| e.to_string())?;
    let suffix = platform_artifact_suffix();
    if suffix == "unknown" {
        return Err("This platform is not supported".to_string());
    }

    let client = crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    .map_err(|e| e.to_string())?;

    // A tagged release when the registry names one (the official plugins
    // ship from the rolling `plugins` release), else the repo's latest
    // release (community plugins).
    let api_url = match &release_tag {
        Some(tag) => format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo, tag
        ),
        None => format!("https://api.github.com/repos/{}/releases/latest", repo),
    };
    let release: GitHubRelease = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("NetworkUnreachable|Failed to download the release information: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Failed to download the release information: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Invalid release response: {}", e))?;

    // Prefer the asset named for this plugin (`<id>-<platform>.zip`): a
    // release holding several plugins would otherwise match the first
    // `.zip` for the platform, which may be another plugin. Fall back to
    // any `.zip` for the platform, for repos whose asset names do not
    // carry the plugin id.
    let asset = release
        .assets
        .iter()
        .find(|a| {
            a.name.contains(plugin_id.as_str())
                && a.name.contains(suffix)
                && a.name.ends_with(".zip")
        })
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|a| a.name.contains(suffix) && a.name.ends_with(".zip"))
        })
        .ok_or_else(|| format!("No build available for {}", suffix))?;

    // Size cap: the registry size when known (the download must not exceed
    // it), the asset size GitHub reports as a fallback, else 500 MB.
    let size_cap = expected_size
        .or(asset.size)
        .unwrap_or(MAX_PLUGIN_ZIP_BYTES);

    download_and_install_zip(
        state,
        &client,
        &asset.browser_download_url,
        &plugin_id,
        expected_sha256,
        size_cap,
        Some(repo),
        Some(release.tag_name.clone()),
    )
    .await
}

#[tauri::command]
pub async fn install_plugin_from_registry(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
    repo: String,
) -> Result<String, String> {
    // Best-effort registry lookup for the download URL, checksum, expected
    // size, and release tag. If the registry is unreachable the install
    // still proceeds via the repo's latest release, but without a checksum
    // to verify against (zip + manifest validation + the loader's ABI/rustc
    // handshake still apply).
    let registry_entry = match fetch_registry_entries().await {
        Ok(entries) => entries.into_iter().find(|e| e.id == plugin_id),
        Err(e) => {
            tracing::debug!(
                "install_plugin_from_registry: registry unavailable, no checksum: {}",
                e
            );
            None
        }
    };
    let suffix = platform_artifact_suffix();
    if suffix == "unknown" {
        return Err("This platform is not supported".to_string());
    }

    let version = if let Some(artifact) = registry_entry
        .as_ref()
        .and_then(|entry| entry.artifacts.as_ref())
        .and_then(|artifacts| artifacts.get(suffix))
    {
        // Direct download: the registry publishes the exact `.zip` URL
        // for this platform, so no GitHub API call and no asset matching.
        // The artifact checksum/size win; the entry-level ones are the
        // fallback for registries that publish a single checksum.
        let expected_sha256 = artifact
            .sha256
            .clone()
            .or_else(|| registry_entry.as_ref().and_then(|e| e.sha256.clone()));
        let expected_size = artifact
            .size
            .or_else(|| registry_entry.as_ref().and_then(|e| e.size));
        let release_tag = registry_entry.as_ref().and_then(|e| e.release_tag.clone());
        install_plugin_zip_from_url(
            &state.inner().clone(),
            plugin_id.clone(),
            artifact.url.clone(),
            expected_sha256,
            expected_size,
            Some(repo),
            release_tag,
        )
        .await?
    } else {
        let (expected_sha256, expected_size) = registry_entry
            .as_ref()
            .map(|e| (e.sha256.clone(), e.size))
            .unwrap_or((None, None));
        let release_tag = registry_entry.as_ref().and_then(|e| e.release_tag.clone());
        install_plugin_zip_from_repo(
            &state.inner().clone(),
            plugin_id.clone(),
            repo,
            expected_sha256,
            expected_size,
            release_tag,
        )
        .await?
    };
    {
        let mut manager = state.write().await;
        if !manager.is_loaded(&plugin_id) {
            let plugins_dir = manager.plugins_dir().to_path_buf();
            let host = build_plugin_host(&app, plugins_dir);
            let _ = manager.load_one(&plugin_id, host);
        }
    }
    emit_plugins_changed(&app);
    Ok(version)
}

/// Install a plugin from a local `.zip` file the user picks (e.g. from the
/// file picker). Same temp-dir pipeline as the registry path — zip opens,
/// `plugin.json` parses with a valid id matching the package, atomic move
/// into place, only then `register_installed` + `load_one` — except there is
/// no registry checksum to verify against, so zip + manifest validation +
/// the loader's ABI/rustc handshake at `load_one` are the checks.
#[tauri::command]
pub async fn install_plugin_from_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    path: String,
) -> Result<String, String> {
    let zip_path = std::path::PathBuf::from(&path);
    if !zip_path.is_file() {
        return Err("Plugin file not found. Please select a .zip file again.".into());
    }
    let is_zip = zip_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("zip"))
        .unwrap_or(false);
    if !is_zip {
        return Err("The plugin file must be a .zip file".into());
    }
    if let Ok(metadata) = std::fs::metadata(&zip_path) {
        if metadata.len() > MAX_PLUGIN_ZIP_BYTES {
            return Err("The plugin file exceeds the allowed limit (500 MB)".into());
        }
    }
    let plugin_id = read_plugin_id_from_zip(&zip_path)?;
    let version = install_from_zip_file(
        &zip_path,
        &plugin_id,
        None,
        None,
        &state.inner().clone(),
    )
    .await?;
    {
        let mut manager = state.write().await;
        if !manager.is_loaded(&plugin_id) {
            let plugins_dir = manager.plugins_dir().to_path_buf();
            let host = build_plugin_host(&app, plugins_dir);
            let _ = manager.load_one(&plugin_id, host);
        }
    }
    emit_plugins_changed(&app);
    Ok(version)
}

async fn fetch_registry_entries() -> Result<Vec<RegistryEntry>, String> {
    let client = crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    .map_err(|e| e.to_string())?;

    let mut last_err = String::from("Failed to fetch registry");
    for url in REGISTRY_URLS {
        match client.get(*url).send().await {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.text().await {
                    Ok(body) => match serde_json::from_str::<PluginRegistry>(&body) {
                        Ok(parsed) => return Ok(parsed.plugins),
                        Err(e) => last_err = format!("Invalid registry: {}", e),
                    },
                    Err(e) => last_err = format!("Failed to read registry: {}", e),
                },
                Err(e) => last_err = format!("Registry request failed: {}", e),
            },
            Err(e) => last_err = format!("Failed to fetch registry: {}", e),
        }
    }
    Err(last_err)
}

/// Latest published versions of the official plugins, from the registry
/// (`id` -> `(version, repo)`). The registry is the source of truth here:
/// the official plugins ship from the main app repo, whose latest release
/// tag is the app version, not the plugin version, so comparing against
/// the repo's latest release would report a phantom update (or miss a real
/// one) on every app release. Best-effort: an unreachable registry means
/// an empty map and callers fall back to the repo check.
async fn registry_official_versions() -> std::collections::HashMap<String, (String, String)> {
    fetch_registry_entries()
        .await
        .map(|entries| {
            entries
                .into_iter()
                .filter(|e| e.official)
                .filter_map(|e| e.version.map(|v| (e.id, (v, e.repo))))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn auto_update_plugins(state: Arc<tokio::sync::RwLock<PluginManager>>) {
    let installed = {
        let manager = state.read().await;
        manager.installed_plugins().to_vec()
    };

    let client = match crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("auto_update_plugins: client build failed: {}", e);
            return;
        }
    };

    let official_versions = registry_official_versions().await;

    for plugin in installed {
        // Official plugins: the registry version, not the repo's latest
        // release tag (which is the app version for the main repo).
        if let Some((latest, _)) = official_versions.get(&plugin.id) {
            if !latest.is_empty() && *latest != plugin.version {
                tracing::info!(
                    "plugin update available for '{}': {} -> {} (silent auto-execution disabled for security)",
                    plugin.id,
                    plugin.version,
                    latest
                );
            }
            continue;
        }
        let repo = match plugin.repo {
            Some(r) => r,
            None => continue,
        };
        let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let latest = match client.get(&api_url).send().await {
            Ok(resp) => match resp.json::<GitHubRelease>().await {
                Ok(r) => r.tag_name.trim_start_matches('v').to_string(),
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        if latest.is_empty() || latest == plugin.version {
            continue;
        }
        tracing::info!(
            "plugin update available for '{}': {} -> {} (silent auto-execution disabled for security)",
            plugin.id,
            plugin.version,
            latest
        );
    }
}

#[derive(Debug, Serialize)]
pub struct PluginUpdateInfo {
    pub id: String,
    pub installed_version: String,
    pub latest_version: String,
    pub repo: String,
    pub has_update: bool,
}

#[tauri::command]
pub async fn check_plugin_updates(
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
) -> Result<Vec<PluginUpdateInfo>, String> {
    let installed = {
        let manager = state.read().await;
        manager.installed_plugins().to_vec()
    };

    // Official plugins compare against the registry version (see
    // `registry_official_versions`); the rest keep the repo check below.
    let official_versions = registry_official_versions().await;

    let client = crate::core::http_client::apply_global_proxy(
        reqwest::Client::builder()
            .user_agent("GumballZ Get")
            .timeout(std::time::Duration::from_secs(15)),
    )
    .build()
    .map_err(|e| e.to_string())?;

    let mut updates = Vec::new();

    for plugin in &installed {
        if let Some((latest, repo)) = official_versions.get(&plugin.id) {
            updates.push(PluginUpdateInfo {
                id: plugin.id.clone(),
                installed_version: plugin.version.clone(),
                latest_version: latest.clone(),
                repo: repo.clone(),
                has_update: *latest != plugin.version,
            });
            continue;
        }
        let repo = match &plugin.repo {
            Some(r) => r.clone(),
            None => continue,
        };

        let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let release: GitHubRelease = match client.get(&api_url).send().await {
            Ok(resp) => match resp.json().await {
                Ok(r) => r,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let latest = release.tag_name.trim_start_matches('v').to_string();
        let has_update = latest != plugin.version;

        updates.push(PluginUpdateInfo {
            id: plugin.id.clone(),
            installed_version: plugin.version.clone(),
            latest_version: latest,
            repo,
            has_update,
        });
    }

    Ok(updates)
}

#[tauri::command]
pub async fn update_plugin(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<tokio::sync::RwLock<PluginManager>>>,
    plugin_id: String,
    repo: String,
) -> Result<String, String> {
    install_plugin_from_registry(app, state, plugin_id, repo).await
}
