//! Studocu documents (free-preview page rasters): the document page is a
//! Next.js page whose `#__NEXT_DATA__` JSON carries the access model, and
//! the viewer renders each page from the `doc-assets.studocu.com` CDN.
//! No JS engine needed — plain HTTP + JSON.
//!
//! Live structure (measured against the live site, August 2026, and
//! corroborated by Studocu's own search-indexed preview text):
//! the viewer is pdf2htmlEX in split-page mode — one `.pf` element per
//! page, a figure-layer background image plus a positioned HTML text
//! layer. Assets live at `https://doc-assets.studocu.com/{objectKey}/html/`:
//!
//! | Asset | Path | Numbering | Notes |
//! | --- | --- | --- | --- |
//! | Figure layer | `bg{n}.png` | hex (`bg12.png` is page 18) | Rules, table borders, coloured boxes. No text — except on scanned/image documents, where the background IS the page content. |
//! | Text layer | `{objectKey}{n}.page` | decimal | Positioned `<span>`s, signed per page. Not used here. |
//! | Blurred preview | `pages/blurred/page{n}.webp` | decimal | Small thumbnail with the blur baked in; served even for locked pages. |
/// | Clear raster | `pages/page{n}.webp` | decimal | The whole page as one image; 403 on premium-locked pages. |
/// | Stylesheet | `{objectKey}.css` | | Fonts and layout. Not used here. |
//!
//! The signing keys live in `#__NEXT_DATA__` under
//! `props.pageProps.documentAccess.signedQueryParams`, and the shape varies:
//!
//! * Scanned / image documents: `{ global }` — one wildcard string that
//!   authorises `/html/*.png`. `hasBlurredPages` is false.
//! * Native text documents: `{ html, css, png, blurredPage, pages }` —
//!   `png` is a wildcard for `/html/*.png`, and `pages` is an **array** of
//!   `{ pageNumber, signedQueryParams }`, one entry per text-bearing page
//!   the reader is allowed to see.
//!
//! A page is premium-locked when the document has a text layer (the
//! `pages` array exists) and the page has no entry in it — keyed off the
//! presence of `pages`, not its length: `pages: []` means every page is
//! locked, not that the document is scanned. A locked page's text is
//! never sent to the browser in any form, so no client can un-blur it:
//! the PDF keeps Studocu's blurred preview for that page instead of
//! pretending the page is clear, and the outcome reports how many pages
//! are blurred.
//!
//! This downloader therefore builds the PDF/DOCX from the clear page
//! rasters (`pages/page{n}.webp`, WebP decoded and re-encoded to JPEG),
//! falling back per page to the blurred thumbnail, then — for scanned
//! documents only — to the figure-layer `bg{n}.png` (which is the whole
//! page there), then to a blank placeholder page so page numbering stays
//! true. A partial result (e.g. 8/12 pages clear) is normal — never claim
//! a full download when pages are missing or blurred.
//!
//! Known limits (honest): pages are raster images, so the PDF text is
//! NOT searchable — Studocu serves no extractable text layer to a plain
//! HTTP client (the `.page` fragments are positioned HTML spans tied to
//! the viewer's own fonts and stylesheet; reassembling them is a separate
//! pdf2htmlEX renderer, not attempted here). Studocu also guards its
//! pages with a Cloudflare bot check: a datacentre IP gets an
//! "Access Blocked" page with no `#__NEXT_DATA__` instead of the
//! document (verified from this VM, 2026-11-27, on two public documents
//! and over both IPv4 and IPv6). A first load from a normal residential
//! connection is expected to pass — the same wall appears for real
//! browsers only after repeated automated loads — but when the fetch is
//! blocked the download fails with a clear message instead of a fake
//! empty file. No login is needed: the free preview is public.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context};
use image::ImageEncoder;

use super::{report_bytes, ProgressFn};

/// CDN root for Studocu document assets.
pub const DOC_ASSETS: &str = "https://doc-assets.studocu.com/";

/// JPEG quality for re-encoded page rasters (WebP/PNG in, JPEG out —
/// the PDF writer below only embeds JPEG).
const JPEG_QUALITY: u8 = 88;

/// Parsed Studocu link: numeric document id + the page URL itself.
#[derive(Debug, Clone)]
pub struct StudocuInfo {
    pub id: String,
    pub url: String,
}

/// Document access model from `#__NEXT_DATA__`.
#[derive(Debug, Clone)]
pub struct StudocuDocument {
    pub id: String,
    pub object_key: String,
    pub page_count: u32,
    pub title: String,
    /// True when `signedQueryParams.pages` exists (native text document):
    /// the `bg{n}.png` is then only the figure layer, never page content.
    pub has_text_layer: bool,
    /// Signed query string for the figure layer (`png` or `global`).
    pub bg_params: String,
    /// Signed query string for the page rasters (`blurredPage` or `global`).
    pub blurred_params: String,
    /// Per-page signed query strings, by page number (native docs only).
    pub page_params: BTreeMap<u32, String>,
}

impl StudocuDocument {
    /// A page is premium-locked when the document has a text layer but
    /// this page has no signed entry of its own.
    pub fn is_locked(&self, page: u32) -> bool {
        self.has_text_layer && !self.page_params.contains_key(&page)
    }

    /// Figure-layer background: HEX page number, `png`/`global` param.
    /// (`bg12.png` is page 18 — pdf2htmlEX writes backgrounds with `%x`.)
    pub fn bg_image_url(&self, page: u32) -> String {
        if self.bg_params.is_empty() {
            return String::new();
        }
        format!(
            "{}{}/html/bg{:x}.png{}",
            DOC_ASSETS, self.object_key, page, self.bg_params
        )
    }

    /// Clear full-page raster: DECIMAL page number, `blurredPage` param
    /// (the clear sibling of the blurred preview, same signed string).
    pub fn clear_page_url(&self, page: u32) -> String {
        if self.blurred_params.is_empty() {
            return String::new();
        }
        format!(
            "{}{}/html/pages/page{}.webp{}",
            DOC_ASSETS, self.object_key, page, self.blurred_params
        )
    }

    /// Studocu's own blurred preview raster: DECIMAL page number.
    pub fn blurred_page_url(&self, page: u32) -> String {
        if self.blurred_params.is_empty() {
            return String::new();
        }
        format!(
            "{}{}/html/pages/blurred/page{}.webp{}",
            DOC_ASSETS, self.object_key, page, self.blurred_params
        )
    }
}

/// One page after fetching: the JPEG bytes that go into the PDF/DOCX
/// plus how the page was obtained (for the honest page counts).
#[derive(Debug, Clone)]
pub struct StudocuPage {
    pub num: u32,
    pub status: StudocuPageStatus,
    pub jpeg: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudocuPageStatus {
    /// Clear raster fetched (`pages/page{n}.webp`, or the figure layer
    /// on a scanned document where that IS the page).
    Clear,
    /// Only the blurred preview thumbnail could be fetched (locked page).
    Blurred,
    /// Nothing could be fetched — a blank placeholder keeps numbering true.
    Missing,
}

/// Outcome of a Studocu download: the file plus the honest page counts.
#[derive(Debug, Clone)]
pub struct StudocuDownloadOutcome {
    pub output_path: String,
    pub pages_got: u32,
    pub pages_total: u32,
    pub pages_blurred: u32,
}

/// Hosts that serve Studocu documents: the main site, the Vietnamese
/// and Indonesian sites, and the Dutch site (StudeerSnel).
const STUDOCU_HOSTS: &[&str] = &["studocu.com", "studocu.vn", "studocu.id", "studeersnel.nl"];

/// Parse a Studocu document link: the path must contain `/document/`
/// and end in a numeric document id
/// (`/{locale}/document/{school}/{course}/{slug}/{id}`).
pub fn parse_studocu_url(url: &str) -> Option<StudocuInfo> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_lowercase();
    let on_host = STUDOCU_HOSTS
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")));
    if !on_host {
        return None;
    }
    let segments: Vec<&str> = parsed
        .path_segments()?
        .filter(|s| !s.is_empty())
        .collect();
    if !segments.iter().any(|s| *s == "document") {
        return None;
    }
    let id = segments.last()?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(StudocuInfo {
        id: id.to_string(),
        url: url.to_string(),
    })
}

fn html_entity_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn first_regex<'a>(html: &'a str, pattern: &str) -> Option<String> {
    let re = regex::Regex::new(pattern).ok()?;
    let cap = re.captures(html)?;
    cap.get(1).map(|m| html_entity_decode(m.as_str().trim()))
}

/// Raw (non-entity-decoded) first capture: for `#__NEXT_DATA__`,
/// whose JSON must reach `serde_json` byte-identical — decoding
/// `&quot;` there would inject a bare `"` and break the JSON.
fn first_regex_raw(html: &str, pattern: &str) -> Option<String> {
    let re = regex::Regex::new(pattern).ok()?;
    let cap = re.captures(html)?;
    cap.get(1).map(|m| m.as_str().trim().to_string())
}

/// Extract the `#__NEXT_DATA__` JSON blob from the document HTML.
fn extract_next_data(html: &str) -> Option<serde_json::Value> {
    let raw = first_regex_raw(
        html,
        r#"(?s)<script[^>]*id="__NEXT_DATA__"[^>]*>(.*?)</script>"#,
    )?;
    serde_json::from_str::<serde_json::Value>(&raw).ok()
}

fn value_str<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

fn value_u64(v: &serde_json::Value, key: &str) -> Option<u64> {
    v.get(key)?.as_u64()
}

/// First string-valued key out of `keys`, or empty. Never returns the
/// `pages` array — concatenating it into a URL once produced
/// `bg8.png[object Object]` requests.
fn pick_param(sp: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(s) = value_str(sp, key) {
            if !s.is_empty() {
                return normalise_param(s);
            }
        }
    }
    String::new()
}

/// Signed query strings arrive with a leading `?`; if one ever comes
/// without it, add it so the URL stays valid.
fn normalise_param(p: &str) -> String {
    if p.is_empty() {
        String::new()
    } else if p.starts_with('?') {
        p.to_string()
    } else {
        format!("?{p}")
    }
}

/// Read the document access model + metadata from a document page's
/// HTML (`#__NEXT_DATA__` first, `<title>` / `og:title` as fallbacks
/// for the title only — the access model has no fallback).
pub fn extract_document(html: &str, id: &str) -> Option<StudocuDocument> {
    let data = extract_next_data(html)?;
    let props = data.get("props")?.get("pageProps")?;
    let access = props.get("documentAccess")?;
    let object_key = value_str(access, "objectKey")?.to_string();
    if object_key.is_empty() {
        return None;
    }
    let sp = access.get("signedQueryParams")?;
    let mut page_params: BTreeMap<u32, String> = BTreeMap::new();
    let has_text_layer = sp.get("pages").map(|p| p.is_array()).unwrap_or(false);
    if let Some(pages) = sp.get("pages").and_then(|p| p.as_array()) {
        for p in pages {
            if let (Some(n), Some(s)) = (
                value_u64(p, "pageNumber"),
                value_str(p, "signedQueryParams"),
            ) {
                if !s.is_empty() && n > 0 && n <= u32::MAX as u64 {
                    page_params.insert(n as u32, normalise_param(s));
                }
            }
        }
    }
    let doc = props.get("document");
    let mut page_count: u32 = 0;
    if let Some(d) = doc {
        page_count = value_u64(d, "numberOfPages")
            .or_else(|| value_u64(d, "pageCount"))
            .unwrap_or(0) as u32;
    }
    if page_count == 0 {
        // Last resort: the highest page with a signed entry.
        page_count = page_params.keys().last().copied().unwrap_or(0);
    }
    if page_count == 0 {
        return None;
    }
    let mut title = doc
        .and_then(|d| value_str(d, "title"))
        .map(|s| s.to_string())
        .unwrap_or_default();
    if title.trim().is_empty() {
        title = first_regex(
            html,
            r#"(?s)<meta[^>]*property="og:title"[^>]*content="([^"]*)""#,
        )
        .or_else(|| {
            first_regex(
                html,
                r#"(?s)<meta[^>]*content="([^"]*)"[^>]*property="og:title""#,
            )
        })
        .or_else(|| first_regex(html, r"(?s)<title[^>]*>(.*?)</title>"))
        .unwrap_or_default();
        if let Some(stripped) = title.strip_suffix(" - Studocu") {
            title = stripped.to_string();
        }
    }
    Some(StudocuDocument {
        id: id.to_string(),
        object_key,
        page_count,
        title: title.trim().to_string(),
        has_text_layer,
        bg_params: pick_param(sp, &["png", "global"]),
        blurred_params: pick_param(sp, &["blurredPage", "global"]),
        page_params,
    })
}

// ---------------------------------------------------------------------------
// Network: fetch the document page, then each page's raster.
// ---------------------------------------------------------------------------

/// Fetch a document page. A 403 here is Studocu's Cloudflare bot wall
/// ("Access Blocked" page, no `#__NEXT_DATA__`) — reported as such so
/// the user knows it is the site blocking the fetch, not a broken link.
pub async fn fetch_document_html(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<String> {
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if status.as_u16() == 403 {
        anyhow::bail!(
            "Studocu chặn truy cập (HTTP 403 — trang \"Access Blocked\"). \
            Studocu chỉ cho trình duyệt thật tải trang này; hãy mở link bằng \
            trình duyệt rồi thử lại sau."
        );
    }
    if !status.is_success() {
        anyhow::bail!("Studocu trả về HTTP {}", status);
    }
    Ok(resp.text().await?)
}

/// GET one asset URL; `None` on any non-200 status (the caller falls
/// back to the next candidate — a 403 on a clear raster is the normal
/// signal for a premium-locked page). Returns the bytes plus their
/// real on-the-wire size.
async fn fetch_bytes(
    client: &reqwest::Client,
    url: &str,
    referer: &str,
) -> Option<(Vec<u8>, u64)> {
    if url.is_empty() {
        return None;
    }
    let referer_value = reqwest::header::HeaderValue::from_str(referer)
        .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("https://www.studocu.com/"));
    let resp = client
        .get(url)
        .header(reqwest::header::REFERER, referer_value)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let len = bytes.len() as u64;
    Some((bytes.to_vec(), len))
}

/// Decode any image the CDN serves (WebP rasters, PNG figure layers)
/// and re-encode it as JPEG, the only image format the PDF writer
/// embeds. Returns the JPEG bytes plus the image size in pixels.
fn image_to_jpeg(bytes: &[u8]) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).context("không đọc được ảnh trang")?;
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();
    let mut buf = Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    enc.encode_image(&rgb)
        .map_err(|e| anyhow!("không mã hoá được JPEG: {}", e))?;
    Ok((buf.into_inner(), width, height))
}

/// A plain white page (A4 at 72 dpi) for pages nothing could be
/// fetched for — keeps the PDF page numbering true to the original.
fn placeholder_jpeg() -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let white = image::RgbImage::from_pixel(595, 842, image::Rgb([255u8, 255, 255]));
    let (width, height) = white.dimensions();
    let mut buf = Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    enc.encode_image(&white)
        .map_err(|e| anyhow!("không mã hoá được JPEG: {}", e))?;
    Ok((buf.into_inner(), width, height))
}

/// Fetch every page's raster for a document, in order, one request at
/// a time (Studocu rate-limits repeated loads — sequential fetches are
/// the polite shape). Per page: clear raster first; on failure the
/// blurred preview; on scanned documents the figure layer (which IS
/// the page there); finally a blank placeholder. A page that yields
/// nothing becomes a placeholder page, never a silent skip.
///
/// Returns the pages plus the real network bytes fetched for them.
pub async fn fetch_pages(
    client: &reqwest::Client,
    doc: &StudocuDocument,
    referer: &str,
    progress: &ProgressFn,
) -> anyhow::Result<(Vec<StudocuPage>, u64)> {
    let mut pages: Vec<StudocuPage> = Vec::new();
    let mut fetched_bytes: u64 = 0;
    for n in 1..=doc.page_count {
        let mut status = StudocuPageStatus::Missing;
        let mut jpeg: Option<(Vec<u8>, u32, u32)> = None;
        // 1. Clear full-page raster.
        if let Some((bytes, len)) = fetch_bytes(client, &doc.clear_page_url(n), referer).await {
            fetched_bytes += len;
            if let Ok(j) = image_to_jpeg(&bytes) {
                jpeg = Some(j);
                status = StudocuPageStatus::Clear;
            }
        }
        // 2. Blurred preview (the only thing a locked page ever serves).
        if jpeg.is_none() {
            if let Some((bytes, len)) =
                fetch_bytes(client, &doc.blurred_page_url(n), referer).await
            {
                fetched_bytes += len;
                if let Ok(j) = image_to_jpeg(&bytes) {
                    jpeg = Some(j);
                    status = StudocuPageStatus::Blurred;
                }
            }
        }
        // 3. Scanned documents only: the figure layer IS the page.
        if jpeg.is_none() && !doc.has_text_layer {
            if let Some((bytes, len)) = fetch_bytes(client, &doc.bg_image_url(n), referer).await {
                fetched_bytes += len;
                if let Ok(j) = image_to_jpeg(&bytes) {
                    jpeg = Some(j);
                    status = StudocuPageStatus::Clear;
                }
            }
        }
        let (jpg, width, height) = match jpeg {
            Some((b, w, h)) => (Some(b), w, h),
            None => {
                let (b, w, h) = placeholder_jpeg()?;
                (Some(b), w, h)
            }
        };
        pages.push(StudocuPage {
            num: n,
            status,
            jpeg: jpg,
            width,
            height,
        });
        report_bytes(
            progress,
            "studocu",
            "fetch",
            n as u64,
            Some(doc.page_count as u64),
            None,
            fetched_bytes,
        );
    }
    Ok((pages, fetched_bytes))
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    let mut s: String = name.chars().take(max_chars).collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() {
        "studocu-document".to_string()
    } else {
        s
    }
}

/// File extension for a requested Studocu format: only `"docx"`
/// (case-insensitive) produces a DOCX — anything else (including
/// empty or unknown) falls back to PDF.
fn studocu_format_ext(format: &str) -> &'static str {
    if format.eq_ignore_ascii_case("docx") {
        "docx"
    } else {
        "pdf"
    }
}

/// Build a PDF from the fetched pages: one page image per Studocu
/// page, in order. Blurred pages keep their blurred thumbnail and
/// missing pages a blank page, so the PDF page numbers match the
/// original document.
pub fn build_pdf(pages: &[StudocuPage]) -> anyhow::Result<Vec<u8>> {
    let jpegs: Vec<Vec<u8>> = pages
        .iter()
        .map(|p| p.jpeg.clone().unwrap_or_default())
        .collect();
    if jpegs.iter().any(|j| j.is_empty()) {
        return Err(anyhow!("thiếu ảnh trang để ghi PDF"));
    }
    super::jpeg_pdf::build_pdf(&jpegs)
}

// ---------------------------------------------------------------------------
// DOCX: same minimal package shape as the Scribd DOCX writer — title,
// one page image per page (content width, aspect kept), a note line
// for blurred / missing pages, page breaks between pages.
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

/// Build a .docx from Studocu pages: title once at the top, then per
/// page its image, a note line when the page is only a blurred preview
/// or missing, and a page break between Studocu pages.
pub fn build_docx(pages: &[StudocuPage], title: &str) -> anyhow::Result<Vec<u8>> {
    let mut body = String::new();
    if !title.trim().is_empty() {
        body.push_str(&format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            docx_escape_xml(title.trim())
        ));
    }
    // Image rel ids start at rId2 (rId1 is the styles part).
    let mut media_count = 0usize;
    for (pi, page) in pages.iter().enumerate() {
        match page.status {
            StudocuPageStatus::Blurred => {
                body.push_str(&docx_para(&format!(
                    "[Trang {}: bản xem trước mờ — trang này bị khóa Premium, Studocu không gửi nội dung rõ]",
                    page.num
                )));
            }
            StudocuPageStatus::Missing => {
                body.push_str(&docx_para(&format!(
                    "[Trang {}: không tải được ảnh trang]",
                    page.num
                )));
            }
            StudocuPageStatus::Clear => {}
        }
        if page.status != StudocuPageStatus::Missing && page.jpeg.is_some() {
            // Missing pages carry a blank placeholder image; the note
            // above already says so, so skip embedding the blank.
            media_count += 1;
            let cy = DOCX_CONTENT_WIDTH_EMU as u64 * page.height.max(1) as u64
                / page.width.max(1) as u64;
            body.push_str(&docx_image_para(
                media_count + 1,
                DOCX_CONTENT_WIDTH_EMU,
                cy as u32,
            ));
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

    // Collect the embedded media in document order (same order the
    // body paragraphs above reference: rId{media_index + 2}).
    let mut media: Vec<&Vec<u8>> = Vec::new();
    for page in pages.iter() {
        if page.status != StudocuPageStatus::Missing {
            if let Some(jpeg) = &page.jpeg {
                media.push(jpeg);
            }
        }
    }
    debug_assert_eq!(media.len(), media_count);

    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    );
    for i in 0..media.len() {
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{}.jpg\"/>",
            i + 2,
            i + 1
        ));
    }
    rels.push_str("</Relationships>");

    let content_types = "\
        <?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>\
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
    for (i, bytes) in media.iter().enumerate() {
        files.push((format!("word/media/image{}.jpg", i + 1), (*bytes).clone()));
    }
    for (name, bytes) in files {
        zip.start_file(name, opts)
            .context("ghi file vào docx")?;
        zip.write_all(&bytes).context("ghi file vào docx")?;
    }
    let out = zip.finish().context("hoàn tất docx")?;
    Ok(out.into_inner())
}

// ---------------------------------------------------------------------------
// Download: fetch the document page, then each page's raster, then
// compile the PDF or DOCX and write it to the destination directory.
// ---------------------------------------------------------------------------

/// Download a Studocu document's free preview as PDF (default) or
/// DOCX. Only the pages Studocu actually serves are included: clear
/// pages at full quality, locked pages as their blurred preview, and
/// a blank page where nothing could be fetched — the outcome counts
/// say exactly how many of each there are.
pub async fn download_document(
    url: &str,
    dest_dir: &str,
    progress: ProgressFn,
    format: &str,
) -> anyhow::Result<StudocuDownloadOutcome> {
    let info = parse_studocu_url(url)
        .ok_or_else(|| anyhow!("không phải link tài liệu Studocu (studocu.com / studocu.vn / studocu.id / studeersnel.nl)"))?;
    let client = super::client()?;
    let html = fetch_document_html(&client, &info.url).await?;
    let doc = extract_document(&html, &info.id)
        .ok_or_else(|| anyhow!("Không đọc được thông tin tài liệu Studocu (trang bị chặn hoặc đã đổi cấu trúc)"))?;
    // Real network bytes so far: the document HTML itself. Page
    // fetches add to this; the compile phase transfers nothing, so the
    // total stays flat there and the speed honestly drops to zero.
    let mut fetched_bytes = html.len() as u64;
    report_bytes(
        &progress,
        "studocu",
        "fetch",
        0,
        Some(doc.page_count as u64),
        None,
        fetched_bytes,
    );
    let (pages, page_bytes) = fetch_pages(&client, &doc, &info.url, &progress).await?;
    fetched_bytes += page_bytes;
    report_bytes(
        &progress,
        "studocu",
        "compile",
        0,
        Some(pages.len() as u64),
        None,
        fetched_bytes,
    );
    let ext = studocu_format_ext(format);
    let bytes = if ext == "docx" {
        build_docx(&pages, &doc.title)?
    } else {
        build_pdf(&pages)?
    };
    report_bytes(
        &progress,
        "studocu",
        "compile",
        pages.len() as u64,
        Some(pages.len() as u64),
        None,
        fetched_bytes,
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
    let got = pages
        .iter()
        .filter(|p| p.status == StudocuPageStatus::Clear)
        .count() as u32;
    let blurred = pages
        .iter()
        .filter(|p| p.status == StudocuPageStatus::Blurred)
        .count() as u32;
    report_bytes(
        &progress,
        "studocu",
        "done",
        pages.len() as u64,
        Some(pages.len() as u64),
        None,
        fetched_bytes,
    );
    Ok(StudocuDownloadOutcome {
        output_path: path.to_string_lossy().to_string(),
        pages_got: got,
        pages_total: doc.page_count,
        pages_blurred: blurred,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#__NEXT_DATA__` for a native text document: 5 pages, only
    /// pages 1–2 signed (the rest are premium-locked). Shape mirrors
    /// the live `props.pageProps.documentAccess` object.
    const NATIVE_HTML: &str = r##"<!doctype html><html><head>
<title>LAW 1 UNIT Summary - Studocu</title>
<meta property="og:title" content="LAW 1 UNIT Summary">
</head><body>
<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{
"documentAccess":{"objectKey":"abc123def","hasBlurredPages":true,
"signedQueryParams":{"png":"?X-Amz-Png=1","blurredPage":"?X-Amz-Blur=1",
"pages":[{"pageNumber":1,"signedQueryParams":"?X-Amz-P1=1"},{"pageNumber":2,"signedQueryParams":"?X-Amz-P2=1"}]}},
"document":{"numberOfPages":5,"title":"LAW 1 UNIT Summary: Key Concepts"}}}}</script>
</body></html>"##;

    /// `#__NEXT_DATA__` for a scanned document: one wildcard param,
    /// no `pages` array — nothing is locked, the figure layer is the page.
    const SCANNED_HTML: &str = r##"<!doctype html><html><head><title>Scanned notes - Studocu</title></head><body>
<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{
"documentAccess":{"objectKey":"scan999","hasBlurredPages":false,
"signedQueryParams":{"global":"?X-Amz-G=9"}},
"document":{"pageCount":3,"title":"Scanned notes"}}}}</script>
</body></html>"##;

    fn native_doc() -> StudocuDocument {
        extract_document(NATIVE_HTML, "15058305").expect("native doc parses")
    }

    #[test]
    fn test_parse_studocu_url() {
        let doc = parse_studocu_url(
            "https://www.studocu.com/en-za/document/university-of-south-africa/introdution-to-law/law-unit-1-summary-detailed-notes-summary-with-questions-and-answers/15058305",
        )
        .expect("document url");
        assert_eq!(doc.id, "15058305");

        let vn = parse_studocu_url(
            "https://www.studocu.vn/vn/document/dai-hoc-x/triet-hoc/tom-tat/123456",
        )
        .expect("studocu.vn url");
        assert_eq!(vn.id, "123456");

        let nl = parse_studocu_url(
            "https://www.studeersnel.nl/nl/document/tu-delft/wiskunde/samenvatting/789",
        )
        .expect("studeersnel.nl url");
        assert_eq!(nl.id, "789");

        let id = parse_studocu_url(
            "https://www.studocu.id/id/document/universitas-x/matematika/catatan/42",
        )
        .expect("studocu.id url");
        assert_eq!(id.id, "42");

        // Non-document paths and other hosts are rejected.
        assert!(parse_studocu_url("https://www.studocu.com/en-za/search?q=law").is_none());
        assert!(parse_studocu_url("https://www.studocu.com/en-za/institution/foo/123").is_none());
        assert!(parse_studocu_url("https://www.scribd.com/document/123/Title").is_none());
        assert!(parse_studocu_url("https://www.studocu.com/en-za/document/x/y/z/notanumber")
            .is_none());
    }

    #[test]
    fn test_extract_native_document() {
        let doc = native_doc();
        assert_eq!(doc.object_key, "abc123def");
        assert_eq!(doc.page_count, 5);
        assert_eq!(doc.title, "LAW 1 UNIT Summary: Key Concepts");
        assert!(doc.has_text_layer);
        assert!(!doc.is_locked(1));
        assert!(!doc.is_locked(2));
        assert!(doc.is_locked(3));
        assert!(doc.is_locked(5));
        assert_eq!(doc.bg_params, "?X-Amz-Png=1");
        assert_eq!(doc.blurred_params, "?X-Amz-Blur=1");
    }

    #[test]
    fn test_url_builders() {
        let doc = native_doc();
        // Figure layer: HEX page number (page 18 -> bg12.png).
        assert_eq!(
            doc.bg_image_url(18),
            "https://doc-assets.studocu.com/abc123def/html/bg12.png?X-Amz-Png=1"
        );
        assert_eq!(
            doc.bg_image_url(10),
            "https://doc-assets.studocu.com/abc123def/html/bga.png?X-Amz-Png=1"
        );
        // Rasters: DECIMAL page number.
        assert_eq!(
            doc.clear_page_url(3),
            "https://doc-assets.studocu.com/abc123def/html/pages/page3.webp?X-Amz-Blur=1"
        );
        assert_eq!(
            doc.blurred_page_url(3),
            "https://doc-assets.studocu.com/abc123def/html/pages/blurred/page3.webp?X-Amz-Blur=1"
        );
    }

    #[test]
    fn test_extract_scanned_document() {
        let doc = extract_document(SCANNED_HTML, "999").expect("scanned doc parses");
        assert_eq!(doc.object_key, "scan999");
        assert_eq!(doc.page_count, 3);
        assert!(!doc.has_text_layer);
        // No `pages` array means nothing is locked.
        assert!(!doc.is_locked(1));
        assert!(!doc.is_locked(3));
        assert_eq!(doc.bg_params, "?X-Amz-G=9");
        assert_eq!(doc.blurred_params, "?X-Amz-G=9");
        assert_eq!(
            doc.bg_image_url(2),
            "https://doc-assets.studocu.com/scan999/html/bg2.png?X-Amz-G=9"
        );
    }

    #[test]
    fn test_all_locked_when_pages_empty() {
        // `pages: []` means every page is locked, not a scanned doc.
        let html = NATIVE_HTML.replace(
            r#""pages":[{"pageNumber":1,"signedQueryParams":"?X-Amz-P1=1"},{"pageNumber":2,"signedQueryParams":"?X-Amz-P2=1"}]"#,
            r#""pages":[]"#,
        );
        let doc = extract_document(&html, "15058305").expect("doc parses");
        assert!(doc.has_text_layer);
        assert!(doc.is_locked(1));
        assert!(doc.is_locked(5));
    }

    #[test]
    fn test_no_next_data_is_none() {
        // The Cloudflare "Access Blocked" page carries no __NEXT_DATA__.
        assert!(extract_document(
            "<html><head><title>Studocu - Access Blocked</title></head></html>",
            "1"
        )
        .is_none());
    }

    #[test]
    fn test_placeholder_jpeg_is_valid() {
        let (bytes, w, h) = placeholder_jpeg().expect("placeholder encodes");
        assert_eq!((w, h), (595, 842));
        let info = super::super::jpeg_pdf::jpeg_info(&bytes).expect("valid JPEG");
        assert_eq!((info.width, info.height), (595, 842));
    }

    #[test]
    fn test_build_pdf_and_docx_from_placeholders() {
        let (jpg, w, h) = placeholder_jpeg().expect("placeholder encodes");
        let pages = vec![
            StudocuPage {
                num: 1,
                status: StudocuPageStatus::Clear,
                jpeg: Some(jpg.clone()),
                width: w,
                height: h,
            },
            StudocuPage {
                num: 2,
                status: StudocuPageStatus::Blurred,
                jpeg: Some(jpg.clone()),
                width: w,
                height: h,
            },
            StudocuPage {
                num: 3,
                status: StudocuPageStatus::Missing,
                jpeg: Some(jpg),
                width: w,
                height: h,
            },
        ];
        let pdf = build_pdf(&pages).expect("pdf builds");
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 3"));
        assert!(text.trim_end().ends_with("%%EOF"));

        let docx = build_docx(&pages, "LAW 1 UNIT Summary").expect("docx builds");
        // A DOCX is a deflated zip, so its text is not greppable in the
        // raw bytes — only the package signature and size are asserted
        // here; the content (title + per-page note lines) is covered by
        // the document.xml construction above.
        assert!(docx.starts_with(b"PK\x03\x04"));
        assert!(docx.len() > 1000);
    }

    #[test]
    fn test_image_to_jpeg_rejects_garbage() {
        assert!(image_to_jpeg(b"not an image at all").is_err());
    }
}
