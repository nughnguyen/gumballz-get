//! Commands of the Blogs category for Tumblr and art favorites: index of likes,
//! offline mirror of a blog and unified index of DeviantArt, ArtStation and
//! Flickr. Reading is all gallery-dl post-processing.

use gumballz_core::core::tools::tumblr::{art, backup, gdl, likes};

use super::{err, progress};

const TUMBLR: &str = "tumblr.com";

/// Cookies of the account chosen in the manager (None = `_default`). Likes and
/// favorites are account pages: without a session gallery-dl only sees what is
/// public.
fn session_of(domain: &str, slug: Option<&str>) -> Option<String> {
    let slug = slug.unwrap_or("_default");
    let content = crate::cookies::storage::read_account_file(domain, slug).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    crate::cookies::touch_last_used(domain, slug);
    Some(content)
}

/// The three art platforms in a single Netscape file — that is what gallery-dl's
/// `--cookies` accepts.
fn art_session(slug: Option<&str>) -> Option<String> {
    let parts: Vec<String> = art::domains()
        .into_iter()
        .filter_map(|d| session_of(d, slug))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(gdl::merge_netscape(&parts))
}

#[tauri::command]
pub async fn tool_tumblr_likes(
    app: tauri::AppHandle,
    opts: likes::Options,
) -> Result<likes::LikesResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(TUMBLR, opts.account_slug.as_deref());
    likes::run(&opts, &progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_tumblr_backup(
    app: tauri::AppHandle,
    opts: backup::Options,
) -> Result<backup::BackupResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(TUMBLR, opts.account_slug.as_deref());
    backup::run(&opts, &progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_art_favorites(
    app: tauri::AppHandle,
    opts: art::Options,
) -> Result<art::ArtResult, String> {
    let mut opts = opts;
    opts.session_netscape = art_session(opts.account_slug.as_deref());
    art::run(&opts, &progress(&app)).await.map_err(err)
}
