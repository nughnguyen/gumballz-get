use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub min_gumballz_get_version: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub nav: Vec<PluginNavItem>,
    #[serde(default)]
    pub events: PluginEvents,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub settings_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub rust_crate: Option<String>,
    #[serde(default)]
    pub frontend_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginNavItem {
    pub route: String,
    pub label: HashMap<String, String>,
    #[serde(default)]
    pub icon_svg: Option<String>,
    #[serde(default = "default_nav_group")]
    pub group: NavGroup,
    #[serde(default = "default_nav_order")]
    pub order: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginEvents {
    #[serde(default)]
    pub progress: Vec<String>,
    #[serde(default)]
    pub complete: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NavGroup {
    Primary,
    Secondary,
}

impl Default for NavGroup {
    fn default() -> Self {
        NavGroup::Secondary
    }
}

fn default_nav_group() -> NavGroup {
    NavGroup::Secondary
}

fn default_nav_order() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub version: String,
    pub installed_at: String,
    pub updated_at: String,
    pub enabled: bool,
    pub repo: Option<String>,
    pub source_release: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub repo: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub official: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Latest published version of the plugin, when the registry knows it.
    /// Update checks compare the installed version against this instead of
    /// the repo's latest release tag — the official plugins ship from the
    /// main app repo, whose latest release tag is the app version, not the
    /// plugin version.
    #[serde(default)]
    pub version: Option<String>,
    /// GitHub release tag holding this plugin's `.zip` assets, when the
    /// plugin ships from a tagged release instead of the repo's latest
    /// release. `None` means the repo's latest release.
    #[serde(default)]
    pub release_tag: Option<String>,
    /// Per-platform downloads, keyed by the platform suffix
    /// (`windows-x86_64`, `linux-x86_64`, `macos-aarch64`, `macos-x86_64`).
    /// A platform with no entry here falls back to the repo-release
    /// download path (latest release, or `release_tag` when set).
    #[serde(default)]
    pub artifacts: Option<HashMap<String, RegistryArtifact>>,
    /// SHA-256 hex of the platform `.zip` asset, when the registry knows it.
    /// `None` means the registry does not publish a checksum for this plugin
    /// and the installer can only validate the zip + manifest. Kept as a
    /// fallback for registries that publish one checksum without the
    /// per-platform `artifacts` map.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Expected size in bytes of the platform `.zip` asset, when known.
    #[serde(default)]
    pub size: Option<u64>,
}

/// One downloadable build of a plugin for a single platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryArtifact {
    /// Direct download URL of the platform `.zip` asset.
    pub url: String,
    /// SHA-256 hex of the `.zip` at `url`, when the registry knows it.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Expected size in bytes of the `.zip`, when known.
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistry {
    #[serde(default)]
    pub schema_version: u32,
    pub plugins: Vec<RegistryEntry>,
}
