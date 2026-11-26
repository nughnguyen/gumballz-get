use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::Digest;

const USER_AGENT: &str = "okhttp/4.8.1";
const CLIENT_ID: &str = "46410c2fb9aa741c1f03cdea099929c795d20de0282b352eac881dfa46b2b89c";
const CLIENT_SECRET: &str = "e2fa7dc5bc347d09820a3931d4ce10e1137a02577ce647b33c60670a72b1acd5";
const MOBILE_TOKEN: &str = "ps407sr3rno1561ro2o4n360q21248s4o24oq33770rpro59o11q9r5469ososoo";

const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma",
];

#[derive(Clone)]
pub struct GumroadSession {
    pub token: String,
    pub email: String,
    pub client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub token: String,
    pub email: String,
    pub saved_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GumroadProduct {
    pub id: String,
    pub name: String,
    pub creator_name: String,
    pub preview_url: Option<String>,
    pub url_redirect_token: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GumroadFile {
    pub id: String,
    pub name: String,
    pub filetype: String,
    pub filegroup: String,
    pub download_url: Option<String>,
    pub size: u64,
}

fn build_client(token: &str) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );

    let client = gumballz_core::core::http_client::apply_global_proxy(reqwest::Client::builder())
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()?;

    Ok(client)
}

fn session_file_path() -> anyhow::Result<PathBuf> {
    crate::platforms::session_file("gumroad_session.json")
}

/// OAuth endpoints for the "Gumroad Mobile" application — the same client
/// this code has always used (the mobile app's client id/secret and the
/// `mobile_api creator_api` scopes).
///
/// Gumroad removed the `password` grant (`unsupported_grant_type`, seen
/// 2026-11-26), so email/password login now goes through the standard
/// authorization-code flow in an in-app browser window, exactly like the
/// official Gumroad mobile app does:
///   1. open the mobile pre-authorization page (it forwards every parameter
///      to `/oauth/authorize`);
///   2. the user signs in on Gumroad's own page and approves the app;
///   3. Gumroad redirects to the app's registered redirect URI with `?code=`;
///   4. the code is exchanged here for an access token (PKCE + client secret).
/// The app never sees the user's Gumroad password in this flow.
const AUTHORIZE_URL: &str = "https://gumroad.com/oauth/mobile_pre_authorization/new";
const TOKEN_URL: &str = "https://gumroad.com/oauth/token";
/// Redirect URI registered for the "Gumroad Mobile" OAuth application, per
/// Gumroad's own gumroad-mobile README:
/// `OauthApplication.create!(name: "Gumroad Mobile", redirect_uri: "gumroadmobile://", scopes: ["mobile_api", "creator_api"])`.
/// The desktop app has no custom-scheme handler; the login webview watches
/// for a navigation to this scheme and reads the `code` off the URL instead.
const REDIRECT_URI: &str = "gumroadmobile://";
const OAUTH_SCOPE: &str = "mobile_api creator_api";
/// How long the login window may stay open waiting for the authorization
/// redirect (same as the generic in-app auth webview's timeout).
const OAUTH_TIMEOUT: Duration = Duration::from_secs(600);

/// 32 random bytes, base64url-encoded without padding (43 characters).
/// Used for both the OAuth `state` (CSRF protection) value and the PKCE
/// verifier. Both are secrets while the login is in flight: never log them.
fn random_token() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The S256 PKCE challenge for a verifier (RFC 7636).
fn pkce_challenge(verifier: &str) -> String {
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// A fresh PKCE pair: the verifier and its S256 challenge. The verifier is
/// only ever sent to Gumroad's token endpoint, never logged.
fn generate_pkce() -> (String, String) {
    let verifier = random_token();
    let challenge = pkce_challenge(&verifier);
    (verifier, challenge)
}

/// The Gumroad authorization URL opened in the login webview. The user signs
/// in on Gumroad's own page; Gumroad then redirects to `REDIRECT_URI` with
/// `?code=…&state=…`.
fn build_authorize_url(state: &str, code_challenge: &str) -> String {
    let mut url = url::Url::parse(AUTHORIZE_URL).expect("hard-coded authorize URL is valid");
    url.query_pairs_mut()
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", OAUTH_SCOPE)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    url.to_string()
}

/// Pulls the authorization `code` (and the echoed `state`) out of the
/// redirect URL the login webview lands on (`gumroadmobile://?code=…&state=…`).
/// `url::Url` cannot always parse a bare custom-scheme URL, so a plain
/// query-string split is the fallback. Returns an error when Gumroad refused
/// the authorization (`?error=access_denied`, …) or when no code is present.
fn parse_redirect_url(redirect_url: &str) -> anyhow::Result<(String, String)> {
    let query = url::Url::parse(redirect_url)
        .ok()
        .and_then(|u| u.query().map(|q| q.to_string()))
        .or_else(|| redirect_url.split_once('?').map(|(_, q)| q.to_string()))
        .unwrap_or_default();
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
    if let Some(error) = params.get("error") {
        return Err(anyhow!("Gumroad authorization failed: {}", error));
    }
    let code = params.get("code").cloned().unwrap_or_default();
    if code.is_empty() {
        return Err(anyhow!("No authorization code in Gumroad redirect"));
    }
    let state = params.get("state").cloned().unwrap_or_default();
    Ok((code, state))
}

/// Exchanges an authorization code for an access token. Same endpoint, client
/// and scopes as the old password-grant request, with
/// `grant_type=authorization_code` plus the PKCE verifier. `email` is only
/// the display address typed in the app's login form — the token response
/// carries no email.
async fn exchange_code(
    code: &str,
    code_verifier: &str,
    email: &str,
) -> anyhow::Result<GumroadSession> {
    let temp_client =
        gumballz_core::core::http_client::apply_global_proxy(reqwest::Client::builder())
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .build()?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
        ("redirect_uri", REDIRECT_URI),
        ("code_verifier", code_verifier),
    ];

    let resp = temp_client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await?;

    let status = resp.status();
    let body_text = resp.text().await?;

    if !status.is_success() {
        return Err(anyhow!(
            "Authentication failed (status {}): {}",
            status,
            &body_text[..body_text.len().min(300)]
        ));
    }

    let body: serde_json::Value = serde_json::from_str(&body_text)?;

    let token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("access_token not found in login response"))?
        .to_string();

    let client = build_client(&token)?;

    Ok(GumroadSession {
        token,
        email: email.to_string(),
        client,
    })
}

/// Logs in through Gumroad's authorization page in an in-app browser window
/// (same pattern as Hotmart's `authenticate`): the user signs in on
/// Gumroad's own page, Gumroad redirects to the `gumroadmobile://` scheme
/// with `?code=`, and the code is exchanged for an access token. The
/// authorization code, PKCE verifier and `state` value are never logged.
pub async fn authenticate_via_browser(
    app: &tauri::AppHandle,
    email: &str,
) -> anyhow::Result<GumroadSession> {
    use std::sync::{Arc, Mutex};
    use tauri::Manager;

    let oauth_state = random_token();
    let (code_verifier, code_challenge) = generate_pkce();
    let authorize_url = build_authorize_url(&oauth_state, &code_challenge);

    // The redirect back from Gumroad lands on the `gumroadmobile://` scheme,
    // which no browser on the machine handles. The webview watches for that
    // navigation and reads the authorization code off the URL instead of
    // letting the navigation load.
    let redirect_seen = Arc::new(Mutex::new(Option::<String>::None));
    let redirect_nav = redirect_seen.clone();

    if let Some(existing) = app.get_webview_window("gumroad-login") {
        let _ = existing.close();
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let login_window = tauri::WebviewWindowBuilder::new(
        app,
        "gumroad-login",
        tauri::WebviewUrl::External(
            authorize_url
                .parse()
                .map_err(|e| anyhow!("Invalid authorize URL: {}", e))?,
        ),
    )
    .title("Gumroad Login")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        if url.scheme() == "gumroadmobile" {
            // The URL carries the authorization code: never log it.
            tracing::info!("[gumroad] authorization redirect received");
            if let Ok(mut slot) = redirect_nav.lock() {
                *slot = Some(url.to_string());
            }
            return false; // do not try to load the custom scheme
        }
        true
    })
    .build()
    .map_err(|e| anyhow!("Failed to create login window: {}", e))?;

    let start = std::time::Instant::now();
    let mut last_log_secs: u64 = 0;

    loop {
        // The redirect wins over a closed window: the code may have arrived
        // in the same instant the user closed the window.
        if let Some(redirect_url) = redirect_seen
            .lock()
            .ok()
            .and_then(|g| g.as_ref().cloned())
        {
            let _ = login_window.close();
            let (code, returned_state) = parse_redirect_url(&redirect_url)?;
            if returned_state != oauth_state {
                return Err(anyhow!(
                    "Login failed: Gumroad returned an unexpected authorization response. Please try again."
                ));
            }
            return exchange_code(&code, &code_verifier, email).await;
        }

        // If the user closed the login window, stop at once instead of
        // waiting out the timeout with nothing to exchange.
        if app.get_webview_window("gumroad-login").is_none() {
            return Err(anyhow!("Login window was closed"));
        }

        if start.elapsed() > OAUTH_TIMEOUT {
            let _ = login_window.close();
            return Err(anyhow!("Login timed out. Please try again."));
        }

        let elapsed_secs = start.elapsed().as_secs();
        if elapsed_secs >= last_log_secs + 30 {
            last_log_secs = elapsed_secs;
            tracing::info!("[gumroad] waiting for authorization ({}s)", elapsed_secs);
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn validate_token(session: &GumroadSession) -> anyhow::Result<bool> {
    let url = format!(
        "https://api.gumroad.com/mobile/purchases/index.json?include_subscriptions=true&include_mobile_unfriendly_products=true&mobile_token={}",
        MOBILE_TOKEN
    );

    let resp = session.client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let body: serde_json::Value = resp.json().await?;
    Ok(body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

pub async fn list_products(session: &GumroadSession) -> anyhow::Result<Vec<GumroadProduct>> {
    let url = format!(
        "https://api.gumroad.com/mobile/purchases/index.json?include_subscriptions=true&include_mobile_unfriendly_products=true&mobile_token={}",
        MOBILE_TOKEN
    );

    let resp = session.client.get(&url).send().await?;

    let status = resp.status();
    let body_text = resp.text().await?;

    if !status.is_success() {
        return Err(anyhow!(
            "list_products returned status {}: {}",
            status,
            &body_text[..body_text.len().min(300)]
        ));
    }

    let body: serde_json::Value = serde_json::from_str(&body_text)?;

    let success = body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !success {
        return Err(anyhow!("API returned success=false"));
    }

    let products = body
        .get("products")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut result = Vec::new();

    for p in &products {
        if p.get("refunded").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if p.get("partially_refunded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if p.get("chargedback")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if p.get("is_archived")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = p
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        let creator_name = p
            .get("creator_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let preview_url = p
            .get("preview_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let url_redirect_token = p
            .get("url_redirect_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let file_count = p
            .get("file_data")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        result.push(GumroadProduct {
            id,
            name,
            creator_name,
            preview_url,
            url_redirect_token,
            file_count,
        });
    }

    Ok(result)
}

pub async fn get_product_files(
    _session: &GumroadSession,
    product_json: &serde_json::Value,
) -> Vec<GumroadFile> {
    let file_data = product_json
        .get("file_data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut files = Vec::new();

    for f in &file_data {
        let id = f
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = f
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("file")
            .to_string();
        let filetype = f
            .get("filetype")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let filegroup = f
            .get("filegroup")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let download_url = f
            .get("download_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let size = f.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

        files.push(GumroadFile {
            id,
            name,
            filetype,
            filegroup,
            download_url,
            size,
        });
    }

    files
}

pub fn is_media_file(file: &GumroadFile) -> bool {
    if file.filegroup == "video" || file.filegroup == "audio" {
        return true;
    }
    MEDIA_EXTENSIONS.contains(&file.filetype.as_str())
}

pub async fn get_stream_url(
    session: &GumroadSession,
    url_redirect_token: &str,
    file_id: &str,
) -> anyhow::Result<String> {
    let url = format!(
        "https://api.gumroad.com/mobile/url_redirects/stream/{}/{}?mobile_token={}",
        url_redirect_token, file_id, MOBILE_TOKEN
    );

    let resp = session.client.get(&url).send().await?;

    let status = resp.status();
    let body_text = resp.text().await?;

    if !status.is_success() {
        return Err(anyhow!(
            "get_stream_url returned status {}: {}",
            status,
            &body_text[..body_text.len().min(300)]
        ));
    }

    let body: serde_json::Value = serde_json::from_str(&body_text)?;

    let playlist_url = body
        .get("playlist_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No playlist_url in stream response"))?
        .to_string();

    Ok(playlist_url)
}

pub fn get_direct_download_url(url_redirect_token: &str, file_id: &str) -> String {
    format!(
        "https://api.gumroad.com/mobile/url_redirects/download/{}/{}?mobile_token={}",
        url_redirect_token, file_id, MOBILE_TOKEN
    )
}

pub async fn save_session(session: &GumroadSession) -> anyhow::Result<()> {
    let path = session_file_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let saved = SavedSession {
        token: session.token.clone(),
        email: session.email.clone(),
        saved_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let json = serde_json::to_string_pretty(&saved)?;
    tokio::fs::write(&path, json).await?;
    tracing::info!("[gumroad] session saved for {}", session.email);
    Ok(())
}

pub async fn load_session() -> anyhow::Result<Option<GumroadSession>> {
    let path = session_file_path()?;
    let json = match tokio::fs::read_to_string(&path).await {
        Ok(j) => j,
        Err(_) => return Ok(None),
    };

    let saved: SavedSession = serde_json::from_str(&json)?;
    let client = build_client(&saved.token)?;

    tracing::info!("[gumroad] session loaded for {}", saved.email);

    Ok(Some(GumroadSession {
        token: saved.token,
        email: saved.email,
        client,
    }))
}

pub async fn delete_saved_session() -> anyhow::Result<()> {
    let path = session_file_path()?;
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        tokio::fs::remove_file(&path).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_test_vector() {
        // RFC 7636, Appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_token_is_urlsafe_and_43_chars() {
        let token = random_token();
        assert_eq!(token.len(), 43);
        assert!(token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(random_token(), token);
    }

    #[test]
    fn authorize_url_carries_mobile_oauth_params() {
        let url = build_authorize_url("test-state", "test-challenge");
        assert!(url.starts_with("https://gumroad.com/oauth/mobile_pre_authorization/new?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("redirect_uri=gumroadmobile%3A%2F%2F"));
        assert!(url.contains("scope=mobile_api"));
        assert!(url.contains("creator_api"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("code_challenge=test-challenge"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn redirect_url_parses_code_and_state() {
        let (code, state) =
            parse_redirect_url("gumroadmobile://?code=abc123&state=xyz").unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "xyz");
    }

    #[test]
    fn redirect_url_rejects_denied_authorization() {
        let result = parse_redirect_url("gumroadmobile://?error=access_denied&state=xyz");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("access_denied"));
    }

    #[test]
    fn redirect_url_rejects_missing_code() {
        assert!(parse_redirect_url("gumroadmobile://?state=xyz").is_err());
        assert!(parse_redirect_url("gumroadmobile://").is_err());
    }
}
