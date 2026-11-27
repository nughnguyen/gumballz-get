//! Commands of the Blogs category: personal archive of Substack newsletters and
//! export of the user's own stories on Medium.
//!
//! Both tools read the session from the cookie manager before calling the core —
//! it is what lets Substack list the account's subscriptions and Medium deliver
//! the drafts. Without a cookie, both still work on what is public (a
//! publication's archive, a profile feed).

use gumballz_core::core::tools::blogs::{medium, substack};

use super::{err, progress};

const SUBSTACK_DOMAIN: &str = "substack.com";
const MEDIUM_DOMAIN: &str = "medium.com";

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

#[tauri::command]
pub async fn tool_blog_substack(
    app: tauri::AppHandle,
    opts: substack::Options,
) -> Result<substack::ArchiveResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(SUBSTACK_DOMAIN, opts.account_slug.as_deref());
    substack::run(&opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_blog_medium(
    app: tauri::AppHandle,
    opts: medium::Options,
) -> Result<medium::ExportResult, String> {
    let mut opts = opts;
    opts.session_netscape = session_of(MEDIUM_DOMAIN, opts.account_slug.as_deref());
    medium::run(&opts, progress(&app)).await.map_err(err)
}
