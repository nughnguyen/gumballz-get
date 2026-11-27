//! Commands of the Lists category: export the Letterboxd, Trakt and Goodreads
//! diary and shelf, and merge it all into a single timeline.
//!
//! Letterboxd and Goodreads have no public API — the way in is the session the
//! extension captured, each in its own cookie bucket. Trakt has an official API
//! and uses the device flow, with credentials the user creates themselves.

use gumballz_core::core::tools::lists::{goodreads, letterboxd, merge, trakt};

use super::{err, progress};

/// Cookies of the account chosen in the manager (None = `_default`).
fn session_of(domain: &str, slug: Option<&str>) -> Option<String> {
    let slug = slug.unwrap_or("_default");
    let content = crate::cookies::storage::read_account_file(domain, slug).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    crate::cookies::touch_last_used(domain, slug);
    Some(content)
}

/// Whether a saved session exists for the domain, so the UI can say if the user
/// is logged in before they press any button.
#[tauri::command]
pub fn tool_lists_session(domain: String, account_slug: Option<String>) -> bool {
    let domain = match domain.as_str() {
        "goodreads" | "goodreads.com" => goodreads::DOMAIN,
        _ => letterboxd::DOMAIN,
    };
    session_of(domain, account_slug.as_deref()).is_some()
}

// ── Letterboxd ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn tool_lb_export(
    app: tauri::AppHandle,
    opts: letterboxd::Options,
) -> Result<letterboxd::ExportResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(letterboxd::DOMAIN, opts.account_slug.as_deref());
    letterboxd::run(&opts, progress(&app)).await.map_err(err)
}

// ── Trakt ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn tool_trakt_creds() -> trakt::CredsView {
    trakt::creds_view()
}

#[tauri::command]
pub fn tool_trakt_save_app(
    client_id: String,
    client_secret: String,
) -> Result<trakt::CredsView, String> {
    trakt::save_app(&client_id, &client_secret).map_err(err)
}

#[tauri::command]
pub fn tool_trakt_disconnect(forget: bool) -> Result<trakt::CredsView, String> {
    if forget {
        trakt::forget().map_err(err)
    } else {
        trakt::disconnect().map_err(err)
    }
}

/// Step 1 of the device flow: the code the user types on the site.
#[tauri::command]
pub async fn tool_trakt_device_code() -> Result<trakt::DeviceCode, String> {
    trakt::device_code().await.map_err(err)
}

/// Step 2: waits for the user to grant access and saves the token.
#[tauri::command]
pub async fn tool_trakt_connect(
    app: tauri::AppHandle,
    code: trakt::DeviceCode,
) -> Result<trakt::ConnectResult, String> {
    trakt::wait_for_token(&code, progress(&app))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn tool_trakt_export(
    app: tauri::AppHandle,
    opts: trakt::Options,
) -> Result<trakt::ExportResult, String> {
    trakt::run(&opts, progress(&app)).await.map_err(err)
}

// ── Goodreads ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn tool_gr_export(
    app: tauri::AppHandle,
    opts: goodreads::Options,
) -> Result<goodreads::ExportResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(goodreads::DOMAIN, opts.account_slug.as_deref());
    goodreads::run(&opts, progress(&app)).await.map_err(err)
}

// ── Single diary ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn tool_media_diary(
    app: tauri::AppHandle,
    opts: merge::Options,
) -> Result<merge::MergeResult, String> {
    let p = progress(&app);
    tokio::task::spawn_blocking(move || merge::run(&opts, &p))
        .await
        .map_err(err)?
        .map_err(err)
}
