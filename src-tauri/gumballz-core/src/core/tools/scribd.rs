//! Scribd documents (text + images): the embed viewer HTML carries the
//! document JSON plus the first pages inline; the remaining pages load
//! as JSONP text layers. No JS engine needed — plain HTTP + regex.
//!
//! Flow: any Scribd URL -> canonical embed URL
//! `https://www.scribd.com/embeds/<id>/content` -> fetch the HTML ->
//! extract `page_count` + title from the `Scribd.EmbedsShow` JSON ->
//! per page, take the inline `<div class="newpage">` chunk when present,
//! otherwise fetch the page JSONP from `html.scribdassets.com` ->
//! draw each text span at its original position (the viewer scales the
//! text layer by 0.2 — see `extract_text_layer_scale`; font size comes
//! from the parent `ff` div, colour + letter-spacing from the span) over
//! the page's sprite image (`html.scribd.com/.../images/<N>-<hash>.(png|jpg)`),
//! whose `image_layer` fragments are drawn at their true positions ->
//! write one PDF page per Scribd page.
//!
//! Sprite layout (verified on doc 477317150, 21 pages): one sprite per
//! page, 904px wide. The sprite packs the page background (top
//! 902x1167, usually a solid colour) plus the page's inline images
//! below it; each `<img class="absimg">` shows one clipped window of
//! the sprite at its page position (CSS `clip:rect()` + `left`/`top`).
//! JPEG sprites (e.g. page 3 of that doc) are full-page-size images,
//! but they hold only the page background + inline images — the body
//! text is NOT baked into them (verified pixel-by-pixel on doc
//! 477317150: sprite 3 is green + the INPUT/OUTPUT diagram only).
//! So the text layer is always drawn VISIBLE, on every page type:
//! an earlier version drew it invisible (mode `3 Tr`) on full-page
//! JPEGs and those pages came out with no visible body text at all.
//! (No evidence exists of a sprite that both covers the page AND
//! carries the body text as pixels; if one ever appears, visible
//! text would double there — that case is unobserved, not handled.)
//! PNG sprites are drawn under the positioned text layer.
//!
//! Known limits (honest): text is real, searchable text in the
//! embedded Be Vietnam Pro font, not the document's original font, so
//! a long span may end a few points early or late — but every span
//! starts at its exact original x, so lines never drift. CSS
//! word-spacing is NOT applied: PDF `Tw` has no effect on Identity-H
//! CID text (verified against poppler — a Tw=5 test left the second
//! word exactly where Tw=0 put it). Letter-spacing IS applied via
//! `Tc` (verified: poppler shifts each glyph by Tc points).
//! Pages with no extractable text and no image get a placeholder line
//! instead of being silently skipped. Scribd only serves preview pages
//! for paywalled documents, so a partial result (e.g. 8/12 pages) is
//! normal — never claim a full download when pages are missing.
//! Documents with scrambled fonts (Scribd serves some with glyph
//! substitution) may produce garbled text. A sprite that cannot be
//! fetched or decoded falls back to text-only for that page, never
//! failing the whole download.
//!
//! The PDF embeds Be Vietnam Pro (SIL OFL, `fonts/OFL.txt`) so
//! Vietnamese diacritics render correctly — the whole font is
//! embedded, which makes each PDF ~130 KB larger.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context};
use image::ImageEncoder;
use serde::Serialize;

use super::{report, ProgressFn};

/// Parsed Scribd link: numeric document id + canonical embed URL.
#[derive(Debug, Clone, Serialize)]
pub struct ScribdInfo {
    pub id: String,
    pub secret_password: Option<String>,
    pub embed_url: String,
}

/// Document metadata from the embed page (`Scribd.EmbedsShow` JSON).
#[derive(Debug, Clone, Serialize)]
pub struct ScribdDocument {
    pub id: String,
    pub page_count: u32,
    pub title: String,
}

/// One Scribd page after text extraction.
#[derive(Debug, Clone)]
pub struct ScribdPage {
    pub num: u32,
    pub width: u32,
    pub height: u32,
    /// Reflowed reading-order text (fallback when `spans` is empty).
    pub text: String,
    /// False when no text could be extracted (image-only page):
    /// the PDF then carries a placeholder line for this page, unless
    /// the page has an image (the image is the content then).
    pub has_text: bool,
    /// Positioned text spans, drawn at their original places over
    /// the page image. Empty when the page has no usable spans — the
    /// reflowed `text` is drawn instead (see `page_spans_from_html`).
    pub spans: Vec<ScribdTextSpan>,
    /// Sprite images for this page (usually one), with the fragments
    /// that show them. Empty when no sprite could be fetched/decoded.
    pub images: Vec<ScribdPageImage>,
}

/// One word run inside a positioned span: Scribd table cells are a
/// single outer span whose words are separated by `<span class=w>`
/// spacers (fixed-width gaps) instead of plain spaces, so the words
/// must be drawn at their own x offsets or the columns collapse
/// ("JV EpisodeWord"). `gap_before` is the gap preceding this run,
/// display px: the spacer width plus, when a space precedes the
/// spacer, the word-spacing, one space advance and the
/// letter-spacing residual (already text-layer scaled). The first
/// run's gap is always 0.
#[derive(Debug, Clone)]
pub struct ScribdWordRun {
    pub gap_before: f64,
    pub text: String,
}

/// One text span at its original position (display px: the text-layer
/// scale is already applied by `page_spans_from_html`). Drawn over the
/// page image with the embedded font — real, searchable text.
#[derive(Debug, Clone)]
pub struct ScribdTextSpan {
    /// Left edge, display px from the page's left edge.
    pub left: f64,
    /// Top edge, display px from the page's top edge.
    pub top: f64,
    /// Font size, display px (inherited from the parent `ff` div).
    pub font_size: f64,
    /// Text content (inner tags stripped, entities decoded).
    pub text: String,
    /// RGB 0.0-1.0 (black when the span sets no colour).
    pub color: (f64, f64, f64),
    /// CSS letter-spacing, display px (default 0).
    pub letter_spacing: f64,
    /// Word runs split at `<span class=w>` spacers (table cells).
    /// Empty for ordinary spans: the whole `text` is drawn at
    /// (`left`, `top`). Non-empty: each run is drawn at its own x
    /// (see `build_text_pdf`); `text` is then the runs joined
    /// with single spaces.
    pub word_runs: Vec<ScribdWordRun>,
}

/// One clipped window of a sprite, as placed on the page
/// (CSS px, page top-left origin; the source rect is sprite px,
/// also top-left origin, at 1:1 scale).
#[derive(Debug, Clone)]
pub struct ScribdImageFragment {
    pub display_x: i32,
    pub display_y: i32,
    pub display_w: u32,
    pub display_h: u32,
    pub src_x: u32,
    pub src_y: u32,
}

/// One sprite image for a page, with the fragments that show it.
#[derive(Debug, Clone)]
pub struct ScribdPageImage {
    pub width: u32,
    pub height: u32,
    pub kind: ScribdImageKind,
    pub fragments: Vec<ScribdImageFragment>,
}

/// Sprite pixel data, ready for the PDF writer.
#[derive(Debug, Clone)]
pub enum ScribdImageKind {
    /// Raw JPEG bytes — embedded as DCTDecode, no alpha.
    /// `gray` is true for 1-component (grayscale) JPEGs.
    Jpeg { bytes: Vec<u8>, gray: bool },
    /// Decoded RGBA pixels, `width * height * 4` bytes (PNG etc.).
    Rgba(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ScribdDownloadOutcome {
    pub output_path: String,
    pub pages_got: u32,
    pub pages_total: u32,
}

/// Accepts `scribd.com/(document|doc|presentation)/<id>/...` and
/// `scribd.com/embeds/<id>/content` (any subdomain, optional
/// `?secret_password=...`). Anything else returns `None`.
pub fn parse_scribd_url(url: &str) -> Option<ScribdInfo> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "scribd.com" && !host.ends_with(".scribd.com") {
        return None;
    }
    let re =
        regex::Regex::new(r"^/(?:document|doc|presentation|embeds)/(\d+)(?:/|$)").ok()?;
    let c = re.captures(parsed.path())?;
    let id = c[1].to_string();
    let secret_password = parsed
        .query_pairs()
        .find(|(k, _)| k == "secret_password")
        .map(|(_, v)| v.to_string());
    let mut embed_url = format!("https://www.scribd.com/embeds/{}/content", id);
    if let Some(pw) = &secret_password {
        embed_url.push_str(&format!("?secret_password={}", urlencoding::encode(pw)));
    }
    Some(ScribdInfo {
        id,
        secret_password,
        embed_url,
    })
}

/// Extract `id`, `page_count` and `title` from the
/// `new Scribd.EmbedsShow("#...", {...})` call in the embed HTML.
/// Brace-matching (string-aware) instead of a lazy regex, so titles
/// containing `}` or `)` do not cut the JSON short.
pub fn extract_document_json(html: &str) -> Option<ScribdDocument> {
    let marker = "new Scribd.EmbedsShow(";
    let start = html.find(marker)?;
    let brace = html[start..].find('{')? + start;
    let end = find_matching_brace(&html[brace..])? + brace;
    let json: serde_json::Value = serde_json::from_str(&html[brace..=end]).ok()?;
    let doc = json.get("document")?;
    Some(ScribdDocument {
        id: doc.get("id")?.as_u64()?.to_string(),
        page_count: doc.get("page_count")?.as_u64()? as u32,
        title: doc.get("title")?.as_str()?.to_string(),
    })
}

/// Byte index of the `}` closing the `{` at `s[0]` (string-aware).
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, ch) in s.char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Page dimensions from `docManager.addPage({pageNum: N, origWidth: W,
/// origHeight: H, ...})`. Missing entries fall back to 902x1274.
pub fn extract_page_sizes(html: &str) -> BTreeMap<u32, (u32, u32)> {
    let mut out = BTreeMap::new();
    let Ok(num_re) = regex::Regex::new(r"pageNum\s*:\s*(\d+)") else {
        return out;
    };
    let Ok(w_re) = regex::Regex::new(r"origWidth\s*:\s*(\d+)") else {
        return out;
    };
    let Ok(h_re) = regex::Regex::new(r"origHeight\s*:\s*(\d+)") else {
        return out;
    };
    let mut rest = html;
    while let Some(pos) = rest.find("addPage({") {
        let chunk = &rest[pos..];
        let end = chunk.find("})").unwrap_or(chunk.len());
        let head = &chunk[..end];
        let num = num_re
            .captures(head)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok());
        let w = w_re
            .captures(head)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok());
        let h = h_re
            .captures(head)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok());
        if let (Some(n), Some(w), Some(h)) = (num, w, h) {
            out.insert(n, (w, h));
        }
        rest = &rest[pos + "addPage({".len()..];
    }
    out
}

/// Inline pages rendered in the embed HTML:
/// `<div class="newpage" id="pageN"> ... </div>`.
/// Each chunk runs up to the next inline page marker or the first
/// `<script>` after it (scripts carry no page spans, but the page
/// footer UI after them might).
pub fn extract_inline_pages(html: &str) -> Vec<(u32, String)> {
    let Ok(re) = regex::Regex::new(r#"<div class="newpage" id="page(\d+)"[^>]*>"#) else {
        return Vec::new();
    };
    let marks: Vec<(u32, usize)> = re
        .captures_iter(html)
        .filter_map(|c| {
            let n = c.get(1)?.as_str().parse::<u32>().ok()?;
            let start = c.get(0)?.start();
            Some((n, start))
        })
        .collect();
    let mut out = Vec::new();
    for (i, (num, start)) in marks.iter().enumerate() {
        let mut end = marks.get(i + 1).map(|(_, s)| *s).unwrap_or(html.len());
        if let Some(script) = html[*start..end].find("<script") {
            end = *start + script;
        }
        out.push((*num, html[*start..end].to_string()));
    }
    out
}

/// JSONP page URLs in the embed HTML:
/// `https://html.scribdassets.com/<prefix>/pages/<N>-<hash>.jsonp`.
/// Deduplicated by page number, sorted ascending.
pub fn extract_jsonp_urls(html: &str) -> Vec<(u32, String)> {
    let Ok(re) = regex::Regex::new(
        r"https://html\.scribdassets\.com/[\w-]+/pages/(\d+)-[\w-]+\.jsonp",
    ) else {
        return Vec::new();
    };
    let mut by_page: BTreeMap<u32, String> = BTreeMap::new();
    for c in re.captures_iter(html) {
        if let (Some(n), Some(u)) = (c.get(1), c.get(0)) {
            if let Ok(num) = n.as_str().parse::<u32>() {
                by_page.entry(num).or_insert_with(|| u.as_str().to_string());
            }
        }
    }
    by_page.into_iter().collect()
}

/// Page size from the page's own `<div class="newpage" ...>` style
/// (`width: 902px; height:1167px`). This is the reliable size: some
/// documents carry no `docManager.addPage` entries at all, and the
/// 902x1274 fallback then stretches the page (and its images).
/// The style attribute may be quote-escaped (`\"`) inside inline
/// chunks, so the match tolerates that.
pub fn extract_newpage_size(page_html: &str) -> Option<(u32, u32)> {
    let Ok(re) = regex::Regex::new(
        r#"newpage[^>]{0,120}?width:\s*(\d+)px;\s*height:\s*(\d+)px"#,
    ) else {
        return None;
    };
    let c = re.captures(page_html)?;
    Some((
        c.get(1)?.as_str().parse::<u32>().ok()?,
        c.get(2)?.as_str().parse::<u32>().ok()?,
    ))
}

/// `image_layer` fragments of one page's HTML (inline chunk or JSONP
/// inner): every `<img class="absimg" ...>` is one clipped window of
/// a sprite (`orig="http://html.scribd.com/<prefix>/images/<N>-<hash>.(png|jpg)"`).
/// Returns `(sprite_url, fragment)` pairs in document order (the
/// background layer first, overlays after — the PDF draws them in
/// this order). The `orig` value may be quote-escaped (`\"`) inside
/// inline chunks. Fragments with an empty clip are skipped.
pub fn extract_image_fragments(page_html: &str) -> Vec<(String, ScribdImageFragment)> {
    let Ok(re) = regex::Regex::new(
        r#"class=\\?"absimg\\?" style=\\?"left:\s*(-?\d+)px;\s*top:\s*(-?\d+)px;\s*width:\s*(\d+)px;\s*height:\s*(\d+)px;\s*clip:rect\(\s*(\d+)px\s+(\d+)px\s+(\d+)px\s+(\d+)px\s*\)\\?" orig=\\?"([^"\\]+)\\?"#,
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for c in re.captures_iter(page_html) {
        let get = |i: usize| -> Option<i32> {
            c.get(i)?.as_str().parse::<i32>().ok()
        };
        let (Some(left), Some(top), Some(clip_top), Some(clip_right), Some(clip_bottom), Some(clip_left)) =
            (get(1), get(2), get(5), get(6), get(7), get(8))
        else {
            continue;
        };
        let w = clip_right - clip_left;
        let h = clip_bottom - clip_top;
        if w <= 0 || h <= 0 {
            continue;
        }
        let Some(url) = c.get(9).map(|m| m.as_str().to_string()) else {
            continue;
        };
        out.push((
            url,
            ScribdImageFragment {
                display_x: left + clip_left,
                display_y: top + clip_top,
                display_w: w as u32,
                display_h: h as u32,
                src_x: clip_left as u32,
                src_y: clip_top as u32,
            },
        ));
    }
    out
}

/// Thumbnail for the paste-link preview: page 1's sprite image
/// (`html.scribd.com/.../images/1-<hash>.(png|jpg)`) — the document
/// cover as the viewer shows it. Taken from the already-fetched embed
/// HTML, so no extra request at resolve time. Page 1's inline chunk
/// first; if page 1 is not inline (or has no sprite), the first sprite
/// anywhere in the HTML, so a document whose first page is text-only
/// still gets a preview from a later illustrated page. `None` when the
/// document has no page image at all — the resolve + download flow must
/// never break because the thumbnail is missing. The `orig` URLs in the
/// raw HTML are `http://`; they are upgraded to `https://`
/// (html.scribd.com serves https — verified live, 200).
pub fn first_page_thumbnail_url(html: &str) -> Option<String> {
    let pages = extract_inline_pages(html);
    let page1 = pages
        .iter()
        .find(|(n, _)| *n == 1)
        .map(|(_, chunk)| chunk.as_str());
    let fragments = match page1 {
        Some(chunk) => {
            let frags = extract_image_fragments(chunk);
            if frags.is_empty() {
                extract_image_fragments(html)
            } else {
                frags
            }
        }
        None => extract_image_fragments(html),
    };
    let (url, _) = fragments.into_iter().next()?;
    Some(upgrade_http_to_https(url))
}

/// `http://x` -> `https://x`; anything else passes through unchanged.
fn upgrade_http_to_https(url: String) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url,
    }
}

/// `window.page4_callback(["<div...>"])` -> the inner HTML string.
/// The callback name varies per page; parse the first JSON value in
/// the body with a streaming deserializer (trailing `);` ignored).
pub fn unwrap_jsonp(body: &str) -> Option<String> {
    let start = body.find('[')?;
    let mut stream =
        serde_json::Deserializer::from_str(&body[start..]).into_iter::<serde_json::Value>();
    let value = stream.next()?.ok()?;
    value
        .as_array()?
        .first()?
        .as_str()
        .map(|s| s.to_string())
}

/// JSONP bodies arrive gzip-compressed; decode to text. The gzip magic
/// bytes decide — a plain-text body passes through untouched.
pub fn decode_jsonp_body(bytes: &[u8]) -> anyhow::Result<String> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut dec = flate2::read::GzDecoder::new(bytes);
        let mut out = String::new();
        dec.read_to_string(&mut out)
            .context("gunzip jsonp scribd")?;
        Ok(out)
    } else {
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Spans closer than this in `top` belong to the same line
/// (coordinates run up to ~6000; line spacing is ~60-100).
const LINE_TOP_TOLERANCE: i32 = 20;

fn decode_entities(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    // Numeric entities: &#233; and &#xE9;
    if let Ok(re) = regex::Regex::new(r"&#(x[0-9a-fA-F]+|\d+);") {
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                let raw = &caps[1];
                let code = if let Some(hex) = raw.strip_prefix('x') {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    raw.parse::<u32>().ok()
                };
                code.and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| caps[0].to_string())
            })
            .into_owned();
    }
    out
}

fn strip_tags(s: &str) -> String {
    match regex::Regex::new(r"<[^>]*>") {
        Ok(re) => re.replace_all(s, "").into_owned(),
        Err(_) => s.to_string(),
    }
}

/// Extract reading-order text from one Scribd page's HTML (inline chunk
/// or unwrapped JSONP). Spans are positioned absolutely and are NOT in
/// reading order in the HTML: sort by (top, left), group near-equal
/// tops into lines, join spans in a line with spaces (a span that
/// already ends with a space gets no extra one).
pub fn page_text_from_html(page_html: &str) -> String {
    let Ok(span_re) = regex::Regex::new(r"(?s)<span\b([^>]*)>(.*?)</span>") else {
        return String::new();
    };
    let Ok(left_re) = regex::Regex::new(r"left\s*:\s*([\d.]+)px") else {
        return String::new();
    };
    let Ok(top_re) = regex::Regex::new(r"top\s*:\s*([\d.]+)px") else {
        return String::new();
    };
    let mut spans: Vec<(i32, i32, String)> = Vec::new();
    for c in span_re.captures_iter(page_html) {
        let attrs = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let inner = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let left = left_re
            .captures(attrs)
            .and_then(|m| m.get(1))
            .and_then(|m| m.as_str().parse::<f64>().ok())
            .map(|v| v.round() as i32);
        let top = top_re
            .captures(attrs)
            .and_then(|m| m.get(1))
            .and_then(|m| m.as_str().parse::<f64>().ok())
            .map(|v| v.round() as i32);
        let (Some(left), Some(top)) = (left, top) else {
            continue;
        };
        let text = decode_entities(&strip_tags(inner));
        if text.trim().is_empty() {
            continue;
        }
        spans.push((top, left, text));
    }
    spans.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut lines: Vec<String> = Vec::new();
    let mut cur_top: Option<i32> = None;
    let mut cur_line = String::new();
    for (top, _left, text) in spans {
        let new_line = match cur_top {
            None => true,
            Some(t) => (top - t).abs() > LINE_TOP_TOLERANCE,
        };
        if new_line {
            if !cur_line.trim().is_empty() {
                lines.push(cur_line.trim_end().to_string());
            }
            cur_line = String::new();
            cur_top = Some(top);
        }
        cur_line.push_str(&text);
        if !text.ends_with(char::is_whitespace) {
            cur_line.push(' ');
        }
    }
    if !cur_line.trim().is_empty() {
        lines.push(cur_line.trim_end().to_string());
    }
    lines.join("\n")
}

/// The viewer scales the text layer (`.text_layer{transform:scale(.2);
/// transform-origin:top left}` in the embed CSS): span coordinates and
/// font sizes live in that scaled-up space. Read the factor from the
/// embed HTML so a future viewer change is picked up; 0.2 when the
/// rule is absent or unparsable.
pub fn extract_text_layer_scale(html: &str) -> f64 {
    let Ok(re) = regex::Regex::new(
        r"\.text_layer\s*\{[^}]*?transform\s*:\s*scale\(\s*([\d.]+)\s*\)",
    ) else {
        return 0.2;
    };
    for c in re.captures_iter(html) {
        if let Some(v) = c.get(1).and_then(|m| m.as_str().parse::<f64>().ok()) {
            if v > 0.0 && v <= 1.0 {
                return v;
            }
        }
    }
    0.2
}

/// CSS `color:#rrggbb` / `#rgb` as 0.0-1.0 RGB; anything else (named
/// colours, `transparent`, missing) falls back to black.
fn parse_css_color(style: &str) -> (f64, f64, f64) {
    let Ok(re) = regex::Regex::new(r"(?:^|;)\s*color\s*:\s*#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b")
    else {
        return (0.0, 0.0, 0.0);
    };
    let Some(hex) = re.captures(style).and_then(|c| c.get(1)).map(|m| m.as_str()) else {
        return (0.0, 0.0, 0.0);
    };
    let channel = |i: usize, n: usize| -> f64 {
        let part = &hex[i..i + n];
        let full = if n == 1 {
            format!("{0}{0}", part)
        } else {
            part.to_string()
        };
        u8::from_str_radix(&full, 16)
            .map(|v| v as f64 / 255.0)
            .unwrap_or(0.0)
    };
    if hex.len() == 6 {
        (channel(0, 2), channel(2, 2), channel(4, 2))
    } else {
        (channel(0, 1), channel(1, 1), channel(2, 1))
    }
}

/// True when a span's inner HTML carries nested `<span class=l>`
/// markers (plain `l`, not the numbered `l1`..`l12` selection
/// markers): each carries an inline negative `margin-left` that
/// cancels the outer span's `letter-spacing` — Scribd's way of
/// justifying table-cell text. The PDF must not re-apply that
/// letter-spacing as Tc or every character spreads apart
/// ("J V 0 0 1   A b e t", doc 254039756). `re` matches
/// `<span ... class=l` with a word boundary after the `l`, so
/// `class=l6` markers do not count.
fn has_l_span(inner: &str, re: &regex::Regex) -> bool {
    re.is_match(inner)
}

/// Split one positioned span's inner HTML at `<span class=w>`
/// word spacers (Scribd table cells: the words inside one cell are
/// separated by fixed-width spacers instead of plain spaces).
/// Returns word runs `(gap_before_display_px, text)` — the gap is
/// the spacer width plus, when a space precedes the spacer, the
/// word-spacing, one space advance (0.25em estimate: the web
/// font's exact space advance is not in the HTML, and the spacer
/// widths dominate the gap), and the letter-spacing residual
/// (`ls_layer` + the margin of the innermost `<span class=l>`
/// wrapping that space — the margins cancel the letter-spacing,
/// but not always exactly: doc 254039756 rows 20/21 carry -30/-28
/// against ls 22/20, a -8 residual that would otherwise shift the
/// word a point too far right). Empty when the span has no spacers:
/// the caller then draws it as one run. The runs' texts are the
/// span's words with tags stripped; the spacer itself contributes
/// no text. `w_re` matches a `<span ... class=w ... width:Wpx>`
/// spacer (word boundary after `w`, so the empty `w7` markers do
/// not count); `l_margin_re` captures the margin-left of an
/// `<span class=l ...>` open tag.
fn split_word_runs(
    inner: &str,
    size_layer: f64,
    ws_layer: f64,
    ls_layer: f64,
    scale: f64,
    w_re: &regex::Regex,
    l_margin_re: &regex::Regex,
) -> Vec<ScribdWordRun> {
    let mut runs: Vec<ScribdWordRun> = Vec::new();
    let mut seg_start = 0usize;
    let mut pending_gap = 0.0f64;
    let mut found = false;
    for cap in w_re.captures_iter(inner) {
        found = true;
        let m = cap.get(0).expect("w-spacer match");
        let seg = &inner[seg_start..m.start()];
        let seg_text = decode_entities(&strip_tags(seg));
        let space_before = seg_text.ends_with(char::is_whitespace);
        let w: f64 = cap
            .get(1)
            .and_then(|v| v.as_str().parse().ok())
            .unwrap_or(0.0);
        let t = seg_text.trim().to_string();
        if !t.is_empty() {
            runs.push(ScribdWordRun {
                gap_before: pending_gap,
                text: t,
            });
            pending_gap = 0.0;
        }
        // Gap introduced by this spacer, carried to the next run.
        pending_gap += w * scale;
        if space_before {
            // The space's own l-span margin (last <span class=l>
            // open tag before the spacer) plus the outer
            // letter-spacing is the residual the browser actually
            // applies after the space; without it rows whose
            // margin is not exactly -ls drift by that residual.
            let margin_space: f64 = l_margin_re
                .captures_iter(seg)
                .filter_map(|c| c.get(1)?.as_str().parse().ok())
                .last()
                .unwrap_or(0.0);
            pending_gap +=
                (0.25 * size_layer + ws_layer + ls_layer + margin_space) * scale;
        }
        // The spacer is empty: skip past its closing tag.
        seg_start = m.end();
        if let Some(rel) = inner[seg_start..].find("</span>") {
            seg_start += rel + 7;
        }
    }
    if !found {
        return Vec::new();
    }
    let tail = decode_entities(&strip_tags(&inner[seg_start..]));
    let t = tail.trim().to_string();
    if !t.is_empty() {
        runs.push(ScribdWordRun {
            gap_before: pending_gap,
            text: t,
        });
    }
    runs
}

/// Extract positioned text spans from one Scribd page's HTML (inline
/// chunk or unwrapped JSONP). Each span sits inside a `<div
/// class="ffN" style="font-size:NNpx">` wrapper (never nested); the
/// font size is inherited from it (a span's own font-size, if any,
/// wins). Coordinates are in text-layer space — `scale` (see
/// `extract_text_layer_scale`) maps them to display px. Returns spans
/// in document order. All-or-nothing: when any span on the page lacks
/// coordinates or a font size, the whole page yields no spans and the
/// caller falls back to the reflowed text — a half-positioned page
/// would lose words. Spans whose text is empty after stripping tags
/// are skipped silently (they draw nothing).
///
/// Paragraph pages nest marker spans inside the positioned span
/// (`<span class=w7></span>`, `<span class=l6>ou </span>` — word
/// fragments / empty markers for the viewer's selection layer). The
/// scanner matches each span's closing tag by nesting depth, so a
/// nested marker is consumed as part of its parent span's text
/// (tags stripped) and never treated as a span of its own — the
/// markers carry no coordinates, so a naive regex would fail the
/// whole page to the reflow fallback.
pub fn page_spans_from_html(page_html: &str, scale: f64) -> Vec<ScribdTextSpan> {
    let Ok(div_re) = regex::Regex::new(r"<div[^>]*font-size\s*:\s*([\d.]+)px") else {
        return Vec::new();
    };
    let Ok(left_re) = regex::Regex::new(r"left\s*:\s*([\d.]+)px") else {
        return Vec::new();
    };
    let Ok(top_re) = regex::Regex::new(r"top\s*:\s*([\d.]+)px") else {
        return Vec::new();
    };
    let Ok(size_re) = regex::Regex::new(r"font-size\s*:\s*([\d.]+)px") else {
        return Vec::new();
    };
    let Ok(ls_re) = regex::Regex::new(r"letter-spacing\s*:\s*(-?[\d.]+)px") else {
        return Vec::new();
    };
    let Ok(ws_re) = regex::Regex::new(r"word-spacing\s*:\s*(-?[\d.]+)px") else {
        return Vec::new();
    };
    // Nested `<span class=l>` (inline negative margins cancel the
    // outer letter-spacing) and `<span class=w ... width:Wpx>`
    // word spacers (table cells). Both regexes use a word boundary
    // after the class letter so the numbered `l1`..`l12` / `w1`..`w12`
    // selection markers never match.
    let Ok(l_re) = regex::Regex::new(r#"<span[^>]*class\s*=\s*["']?l\b"#) else {
        return Vec::new();
    };
    let Ok(w_spacer_re) = regex::Regex::new(
        r#"<span[^>]*class\s*=\s*["']?w\b[^>]*?width\s*:\s*([\d.]+)px"#,
    ) else {
        return Vec::new();
    };
    // Margin of an `<span class=l>` open tag: the innermost one
    // wrapping the space before a w-spacer carries the
    // letter-spacing residual (see `split_word_runs`).
    let Ok(l_margin_re) = regex::Regex::new(
        r#"<span[^>]*class\s*=\s*["']?l\b[^>]*margin-left\s*:\s*(-?[\d.]+)px"#,
    ) else {
        return Vec::new();
    };
    // Font sizes by byte offset: the last <div ... font-size ...>
    // before a span is its parent wrapper (ff divs never nest).
    let sizes: Vec<(usize, f64)> = div_re
        .captures_iter(page_html)
        .filter_map(|c| {
            let off = c.get(0)?.start();
            let v = c.get(1)?.as_str().parse::<f64>().ok()?;
            Some((off, v))
        })
        .collect();
    let num = |re: &regex::Regex, attrs: &str| -> Option<f64> {
        re.captures(attrs)
            .and_then(|m| m.get(1))
            .and_then(|m| m.as_str().parse::<f64>().ok())
    };
    // `<span` is a real tag open only when followed by whitespace, `>`
    // or `/` (not e.g. `<spanx`).
    let is_open = |html: &str, at: usize| -> bool {
        matches!(
            html.as_bytes().get(at + 5).copied(),
            Some(b' ') | Some(b'>') | Some(b'/') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        )
    };
    let mut spans = Vec::new();
    let mut pos = 0usize;
    while let Some(rel) = page_html[pos..].find("<span") {
        let open_start = pos + rel;
        if !is_open(page_html, open_start) {
            pos = open_start + 5;
            continue;
        }
        let Some(gt_rel) = page_html[open_start..].find('>') else {
            return Vec::new();
        };
        let open_end = open_start + gt_rel + 1;
        // Find this span's matching </span>, counting nested opens.
        let mut depth = 1usize;
        let mut cur = open_end;
        let mut close_start = None;
        while depth > 0 {
            let rest = &page_html[cur..];
            let next_open = rest.find("<span");
            let next_close = rest.find("</span>");
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    if is_open(page_html, cur + o) {
                        depth += 1;
                    }
                    cur += o + 5;
                }
                (_, Some(c)) => {
                    depth -= 1;
                    if depth == 0 {
                        close_start = Some(cur + c);
                    }
                    cur += c + 7;
                }
                (Some(_), None) | (None, None) => return Vec::new(),
            }
        }
        let Some(close_start) = close_start else {
            return Vec::new();
        };
        let attrs = &page_html[open_start + 5..open_end - 1];
        let inner = &page_html[open_end..close_start];
        let (Some(left), Some(top)) = (num(&left_re, attrs), num(&top_re, attrs)) else {
            return Vec::new();
        };
        // Font size: the span's own style wins, else the enclosing
        // ff div (the last font-size div before this span).
        let size = num(&size_re, attrs).or_else(|| {
            sizes
                .iter()
                .rev()
                .find(|(off, _)| *off < open_start)
                .map(|(_, v)| *v)
        });
        let Some(size) = size else {
            return Vec::new();
        };
        let raw_text = decode_entities(&strip_tags(inner));
        if raw_text.trim().is_empty() {
            pos = close_start + 7;
            continue;
        }
        let ls = num(&ls_re, attrs).unwrap_or(0.0);
        let ws = num(&ws_re, attrs).unwrap_or(0.0);
        // Table cells (doc 254039756): nested `<span class=l>`
        // margins cancel the outer letter-spacing, and `<span
        // class=w>` spacers are the real word gaps. Without this,
        // the PDF re-applies the raw letter-spacing as Tc and
        // drops the gaps, so "JV001 Abet" comes out letter-spaced
        // and "JV EpisodeWord" runs together.
        let letter_spacing = if has_l_span(inner, &l_re) {
            0.0
        } else {
            ls * scale
        };
        let word_runs =
            split_word_runs(inner, size, ws, ls, scale, &w_spacer_re, &l_margin_re);
        let text = if word_runs.is_empty() {
            raw_text
        } else {
            word_runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        spans.push(ScribdTextSpan {
            left: left * scale,
            top: top * scale,
            font_size: size * scale,
            text,
            color: parse_css_color(attrs),
            letter_spacing,
            word_runs,
        });
        pos = close_start + 7;
    }
    spans
}

/// JPEG header info `(width, height, components)` from the SOF0/SOF2
/// marker, without decoding pixels. `None` when the bytes are not a
/// JPEG or carry no SOF marker. The component count decides the PDF
/// colour space (1 = DeviceGray, 3 = DeviceRGB); anything else falls
/// back to the RGBA decode path so a CMYK JPEG never gets embedded
/// with the wrong colour space.
pub fn jpeg_info(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut off = 2usize;
    while off + 9 < bytes.len() {
        if bytes[off] != 0xFF {
            return None;
        }
        let marker = bytes[off + 1];
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            off += 2;
            continue;
        }
        if marker == 0xDA {
            return None; // start of scan: no SOF before it
        }
        let len = u16::from_be_bytes([bytes[off + 2], bytes[off + 3]]) as usize;
        if len < 2 || off + 2 + len > bytes.len() {
            return None;
        }
        if marker == 0xC0 || marker == 0xC2 {
            let h = u16::from_be_bytes([bytes[off + 5], bytes[off + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[off + 7], bytes[off + 8]]) as u32;
            let comps = bytes[off + 9];
            if w > 0 && h > 0 {
                return Some((w, h, comps));
            }
            return None;
        }
        off += 2 + len;
    }
    None
}

/// Decode one sprite image (PNG/JPEG/WebP...) with the `image` crate
/// (already a workspace dependency — no new crates). JPEGs keep their
/// raw DCT bytes so the PDF embeds them directly (smaller); anything
/// else is decoded to RGBA pixels. Returns `(width, height, kind)`.
/// A sprite that fails to decode returns `Err` — the caller then
/// falls back to text-only for that page.
pub fn decode_sprite(bytes: &[u8]) -> anyhow::Result<(u32, u32, ScribdImageKind)> {
    if let Some((w, h, comps)) = jpeg_info(bytes) {
        if comps == 3 || comps == 1 {
            // Validate it really decodes before embedding the raw bytes.
            let img = image::load_from_memory(bytes).context("giải mã JPEG sprite Scribd")?;
            debug_assert_eq!((img.width(), img.height()), (w, h));
            return Ok((
                w,
                h,
                ScribdImageKind::Jpeg {
                    bytes: bytes.to_vec(),
                    gray: comps == 1,
                },
            ));
        }
    }
    let img = image::load_from_memory(bytes).context("giải mã sprite Scribd")?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Ok((w, h, ScribdImageKind::Rgba(rgba.into_raw())))
}

// ---------------------------------------------------------------------------
// Minimal text-only PDF writer (no new crates).
// Embedded font: Be Vietnam Pro (SIL OFL, see `fonts/OFL.txt`) so
// Vietnamese diacritics render correctly. The font is embedded as
// CIDFontType2 with Identity-H encoding: text is written as 2-byte
// glyph ids and a ToUnicode CMap keeps it searchable/copyable.
// One Scribd page per PDF page (flowing to extra PDF pages when the
// text overflows). Page images are drawn as XObjects under the text:
// JPEG sprites embed their raw DCT bytes, PNG sprites are decoded to
// RGB (+ a soft-mask XObject when any pixel is transparent) and
// re-encoded with FlateDecode.
// ---------------------------------------------------------------------------

const EMBEDDED_FONT: &[u8] =
    include_bytes!("../../../fonts/BeVietnamPro-Regular.ttf");

const PDF_FONT_SIZE: f64 = 11.0;
const PDF_LEADING: f64 = 14.0;
const PDF_MARGIN: f64 = 36.0;
const A4_WIDTH: f64 = 595.0;
const A4_HEIGHT: f64 = 842.0;

fn be_u16(data: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*data.get(off)?, *data.get(off + 1)?]))
}

fn be_i16(data: &[u8], off: usize) -> Option<i16> {
    Some(i16::from_be_bytes([*data.get(off)?, *data.get(off + 1)?]))
}

fn be_u32(data: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *data.get(off)?,
        *data.get(off + 1)?,
        *data.get(off + 2)?,
        *data.get(off + 3)?,
    ]))
}

/// Minimal TrueType reader: Unicode -> glyph id (cmap format 4 / 12)
/// plus advance widths (hmtx) and the metrics the FontDescriptor needs.
struct EmbeddedFont<'a> {
    data: &'a [u8],
    units_per_em: u16,
    bbox: [i16; 4],
    ascent: i16,
    descent: i16,
    cap_height: i16,
    num_h_metrics: u16,
    hmtx: usize,
    cmap4: Option<usize>,
    cmap12: Option<usize>,
    question_gid: u16,
}

impl<'a> EmbeddedFont<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        let num_tables = be_u16(data, 4)? as usize;
        let table = |tag: &[u8; 4]| -> Option<usize> {
            for i in 0..num_tables {
                let off = 12 + i * 16;
                if data.get(off..off + 4) == Some(tag.as_slice()) {
                    return be_u32(data, off + 8).map(|v| v as usize);
                }
            }
            None
        };
        let head = table(b"head")?;
        let hhea = table(b"hhea")?;
        let hmtx = table(b"hmtx")?;
        let cmap = table(b"cmap")?;
        let os2 = table(b"OS/2")?;
        let units_per_em = be_u16(data, head + 18)?;
        let bbox = [
            be_i16(data, head + 36)?,
            be_i16(data, head + 38)?,
            be_i16(data, head + 40)?,
            be_i16(data, head + 42)?,
        ];
        let ascent = be_i16(data, hhea + 4)?;
        let descent = be_i16(data, hhea + 6)?;
        let cap_height = be_i16(data, os2 + 88)?;
        let num_h_metrics = be_u16(data, hhea + 34)?;
        // cmap subtables: prefer format 12, else format 4.
        let num_sub = be_u16(data, cmap + 2)? as usize;
        let mut cmap4 = None;
        let mut cmap12 = None;
        for i in 0..num_sub {
            let o = cmap + 4 + i * 8;
            let sub = cmap + be_u32(data, o + 4)? as usize;
            match be_u16(data, sub)? {
                4 if cmap4.is_none() => cmap4 = Some(sub),
                12 if cmap12.is_none() => cmap12 = Some(sub),
                _ => {}
            }
        }
        let mut font = EmbeddedFont {
            data,
            units_per_em,
            bbox,
            ascent,
            descent,
            cap_height,
            num_h_metrics,
            hmtx,
            cmap4,
            cmap12,
            question_gid: 0,
        };
        font.question_gid = font.glyph_id('?' as u32);
        Some(font)
    }

    /// Glyph id for a Unicode scalar, 0 when the font has no glyph.
    fn glyph_id(&self, code: u32) -> u16 {
        if let Some(base) = self.cmap12 {
            if let Some(gid) = self.glyph_id_12(base, code) {
                return gid;
            }
        }
        if let Some(base) = self.cmap4 {
            if code <= 0xFFFF {
                return self.glyph_id_4(base, code as u16);
            }
        }
        0
    }

    fn glyph_id_12(&self, base: usize, code: u32) -> Option<u16> {
        let n = be_u32(self.data, base + 12)? as usize;
        for i in 0..n {
            let o = base + 16 + i * 12;
            let start = be_u32(self.data, o)?;
            let end = be_u32(self.data, o + 4)?;
            if start <= code && code <= end {
                let gid = be_u32(self.data, o + 8)? as u64 + (code - start) as u64;
                return Some((gid & 0xFFFF) as u16);
            }
        }
        None
    }

    fn glyph_id_4(&self, base: usize, code: u16) -> u16 {
        let seg_count = match be_u16(self.data, base + 6) {
            Some(v) => (v / 2) as usize,
            None => return 0,
        };
        let end_base = base + 14;
        let start_base = base + 16 + 2 * seg_count;
        let delta_base = start_base + 2 * seg_count;
        let range_base = delta_base + 2 * seg_count;
        for i in 0..seg_count {
            let start = match be_u16(self.data, start_base + 2 * i) {
                Some(v) => v,
                None => return 0,
            };
            let end = match be_u16(self.data, end_base + 2 * i) {
                Some(v) => v,
                None => return 0,
            };
            if start <= code && code <= end {
                let delta = match be_i16(self.data, delta_base + 2 * i) {
                    Some(v) => v as i32,
                    None => return 0,
                };
                let range_off = match be_u16(self.data, range_base + 2 * i) {
                    Some(v) => v as usize,
                    None => return 0,
                };
                if range_off == 0 {
                    return (code as i32 + delta).rem_euclid(65536) as u16;
                }
                // idRangeOffset is relative to its own entry's address.
                let addr =
                    range_base + 2 * i + range_off + 2 * ((code - start) as usize);
                let gid = match be_u16(self.data, addr) {
                    Some(v) => v,
                    None => return 0,
                };
                if gid == 0 {
                    return 0;
                }
                return (gid as i32 + delta).rem_euclid(65536) as u16;
            }
        }
        0
    }

    /// Advance width of a glyph, in font units.
    fn advance(&self, gid: u16) -> u16 {
        let idx = (gid as usize).min(self.num_h_metrics.saturating_sub(1) as usize);
        be_u16(self.data, self.hmtx + idx * 4).unwrap_or(self.units_per_em)
    }

    /// Width of a string in points at the given font size.
    fn text_width_pt(&self, text: &str, size: f64) -> f64 {
        let units: u32 = text
            .chars()
            .map(|c| self.advance(self.glyph_id(c as u32)) as u32)
            .sum();
        units as f64 * size / self.units_per_em as f64
    }

    /// Encode one line as a PDF hex string of 2-byte glyph ids
    /// (`<00D000D1...>`), recording each used glyph (first char wins)
    /// for the ToUnicode CMap. Unmapped chars fall back to `?`.
    fn encode_line(&self, line: &str, used: &mut BTreeMap<u16, char>) -> Vec<u8> {
        let mut out = vec![b'<'];
        for c in line.chars() {
            let gid = self.glyph_id(c as u32);
            let gid = if gid == 0 {
                self.question_gid
            } else {
                used.entry(gid).or_insert(c);
                gid
            };
            out.extend_from_slice(format!("{:04X}", gid).as_bytes());
        }
        out.push(b'>');
        out
    }
}

/// Word-wrap one paragraph to a width budget (points, real advances).
fn wrap_paragraph(para: &str, font: &EmbeddedFont, max_width_pt: f64) -> Vec<String> {
    let space_w = font.text_width_pt(" ", PDF_FONT_SIZE);
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0f64;
    for word in para.split_whitespace() {
        let w = font.text_width_pt(word, PDF_FONT_SIZE);
        if cur.is_empty() {
            cur.push_str(word);
            cur_w = w;
        } else if cur_w + space_w + w <= max_width_pt {
            cur.push(' ');
            cur.push_str(word);
            cur_w += space_w + w;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
            cur_w = w;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Page size in points: the Scribd page's own aspect ratio, scaled to
/// fit inside A4 (never upscale a tiny page past A4).
fn page_size_pt(width: u32, height: u32) -> (f64, f64) {
    let (w, h) = (width.max(1) as f64, height.max(1) as f64);
    let scale = (A4_WIDTH / w).min(A4_HEIGHT / h).min(1.0);
    (w * scale, h * scale)
}

/// Format a float as a short PDF number (max 3 decimals, no
/// trailing zeros): `595`, `769.806`, `0.66`.
fn pdf_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Deflate-compress bytes for a FlateDecode stream object. PDF
/// FlateDecode is the zlib format (RFC 1950: header + Adler-32),
/// NOT raw deflate (RFC 1951) — a raw stream fails to decode in
/// readers ("Unknown compression method in flate stream"). Verified
/// with a poppler render of a simulated page during the probe.
fn deflate(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    if enc.write_all(bytes).is_err() {
        return Vec::new();
    }
    enc.finish().unwrap_or_default()
}

/// One sprite image prepared for the PDF: its XObject ids plus the
/// borrowed sprite (for the stream bodies at write time).
struct ImageEntry<'a> {
    image: &'a ScribdPageImage,
    color_id: usize,
    mask_id: Option<usize>,
    /// True when any RGBA pixel is transparent (needs an SMask).
    has_alpha: bool,
}

/// One output PDF page (a Scribd page may flow onto several in
/// reflowed mode; positioned mode is always exactly one).
struct PdfPageOut {
    pw: f64,
    ph: f64,
    /// CSS-px -> PDF-point scale (pw / scribd page width).
    scale: f64,
    /// Reflowed lines (fallback path) — empty in positioned mode.
    lines: Vec<Vec<u8>>,
    /// Positioned spans (empty in reflowed mode).
    spans: Vec<PositionedSpan>,
    /// Image-table indices drawn on this PDF page (only the first
    /// chunk of a Scribd page draws its images).
    images: Vec<usize>,
}

/// One positioned text span, encoded for the PDF (points,
/// bottom-left origin). `tc` is letter-spacing in points — the `Tc`
/// operator shifts each glyph by that many points (verified against
/// poppler; `Tw` does not apply to Identity-H CID text, so
/// word-spacing is deliberately not carried over).
struct PositionedSpan {
    x: f64,
    y: f64, // baseline
    size: f64,
    color: (f64, f64, f64),
    tc: f64,
    hex: Vec<u8>,
}

/// Baseline offset as a fraction of the font size: the web renders
/// the text layer with line-height:1, so the baseline sits at
/// top + (ascent + half-leading), half-leading =
/// (1em − ascent − descent) / 2 — computed from the embedded font's
/// own metrics (Be Vietnam Pro: ascent 1000, descent −265, upem
/// 1000 -> 0.8675).
fn baseline_offset_em(font: &EmbeddedFont) -> f64 {
    let upem = font.units_per_em as f64;
    let ascent = font.ascent as f64;
    let descent = font.descent as f64;
    (ascent + (upem - ascent + descent) / 2.0) / upem
}

/// Build the PDF bytes for all pages (one Scribd page -> one or more
/// PDF pages when the text overflows). Page images are drawn first
/// (under the text) as clipped sprite windows; the text layer stays
/// on top so it remains searchable.
pub fn build_text_pdf(pages: &[ScribdPage]) -> anyhow::Result<Vec<u8>> {
    let font = EmbeddedFont::parse(EMBEDDED_FONT)
        .ok_or_else(|| anyhow!("không đọc được font nhúng BeVietnamPro"))?;

    // Image XObjects first: ids 8..; a soft-mask XObject follows its
    // colour XObject when the sprite has any transparent pixel.
    let mut image_entries: Vec<ImageEntry> = Vec::new();
    let mut page_image_table: Vec<Vec<usize>> = Vec::new();
    let mut next_id = 8usize;
    for page in pages {
        let mut idxs = Vec::new();
        for image in &page.images {
            let has_alpha = match &image.kind {
                ScribdImageKind::Rgba(rgba) => {
                    rgba.chunks_exact(4).any(|px| px[3] < 255)
                }
                ScribdImageKind::Jpeg { .. } => false,
            };
            let color_id = next_id;
            next_id += 1;
            let mask_id = if has_alpha {
                let id = next_id;
                next_id += 1;
                Some(id)
            } else {
                None
            };
            idxs.push(image_entries.len());
            image_entries.push(ImageEntry {
                image,
                color_id,
                mask_id,
                has_alpha,
            });
        }
        page_image_table.push(idxs);
    }
    let first_page_id = next_id;

    // First pass: wrap + encode every line, collecting used glyphs.
    let mut used: BTreeMap<u16, char> = BTreeMap::new();
    let mut pdf_pages: Vec<PdfPageOut> = Vec::new();
    let baseline_em = baseline_offset_em(&font);
    for (pi, page) in pages.iter().enumerate() {
        let (pw, ph) = page_size_pt(page.width, page.height);
        let scale = pw / page.width.max(1) as f64;
        if !page.spans.is_empty() {
            // Positioned mode: one PDF page, every span at its
            // original place (x, baseline, size in points). A span
            // with word runs (table cell) draws each run at its own
            // x: the run's x is the span's left plus the gaps and
            // the measured widths of the runs before it, so the
            // words land in their true columns.
            let mut pspans = Vec::new();
            for span in &page.spans {
                let size = span.font_size * scale;
                let y = ph - (span.top + span.font_size * baseline_em) * scale;
                let tc = span.letter_spacing * scale;
                if span.word_runs.is_empty() {
                    let x = span.left * scale;
                    let hex = font.encode_line(&span.text, &mut used);
                    pspans.push(PositionedSpan {
                        x,
                        y,
                        size,
                        color: span.color,
                        tc,
                        hex,
                    });
                } else {
                    let mut x = span.left * scale;
                    for run in &span.word_runs {
                        x += run.gap_before * scale;
                        let hex = font.encode_line(&run.text, &mut used);
                        pspans.push(PositionedSpan {
                            x,
                            y,
                            size,
                            color: span.color,
                            tc,
                            hex,
                        });
                        x += font.text_width_pt(&run.text, size);
                    }
                }
            }
            pdf_pages.push(PdfPageOut {
                pw,
                ph,
                scale,
                lines: Vec::new(),
                spans: pspans,
                images: page_image_table[pi].clone(),
            });
            continue;
        }
        let max_width = pw - 2.0 * PDF_MARGIN;
        let lines_per_page = (((ph - 2.0 * PDF_MARGIN) / PDF_LEADING) as usize).max(1);
        let mut encoded: Vec<Vec<u8>> = Vec::new();
        for para in page.text.split('\n') {
            if para.trim().is_empty() {
                encoded.push(font.encode_line("", &mut used));
            } else {
                for line in wrap_paragraph(para, &font, max_width) {
                    encoded.push(font.encode_line(&line, &mut used));
                }
            }
        }
        if encoded.is_empty() {
            encoded.push(font.encode_line("", &mut used));
        }
        for (ci, chunk) in encoded.chunks(lines_per_page).enumerate() {
            pdf_pages.push(PdfPageOut {
                pw,
                ph,
                scale,
                lines: chunk.to_vec(),
                spans: Vec::new(),
                images: if ci == 0 {
                    page_image_table[pi].clone()
                } else {
                    Vec::new()
                },
            });
        }
    }
    if pdf_pages.is_empty() {
        anyhow::bail!("không có trang nào để ghi PDF");
    }
    if used.is_empty() {
        // Keep the ToUnicode CMap non-empty even for blank documents.
        used.insert(font.question_gid, '?');
    }

    // Object ids: 1 Catalog, 2 Pages, 3 Type0 font, 4 CIDFontType2,
    // 5 ToUnicode, 6 FontDescriptor, 7 FontFile2, 8.. image XObjects
    // (+ soft masks), then per PDF page a page object + a
    // content-stream object.
    let page_count = pdf_pages.len();
    let total = first_page_id + page_count * 2;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = vec![0; total + 1];
    let write_obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, id: usize, body: &[u8]| {
        offsets[id] = out.len();
        out.extend_from_slice(format!("{} 0 obj\n", id).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };

    out.extend_from_slice(b"%PDF-1.4\n");
    let kids: Vec<String> = (0..page_count)
        .map(|i| format!("{} 0 R", first_page_id + i * 2))
        .collect();
    write_obj(
        &mut out,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );
    write_obj(
        &mut out,
        &mut offsets,
        2,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            page_count
        )
        .as_bytes(),
    );
    write_obj(
        &mut out,
        &mut offsets,
        3,
        b"<< /Type /Font /Subtype /Type0 /BaseFont /BeVietnamPro-Regular /Encoding /Identity-H /DescendantFonts [4 0 R] /ToUnicode 5 0 R >>",
    );
    // /W array: consecutive glyph runs with their advances.
    let mut w_parts: Vec<String> = Vec::new();
    let gids: Vec<u16> = used.keys().copied().collect();
    let mut i = 0;
    while i < gids.len() {
        let mut j = i;
        while j + 1 < gids.len() && gids[j + 1] == gids[j] + 1 {
            j += 1;
        }
        let widths: Vec<String> = gids[i..=j]
            .iter()
            .map(|g| font.advance(*g).to_string())
            .collect();
        w_parts.push(format!("{} [{}]", gids[i], widths.join(" ")));
        i = j + 1;
    }
    write_obj(
        &mut out,
        &mut offsets,
        4,
        format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /BeVietnamPro-Regular /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 6 0 R /DW {} /W [{}] >>",
            font.advance(0),
            w_parts.join(" ")
        )
        .as_bytes(),
    );
    // ToUnicode CMap.
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", used.len()));
    for (gid, ch) in &used {
        cmap.push_str(&format!("<{:04X}> <{:04X}>\n", gid, *ch as u32));
    }
    cmap.push_str(
        "endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n",
    );
    let cmap_bytes = cmap.into_bytes();
    let mut tounicode = format!("<< /Length {} >>\nstream\n", cmap_bytes.len()).into_bytes();
    tounicode.extend_from_slice(&cmap_bytes);
    tounicode.extend_from_slice(b"\nendstream");
    write_obj(&mut out, &mut offsets, 5, &tounicode);
    write_obj(
        &mut out,
        &mut offsets,
        6,
        format!(
            "<< /Type /FontDescriptor /FontName /BeVietnamPro-Regular /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 7 0 R >>",
            font.bbox[0], font.bbox[1], font.bbox[2], font.bbox[3],
            font.ascent, font.descent, font.cap_height
        )
        .as_bytes(),
    );
    let mut fontfile = format!(
        "<< /Length {} /Length1 {} >>\nstream\n",
        EMBEDDED_FONT.len(),
        EMBEDDED_FONT.len()
    )
    .into_bytes();
    fontfile.extend_from_slice(EMBEDDED_FONT);
    fontfile.extend_from_slice(b"\nendstream");
    write_obj(&mut out, &mut offsets, 7, &fontfile);

    // Image XObjects.
    for entry in &image_entries {
        let img = entry.image;
        match &img.kind {
            ScribdImageKind::Jpeg { bytes, gray } => {
                let dict = format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                    img.width,
                    img.height,
                    if *gray { "/DeviceGray" } else { "/DeviceRGB" },
                    bytes.len()
                );
                let mut body = dict.into_bytes();
                body.extend_from_slice(bytes);
                body.extend_from_slice(b"\nendstream");
                write_obj(&mut out, &mut offsets, entry.color_id, &body);
            }
            ScribdImageKind::Rgba(rgba) => {
                let (w, h) = (img.width as usize, img.height as usize);
                let mut rgb = Vec::with_capacity(w * h * 3);
                let mut alpha = Vec::with_capacity(w * h);
                for px in rgba.chunks_exact(4) {
                    rgb.extend_from_slice(&px[..3]);
                    alpha.push(px[3]);
                }
                if entry.has_alpha {
                    if let Some(mask_id) = entry.mask_id {
                        let mask_data = deflate(&alpha);
                        let dict = format!(
                            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                            img.width, img.height, mask_data.len()
                        );
                        let mut body = dict.into_bytes();
                        body.extend_from_slice(&mask_data);
                        body.extend_from_slice(b"\nendstream");
                        write_obj(&mut out, &mut offsets, mask_id, &body);
                    }
                }
                let rgb_data = deflate(&rgb);
                let smask = match entry.mask_id {
                    Some(id) => format!(" /SMask {} 0 R", id),
                    None => String::new(),
                };
                let dict = format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode{} /Length {} >>\nstream\n",
                    img.width, img.height, smask, rgb_data.len()
                );
                let mut body = dict.into_bytes();
                body.extend_from_slice(&rgb_data);
                body.extend_from_slice(b"\nendstream");
                write_obj(&mut out, &mut offsets, entry.color_id, &body);
            }
        }
    }

    for (i, pdf_page) in pdf_pages.iter().enumerate() {
        let page_id = first_page_id + i * 2;
        let content_id = page_id + 1;
        let (pw, ph, scale) = (pdf_page.pw, pdf_page.ph, pdf_page.scale);
        // Content stream: sprite fragments first (clipped windows of
        // the sprite at their page positions), then the text layer.
        // CSS px (top-left origin) -> PDF points (bottom-left origin):
        // x = dx*s, y = ph - (dy+dh)*s. The image matrix places sprite
        // pixel (sx, sy) at display (dx, dy): e = (dx-sx)*s,
        // f = ph - s*(dy + H - sy).
        let mut stream: Vec<u8> = Vec::new();
        for &table_idx in &pdf_page.images {
            let entry = &image_entries[table_idx];
            let img = entry.image;
            let (iw, ih) = (img.width as f64, img.height as f64);
            for frag in &img.fragments {
                let dx = frag.display_x as f64;
                let dy = frag.display_y as f64;
                let dw = frag.display_w as f64;
                let dh = frag.display_h as f64;
                let sx = frag.src_x as f64;
                let sy = frag.src_y as f64;
                stream.extend_from_slice(
                    format!(
                        "q {} {} {} {} re W n {} 0 0 {} {} {} cm /Im{} Do Q ",
                        pdf_num(dx * scale),
                        pdf_num(ph - (dy + dh) * scale),
                        pdf_num(dw * scale),
                        pdf_num(dh * scale),
                        pdf_num(iw * scale),
                        pdf_num(ih * scale),
                        pdf_num((dx - sx) * scale),
                        pdf_num(ph - scale * (dy + ih - sy)),
                        entry.color_id
                    )
                    .as_bytes(),
                );
            }
        }
        if !pdf_page.spans.is_empty() {
            // Positioned text: one BT block; each span sets its own
            // size, letter-spacing (Tc, points) and colour, then an
            // absolute text matrix (Tm resets the pen, so spans are
            // independent of each other). The text is always drawn
            // visible: the sprite never carries the body text (see
            // the module docs), so hiding it would blank the page.
            stream.extend_from_slice(b"BT ");
            for span in &pdf_page.spans {
                stream.extend_from_slice(
                    format!(
                        "/F1 {} Tf {} Tc {} {} {} rg 1 0 0 1 {} {} Tm ",
                        pdf_num(span.size),
                        pdf_num(span.tc),
                        pdf_num(span.color.0),
                        pdf_num(span.color.1),
                        pdf_num(span.color.2),
                        pdf_num(span.x),
                        pdf_num(span.y),
                    )
                    .as_bytes(),
                );
                stream.extend_from_slice(&span.hex);
                stream.extend_from_slice(b" Tj ");
            }
            stream.extend_from_slice(b"ET");
        } else {
            // Reflowed fallback: BT /F1 11 Tf 14 TL 36 <top> Td <hex> Tj T* ... ET
            stream.extend_from_slice(b"BT /F1 11 Tf ");
            stream.extend_from_slice(b"14 TL ");
            stream.extend_from_slice(
                format!("{} {} Td ", PDF_MARGIN, ph - PDF_MARGIN - PDF_FONT_SIZE).as_bytes(),
            );
            for (j, line) in pdf_page.lines.iter().enumerate() {
                if j > 0 {
                    stream.extend_from_slice(b"T* ");
                }
                stream.extend_from_slice(line);
                stream.extend_from_slice(b" Tj ");
            }
            stream.extend_from_slice(b"ET");
        }
        let xobject = if pdf_page.images.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = pdf_page
                .images
                .iter()
                .map(|&ti| {
                    let id = image_entries[ti].color_id;
                    format!("/Im{} {} 0 R", id, id)
                })
                .collect();
            format!(" /XObject << {} >>", parts.join(" "))
        };
        write_obj(
            &mut out,
            &mut offsets,
            page_id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >>{} >> /Contents {} 0 R >>",
                pdf_num(pw),
                pdf_num(ph),
                xobject,
                content_id
            )
            .as_bytes(),
        );
        let mut content_body = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        content_body.extend_from_slice(&stream);
        content_body.extend_from_slice(b"\nendstream");
        write_obj(&mut out, &mut offsets, content_id, &content_body);
    }

    let xref_pos = out.len();
    out.extend_from_slice(b"xref\n");
    out.extend_from_slice(format!("0 {}\n", total).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for id in 1..total {
        out.extend_from_slice(format!("{:010} 00000 n \n", offsets[id]).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            total, xref_pos
        )
        .as_bytes(),
    );
    Ok(out)
}

// ---------------------------------------------------------------------------
// Minimal DOCX writer (no new crates): the `zip` crate + hand-built
// WordprocessingML. DOCX is flow layout, so spans become paragraphs in
// document order (their PDF positions are irrelevant here); consecutive
// table-cell spans merge into one Word table; each page's sprite is
// embedded as an inline image after its text; page breaks between
// Scribd pages. Text is plain UTF-8, XML-escaped; Calibri is named,
// never embedded, so Vietnamese diacritics render in any Word reader.
// ---------------------------------------------------------------------------

/// Content width of the DOCX page in EMU (Letter 8.5" with 1" margins:
/// 6.5" * 914400 EMU/inch).
const DOCX_CONTENT_WIDTH_EMU: u32 = 5_943_600;

/// Escape text for a WordprocessingML text node (`&<>"'`).
fn docx_escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn docx_para(text: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        docx_escape_xml(text)
    )
}

/// One Word table with visible single borders; each row is one
/// table-cell span, each run one cell.
fn docx_table(rows: &[Vec<String>]) -> String {
    let mut out = String::from(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders>\
        <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
        <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
        <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
        <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
        <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
        <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\
        </w:tblBorders><w:tblLayout w:type=\"autofit\"/></w:tblPr>",
    );
    for row in rows {
        out.push_str("<w:tr>");
        for cell in row {
            out.push_str("<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr>");
            if cell.trim().is_empty() {
                out.push_str("<w:p/>");
            } else {
                out.push_str(&docx_para(cell));
            }
            out.push_str("</w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}

/// Paragraph holding one inline image (drawing), sized to the page
/// content width with its aspect ratio kept. `rel_id` is the
/// `r:embed` relationship id from `word/_rels/document.xml.rels`.
fn docx_image_para(rel_id: usize, cx: u32, cy: u32) -> String {
    format!(
        "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
        <wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{rel_id}\" name=\"Picture {rel_id}\"/>\
        <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
        <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
        <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
        <pic:nvPicPr><pic:cNvPr id=\"{rel_id}\" name=\"image{rel_id}\"/><pic:cNvPicPr/></pic:nvPicPr>\
        <pic:blipFill><a:blip r:embed=\"rId{rel_id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
        <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
        <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
        </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}

/// One image ready for the DOCX package: (bytes, extension,
/// content type). JPEG sprites embed their raw bytes; PNG sprites
/// were decoded to RGBA for the PDF writer, so they are re-encoded
/// to PNG here. `None` on any encode problem — the caller skips
/// that image instead of failing the download.
fn docx_encode_image(image: &ScribdPageImage) -> Option<(Vec<u8>, &'static str, &'static str)> {
    match &image.kind {
        ScribdImageKind::Jpeg { bytes, .. } => {
            Some((bytes.clone(), "jpg", "image/jpeg"))
        }
        ScribdImageKind::Rgba(rgba) => {
            let mut buf = Vec::new();
            let mut enc = image::codecs::png::PngEncoder::new(&mut buf);
            enc.write_image(
                rgba,
                image.width,
                image.height,
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
            Some((buf, "png", "image/png"))
        }
    }
}

struct DocxMedia {
    bytes: Vec<u8>,
    ext: &'static str,
    width: u32,
    height: u32,
}

/// Build a .docx from Scribd pages: title once at the top, then per
/// page the spans as paragraphs (table-cell spans merged into one
/// table), the page's sprite image after its text, and a page break
/// between Scribd pages.
pub fn build_docx(pages: &[ScribdPage], title: &str) -> anyhow::Result<Vec<u8>> {
    // Collect embeddable images first, in page order (rId1 is the
    // styles part, so image rel ids start at rId2).
    let mut media: Vec<DocxMedia> = Vec::new();
    let mut page_media: Vec<Vec<usize>> = vec![Vec::new(); pages.len()];
    for (pi, page) in pages.iter().enumerate() {
        for image in &page.images {
            if let Some((bytes, ext, _ct)) = docx_encode_image(image) {
                page_media[pi].push(media.len());
                media.push(DocxMedia {
                    bytes,
                    ext,
                    width: image.width,
                    height: image.height,
                });
            }
        }
    }

    let mut body = String::new();
    if !title.trim().is_empty() {
        body.push_str(&format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            docx_escape_xml(title.trim())
        ));
    }
    for (pi, page) in pages.iter().enumerate() {
        let mut table: Vec<Vec<String>> = Vec::new();
        if !page.spans.is_empty() {
            for span in &page.spans {
                if span.word_runs.is_empty() {
                    if !table.is_empty() {
                        body.push_str(&docx_table(&table));
                        table.clear();
                    }
                    let t = span.text.trim();
                    if !t.is_empty() {
                        body.push_str(&docx_para(t));
                    }
                } else {
                    table.push(
                        span.word_runs
                            .iter()
                            .map(|r| r.text.clone())
                            .collect(),
                    );
                }
            }
        } else {
            // Reflow fallback: one paragraph per non-empty line.
            for line in page.text.lines() {
                let t = line.trim();
                if !t.is_empty() {
                    body.push_str(&docx_para(t));
                }
            }
            if page.text.trim().is_empty() && page.images.is_empty() {
                body.push_str(&docx_para(&format!(
                    "[Trang {}: trang ảnh, không trích xuất được chữ]",
                    page.num
                )));
            }
        }
        if !table.is_empty() {
            body.push_str(&docx_table(&table));
            table.clear();
        }
        for &mi in &page_media[pi] {
            let m = &media[mi];
            let cy = DOCX_CONTENT_WIDTH_EMU as u64 * m.height.max(1) as u64
                / m.width.max(1) as u64;
            body.push_str(&docx_image_para(mi + 2, DOCX_CONTENT_WIDTH_EMU, cy as u32));
        }
        if pi + 1 < pages.len() {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
    }

    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
        xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
        xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
        xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
        xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
        <w:body>{body}<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/>\
        <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>\
        </w:sectPr></w:body></w:document>"
    );

    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    );
    for (i, m) in media.iter().enumerate() {
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{}.{}\"/>",
            i + 2,
            i + 1,
            m.ext
        ));
    }
    rels.push_str("</Relationships>");

    let content_types = "\
        <?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>\
        <Default Extension=\"png\" ContentType=\"image/png\"/>\
        <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
        <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
        </Types>";

    let package_rels = "\
        <?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
        </Relationships>";

    // Minimal styles: Normal + Title, Calibri throughout.
    let styles = "\
        <?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
        <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
        <w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"en-US\"/></w:rPr></w:rPrDefault>\
        <w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
        <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/>\
        <w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/><w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr></w:style>\
        <w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/>\
        <w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/><w:b/><w:sz w:val=\"32\"/><w:szCs w:val=\"32\"/></w:rPr></w:style>\
        </w:styles>";

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), content_types.as_bytes().to_vec()),
        ("_rels/.rels".to_string(), package_rels.as_bytes().to_vec()),
        ("word/document.xml".to_string(), document_xml.into_bytes()),
        ("word/_rels/document.xml.rels".to_string(), rels.into_bytes()),
        ("word/styles.xml".to_string(), styles.as_bytes().to_vec()),
    ];
    for (i, m) in media.into_iter().enumerate() {
        files.push((format!("word/media/image{}.{}", i + 1, m.ext), m.bytes));
    }
    for (name, bytes) in files {
        zip.start_file(name, opts)
            .context("ghi file vào docx")?;
        zip.write_all(&bytes).context("ghi file vào docx")?;
    }
    let out = zip.finish().context("hoàn tất docx")?;
    Ok(out.into_inner())
}

/// File extension for a requested Scribd format: only `"docx"`
/// (case-insensitive) produces a DOCX — anything else (including
/// empty or unknown) falls back to PDF.
fn scribd_format_ext(format: &str) -> &'static str {
    if format.eq_ignore_ascii_case("docx") {
        "docx"
    } else {
        "pdf"
    }
}

// ---------------------------------------------------------------------------
// Network: fetch the embed page, then each page's text (inline or JSONP).
// ---------------------------------------------------------------------------

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// GET the embed page HTML. Plain reqwest GET with a browser UA —
/// the viewer HTML is server-rendered, no JS needed.
pub async fn fetch_embed_html(
    client: &reqwest::Client,
    embed_url: &str,
) -> anyhow::Result<String> {
    let resp = client.get(embed_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Scribd trả về HTTP {}", resp.status());
    }
    Ok(resp.text().await?)
}

/// Fetch every page's text + sprite image for a document whose embed
/// HTML is already loaded. Inline pages first, JSONP second; a page
/// that yields no text (or whose JSONP fetch fails — preview ends here
/// for paywalled docs) becomes a placeholder page, never a silent
/// skip — unless the page has a sprite image, in which case the image
/// is the content. A sprite that cannot be fetched or decoded falls
/// back to text-only for that page, never failing the download.
pub async fn fetch_page_texts(
    client: &reqwest::Client,
    doc: &ScribdDocument,
    html: &str,
    progress: &ProgressFn,
) -> anyhow::Result<Vec<ScribdPage>> {
    let inline: BTreeMap<u32, String> = extract_inline_pages(html).into_iter().collect();
    let jsonp: BTreeMap<u32, String> = extract_jsonp_urls(html).into_iter().collect();
    let sizes = extract_page_sizes(html);
    // The viewer scales the text layer (see extract_text_layer_scale):
    // span coordinates are in that scaled-up space.
    let text_scale = extract_text_layer_scale(html);
    let mut pages = Vec::new();
    for n in 1..=doc.page_count {
        let mut text = String::new();
        let mut page_html: Option<String> = None;
        if let Some(chunk) = inline.get(&n) {
            text = page_text_from_html(chunk);
            page_html = Some(chunk.clone());
        } else if let Some(url) = jsonp.get(&n) {
            // JSONP bodies are gzip-compressed: ask for them raw and
            // decode by hand (flate2), so a plain body still works.
            if let Ok(resp) = client
                .get(url)
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .header(reqwest::header::USER_AGENT, BROWSER_UA)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(bytes) = resp.bytes().await {
                        if let Ok(body) = decode_jsonp_body(&bytes) {
                            if let Some(inner) = unwrap_jsonp(&body) {
                                text = page_text_from_html(&inner);
                                page_html = Some(inner);
                            }
                        }
                    }
                }
            }
        }
        // Real page size: the page's own newpage div first, then the
        // addPage map, then the fallback.
        let (w, h) = page_html
            .as_deref()
            .and_then(extract_newpage_size)
            .or_else(|| sizes.get(&n).copied())
            .unwrap_or((902, 1274));
        // Sprite image: one fetch per distinct sprite URL on the page.
        let mut images: Vec<ScribdPageImage> = Vec::new();
        if let Some(page_html) = &page_html {
            let mut by_url: Vec<(String, Vec<ScribdImageFragment>)> = Vec::new();
            for (url, frag) in extract_image_fragments(page_html) {
                match by_url.iter_mut().find(|(u, _)| u == &url) {
                    Some((_, fs)) => fs.push(frag),
                    None => by_url.push((url, vec![frag])),
                }
            }
            for (url, fragments) in by_url {
                if let Ok(resp) = client
                    .get(&url)
                    .header(reqwest::header::USER_AGENT, BROWSER_UA)
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        if let Ok(bytes) = resp.bytes().await {
                            if let Ok((sw, sh, kind)) = decode_sprite(&bytes) {
                                images.push(ScribdPageImage {
                                    width: sw,
                                    height: sh,
                                    kind,
                                    fragments,
                                });
                            }
                        }
                    }
                }
            }
        }
        let has_text = !text.trim().is_empty();
        if !has_text && images.is_empty() {
            text = format!("[Trang {}: trang ảnh, không trích xuất được chữ]", n);
        }
        // Positioned spans (empty -> the reflowed text is drawn
        // instead, see page_spans_from_html).
        let spans = page_html
            .as_deref()
            .map(|h| page_spans_from_html(h, text_scale))
            .unwrap_or_default();
        pages.push(ScribdPage {
            num: n,
            width: w,
            height: h,
            text,
            has_text,
            spans,
            images,
        });
        report(
            progress,
            "scribd",
            "fetch",
            n as u64,
            Some(doc.page_count as u64),
            None,
        );
    }
    Ok(pages)
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    let mut s: String = name.chars().take(max_chars).collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() {
        "scribd-document".to_string()
    } else {
        s
    }
}

/// Full pipeline: parse -> embed HTML -> page texts -> PDF/DOCX ->
/// `<sanitized title>.pdf|.docx` in `dest_dir` (suffixed ` (2)`,
/// ` (3)`… on collision). Returns the output path + page counts.
/// `format` is `"docx"` for a Word document; anything else builds a PDF.
pub async fn download_document(
    url: &str,
    dest_dir: &str,
    progress: ProgressFn,
    format: &str,
) -> anyhow::Result<ScribdDownloadOutcome> {
    let info = parse_scribd_url(url)
        .ok_or_else(|| anyhow!("không phải link Scribd (document/doc/presentation/embeds)"))?;
    let client = super::client()?;
    let html = fetch_embed_html(&client, &info.embed_url).await?;
    let doc = extract_document_json(&html)
        .ok_or_else(|| anyhow!("không đọc được thông tin tài liệu Scribd"))?;
    report(
        &progress,
        "scribd",
        "fetch",
        0,
        Some(doc.page_count as u64),
        None,
    );
    let pages = fetch_page_texts(&client, &doc, &html, &progress).await?;
    report(
        &progress,
        "scribd",
        "compile",
        0,
        Some(pages.len() as u64),
        None,
    );
    let ext = scribd_format_ext(format);
    let bytes = if ext == "docx" {
        build_docx(&pages, &doc.title)?
    } else {
        build_text_pdf(&pages)?
    };
    report(
        &progress,
        "scribd",
        "compile",
        pages.len() as u64,
        Some(pages.len() as u64),
        None,
    );

    let dir = PathBuf::from(dest_dir);
    std::fs::create_dir_all(&dir)?;
    let base = truncate_name(&super::sanitize_name(&doc.title), 120);
    let mut path = dir.join(format!("{}.{}", base, ext));
    let mut i = 2;
    while path.exists() {
        path = dir.join(format!("{} ({}).{}", base, i, ext));
        i += 1;
    }
    std::fs::write(&path, &bytes)?;
    let got = pages.iter().filter(|p| p.has_text).count() as u32;
    report(
        &progress,
        "scribd",
        "done",
        pages.len() as u64,
        Some(pages.len() as u64),
        None,
    );
    Ok(ScribdDownloadOutcome {
        output_path: path.to_string_lossy().to_string(),
        pages_got: got,
        pages_total: doc.page_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMBED_HTML: &str = r##"<!doctype html><html><head></head><body>
<script>window.Scribd = {}; docManager.assetPrefix = "4aru250fy86kbd6b";
docManager.addPage({pageNum: 1, origWidth: 902, origHeight: 1274});
docManager.addPage({pageNum: 2, origWidth: 902, origHeight: 1274});
docManager.addPage({pageNum: 3, origWidth: 800, origHeight: 1000});</script>
<script>new Scribd.EmbedsShow("#autogen_id_1", {"document":{"id":388437553,"page_count":3,"title":"Example memo (draft) } final","url":"https://www.scribd.com/document/388437553/Example-memo"},"start_page":1})</script>
<div class="newpage" id="page1"><div class="text_layer"><div class="ff1" style="font-size:83px"><span class="a" style="left:1500px;top:569px">MOOT</span><span class="a" style="left:1301px;top:569px">THE ICC</span><span style="left:1700px;top:569px">COURT </span></div><div class="ff1"><span style="left:100px;top:700px">Second line here</span></div></div></div>
<div class="newpage" id="page2"><div class="text_layer"><div class="ff2"><span style="left:50px;top:100px">Only page two</span></div></div></div>
<script src="https://html.scribdassets.com/4aru250fy86kbd6b/pages/3-abc123.jsonp"></script>
</body></html>"##;

    #[test]
    fn test_parse_scribd_url() {
        let doc = parse_scribd_url("https://www.scribd.com/document/388437553/Example-memo")
            .expect("document url");
        assert_eq!(doc.id, "388437553");
        assert_eq!(
            doc.embed_url,
            "https://www.scribd.com/embeds/388437553/content"
        );
        assert_eq!(doc.secret_password, None);

        let legacy = parse_scribd_url("https://de.scribd.com/doc/12345/Title").expect("doc url");
        assert_eq!(legacy.id, "12345");
        assert_eq!(
            legacy.embed_url,
            "https://www.scribd.com/embeds/12345/content"
        );

        let pres = parse_scribd_url("https://www.scribd.com/presentation/777/Slides")
            .expect("presentation url");
        assert_eq!(pres.id, "777");

        let embed =
            parse_scribd_url("https://www.scribd.com/embeds/999/content").expect("embed url");
        assert_eq!(embed.id, "999");

        let secret = parse_scribd_url(
            "https://www.scribd.com/document/555/Priv?secret_password=s3cr3t",
        )
        .expect("secret url");
        assert_eq!(secret.secret_password.as_deref(), Some("s3cr3t"));
        assert!(secret.embed_url.contains("secret_password=s3cr3t"));

        // Non-Scribd hosts and non-document paths are rejected.
        assert!(parse_scribd_url("https://notscribd.com/document/123/Title").is_none());
        assert!(parse_scribd_url("https://www.scribd.com/search?query=test").is_none());
        assert!(parse_scribd_url("https://www.scribd.com/document/abc/Title").is_none());
        assert!(parse_scribd_url("not a url").is_none());
    }

    #[test]
    fn test_extract_document_json() {
        let doc = extract_document_json(EMBED_HTML).expect("doc json");
        assert_eq!(doc.id, "388437553");
        assert_eq!(doc.page_count, 3);
        // Title contains `}` and `)` — the brace matcher must not cut early.
        assert_eq!(doc.title, "Example memo (draft) } final");
        assert!(extract_document_json("<html>no embed here</html>").is_none());
    }

    #[test]
    fn test_extract_page_sizes() {
        let sizes = extract_page_sizes(EMBED_HTML);
        assert_eq!(sizes.get(&1), Some(&(902, 1274)));
        assert_eq!(sizes.get(&2), Some(&(902, 1274)));
        assert_eq!(sizes.get(&3), Some(&(800, 1000)));
        assert_eq!(sizes.get(&4), None);
    }

    #[test]
    fn test_extract_inline_pages() {
        let pages = extract_inline_pages(EMBED_HTML);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].0, 1);
        assert!(pages[0].1.contains("THE ICC"));
        assert_eq!(pages[1].0, 2);
        assert!(pages[1].1.contains("Only page two"));
        // The trailing <script> (JSONP ref) must not leak into page 2.
        assert!(!pages[1].1.contains("jsonp"));
    }

    #[test]
    fn test_extract_jsonp_urls() {
        let html = "a https://html.scribdassets.com/4aru250fy86kbd6b/pages/3-abc123.jsonp b \
            https://html.scribdassets.com/4aru250fy86kbd6b/pages/4-def456.jsonp c \
            https://html.scribdassets.com/4aru250fy86kbd6b/pages/3-abc123.jsonp";
        let urls = extract_jsonp_urls(html);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].0, 3);
        assert_eq!(urls[1].0, 4);
        assert!(urls[0].1.ends_with("/pages/3-abc123.jsonp"));
    }

    #[test]
    fn test_unwrap_jsonp() {
        let body = "window.page4_callback([\"<div class=\\\"newpage\\\" id=\\\"page4\\\">hi</div>\"]);\n";
        assert_eq!(
            unwrap_jsonp(body).as_deref(),
            Some("<div class=\"newpage\" id=\"page4\">hi</div>")
        );
        // Different callback name + trailing whitespace is fine.
        let body2 = "window.page12_callback([\"text\"]);   ";
        assert_eq!(unwrap_jsonp(body2).as_deref(), Some("text"));
        assert!(unwrap_jsonp("no json here").is_none());
    }

    #[test]
    fn test_decode_jsonp_body() {
        // Plain text passes through.
        let plain = b"window.page1_callback([\"x\"])";
        assert_eq!(
            decode_jsonp_body(plain).unwrap(),
            "window.page1_callback([\"x\"])"
        );
        // Gzip magic bytes trigger decompression.
        let mut enc = flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        );
        use std::io::Write;
        enc.write_all(b"hello gzip").unwrap();
        let gz = enc.finish().unwrap();
        assert!(gz.starts_with(&[0x1f, 0x8b]));
        assert_eq!(decode_jsonp_body(&gz).unwrap(), "hello gzip");
    }

    #[test]
    fn test_page_text_from_html_reading_order() {
        let html = r#"<div class="text_layer">
<div class="ff1"><span style="left:1500px;top:569px">MOOT</span><span style="left:1301px;top:569px">THE ICC</span><span style="left:1700px;top:569px">COURT </span></div>
<div class="ff1"><span style="left:100px;top:700px">Second line here</span></div>
<div class="ff1"><span style="left:100px;top:569px">LEAD</span></div>
</div>"#;
        // Spans arrive out of order in the HTML; output must be sorted
        // by (top, left): LEAD / THE ICC / MOOT / COURT on line 1.
        assert_eq!(
            page_text_from_html(html),
            "LEAD THE ICC MOOT COURT\nSecond line here"
        );
    }

    #[test]
    fn test_page_text_from_html_entities_and_empty() {
        let html = r#"<div class="text_layer"><span style="left:10px;top:10px">Fish &amp; Chips &#233; &#x2014;</span></div>"#;
        assert_eq!(page_text_from_html(html), "Fish & Chips é —");
        // No spans at all -> empty (image-only page).
        assert_eq!(page_text_from_html("<div class=\"text_layer\"></div>"), "");
        // A span without coordinates cannot be ordered -> skipped.
        assert_eq!(
            page_text_from_html("<span>floating text</span>"),
            ""
        );
    }

    #[test]
    fn test_cmap_glyph_ids() {
        // Glyph ids verified against the vendored font with an
        // independent Python cmap parser (spec offsets: endCode @+14,
        // startCode @+16+2*seg, idDelta @+16+4*seg, idRangeOffset
        // @+16+6*seg — an earlier probe with a +2 shift gave wrong ids).
        let font = EmbeddedFont::parse(EMBEDDED_FONT).expect("font parses");
        assert_eq!(font.units_per_em, 1000);
        assert_eq!(font.glyph_id('A' as u32), 1);
        assert_eq!(font.glyph_id('a' as u32), 156);
        assert_eq!(font.glyph_id(' ' as u32), 401);
        assert_eq!(font.glyph_id('?' as u32), 413);
        assert_eq!(font.glyph_id('đ' as u32), 188);
        assert_eq!(font.glyph_id('Đ' as u32), 33);
        assert_eq!(font.glyph_id('ơ' as u32), 249);
        assert_eq!(font.glyph_id('ư' as u32), 282);
        assert_eq!(font.glyph_id('ộ' as u32), 241);
        assert_eq!(font.glyph_id('ữ' as u32), 287);
        assert_eq!(font.glyph_id('à' as u32), 172);
        assert_eq!(font.advance(1), 736);
        assert_eq!(font.advance(188), 662);
        assert_eq!(font.glyph_id(0xFFFF), 0);
    }

    #[test]
    fn test_extract_newpage_size() {
        // JSONP inner HTML (plain quotes).
        let jsonp = r#"<div class="newpage" id="page4" style="width: 902px; height:1167px">"#;
        assert_eq!(extract_newpage_size(jsonp), Some((902, 1167)));
        // Inline chunk (quote-escaped inside the embed JS string).
        let inline = r#"<div class=\"newpage\" id=\"page1\" style=\"width: 902px; height:1167px\">"#;
        assert_eq!(extract_newpage_size(inline), Some((902, 1167)));
        assert_eq!(extract_newpage_size("<div>no page here</div>"), None);
    }

    #[test]
    fn test_extract_image_fragments() {
        // JSONP inner HTML (plain quotes): background fragment + one overlay.
        let jsonp = r#"<div class="image_layer" style="z-index: 1"><div class="ie_fix"><img class="absimg" style="left:-1px;top:-1px;width:904px;height:2259px;clip:rect(1px 903px 1168px 1px)" orig="http://html.scribd.com/34t2dbw9kw8500ws/images/1-64298bd981.png"/></div></div><div class="image_layer" style="z-index: 3"><div class="ie_fix"><img class="absimg" style="left:-162px;top:-1047px;width:904px;height:2378px;clip:rect(1170px 255px 1252px 162px)" orig="http://html.scribd.com/34t2dbw9kw8500ws/images/4-481b69ec33.png"/></div></div>"#;
        let frags = extract_image_fragments(jsonp);
        assert_eq!(frags.len(), 2);
        assert_eq!(
            frags[0].0,
            "http://html.scribd.com/34t2dbw9kw8500ws/images/1-64298bd981.png"
        );
        // Background: the clip starts 1px inside the sprite, so the
        // visible region begins at display (0,0) 902x1167 — the img
        // sits at (-1,-1) but its first clipped pixel lands at
        // (0,0), covering the whole 902x1167 page. Sprite src
        // (1,1). Same rule as the overlay below: display = left +
        // clip_left, src = clip offset.
        assert_eq!(frags[0].1.display_x, 0);
        assert_eq!(frags[0].1.display_y, 0);
        assert_eq!(frags[0].1.display_w, 902);
        assert_eq!(frags[0].1.display_h, 1167);
        assert_eq!(frags[0].1.src_x, 1);
        assert_eq!(frags[0].1.src_y, 1);
        // Overlay: clip (1170,255)-(1252,162) at left -162 / top -1047
        // -> display (0,123) 93x82, sprite src (162,1170).
        assert_eq!(frags[1].1.display_x, 0);
        assert_eq!(frags[1].1.display_y, 123);
        assert_eq!(frags[1].1.display_w, 93);
        assert_eq!(frags[1].1.display_h, 82);
        assert_eq!(frags[1].1.src_x, 162);
        assert_eq!(frags[1].1.src_y, 1170);
        // Inline chunk (quote-escaped) parses the same way.
        let inline = jsonp.replace('"', "\\\"");
        assert_eq!(extract_image_fragments(&inline).len(), 2);
        // Empty clip is skipped.
        let empty = r#"<img class="absimg" style="left:0px;top:0px;width:904px;height:100px;clip:rect(5px 10px 5px 10px)" orig="http://x/y.png"/>"#;
        assert!(extract_image_fragments(empty).is_empty());
    }

    #[test]
    fn test_first_page_thumbnail_url() {
        // Page 1 inline with an image_layer: the first sprite, http
        // upgraded to https.
        let html = r##"<!doctype html><html><body>
<div class="newpage" id="page1"><div class="image_layer" style="z-index: 1"><div class="ie_fix"><img class="absimg" style="left:-1px;top:-1px;width:904px;height:2259px;clip:rect(1px 903px 1168px 1px)" orig="http://html.scribd.com/34t2dbw9kw8500ws/images/1-64298bd981.png"/></div></div></div>
<div class="newpage" id="page2"><div class="image_layer"><div class="ie_fix"><img class="absimg" style="left:-1px;top:-1px;width:904px;height:2259px;clip:rect(1px 903px 1168px 1px)" orig="http://html.scribd.com/34t2dbw9kw8500ws/images/2-aaaa0000.jpg"/></div></div></div>
</body></html>"##;
        assert_eq!(
            first_page_thumbnail_url(html),
            Some(
                "https://html.scribd.com/34t2dbw9kw8500ws/images/1-64298bd981.png"
                    .to_string()
            )
        );
        // An already-https sprite passes through unchanged.
        let https_html = html.replacen(
            "http://html.scribd.com",
            "https://html.scribd.com",
            1,
        );
        assert_eq!(
            first_page_thumbnail_url(&https_html),
            Some(
                "https://html.scribd.com/34t2dbw9kw8500ws/images/1-64298bd981.png"
                    .to_string()
            )
        );
        // Page 1 text-only, page 2 illustrated: falls back to the
        // first sprite anywhere in the HTML.
        let text_first = r##"<!doctype html><html><body>
<div class="newpage" id="page1"><div class="text_layer"><span>hello</span></div></div>
<div class="newpage" id="page2"><div class="image_layer"><div class="ie_fix"><img class="absimg" style="left:-1px;top:-1px;width:904px;height:2259px;clip:rect(1px 903px 1168px 1px)" orig="http://html.scribd.com/34t2dbw9kw8500ws/images/2-aaaa0000.jpg"/></div></div></div>
</body></html>"##;
        assert_eq!(
            first_page_thumbnail_url(text_first),
            Some(
                "https://html.scribd.com/34t2dbw9kw8500ws/images/2-aaaa0000.jpg"
                    .to_string()
            )
        );
        // No page image at all (the EMBED_HTML fixture): None.
        assert_eq!(first_page_thumbnail_url(EMBED_HTML), None);
    }

    #[test]
    fn test_jpeg_info() {
        // Minimal header-only JPEG: SOI + APP0 + SOF0 (3 components,
        // 2x3). No image data needed for the header parse.
        // The APP0 payload is exactly 14 bytes: the length field
        // 0x0010 counts 2 length bytes + 14 payload bytes. (An
        // earlier version of this test had one extra trailing NUL
        // — 15 payload bytes — which shifted the SOF0 marker by
        // one byte, so the strict parser correctly returned None.)
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        bytes.extend_from_slice(&[
            0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x02, 0x00, 0x03, 0x03, 0x01, 0x22,
            0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
        ]);
        assert_eq!(jpeg_info(&bytes), Some((3, 2, 3)));
        assert!(jpeg_info(b"not a jpeg").is_none());
        assert!(jpeg_info(&[0xFF, 0xD8, 0xFF, 0xDA]).is_none()); // SOS, no SOF
        // A length field that disagrees with the payload is corrupt:
        // one extra NUL inside APP0 must be rejected, not tolerated.
        let mut corrupt = bytes.clone();
        corrupt.insert(19, 0);
        assert!(jpeg_info(&corrupt).is_none());
    }

    // Tiny PNGs generated with node (zlib deflate + real CRCs), see
    // /tmp/scribd-probe/gen-test-png.mjs in the probe session.
    const PNG_2X2_RGB: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0, 253, 212, 154, 115, 0, 0, 0, 20, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 192, 0, 194, 12, 255, 255, 255, 255, 15, 0, 31, 238, 5, 251, 176, 184, 151, 201, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
    const PNG_2X2_RGBA: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 21, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 159, 225, 63, 67, 3, 35, 152, 6, 50, 1, 61, 226, 7, 123, 177, 111, 175, 117, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
    const PNG_2X2_PALETTE_ALPHA: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 3, 0, 0, 0, 69, 104, 253, 22, 0, 0, 0, 12, 80, 76, 84, 69, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 155, 192, 19, 220, 0, 0, 0, 4, 116, 82, 78, 83, 255, 128, 255, 0, 31, 135, 134, 153, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 96, 96, 100, 96, 98, 6, 0, 0, 17, 0, 7, 158, 162, 42, 18, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];

    #[test]
    fn test_decode_sprite_png() {
        // 2x2 RGB: red, green / blue, white.
        let (w, h, kind) = decode_sprite(PNG_2X2_RGB).expect("rgb png");
        assert_eq!((w, h), (2, 2));
        match kind {
            ScribdImageKind::Rgba(px) => assert_eq!(
                px,
                vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]
            ),
            other => panic!("expected Rgba, got {:?}", other),
        }
        // 2x2 RGBA: red opaque, green 50% / blue transparent (Sub
        // filter on row 2), white opaque.
        let (_, _, kind) = decode_sprite(PNG_2X2_RGBA).expect("rgba png");
        match kind {
            ScribdImageKind::Rgba(px) => assert_eq!(
                px,
                vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255]
            ),
            other => panic!("expected Rgba, got {:?}", other),
        }
        // 2x2 palette + tRNS: black opaque, red 50% / green opaque,
        // blue fully transparent.
        let (_, _, kind) = decode_sprite(PNG_2X2_PALETTE_ALPHA).expect("palette png");
        match kind {
            ScribdImageKind::Rgba(px) => assert_eq!(
                px,
                vec![0, 0, 0, 255, 255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 0]
            ),
            other => panic!("expected Rgba, got {:?}", other),
        }
        assert!(decode_sprite(b"not an image").is_err());
    }

    #[test]
    fn test_build_text_pdf_with_image() {
        // One page with a 2x2 RGBA sprite (one semi-transparent
        // pixel) drawn full-page, plus one page with a dummy JPEG
        // sprite covering the whole page. The text is drawn VISIBLE
        // on both: the sprite never carries the body text (verified
        // on doc 477317150 — its full-page JPEG is background +
        // diagram only), so hiding it would blank the page.
        // The pages are real-sized (902x1167): a tiny page leaves
        // a negative text width after the 36pt margin, so the
        // reflow splits each short text across several PDF pages
        // and /Count is no longer 2.
        let rgba_page = ScribdPage {
            num: 1,
            width: 902,
            height: 1167,
            text: "Hello image".to_string(),
            has_text: true,
            spans: vec![],
            images: vec![ScribdPageImage {
                width: 2,
                height: 2,
                kind: ScribdImageKind::Rgba(vec![
                    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
                ]),
                fragments: vec![ScribdImageFragment {
                    display_x: 0,
                    display_y: 0,
                    display_w: 4,
                    display_h: 4,
                    src_x: 0,
                    src_y: 0,
                }],
            }],
        };
        let jpeg_page = ScribdPage {
            num: 2,
            width: 902,
            height: 1167,
            text: "Rendered page text".to_string(),
            has_text: true,
            spans: vec![],
            images: vec![ScribdPageImage {
                width: 2,
                height: 2,
                kind: ScribdImageKind::Jpeg {
                    bytes: vec![0xFF, 0xD8, 0xFF, 0xD9],
                    gray: false,
                },
                fragments: vec![ScribdImageFragment {
                    display_x: 0,
                    display_y: 0,
                    display_w: 4,
                    display_h: 4,
                    src_x: 0,
                    src_y: 0,
                }],
            }],
        };
        let pdf = build_text_pdf(&[rgba_page, jpeg_page]).expect("pdf builds");
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Subtype /Image"));
        // RGBA sprite with a transparent pixel -> soft mask.
        assert!(text.contains("/SMask"));
        // JPEG sprite embeds its raw DCT stream.
        assert!(text.contains("/Filter /DCTDecode"));
        assert!(text.contains("/ColorSpace /DeviceRGB"));
        // Both images are drawn (two Do operators, two XObject names).
        assert!(text.contains("/Im8 Do"));
        assert!(text.contains("/Im10 Do"));
        // No invisible-text mode anywhere: the text stays visible
        // even over a full-page JPEG sprite.
        assert!(!text.contains("3 Tr"));
        assert!(text.contains("/Count 2"));
        // xref still valid.
        let startxref: usize = text
            .split("startxref\n")
            .nth(1)
            .and_then(|s| s.lines().next())
            .and_then(|s| s.trim().parse().ok())
            .expect("startxref");
        assert_eq!(&pdf[startxref..startxref + 4], b"xref");
    }

    #[test]
    fn test_build_text_pdf() {
        let pages = vec![
            ScribdPage {
                num: 1,
                width: 902,
                height: 1274,
                text: "Hello Scribd world".to_string(),
                has_text: true,
                spans: vec![],
                images: vec![],
            },
            ScribdPage {
                num: 2,
                width: 902,
                height: 1274,
                text: "[Trang 2: trang ảnh, không trích xuất được chữ]".to_string(),
                has_text: false,
                spans: vec![],
                images: vec![],
            },
        ];
        let pdf = build_text_pdf(&pages).expect("pdf builds");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/CIDFontType2"));
        assert!(text.contains("/Identity-H"));
        assert!(text.contains("/ToUnicode"));
        assert!(text.contains("/FontFile2"));
        assert!(text.contains("/BeVietnamPro-Regular"));
        assert!(text.contains("/Count 2"));
        // "Hello" as glyph ids: H=57 e=189 l=228 l=228 o=237.
        assert!(text.contains("003900BD00E400E400ED"));
        // xref table present and startxref points at it.
        let startxref: usize = text
            .split("startxref\n")
            .nth(1)
            .and_then(|s| s.lines().next())
            .and_then(|s| s.trim().parse().ok())
            .expect("startxref");
        assert_eq!(&pdf[startxref..startxref + 4], b"xref");
        // Vietnamese renders as real glyphs, not '?' (được = đ 188,
        // ư 282, ợ 251, c 180) and the ToUnicode map keeps it searchable.
        let vn = build_text_pdf(&[ScribdPage {
            num: 1,
            width: 902,
            height: 1274,
            text: "được".to_string(),
            has_text: true,
            spans: vec![],
            images: vec![],
        }])
        .unwrap();
        let vn_text = String::from_utf8_lossy(&vn);
        assert!(vn_text.contains("00BC011A00FB00B4"));
        assert!(vn_text.contains("<00BC> <0111>"));
        assert!(!vn_text.contains("???c"));
    }

    #[test]
    fn test_build_text_pdf_overflow_flows() {
        // A very long page must flow onto extra PDF pages, not clip.
        // "word " x 3000 wraps to 177 lines at 11pt ("word" is 28.12pt
        // wide, 'w' alone 870 units) and 55 lines fit per A4 page, so
        // the PDF has exactly 4 pages.
        let long_text = "word ".repeat(3000);
        let pdf = build_text_pdf(&[ScribdPage {
            num: 1,
            width: 595,
            height: 842,
            text: long_text,
            has_text: true,
            spans: vec![],
            images: vec![],
        }])
        .unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 4"));
    }

    #[test]
    fn test_extract_text_layer_scale() {
        let css = ".link_layer,.text_layer{width:0;height:0;position:absolute;top:0;left:0}.text_layer{transform:scale(.2);transform-origin:top left}";
        assert!((extract_text_layer_scale(css) - 0.2).abs() < 1e-9);
        assert!((extract_text_layer_scale("<html>no css</html>") - 0.2).abs() < 1e-9);
        assert!((extract_text_layer_scale(".text_layer{transform:scale(1)}") - 1.0).abs() < 1e-9);
        // Unparsable or out-of-range scale falls back to 0.2.
        assert!((extract_text_layer_scale(".text_layer{transform:scale(abc)}") - 0.2).abs() < 1e-9);
        assert!((extract_text_layer_scale(".text_layer{transform:scale(2.5)}") - 0.2).abs() < 1e-9);
    }

    #[test]
    fn test_page_spans_from_html() {
        // Real page-4 structure (ff divs, colours, spacing, a link).
        let html = r#"<div class="newpage" id="page4" style="width: 902px; height:1167px">
<div class=text_layer style="z-index:2"><div class=ie_fix>
<div class="ff3" style="font-size:95px">
<span class=a style="left:530px;top:734px;word-spacing:-2px;color:#365f91">FREE English </span><span class=a style="left:530px;top:874px;letter-spacing:-1px">Lessons - </span></div>
<div class="ff0" style="font-size:73px">
<span class=a style="left:2068px;top:5394px">Halaman_4 </span><span class=a style="left:100px;top:100px"><a class=ll orig="eA==">about</a> </span></div>
</div></div>"#;
        let spans = page_spans_from_html(html, 0.2);
        assert_eq!(spans.len(), 4);
        // FREE English: left 530*0.2=106, top 734*0.2=146.8,
        // size 95*0.2=19, colour #365f91.
        assert!((spans[0].left - 106.0).abs() < 1e-9);
        assert!((spans[0].top - 146.8).abs() < 1e-9);
        assert!((spans[0].font_size - 19.0).abs() < 1e-9);
        assert_eq!(spans[0].text, "FREE English ");
        assert_eq!(
            spans[0].color,
            (54.0 / 255.0, 95.0 / 255.0, 145.0 / 255.0)
        );
        // Lessons: letter-spacing -1 span px -> -0.2 display px.
        assert!((spans[1].letter_spacing - (-0.2)).abs() < 1e-9);
        // Halaman_4: ff0 73px -> 14.6 display px.
        assert!((spans[2].font_size - 14.6).abs() < 1e-9);
        assert!((spans[2].left - 413.6).abs() < 1e-9);
        // Link span: the inner <a> tag is stripped, text kept.
        assert_eq!(spans[3].text, "about ");
        // A span without coordinates -> the whole page falls back.
        let bad = r#"<div class="ff1" style="font-size:80px"><span class=a style="left:10px;top:10px">ok</span><span class=a>no coords</span></div>"#;
        assert!(page_spans_from_html(bad, 0.2).is_empty());
        // A span before any ff div (no font size) -> empty.
        let bad2 = r#"<span class=a style="left:10px;top:10px">orphan</span>"#;
        assert!(page_spans_from_html(bad2, 0.2).is_empty());
        // Empty-text spans are skipped, not fatal.
        let empty = r#"<div class="ff1" style="font-size:80px"><span class=a style="left:10px;top:10px"> </span><span class=a style="left:20px;top:10px">hi</span></div>"#;
        assert_eq!(page_spans_from_html(empty, 0.2).len(), 1);
        // Nested marker spans (w7/l6) inside a positioned span are
        // consumed as part of the parent's text, not spans of their
        // own — this is the real page-3 shape (doc 477317150).
        let nested = r#"<div class="ff3" style="font-size:81px"><span class=a style="left:530px;top:3483px;word-spacing:-1px">It means that you will need to <span class=w7></span>listen to every example sentence many, many t<span class=w7></span>imes, until y<span class=l6>ou </span></span><span class=a style="left:530px;top:3586px">MASTER it. Don't listen just one time or a <span class=w6></span>few times. It's not enough. Yo<span class=w6></span>u will f<span class=l6>orget <span class=w6></span>it very </span></span></div>"#;
        let spans = page_spans_from_html(nested, 0.2);
        assert_eq!(spans.len(), 2);
        assert_eq!(
            spans[0].text,
            "It means that you will need to listen to every example sentence many, many times, until you "
        );
        assert_eq!(
            spans[1].text,
            "MASTER it. Don't listen just one time or a few times. It's not enough. You will forget it very "
        );
        assert!((spans[0].top - 696.6).abs() < 1e-9);
    }

    #[test]
    fn test_page_spans_table_cell() {
        // Real table-cell span from doc 254039756 page 2 (nested
        // `<span class=l>` markers, one `<span class=w>` spacer):
        // the outer span's letter-spacing is cancelled by the
        // nested margins, and the words are separated by the
        // spacer — the PDF must draw the words at their true
        // column positions with no letter spreading.
        let html = r#"<div class="ff0" style="font-size:88px"><span class=a style="left:478px;top:1334px;word-spacing:265px;letter-spacing:37px">J<span class=l style="margin-left:-38px">V<span class=l style="margin-left:-37px">0<span class=l style="margin-left:-37px">0<span class=l style="margin-left:-38px">1 <span class=w style="width:262px"></span>A<span class=l style="margin-left:-38px">b<span class=l style="margin-left:-37px">e<span class=l style="margin-left:-37px">t</span></span></span></span></span></span></span></span></div>"#;
        let spans = page_spans_from_html(html, 0.2);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "JV001 Abet");
        // Letter-spacing cancelled by the l margins (was 37px,
        // which used to come out as Tc 4.88pt: "J V 0 0 1").
        assert!((spans[0].letter_spacing - 0.0).abs() < 1e-9);
        assert!((spans[0].left - 95.6).abs() < 1e-9);
        assert!((spans[0].top - 266.8).abs() < 1e-9);
        assert!((spans[0].font_size - 17.6).abs() < 1e-9);
        assert_eq!(spans[0].word_runs.len(), 2);
        assert_eq!(spans[0].word_runs[0].text, "JV001");
        assert!((spans[0].word_runs[0].gap_before - 0.0).abs() < 1e-9);
        assert_eq!(spans[0].word_runs[1].text, "Abet");
        // Gap = (262 + 0.25*88 + 265 + 37 - 38) * 0.2 = 109.6
        // display px: spacer + space + word-spacing + the
        // letter-spacing residual (ls + innermost l margin).
        assert!((spans[0].word_runs[1].gap_before - 109.6).abs() < 1e-9);

        // Real header span (same page): two spacers. The first
        // follows "Episode" with no space before it, so its gap
        // is the spacer width only; the second follows "Word "
        // with a space before it, so its gap also carries the
        // space advance + word-spacing + residual.
        let header = r#"<div class="ff0" style="font-size:86px"><span class=a style="left:478px;top:1039px;word-spacing:96px;letter-spacing:13px">J<span class=l style="margin-left:-14px">V <span class=l style="margin-left:-122px">E<span class=l style="margin-left:-13px">p<span class=l style="margin-left:-13px">i<span class=l style="margin-left:-13px">s<span class=l style="margin-left:-14px">o<span class=l style="margin-left:-13px">d<span class=l style="margin-left:-14px">e<span class=w style="width:381px"></span>W<span class=l style="margin-left:-16px">o<span class=l style="margin-left:-13px">r<span class=l style="margin-left:-14px">d <span class=w style="width:436px"></span>P<span class=l style="margin-left:-14px">a<span class=l style="margin-left:-13px">r<span class=l style="margin-left:-14px">t <span class=l style="margin-left:-122px">o<span class=l style="margin-left:-14px">f <span class=l style="margin-left:-122px">w<span class=l style="margin-left:-13px">h<span class=l style="margin-left:-14px">i<span class=l style="margin-left:-13px">c<span class=l style="margin-left:-13px">h <span class=l style="margin-left:-122px">v<span class=l style="margin-left:-14px">o<span class=l style="margin-left:-13px">l<span class=l style="margin-left:-13px">u<span class=l style="margin-left:-13px">m<span class=l style="margin-left:-15px">e<span class=l style="margin-left:-13px">:</span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></span></div>"#;
        let spans = page_spans_from_html(header, 0.2);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].word_runs.len(), 3);
        assert_eq!(spans[0].word_runs[0].text, "JV Episode");
        assert_eq!(spans[0].word_runs[1].text, "Word");
        assert_eq!(spans[0].word_runs[2].text, "Part of which volume:");
        // Gap 1 = 381 * 0.2 = 76.2 (no space before the spacer).
        assert!((spans[0].word_runs[1].gap_before - 76.2).abs() < 1e-9);
        // Gap 2 = (436 + 0.25*86 + 96 + 13 - 14) * 0.2 = 110.5.
        assert!((spans[0].word_runs[2].gap_before - 110.5).abs() < 1e-9);
        assert!((spans[0].letter_spacing - 0.0).abs() < 1e-9);

        // A plain span (no l/w markers) keeps its letter-spacing
        // and has no word runs — existing behaviour unchanged.
        let plain = r#"<div class="ff3" style="font-size:95px"><span class=a style="left:530px;top:734px;letter-spacing:-1px">Hi</span></div>"#;
        let spans = page_spans_from_html(plain, 0.2);
        assert_eq!(spans.len(), 1);
        assert!((spans[0].letter_spacing - (-0.2)).abs() < 1e-9);
        assert!(spans[0].word_runs.is_empty());
    }

    #[test]
    fn test_build_text_pdf_word_runs() {
        // A table-cell span draws each word run at its own x (same
        // baseline, Tc 0): the words land in their true columns
        // instead of collapsing into one run.
        let font = EmbeddedFont::parse(EMBEDDED_FONT).expect("font parses");
        let s = 595.0 / 902.0;
        let ph = 1167.0 * s;
        let size_pt = 17.6 * s;
        let y = ph - (266.8 + 17.6 * 0.8675) * s;
        let page = ScribdPage {
            num: 1,
            width: 902,
            height: 1167,
            text: "JV001 Abet".to_string(),
            has_text: true,
            spans: vec![ScribdTextSpan {
                left: 95.6,
                top: 266.8,
                font_size: 17.6,
                text: "JV001 Abet".to_string(),
                color: (0.0, 0.0, 0.0),
                letter_spacing: 0.0,
                word_runs: vec![
                    ScribdWordRun {
                        gap_before: 0.0,
                        text: "JV001".to_string(),
                    },
                    ScribdWordRun {
                        gap_before: 109.6,
                        text: "Abet".to_string(),
                    },
                ],
            }],
            images: vec![],
        };
        let pdf = build_text_pdf(&[page]).expect("pdf builds");
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 1"));
        // Run 1 at the span's left, Tc 0.
        let run1 = format!(
            "/F1 {} Tf {} Tc {} {} {} rg 1 0 0 1 {} {} Tm ",
            pdf_num(size_pt),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(95.6 * s),
            pdf_num(y),
        );
        assert!(text.contains(&run1));
        // Run 2 on the same baseline, x = (left + gap) * s plus the
        // measured width of run 1 (already in points).
        let x2 = (95.6 + 109.6) * s + font.text_width_pt("JV001", size_pt);
        let run2 = format!(
            "/F1 {} Tf {} Tc {} {} {} rg 1 0 0 1 {} {} Tm ",
            pdf_num(size_pt),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(x2),
            pdf_num(y),
        );
        assert!(text.contains(&run2));
    }

    #[test]
    fn test_build_docx() {
        // A real 2x2 JPEG (encoded here with the image crate) and a
        // 2x2 RGBA sprite: both image paths must land in the package.
        let mut jpeg_bytes = Vec::new();
        {
            let mut enc = image::codecs::jpeg::JpegEncoder::new(&mut jpeg_bytes);
            enc.encode(
                &[255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
                2,
                2,
                image::ExtendedColorType::Rgb8,
            )
            .expect("jpeg encodes");
        }
        assert!(!jpeg_bytes.is_empty());
        let rgba = vec![
            0u8, 0, 255, 255, // blue, opaque
            0, 255, 0, 128, // green, semi-transparent
            255, 255, 255, 255, // white, opaque
            0, 0, 0, 255, // black, opaque
        ];
        let pages = vec![
            ScribdPage {
                num: 1,
                width: 902,
                height: 1167,
                text: "Xin chào Việt Nam".to_string(),
                has_text: true,
                spans: vec![
                    ScribdTextSpan {
                        left: 95.6,
                        top: 100.0,
                        font_size: 17.6,
                        text: "Xin chào Việt Nam".to_string(),
                        color: (0.0, 0.0, 0.0),
                        letter_spacing: 0.0,
                        word_runs: vec![],
                    },
                    ScribdTextSpan {
                        left: 95.6,
                        top: 266.8,
                        font_size: 17.6,
                        text: "JV001 Abet".to_string(),
                        color: (0.0, 0.0, 0.0),
                        letter_spacing: 0.0,
                        word_runs: vec![
                            ScribdWordRun {
                                gap_before: 0.0,
                                text: "JV001".to_string(),
                            },
                            ScribdWordRun {
                                gap_before: 109.6,
                                text: "Abet".to_string(),
                            },
                        ],
                    },
                ],
                images: vec![ScribdPageImage {
                    width: 2,
                    height: 2,
                    kind: ScribdImageKind::Rgba(rgba),
                    fragments: vec![],
                }],
            },
            ScribdPage {
                num: 2,
                width: 902,
                height: 1167,
                text: "Dòng hai\nDòng ba".to_string(),
                has_text: true,
                spans: vec![],
                images: vec![ScribdPageImage {
                    width: 2,
                    height: 2,
                    kind: ScribdImageKind::Jpeg {
                        bytes: jpeg_bytes,
                        gray: false,
                    },
                    fragments: vec![],
                }],
            },
        ];
        let docx = build_docx(&pages, "Tài liệu thử — Tiếng Việt").expect("docx builds");
        let mut zip = zip::ZipArchive::new(Cursor::new(docx)).expect("docx is a valid zip");
        for name in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/_rels/document.xml.rels",
            "word/styles.xml",
            "word/media/image1.png",
            "word/media/image2.jpg",
        ] {
            assert!(zip.by_name(name).is_ok(), "missing part {name}");
        }
        let mut doc_xml = String::new();
        zip.by_name("word/document.xml")
            .expect("document.xml")
            .read_to_string(&mut doc_xml)
            .expect("document.xml reads");
        // Title + Vietnamese diacritics survive as UTF-8.
        assert!(doc_xml.contains("Tài liệu thử — Tiếng Việt"));
        assert!(doc_xml.contains("Xin chào Việt Nam"));
        // Table-cell span became one Word table, one cell per run.
        assert!(doc_xml.contains("<w:tbl>"));
        assert!(doc_xml.contains("JV001"));
        assert!(doc_xml.contains("Abet"));
        // Reflow fallback page: one paragraph per line.
        assert!(doc_xml.contains("Dòng hai"));
        assert!(doc_xml.contains("Dòng ba"));
        // Both images referenced, and a page break between the pages.
        assert!(doc_xml.contains("r:embed=\"rId2\""));
        assert!(doc_xml.contains("r:embed=\"rId3\""));
        assert!(doc_xml.contains("w:type=\"page\""));
        // Media parts hold real image bytes, not empty placeholders.
        let mut png = Vec::new();
        zip.by_name("word/media/image1.png")
            .expect("image1")
            .read_to_end(&mut png)
            .expect("image1 reads");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let mut jpg = Vec::new();
        zip.by_name("word/media/image2.jpg")
            .expect("image2")
            .read_to_end(&mut jpg)
            .expect("image2 reads");
        assert!(jpg.starts_with(b"\xff\xd8"));
    }

    #[test]
    fn test_scribd_format_ext() {
        assert_eq!(scribd_format_ext("docx"), "docx");
        assert_eq!(scribd_format_ext("DOCX"), "docx");
        assert_eq!(scribd_format_ext("pdf"), "pdf");
        assert_eq!(scribd_format_ext("epub"), "pdf");
        assert_eq!(scribd_format_ext(""), "pdf");
    }

    #[test]
    fn test_build_text_pdf_positioned() {
        // Baseline factor from the embedded font (ascent 1000,
        // descent -265, upem 1000, line-height 1em -> 0.8675).
        let font = EmbeddedFont::parse(EMBEDDED_FONT).expect("font parses");
        assert!((baseline_offset_em(&font) - 0.8675).abs() < 1e-9);

        let page = ScribdPage {
            num: 1,
            width: 902,
            height: 1167,
            text: "Hello World".to_string(),
            has_text: true,
            spans: vec![
                ScribdTextSpan {
                    left: 106.0,
                    top: 146.8,
                    font_size: 19.0,
                    text: "Hello".to_string(),
                    color: (54.0 / 255.0, 95.0 / 255.0, 145.0 / 255.0),
                    letter_spacing: 0.0,
                    word_runs: vec![],
                },
                ScribdTextSpan {
                    left: 200.0,
                    top: 200.0,
                    font_size: 16.0,
                    text: "World".to_string(),
                    color: (0.0, 0.0, 0.0),
                    letter_spacing: -0.2,
                    word_runs: vec![],
                },
            ],
            images: vec![],
        };
        let pdf = build_text_pdf(&[page]).expect("pdf builds");
        let text = String::from_utf8_lossy(&pdf);
        // One Scribd page -> exactly one PDF page (no reflow overflow).
        assert!(text.contains("/Count 1"));
        // No reflow operators: positions come from Tm, not Td/T*.
        assert!(!text.contains(" Td "));
        assert!(!text.contains("14 TL"));
        // Span 1 operators: size 19*s, Tc 0, colour #365f91, Tm at
        // (106*s, ph - (146.8 + 19*0.8675)*s).
        let s = 595.0 / 902.0;
        let ph = 1167.0 * s;
        let y1 = ph - (146.8 + 19.0 * 0.8675) * s;
        let expected1 = format!(
            "/F1 {} Tf {} Tc {} {} {} rg 1 0 0 1 {} {} Tm ",
            pdf_num(19.0 * s),
            pdf_num(0.0),
            pdf_num(54.0 / 255.0),
            pdf_num(95.0 / 255.0),
            pdf_num(145.0 / 255.0),
            pdf_num(106.0 * s),
            pdf_num(y1),
        );
        assert!(text.contains(&expected1));
        // Span 2: letter-spacing -0.2 display px -> Tc = -0.2*s pt.
        assert!(text.contains(&format!("{} Tc ", pdf_num(-0.2 * s))));
        // "Hello" glyph ids (H=57 e=189 l=228 l=228 o=237) — the
        // positioned text stays searchable.
        assert!(text.contains("003900BD00E400E400ED"));
        // xref still valid.
        let startxref: usize = text
            .split("startxref\n")
            .nth(1)
            .and_then(|s| s.lines().next())
            .and_then(|s| s.trim().parse().ok())
            .expect("startxref");
        assert_eq!(&pdf[startxref..startxref + 4], b"xref");
    }

    #[test]
    fn test_build_text_pdf_positioned_full_page_jpeg_visible() {
        // Full-page JPEG sprite + spans: the text is drawn VISIBLE
        // at its exact position (Tm present, no 3 Tr). The sprite
        // holds background + inline images only, never the body
        // text — hiding the text here used to blank the whole page
        // (doc 477317150 page 3).
        let page = ScribdPage {
            num: 1,
            width: 4,
            height: 4,
            text: "Rendered page text".to_string(),
            has_text: true,
            spans: vec![ScribdTextSpan {
                left: 1.0,
                top: 1.0,
                font_size: 2.0,
                text: "Hi".to_string(),
                color: (0.0, 0.0, 0.0),
                letter_spacing: 0.0,
                word_runs: vec![],
            }],
            images: vec![ScribdPageImage {
                width: 2,
                height: 2,
                kind: ScribdImageKind::Jpeg {
                    bytes: vec![0xFF, 0xD8, 0xFF, 0xD9],
                    gray: false,
                },
                fragments: vec![ScribdImageFragment {
                    display_x: 0,
                    display_y: 0,
                    display_w: 4,
                    display_h: 4,
                    src_x: 0,
                    src_y: 0,
                }],
            }],
        };
        let pdf = build_text_pdf(&[page]).expect("pdf builds");
        let text = String::from_utf8_lossy(&pdf);
        assert!(!text.contains("3 Tr"));
        assert!(text.contains(" Tm "));
        assert!(text.contains("/Count 1"));
    }
}
