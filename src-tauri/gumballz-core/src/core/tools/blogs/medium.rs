//! Export the stories the **user themselves** published on Medium —
//! including the drafts, which only exist for the owning account.
//!
//! ## The two paths
//!
//! 1. **Public feed** `https://medium.com/feed/@<user>` — **200
//!    verified on 2026-09-09**. Returns the ~10 most recent published stories
//!    with the whole body in `<content:encoded>`, plus `pubDate`,
//!    `link` and the `<category>` (the tags). It is the path that works without
//!    a session, and the only one that could really be exercised.
//! 2. **Session** — `https://medium.com/me/stories/public?format=json` and
//!    `.../drafts?format=json` list what the account published and what it has
//!    in draft; `https://medium.com/p/<id>?format=json` brings the whole story
//!    in Medium's paragraph model. Every response comes prefixed
//!    with `])}while(1);</x>` (defence against JSON hijacking) and needs the
//!    trim before parsing — that is `blogs::strip_json_prefix`.
//!
//!    **This path could not be verified**: from here Medium's Cloudflare
//!    answers 403 to any request that does not come from a browser with a
//!    session (`/@user?format=json` and `/_/graphql` gave 403 on 2026-09-09,
//!    while the RSS feed gave 200). The code is written for it, the paragraph
//!    model's format is covered by a pure test, and the tool ships as
//!    `beta`. Without a session, or if the session fails, the tool falls back to the feed.
//!
//! A draft only comes out via session — by definition, it is content only the owning account
//! receives.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    clean_url, file_stem, html_to_markdown, localize_images, tidy_markdown, write_post, Fetcher,
    PostDoc, PostMeta,
};
use crate::core::tools::ProgressFn;

const ID: &str = "blog-medium";
const COOKIE_DOMAINS: &[&str] = &["medium.com"];

// ── Options and result ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Your Medium username, with or without `@`. Required on the feed path.
    pub user: String,
    pub dest: String,
    /// Include the drafts (only works with the session).
    pub drafts: bool,
    /// Story ceiling. 0 = everything the source returns.
    pub limit: usize,
    pub markdown: bool,
    pub html: bool,
    pub json: bool,
    pub images: bool,
    pub delay_ms: u64,
    pub max_requests: u32,
    pub account_slug: Option<String>,
    pub session_netscape: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            user: String::new(),
            dest: String::new(),
            drafts: true,
            limit: 0,
            markdown: true,
            html: false,
            json: false,
            images: false,
            delay_ms: 1200,
            max_requests: 200,
            account_slug: None,
            session_netscape: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub used_session: bool,
    /// "session" quando saiu da conta logada, "rss" quando saiu do feed.
    pub source: String,
    pub user: String,
    pub dest: String,
    pub posts: usize,
    pub drafts: usize,
    pub images: usize,
    pub requests: u32,
    pub files: Vec<String>,
    /// Note for the UI when the session did not work out and the tool fell back to the feed.
    pub note: Option<String>,
}

// ── URLs ───────────────────────────────────────────────────────────────

/// `@dhh`, `dhh`, `https://medium.com/@dhh` — all become `dhh`.
pub fn normalize_user(input: &str) -> String {
    let s = input.trim().trim_matches('/');
    let s = s.rsplit('/').next().unwrap_or(s);
    let s = s.split('?').next().unwrap_or(s);
    s.trim_start_matches('@').trim().to_string()
}

pub fn feed_url(user: &str) -> String {
    format!("https://medium.com/feed/@{}", normalize_user(user))
}

/// `kind` is "public" (published) or "drafts".
pub fn list_url(kind: &str) -> String {
    format!("https://medium.com/me/stories/{}?format=json", kind)
}

pub fn story_url(id: &str) -> String {
    format!("https://medium.com/p/{}?format=json", id)
}

// ── Feed RSS ───────────────────────────────────────────────────────────

static RE_ITEM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<item>(.*?)</item>").expect("RSS item regex"));
static RE_CDATA: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)^\s*<!\[CDATA\[(.*?)\]\]>\s*$").expect("CDATA regex"));

fn tag_regex(tag: &str) -> Regex {
    Regex::new(&format!(
        r"(?s)<{0}(?:\s[^>]*)?>(.*?)</{0}>",
        regex::escape(tag)
    ))
    .unwrap_or_else(|_| RE_ITEM.clone())
}

fn field(block: &str, tag: &str) -> String {
    tag_regex(tag)
        .captures(block)
        .map(|c| unwrap_cdata(&c[1]))
        .unwrap_or_default()
}

fn fields(block: &str, tag: &str) -> Vec<String> {
    tag_regex(tag)
        .captures_iter(block)
        .map(|c| unwrap_cdata(&c[1]))
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn unwrap_cdata(raw: &str) -> String {
    match RE_CDATA.captures(raw) {
        Some(c) => c[1].to_string(),
        None => decode_entities(raw.trim()),
    }
}

/// The five entities that appear in RSS titles and categories, plus the
/// numeric ones. This is not an HTML parser: it is the minimum for the text
/// to come out clean.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail[..tail.len().min(12)].find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..end];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent
                .strip_prefix('#')
                .and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse::<u32>().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A story from the feed.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub date: String,
    pub author: String,
    pub tags: Vec<String>,
    pub html: String,
}

/// Reads Medium's RSS. The channel's `<item>` and the header's `<image>` do
/// not get in the way: only what is inside `<item>` counts.
pub fn parse_feed(xml: &str) -> Vec<FeedItem> {
    RE_ITEM
        .captures_iter(xml)
        .map(|c| {
            let block = &c[1];
            FeedItem {
                title: field(block, "title"),
                link: clean_url(&field(block, "link")),
                date: field(block, "pubDate"),
                author: field(block, "dc:creator"),
                tags: fields(block, "category"),
                html: field(block, "content:encoded"),
            }
        })
        .filter(|i| !i.title.trim().is_empty() || !i.html.trim().is_empty())
        .collect()
}

/// The slug of a Medium story URL: `.../post-title-abc123`.
pub fn slug_of_link(link: &str) -> String {
    let path = link.split('?').next().unwrap_or(link);
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Converts the RSS date (RFC 2822) to ISO 8601. If it fails, returns what
/// came in — front-matter with the original date is better than no date.
pub fn iso_from_rfc2822(date: &str) -> String {
    match chrono::DateTime::parse_from_rfc2822(date.trim()) {
        Ok(d) => d.to_utc().to_rfc3339(),
        Err(_) => date.trim().to_string(),
    }
}

pub fn doc_from_feed(item: &FeedItem, publication: &str) -> PostDoc {
    let meta = PostMeta {
        title: item.title.clone(),
        subtitle: None,
        author: item.author.clone(),
        publication: publication.to_string(),
        date: iso_from_rfc2822(&item.date),
        url: item.link.clone(),
        tags: item.tags.clone(),
        audience: None,
        kind: "post".into(),
        words: None,
    };
    PostDoc {
        slug: slug_of_link(&item.link),
        markdown: html_to_markdown(&item.html),
        html: Some(item.html.clone()),
        locked: item.html.trim().is_empty(),
        meta,
    }
}

// ── Paragraph model (session) ──────────────────────────────────────────

/// Reference to a story in the account's listing.
#[derive(Debug, Clone, PartialEq)]
pub struct StoryRef {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub draft: bool,
}

fn as_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Reads `payload.value` from `/me/stories/<kind>?format=json`.
pub fn parse_story_list(v: &Value, draft: bool) -> Vec<StoryRef> {
    let items = v
        .pointer("/payload/value")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|s| {
            let id = as_str(s, "id");
            if id.is_empty() {
                return None;
            }
            Some(StoryRef {
                title: as_str(s, "title"),
                slug: as_str(s, "uniqueSlug"),
                draft,
                id,
            })
        })
        .collect()
}

fn ms_to_iso(ms: i64) -> String {
    match chrono::DateTime::from_timestamp_millis(ms) {
        Some(d) => d.to_rfc3339(),
        None => String::new(),
    }
}

/// A Medium markup (bold, italic, link, code) arrives with `start` and `end`
/// in UTF-16 units, the way JavaScript counts. Here we convert to a character
/// index before touching the text — without that, an emoji in the middle of
/// the paragraph shifts everything after it.
fn utf16_to_char_index(text: &str, target: usize) -> usize {
    let mut units = 0usize;
    for (i, ch) in text.chars().enumerate() {
        if units >= target {
            return i;
        }
        units += ch.len_utf16();
    }
    text.chars().count()
}

fn markup_kind(m: &Value) -> String {
    match m.get("type") {
        Some(Value::String(s)) => s.to_uppercase(),
        Some(Value::Number(n)) => match n.as_i64().unwrap_or(0) {
            1 => "STRONG".into(),
            2 => "EM".into(),
            3 => "A".into(),
            10 => "CODE".into(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Applies a paragraph's markups to the plain text.
pub fn apply_markups(text: &str, markups: &[Value]) -> String {
    // (position in characters, 0 = close before opening, text to insert)
    let mut inserts: Vec<(usize, u8, String)> = Vec::new();
    for m in markups {
        let kind = markup_kind(m);
        let start = m.get("start").and_then(Value::as_u64).unwrap_or(0) as usize;
        let end = m.get("end").and_then(Value::as_u64).unwrap_or(0) as usize;
        if end <= start {
            continue;
        }
        let s = utf16_to_char_index(text, start);
        let e = utf16_to_char_index(text, end);
        let (open, close) = match kind.as_str() {
            "STRONG" => ("**".to_string(), "**".to_string()),
            "EM" => ("*".to_string(), "*".to_string()),
            "CODE" => ("`".to_string(), "`".to_string()),
            "A" => {
                let href = match as_str(m, "href") {
                    h if !h.is_empty() => clean_url(&h),
                    _ => {
                        let user = as_str(m, "userId");
                        if user.is_empty() {
                            continue;
                        }
                        format!("https://medium.com/u/{}", user)
                    }
                };
                ("[".to_string(), format!("]({})", href))
            }
            _ => continue,
        };
        inserts.push((s, 1, open));
        inserts.push((e, 0, close));
    }
    if inserts.is_empty() {
        return text.to_string();
    }
    inserts.sort_by_key(|(pos, order, _)| (*pos, *order));
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut next = 0usize;
    for (i, ch) in chars.iter().enumerate() {
        while next < inserts.len() && inserts[next].0 == i {
            out.push_str(&inserts[next].2);
            next += 1;
        }
        out.push(*ch);
    }
    while next < inserts.len() {
        out.push_str(&inserts[next].2);
        next += 1;
    }
    out
}

fn image_url(id: &str) -> String {
    format!("https://miro.medium.com/v2/resize:fit:2000/{}", id)
}

/// Medium's paragraph model becoming Markdown. The types are the API's own:
/// 1 paragraph, 2/3/13 headings, 4 image, 6/7 quote, 8 code, 9/10 list,
/// 11 embed, 14 link card, 15 caption.
pub fn paragraphs_to_markdown(paragraphs: &[Value]) -> String {
    // Each block carries whether it is a list item: an item glued to an item
    // becomes a real list; the rest stays separated by a blank line.
    let mut blocks: Vec<(String, bool)> = Vec::new();
    let mut ordinal = 0usize;
    let mut fence: Vec<String> = Vec::new();
    for p in paragraphs {
        let kind = p.get("type").and_then(Value::as_i64).unwrap_or(1);
        let raw = as_str(p, "text");
        let markups = p
            .get("markups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let text = apply_markups(&raw, &markups);

        if kind != 10 {
            ordinal = 0;
        }
        if kind != 8 && !fence.is_empty() {
            blocks.push((format!("```\n{}\n```", fence.join("\n")), false));
            fence.clear();
        }

        match kind {
            2 => blocks.push((format!("## {}", text.trim()), false)),
            3 => blocks.push((format!("### {}", text.trim()), false)),
            13 => blocks.push((format!("#### {}", text.trim()), false)),
            4 => {
                let id = p
                    .pointer("/metadata/id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                blocks.push((format!("![]({})", image_url(id)), false));
                if !raw.trim().is_empty() {
                    blocks.push((format!("*{}*", text.trim()), false));
                }
            }
            6 | 7 => {
                let quoted: Vec<String> = text
                    .trim()
                    .lines()
                    .map(|l| format!("> {}", l.trim()))
                    .collect();
                blocks.push((quoted.join("\n"), false));
            }
            8 => fence.push(raw.clone()),
            9 => blocks.push((format!("- {}", text.trim()), true)),
            10 => {
                ordinal += 1;
                blocks.push((format!("{}. {}", ordinal, text.trim()), true));
            }
            11 => {
                let href = p
                    .pointer("/iframe/mediaResource/href")
                    .and_then(Value::as_str)
                    .map(clean_url)
                    .or_else(|| {
                        p.pointer("/iframe/mediaResourceId")
                            .and_then(Value::as_str)
                            .map(|id| format!("https://medium.com/media/{}", id))
                    });
                if let Some(href) = href {
                    blocks.push((format!("[embed]({})", href), false));
                }
            }
            14 => {
                let href = p
                    .pointer("/mixtapeMetadata/href")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let label = if raw.trim().is_empty() {
                    href.to_string()
                } else {
                    raw.trim().to_string()
                };
                if !href.is_empty() {
                    blocks.push((format!("[{}]({})", label, clean_url(href)), false));
                }
            }
            15 => {
                if !raw.trim().is_empty() {
                    blocks.push((format!("*{}*", text.trim()), false));
                }
            }
            _ => {
                if !raw.trim().is_empty() {
                    blocks.push((text.trim().to_string(), false));
                }
            }
        }
    }
    if !fence.is_empty() {
        blocks.push((format!("```\n{}\n```", fence.join("\n")), false));
    }

    let mut md = String::new();
    let mut previous_list = false;
    for (i, (text, is_list)) in blocks.iter().enumerate() {
        if i > 0 {
            md.push_str(if *is_list && previous_list {
                "\n"
            } else {
                "\n\n"
            });
        }
        md.push_str(text);
        previous_list = *is_list;
    }
    tidy_markdown(&md)
}

/// Monta o documento a partir de `/p/<id>?format=json`.
pub fn parse_story(v: &Value, draft: bool) -> PostDoc {
    let value = v.pointer("/payload/value").unwrap_or(v);
    let paragraphs = value
        .pointer("/content/bodyModel/paragraphs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let subtitle = value
        .pointer("/content/subtitle")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tags = value
        .pointer("/virtuals/tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|t| {
                    let n = as_str(t, "name");
                    if n.is_empty() {
                        as_str(t, "slug")
                    } else {
                        n
                    }
                })
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let ms = value
        .get("firstPublishedAt")
        .and_then(Value::as_i64)
        .or_else(|| value.get("latestPublishedAt").and_then(Value::as_i64))
        .or_else(|| value.get("createdAt").and_then(Value::as_i64))
        .unwrap_or(0);
    let slug = as_str(value, "uniqueSlug");
    let author = value
        .pointer("/creator/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let words = value.pointer("/virtuals/wordCount").and_then(Value::as_u64);
    let markdown = paragraphs_to_markdown(&paragraphs);
    // The model's first paragraph is the title itself; without it the body
    // starts straight into the text, which is what the front-matter already covers.
    let markdown = strip_leading_title(&markdown, &as_str(value, "title"));
    let meta = PostMeta {
        title: as_str(value, "title"),
        subtitle: if subtitle.trim().is_empty() {
            None
        } else {
            Some(subtitle)
        },
        author,
        publication: "Medium".into(),
        date: if ms > 0 { ms_to_iso(ms) } else { String::new() },
        url: if slug.is_empty() {
            String::new()
        } else {
            format!("https://medium.com/@/{}", slug)
        },
        tags,
        audience: None,
        kind: if draft { "draft".into() } else { "post".into() },
        words,
    };
    PostDoc {
        slug,
        markdown,
        html: None,
        locked: paragraphs.is_empty(),
        meta,
    }
}

/// Strips the opening `## Title` when it repeats the post's title.
pub fn strip_leading_title(md: &str, title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        return md.to_string();
    }
    for prefix in ["## ", "# ", "### "] {
        let head = format!("{}{}", prefix, title);
        if let Some(rest) = md.strip_prefix(&head) {
            return rest.trim_start().to_string();
        }
    }
    md.to_string()
}

// ── Execution ──────────────────────────────────────────────────────────

pub async fn run(opts: &Options, progress: ProgressFn) -> Result<ExportResult> {
    let dest = opts.dest.trim();
    if dest.is_empty() {
        return Err(anyhow!("pick the destination folder"));
    }
    if !opts.markdown && !opts.html && !opts.json {
        return Err(anyhow!("pick at least one format"));
    }
    let user = normalize_user(&opts.user);
    let root = PathBuf::from(dest);
    std::fs::create_dir_all(&root)?;

    let domains: Vec<String> = COOKIE_DOMAINS.iter().map(|d| d.to_string()).collect();
    let fetcher = Fetcher::new(opts.delay_ms, opts.session_netscape.as_deref(), &domains)?;

    let mut note: Option<String> = None;
    let mut source = "rss";
    let mut docs: Vec<PostDoc> = Vec::new();

    if fetcher.has_session() {
        match from_session(&fetcher, opts, &progress).await {
            Ok(d) => {
                source = "session";
                docs = d;
            }
            Err(e) => {
                if user.is_empty() {
                    return Err(e);
                }
                note = Some(format!(
                    "the Medium session did not respond ({}); out came what the public feed delivers",
                    e
                ));
            }
        }
    }

    if docs.is_empty() {
        if user.is_empty() {
            return Err(anyhow!(
                "provide your Medium username (or capture the medium.com cookies in the manager to include drafts)"
            ));
        }
        crate::core::tools::report(&progress, ID, "feed", 0, None, Some(user.clone()));
        let xml = fetcher.get_text(&feed_url(&user)).await?;
        let items = parse_feed(&xml);
        if items.is_empty() {
            return Err(anyhow!(
                "@{}'s feed brought no published story",
                user
            ));
        }
        for it in items {
            docs.push(doc_from_feed(&it, "Medium"));
        }
    }

    if opts.limit > 0 && docs.len() > opts.limit {
        docs.truncate(opts.limit);
    }

    let mut out = ExportResult {
        used_session: fetcher.has_session(),
        source: source.to_string(),
        user: user.clone(),
        dest: root.to_string_lossy().to_string(),
        posts: 0,
        drafts: 0,
        images: 0,
        requests: 0,
        files: Vec::new(),
        note,
    };

    let total = docs.len() as u64;
    for (i, doc) in docs.iter_mut().enumerate() {
        crate::core::tools::report(
            &progress,
            ID,
            "progress",
            i as u64,
            Some(total),
            Some(doc.meta.title.clone()),
        );
        if doc.locked {
            continue;
        }
        if doc.meta.author.trim().is_empty() && !user.is_empty() {
            doc.meta.author = format!("@{}", user);
        }
        let dir = if doc.meta.kind == "draft" {
            root.join("rascunhos")
        } else {
            root.clone()
        };
        let stem = file_stem(&doc.meta.date, &doc.meta.title, &doc.slug);
        if opts.images {
            let rel = format!("{}-imagens", stem);
            match localize_images(&doc.markdown, &fetcher, &dir, &rel, &progress, ID).await {
                Ok((md, n)) => {
                    doc.markdown = md;
                    out.images += n;
                }
                Err(e) => tracing::warn!("medium: images of {} failed: {}", doc.slug, e),
            }
        }
        let files = write_post(&dir, &stem, doc, opts.markdown, opts.html, opts.json)?;
        for f in files {
            out.files.push(f.to_string_lossy().to_string());
        }
        if doc.meta.kind == "draft" {
            out.drafts += 1;
        } else {
            out.posts += 1;
        }
    }

    out.requests = fetcher.requests();
    crate::core::tools::report(
        &progress,
        ID,
        "done",
        (out.posts + out.drafts) as u64,
        Some(total),
        None,
    );
    Ok(out)
}

async fn from_session(
    fetcher: &Fetcher,
    opts: &Options,
    progress: &ProgressFn,
) -> Result<Vec<PostDoc>> {
    let mut refs: Vec<StoryRef> = Vec::new();
    crate::core::tools::report(&progress.clone(), ID, "list", 0, None, None);
    let public = fetcher.get_json(&list_url("public")).await?;
    refs.extend(parse_story_list(&public, false));
    if opts.drafts {
        match fetcher.get_json(&list_url("drafts")).await {
            Ok(v) => refs.extend(parse_story_list(&v, true)),
            Err(e) => tracing::warn!("medium: drafts did not come: {}", e),
        }
    }
    if refs.is_empty() {
        return Err(anyhow!(
            "the Medium session listed none of your stories (the session may have expired)"
        ));
    }
    if opts.limit > 0 && refs.len() > opts.limit {
        refs.truncate(opts.limit);
    }

    let mut docs = Vec::new();
    let total = refs.len() as u64;
    for (i, r) in refs.iter().enumerate() {
        if fetcher.requests() >= opts.max_requests {
            break;
        }
        crate::core::tools::report(
            progress,
            ID,
            "story",
            i as u64,
            Some(total),
            Some(r.title.clone()),
        );
        match fetcher.get_json(&story_url(&r.id)).await {
            Ok(v) => {
                let mut doc = parse_story(&v, r.draft);
                if doc.meta.title.trim().is_empty() {
                    doc.meta.title.clone_from(&r.title);
                }
                if doc.slug.is_empty() {
                    doc.slug.clone_from(&r.slug);
                }
                docs.push(doc);
            }
            Err(e) => tracing::warn!("medium: story {} failed: {}", r.id, e),
        }
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> Value {
        serde_json::from_str(s).expect("sample json")
    }

    #[test]
    fn usuario_e_urls() {
        assert_eq!(normalize_user("@dhh"), "dhh");
        assert_eq!(normalize_user("https://medium.com/@dhh"), "dhh");
        assert_eq!(normalize_user("  dhh  "), "dhh");
        assert_eq!(feed_url("@dhh"), "https://medium.com/feed/@dhh");
        assert_eq!(
            list_url("drafts"),
            "https://medium.com/me/stories/drafts?format=json"
        );
        assert_eq!(
            story_url("abc123"),
            "https://medium.com/p/abc123?format=json"
        );
    }

    #[test]
    fn feed_do_medium_vira_documentos() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
<title><![CDATA[Stories by DHH on Medium]]></title>
<item>
  <title><![CDATA[Signal v Noise exits Medium]]></title>
  <link>https://medium.com/signal-v-noise/signal-v-noise-exits-medium-56c483d827fc?source=rss-54bcbf647830------2</link>
  <guid>https://medium.com/p/56c483d827fc</guid>
  <dc:creator><![CDATA[DHH]]></dc:creator>
  <category><![CDATA[medium]]></category>
  <category><![CDATA[publishing]]></category>
  <pubDate>Mon, 11 Feb 2019 15:51:56 GMT</pubDate>
  <content:encoded><![CDATA[<p>Three anos <a href="https://x.com/a?utm_source=rss">atr&aacute;s</a>.</p><figure><img src="https://cdn-images-1.medium.com/max/1024/1*abc.png" alt="capa"><figcaption>A capa</figcaption></figure>]]></content:encoded>
</item>
</channel></rss>"#;
        let items = parse_feed(xml);
        assert_eq!(items.len(), 1, "only what is inside <item>");
        let it = &items[0];
        assert_eq!(it.title, "Signal v Noise exits Medium");
        assert_eq!(it.tags, vec!["medium", "publishing"]);
        assert_eq!(it.author, "DHH");
        assert_eq!(
            it.link, "https://medium.com/signal-v-noise/signal-v-noise-exits-medium-56c483d827fc",
            "the RSS ?source= is tracking and goes out"
        );
        let doc = doc_from_feed(it, "Medium");
        assert_eq!(doc.slug, "signal-v-noise-exits-medium-56c483d827fc");
        assert_eq!(doc.meta.date, "2019-02-11T15:51:56+00:00");
        assert!(doc.markdown.contains("atrás"), "{}", doc.markdown);
        assert!(
            doc.markdown
                .contains("![capa](https://cdn-images-1.medium.com/max/1024/1*abc.png)"),
            "{}",
            doc.markdown
        );
        assert!(doc.markdown.contains("*A capa*"), "{}", doc.markdown);
        assert_eq!(
            file_stem(&doc.meta.date, &doc.meta.title, &doc.slug),
            "2019-02-11-signal-v-noise-exits-medium-56c483d827fc"
        );
    }

    #[test]
    fn entidades_do_rss_sao_decodificadas() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt;"), "a & b <c>");
        assert_eq!(decode_entities("aspas &quot;x&quot;"), "aspas \"x\"");
        assert_eq!(decode_entities("caf&#233; &#x2014; ok"), "café — ok");
        assert_eq!(decode_entities("cifr&o sozinho"), "cifr&o sozinho");
    }

    #[test]
    fn marcadores_contam_em_utf16_como_o_javascript() {
        // "🙂 ab": the emoji takes 2 UTF-16 units, so "ab" starts at 3.
        let text = "🙂 ab";
        let markups = vec![json(r#"{"type":"STRONG","start":3,"end":5}"#)];
        assert_eq!(apply_markups(text, &markups), "🙂 **ab**");
    }

    #[test]
    fn negrito_italico_codigo_e_link_saem_do_modelo() {
        let text = "um dois tres quatro";
        let markups = vec![
            json(r#"{"type":1,"start":0,"end":2}"#),
            json(r#"{"type":2,"start":3,"end":7}"#),
            json(r#"{"type":10,"start":8,"end":12}"#),
            json(r#"{"type":3,"start":13,"end":19,"href":"https://x.com/a?utm_source=medium"}"#),
        ];
        assert_eq!(
            apply_markups(text, &markups),
            "**um** *dois* `tres` [quatro](https://x.com/a)"
        );
    }

    #[test]
    fn link_de_usuario_sem_href_vira_perfil() {
        let markups = vec![json(
            r#"{"type":"A","start":0,"end":3,"anchorType":"USER","userId":"abc"}"#,
        )];
        assert_eq!(
            apply_markups("DHH escreveu", &markups),
            "[DHH](https://medium.com/u/abc) escreveu"
        );
    }

    #[test]
    fn modelo_de_paragrafos_vira_markdown() {
        let paragraphs: Vec<Value> = serde_json::from_str(
            r#"[
              {"type":3,"text":"Título grande","markups":[]},
              {"type":1,"text":"Um parágrafo com negrito","markups":[{"type":1,"start":17,"end":24}]},
              {"type":4,"text":"A legenda","metadata":{"id":"1*abc.png"},"markups":[]},
              {"type":6,"text":"Citado","markups":[]},
              {"type":8,"text":"fn main() {","markups":[]},
              {"type":8,"text":"}","markups":[]},
              {"type":9,"text":"um","markups":[]},
              {"type":9,"text":"dois","markups":[]},
              {"type":10,"text":"primeiro","markups":[]},
              {"type":10,"text":"segundo","markups":[]},
              {"type":14,"text":"Um cartão","mixtapeMetadata":{"href":"https://x.com/c"},"markups":[]},
              {"type":11,"iframe":{"mediaResourceId":"med123"},"markups":[]},
              {"type":15,"text":"Legenda do embed","markups":[]}
            ]"#,
        )
        .expect("amostra");
        let md = paragraphs_to_markdown(&paragraphs);
        assert!(md.contains("### Título grande"), "{}", md);
        assert!(md.contains("com **negrito**"), "{}", md);
        assert!(
            md.contains("![](https://miro.medium.com/v2/resize:fit:2000/1*abc.png)"),
            "{}",
            md
        );
        assert!(md.contains("*A legenda*"), "{}", md);
        assert!(md.contains("> Citado"), "{}", md);
        assert!(md.contains("```\nfn main() {\n}\n```"), "{}", md);
        assert!(md.contains("- um\n- dois"), "{}", md);
        assert!(md.contains("1. primeiro\n2. segundo"), "{}", md);
        assert!(md.contains("[Um cartão](https://x.com/c)"), "{}", md);
        assert!(
            md.contains("[embed](https://medium.com/media/med123)"),
            "{}",
            md
        );
        assert!(md.contains("*Legenda do embed*"), "{}", md);
        assert!(md.ends_with('\n'));
    }

    #[test]
    fn listagem_e_historia_da_sessao() {
        let list = json(
            r#"{"payload":{"value":[
                 {"id":"abc123","title":"Meu post","uniqueSlug":"meu-post-abc123"},
                 {"title":"sem id"}
               ]}}"#,
        );
        let refs = parse_story_list(&list, true);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, "abc123");
        assert!(refs[0].draft);

        let story = json(
            r#"{"payload":{"value":{
                 "id":"abc123","title":"Meu post","uniqueSlug":"meu-post-abc123",
                 "firstPublishedAt":1549900316000,
                 "creator":{"name":"DHH"},
                 "virtuals":{"tags":[{"name":"Rust","slug":"rust"}],"wordCount":420},
                 "content":{"subtitle":"O sub","bodyModel":{"paragraphs":[
                    {"type":3,"text":"Meu post","markups":[]},
                    {"type":1,"text":"corpo","markups":[]}
                 ]}}
               }}}"#,
        );
        let doc = parse_story(&story, false);
        assert!(!doc.locked);
        assert_eq!(doc.meta.kind, "post");
        assert_eq!(doc.meta.author, "DHH");
        assert_eq!(doc.meta.tags, vec!["Rust"]);
        assert_eq!(doc.meta.words, Some(420));
        assert_eq!(doc.meta.subtitle.as_deref(), Some("O sub"));
        assert!(doc.meta.date.starts_with("2019-02-11"), "{}", doc.meta.date);
        assert_eq!(doc.markdown.trim(), "corpo", "the title does not repeat in the body");
    }

    #[test]
    fn historia_vazia_e_bloqueada() {
        let doc = parse_story(&json(r#"{"payload":{"value":{"title":"x"}}}"#), true);
        assert!(doc.locked);
        assert_eq!(doc.meta.kind, "draft");
    }

    #[test]
    fn titulo_repetido_no_topo_some() {
        assert_eq!(strip_leading_title("## Oi\n\ncorpo", "Oi"), "corpo");
        assert_eq!(strip_leading_title("# Oi\n\ncorpo", "Oi"), "corpo");
        assert_eq!(strip_leading_title("corpo", "Oi"), "corpo");
        assert_eq!(
            strip_leading_title("## Outro\n\ncorpo", "Oi"),
            "## Outro\n\ncorpo"
        );
    }

    #[tokio::test]
    #[ignore = "rede: le o feed publico de um perfil real do Medium"]
    async fn rede_feed_publico_responde() {
        let f = Fetcher::new(1200, None, &["medium.com".to_string()]).expect("cliente");
        let xml = f.get_text(&feed_url("dhh")).await.expect("feed");
        let items = parse_feed(&xml);
        assert!(!items.is_empty());
        let doc = doc_from_feed(&items[0], "Medium");
        assert!(!doc.markdown.trim().is_empty());
    }

    #[tokio::test]
    #[ignore = "rede: exporta de ponta a ponta o feed publico de um perfil para uma pasta temporaria"]
    async fn rede_exporta_feed_de_ponta_a_ponta() {
        let dir = crate::core::tools::temp_dir().join("teste-medium");
        let _ = std::fs::remove_dir_all(&dir);
        let opts = Options {
            user: "@dhh".into(),
            dest: dir.to_string_lossy().to_string(),
            limit: 3,
            markdown: true,
            html: true,
            ..Default::default()
        };
        let out = run(&opts, crate::core::tools::noop_progress())
            .await
            .expect("exportacao");
        assert_eq!(out.source, "rss");
        assert_eq!(out.posts, 3, "{:?}", out.files);
        let md = std::fs::read_to_string(
            out.files
                .iter()
                .find(|f| f.ends_with(".md"))
                .expect("a .md"),
        )
        .expect("read the markdown");
        assert!(md.starts_with("---\ntitle: \""), "{}", &md[..80]);
        assert!(md.len() > 500);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    #[ignore = "sessao: lista as historias e rascunhos da conta logada (precisa dos cookies de medium.com)"]
    async fn sessao_lista_historias_e_rascunhos() {
        let path = std::env::var("GUMBALLZ_MEDIUM_COOKIES").expect("GUMBALLZ_MEDIUM_COOKIES");
        let content = std::fs::read_to_string(path).expect("netscape file");
        let f = Fetcher::new(1200, Some(&content), &["medium.com".to_string()]).expect("cliente");
        assert!(f.has_session(), "nenhum cookie de medium.com no arquivo");
        let v = f.get_json(&list_url("public")).await.expect("listagem");
        assert!(!parse_story_list(&v, false).is_empty());
    }
}
