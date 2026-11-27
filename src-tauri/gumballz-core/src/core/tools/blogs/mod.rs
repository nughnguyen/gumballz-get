//! Shared layer for the blog exports (Substack, Medium).
//!
//! Three things live here, because every tool in the category needs all three:
//!
//! 1. **A throttled client carrying the user's session.** Same design as the
//!    Reddit `Fetcher`: one request at a time, a configurable wait between
//!    them, growing backoff on 429, and a cookie jar seeded from the Netscape
//!    file the manager writes — filtered by domain, so a cookie from one site
//!    never leaks to another.
//! 2. **HTML → Markdown with the project's taste.** `htmd` does the rough
//!    work; here we decide the rest, in the same spirit as
//!    `pdf_markdown.rs`: ATX for headings, `-` for lists, backticks for
//!    code, the image preserved (with the original URL, without the CDN's
//!    resizer), caption in italics right below it, and no signature widget,
//!    share button or tracking parameter in the link.
//! 3. **Saving the post.** YAML front-matter, a safe and predictable
//!    filename (`YYYY-MM-DD-slug`), and optional download of the images into a
//!    folder next to it with the link rewritten to a relative path.
//!
//! Limit the tools in this category respect: only what the logged-in account
//! legitimately receives or owns leaves here. A third-party paid post the
//! session cannot open is counted as skipped, never worked around.

pub mod medium;
pub mod substack;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use htmd::options::{
    BrStyle, BulletListMarker, CodeBlockFence, CodeBlockStyle, HeadingStyle, HrStyle,
    LinkReferenceStyle, LinkStyle, Options as MdOptions, TranslationMode,
};
use htmd::{element_handler::HandlerResult, Element, HtmlToMarkdown};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

use super::ProgressFn;

// ── Client ─────────────────────────────────────────────────────────────

/// A domain from the Netscape file matches the target when it is exactly it or
/// a subdomain of it. `substack.com` matches `on.substack.com`, but
/// `notsubstack.com` matches nothing.
pub fn domain_matches(domain: &str, allowed: &[String]) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    allowed.iter().any(|a| {
        let a = a.trim_start_matches('.').to_lowercase();
        d == a || d.ends_with(&format!(".{}", a))
    })
}

/// Builds the client's cookie jar from the manager's Netscape file, letting in
/// only what belongs to the requested domains. Returns the jar and how many
/// cookies got in.
pub fn jar_from_netscape(content: &str, allowed: &[String]) -> (Arc<reqwest::cookie::Jar>, usize) {
    let jar = reqwest::cookie::Jar::default();
    let mut n = 0;
    for c in crate::core::tools::instagram::parse_netscape(content) {
        if !domain_matches(&c.domain, allowed) {
            continue;
        }
        let domain = c.domain.trim_start_matches('.');
        let path = if c.path.is_empty() { "/" } else { &c.path };
        let scheme = if c.secure { "https" } else { "http" };
        let Ok(url) = format!("{}://{}{}", scheme, domain, path).parse::<reqwest::Url>() else {
            continue;
        };
        jar.add_cookie_str(
            &format!("{}={}; Domain={}; Path={}", c.name, c.value, c.domain, path),
            &url,
        );
        n += 1;
    }
    (Arc::new(jar), n)
}

fn cookie_client(session: Option<&str>, allowed: &[String]) -> Result<(reqwest::Client, bool)> {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, USER_AGENT};
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9,pt-BR;q=0.8"),
    );
    let (jar, seeded) = match session {
        Some(content) => jar_from_netscape(content, allowed),
        None => (Arc::new(reqwest::cookie::Jar::default()), 0),
    };
    let client = crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
        .default_headers(headers)
        .cookie_provider(jar)
        .timeout(Duration::from_secs(120))
        .build()?;
    Ok((client, seeded > 0))
}

/// Patient client: one request at a time, with a wait between them. A newsletter
/// archive is bulk reading; hammering the server for free only earns a 429.
pub struct Fetcher {
    client: reqwest::Client,
    delay: Duration,
    last: tokio::sync::Mutex<Option<Instant>>,
    requests: AtomicU32,
    has_session: bool,
}

impl Fetcher {
    pub fn new(delay_ms: u64, session: Option<&str>, allowed: &[String]) -> Result<Self> {
        let (client, has_session) = cookie_client(session, allowed)?;
        Ok(Self {
            client,
            delay: Duration::from_millis(delay_ms.clamp(200, 10_000)),
            last: tokio::sync::Mutex::new(None),
            requests: AtomicU32::new(0),
            has_session,
        })
    }

    pub fn has_session(&self) -> bool {
        self.has_session
    }

    pub fn requests(&self) -> u32 {
        self.requests.load(Ordering::Relaxed)
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn pace(&self) {
        let mut last = self.last.lock().await;
        if let Some(t) = *last {
            let since = t.elapsed();
            if since < self.delay {
                tokio::time::sleep(self.delay - since).await;
            }
        }
        *last = Some(Instant::now());
    }

    /// GET with text back. Retries on 429 and on server errors.
    pub async fn get_text(&self, url: &str) -> Result<String> {
        const TRIES: u32 = 4;
        let mut wait = Duration::from_secs(3);
        for attempt in 1..=TRIES {
            self.pace().await;
            self.requests.fetch_add(1, Ordering::Relaxed);
            match self.client.get(url).send().await {
                Ok(r) if r.status().is_success() => return Ok(r.text().await?),
                Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => {
                    return Err(anyhow!(
                        "acesso negado (HTTP {}). Capture os cookies da sua conta no gerenciador e tente de novo",
                        r.status()
                    ));
                }
                Ok(r) if r.status().as_u16() == 404 => {
                    return Err(anyhow!("not found: {}", url));
                }
                Ok(r) if r.status().as_u16() == 429 || r.status().is_server_error() => {
                    if attempt == TRIES {
                        return Err(anyhow!(
                            "the server is limiting access (HTTP {}). Try again in a bit",
                            r.status()
                        ));
                    }
                    let retry = r
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(Duration::from_secs);
                    tokio::time::sleep(retry.unwrap_or(wait)).await;
                    wait *= 2;
                }
                Ok(r) => return Err(anyhow!("HTTP {} on {}", r.status(), url)),
                Err(e) if attempt < TRIES => {
                    tokio::time::sleep(wait).await;
                    wait *= 2;
                    let _ = e;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(anyhow!("could not read {}", url))
    }

    /// GET with JSON back. Tolerates Medium's anti-hijack prefix.
    pub async fn get_json(&self, url: &str) -> Result<serde_json::Value> {
        let text = self.get_text(url).await?;
        let body = strip_json_prefix(&text);
        serde_json::from_str(body)
            .map_err(|e| anyhow!("the server answered something that is not JSON ({}): {}", e, url))
    }
}

/// Medium serves JSON prefixed with `])}while(1);</x>` so nobody can include
/// the response as a `<script>`. It is junk before the first `{` or `[`;
/// trimming it is mandatory before parsing.
pub fn strip_json_prefix(text: &str) -> &str {
    let t = text.trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        return t;
    }
    match t.find(['{', '[']) {
        Some(i) => &t[i..],
        None => t,
    }
}

// ── HTML → Markdown ────────────────────────────────────────────────────

/// Blocks that are not the text: signature, button, share box, post footer.
/// Matched by a piece of `class`, which is how Substack and Medium name
/// these boxes.
const JUNK_CLASSES: &[&str] = &[
    "subscription-widget",
    "subscribe-widget",
    "subscribe-footer",
    "button-wrapper",
    "share-dialog",
    "post-ufi",
    "post-footer",
    "comments-page",
    "paywall",
    "pencraft-cta",
    "js-postMetaInline",
    "postActions",
    "recommendation",
];

fn attr<'a>(el: &'a Element<'a>, name: &str) -> Option<String> {
    el.attrs
        .iter()
        .find(|a| a.name.local.as_ref() == name)
        .map(|a| a.value.to_string())
}

fn is_junk(class: &str) -> bool {
    let c = class.to_lowercase();
    JUNK_CLASSES.iter().any(|j| c.contains(&j.to_lowercase()))
}

/// Substack serves images through a resizer that embeds the original URL at
/// the end of the path, escaped. `.../w_1456,c_limit,f_auto/https%3A%2F%2F…png`
/// becomes the real URL — it is the one we keep.
pub fn unwrap_cdn_image(url: &str) -> String {
    let marker = ["https%3A%2F%2F", "http%3A%2F%2F", "https%3a%2f%2f"];
    for m in marker {
        if let Some(i) = url.rfind(m) {
            let enc = &url[i..];
            let dec = enc
                .replace("%3A", ":")
                .replace("%3a", ":")
                .replace("%2F", "/")
                .replace("%2f", "/");
            return dec;
        }
    }
    url.to_string()
}

/// Parameters that only serve to count clicks. They leave the link and the image.
const TRACKING_KEYS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_id",
    "source",
    "ref",
    "referrer",
    "publication_id",
    "post_id",
    "isFreemail",
    "triedRedirect",
    "r",
    "showWelcomeOnShare",
    "gi",
    "sk",
];

/// Removes the tracking parameters from a URL, preserving the rest of the query.
pub fn clean_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let (query, frag) = match query.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (query, None),
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            let key = p.split_once('=').map(|(k, _)| k).unwrap_or(p);
            !TRACKING_KEYS.iter().any(|t| t.eq_ignore_ascii_case(key))
        })
        .collect();
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

/// The converter with the project's taste. It is expensive to build (it registers
/// handlers), so it lives in a `Lazy`.
static CONVERTER: Lazy<HtmlToMarkdown> = Lazy::new(build_converter);

fn build_converter() -> HtmlToMarkdown {
    let options = MdOptions {
        heading_style: HeadingStyle::Atx,
        hr_style: HrStyle::Dashes,
        br_style: BrStyle::TwoSpaces,
        link_style: LinkStyle::Inlined,
        link_reference_style: LinkReferenceStyle::Full,
        code_block_style: CodeBlockStyle::Fenced,
        code_block_fence: CodeBlockFence::Backticks,
        bullet_list_marker: BulletListMarker::Dash,
        ul_bullet_spacing: 1,
        ol_number_spacing: 1,
        preformatted_code: true,
        translation_mode: TranslationMode::Pure,
    };
    HtmlToMarkdown::builder()
        .options(options)
        .skip_tags(vec![
            "script", "style", "noscript", "form", "button", "svg", "nav", "input", "select",
        ])
        // Signature box, button and footer: out. The rest of the block follows
        // the normal htmd path.
        .add_handler(
            vec!["div", "section", "aside", "footer"],
            |handlers: &dyn htmd::element_handler::Handlers, el: Element| {
                if attr(&el, "class").is_some_and(|c| is_junk(&c)) {
                    return Some(HandlerResult::from(""));
                }
                handlers.fallback(el)
            },
        )
        // Image: original URL instead of the resizer, no tracking.
        .add_handler(
            vec!["img"],
            |_h: &dyn htmd::element_handler::Handlers, el: Element| {
                let src = attr(&el, "src")
                    .or_else(|| attr(&el, "data-src"))
                    .unwrap_or_default();
                if src.trim().is_empty() || src.starts_with("data:") {
                    return Some(HandlerResult::from(""));
                }
                let alt = attr(&el, "alt")
                    .unwrap_or_default()
                    .replace(['\n', '\r'], " ");
                let url = clean_url(&unwrap_cdn_image(src.trim()));
                Some(HandlerResult::from(format!("![{}]({})", alt.trim(), url)))
            },
        )
        // Figure caption in italics, on the line below the image.
        .add_handler(
            vec!["figcaption"],
            |handlers: &dyn htmd::element_handler::Handlers, el: Element| {
                let text = handlers.walk_children(el.node).content;
                let text = text.trim();
                if text.is_empty() {
                    return Some(HandlerResult::from(""));
                }
                Some(HandlerResult::from(format!("\n\n*{}*\n\n", text)))
            },
        )
        // Embed (video, tweet, player): becomes a link, which is what survives
        // in a text file.
        .add_handler(
            vec!["iframe", "video", "audio", "embed"],
            |_h: &dyn htmd::element_handler::Handlers, el: Element| {
                let src = attr(&el, "src").unwrap_or_default();
                if src.trim().is_empty() {
                    return Some(HandlerResult::from(""));
                }
                Some(HandlerResult::from(format!(
                    "\n\n[embed]({})\n\n",
                    clean_url(src.trim())
                )))
            },
        )
        .build()
}

/// Converts a post's HTML into the project's Markdown.
pub fn html_to_markdown(html: &str) -> String {
    let md = CONVERTER.convert(html).unwrap_or_default();
    tidy_markdown(&md)
}

static BLANKS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\n{3,}").expect("blank-lines regex"));
static TRAIL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)[ \t]+$").expect("trailing-space regex"));

/// Fixes the spacing: at most one blank line between blocks, no trailing
/// space at the end of lines, and the file ends with a `\n`.
pub fn tidy_markdown(md: &str) -> String {
    let md = TRAIL.replace_all(md, "");
    let md = BLANKS.replace_all(&md, "\n\n");
    format!("{}\n", md.trim())
}

// ── Front-matter and files ─────────────────────────────────────────────

/// Metadata of a post, the same for Substack and Medium. They become the
/// front-matter
/// YAML at the top of the `.md` and the JSON `meta`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PostMeta {
    pub title: String,
    pub subtitle: Option<String>,
    pub author: String,
    pub publication: String,
    /// ISO 8601, as the server delivers it.
    pub date: String,
    pub url: String,
    pub tags: Vec<String>,
    /// "everyone" | "only_paid" | "founding" on Substack; absent on Medium.
    pub audience: Option<String>,
    /// "post" | "draft".
    pub kind: String,
    pub words: Option<u64>,
}

fn yaml_scalar(s: &str) -> String {
    let clean = s.replace(['\n', '\r'], " ");
    format!("\"{}\"", clean.replace('\\', "\\\\").replace('"', "\\\""))
}

/// YAML front-matter of the post. An empty field does not become a line: a text
/// file with `subtitle: ""` only gets in the way of whoever reads it later.
pub fn front_matter(meta: &PostMeta) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", yaml_scalar(&meta.title)));
    if let Some(s) = meta.subtitle.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("subtitle: {}\n", yaml_scalar(s)));
    }
    if !meta.author.trim().is_empty() {
        out.push_str(&format!("author: {}\n", yaml_scalar(&meta.author)));
    }
    if !meta.publication.trim().is_empty() {
        out.push_str(&format!(
            "publication: {}\n",
            yaml_scalar(&meta.publication)
        ));
    }
    if !meta.date.trim().is_empty() {
        out.push_str(&format!("date: {}\n", yaml_scalar(&meta.date)));
    }
    if !meta.url.trim().is_empty() {
        out.push_str(&format!("url: {}\n", yaml_scalar(&meta.url)));
    }
    if !meta.tags.is_empty() {
        let tags: Vec<String> = meta.tags.iter().map(|t| yaml_scalar(t)).collect();
        out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
    }
    if let Some(a) = meta.audience.as_deref().filter(|a| !a.trim().is_empty()) {
        out.push_str(&format!("audience: {}\n", yaml_scalar(a)));
    }
    if !meta.kind.trim().is_empty() {
        out.push_str(&format!("kind: {}\n", yaml_scalar(&meta.kind)));
    }
    if let Some(w) = meta.words {
        out.push_str(&format!("words: {}\n", w));
    }
    out.push_str("---\n\n");
    out
}

/// Document ready to save: metadata, Markdown body and, when the source had
/// HTML, the original HTML.
#[derive(Debug, Clone)]
pub struct PostDoc {
    pub meta: PostMeta,
    pub slug: String,
    pub markdown: String,
    pub html: Option<String>,
    /// The body came truncated or did not come: the account has no access to this post.
    pub locked: bool,
}

/// A piece of text becoming a piece of filename: lowercase, ASCII, a hyphen
/// in place of everything that is not a letter or number.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if ch.is_alphanumeric() {
            // Accents and non-ASCII alphabets become a hyphen: a predictable
            // filename is worth more than fidelity here.
            if !dash && !out.is_empty() {
                out.push('-');
                dash = true;
            }
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.len() > 70 {
        s[..70].trim_end_matches('-').to_string()
    } else {
        s
    }
}

/// Base filename: `YYYY-MM-DD-slug`, with the service's own slug when it
/// exists. It sorts itself in the folder and never collides with the
/// filesystem.
pub fn file_stem(date: &str, title: &str, slug: &str) -> String {
    let day: String = date.chars().take(10).collect();
    let day = if day.len() == 10 && day.as_bytes()[4] == b'-' {
        day
    } else {
        String::new()
    };
    let mut name = slugify(slug);
    if name.is_empty() {
        name = slugify(title);
    }
    if name.is_empty() {
        name = "post".into();
    }
    let joined = if day.is_empty() {
        name
    } else {
        format!("{}-{}", day, name)
    };
    super::sanitize_name(&joined)
}

/// Free path: if it already exists, it becomes `name (2)`, `name (3)`…
pub fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "post".into());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..1000 {
        let cand = dir.join(format!("{} ({}){}", stem, n, ext));
        if !cand.exists() {
            return cand;
        }
    }
    path
}

/// Minimal HTML so the `.html` file stays readable on its own, with no outside CSS.
pub fn wrap_html(meta: &PostMeta, body: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    format!(
        "<!doctype html>\n<html lang=\"pt-BR\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>{title}</title>\n\
         <style>body{{max-width:44rem;margin:3rem auto;padding:0 1.25rem;font:16px/1.65 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;color:#1c1c1e}}img{{max-width:100%;height:auto}}blockquote{{margin:1.5rem 0;padding-left:1rem;border-left:3px solid #d1d1d6;color:#3a3a3c}}pre{{overflow:auto;background:#f2f2f7;padding:1rem;border-radius:8px}}</style>\n\
         </head>\n<body>\n<h1>{title}</h1>\n<p><small>{sub}</small></p>\n{body}\n</body>\n</html>\n",
        title = esc(&meta.title),
        sub = esc(&format!(
            "{}{}{}",
            meta.author,
            if meta.author.is_empty() || meta.date.is_empty() { "" } else { " · " },
            meta.date
        )),
        body = body
    )
}

// ── Images ─────────────────────────────────────────────────────────────

static MD_IMAGE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"!\[(?P<alt>[^\]]*)\]\((?P<url>[^)\s]+)\)").expect("markdown image regex")
});

/// URLs of the images that appear in the Markdown, in order, without repeats.
pub fn image_urls(md: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for c in MD_IMAGE.captures_iter(md) {
        let url = c["url"].to_string();
        if !url.starts_with("http") {
            continue;
        }
        if !seen.contains(&url) {
            seen.push(url);
        }
    }
    seen
}

/// Filename of an image from its URL: the last piece of the path, with a
/// number in front so it does not collide and a guessed extension when missing.
pub fn image_file_name(index: usize, url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let tail = path.rsplit('/').next().unwrap_or("image");
    let tail = super::sanitize_name(tail);
    let has_ext = Path::new(&tail)
        .extension()
        .map(|e| {
            matches!(
                e.to_string_lossy().to_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "svg"
            )
        })
        .unwrap_or(false);
    let tail = if has_ext {
        tail
    } else {
        format!("{}.jpg", tail)
    };
    let tail: String = if tail.len() > 60 {
        let ext = Path::new(&tail)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "jpg".into());
        format!("imagem.{}", ext)
    } else {
        tail
    };
    format!("{:03}-{}", index + 1, tail)
}

/// Troca as URLs das imagens pelos caminhos relativos, no Markdown inteiro.
pub fn rewrite_images(md: &str, map: &HashMap<String, String>) -> String {
    MD_IMAGE
        .replace_all(md, |c: &regex::Captures| {
            let url = &c["url"];
            match map.get(url) {
                Some(rel) => format!("![{}]({})", &c["alt"], rel),
                None => c[0].to_string(),
            }
        })
        .to_string()
}

/// Downloads the post's images into `dir/rel` and returns the Markdown with the
/// links pointing at the local files. An image that fails keeps its original
/// URL — the text stays complete.
pub async fn localize_images(
    md: &str,
    fetcher: &Fetcher,
    dir: &Path,
    rel: &str,
    progress: &ProgressFn,
    id: &str,
) -> Result<(String, usize)> {
    let urls = image_urls(md);
    if urls.is_empty() {
        return Ok((md.to_string(), 0));
    }
    let target = dir.join(rel);
    std::fs::create_dir_all(&target)?;
    let mut map: HashMap<String, String> = HashMap::new();
    let mut ok = 0usize;
    for (i, url) in urls.iter().enumerate() {
        let name = image_file_name(i, url);
        let dest = target.join(&name);
        super::report(
            progress,
            id,
            "images",
            i as u64,
            Some(urls.len() as u64),
            Some(name.clone()),
        );
        if dest.exists() {
            map.insert(url.clone(), format!("{}/{}", rel, name));
            ok += 1;
            continue;
        }
        match super::download_to(fetcher.client(), url, &dest, progress, id).await {
            Ok(_) => {
                map.insert(url.clone(), format!("{}/{}", rel, name));
                ok += 1;
            }
            Err(e) => {
                tracing::warn!("blogs: image {} failed: {}", url, e);
            }
        }
    }
    Ok((rewrite_images(md, &map), ok))
}

/// Saves the post in the requested formats and returns the created paths.
pub fn write_post(
    dir: &Path,
    stem: &str,
    doc: &PostDoc,
    markdown: bool,
    html: bool,
    json: bool,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut files = Vec::new();
    if markdown {
        let path = unique_path(dir.join(format!("{}.md", stem)));
        let body = format!("{}{}", front_matter(&doc.meta), doc.markdown);
        std::fs::write(&path, body)?;
        files.push(path);
    }
    if html {
        if let Some(raw) = doc.html.as_deref().filter(|h| !h.trim().is_empty()) {
            let path = unique_path(dir.join(format!("{}.html", stem)));
            std::fs::write(&path, wrap_html(&doc.meta, raw))?;
            files.push(path);
        }
    }
    if json {
        let path = unique_path(dir.join(format!("{}.json", stem)));
        let value = serde_json::json!({
            "meta": doc.meta,
            "slug": doc.slug,
            "locked": doc.locked,
            "markdown": doc.markdown,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&value)?)?;
        files.push(path);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_so_do_dominio_certo_entra_no_pote() {
        let content = "# Netscape HTTP Cookie File\n\
.substack.com\tTRUE\t/\tTRUE\t0\tsubstack.sid\tabc\n\
on.substack.com\tTRUE\t/\tTRUE\t0\tconnect.sid\tdef\n\
.medium.com\tTRUE\t/\tTRUE\t0\tsid\txyz\n\
notsubstack.com\tTRUE\t/\tTRUE\t0\tfake\t666\n";
        let (_, n) = jar_from_netscape(content, &["substack.com".to_string()]);
        assert_eq!(n, 2, "only substack.com and its subdomains");
        let (_, m) = jar_from_netscape(content, &["medium.com".to_string()]);
        assert_eq!(m, 1);
        let (_, z) = jar_from_netscape("# vazio\n", &["substack.com".to_string()]);
        assert_eq!(z, 0);
    }

    #[test]
    fn dominio_casa_subdominio_mas_nao_sufixo_parecido() {
        let allow = vec!["substack.com".to_string()];
        assert!(domain_matches("substack.com", &allow));
        assert!(domain_matches(".substack.com", &allow));
        assert!(domain_matches("on.substack.com", &allow));
        assert!(!domain_matches("notsubstack.com", &allow));
        assert!(!domain_matches("substack.com.br", &allow));
    }

    #[test]
    fn prefixo_anti_sequestro_do_medium_e_cortado() {
        let raw = "])}while(1);</x>{\"payload\":{\"value\":[]}}";
        let v: serde_json::Value =
            serde_json::from_str(strip_json_prefix(raw)).expect("json after the trim");
        assert!(v["payload"]["value"].is_array());
        // JSON limpo passa intacto.
        assert_eq!(strip_json_prefix("  {\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_json_prefix("[1,2]"), "[1,2]");
    }

    #[test]
    fn paragrafo_titulo_e_lista_viram_markdown() {
        let md = html_to_markdown(
            "<h2>Título</h2><p>Um <strong>texto</strong> com <em>ênfase</em>.</p>\
             <ul><li>um</li><li>dois</li></ul>",
        );
        assert!(md.contains("## Título"), "{}", md);
        assert!(md.contains("**texto**"), "{}", md);
        assert!(md.contains("*ênfase*"), "{}", md);
        assert!(md.contains("- um"), "{}", md);
        assert!(md.ends_with('\n'));
    }

    #[test]
    fn imagem_com_legenda_sai_com_a_url_original() {
        let html = "<figure><img src=\"https://substackcdn.com/image/fetch/w_1456,c_limit,f_auto,q_auto:good/https%3A%2F%2Fsub.s3.amazonaws.com%2Fpublic%2Fimages%2Fum.png\" alt=\"gato\"><figcaption>Um gato</figcaption></figure>";
        let md = html_to_markdown(html);
        assert!(
            md.contains("![gato](https://sub.s3.amazonaws.com/public/images/um.png)"),
            "{}",
            md
        );
        assert!(md.contains("*Um gato*"), "{}", md);
    }

    #[test]
    fn bloco_de_codigo_sai_em_cerca() {
        let md = html_to_markdown("<pre><code>fn main() {}\nlet x = 1;</code></pre>");
        assert!(md.contains("```"), "{}", md);
        assert!(md.contains("fn main() {}"), "{}", md);
    }

    #[test]
    fn citacao_e_pullquote_viram_blockquote() {
        let md = html_to_markdown(
            "<blockquote><p>Citado</p></blockquote>\
             <blockquote class=\"pullquote\"><p>Destaque</p></blockquote>",
        );
        assert!(md.contains("> Citado"), "{}", md);
        assert!(md.contains("> Destaque"), "{}", md);
    }

    #[test]
    fn embed_vira_link_e_widget_de_assinatura_some() {
        let md = html_to_markdown(
            "<p>antes</p>\
             <div class=\"subscription-widget-wrap\"><p>Assine já</p></div>\
             <iframe src=\"https://www.youtube.com/embed/abc\"></iframe>\
             <p>depois</p>",
        );
        assert!(!md.contains("Assine já"), "{}", md);
        assert!(
            md.contains("[embed](https://www.youtube.com/embed/abc)"),
            "{}",
            md
        );
        assert!(md.contains("antes") && md.contains("depois"), "{}", md);
    }

    #[test]
    fn script_e_estilo_nao_viram_texto() {
        let md = html_to_markdown("<p>oi</p><script>var a=1;</script><style>p{color:red}</style>");
        assert!(!md.contains("var a"), "{}", md);
        assert!(!md.contains("color:red"), "{}", md);
        assert_eq!(md.trim(), "oi");
    }

    #[test]
    fn link_perde_parametro_de_rastreio() {
        assert_eq!(
            clean_url("https://x.com/p/abc?utm_source=substack&utm_medium=email&id=7"),
            "https://x.com/p/abc?id=7"
        );
        assert_eq!(
            clean_url("https://x.com/p/abc?utm_source=x"),
            "https://x.com/p/abc"
        );
        assert_eq!(clean_url("https://x.com/p/abc"), "https://x.com/p/abc");
        assert_eq!(
            clean_url("https://x.com/p?ref=home&q=1#topo"),
            "https://x.com/p?q=1#topo"
        );
    }

    #[test]
    fn front_matter_tem_so_o_que_existe() {
        let meta = PostMeta {
            title: "Um \"título\" difícil".into(),
            subtitle: None,
            author: "Autor".into(),
            publication: "Boletim".into(),
            date: "2026-09-08T12:04:21.658Z".into(),
            url: "https://x.substack.com/p/um".into(),
            tags: vec!["ia".into(), "rust".into()],
            audience: Some("everyone".into()),
            kind: "post".into(),
            words: Some(1200),
        };
        let fm = front_matter(&meta);
        assert!(fm.starts_with("---\n") && fm.ends_with("---\n\n"));
        assert!(
            fm.contains("title: \"Um \\\"título\\\" difícil\""),
            "{}",
            fm
        );
        assert!(fm.contains("tags: [\"ia\", \"rust\"]"), "{}", fm);
        assert!(fm.contains("words: 1200"));
        assert!(!fm.contains("subtitle:"), "empty field does not become a line");
    }

    #[test]
    fn nome_de_arquivo_e_seguro_e_ordenavel() {
        assert_eq!(
            file_stem(
                "2026-09-08T12:04:21.658Z",
                "God Help Us",
                "god-help-us-lets"
            ),
            "2026-09-08-god-help-us-lets"
        );
        // Without the service's slug, the title becomes the slug.
        assert_eq!(
            file_stem("2026-01-02T00:00:00Z", "Ação: café & pão!", ""),
            "2026-01-02-a-o-caf-p-o"
        );
        // No date, just the slug.
        assert_eq!(file_stem("", "Título", "meu-post"), "meu-post");
        // Slash and colon never reach the filesystem.
        let s = file_stem("2026-01-02", "a/b:c", "");
        assert!(!s.contains('/') && !s.contains(':'), "{}", s);
    }

    #[test]
    fn nome_de_imagem_numera_e_garante_extensao() {
        assert_eq!(
            image_file_name(0, "https://x.com/a/b/foto.png?w=1"),
            "001-foto.png"
        );
        assert_eq!(image_file_name(9, "https://x.com/a/b/foto"), "010-foto.jpg");
        let longo = format!("https://x.com/{}", "a".repeat(120));
        assert_eq!(image_file_name(0, &longo), "001-imagem.jpg");
    }

    #[test]
    fn imagens_do_markdown_sao_listadas_e_reescritas() {
        let md = "![a](https://x.com/1.png)\n\n![b](https://x.com/2.png)\n\n![a de novo](https://x.com/1.png)\n\n![local](imagens/3.png)";
        let urls = image_urls(md);
        assert_eq!(urls, vec!["https://x.com/1.png", "https://x.com/2.png"]);
        let mut map = HashMap::new();
        map.insert(
            "https://x.com/1.png".to_string(),
            "img/001-1.png".to_string(),
        );
        let out = rewrite_images(md, &map);
        assert!(out.contains("![a](img/001-1.png)"), "{}", out);
        assert!(out.contains("![a de novo](img/001-1.png)"), "{}", out);
        assert!(
            out.contains("![b](https://x.com/2.png)"),
            "unmapped stays"
        );
        assert!(out.contains("![local](imagens/3.png)"), "relativa fica");
    }

    #[test]
    fn tidy_corta_linha_em_branco_sobrando() {
        assert_eq!(tidy_markdown("a  \n\n\n\n\nb   "), "a\n\nb\n");
    }
}
