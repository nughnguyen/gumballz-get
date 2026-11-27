//! Commands of the Vimeo category: private/unlisted video (link with a hash or a
//! password the user already has) and batch backup of showcase, album and channel.
//!
//! The cookie manager's session comes in here and only here — the frontend never
//! sends a cookie, and the password never comes back in anything the frontend
//! receives.

use gumballz_core::core::tools::vimeo;

use super::{err, progress};

const DOMAIN: &str = "vimeo.com";

/// Cookies of the account chosen in the manager (None = `_default`). Without a
/// session Vimeo only delivers what is public; with it, the showcase and the
/// video only the user's account can see start opening.
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
pub async fn tool_vm_list(
    app: tauri::AppHandle,
    opts: vimeo::ListOptions,
) -> Result<vimeo::Listing, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    vimeo::list(&opts, &progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_vm_showcase(
    app: tauri::AppHandle,
    opts: vimeo::BackupOptions,
) -> Result<vimeo::BackupResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    vimeo::backup(&opts, &progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_vm_private(
    app: tauri::AppHandle,
    opts: vimeo::PrivateOptions,
) -> Result<vimeo::PrivateResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(opts.account_slug.as_deref());
    vimeo::private(&opts, &progress(&app)).await.map_err(err)
}
