use gumballz_core::core::tools::{ryd, sponsorblock};

use super::err;

#[tauri::command]
pub async fn tool_sponsorblock(
    url: String,
    categories: Option<Vec<String>>,
) -> Result<sponsorblock::SponsorResult, String> {
    sponsorblock::segments(&url, &categories.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn tool_ryd(url: String) -> Result<ryd::Votes, String> {
    ryd::votes(&url).await.map_err(err)
}

#[tauri::command]
pub fn tool_yt_video_id(url: String) -> Option<String> {
    sponsorblock::video_id(&url)
}

/// Downloads a plain URL (thumbnail, frame) to a file.
#[tauri::command]
pub async fn tool_save_url(url: String, dest: String) -> Result<String, String> {
    let client = gumballz_core::core::tools::client().map_err(err)?;
    let resp = client.get(&url).send().await.map_err(err)?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(err)?;
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    tokio::fs::write(&dest, &bytes).await.map_err(err)?;
    Ok(dest)
}

// ── Archive, notes and chapters (round 3) ──────────────────────────────

use gumballz_core::core::tools::{yt_archive, yt_chapters, yt_notes};

use super::progress;

const YT_DOMAIN: &str = "youtube.com";

/// Cookies of the account chosen in the manager (None = `_default`). Watch Later
/// and private playlists do not exist without a session; a public video needs none.
fn yt_session(slug: Option<&str>) -> Option<String> {
    let slug = slug.unwrap_or("_default");
    let content = crate::cookies::storage::read_account_file(YT_DOMAIN, slug).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    crate::cookies::touch_last_used(YT_DOMAIN, slug);
    Some(content)
}

/// Only enumerates the collection and merges it with the saved state — downloads nothing.
#[tauri::command]
pub async fn tool_yt_archive_scan(
    app: tauri::AppHandle,
    opts: yt_archive::Options,
) -> Result<yt_archive::ArchiveResult, String> {
    let mut opts = opts;
    opts.session_netscape = yt_session(opts.account_slug.as_deref());
    yt_archive::scan(&opts, &progress(&app)).await.map_err(err)
}

/// Continues the archive from where it stopped.
#[tauri::command]
pub async fn tool_yt_archive_run(
    app: tauri::AppHandle,
    opts: yt_archive::Options,
) -> Result<yt_archive::ArchiveResult, String> {
    let mut opts = opts;
    opts.session_netscape = yt_session(opts.account_slug.as_deref());
    yt_archive::run(opts, progress(&app)).await.map_err(err)
}

/// State saved in the folder, no network — it is what the screen shows on open.
#[tauri::command]
pub fn tool_yt_archive_state(dest: String) -> Result<yt_archive::ArchiveResult, String> {
    yt_archive::state(&dest).map_err(err)
}

#[tauri::command]
pub fn tool_yt_archive_cancel() {
    yt_archive::cancel();
}

#[tauri::command]
pub async fn tool_yt_notes(
    app: tauri::AppHandle,
    opts: yt_notes::Options,
) -> Result<yt_notes::NotesResult, String> {
    let mut opts = opts;
    opts.session_netscape = yt_session(opts.account_slug.as_deref());
    yt_notes::run(opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_yt_chapters(
    app: tauri::AppHandle,
    opts: yt_chapters::Options,
) -> Result<yt_chapters::ChaptersResult, String> {
    yt_chapters::run(opts, progress(&app)).await.map_err(err)
}

/// Stable identifier of the local SponsorBlock key. The key itself never leaves
/// the backend.
#[derive(serde::Serialize)]
pub struct SponsorUser {
    pub fingerprint: String,
}

#[tauri::command]
pub fn tool_sb_user() -> Result<SponsorUser, String> {
    let key = sponsorblock::local_user_id().map_err(err)?;
    Ok(SponsorUser {
        fingerprint: sponsorblock::public_fingerprint(&key)[..12].to_string(),
    })
}

/// Sends segments to SponsorBlock. `opts.confirmed` may only come from the user's
/// explicit confirmation: the submission is public and counts for everyone.
#[tauri::command]
pub async fn tool_sb_submit(
    opts: sponsorblock::SubmitOptions,
) -> Result<sponsorblock::SubmitResult, String> {
    sponsorblock::submit(opts).await.map_err(err)
}
