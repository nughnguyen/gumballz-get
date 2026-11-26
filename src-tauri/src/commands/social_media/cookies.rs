#![allow(clippy::all)]
#![allow(warnings)]

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DetectedBrowser {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub profile_dir: String,
    pub available: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserIdentity {
    pub id: String,
    pub name: String,
    pub engine: String,
}

struct BrowserCandidateDef {
    id: &'static str,
    name: &'static str,
    engine: &'static str,
    win_dirs: &'static [&'static str],
    mac_dirs: &'static [&'static str],
    linux_dirs: &'static [&'static str],
}

pub struct SafeCookieSandbox {
    pub temp_path: PathBuf,
}

impl SafeCookieSandbox {
    pub fn create(master_path: &Path, temp_base: &Path) -> Result<Self, String> {
        if !master_path.exists() {
            return Err(format!(
                "Master cookie file does not exist: {}",
                master_path.display()
            ));
        }

        fs::create_dir_all(temp_base)
            .map_err(|e| format!("Failed to create temp cookie directory: {}", e))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let rand_suffix: u32 = rand::random();
        let temp_filename = format!("social_media_cookie_{}_{:x}.txt", timestamp, rand_suffix);
        let temp_path = temp_base.join(temp_filename);

        fs::copy(master_path, &temp_path).map_err(|e| {
            format!(
                "Failed to copy cookie file to sandbox {} -> {}: {}",
                master_path.display(),
                temp_path.display(),
                e
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&temp_path, perms);
        }

        Ok(Self { temp_path })
    }

    pub fn path(&self) -> &Path {
        &self.temp_path
    }
}

impl Drop for SafeCookieSandbox {
    fn drop(&mut self) {
        if self.temp_path.exists() {
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

/// A throwaway copy of one Chromium profile's cookie database, laid out
/// like a real "User Data" dir so gallery-dl/yt-dlp can read it through
/// `--cookies-from-browser chrome:<dir>`. Removed when dropped.
///
/// Why this exists: a running Chromium browser locks its live `Cookies`
/// file, so gallery-dl reading it directly fails on Windows with
/// `[Errno 13] Permission denied` — while a plain file copy of the same
/// database succeeds. The copy also carries the "Local State" file, which
/// gallery-dl needs to decrypt Chrome v10+ cookie values (AES-GCM key).
pub struct ShadowChromeProfile {
    pub dir: PathBuf,
}

impl Drop for ShadowChromeProfile {
    fn drop(&mut self) {
        if self.dir.exists() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Copy one Chromium profile's cookie database (+ WAL) and the "Local
/// State" key file into a fresh temp "User Data" dir. `profile_dir` is the
/// profile folder (e.g. `.../User Data/Profile 5`); returns the shadow
/// User Data dir, or `None` when the copy cannot be made (callers fall
/// back to pointing at the live profile, i.e. the previous behaviour).
/// A missing "Local State" is tolerated (pre-v10 cookie scheme); a
/// present-but-uncopyable one aborts, since the shadow would hold
/// undecryptable values.
pub fn build_chrome_shadow_profile(
    user_data_dir: &Path,
    profile_dir: &Path,
    temp_base: &Path,
) -> Option<PathBuf> {
    let profile_name = profile_dir.file_name()?.to_str()?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let rand_suffix: u32 = rand::random();
    let shadow = temp_base.join(format!(
        "chrome_shadow_{}_{:x}",
        timestamp, rand_suffix
    ));
    let shadow_network = shadow.join(profile_name).join("Network");
    fs::create_dir_all(&shadow_network).ok()?;

    // Modern location first, legacy `Cookies` next to the profile root
    // second; both are normalized into the modern layout in the shadow.
    let db_src = {
        let modern = profile_dir.join("Network").join("Cookies");
        if modern.is_file() {
            modern
        } else {
            profile_dir.join("Cookies")
        }
    };
    let db_dst = shadow_network.join("Cookies");
    if !db_src.is_file() || fs::copy(&db_src, &db_dst).is_err() {
        let _ = fs::remove_dir_all(&shadow);
        return None;
    }
    let wal_src = PathBuf::from(format!("{}-wal", db_src.display()));
    if wal_src.is_file() {
        let _ = fs::copy(
            &wal_src,
            PathBuf::from(format!("{}-wal", db_dst.display())),
        );
    }

    let state_src = user_data_dir.join("Local State");
    if state_src.is_file() && fs::copy(&state_src, shadow.join("Local State")).is_err() {
        let _ = fs::remove_dir_all(&shadow);
        return None;
    }

    Some(shadow)
}

pub struct ResolvedCookies {
    pub sandbox: Option<SafeCookieSandbox>,
    pub browser_arg: Option<String>,
    /// Shadow copy of a Chromium profile (live DB is locked while the
    /// browser runs); kept alive for the whole download, removed on drop.
    pub shadow_profile: Option<ShadowChromeProfile>,
}

impl ResolvedCookies {
    pub fn empty() -> Self {
        Self {
            sandbox: None,
            browser_arg: None,
            shadow_profile: None,
        }
    }

    pub fn cookie_file_path(&self) -> Option<&Path> {
        self.sandbox.as_ref().map(|s| s.path())
    }

    pub fn browser_arg(&self) -> Option<&str> {
        self.browser_arg.as_deref()
    }

    pub fn has_cookies(&self) -> bool {
        self.sandbox.is_some() || self.browser_arg.is_some()
    }
}

pub fn parse_windows_default_browser_prog_id(raw_output: &str) -> Option<String> {
    if raw_output.is_empty() {
        return None;
    }
    for line in raw_output.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find("ProgId") {
            let after = trimmed[pos + "ProgId".len()..].trim();
            let parts: Vec<&str> = after.split_whitespace().collect();
            if parts.len() == 2 && parts[0].starts_with("REG_") {
                let prog_id = parts[1].trim();
                if !prog_id.is_empty()
                    && prog_id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
                {
                    return Some(prog_id.to_string());
                }
            }
        }
    }
    None
}

pub fn map_prog_id_to_browser_id(prog_id: &str, command_output: Option<&str>) -> BrowserIdentity {
    let p_lower = prog_id.to_ascii_lowercase();
    let cmd = command_output.unwrap_or("").to_ascii_lowercase();

    if p_lower.contains("helium") || cmd.contains("helium") {
        return BrowserIdentity {
            id: "helium".to_string(),
            name: "Helium".to_string(),
            engine: "chrome".to_string(),
        };
    }
    if p_lower.starts_with("chromehtml") || p_lower.contains("chrome") || cmd.contains("chrome.exe")
    {
        return BrowserIdentity {
            id: "chrome".to_string(),
            name: "Google Chrome".to_string(),
            engine: "chrome".to_string(),
        };
    }
    if p_lower.starts_with("msedge")
        || p_lower.contains("edge")
        || p_lower.starts_with("appxq0fevzme2pys62n3e0fbqsw5cj08ege9")
        || cmd.contains("msedge.exe")
    {
        return BrowserIdentity {
            id: "edge".to_string(),
            name: "Microsoft Edge".to_string(),
            engine: "edge".to_string(),
        };
    }
    if p_lower.starts_with("brave") || p_lower.contains("brave") || cmd.contains("brave.exe") {
        return BrowserIdentity {
            id: "brave".to_string(),
            name: "Brave Browser".to_string(),
            engine: "brave".to_string(),
        };
    }
    if p_lower.starts_with("firefox") || p_lower.contains("firefox") || cmd.contains("firefox.exe")
    {
        return BrowserIdentity {
            id: "firefox".to_string(),
            name: "Mozilla Firefox".to_string(),
            engine: "firefox".to_string(),
        };
    }
    if p_lower.contains("zen") || cmd.contains("zen") {
        return BrowserIdentity {
            id: "zen".to_string(),
            name: "Zen Browser".to_string(),
            engine: "firefox".to_string(),
        };
    }
    if p_lower.starts_with("opera") || p_lower.contains("opera") || cmd.contains("opera.exe") {
        return BrowserIdentity {
            id: "opera".to_string(),
            name: "Opera".to_string(),
            engine: "opera".to_string(),
        };
    }
    if p_lower.starts_with("vivaldi") || p_lower.contains("vivaldi") || cmd.contains("vivaldi.exe")
    {
        return BrowserIdentity {
            id: "vivaldi".to_string(),
            name: "Vivaldi".to_string(),
            engine: "vivaldi".to_string(),
        };
    }
    // Unbranded Chromium registers ProgId "ChromiumHTM". gallery-dl and
    // yt-dlp both know the "chromium" browser key, so map it explicitly
    // instead of letting the fallback pass the raw ProgId through
    // ("chromiumhtm"), which they reject with "Unsupported browser".
    if p_lower.contains("chromium") {
        return BrowserIdentity {
            id: "chromium".to_string(),
            name: "Chromium".to_string(),
            engine: "chromium".to_string(),
        };
    }
    // Coc Coc is Chromium-based but has no gallery-dl/yt-dlp browser key;
    // it gets its own id so its profile dir can be found, and the cookie
    // arg is resolved through the chrome extractor (see
    // resolve_browser_cookie_arg).
    if p_lower.contains("coccoc") {
        return BrowserIdentity {
            id: "coccoc".to_string(),
            name: "Coc Coc".to_string(),
            engine: "chrome".to_string(),
        };
    }

    // Never pass an unrecognised ProgId through as the browser id:
    // gallery-dl/yt-dlp reject unknown keys outright, so an unknown
    // Windows HTM ProgId (a Chromium fork in practice) falls back to
    // "chromium", whose default profile both tools know how to read.
    BrowserIdentity {
        id: "chromium".to_string(),
        name: if prog_id.is_empty() {
            "Chromium".to_string()
        } else {
            prog_id.to_string()
        },
        engine: "chromium".to_string(),
    }
}

pub fn parse_mac_default_browser_bundle_id(raw_output: &str) -> Option<String> {
    if raw_output.is_empty() {
        return None;
    }
    let blocks: Vec<&str> = raw_output.split(|c| c == '{' || c == '}').collect();
    let mut http_bundle: Option<String> = None;

    for block in blocks {
        let is_https = block.contains("LSHandlerURLScheme = https")
            || block.contains("LSHandlerURLScheme = \"https\"");
        let is_http = block.contains("LSHandlerURLScheme = http")
            || block.contains("LSHandlerURLScheme = \"http\"");
        if !is_https && !is_http {
            continue;
        }

        let role_section = if let Some(pos) = block.find("LSHandlerRoleAll") {
            Some(&block[pos..])
        } else if let Some(pos) = block.find("LSHandlerRoleViewer") {
            Some(&block[pos..])
        } else {
            None
        };

        if let Some(role_str) = role_section {
            if let Some(eq_pos) = role_str.find('=') {
                let rest = &role_str[eq_pos + 1..];
                let semi_pos = rest.find(';').unwrap_or(rest.len());
                let bundle = rest[..semi_pos]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim();
                if !bundle.is_empty() {
                    if is_https {
                        return Some(bundle.to_string());
                    }
                    if http_bundle.is_none() {
                        http_bundle = Some(bundle.to_string());
                    }
                }
            }
        }
    }
    http_bundle
}

pub fn map_bundle_id_to_browser_id(bundle_id: &str) -> BrowserIdentity {
    let lower = bundle_id.to_ascii_lowercase();
    if lower.contains("helium") {
        return BrowserIdentity {
            id: "helium".to_string(),
            name: "Helium".to_string(),
            engine: "chrome".to_string(),
        };
    }
    if lower.contains("chrome") {
        return BrowserIdentity {
            id: "chrome".to_string(),
            name: "Google Chrome".to_string(),
            engine: "chrome".to_string(),
        };
    }
    if lower.contains("edge") {
        return BrowserIdentity {
            id: "edge".to_string(),
            name: "Microsoft Edge".to_string(),
            engine: "edge".to_string(),
        };
    }
    if lower.contains("brave") {
        return BrowserIdentity {
            id: "brave".to_string(),
            name: "Brave Browser".to_string(),
            engine: "brave".to_string(),
        };
    }
    if lower.contains("firefox") {
        return BrowserIdentity {
            id: "firefox".to_string(),
            name: "Mozilla Firefox".to_string(),
            engine: "firefox".to_string(),
        };
    }
    if lower.contains("zen") {
        return BrowserIdentity {
            id: "zen".to_string(),
            name: "Zen Browser".to_string(),
            engine: "firefox".to_string(),
        };
    }
    if lower.contains("opera") {
        return BrowserIdentity {
            id: "opera".to_string(),
            name: "Opera".to_string(),
            engine: "opera".to_string(),
        };
    }
    if lower.contains("vivaldi") {
        return BrowserIdentity {
            id: "vivaldi".to_string(),
            name: "Vivaldi".to_string(),
            engine: "vivaldi".to_string(),
        };
    }
    if lower.contains("safari") {
        return BrowserIdentity {
            id: "safari".to_string(),
            name: "Safari".to_string(),
            engine: "safari".to_string(),
        };
    }
    BrowserIdentity {
        id: lower,
        name: bundle_id.to_string(),
        engine: "chromium".to_string(),
    }
}

pub fn parse_linux_default_browser_desktop(raw_output: &str) -> Option<String> {
    if raw_output.is_empty() {
        return None;
    }
    for line in raw_output.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("ERROR:") && !trimmed.contains("not found") {
            if trimmed.ends_with(".desktop")
                || trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
            {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn map_linux_desktop_to_browser_id(desktop_file: &str) -> BrowserIdentity {
    let lower = desktop_file.to_ascii_lowercase();
    if lower.contains("helium") {
        return BrowserIdentity {
            id: "helium".to_string(),
            name: "Helium".to_string(),
            engine: "chrome".to_string(),
        };
    }
    if lower.contains("chrome") {
        return BrowserIdentity {
            id: "chrome".to_string(),
            name: "Google Chrome".to_string(),
            engine: "chrome".to_string(),
        };
    }
    if lower.contains("edge") {
        return BrowserIdentity {
            id: "edge".to_string(),
            name: "Microsoft Edge".to_string(),
            engine: "edge".to_string(),
        };
    }
    if lower.contains("brave") {
        return BrowserIdentity {
            id: "brave".to_string(),
            name: "Brave Browser".to_string(),
            engine: "brave".to_string(),
        };
    }
    if lower.contains("firefox") {
        return BrowserIdentity {
            id: "firefox".to_string(),
            name: "Mozilla Firefox".to_string(),
            engine: "firefox".to_string(),
        };
    }
    if lower.contains("zen") {
        return BrowserIdentity {
            id: "zen".to_string(),
            name: "Zen Browser".to_string(),
            engine: "firefox".to_string(),
        };
    }
    if lower.contains("opera") {
        return BrowserIdentity {
            id: "opera".to_string(),
            name: "Opera".to_string(),
            engine: "opera".to_string(),
        };
    }
    if lower.contains("vivaldi") {
        return BrowserIdentity {
            id: "vivaldi".to_string(),
            name: "Vivaldi".to_string(),
            engine: "vivaldi".to_string(),
        };
    }
    BrowserIdentity {
        id: lower.trim_end_matches(".desktop").to_string(),
        name: desktop_file.to_string(),
        engine: "chromium".to_string(),
    }
}

pub fn detect_system_default_browser() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("reg")
            .args([
                "query",
                "HKCU\\Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\https\\UserChoice",
                "/v",
                "ProgId",
            ])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        if let Some(prog_id) = parse_windows_default_browser_prog_id(&out) {
            let cmd_out = Command::new("reg")
                .args(["query", &format!("HKCR\\{}\\shell\\open\\command", prog_id)])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok());
            return Some(map_prog_id_to_browser_id(&prog_id, cmd_out.as_deref()).id);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let out = Command::new("defaults")
            .args([
                "read",
                "com.apple.LaunchServices/com.apple.launchservices.secure",
                "LSHandlers",
            ])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        if let Some(bundle_id) = parse_mac_default_browser_bundle_id(&out) {
            return Some(map_bundle_id_to_browser_id(&bundle_id).id);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("xdg-settings")
            .args(["get", "default-web-browser"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        if let Some(desktop) = parse_linux_default_browser_desktop(&out) {
            return Some(map_linux_desktop_to_browser_id(&desktop).id);
        }

        let mime_out = Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/https"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        if let Some(desktop) = parse_linux_default_browser_desktop(&mime_out) {
            return Some(map_linux_desktop_to_browser_id(&desktop).id);
        }
    }

    None
}

pub fn list_detected_browsers() -> Vec<DetectedBrowser> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let default_id = detect_system_default_browser();

    let candidates: [BrowserCandidateDef; 11] = [
        BrowserCandidateDef {
            id: "chrome",
            name: "Google Chrome",
            engine: "chrome",
            win_dirs: &["Google/Chrome/User Data"],
            mac_dirs: &["Library/Application Support/Google/Chrome"],
            linux_dirs: &[
                ".config/google-chrome",
                ".var/app/com.google.Chrome/config/google-chrome",
                ".config/chromium",
                ".var/app/org.chromium.Chromium/config/chromium",
                ".config/google-chrome-for-testing",
            ],
        },
        BrowserCandidateDef {
            id: "chromium",
            name: "Chromium",
            engine: "chromium",
            win_dirs: &["Chromium/User Data"],
            mac_dirs: &["Library/Application Support/Chromium"],
            linux_dirs: &[
                ".config/chromium",
                ".var/app/org.chromium.Chromium/config/chromium",
            ],
        },
        BrowserCandidateDef {
            id: "edge",
            name: "Microsoft Edge",
            engine: "edge",
            win_dirs: &["Microsoft/Edge/User Data"],
            mac_dirs: &["Library/Application Support/Microsoft Edge"],
            linux_dirs: &[
                ".config/microsoft-edge",
                ".var/app/com.microsoft.Edge/config/microsoft-edge",
            ],
        },
        BrowserCandidateDef {
            id: "brave",
            name: "Brave Browser",
            engine: "brave",
            win_dirs: &["BraveSoftware/Brave-Browser/User Data"],
            mac_dirs: &["Library/Application Support/BraveSoftware/Brave-Browser"],
            linux_dirs: &[
                ".config/BraveSoftware/Brave-Browser",
                ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
            ],
        },
        BrowserCandidateDef {
            id: "firefox",
            name: "Mozilla Firefox",
            engine: "firefox",
            win_dirs: &["Mozilla/Firefox/Profiles", "Mozilla/Firefox"],
            mac_dirs: &[
                "Library/Application Support/Firefox/Profiles",
                "Library/Application Support/Firefox",
            ],
            linux_dirs: &[
                ".mozilla/firefox",
                ".config/mozilla/firefox",
                ".var/app/org.mozilla.firefox/.mozilla/firefox",
            ],
        },
        BrowserCandidateDef {
            id: "zen",
            name: "Zen Browser",
            engine: "firefox",
            win_dirs: &["zen/Profiles", "zen"],
            mac_dirs: &[
                "Library/Application Support/zen/Profiles",
                "Library/Application Support/zen",
            ],
            linux_dirs: &[".zen", ".var/app/app.zen_browser.zen/.zen"],
        },
        BrowserCandidateDef {
            id: "helium",
            name: "Helium",
            engine: "chrome",
            win_dirs: &["imput/Helium/User Data"],
            mac_dirs: &["Library/Application Support/net.imput.Helium"],
            linux_dirs: &[".config/helium", ".var/app/net.imput.Helium/config/helium"],
        },
        BrowserCandidateDef {
            id: "coccoc",
            name: "Coc Coc",
            engine: "chrome",
            win_dirs: &["CocCoc/Browser/User Data"],
            mac_dirs: &["Library/Application Support/CocCoc"],
            linux_dirs: &[".config/coccoc"],
        },
        BrowserCandidateDef {
            id: "opera",
            name: "Opera",
            engine: "opera",
            win_dirs: &["Opera Software/Opera Stable"],
            mac_dirs: &["Library/Application Support/com.operasoftware.Opera"],
            linux_dirs: &[".config/opera", ".var/app/com.opera.Opera/config/opera"],
        },
        BrowserCandidateDef {
            id: "vivaldi",
            name: "Vivaldi",
            engine: "vivaldi",
            win_dirs: &["Vivaldi/User Data"],
            mac_dirs: &["Library/Application Support/Vivaldi"],
            linux_dirs: &[
                ".config/vivaldi",
                ".var/app/com.vivaldi.Vivaldi/config/vivaldi",
            ],
        },
        BrowserCandidateDef {
            id: "librewolf",
            name: "LibreWolf",
            engine: "firefox",
            win_dirs: &["librewolf/Profiles"],
            mac_dirs: &["Library/Application Support/librewolf/Profiles"],
            linux_dirs: &[
                ".librewolf",
                ".var/app/io.gitlab.librewolf-community/.librewolf",
            ],
        },
    ];

    let mut result = Vec::new();

    for c in &candidates {
        let (dirs, prefix): (&[&str], PathBuf) = if cfg!(target_os = "windows") {
            let app_data = std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join("AppData").join("Local"));
            (c.win_dirs, app_data)
        } else if cfg!(target_os = "macos") {
            (c.mac_dirs, home.clone())
        } else {
            (c.linux_dirs, home.clone())
        };

        let mut found_dir = PathBuf::new();
        let mut available = false;

        for d in dirs {
            let check = prefix.join(d);
            if check.exists() {
                found_dir = check;
                available = true;
                break;
            }
        }

        if !available && !dirs.is_empty() {
            found_dir = prefix.join(dirs[0]);
        }

        let is_default = default_id.as_deref() == Some(c.id);

        result.push(DetectedBrowser {
            id: c.id.to_string(),
            name: c.name.to_string(),
            engine: c.engine.to_string(),
            profile_dir: found_dir.to_string_lossy().to_string(),
            available,
            is_default,
        });
    }

    result
}

pub fn find_gecko_profile_dir(base_dir: &Path) -> Option<PathBuf> {
    if !base_dir.exists() {
        return None;
    }

    if base_dir.join("cookies.sqlite").exists() {
        return Some(base_dir.to_path_buf());
    }

    let ini_path = base_dir.join("profiles.ini");
    if ini_path.exists() {
        if let Ok(content) = fs::read_to_string(&ini_path) {
            let mut default_path: Option<String> = None;
            let mut first_path: Option<String> = None;
            let mut current_path: Option<String> = None;
            let mut current_is_default = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') {
                    if let Some(cp) = current_path.take() {
                        if current_is_default && default_path.is_none() {
                            default_path = Some(cp.clone());
                        }
                        if first_path.is_none() {
                            first_path = Some(cp);
                        }
                    }
                    current_is_default = false;
                } else if let Some(p) = trimmed.strip_prefix("Path=") {
                    current_path = Some(p.trim().to_string());
                } else if trimmed == "Default=1" {
                    current_is_default = true;
                }
            }
            if let Some(cp) = current_path {
                if current_is_default && default_path.is_none() {
                    default_path = Some(cp.clone());
                }
                if first_path.is_none() {
                    first_path = Some(cp);
                }
            }

            let chosen_rel = default_path.or(first_path);
            if let Some(rel) = chosen_rel {
                let full = if Path::new(&rel).is_absolute() {
                    PathBuf::from(rel)
                } else {
                    base_dir.join(rel)
                };
                if full.join("cookies.sqlite").exists() {
                    return Some(full);
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(base_dir) {
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    subdirs.push(entry.path());
                }
            }
        }
        subdirs.sort_by(|a, b| {
            let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let a_score = if a_name.contains("default-release") {
                2
            } else if a_name.contains("default") {
                1
            } else {
                0
            };
            let b_score = if b_name.contains("default-release") {
                2
            } else if b_name.contains("default") {
                1
            } else {
                0
            };
            b_score.cmp(&a_score)
        });

        for dir in subdirs {
            if dir.join("cookies.sqlite").exists() {
                return Some(dir);
            }
        }
    }

    None
}

pub fn extract_gecko_cookies_to_netscape(
    profile_dir: &Path,
    platform_domains: &[&str],
) -> Option<String> {
    let sqlite_file = profile_dir.join("cookies.sqlite");
    if !sqlite_file.exists() {
        return None;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let rand_suffix: u32 = rand::random();
    let temp_db = std::env::temp_dir().join(format!(
        "gumballz_get_gecko_{}_{:x}.sqlite",
        timestamp, rand_suffix
    ));
    let temp_wal = std::env::temp_dir().join(format!(
        "gumballz_get_gecko_{}_{:x}.sqlite-wal",
        timestamp, rand_suffix
    ));
    let src_wal = profile_dir.join("cookies.sqlite-wal");

    if fs::copy(&sqlite_file, &temp_db).is_err() {
        return None;
    }
    if src_wal.exists() {
        let _ = fs::copy(&src_wal, &temp_wal);
    }

    let result = (|| -> Result<String, rusqlite::Error> {
        let conn = Connection::open_with_flags(
            &temp_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        let mut query =
            "SELECT host, path, isSecure, expiry, name, value, isHttpOnly FROM moz_cookies"
                .to_string();
        if !platform_domains.is_empty() {
            let clauses: Vec<String> = platform_domains
                .iter()
                .map(|_| "host LIKE ?".to_string())
                .collect();
            query.push_str(" WHERE ");
            query.push_str(&clauses.join(" OR "));
        }

        let mut stmt = conn.prepare(&query)?;
        let mut rows = if platform_domains.is_empty() {
            stmt.query([])?
        } else {
            let params: Vec<&dyn rusqlite::ToSql> = platform_domains
                .iter()
                .map(|d| d as &dyn rusqlite::ToSql)
                .collect();
            stmt.query(params.as_slice())?
        };

        let mut lines = vec!["# Netscape HTTP Cookie File".to_string()];
        let mut count = 0;

        while let Some(row) = rows.next()? {
            let host: String = row.get(0).unwrap_or_default();
            let path: String = row.get(1).unwrap_or_else(|_| "/".to_string());
            let is_secure: i64 = row.get(2).unwrap_or(0);
            let mut expiry: i64 = row.get(3).unwrap_or(0);
            let name: String = row.get(4).unwrap_or_default();
            let value: String = row.get(5).unwrap_or_default();
            let is_http_only: i64 = row.get(6).unwrap_or(0);

            if host.is_empty() || name.is_empty() {
                continue;
            }

            if expiry > 100_000_000_000_000 {
                expiry /= 1_000_000;
            } else if expiry > 100_000_000_000 {
                expiry /= 1_000;
            }

            let domain = if is_http_only == 1 {
                format!("#HttpOnly_{}", host)
            } else {
                host.clone()
            };
            let sub = if host.starts_with('.') {
                "TRUE"
            } else {
                "FALSE"
            };
            let sec = if is_secure == 1 { "TRUE" } else { "FALSE" };

            lines.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                domain, sub, path, sec, expiry, name, value
            ));
            count += 1;
        }

        if count == 0 {
            return Ok(String::new());
        }

        lines.push(String::new());
        Ok(lines.join("\n"))
    })();

    let _ = fs::remove_file(&temp_db);
    let _ = fs::remove_file(&temp_wal);

    match result {
        Ok(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

pub fn resolve_browser_cookie_arg(browser_id: &str, profile_dir: Option<&str>) -> Option<String> {
    let trimmed = browser_id.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower == "auto" {
        let def = detect_system_default_browser();
        return def.and_then(|id| resolve_browser_cookie_arg(&id, None));
    }
    if lower == "zen" {
        if let Some(prof) = profile_dir {
            let p = Path::new(prof);
            if let Some(gecko) = find_gecko_profile_dir(p) {
                return Some(format!("firefox:{}", gecko.display()));
            }
            return Some(format!("firefox:{}", prof));
        }
        let detected = list_detected_browsers();
        if let Some(zen) = detected.iter().find(|b| b.id == "zen" && b.available) {
            let p = Path::new(&zen.profile_dir);
            if let Some(gecko) = find_gecko_profile_dir(p) {
                return Some(format!("firefox:{}", gecko.display()));
            }
            return Some(format!("firefox:{}", zen.profile_dir));
        }
        return Some("firefox".to_string());
    }
    if lower == "helium" {
        if let Some(prof) = profile_dir {
            return Some(format!("chrome:{}", prof));
        }
        let detected = list_detected_browsers();
        if let Some(helium) = detected.iter().find(|b| b.id == "helium" && b.available) {
            return Some(format!("chrome:{}", helium.profile_dir));
        }
        return Some("chrome".to_string());
    }
    if lower == "firefox" {
        if let Some(prof) = profile_dir {
            let p = Path::new(prof);
            if let Some(gecko) = find_gecko_profile_dir(p) {
                return Some(format!("firefox:{}", gecko.display()));
            }
            return Some(format!("firefox:{}", prof));
        }
        return Some("firefox".to_string());
    }
    if lower == "chromium" {
        if let Some(prof) = profile_dir {
            return Some(format!("chromium:{}", prof));
        }
        return Some("chromium".to_string());
    }
    if lower == "coccoc" {
        // Coc Coc has no gallery-dl/yt-dlp browser key, but it stores
        // Chrome-format cookies, so read them through the chrome extractor.
        if let Some(prof) = profile_dir {
            return Some(format!("chrome:{}", prof));
        }
        return Some("chrome".to_string());
    }
    if ["edge", "brave", "vivaldi", "opera"].contains(&lower.as_str()) {
        if let Some(prof) = profile_dir {
            return Some(format!("{}:{}", lower, prof));
        }
        return Some(lower);
    }
    Some(lower)
}

pub fn get_platform_cookie_domains(platform: &str) -> Vec<&'static str> {
    match platform.to_lowercase().as_str() {
        "instagram" => vec!["%instagram.com%"],
        "tiktok" => vec!["%tiktok.com%"],
        "facebook" => vec!["%facebook.com%", "%fb.com%"],
        "x" | "twitter" => vec!["%x.com%", "%twitter.com%"],
        _ => vec![],
    }
}

/// Existing Chrome-format cookie databases under a browser "User Data" dir:
/// `Default` plus every `Profile *` dir, each checking the modern
/// `Network/Cookies` location first and the legacy `Cookies` location.
/// Returns `(database_path, profile_dir)` pairs.
pub fn chrome_profile_cookie_db_paths(user_data_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut profiles: Vec<PathBuf> = vec![user_data_dir.join("Default")];
    if let Ok(entries) = fs::read_dir(user_data_dir) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("Profile ")
            {
                profiles.push(entry.path());
            }
        }
    }
    let mut dbs = Vec::new();
    for profile in profiles {
        for candidate in [
            profile.join("Network").join("Cookies"),
            profile.join("Cookies"),
        ] {
            if candidate.is_file() {
                dbs.push((candidate, profile.clone()));
            }
        }
    }
    dbs
}

/// Best-effort check: does this Chrome-format cookie database hold at least
/// one cookie for any of the given LIKE-pattern domains (host_key is stored
/// in plaintext, so the encrypted values never need decrypting here)?
/// The database is copied to a temp dir first so a running browser's lock
/// (and its WAL file) cannot interfere. Any failure means "no".
pub fn chrome_db_has_domain_cookie(db_path: &Path, domains: &[&str]) -> bool {
    if domains.is_empty() || !db_path.is_file() {
        return false;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let temp_dir = std::env::temp_dir().join(format!(
        "gumballz_cookie_check_{}_{:x}",
        stamp,
        rand::random::<u32>()
    ));
    if fs::create_dir_all(&temp_dir).is_err() {
        return false;
    }
    let temp_db = temp_dir.join("Cookies");
    if fs::copy(db_path, &temp_db).is_err() {
        let _ = fs::remove_dir_all(&temp_dir);
        return false;
    }
    // Copy the WAL too so recently written cookies are visible.
    let wal_src = PathBuf::from(format!("{}-wal", db_path.display()));
    if wal_src.is_file() {
        let wal_dst = PathBuf::from(format!("{}-wal", temp_db.display()));
        let _ = fs::copy(&wal_src, &wal_dst);
    }
    let found = (|| {
        let conn = Connection::open(&temp_db).ok()?;
        for domain in domains {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM cookies WHERE host_key LIKE ?1",
                    [*domain],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if count > 0 {
                return Some(true);
            }
        }
        Some(false)
    })()
    .unwrap_or(false);
    let _ = fs::remove_dir_all(&temp_dir);
    found
}

/// Find the browser that actually holds a cookie for this site. The target
/// (usually the system default browser) is tried first; then every other
/// detected Chromium-based browser that is installed. Returns the browser
/// id plus its "User Data" dir. Returns None when no installed browser has
/// a cookie for the site, in which case the caller keeps its previous
/// behaviour of pointing at the default browser.
/// Find the first browser (target first, then the others) whose cookie
/// database holds a cookie for one of `domains`. Returns
/// `(browser_id, user_data_dir, profile_dir)` — the profile dir is the
/// folder whose cookie database matched (e.g. `.../User Data/Profile 5`),
/// so callers can copy exactly that profile.
pub fn find_browser_with_site_cookies(
    target_id: &str,
    browsers: &[DetectedBrowser],
    domains: &[&str],
) -> Option<(String, String, PathBuf)> {
    let mut ordered: Vec<&DetectedBrowser> = Vec::new();
    if let Some(target) = browsers.iter().find(|b| b.id == target_id) {
        ordered.push(target);
    }
    for b in browsers {
        if b.id != target_id
            && b.engine != "firefox"
            && b.engine != "safari"
            && b.available
            && !ordered.iter().any(|o| o.id == b.id)
        {
            ordered.push(b);
        }
    }
    for b in ordered {
        let dir = Path::new(&b.profile_dir);
        for (db, profile_dir) in chrome_profile_cookie_db_paths(dir) {
            if chrome_db_has_domain_cookie(&db, domains) {
                return Some((b.id.clone(), b.profile_dir.clone(), profile_dir));
            }
        }
    }
    None
}

pub fn resolve_active_cookies(
    platform: &str,
    manual_path: Option<&str>,
    browser_selection: Option<&str>,
    app_data_dir: &Path,
) -> ResolvedCookies {
    let temp_base = app_data_dir.join("temp");

    if let Some(candidate) = find_candidate_cookie(platform, manual_path, app_data_dir) {
        if let Ok(sb) = SafeCookieSandbox::create(&candidate, &temp_base) {
            return ResolvedCookies {
                sandbox: Some(sb),
                browser_arg: None,
                shadow_profile: None,
            };
        }
    }

    // Extension-captured cookies (Cookie Manager store) beat the
    // browser profile: the extension reads already-decrypted cookie
    // values through `chrome.cookies`, while gallery-dl cannot decrypt
    // the browser's own database on machines with app-bound encryption
    // (Chromium 127+ reports `[cookies][warning] Failed to decrypt
    // cookie (DPAPI)` and the download then runs unauthenticated).
    if let Some(candidate) = find_cookie_manager_account(platform, app_data_dir) {
        if let Ok(sb) = SafeCookieSandbox::create(&candidate, &temp_base) {
            return ResolvedCookies {
                sandbox: Some(sb),
                browser_arg: None,
                shadow_profile: None,
            };
        }
    }

    let b_choice = browser_selection.unwrap_or("auto").trim();
    if b_choice.is_empty() || b_choice.eq_ignore_ascii_case("none") {
        return ResolvedCookies::empty();
    }

    let target_browser = if b_choice.eq_ignore_ascii_case("auto") {
        detect_system_default_browser().unwrap_or_else(|| "chrome".to_string())
    } else {
        b_choice.to_ascii_lowercase()
    };

    let browsers = list_detected_browsers();
    let matched_browser = browsers.iter().find(|b| b.id == target_browser);

    if target_browser == "firefox" || target_browser == "zen" || target_browser == "librewolf" {
        if let Some(b) = matched_browser {
            if b.available {
                let prof_path = Path::new(&b.profile_dir);
                if let Some(gecko_dir) = find_gecko_profile_dir(prof_path) {
                    let domains = get_platform_cookie_domains(platform);
                    if let Some(netscape_data) =
                        extract_gecko_cookies_to_netscape(&gecko_dir, &domains)
                    {
                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis())
                            .unwrap_or(0);
                        let rand_suffix: u32 = rand::random();
                        let temp_file = temp_base.join(format!(
                            "browser_cookies_{}_{:x}.txt",
                            timestamp, rand_suffix
                        ));
                        let _ = fs::create_dir_all(&temp_base);
                        if fs::write(&temp_file, netscape_data).is_ok() {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = fs::set_permissions(
                                    &temp_file,
                                    fs::Permissions::from_mode(0o600),
                                );
                            }
                            return ResolvedCookies {
                                sandbox: Some(SafeCookieSandbox {
                                    temp_path: temp_file,
                                }),
                                browser_arg: None,
                                shadow_profile: None,
                            };
                        }
                    }
                }
            }
        }
    }

    // Chromium-based browsers hand gallery-dl/yt-dlp a profile dir to read
    // themselves, so a profile with no cookie database (or no cookie for
    // this site) fails the download. Prefer the browser that actually has
    // a cookie for this site; only if none does, keep the previous
    // behaviour of pointing at the default browser. This also covers a
    // Firefox default whose login lives in a Chromium browser, since the
    // Gecko extraction above already failed by the time we get here (a
    // Firefox profile dir simply holds no Chrome-format cookie database,
    // so the scan skips it).
    //
    // When a matching profile is found, hand gallery-dl/yt-dlp a shadow
    // copy of it instead of the live dir: a running Chromium browser
    // locks its live `Cookies` file (Windows `[Errno 13] Permission
    // denied`), while the copy reads fine. If the shadow cannot be built,
    // fall back to the live dir (previous behaviour).
    let domains = get_platform_cookie_domains(platform);
    let (browser_arg, shadow_profile) =
        match find_browser_with_site_cookies(&target_browser, &browsers, &domains) {
            Some((id, dir, profile_dir)) => {
                match build_chrome_shadow_profile(Path::new(&dir), &profile_dir, &temp_base) {
                    Some(shadow_dir) => (
                        resolve_browser_cookie_arg(
                            &id,
                            Some(&shadow_dir.to_string_lossy()),
                        ),
                        Some(ShadowChromeProfile { dir: shadow_dir }),
                    ),
                    None => (resolve_browser_cookie_arg(&id, Some(&dir)), None),
                }
            }
            None => (
                resolve_browser_cookie_arg(
                    &target_browser,
                    matched_browser.map(|b| b.profile_dir.as_str()),
                ),
                None,
            ),
        };
    ResolvedCookies {
        sandbox: None,
        browser_arg,
        shadow_profile,
    }
}

/// Root domains whose Cookie Manager bucket may hold a captured login
/// for each social platform. Mirrors `get_platform_cookie_domains`
/// (same domains, without the SQL LIKE wildcards).
fn cookie_manager_domains(platform: &str) -> Vec<&'static str> {
    match platform.to_lowercase().as_str() {
        "instagram" => vec!["instagram.com"],
        "tiktok" => vec!["tiktok.com"],
        "facebook" => vec!["facebook.com", "fb.com"],
        "x" | "twitter" => vec!["x.com", "twitter.com"],
        _ => vec![],
    }
}

/// Extension-captured login cookies for this platform, if the Cookie
/// Manager store holds a usable account for it.
///
/// The social downloader runs with `app_data_dir` =
/// `<app_data>/social-media`, while the Cookie Manager store lives at
/// `<app_data>/cookies/<domain>/_default.txt` — the extension captures
/// decrypted cookies and the bridge writes them there through
/// `ingest_batch`. The social flow has no selected account (no slug
/// reaches it), so the domain's default account is used. Returns the
/// first candidate file that exists and parses as Netscape cookies
/// for the domain; `None` when the store has nothing usable, so the
/// caller falls through to the browser profile exactly as before.
fn find_cookie_manager_account(platform: &str, app_data_dir: &Path) -> Option<PathBuf> {
    let cookies_root = app_data_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())?
        .join("cookies");
    for domain in cookie_manager_domains(platform) {
        if let Some(path) =
            crate::cookies::account_path_for_consumer_in(&cookies_root, domain, None)
        {
            if cookie_file_has_usable_cookies(&path, domain) {
                return Some(path);
            }
        }
    }
    None
}

fn cookie_file_has_usable_cookies(path: &Path, domain: &str) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return false,
    };
    if content.len() <= 10 {
        return false;
    }
    !super::http::parse_netscape_cookies(&content, Some(domain)).is_empty()
}

pub fn find_candidate_cookie(
    platform: &str,
    custom_path: Option<&str>,
    app_data_dir: &Path,
) -> Option<PathBuf> {
    if let Some(custom) = custom_path {
        let p = PathBuf::from(custom.trim());
        if p.exists() && fs::metadata(&p).map(|m| m.len() > 10).unwrap_or(false) {
            return Some(p);
        }
    }

    let filenames = match platform.to_lowercase().as_str() {
        "instagram" => vec!["instagram.com_cookies.txt"],
        "tiktok" => vec!["tiktok.com_cookies.txt"],
        "facebook" => vec!["facebook.com_cookies.txt", "fb.com_cookies.txt"],
        "x" | "twitter" => vec!["x.com_cookies.txt", "twitter.com_cookies.txt"],
        _ => vec![],
    };

    let mut candidate_dirs: Vec<PathBuf> = vec![app_data_dir.join("cookies")];

    if let Some(user_home) = dirs::home_dir() {
        candidate_dirs.push(user_home.join(".social-media").join("cookies"));
        candidate_dirs.push(user_home.join("cookies"));
    }

    for dir in candidate_dirs {
        for fname in &filenames {
            let full = dir.join(fname);
            if full.exists() && fs::metadata(&full).map(|m| m.len() > 10).unwrap_or(false) {
                return Some(full);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_safe_cookie_sandbox_lifecycle() {
        let temp_dir = std::env::temp_dir().join("social_media_test_cookie");
        let _ = fs::create_dir_all(&temp_dir);

        let master = temp_dir.join("master_cookies.txt");
        let mut f = fs::File::create(&master).unwrap();
        writeln!(f, "# Netscape HTTP Cookie File").unwrap();
        drop(f);

        let sandbox_dir = temp_dir.join("sandbox");
        let temp_file_path;
        {
            let sandbox = SafeCookieSandbox::create(&master, &sandbox_dir).expect("Create sandbox");
            temp_file_path = sandbox.path().to_path_buf();
            assert!(temp_file_path.exists());
            assert_ne!(temp_file_path, master);
        }
        assert!(!temp_file_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_parse_windows_default_browser_prog_id() {
        let chrome_raw = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\https\\UserChoice\r\n    ProgId    REG_SZ    ChromeHTML\r\n";
        assert_eq!(
            parse_windows_default_browser_prog_id(chrome_raw),
            Some("ChromeHTML".to_string())
        );

        let helium_raw = "    ProgId    REG_SZ    HeliumHTM\r\n";
        assert_eq!(
            parse_windows_default_browser_prog_id(helium_raw),
            Some("HeliumHTM".to_string())
        );

        let firefox_raw = "    ProgId    REG_SZ    FirefoxURL-308046B0AF4A39CB\r\n";
        assert_eq!(
            parse_windows_default_browser_prog_id(firefox_raw),
            Some("FirefoxURL-308046B0AF4A39CB".to_string())
        );

        let injection_raw = "    ProgId    REG_SZ    ChromeHTML & calc.exe\r\n";
        assert_eq!(parse_windows_default_browser_prog_id(injection_raw), None);

        assert_eq!(parse_windows_default_browser_prog_id(""), None);
    }

    #[test]
    fn test_map_prog_id_to_browser_id() {
        let chrome = map_prog_id_to_browser_id("ChromeHTML", None);
        assert_eq!(chrome.id, "chrome");
        assert_eq!(chrome.name, "Google Chrome");

        let edge = map_prog_id_to_browser_id("MSEdgeHTM", None);
        assert_eq!(edge.id, "edge");

        let edge_uwp = map_prog_id_to_browser_id("AppXq0fevzme2pys62n3e0fbqsw5cj08ege9", None);
        assert_eq!(edge_uwp.id, "edge");

        let brave = map_prog_id_to_browser_id("BraveHTML", None);
        assert_eq!(brave.id, "brave");

        let firefox = map_prog_id_to_browser_id("FirefoxURL-308046B0AF4A39CB", None);
        assert_eq!(firefox.id, "firefox");

        let helium =
            map_prog_id_to_browser_id("CustomProgId", Some("\"C:\\imput\\Helium\\chrome.exe\" %1"));
        assert_eq!(helium.id, "helium");

        let zen = map_prog_id_to_browser_id("ZenURL", Some("\"C:\\Zen\\zen.exe\" %1"));
        assert_eq!(zen.id, "zen");

        let chromium = map_prog_id_to_browser_id("ChromiumHTM", None);
        assert_eq!(chromium.id, "chromium");

        let coccoc = map_prog_id_to_browser_id("CocCocHTM", None);
        assert_eq!(coccoc.id, "coccoc");

        // An unrecognised Chromium-fork ProgId must never leak through raw:
        // gallery-dl/yt-dlp reject unknown browser keys.
        let unknown = map_prog_id_to_browser_id("SomeForkHTM", None);
        assert_eq!(unknown.id, "chromium");
    }

    #[test]
    fn test_parse_mac_and_linux_default_browser() {
        let mac_output = r#"
        (
            {
                LSHandlerRoleAll = "com.google.chrome";
                LSHandlerURLScheme = https;
            }
        )
        "#;
        assert_eq!(
            parse_mac_default_browser_bundle_id(mac_output),
            Some("com.google.chrome".to_string())
        );
        let mac_browser = map_bundle_id_to_browser_id("com.google.chrome");
        assert_eq!(mac_browser.id, "chrome");

        let linux_output = "google-chrome.desktop\n";
        assert_eq!(
            parse_linux_default_browser_desktop(linux_output),
            Some("google-chrome.desktop".to_string())
        );
        let linux_browser = map_linux_desktop_to_browser_id("google-chrome.desktop");
        assert_eq!(linux_browser.id, "chrome");

        let zen_linux = map_linux_desktop_to_browser_id("app.zen_browser.zen.desktop");
        assert_eq!(zen_linux.id, "zen");
        assert_eq!(zen_linux.engine, "firefox");
    }

    #[test]
    fn test_resolve_browser_cookie_arg() {
        assert_eq!(
            resolve_browser_cookie_arg("chrome", None),
            Some("chrome".to_string())
        );
        assert_eq!(
            resolve_browser_cookie_arg("firefox", None),
            Some("firefox".to_string())
        );
        assert_eq!(resolve_browser_cookie_arg("none", None), None);
        assert_eq!(resolve_browser_cookie_arg("", None), None);

        let zen_arg = resolve_browser_cookie_arg("zen", Some("/home/user/.zen/profile.default"));
        assert!(zen_arg.unwrap().starts_with("firefox:"));

        let chromium_arg = resolve_browser_cookie_arg(
            "chromium",
            Some("C:\\Users\\u\\AppData\\Local\\Chromium\\User Data"),
        );
        assert_eq!(
            chromium_arg,
            Some("chromium:C:\\Users\\u\\AppData\\Local\\Chromium\\User Data".to_string())
        );
        assert_eq!(
            resolve_browser_cookie_arg("chromium", None),
            Some("chromium".to_string())
        );

        let coccoc_arg = resolve_browser_cookie_arg(
            "coccoc",
            Some("C:\\Users\\u\\AppData\\Local\\CocCoc\\Browser\\User Data"),
        );
        assert!(coccoc_arg.unwrap().starts_with("chrome:"));
    }

    #[test]
    fn test_gecko_profile_detection_and_sqlite_cookie_extraction() {
        let temp_dir =
            std::env::temp_dir().join(format!("social_media_gecko_test_{}", rand::random::<u32>()));
        let _ = fs::create_dir_all(&temp_dir);

        let profile_dir = temp_dir.join("profile.default-release");
        let _ = fs::create_dir_all(&profile_dir);

        let db_path = profile_dir.join("cookies.sqlite");
        {
            let conn = Connection::open(&db_path).expect("Create test sqlite db");
            conn.execute(
                "CREATE TABLE moz_cookies (
                    id INTEGER PRIMARY KEY,
                    host TEXT,
                    path TEXT,
                    isSecure INTEGER,
                    expiry INTEGER,
                    name TEXT,
                    value TEXT,
                    isHttpOnly INTEGER
                )",
                [],
            )
            .unwrap();

            conn.execute(
                "INSERT INTO moz_cookies (host, path, isSecure, expiry, name, value, isHttpOnly)
                 VALUES ('.instagram.com', '/', 1, 1750000000, 'sessionid', 'mock_ig_session_123', 1)",
                [],
            ).unwrap();

            conn.execute(
                "INSERT INTO moz_cookies (host, path, isSecure, expiry, name, value, isHttpOnly)
                 VALUES ('.facebook.com', '/', 1, 1750000000, 'c_user', '10002345', 0)",
                [],
            )
            .unwrap();
        }

        let resolved_profile = find_gecko_profile_dir(&temp_dir);
        assert_eq!(resolved_profile, Some(profile_dir.clone()));

        let ig_cookies = extract_gecko_cookies_to_netscape(&profile_dir, &["%instagram.com%"]);
        assert!(ig_cookies.is_some());
        let content = ig_cookies.unwrap();
        assert!(content.contains("# Netscape HTTP Cookie File"));
        assert!(content.contains("#HttpOnly_.instagram.com"));
        assert!(content.contains("sessionid\tmock_ig_session_123"));
        assert!(!content.contains("facebook.com"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_chrome_profile_cookie_db_paths() {
        let temp_dir = std::env::temp_dir().join(format!(
            "social_media_chrome_paths_test_{}",
            rand::random::<u32>()
        ));
        let _ = fs::create_dir_all(&temp_dir);

        let default_db = temp_dir.join("Default").join("Network").join("Cookies");
        let _ = fs::create_dir_all(default_db.parent().unwrap());
        let _ = fs::write(&default_db, b"");

        let profile1_db = temp_dir.join("Profile 1").join("Network").join("Cookies");
        let _ = fs::create_dir_all(profile1_db.parent().unwrap());
        let _ = fs::write(&profile1_db, b"");

        // Legacy location (no Network subdir).
        let profile2_db = temp_dir.join("Profile 2").join("Cookies");
        let _ = fs::create_dir_all(profile2_db.parent().unwrap());
        let _ = fs::write(&profile2_db, b"");

        // Guest Profile is not a real profile and must be ignored.
        let guest_db = temp_dir.join("Guest Profile").join("Network").join("Cookies");
        let _ = fs::create_dir_all(guest_db.parent().unwrap());
        let _ = fs::write(&guest_db, b"");

        let mut found = chrome_profile_cookie_db_paths(&temp_dir);
        found.sort();
        let mut expected = vec![
            (default_db.clone(), temp_dir.join("Default")),
            (profile1_db.clone(), temp_dir.join("Profile 1")),
            (profile2_db.clone(), temp_dir.join("Profile 2")),
        ];
        expected.sort();
        assert_eq!(found, expected);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_build_chrome_shadow_profile() {
        let temp_dir = std::env::temp_dir().join(format!(
            "social_media_chrome_shadow_test_{}",
            rand::random::<u32>()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        let temp_base = temp_dir.join("temp");

        // A Coc Coc-style User Data dir: Facebook login in Profile 5, and
        // a "Local State" key file at the User Data root.
        let user_data = temp_dir.join("CocCoc").join("User Data");
        let profile5 = user_data.join("Profile 5");
        let profile5_db = profile5.join("Network").join("Cookies");
        make_chrome_cookie_db(&profile5_db, &[".facebook.com"]);
        let _ = fs::write(user_data.join("Local State"), b"{\"os_crypt\":{}}");

        let shadow = build_chrome_shadow_profile(&user_data, &profile5, &temp_base)
            .expect("shadow should build");
        let shadow_db = shadow.join("Profile 5").join("Network").join("Cookies");
        assert!(shadow_db.is_file());
        assert!(chrome_db_has_domain_cookie(
            &shadow_db,
            &["%facebook.com%"]
        ));
        assert!(shadow.join("Local State").is_file());
        assert_ne!(shadow_db, profile5_db);

        // Dropping the shadow removes the whole temp dir.
        let shadow_path = shadow.clone();
        drop(ShadowChromeProfile { dir: shadow });
        assert!(!shadow_path.exists());

        // No cookie database in the profile -> no shadow.
        let empty_profile = user_data.join("Profile 9");
        let _ = fs::create_dir_all(empty_profile.join("Network"));
        assert!(build_chrome_shadow_profile(&user_data, &empty_profile, &temp_base).is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn make_chrome_cookie_db(db_path: &Path, hosts: &[&str]) {
        let _ = fs::create_dir_all(db_path.parent().unwrap());
        let conn = Connection::open(db_path).expect("Create test sqlite db");
        conn.execute(
            "CREATE TABLE cookies (
                host_key TEXT,
                name TEXT,
                value TEXT,
                encrypted_value BLOB
            )",
            [],
        )
        .unwrap();
        for host in hosts {
            conn.execute(
                "INSERT INTO cookies (host_key, name, value, encrypted_value)
                 VALUES (?1, 'c_user', '10002345', X'00')",
                [*host],
            )
            .unwrap();
        }
    }

    #[test]
    fn test_chrome_db_has_domain_cookie() {
        let temp_dir = std::env::temp_dir().join(format!(
            "social_media_chrome_db_test_{}",
            rand::random::<u32>()
        ));
        let _ = fs::create_dir_all(&temp_dir);

        let fb_db = temp_dir.join("fb_cookies");
        make_chrome_cookie_db(&fb_db, &[".facebook.com", ".instagram.com"]);
        assert!(chrome_db_has_domain_cookie(&fb_db, &["%facebook.com%"]));
        assert!(chrome_db_has_domain_cookie(
            &fb_db,
            &["%facebook.com%", "%fb.com%"]
        ));
        assert!(!chrome_db_has_domain_cookie(&fb_db, &["%tiktok.com%"]));

        let ig_only_db = temp_dir.join("ig_cookies");
        make_chrome_cookie_db(&ig_only_db, &[".instagram.com"]);
        assert!(!chrome_db_has_domain_cookie(
            &ig_only_db,
            &["%facebook.com%"]
        ));

        // Missing file and an empty table both mean "no".
        assert!(!chrome_db_has_domain_cookie(
            &temp_dir.join("does_not_exist"),
            &["%facebook.com%"]
        ));
        let empty_db = temp_dir.join("empty_cookies");
        {
            let conn = Connection::open(&empty_db).expect("Create test sqlite db");
            conn.execute("CREATE TABLE cookies (host_key TEXT)", [])
                .unwrap();
        }
        assert!(!chrome_db_has_domain_cookie(&empty_db, &["%facebook.com%"]));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_find_browser_with_site_cookies() {
        let temp_dir = std::env::temp_dir().join(format!(
            "social_media_find_browser_test_{}",
            rand::random::<u32>()
        ));
        let _ = fs::create_dir_all(&temp_dir);

        // Chromium: installed but no cookie database at all.
        let chromium_dir = temp_dir.join("Chromium").join("User Data");
        let _ = fs::create_dir_all(chromium_dir.join("Default").join("Network"));

        // Coc Coc: has a Facebook cookie in its default profile.
        let coccoc_dir = temp_dir.join("CocCoc").join("User Data");
        let coccoc_db = coccoc_dir.join("Default").join("Network").join("Cookies");
        make_chrome_cookie_db(&coccoc_db, &[".facebook.com"]);

        let browsers = vec![
            DetectedBrowser {
                id: "chromium".to_string(),
                name: "Chromium".to_string(),
                engine: "chromium".to_string(),
                profile_dir: chromium_dir.to_string_lossy().to_string(),
                available: true,
                is_default: true,
            },
            DetectedBrowser {
                id: "coccoc".to_string(),
                name: "Cốc Cốc".to_string(),
                engine: "chrome".to_string(),
                profile_dir: coccoc_dir.to_string_lossy().to_string(),
                available: true,
                is_default: false,
            },
        ];

        // The default browser has no cookie, so the scan falls through to
        // Coc Coc, which does.
        let found = find_browser_with_site_cookies(
            "chromium",
            &browsers,
            &["%facebook.com%", "%fb.com%"],
        );
        assert_eq!(
            found,
            Some((
                "coccoc".to_string(),
                coccoc_dir.to_string_lossy().to_string(),
                coccoc_dir.join("Default")
            ))
        );

        // No browser has a TikTok cookie.
        assert_eq!(
            find_browser_with_site_cookies("chromium", &browsers, &["%tiktok.com%"]),
            None
        );

        // A Firefox default whose login lives in Coc Coc falls through to
        // Coc Coc the same way, once the Gecko extraction has failed.
        let firefox_dir = temp_dir.join("Firefox").join("Profiles").join("abc.default-release");
        let _ = fs::create_dir_all(&firefox_dir);
        let browsers_ff = vec![
            DetectedBrowser {
                id: "firefox".to_string(),
                name: "Firefox".to_string(),
                engine: "firefox".to_string(),
                profile_dir: firefox_dir.to_string_lossy().to_string(),
                available: true,
                is_default: true,
            },
            DetectedBrowser {
                id: "coccoc".to_string(),
                name: "Cốc Cốc".to_string(),
                engine: "chrome".to_string(),
                profile_dir: coccoc_dir.to_string_lossy().to_string(),
                available: true,
                is_default: false,
            },
        ];
        assert_eq!(
            find_browser_with_site_cookies("firefox", &browsers_ff, &["%facebook.com%"]),
            Some((
                "coccoc".to_string(),
                coccoc_dir.to_string_lossy().to_string(),
                coccoc_dir.join("Default")
            ))
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // --- Cookie Manager store (extension-captured cookies) ---

    const IG_NETSCAPE: &str = "# Netscape HTTP Cookie File\n.instagram.com\tTRUE\t/\tTRUE\t1770000000\tsessionid\tfake_session_value\n";
    const FB_NETSCAPE: &str = "# Netscape HTTP Cookie File\n.fb.com\tTRUE\t/\tTRUE\t1770000000\tc_user\tfake_user_value\n";

    /// Temp layout mirroring production: `app_data_dir` is
    /// `<base>/social-media` (what the dispatcher passes) and the
    /// Cookie Manager store lives at `<base>/cookies/`.
    fn cookie_manager_test_layout(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "social_media_cookie_mgr_{}_{}",
            name,
            rand::random::<u32>()
        ));
        let app_data_dir = base.join("social-media");
        let _ = fs::create_dir_all(&app_data_dir);
        (base, app_data_dir)
    }

    fn write_store_account(base: &Path, domain: &str, content: &str) -> PathBuf {
        let path = base.join("cookies").join(domain).join("_default.txt");
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_cookie_manager_account_used_over_browser_profile() {
        let (base, app_data_dir) = cookie_manager_test_layout("store_first");
        let store_path = write_store_account(&base, "instagram.com", IG_NETSCAPE);

        let resolved = resolve_active_cookies("Instagram", None, Some("auto"), &app_data_dir);
        assert!(resolved.browser_arg().is_none());
        let cookie_path = resolved
            .cookie_file_path()
            .expect("store cookies expected")
            .to_path_buf();
        assert_ne!(cookie_path, store_path);
        let sandboxed = fs::read_to_string(&cookie_path).unwrap();
        assert!(sandboxed.contains("sessionid\tfake_session_value"));

        // The sandbox is a throwaway copy, removed when dropped.
        drop(resolved);
        assert!(!cookie_path.exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_manual_cookie_file_still_first() {
        let (base, app_data_dir) = cookie_manager_test_layout("manual_first");
        let _store_path = write_store_account(&base, "instagram.com", IG_NETSCAPE);

        let manual = base.join("manual_cookies.txt");
        fs::write(
            &manual,
            "# Netscape HTTP Cookie File\n.instagram.com\tTRUE\t/\tTRUE\t1770000000\tsessionid\tmanual_value\n",
        )
        .unwrap();

        let resolved = resolve_active_cookies(
            "Instagram",
            Some(manual.to_str().unwrap()),
            Some("auto"),
            &app_data_dir,
        );
        let cookie_path = resolved
            .cookie_file_path()
            .expect("manual cookies expected")
            .to_path_buf();
        let sandboxed = fs::read_to_string(&cookie_path).unwrap();
        assert!(sandboxed.contains("sessionid\tmanual_value"));
        assert!(!sandboxed.contains("fake_session_value"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_missing_or_empty_store_falls_through() {
        let (base, app_data_dir) = cookie_manager_test_layout("store_missing");

        // No store file at all: with browser selection "none" the result
        // must be empty (no false positive, no browser read attempted).
        let resolved = resolve_active_cookies("Instagram", None, Some("none"), &app_data_dir);
        assert!(!resolved.has_cookies());
        assert!(resolved.cookie_file_path().is_none());
        assert!(resolved.browser_arg().is_none());

        // A store file with no parseable cookies must fall through too.
        let _header_only =
            write_store_account(&base, "instagram.com", "# Netscape HTTP Cookie File\n");
        let resolved = resolve_active_cookies("Instagram", None, Some("none"), &app_data_dir);
        assert!(!resolved.has_cookies());

        // Garbage content (not Netscape) must fall through as well.
        let _garbage =
            write_store_account(&base, "instagram.com", "this is not a cookie file at all\n");
        let resolved = resolve_active_cookies("Instagram", None, Some("none"), &app_data_dir);
        assert!(!resolved.has_cookies());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_disabled_domain_store_is_ignored() {
        let (base, app_data_dir) = cookie_manager_test_layout("store_disabled");
        let _store_path = write_store_account(&base, "instagram.com", IG_NETSCAPE);
        let registry = base.join("cookies").join("_meta.json");
        fs::write(
            &registry,
            "{\"buckets\": {}, \"disabled_domains\": [\"instagram.com\"]}",
        )
        .unwrap();

        let resolved = resolve_active_cookies("Instagram", None, Some("none"), &app_data_dir);
        assert!(!resolved.has_cookies());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_find_cookie_manager_account_domain_fallbacks() {
        let (base, app_data_dir) = cookie_manager_test_layout("domain_fallback");

        // Only fb.com exists: Facebook must still find it after
        // facebook.com misses.
        let fb_path = write_store_account(&base, "fb.com", FB_NETSCAPE);
        assert_eq!(
            find_cookie_manager_account("Facebook", &app_data_dir),
            Some(fb_path)
        );

        // Unknown platform: no store domains at all.
        assert_eq!(find_cookie_manager_account("youtube", &app_data_dir), None);

        let _ = fs::remove_dir_all(&base);
    }
}
