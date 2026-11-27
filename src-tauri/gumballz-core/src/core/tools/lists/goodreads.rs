//! Goodreads — the official "Export Library", automated.
//!
//! The public Goodreads API was switched off in 2020 and never came back. What
//! is left is the "Export Library" button at `/review/import`: it asks for a
//! CSV to be generated and the page itself warns, when the file is ready, with a link
//! of the type `/review_porter/export/<id>/goodreads_export.csv`. It is asynchronous and
//! can take minutes on a big shelf, so here we fire it,
//! wait patiently and download — the same path as the browser, without scraping
//! any shelf.
//!
//! Scraping is left only for what the CSV does not give well: the full text of the
//! review per shelf, which comes from the `/review/list/<id>?shelf=…` pages.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::{cell, header_index, iso_date, parse_csv, parse_year, strip_html, Entry, Fetcher};
use crate::core::tools::{report, ProgressFn};

const TOOL_ID: &str = "gr-export";
pub const DOMAIN: &str = "goodreads.com";
pub const IMPORT_URL: &str = "https://www.goodreads.com/review/import";
/// The base of "Export Library". The button does `POST .../export/<user id>`
/// and the ready file sits at `.../export/<id>/goodreads_export.csv`.
pub const EXPORT_BASE: &str = "https://www.goodreads.com/review_porter/export";

pub fn export_post_url(user: &str) -> String {
    format!("{}/{}", EXPORT_BASE, user)
}

pub fn export_csv_url(user: &str) -> String {
    format!("{}/{}/goodreads_export.csv", EXPORT_BASE, user)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    pub dest: String,
    /// CSV already downloaded by hand. Empty = fires the official export.
    #[serde(default)]
    pub csv_path: Option<String>,
    #[serde(default)]
    pub account_slug: Option<String>,
    #[serde(default)]
    pub session_netscape: Option<String>,
    #[serde(default)]
    pub formats: Vec<String>,
    /// Write one file per shelf, with the reviews.
    #[serde(default = "default_true")]
    pub by_shelf: bool,
    /// Generate a new export even if there is already a ready one on the account. Careful:
    /// Goodreads only lets you generate every so often and the new one erases the old.
    #[serde(default)]
    pub force: bool,
    /// Shelves to visit on the web to complete reviews the CSV cut off.
    /// Empty = visits nothing.
    #[serde(default)]
    pub enrich_shelves: Vec<String>,
    /// Cap on pages per shelf (20 books per page with `per_page`).
    #[serde(default = "default_pages")]
    pub max_pages: u32,
    /// How long to wait, at most, for Goodreads to generate the CSV.
    #[serde(default = "default_wait")]
    pub wait_secs: u64,
    #[serde(default = "default_delay")]
    pub delay_ms: u64,
}

fn default_true() -> bool {
    true
}
fn default_pages() -> u32 {
    20
}
fn default_wait() -> u64 {
    300
}
fn default_delay() -> u64 {
    2000
}

#[derive(Debug, Clone, Serialize)]
pub struct ShelfCount {
    pub shelf: String,
    pub books: usize,
    pub reviews: usize,
    pub rated: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub used_session: bool,
    pub source: String,
    pub entries: usize,
    pub shelves: Vec<ShelfCount>,
    pub reviews: usize,
    /// How many reviews came from the shelf pages, not from the CSV.
    pub enriched: usize,
    /// How many seconds Goodreads took to generate the CSV.
    pub waited_secs: u64,
    pub requests: u32,
    pub files: Vec<String>,
    pub dest: String,
    pub sample: Vec<Entry>,
}

// ── Official CSV ─────────────────────────────────────────────────────────

/// The CSV writes ISBN as `="0439023483"` so Excel does not eat the zero on the
/// left; here that is just noise.
fn clean_isbn(s: &str) -> String {
    s.trim()
        .trim_start_matches('=')
        .trim_matches('"')
        .trim()
        .to_string()
}

fn rating_10(s: &str) -> Option<f32> {
    let v = s.trim().parse::<f32>().ok()?;
    if v <= 0.0 {
        return None;
    }
    Some((v * 2.0).clamp(0.0, 10.0))
}

/// Reads `goodreads_library_export.csv`. Columns are looked up by name,
/// not by position: Goodreads has already added a column in the middle before.
pub fn parse_export_csv(text: &str) -> Vec<Entry> {
    let rows = parse_csv(text);
    let Some(header) = rows.first() else {
        return Vec::new();
    };
    let idx = header_index(header);
    let mut out = Vec::new();
    for row in rows.iter().skip(1) {
        let title = cell(row, &idx, &["title"]);
        if title.is_empty() {
            continue;
        }
        let mut e = Entry::new("livro", "goodreads", title);
        e.creator = cell(row, &idx, &["author", "author l-f"]).to_string();
        let extra = cell(row, &idx, &["additional authors"]);
        if !extra.is_empty() {
            e.creator = format!("{}, {}", e.creator, extra);
        }
        e.year = parse_year(cell(
            row,
            &idx,
            &["original publication year", "year published"],
        ));
        // What matters on the timeline is when it was read; whoever is only
        // in the queue enters by the date they joined the queue.
        e.date = iso_date(cell(row, &idx, &["date read", "date added"]));
        e.rating = rating_10(cell(row, &idx, &["my rating"]));
        e.review = strip_html(cell(row, &idx, &["my review"]));
        e.list = cell(row, &idx, &["exclusive shelf"]).to_string();
        e.tags = cell(row, &idx, &["bookshelves"])
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let isbn13 = clean_isbn(cell(row, &idx, &["isbn13"]));
        e.isbn = if isbn13.is_empty() {
            clean_isbn(cell(row, &idx, &["isbn"]))
        } else {
            isbn13
        };
        let book_id = cell(row, &idx, &["book id"]);
        if !book_id.is_empty() {
            e.url = format!("https://www.goodreads.com/book/show/{}", book_id);
        }
        let reads = cell(row, &idx, &["read count"]).parse::<u32>().unwrap_or(1);
        e.times = reads.max(1);
        out.push(e);
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then(a.title.cmp(&b.title)));
    out
}

// ── Firing and waiting for the export ──────────────────────────────────────────

/// The page's anti-CSRF token. Goodreads' Rails publishes both: a
/// `<meta name="csrf-token">` for JavaScript and a hidden `<input>` in the
/// forms — the first one that shows up will do.
pub fn csrf_token(html: &str) -> Option<String> {
    if let Some(at) = html.find("name=\"csrf-token\"") {
        let head = &html[at.saturating_sub(200)..(at + 400).min(html.len())];
        if let Some(v) = super::attr_after(head, "csrf-token", "content") {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    let at = html.find("name=\"authenticity_token\"")?;
    let window = &html[at.saturating_sub(300)..(at + 600).min(html.len())];
    super::attr_after(window, "authenticity_token", "value").filter(|v| !v.is_empty())
}

/// The link to the ready CSV inside the import page. While Goodreads
/// is generating, it simply does not exist.
pub fn export_link(html: &str) -> Option<String> {
    let marker = "review_porter/export/";
    let at = html.find(marker)?;
    // Go back to the start of the URL.
    let start = html[..at].rfind(['"', '\''])? + 1;
    let rest = &html[start..];
    let end = rest.find(['"', '\''])?;
    let url = rest[..end].to_string();
    if !url.contains(".csv") {
        return None;
    }
    Some(if url.starts_with("http") {
        url
    } else if let Some(path) = url.strip_prefix('/') {
        format!("https://www.goodreads.com/{}", path)
    } else {
        format!("https://www.goodreads.com/{}", url)
    })
}

/// The logged-in user's page carries their numeric id in more than one
/// place; the export link is the most reliable, and the `/review/list/<id>` of the
/// page itself serves as a backup.
pub fn user_id(html: &str) -> Option<String> {
    for marker in ["review_porter/export/", "/review/list/"] {
        if let Some(at) = html.find(marker) {
            let rest = &html[at + marker.len()..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

/// Fires the export and waits for the CSV to show up. Returns `(csv, seconds)`.
///
/// The path is the same as the browser's: take the anti-CSRF token from the import
/// page, send the POST to `/review_porter/export/<id>` and then keep
/// asking for the file, which answers 404 while it is not ready.
async fn trigger_and_wait(f: &Fetcher, opts: &Options, p: &ProgressFn) -> Result<(String, u64)> {
    let page = f.get_text(IMPORT_URL).await?;
    if is_signed_out(&page) {
        return Err(anyhow!(
            "the Goodreads session is not valid: capture the goodreads.com cookies in the extension"
        ));
    }
    let uid = user_id(&page).ok_or_else(|| {
        anyhow!("could not find your account id on the Goodreads import page")
    })?;
    let csv_url = export_csv_url(&uid);

    // If a ready export already exists, use it: Goodreads only lets you generate one
    // every few days, and generating a new one erases the previous.
    let ready = !opts.force && export_link(&page).is_some();
    let started = Instant::now();
    if !ready {
        let token = csrf_token(&page).ok_or_else(|| {
            anyhow!(
                "could not find the Goodreads anti-CSRF token; recapture the cookies and try again"
            )
        })?;
        f.pace().await;
        let resp = f
            .client()
            .post(export_post_url(&uid))
            .header(reqwest::header::REFERER, IMPORT_URL)
            .header(reqwest::header::ORIGIN, "https://www.goodreads.com")
            .header(reqwest::header::ACCEPT, "*/*")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("X-CSRF-Token", token)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body("format=json")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 302 {
            return Err(anyhow!(
                "Goodreads refused the export request (HTTP {}). It only lets you generate one every few days",
                status
            ));
        }
    }

    // Wait: the CSV is generated in the background and answers 404 until it is ready.
    let limit = Duration::from_secs(opts.wait_secs.clamp(30, 3600));
    loop {
        let code = f.head_status(&csv_url).await.unwrap_or(0);
        if code == 200 {
            break;
        }
        if code != 404 && code != 0 && code != 403 {
            return Err(anyhow!(
                "Goodreads answered HTTP {} on the export file",
                code
            ));
        }
        if started.elapsed() >= limit {
            return Err(anyhow!(
                "Goodreads has not generated the CSV after {}s. Leave goodreads.com/review/import open, wait and run again — the link stays saved there",
                limit.as_secs()
            ));
        }
        report(
            p,
            TOOL_ID,
            "progress",
            started.elapsed().as_secs(),
            Some(limit.as_secs()),
            Some("esperando o Goodreads gerar o CSV".into()),
        );
        tokio::time::sleep(Duration::from_secs(10)).await;
    }

    let (bytes, _) = f.get_bytes(&csv_url).await?;
    let csv = String::from_utf8_lossy(&bytes).to_string();
    if !csv.to_lowercase().contains("title") {
        return Err(anyhow!(
            "the downloaded file does not look like the Goodreads export"
        ));
    }
    Ok((csv, started.elapsed().as_secs()))
}

/// The page returned when the session is no good: Goodreads answers 200 with
/// the sign-in screen instead of sending you elsewhere.
pub fn is_signed_out(html: &str) -> bool {
    let has_form = html.contains("review_porter") || html.contains("js-LibraryExport");
    !has_form && (html.contains("/user/sign_in") || html.contains("/user/new"))
}

// ── Shelf on the web ───────────────────────────────────────────────────

pub fn shelf_url(user: &str, shelf: &str, page: u32) -> String {
    format!(
        "https://www.goodreads.com/review/list/{}?shelf={}&per_page=100&page={}&print=true",
        user,
        urlencoding::encode(shelf),
        page
    )
}

/// The reviews from a shelf's printed page: `title -> text`.
///
/// Goodreads' HTML is old and predictable: one `<tr id="review_<id>">` per
/// book, the title in the link's `title` attribute (which is the canonical name, without the
/// "(Series, #2)" the visible text carries) and the review in **two** spans —
/// `freeTextContainerreview<id>` with the cut excerpt and `freeTextreview<id>`
/// with the whole text. It is the second one that matters.
pub fn parse_shelf_html(html: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for row in html.split("<tr").skip(1) {
        let Some(title) = row_title(row) else {
            continue;
        };
        let Some(review) = row_review(row) else {
            continue;
        };
        if title.is_empty() || review.is_empty() || review == "None" {
            continue;
        }
        out.insert(super::normalize_title(&title), review);
    }
    out
}

/// The line's canonical title: the book link's `title` attribute.
fn row_title(row: &str) -> Option<String> {
    let at = row.find("field title")?;
    let rest = &row[at..];
    let end = rest.find("</td>").unwrap_or(rest.len());
    let cell = &rest[..end];
    if let Some(v) = super::attr_after(cell, "<a", "title") {
        let v = strip_html(&v);
        if !v.is_empty() {
            return Some(v);
        }
    }
    // Without the attribute, the visible text is left — which brings the series along.
    let text = strip_html(cell);
    let text = text.strip_prefix("title").unwrap_or(&text).trim();
    Some(text.to_string())
}

/// The whole review, from the hidden span; the visible one is just the cut preview.
fn row_review(row: &str) -> Option<String> {
    let id = row_id(row)?;
    for prefix in ["freeTextreview", "freeText"] {
        let marker = format!("id=\"{}{}\"", prefix, id);
        if let Some(at) = row.find(&marker) {
            let rest = &row[at..];
            let start = rest.find('>')? + 1;
            let end = rest[start..].find("</span>").unwrap_or(rest.len() - start) + start;
            let text = strip_html(&rest[start..end]);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    // Without a free-text span the line has no review at all.
    None
}

/// The review's id, which ties the row to the text spans (`review_123`).
fn row_id(row: &str) -> Option<String> {
    let at = row.find("id=\"review_")? + "id=\"review_".len();
    let id: String = row[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

// ── Output per shelf ────────────────────────────────────────────────

pub fn shelves_of(entries: &[Entry]) -> Vec<ShelfCount> {
    let mut map: HashMap<String, ShelfCount> = HashMap::new();
    for e in entries {
        let mut names: Vec<String> = e.tags.clone();
        if !e.list.is_empty() && !names.contains(&e.list) {
            names.push(e.list.clone());
        }
        if names.is_empty() {
            names.push("sem prateleira".to_string());
        }
        for n in names {
            let slot = map.entry(n.clone()).or_insert_with(|| ShelfCount {
                shelf: n,
                books: 0,
                reviews: 0,
                rated: 0,
            });
            slot.books += 1;
            if !e.review.is_empty() {
                slot.reviews += 1;
            }
            if e.rating.is_some() {
                slot.rated += 1;
            }
        }
    }
    let mut out: Vec<ShelfCount> = map.into_values().collect();
    out.sort_by(|a, b| b.books.cmp(&a.books).then(a.shelf.cmp(&b.shelf)));
    out
}

/// One Markdown per shelf, with the whole review under each book.
pub fn shelf_markdown(shelf: &str, entries: &[Entry]) -> String {
    let mut out = format!("# {}\n\n{} livros\n\n", shelf, entries.len());
    for e in entries {
        out.push_str(&format!("## {}", e.title));
        if let Some(y) = e.year {
            out.push_str(&format!(" ({})", y));
        }
        out.push('\n');
        if !e.creator.is_empty() {
            out.push_str(&format!("\n{}\n", e.creator));
        }
        let mut meta = Vec::new();
        if let Some(r) = e.rating {
            meta.push(format!("{:.1}/10", r));
        }
        if !e.date.is_empty() {
            meta.push(e.date.clone());
        }
        if e.times > 1 {
            meta.push(format!("{} leituras", e.times));
        }
        if !meta.is_empty() {
            out.push_str(&format!("\n{}\n", meta.join(" · ")));
        }
        if !e.review.is_empty() {
            out.push_str(&format!("\n{}\n", e.review));
        }
        out.push('\n');
    }
    out
}

fn slug(s: &str) -> String {
    let base: String = crate::core::tools::music::norm::strip_accents(s)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let parts: Vec<&str> = base.split('-').filter(|p| !p.is_empty()).collect();
    let s = parts.join("-");
    if s.is_empty() {
        "prateleira".to_string()
    } else {
        s
    }
}

// ── Execution ────────────────────────────────────────────────────────────

pub async fn run(opts: &Options, p: ProgressFn) -> Result<ExportResult> {
    if opts.dest.trim().is_empty() {
        return Err(anyhow!("choose the destination folder"));
    }
    let dest = PathBuf::from(opts.dest.trim());
    std::fs::create_dir_all(&dest)?;
    let f = Fetcher::new(opts.delay_ms, opts.session_netscape.as_deref(), DOMAIN)?;

    let (csv, source, waited) = match opts.csv_path.as_deref().map(str::trim) {
        Some(path) if !path.is_empty() => (
            std::fs::read_to_string(path).with_context(|| format!("could not read {}", path))?,
            path.to_string(),
            0,
        ),
        _ => {
            if !f.has_session() {
                return Err(anyhow!(
                    "no Goodreads session: capture the goodreads.com cookies in the extension, or point to the goodreads_library_export.csv you already downloaded"
                ));
            }
            let (csv, secs) = trigger_and_wait(&f, opts, &p).await?;
            let _ = std::fs::write(dest.join("goodreads_library_export.csv"), &csv);
            (csv, EXPORT_BASE.to_string(), secs)
        }
    };

    let mut entries = parse_export_csv(&csv);
    if entries.is_empty() {
        return Err(anyhow!("the export came with no books"));
    }

    // Reviews the CSV did not bring, fetched from the shelf.
    let mut enriched = 0usize;
    if !opts.enrich_shelves.is_empty() && f.has_session() {
        let page = f.get_text(IMPORT_URL).await.unwrap_or_default();
        match user_id(&page) {
            None => tracing::debug!("gr-export: could not find the user id; skipped the enrichment"),
            Some(uid) => {
                let mut found: HashMap<String, String> = HashMap::new();
                for shelf in &opts.enrich_shelves {
                    for page_n in 1..=opts.max_pages.max(1) {
                        report(
                            &p,
                            TOOL_ID,
                            "progress",
                            page_n as u64,
                            Some(opts.max_pages as u64),
                            Some(format!("{} ({})", shelf, page_n)),
                        );
                        let url = shelf_url(&uid, shelf, page_n);
                        let Ok(html) = f.get_text(&url).await else {
                            break;
                        };
                        let batch = parse_shelf_html(&html);
                        let empty = batch.is_empty();
                        found.extend(batch);
                        if empty {
                            break;
                        }
                    }
                }
                for e in entries.iter_mut() {
                    if !e.review.is_empty() {
                        continue;
                    }
                    if let Some(text) = found.get(&super::normalize_title(&e.title)) {
                        e.review = text.clone();
                        enriched += 1;
                    }
                }
            }
        }
    }

    let formats = if opts.formats.is_empty() {
        vec!["json".to_string(), "csv".to_string(), "md".to_string()]
    } else {
        opts.formats.clone()
    };
    let mut files = super::write_exports(&dest, "goodreads", &entries, &formats, |es| {
        super::merge::timeline_markdown("Goodreads", es)
    })?;

    let shelves = shelves_of(&entries);
    if opts.by_shelf {
        let shelf_dir = dest.join("shelves");
        std::fs::create_dir_all(&shelf_dir)?;
        for s in &shelves {
            let books: Vec<Entry> = entries
                .iter()
                .filter(|e| e.list == s.shelf || e.tags.contains(&s.shelf))
                .cloned()
                .collect();
            if books.is_empty() {
                continue;
            }
            let md = shelf_dir.join(format!("{}.md", slug(&s.shelf)));
            std::fs::write(&md, shelf_markdown(&s.shelf, &books))?;
            files.push(md.to_string_lossy().to_string());
            let json = shelf_dir.join(format!("{}.json", slug(&s.shelf)));
            std::fs::write(&json, serde_json::to_string_pretty(&books)?)?;
            files.push(json.to_string_lossy().to_string());
        }
    }

    let reviews = entries.iter().filter(|e| !e.review.is_empty()).count();
    report(&p, TOOL_ID, "done", 1, Some(1), None);
    Ok(ExportResult {
        used_session: f.has_session(),
        source,
        entries: entries.len(),
        shelves,
        reviews,
        enriched,
        waited_secs: waited,
        requests: f.requests(),
        files,
        dest: dest.to_string_lossy().to_string(),
        sample: entries.iter().take(40).cloned().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "Book Id,Title,Author,Author l-f,Additional Authors,ISBN,ISBN13,My Rating,Average Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Bookshelves with positions,Exclusive Shelf,My Review,Spoiler,Private Notes,Read Count,Owned Copies\n\
234225,Duna,\"Frank Herbert\",\"Herbert, Frank\",,=\"0441013597\",=\"9780441013593\",5,4.25,Ace,Paperback,604,2005,1965,2021/10/22,2021/09/01,\"ficcao-cientifica, favoritos\",\"ficcao-cientifica (#1)\",read,\"O melhor <i>livro</i> de todos.\",,,2,1\n\
3735293,Clean Code,\"Robert C. Martin\",\"Martin, Robert C.\",,=\"0132350882\",=\"\",0,4.19,Prentice Hall,Paperback,464,2008,2008,,2023/02/14,\"tecnicos\",,to-read,,,,0,0\n";

    #[test]
    fn csv_oficial_vira_linhas_de_livro() {
        let e = parse_export_csv(CSV);
        assert_eq!(e.len(), 2);
        // The order is by descending date: Clean Code (2023) comes first.
        let duna = e
            .iter()
            .find(|x| x.title == "Duna")
            .unwrap_or_else(|| unreachable!());
        assert_eq!(duna.title, "Duna");
        assert_eq!(duna.kind, "livro");
        assert_eq!(duna.creator, "Frank Herbert");
        // The year that matters is the work's, not the edition's.
        assert_eq!(duna.year, Some(1965));
        assert_eq!(duna.date, "2021-10-22");
        assert_eq!(duna.rating, Some(10.0));
        assert_eq!(duna.review, "O melhor livro de todos.");
        assert_eq!(duna.list, "read");
        assert_eq!(duna.tags, vec!["ficcao-cientifica", "favoritos"]);
        assert_eq!(duna.isbn, "9780441013593");
        assert_eq!(duna.url, "https://www.goodreads.com/book/show/234225");
        assert_eq!(duna.times, 2);
    }

    #[test]
    fn livro_da_fila_entra_pela_data_em_que_entrou() {
        let e = parse_export_csv(CSV);
        let clean = e
            .iter()
            .find(|x| x.title == "Clean Code")
            .unwrap_or_else(|| unreachable!());
        assert_eq!(clean.title, "Clean Code");
        assert_eq!(clean.list, "to-read");
        assert_eq!(clean.rating, None);
        assert_eq!(clean.review, "");
        assert_eq!(clean.date, "2023-02-14");
        // An empty ISBN13 falls back to the 10-digit ISBN.
        assert_eq!(clean.isbn, "0132350882");
        assert_eq!(clean.times, 1);
    }

    #[test]
    fn csv_vazio_ou_estranho_nao_quebra() {
        assert!(parse_export_csv("").is_empty());
        assert!(parse_export_csv("Title,Author\n,\n").is_empty());
    }

    #[test]
    fn isbn_perde_o_disfarce_de_planilha() {
        assert_eq!(clean_isbn("=\"0441013597\""), "0441013597");
        assert_eq!(clean_isbn("=\"\""), "");
        assert_eq!(clean_isbn("0441013597"), "0441013597");
    }

    #[test]
    fn acha_o_token_anti_csrf_dos_dois_jeitos() {
        let meta = r#"<meta name="csrf-token" content="abc123==" />"#;
        assert_eq!(csrf_token(meta), Some("abc123==".into()));
        let input =
            r#"<form><input type="hidden" name="authenticity_token" value="xyz789" /></form>"#;
        assert_eq!(csrf_token(input), Some("xyz789".into()));
        assert_eq!(csrf_token("<html></html>"), None);
    }

    #[test]
    fn link_do_csv_so_aparece_quando_fica_pronto() {
        let esperando =
            r#"<div class="exportSection"><p>Your export is being generated.</p></div>"#;
        assert_eq!(export_link(esperando), None);
        let pronto = r#"<a href="/review_porter/export/12345678/goodreads_export.csv">Your export from 2026-09-08</a>"#;
        assert_eq!(
            export_link(pronto),
            Some(
                "https://www.goodreads.com/review_porter/export/12345678/goodreads_export.csv"
                    .into()
            )
        );
        let absoluto = r#"<a href="https://www.goodreads.com/review_porter/export/9/goodreads_export.csv">x</a>"#;
        assert!(export_link(absoluto)
            .unwrap_or_default()
            .starts_with("https://"));
    }

    #[test]
    fn id_do_usuario_sai_do_link_do_export_ou_da_estante() {
        let a = r#"<a href="/review_porter/export/12345678/goodreads_export.csv">x</a>"#;
        assert_eq!(user_id(a), Some("12345678".into()));
        let b = r#"<a href="/review/list/4321-fulano?shelf=read">minha estante</a>"#;
        assert_eq!(user_id(b), Some("4321".into()));
        assert_eq!(user_id("<html></html>"), None);
    }

    #[test]
    fn url_da_prateleira_escapa_o_nome() {
        let u = shelf_url("123", "ficção científica", 2);
        assert!(u.contains("/review/list/123?"));
        assert!(u.contains("shelf=fic%C3%A7%C3%A3o%20cient%C3%ADfica"));
        assert!(u.contains("page=2"));
        assert!(u.contains("print=true"));
    }

    #[test]
    fn review_comes_from_the_full_span_not_the_cut_preview() {
        // Real markup: the visible span brings the cut preview and the hidden one
        // brings the whole text. Taking the whole cell would join the two.
        let html = r#"
<table id="books"><tbody id="booksBody">
<tr id="review_5040164738" class="bookalike review">
  <td class="field title"><label>title</label><div class="value">
    <a title="Duna (Duna, #1)" href="/book/show/234225-duna">Duna
      <span class="darkGreyText">(Duna, #1)</span></a></div></td>
  <td class="field review" style="display: none"><label>review</label><div class="value">
    <span id="freeTextContainerreview5040164738">Um classico que envelheceu...mais</span>
    <span id="freeTextreview5040164738" style="display:none">Um classico que envelheceu bem.<br /><br />Leio de novo todo ano.</span>
  </div></td>
</tr>
<tr id="review_99" class="bookalike review">
  <td class="field title"><label>title</label><div class="value"><a title="Sem review" href="/book/show/1">Sem review</a></div></td>
  <td class="field review"><label>review</label><div class="value"><span class="greyText">None</span></div></td>
</tr>
</tbody></table>"#;
        let map = parse_shelf_html(html);
        assert_eq!(map.len(), 1);
        // The key is the canonical title from the attribute, without "(Duna, #1)".
        assert_eq!(
            map.get("duna").map(String::as_str),
            Some("Um classico que envelheceu bem. Leio de novo todo ano.")
        );
        assert!(parse_shelf_html("<html></html>").is_empty());
    }

    #[test]
    fn linha_da_prateleira_da_o_id_e_o_titulo_canonico() {
        let row = r#" id="review_123" class="bookalike review"><td class="field title"><label>title</label><div class="value"><a title="O Nome do Vento (A Cronica do Matador do Rei, #1)" href="/book/show/9">O Nome do Vento</a></div></td>"#;
        assert_eq!(row_id(row), Some("123".into()));
        assert_eq!(
            row_title(row),
            Some("O Nome do Vento (A Cronica do Matador do Rei, #1)".into())
        );
        assert_eq!(row_id("<td>sem id</td>"), None);
        assert_eq!(row_review(row), None);
    }

    #[test]
    fn endereco_do_export_leva_o_id_da_conta() {
        assert_eq!(
            export_post_url("12345678"),
            "https://www.goodreads.com/review_porter/export/12345678"
        );
        assert_eq!(
            export_csv_url("12345678"),
            "https://www.goodreads.com/review_porter/export/12345678/goodreads_export.csv"
        );
    }

    #[test]
    fn tela_de_entrada_e_reconhecida_como_sessao_morta() {
        assert!(is_signed_out(
            r#"<a href="/user/sign_in?returnurl=%2Freview%2Fimport">Sign in</a>"#
        ));
        assert!(is_signed_out(r#"<form action="/user/new">…</form>"#));
        // The real page has the export button, even while mentioning login.
        assert!(!is_signed_out(
            r#"<a href="/user/sign_in">x</a><button class="js-LibraryExport">Export Library</button>"#
        ));
        assert!(!is_signed_out(
            r#"<a href="/review_porter/export/1/goodreads_export.csv">x</a>"#
        ));
    }

    #[test]
    fn shelves_count_book_review_and_rating() {
        let e = parse_export_csv(CSV);
        let s = shelves_of(&e);
        let by = |name: &str| s.iter().find(|x| x.shelf == name).cloned();
        let fc = by("ficcao-cientifica").unwrap_or_else(|| unreachable!());
        assert_eq!(fc.books, 1);
        assert_eq!(fc.reviews, 1);
        assert_eq!(fc.rated, 1);
        let fila = by("to-read").unwrap_or_else(|| unreachable!());
        assert_eq!(fila.books, 1);
        assert_eq!(fila.reviews, 0);
        // The exclusive shelf also becomes a count.
        assert!(by("read").is_some());
    }

    #[test]
    fn markdown_da_prateleira_traz_a_review_inteira() {
        let e = parse_export_csv(CSV);
        let duna: Vec<Entry> = e.into_iter().filter(|x| x.title == "Duna").collect();
        let md = shelf_markdown("favoritos", &duna);
        assert!(md.starts_with("# favoritos"));
        assert!(md.contains("## Duna (1965)"));
        assert!(md.contains("Frank Herbert"));
        assert!(md.contains("10.0/10"));
        assert!(md.contains("O melhor livro de todos."));
        assert!(md.contains("2 leituras"));
    }

    #[test]
    fn nome_de_arquivo_da_prateleira_e_previsivel() {
        assert_eq!(slug("Ficção Científica"), "ficcao-cientifica");
        assert_eq!(slug("to-read"), "to-read");
        assert_eq!(slug("  "), "prateleira");
    }

    #[test]
    #[ignore = "network + real session: triggers Export Library and waits for the Goodreads CSV"]
    fn real_official_export() {
        // Goodreads only lets you generate one export every few days; exercised
        // by hand with a logged-in account.
    }
}
