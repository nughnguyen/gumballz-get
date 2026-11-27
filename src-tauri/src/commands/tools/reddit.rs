//! Commands of the Reddit category. All public content: downloading a post's
//! media (v.redd.it video with audio, gallery, image, outside link), archiving
//! a whole thread with its comments and reading the user's official data export
//! (that last one without touching the network).

use gumballz_core::core::tools::reddit::{download, gdpr, thread};

use super::{err, progress};

const DOMAIN: &str = "reddit.com";

/// Cookies of the account chosen in the manager (None = `_default`). Reddit
/// started answering 403 on every anonymous `.json`; with the extension's
/// session it answers the way it answers the user's browser, and the entry
/// gate's JavaScript challenge becomes just the safety net for whoever captured
/// no cookie at all.
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
pub async fn tool_rd_download(
    app: tauri::AppHandle,
    opts: download::Options,
) -> Result<download::DownloadResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    download::run(&opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_rd_thread(
    app: tauri::AppHandle,
    opts: thread::Options,
) -> Result<thread::ThreadResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    thread::run(&opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_rd_gdpr(
    app: tauri::AppHandle,
    opts: gdpr::Options,
) -> Result<gdpr::GdprResult, String> {
    let p = progress(&app);
    tokio::task::spawn_blocking(move || gdpr::run(&opts, &p))
        .await
        .map_err(err)?
        .map_err(err)
}
