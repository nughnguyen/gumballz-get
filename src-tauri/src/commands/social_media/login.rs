//! In-app login for Facebook and Instagram (social-media downloads).
//!
//! Users who do not run the browser extension — and whose browser cookies
//! cannot be read directly (Chromium 127+ app-bound encryption / DPAPI) —
//! sign in inside a Tauri webview window instead. The webview shares the
//! app's default WebView2 profile, so its native cookie store (including
//! HttpOnly session cookies, which `document.cookie` can never see) is read
//! through the existing `open_auth_webview` helper and saved into the
//! Cookie Manager store (`<app_data>/cookies/<domain>/_default.txt`,
//! Netscape format) via `ingest_batch`. From there the existing
//! `resolve_active_cookies` path picks the login up with no changes to the
//! download flow.
//!
//! Fully automatic from the user's side: click "Sign in" -> the login
//! window opens -> the user signs in normally (or is already signed in) ->
//! the session cookie is detected and saved with no further click -> the
//! window closes and the UI shows the logged-in state. No password is ever
//! stored or sent anywhere: only the cookies the site itself set in the
//! webview are written to the local store, and only for the platform's own
//! root domains (an SSO detour such as accounts.google.com leaves no
//! cookies behind).

#![allow(clippy::all)]
#![allow(warnings)]

use std::collections::HashSet;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::AppHandle;

use crate::commands::auth_webview::{open_auth_webview, AuthCookie, AuthWebviewRequest};
use crate::cookies::platform::root_domain_of;
use crate::cookies::{account_path_for_consumer, ingest_batch, load_registry, IngestSource};
use crate::extension_storage::ExtensionCookie;

/// Native cookie extraction carries no expiry, and session cookies are
/// long-lived in practice, so saved logins are written with a 90-day
/// horizon — yt-dlp drops cookies past their expiry, and the Cookie
/// Manager health check already nudges a re-login after 28 days.
const SESSION_COOKIE_TTL_SECS: i64 = 90 * 86400;

const DEFAULT_SLUG: &str = "_default";

struct PlatformConfig {
    platform: &'static str,
    display_name: &'static str,
    login_url: &'static str,
    login_host: &'static str,
    home_url: &'static str,
    wait_for_cookie: &'static str,
    cookie_domains: Vec<String>,
    store_roots: Vec<&'static str>,
    session_cookies: Vec<&'static str>,
}

fn config_for(platform: &str) -> Result<PlatformConfig, String> {
    match platform.to_lowercase().as_str() {
        "facebook" => Ok(PlatformConfig {
            platform: "facebook",
            display_name: "Facebook",
            login_url: "https://www.facebook.com/login/",
            login_host: "www.facebook.com",
            home_url: "https://www.facebook.com/",
            wait_for_cookie: "c_user|xs",
            cookie_domains: vec!["facebook.com".to_string(), "www.facebook.com".to_string()],
            store_roots: vec!["facebook.com", "fb.com"],
            session_cookies: vec!["c_user", "xs"],
        }),
        "instagram" => Ok(PlatformConfig {
            platform: "instagram",
            display_name: "Instagram",
            login_url: "https://www.instagram.com/accounts/login/",
            login_host: "www.instagram.com",
            home_url: "https://www.instagram.com/",
            wait_for_cookie: "sessionid",
            cookie_domains: vec!["instagram.com".to_string(), "www.instagram.com".to_string()],
            store_roots: vec!["instagram.com"],
            session_cookies: vec!["sessionid"],
        }),
        _ => Err(format!("Unsupported login platform: {}", platform)),
    }
}

fn login_window_title(cfg: &PlatformConfig) -> String {
    // A Tauri webview window has no address bar, so the window title carries
    // the domain as the visible trust signal that this is the real site.
    format!("Sign in to {} · {}", cfg.display_name, cfg.login_host)
}

fn to_extension_cookie(c: &AuthCookie, expires: i64) -> ExtensionCookie {
    ExtensionCookie {
        domain: c.domain.clone(),
        http_only: c.http_only,
        path: if c.path.is_empty() {
            "/".to_string()
        } else {
            c.path.clone()
        },
        secure: c.secure,
        expires,
        name: c.name.clone(),
        value: c.value.clone(),
        // None: the Netscape writer derives host-only from the leading dot
        // in the domain (".facebook.com" -> shared, "www.facebook.com" ->
        // host-only), which is exactly what the native store reports.
        host_only: None,
        same_site: None,
    }
}

fn map_login_error(raw: &str) -> String {
    if raw.contains("closed before a session") {
        "settings.cookies.social_login_cancelled".to_string()
    } else if raw.contains("timed out") {
        "settings.cookies.social_login_timeout".to_string()
    } else {
        raw.to_string()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SocialLoginResult {
    pub platform: String,
    pub logged_in: bool,
    pub cookie_count: usize,
    pub final_url: String,
}

#[tauri::command]
pub async fn social_media_open_login_window(
    app: AppHandle,
    platform: String,
) -> Result<SocialLoginResult, String> {
    let cfg = config_for(&platform)?;

    let request = AuthWebviewRequest {
        url: cfg.login_url.to_string(),
        title: login_window_title(&cfg),
        cookie_domains: cfg.cookie_domains.clone(),
        success_url_contains: None,
        wait_for_cookie: Some(cfg.wait_for_cookie.to_string()),
        initialization_script: None,
        width: Some(1000.0),
        height: Some(720.0),
    };

    let result = open_auth_webview(app, request)
        .await
        .map_err(|e| map_login_error(&e))?;

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let expires = now_secs + SESSION_COOKIE_TTL_SECS;

    let cookies: Vec<ExtensionCookie> = result
        .cookies
        .iter()
        .filter(|c| {
            cfg.store_roots
                .iter()
                .any(|r| root_domain_of(&c.domain) == *r)
        })
        .map(|c| to_extension_cookie(c, expires))
        .collect();

    // `open_auth_webview` already waited for the session cookie, but
    // re-check after the domain filter: a same-named cookie from another
    // host must not fake a login.
    let names: HashSet<&str> = cookies.iter().map(|c| c.name.as_str()).collect();
    if !cfg
        .session_cookies
        .iter()
        .all(|n| names.contains(n))
    {
        return Err("settings.cookies.social_login_no_session".to_string());
    }

    let written = ingest_batch(
        &cookies,
        IngestSource {
            source_url: Some(cfg.home_url.to_string()),
            source_label: "Sign in in app".to_string(),
            alias_hint: Some(cfg.display_name.to_string()),
        },
    )
    .map_err(|e| e.to_string())?;
    let cookie_count: usize = written.iter().map(|(_, n)| n).sum();

    Ok(SocialLoginResult {
        platform: cfg.platform.to_string(),
        logged_in: true,
        cookie_count,
        final_url: result.final_url,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SocialLoginStatus {
    pub platform: String,
    pub logged_in: bool,
    pub cookie_count: usize,
    pub captured_at_ms: Option<i64>,
    pub alias: Option<String>,
}

#[tauri::command]
pub async fn social_media_login_status(platform: String) -> Result<SocialLoginStatus, String> {
    let cfg = config_for(&platform)?;
    let registry = load_registry();

    for root in &cfg.store_roots {
        let Some(path) = account_path_for_consumer(root, None) else {
            continue;
        };
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(parsed) = crate::cookies::parsers::parse_netscape(&content) else {
            continue;
        };
        let names: HashSet<&str> = parsed.iter().map(|c| c.name.as_str()).collect();
        if !cfg
            .session_cookies
            .iter()
            .all(|n| names.contains(n))
        {
            continue;
        }
        let account = registry
            .buckets
            .get(*root)
            .and_then(|b| b.accounts.iter().find(|a| a.slug == DEFAULT_SLUG));
        return Ok(SocialLoginStatus {
            platform: cfg.platform.to_string(),
            logged_in: true,
            cookie_count: parsed.len(),
            captured_at_ms: account.map(|a| a.captured_at_ms),
            alias: account.map(|a| a.alias.clone()),
        });
    }

    Ok(SocialLoginStatus {
        platform: cfg.platform.to_string(),
        logged_in: false,
        cookie_count: 0,
        captured_at_ms: None,
        alias: None,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SocialLogoutResult {
    pub platform: String,
    pub logged_in: bool,
}

#[tauri::command]
pub async fn social_media_logout_platform(platform: String) -> Result<SocialLogoutResult, String> {
    let cfg = config_for(&platform)?;
    // Same as the existing "clear cookies" action: the store file goes to
    // the 24h trash, so a mistaken logout is recoverable from there.
    // Note: the webview profile keeps its own session, so opening the login
    // window again signs straight back in — that is the re-login shortcut,
    // not a leak: downloads only ever read the store file.
    for root in &cfg.store_roots {
        crate::cookies::storage::move_to_trash(root, DEFAULT_SLUG).map_err(|e| e.to_string())?;
    }
    Ok(SocialLogoutResult {
        platform: cfg.platform.to_string(),
        logged_in: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_for_accepts_both_platforms_case_insensitively() {
        let fb = config_for("Facebook").expect("facebook");
        assert_eq!(fb.login_url, "https://www.facebook.com/login/");
        assert_eq!(fb.wait_for_cookie, "c_user|xs");
        assert_eq!(fb.store_roots, vec!["facebook.com", "fb.com"]);

        let ig = config_for("INSTAGRAM").expect("instagram");
        assert_eq!(
            ig.login_url,
            "https://www.instagram.com/accounts/login/"
        );
        assert_eq!(ig.wait_for_cookie, "sessionid");
        assert_eq!(ig.session_cookies, vec!["sessionid"]);
    }

    #[test]
    fn config_for_rejects_other_platforms() {
        assert!(config_for("tiktok").is_err());
        assert!(config_for("").is_err());
    }

    #[test]
    fn login_window_title_shows_the_domain() {
        let fb = config_for("facebook").expect("facebook");
        assert_eq!(
            login_window_title(&fb),
            "Sign in to Facebook · www.facebook.com"
        );
    }

    #[test]
    fn error_mapping_keeps_i18n_keys_for_known_cases() {
        assert_eq!(
            map_login_error("Login window was closed before a session was detected (no c_user/xs cookie)"),
            "settings.cookies.social_login_cancelled"
        );
        assert_eq!(
            map_login_error("Auth timed out"),
            "settings.cookies.social_login_timeout"
        );
        assert_eq!(
            map_login_error("Failed to create auth window: boom"),
            "Failed to create auth window: boom"
        );
    }

    #[test]
    fn extension_cookie_carries_the_90_day_horizon() {
        let auth = AuthCookie {
            name: "sessionid".to_string(),
            value: "abc".to_string(),
            domain: ".instagram.com".to_string(),
            path: "/".to_string(),
            http_only: true,
            secure: true,
        };
        let ext = to_extension_cookie(&auth, 1_800_000_000 + SESSION_COOKIE_TTL_SECS);
        assert_eq!(ext.expires, 1_800_000_000 + 90 * 86400);
        assert_eq!(ext.host_only, None);
    }
}
