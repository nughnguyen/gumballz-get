//! Commands of the TikTok category: batch watermark-free video, the sound with
//! attribution and the index of favorites / profile media.
//!
//! Video and sound are public content — the session only comes in as reinforcement,
//! for private accounts and so the site does not treat us like a bot. Favorites,
//! on the other hand, do not exist without the user's session.

use gumballz_core::core::tools::tiktok::{download, favorites, sound};

use super::{err, progress};

const DOMAIN: &str = "tiktok.com";

/// Cookies of the account chosen in the manager (None = `_default`).
fn session_of(slug: Option<&str>) -> Option<String> {
    let slug = slug.unwrap_or("_default");
    let content = crate::cookies::storage::read_account_file(DOMAIN, slug).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    crate::cookies::touch_last_used(DOMAIN, slug);
    Some(content)
}

#[tauri::command]
pub async fn tool_tt_download(
    app: tauri::AppHandle,
    opts: download::Options,
) -> Result<download::DownloadResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    download::run(&opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_tt_sound(
    app: tauri::AppHandle,
    opts: sound::Options,
) -> Result<sound::SoundResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    sound::run(&opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_tt_favorites(
    app: tauri::AppHandle,
    opts: favorites::Options,
) -> Result<favorites::FavoritesResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    favorites::run(&opts, progress(&app)).await.map_err(err)
}
