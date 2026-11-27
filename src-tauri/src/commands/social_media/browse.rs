//! Friends browser for Facebook and Instagram (social-media downloads).
//!
//! After the user logs in (in-app login window or the browser extension
//! auto-capturing cookies into the Cookie Manager store), this module
//! lists their friends and each friend's recent posts using ONLY the
//! stored session cookies — no passwords, no new login flow.
//!
//! Cookie source: the same `resolve_active_cookies` path the downloader
//! uses (manual file -> Cookie Manager store -> browser profile). Only
//! the sandbox file yields readable cookie values, so a browser-arg-only
//! resolution errors out with a message that says to capture cookies
//! via the extension or the in-app login instead of silently failing.
//!
//! Instagram: the private `i.instagram.com/api/v1/` endpoints with the
//! `sessionid` cookie (current_user, friendships following/followers,
//! user feed). These are the stable read endpoints the mobile app uses.
//!
//! Facebook: there is NO stable cookie-only GraphQL route — the friends
//! queries need a `doc_id` that rotates with every Meta rebuild and
//! cannot be obtained without a live session. Instead the friends list
//! and timelines are read from `mbasic.facebook.com` HTML with the
//! stored `c_user`/`xs` cookies. That surface is old and Meta may
//! remove it; if it answers with a login wall the command returns an
//! error that says so instead of fake data.

#![allow(clippy::all)]
#![allow(warnings)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use tauri::command;

use super::cookies::{resolve_active_cookies, ResolvedCookies};
use super::http::parse_netscape_cookies;

const IG_APP_ID: &str = "936619743392459";
const IG_UA: &str = "Instagram 269.0.0.18.75 Android (26/8.0.0; 480dpi; 1080x1920; samsung; SM-G998B; p3s; qcom; en_US; 458229237)";
const FB_MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 10; SM-G973F) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";

const MAX_FRIENDS: usize = 50;
const MAX_POSTS: usize = 30;
const MAX_FB_PAGES: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct BrowsePerson {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub profile_url: String,
    /// "following" | "followers" (Instagram) or None (Facebook).
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowsePost {
    pub id: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub caption: Option<String>,
    pub taken_at: Option<i64>,
    /// "photo" | "video" | "carousel".
    pub media_type: String,
}

/// A resolved login session for one platform: the cookie values plus
/// the sandbox that owns the temp cookie file (kept alive for the
/// whole browse call — dropping it deletes the file).
struct BrowseSession {
    cookie_header: String,
    cookies: HashMap<String, String>,
    _resolved: ResolvedCookies,
}

fn browse_session(platform: &str) -> Result<BrowseSession, String> {
    let platform = platform.to_lowercase();
    let (domain, session_names): (&str, &[&str]) = match platform.as_str() {
        "instagram" => ("instagram.com", &["sessionid"]),
        "facebook" => ("facebook.com", &["c_user", "xs"]),
        _ => return Err(format!("Unsupported browse platform: {}", platform)),
    };

    let social_dir = super::gumballz_get_social_media_dir()?;
    let resolved = resolve_active_cookies(&platform, None, Some("auto"), &social_dir);
    let Some(sb) = resolved.sandbox.as_ref() else {
        return Err(
            "Could not read the session cookie — capture cookies with the extension or sign in in the app (Cài đặt → Cookies → Đăng nhập trong app), then try again."
                .to_string(),
        );
    };
    let content = fs::read_to_string(sb.path())
        .map_err(|e| format!("Could not read the cookie file: {}", e))?;
    let pairs = parse_netscape_cookies(&content, Some(domain));
    if pairs.is_empty() {
        return Err(format!(
            "No {} cookie in the store — sign in again (Cài đặt → Cookies → Đăng nhập trong app).",
            platform
        ));
    }
    let cookies: HashMap<String, String> = pairs.iter().cloned().collect();
    for name in session_names {
        if !cookies.contains_key(*name) {
            return Err(format!(
                "The {} session has expired (missing cookie {}) — sign in again (Cài đặt → Cookies → Đăng nhập trong app).",
                platform, name
            ));
        }
    }
    let cookie_header = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");
    Ok(BrowseSession {
        cookie_header,
        cookies,
        _resolved: resolved,
    })
}

fn api_client() -> Result<reqwest::Client, String> {
    crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

fn cookie_header_value(header: &str) -> Result<reqwest::header::HeaderValue, String> {
    reqwest::header::HeaderValue::from_str(header)
        .map_err(|_| "The session cookie contains invalid characters — sign in again.".to_string())
}

// ---------------------------------------------------------------------------
// Instagram (i.instagram.com/api/v1, sessionid cookie)
// ---------------------------------------------------------------------------

async fn ig_get(
    client: &reqwest::Client,
    url: &str,
    cookie: &str,
) -> Result<serde_json::Value, String> {
    let resp = client
        .get(url)
        .header(reqwest::header::USER_AGENT, IG_UA)
        .header("X-IG-App-ID", IG_APP_ID)
        .header(reqwest::header::ACCEPT_LANGUAGE, "en-US")
        .header(reqwest::header::COOKIE, cookie_header_value(cookie)?)
        .send()
        .await
        .map_err(|e| format!("Could not reach Instagram: {}", e))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Could not read the reply from Instagram: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "Instagram returned HTTP {} — the session may have expired or the account may be under check, sign in again.",
            status.as_u16()
        ));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Instagram returned strange data (not JSON).".to_string())?;
    if value.get("status").and_then(|s| s.as_str()) == Some("fail") {
        let msg = value
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("request rejected");
        return Err(format!("Instagram: {}", msg));
    }
    Ok(value)
}

fn ig_person(u: &serde_json::Value, kind: &str) -> Option<BrowsePerson> {
    let id = u
        .get("pk")
        .and_then(|p| p.as_u64())
        .map(|n| n.to_string())
        .or_else(|| {
            u.get("pk")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string())
        })?;
    let username = u
        .get("username")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let full_name = u
        .get("full_name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let name = if full_name.trim().is_empty() {
        username.clone()
    } else {
        full_name
    };
    if name.trim().is_empty() {
        return None;
    }
    Some(BrowsePerson {
        id,
        name,
        username: if username.is_empty() {
            None
        } else {
            Some(username.clone())
        },
        avatar_url: u
            .get("profile_pic_url")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        profile_url: if username.is_empty() {
            "https://www.instagram.com/".to_string()
        } else {
            format!("https://www.instagram.com/{}/", username)
        },
        kind: Some(kind.to_string()),
    })
}

fn parse_ig_people(value: &serde_json::Value, kind: &str) -> Vec<BrowsePerson> {
    let mut out = Vec::new();
    if let Some(users) = value.get("users").and_then(|u| u.as_array()) {
        for u in users {
            if let Some(p) = ig_person(u, kind) {
                out.push(p);
            }
        }
    }
    out
}

fn parse_ig_posts(value: &serde_json::Value) -> Vec<BrowsePost> {
    let mut out = Vec::new();
    let Some(items) = value.get("items").and_then(|i| i.as_array()) else {
        return out;
    };
    for it in items {
        let Some(code) = it.get("code").and_then(|c| c.as_str()) else {
            continue;
        };
        if code.is_empty() {
            continue;
        }
        let media_type = match it.get("media_type").and_then(|m| m.as_u64()) {
            Some(2) => "video",
            Some(8) => "carousel",
            _ => "photo",
        }
        .to_string();
        let thumbnail_url = it
            .get("image_versions2")
            .and_then(|iv| iv.get("candidates"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());
        out.push(BrowsePost {
            id: it
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or(code)
                .to_string(),
            url: format!("https://www.instagram.com/p/{}/", code),
            thumbnail_url,
            caption: it
                .get("caption")
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string()),
            taken_at: it.get("taken_at").and_then(|t| t.as_i64()),
            media_type,
        });
    }
    out
}

async fn browse_ig_friends(session: &BrowseSession) -> Result<Vec<BrowsePerson>, String> {
    let client = api_client()?;
    let user_id = match session.cookies.get("ds_user_id") {
        Some(id) if !id.trim().is_empty() => id.trim().to_string(),
        _ => {
            let me = ig_get(
                &client,
                "https://i.instagram.com/api/v1/accounts/current_user/?edit=true",
                &session.cookie_header,
            )
            .await?;
            me.get("user")
                .and_then(|u| u.get("pk"))
                .and_then(|p| p.as_u64())
                .map(|n| n.to_string())
                .ok_or_else(|| {
                    "Instagram did not return the current account — sign in again.".to_string()
                })?
        }
    };

    let following = ig_get(
        &client,
        &format!(
            "https://i.instagram.com/api/v1/friendships/{}/following/?count=50",
            user_id
        ),
        &session.cookie_header,
    )
    .await
    .map_err(|e| format!("Could not load the following list: {}", e))?;
    let mut people = parse_ig_people(&following, "following");

    let followers = ig_get(
        &client,
        &format!(
            "https://i.instagram.com/api/v1/friendships/{}/followers/?count=50",
            user_id
        ),
        &session.cookie_header,
    )
    .await
    .map_err(|e| format!("Could not load the followers list: {}", e))?;
    people.extend(parse_ig_people(&followers, "followers"));

    people.truncate(MAX_FRIENDS);
    Ok(people)
}

async fn browse_ig_posts(session: &BrowseSession, person_id: &str) -> Result<Vec<BrowsePost>, String> {
    let client = api_client()?;
    let feed = ig_get(
        &client,
        &format!(
            "https://i.instagram.com/api/v1/feed/user/{}/?count=30",
            person_id.trim()
        ),
        &session.cookie_header,
    )
    .await
    .map_err(|e| format!("Could not load the posts: {}", e))?;
    let mut posts = parse_ig_posts(&feed);
    posts.truncate(MAX_POSTS);
    Ok(posts)
}

// ---------------------------------------------------------------------------
// Facebook (mbasic.facebook.com HTML, c_user/xs cookies)
// ---------------------------------------------------------------------------

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    html_unescape(&out)
}

fn fb_is_login_wall(url: &str, html: &str) -> bool {
    url.contains("login")
        || (html.contains("name=\"email\"") && html.contains("name=\"pass\""))
        || html.contains("Log into Facebook")
}

fn fb_resolve(base: &str, href: &str) -> String {
    let href = href.trim();
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if let Some(stripped) = href.strip_prefix('/') {
        return format!("{}{}", base.trim_end_matches('/'), format!("/{}", stripped));
    }
    format!("{}/{}", base.trim_end_matches('/'), href)
}

/// One friend link from the mbasic friends page: numeric profile.php id
/// or a username slug, plus the display name and avatar when present.
fn parse_fb_friends_html(html: &str, self_id: &str) -> Vec<BrowsePerson> {
    let mut out: Vec<BrowsePerson> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let anchor_re = Regex::new(r#"(?is)<a\s+[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
        .expect("anchor regex");
    let img_re =
        Regex::new(r#"(?i)<img\s+[^>]*src="([^"]+)""#).expect("img regex");
    let id_re = Regex::new(r"profile\.php\?id=(\d+)").expect("id regex");
    let slug_re = Regex::new(r"^/([A-Za-z0-9._]{2,64})(\?|$)").expect("slug regex");
    // mbasic chrome / actions, never a person.
    let skip_slugs: &[&str] = &[
        "login", "logout", "home", "search", "notifications", "messages", "groups",
        "events", "pages", "marketplace", "watch", "saved", "settings", "help",
        "story.php", "permalink.php", "photo.php", "video.php", "reel",
    ];

    for cap in anchor_re.captures_iter(html) {
        let href = html_unescape(cap.get(1).map(|m| m.as_str()).unwrap_or(""));
        let inner = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let name = strip_tags(inner).trim().to_string();
        if name.is_empty() || name.len() > 120 {
            continue;
        }
        let lower_name = name.to_lowercase();
        if lower_name.contains("xem thêm") || lower_name.contains("see more") {
            continue;
        }
        let (id, username, profile_url) = if let Some(m) = id_re.captures(&href) {
            let id = m.get(1).unwrap().as_str().to_string();
            if id == self_id {
                continue;
            }
            let url = format!("https://www.facebook.com/profile.php?id={}", id);
            (id, None, url)
        } else if let Some(m) = slug_re.captures(href.split('#').next().unwrap_or("")) {
            let slug = m.get(1).unwrap().as_str().to_string();
            if skip_slugs.contains(&slug.to_lowercase().as_str()) {
                continue;
            }
            // A slug link with no readable name is navigation, not a person.
            if name.chars().any(|c| c.is_ascii_digit()) && name.len() < 3 {
                continue;
            }
            let url = format!("https://www.facebook.com/{}/", slug);
            (slug.clone(), Some(slug), url)
        } else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let avatar_url = img_re
            .captures(inner)
            .and_then(|c| c.get(1))
            .map(|m| html_unescape(m.as_str()));
        out.push(BrowsePerson {
            id,
            name,
            username,
            avatar_url,
            profile_url,
            kind: None,
        });
        if out.len() >= MAX_FRIENDS {
            break;
        }
    }
    out
}

/// "See more friends" link on the mbasic friends page, if any.
fn fb_friends_next_link(html: &str) -> Option<String> {
    let anchor_re = Regex::new(r#"(?is)<a\s+[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#).ok()?;
    for cap in anchor_re.captures_iter(html) {
        let href = html_unescape(cap.get(1).map(|m| m.as_str()).unwrap_or(""));
        let text = strip_tags(cap.get(2).map(|m| m.as_str()).unwrap_or(""))
            .trim()
            .to_lowercase();
        if href.contains("v=friends") && (text.contains("xem thêm") || text.contains("see more"))
        {
            return Some(href);
        }
    }
    None
}

/// Recent posts from a mbasic profile timeline: story links
/// (`story.php` / `permalink.php` with a story_fbid) become canonical
/// www post URLs; the caption is the nearest preceding paragraph text.
fn parse_fb_posts_html(html: &str) -> Vec<BrowsePost> {
    let mut out: Vec<BrowsePost> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let story_re = Regex::new(
        r#"(?i)(?:story\.php|permalink\.php)\?story_fbid=(\d+)(?:&amp;|&)id=(\d+)"#,
    )
    .expect("story regex");
    let para_re = Regex::new(r"(?is)<p[^>]*>(.*?)</p>").expect("para regex");
    let img_re =
        Regex::new(r#"(?i)<img\s+[^>]*src="([^"]+)""#).expect("img regex");

    for cap in story_re.captures_iter(html) {
        let fbid = cap.get(1).unwrap().as_str().to_string();
        let owner = cap.get(2).unwrap().as_str().to_string();
        if !seen.insert(fbid.clone()) {
            continue;
        }
        let m = cap.get(0).unwrap();
        let before = &html[..m.start()];
        let after = &html[m.end()..];
        // Caption: the last <p> before this story link, else the tail of
        // the stripped text before it (navigation words trimmed).
        let mut caption: Option<String> = para_re
            .captures_iter(before)
            .filter_map(|c| c.get(1))
            .map(|t| strip_tags(t.as_str()).trim().to_string())
            .filter(|t| !t.is_empty() && t.len() <= 400)
            .last();
        if caption.is_none() {
            let tail = strip_tags(&before[before.len().saturating_sub(1500)..])
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let tail = tail.trim().to_string();
            if !tail.is_empty() && tail.len() <= 400 {
                caption = Some(tail);
            }
        }
        let thumbnail_url = img_re
            .captures(&after[..after.len().min(3000)])
            .and_then(|c| c.get(1))
            .map(|m| html_unescape(m.as_str()));
        out.push(BrowsePost {
            id: fbid.clone(),
            url: format!(
                "https://www.facebook.com/permalink.php?story_fbid={}&id={}",
                fbid, owner
            ),
            thumbnail_url,
            caption,
            taken_at: None,
            media_type: "photo".to_string(),
        });
        if out.len() >= MAX_POSTS {
            break;
        }
    }
    out
}

async fn fb_fetch(
    client: &reqwest::Client,
    url: &str,
    cookie: &str,
) -> Result<(String, String), String> {
    let resp = client
        .get(url)
        .header(reqwest::header::USER_AGENT, FB_MOBILE_UA)
        .header(reqwest::header::ACCEPT_LANGUAGE, "vi-VN,vi;q=0.9,en-US;q=0.8")
        .header(reqwest::header::COOKIE, cookie_header_value(cookie)?)
        .send()
        .await
        .map_err(|e| format!("Could not reach Facebook: {}", e))?;
    let final_url = resp.url().to_string();
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Could not read the reply from Facebook: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "Facebook returned HTTP {} — the session may have expired, sign in again.",
            status.as_u16()
        ));
    }
    if fb_is_login_wall(&final_url, &text) {
        return Err(
            "Facebook requires signing in again (session expired or under check) — sign in in the app (Cài đặt → Cookies → Đăng nhập trong app)."
                .to_string(),
        );
    }
    Ok((final_url, text))
}

async fn browse_fb_friends(session: &BrowseSession) -> Result<Vec<BrowsePerson>, String> {
    let uid = session
        .cookies
        .get("c_user")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if uid.is_empty() {
        return Err("Missing c_user cookie — sign in to Facebook again.".to_string());
    }
    let client = api_client()?;
    let base = "https://mbasic.facebook.com";
    let mut url = format!("{}/profile.php?v=friends&id={}", base, uid);
    let mut people: Vec<BrowsePerson> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..MAX_FB_PAGES {
        let (_, html) = fb_fetch(&client, &url, &session.cookie_header).await?;
        for p in parse_fb_friends_html(&html, &uid) {
            if seen.insert(p.id.clone()) {
                people.push(p);
            }
            if people.len() >= MAX_FRIENDS {
                break;
            }
        }
        if people.len() >= MAX_FRIENDS {
            break;
        }
        match fb_friends_next_link(&html) {
            Some(next) => url = fb_resolve(base, &next),
            None => break,
        }
    }
    Ok(people)
}

async fn browse_fb_posts(session: &BrowseSession, person_id: &str) -> Result<Vec<BrowsePost>, String> {
    let client = api_client()?;
    let person_id = person_id.trim();
    if person_id.is_empty() {
        return Err("Missing Facebook user ID.".to_string());
    }
    let url = if person_id.chars().all(|c| c.is_ascii_digit()) {
        format!("https://mbasic.facebook.com/profile.php?id={}", person_id)
    } else {
        format!(
            "https://mbasic.facebook.com/{}",
            person_id.trim_matches('/')
        )
    };
    let (_, html) = fb_fetch(&client, &url, &session.cookie_header).await?;
    let mut posts = parse_fb_posts_html(&html);
    posts.truncate(MAX_POSTS);
    Ok(posts)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[command]
pub async fn social_media_browse_friends(platform: String) -> Result<Vec<BrowsePerson>, String> {
    let platform = platform.to_lowercase();
    let session = browse_session(&platform)?;
    match platform.as_str() {
        "instagram" => browse_ig_friends(&session).await,
        "facebook" => browse_fb_friends(&session).await,
        _ => Err(format!("Unsupported browse platform: {}", platform)),
    }
}

#[command]
pub async fn social_media_browse_posts(
    platform: String,
    person_id: String,
) -> Result<Vec<BrowsePost>, String> {
    let platform = platform.to_lowercase();
    let session = browse_session(&platform)?;
    match platform.as_str() {
        "instagram" => browse_ig_posts(&session, &person_id).await,
        "facebook" => browse_fb_posts(&session, &person_id).await,
        _ => Err(format!("Unsupported browse platform: {}", platform)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IG_PEOPLE_JSON: &str = r#"{
        "users": [
            {"pk": 111, "username": "anna.ng", "full_name": "Anna Nguyễn", "profile_pic_url": "https://cdninstagram.com/a.jpg", "is_verified": false},
            {"pk": "222", "username": "bobtran", "full_name": "", "profile_pic_url": null},
            {"username": "noid", "full_name": "No Id"}
        ],
        "next_max_id": null,
        "status": "ok"
    }"#;

    const IG_FEED_JSON: &str = r#"{
        "items": [
            {"id": "333_444", "code": "AbC123", "taken_at": 1764050000, "media_type": 1,
             "caption": {"text": "Chào buổi sáng ☕"}, "user": {"pk": 111},
             "image_versions2": {"candidates": [{"url": "https://cdninstagram.com/p1.jpg", "width": 1080, "height": 1080}]}},
            {"id": "555_666", "code": "XyZ789", "taken_at": 1764040000, "media_type": 2,
             "caption": null,
             "image_versions2": {"candidates": [{"url": "https://cdninstagram.com/p2.jpg"}]}},
            {"id": "777", "taken_at": 1764030000, "media_type": 8}
        ],
        "status": "ok"
    }"#;

    const FB_FRIENDS_HTML: &str = r#"<html><body>
        <div><a href="/profile.php?id=10001&amp;refid=1"><img src="https://fbcdn.net/a1.jpg" />Minh Anh</a></div>
        <div><a href="/hung.nguyen?refid=2"><img src="https://fbcdn.net/a2.jpg" />Hưng Nguyễn</a></div>
        <div><a href="/profile.php?id=10001&amp;refid=1"><img src="https://fbcdn.net/a1.jpg" />Minh Anh</a></div>
        <div><a href="/login/?next=1">Log in</a></div>
        <div><a href="/story.php?story_fbid=9&amp;id=10001">A story, not a friend</a></div>
        <div><a href="/profile.php?v=friends&amp;id=999&amp;start=10">Xem thêm bạn bè</a></div>
    </body></html>"#;

    const FB_TIMELINE_HTML: &str = r#"<html><body>
        <div><p>Hôm nay trời đẹp quá, đi chơi thôi!</p>
        <a href="/story.php?story_fbid=9001&amp;id=10001">2 giờ</a>
        <a href="/photo.php?fbid=55"><img src="https://fbcdn.net/post1.jpg" /></a></div>
        <div><p>Ảnh kỷ niệm</p>
        <a href="/permalink.php?story_fbid=9002&amp;id=10001">Hôm qua</a></div>
        <div><a href="/story.php?story_fbid=9001&amp;id=10001">2 giờ</a></div>
    </body></html>"#;

    #[test]
    fn ig_people_parses_pk_username_avatar_and_kind() {
        let value: serde_json::Value = serde_json::from_str(IG_PEOPLE_JSON).unwrap();
        let people = parse_ig_people(&value, "following");
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].id, "111");
        assert_eq!(people[0].name, "Anna Nguyễn");
        assert_eq!(people[0].username.as_deref(), Some("anna.ng"));
        assert_eq!(
            people[0].avatar_url.as_deref(),
            Some("https://cdninstagram.com/a.jpg")
        );
        assert_eq!(
            people[0].profile_url,
            "https://www.instagram.com/anna.ng/"
        );
        assert_eq!(people[0].kind.as_deref(), Some("following"));
        // Empty full_name falls back to the username.
        assert_eq!(people[1].name, "bobtran");
        assert!(people[1].avatar_url.is_none());
    }

    #[test]
    fn ig_posts_skip_items_without_code_and_keep_caption_and_thumb() {
        let value: serde_json::Value = serde_json::from_str(IG_FEED_JSON).unwrap();
        let posts = parse_ig_posts(&value);
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].id, "333_444");
        assert_eq!(posts[0].url, "https://www.instagram.com/p/AbC123/");
        assert_eq!(posts[0].caption.as_deref(), Some("Chào buổi sáng ☕"));
        assert_eq!(posts[0].taken_at, Some(1764050000));
        assert_eq!(posts[0].media_type, "photo");
        assert_eq!(
            posts[0].thumbnail_url.as_deref(),
            Some("https://cdninstagram.com/p1.jpg")
        );
        assert_eq!(posts[1].media_type, "video");
        assert!(posts[1].caption.is_none());
    }

    #[test]
    fn fb_friends_parses_ids_and_slugs_dedupes_and_skips_nav() {
        let people = parse_fb_friends_html(FB_FRIENDS_HTML, "999");
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].id, "10001");
        assert_eq!(people[0].name, "Minh Anh");
        assert_eq!(
            people[0].avatar_url.as_deref(),
            Some("https://fbcdn.net/a1.jpg")
        );
        assert_eq!(
            people[0].profile_url,
            "https://www.facebook.com/profile.php?id=10001"
        );
        assert_eq!(people[1].id, "hung.nguyen");
        assert_eq!(people[1].username.as_deref(), Some("hung.nguyen"));
        assert_eq!(people[1].kind, None);
    }

    #[test]
    fn fb_friends_next_link_finds_see_more() {
        let next = fb_friends_next_link(FB_FRIENDS_HTML);
        assert_eq!(
            next.as_deref(),
            Some("/profile.php?v=friends&id=999&start=10")
        );
    }

    #[test]
    fn fb_posts_parse_story_links_with_caption_and_thumb() {
        let posts = parse_fb_posts_html(FB_TIMELINE_HTML);
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].id, "9001");
        assert_eq!(
            posts[0].url,
            "https://www.facebook.com/permalink.php?story_fbid=9001&id=10001"
        );
        assert_eq!(
            posts[0].caption.as_deref(),
            Some("Hôm nay trời đẹp quá, đi chơi thôi!")
        );
        assert_eq!(
            posts[0].thumbnail_url.as_deref(),
            Some("https://fbcdn.net/post1.jpg")
        );
        assert_eq!(posts[1].id, "9002");
        assert_eq!(posts[1].caption.as_deref(), Some("Ảnh kỷ niệm"));
        assert!(posts[1].thumbnail_url.is_none());
    }

    #[test]
    fn fb_login_wall_detection() {
        assert!(fb_is_login_wall(
            "https://mbasic.facebook.com/login.php",
            "<html>whatever</html>"
        ));
        assert!(fb_is_login_wall(
            "https://mbasic.facebook.com/profile.php?id=1",
            "<form action=\"/login.php\"><input name=\"email\" /><input name=\"pass\" /></form>"
        ));
        assert!(!fb_is_login_wall(
            "https://mbasic.facebook.com/profile.php?v=friends&id=1",
            "<html><body>friends list</body></html>"
        ));
    }

    #[test]
    fn html_unescape_and_strip_tags() {
        assert_eq!(html_unescape("a &amp; b &#039;c&#039;"), "a & b 'c'");
        assert_eq!(strip_tags("<b>Hi</b> <i>there</i>"), "Hi there");
    }
}
