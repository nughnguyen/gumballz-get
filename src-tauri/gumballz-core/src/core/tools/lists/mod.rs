//! Consumption lists and diaries (`lists` category). The house rule here is
//! just one: when the platform already has an official export, the tool presses
//! the button that already exists instead of scraping pages. Scraping is left only for what
//! the export does not cover (TMDB id, review text per shelf).
//!
//! - `letterboxd.rs`: downloads the ZIP from `/data/export/` with the user's session
//!   and normalizes diary, ratings, reviews, watchlist and lists.
//! - `trakt.rs`: official API with OAuth device flow; history, ratings,
//!   watchlist and lists on the free plan.
//! - `goodreads.rs`: fires the official "Export Library", waits for the CSV to be
//!   ready and enriches it per shelf.
//! - `merge.rs`: joins the three (plus the Spotify export) into a single
//!   read/watched/listened timeline.

pub mod goodreads;
pub mod letterboxd;
pub mod merge;
pub mod trakt;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// Reddit export CSV reader: same dialect (RFC 4180 with doubled quotes),
/// so it is not worth rewriting.
pub use crate::core::tools::reddit::gdpr::parse_csv;

// ── Common schema ───────────────────────────────────────────────────────

/// One timeline line, whichever service it came from. It is the format that
/// `merge.rs` consumes and each module produces.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    /// `YYYY-MM-DD` when the service gives the date; empty when it does not.
    #[serde(default)]
    pub date: String,
    /// `filme` | `serie` | `livro` | `musica`.
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub year: Option<i32>,
    /// Director, author or artist — whatever the source can say.
    #[serde(default)]
    pub creator: String,
    /// Already on the 0 to 10 scale (Letterboxd 0–5 becomes 0–10, Goodreads 0–5 likewise).
    #[serde(default)]
    pub rating: Option<f32>,
    #[serde(default)]
    pub review: String,
    /// `letterboxd` | `trakt` | `goodreads` | `spotify`.
    pub source: String,
    #[serde(default)]
    pub url: String,
    /// Which list/shelf/activity produced the line (`diary`, `watchlist`,
    /// `read`, `lista: Favoritos`…).
    #[serde(default)]
    pub list: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tmdb_id: Option<u64>,
    #[serde(default)]
    pub imdb_id: String,
    #[serde(default)]
    pub isbn: String,
    /// How many times the same thing appeared after deduplication.
    #[serde(default)]
    pub times: u32,
}

impl Entry {
    pub fn new(kind: &str, source: &str, title: &str) -> Self {
        Self {
            kind: kind.to_string(),
            source: source.to_string(),
            title: title.to_string(),
            times: 1,
            ..Default::default()
        }
    }

    /// Deduplication key: type + normalized title + year.
    pub fn dedup_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.kind,
            normalize_title(&self.title),
            self.year.map(|y| y.to_string()).unwrap_or_default()
        )
    }

    /// `YYYY-MM` of the line, or empty if it has no usable date.
    pub fn month(&self) -> String {
        if self.date.len() >= 7 {
            self.date[..7].to_string()
        } else {
            String::new()
        }
    }

    pub fn year_of_date(&self) -> String {
        if self.date.len() >= 4 {
            self.date[..4].to_string()
        } else {
            String::new()
        }
    }
}

// ── Title normalization ──────────────────────────────────────────────

/// Leading articles that only get in the way of matching across services: the same
/// movie is "The Thing" on Trakt and "Thing, The" in an old export, and the same
/// book is "O Nome do Vento" and "Nome do Vento, O".
const ARTICLES: &[&str] = &[
    "the ", "a ", "an ", "o ", "os ", "as ", "um ", "uma ", "el ", "la ", "los ", "las ", "le ",
    "les ", "il ", "lo ", "der ", "die ", "das ",
];

/// Comparable title: no accents, no case, no leading article, no
/// subtitle after `:` or ` - `, no punctuation and no leftover space.
///
/// The subtitle drops out because that is where services diverge most ("Duna" vs
/// "Duna: Parte Um" vs "Dune - Part One"), and the parenthesis along with it — that is
/// where the year ("Duna (2021)") and the series ("Duna (Duna, #1)") come from, which are
/// their own fields in the schema. What is left still separates different works
/// because the year enters the dedup key along with it.
pub fn normalize_title(input: &str) -> String {
    let s = crate::core::tools::music::norm::strip_accents(input).to_lowercase();
    // Subtitle: only cut if something is left before it.
    let mut base = s.as_str();
    for sep in [": ", " - ", " — ", " – ", " ("] {
        if let Some(pos) = base.find(sep) {
            if pos > 0 {
                base = &base[..pos];
            }
        }
    }
    let base = base.strip_suffix(':').unwrap_or(base);
    // "..., The" suffix that some old exports still use.
    let mut s = base.to_string();
    for art in ARTICLES {
        let tail = format!(", {}", art.trim());
        if let Some(head) = s.strip_suffix(&tail) {
            s = format!("{} {}", art.trim(), head);
            break;
        }
    }
    // Punctuation becomes a space; digits and letters stay.
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut out = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    for art in ARTICLES {
        if let Some(rest) = out.strip_prefix(art) {
            out = rest.to_string();
            break;
        }
    }
    out.trim().to_string()
}

/// Year inside the title ("Duna (2021)") or from a loose field. Accepts from 1870
/// (first cinema records) up to next year.
pub fn parse_year(s: &str) -> Option<i32> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 4 {
        return digits
            .parse::<i32>()
            .ok()
            .filter(|y| (1400..=2200).contains(y));
    }
    let bytes: Vec<char> = s.chars().collect();
    for w in bytes.windows(4) {
        if w.iter().all(|c| c.is_ascii_digit()) {
            let y: String = w.iter().collect();
            if let Ok(y) = y.parse::<i32>() {
                if (1870..=2100).contains(&y) {
                    return Some(y);
                }
            }
        }
    }
    None
}

/// `2021-05-13T14:33:00Z`, `2021-05-13 14:33`, `2021/05/13` and `13/05/2021`
/// all become `2021-05-13`. Returns empty when it cannot be read.
pub fn iso_date(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return String::new();
    }
    let head: String = s.chars().take(10).collect();
    let parts: Vec<&str> = head.split(['-', '/', '.']).collect();
    if parts.len() == 3 {
        let (a, b, c) = (parts[0], parts[1], parts[2]);
        if a.len() == 4 {
            if let (Ok(y), Ok(m), Ok(d)) = (a.parse::<i32>(), b.parse::<u32>(), c.parse::<u32>()) {
                if (1..=12).contains(&m) && (1..=31).contains(&d) {
                    return format!("{:04}-{:02}-{:02}", y, m, d);
                }
            }
        } else if c.len() == 4 {
            // dd/mm/yyyy — Goodreads writes mm/dd/yyyy, handled there.
            if let (Ok(d), Ok(m), Ok(y)) = (a.parse::<u32>(), b.parse::<u32>(), c.parse::<i32>()) {
                if (1..=12).contains(&m) && (1..=31).contains(&d) {
                    return format!("{:04}-{:02}-{:02}", y, m, d);
                }
            }
        }
    }
    // Goodreads spelled-out date: "May 13, 2021" / "Sep 2021".
    if let Some(d) = parse_long_date(s) {
        return d;
    }
    String::new()
}

const MONTHS: &[&str] = &[
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn parse_long_date(s: &str) -> Option<String> {
    let low = s.to_lowercase();
    let clean: String = low
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let toks: Vec<&str> = clean.split_whitespace().collect();
    let mut month = None;
    let mut day: Option<u32> = None;
    let mut year: Option<i32> = None;
    for tok in &toks {
        if month.is_none() && tok.len() >= 3 {
            if let Some(i) = MONTHS.iter().position(|m| tok.starts_with(m)) {
                month = Some(i as u32 + 1);
                continue;
            }
        }
        if tok.len() == 4 {
            if let Ok(y) = tok.parse::<i32>() {
                if (1400..=2200).contains(&y) {
                    year = Some(y);
                    continue;
                }
            }
        }
        if day.is_none() && tok.len() <= 2 {
            if let Ok(d) = tok.parse::<u32>() {
                if (1..=31).contains(&d) {
                    day = Some(d);
                }
            }
        }
    }
    let (m, y) = (month?, year?);
    Some(format!("{:04}-{:02}-{:02}", y, m, day.unwrap_or(1)))
}

// ── Output CSV ────────────────────────────────────────────────────────

pub fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn csv_line(cells: &[String]) -> String {
    let row: Vec<String> = cells.iter().map(|c| csv_cell(c)).collect();
    format!("{}\n", row.join(","))
}

/// Index of each column by header name, without depending on order or
/// case — Letterboxd has already changed the column order more than once.
pub fn header_index(header: &[String]) -> std::collections::HashMap<String, usize> {
    header
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_lowercase(), i))
        .collect()
}

/// Cell by column name, accepting aliases (the same field changed name
/// between export versions).
pub fn cell<'a>(
    row: &'a [String],
    idx: &std::collections::HashMap<String, usize>,
    names: &[&str],
) -> &'a str {
    for name in names {
        if let Some(i) = idx.get(&name.to_lowercase()) {
            if let Some(v) = row.get(*i) {
                let v = v.trim();
                if !v.is_empty() {
                    return v;
                }
            }
        }
    }
    ""
}

/// Writes the lines as CSV with the common-schema header.
pub fn entries_csv(entries: &[Entry]) -> String {
    let mut out = String::from(
        "date,kind,title,year,creator,rating,review,source,list,tags,tmdb_id,imdb_id,isbn,times\n",
    );
    for e in entries {
        out.push_str(&csv_line(&[
            e.date.clone(),
            e.kind.clone(),
            e.title.clone(),
            e.year.map(|y| y.to_string()).unwrap_or_default(),
            e.creator.clone(),
            e.rating.map(|r| format!("{:.1}", r)).unwrap_or_default(),
            e.review.replace('\n', " ").trim().to_string(),
            e.source.clone(),
            e.list.clone(),
            e.tags.join("; "),
            e.tmdb_id.map(|v| v.to_string()).unwrap_or_default(),
            e.imdb_id.clone(),
            e.isbn.clone(),
            e.times.to_string(),
        ]));
    }
    out
}

/// Writes the requested formats (`json`, `csv`, `md`) and returns the paths.
pub fn write_exports(
    dir: &std::path::Path,
    stem: &str,
    entries: &[Entry],
    formats: &[String],
    markdown: impl FnOnce(&[Entry]) -> String,
) -> Result<Vec<String>> {
    std::fs::create_dir_all(dir)?;
    let want = |f: &str| formats.iter().any(|x| x.eq_ignore_ascii_case(f));
    let mut files = Vec::new();
    if want("json") {
        let p = dir.join(format!("{}.json", stem));
        std::fs::write(&p, serde_json::to_string_pretty(entries)?)?;
        files.push(p.to_string_lossy().to_string());
    }
    if want("csv") {
        let p = dir.join(format!("{}.csv", stem));
        std::fs::write(&p, entries_csv(entries))?;
        files.push(p.to_string_lossy().to_string());
    }
    if want("md") {
        let p = dir.join(format!("{}.md", stem));
        std::fs::write(&p, markdown(entries))?;
        files.push(p.to_string_lossy().to_string());
    }
    Ok(files)
}

// ── Session client ──────────────────────────────────────────────────

/// Filters the cookies of one domain from the Netscape file and builds the jar. The
/// manager writes one file per (domain, account) and one file can
/// carry a neighbour's cookie, so the filter is not decoration.
pub fn jar_from_netscape(content: &str, domain: &str) -> (Arc<reqwest::cookie::Jar>, usize) {
    let jar = reqwest::cookie::Jar::default();
    let suffix = format!(".{}", domain);
    let mut n = 0;
    for c in crate::core::tools::instagram::parse_netscape(content) {
        let d = c.domain.trim_start_matches('.');
        if d != domain && !d.ends_with(&suffix) {
            continue;
        }
        let path = if c.path.is_empty() { "/" } else { &c.path };
        let scheme = if c.secure { "https" } else { "http" };
        let Ok(url) = format!("{}://{}{}", scheme, d, path).parse::<reqwest::Url>() else {
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

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

fn cookie_client(session: Option<&str>, domain: &str) -> Result<(reqwest::Client, bool)> {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, USER_AGENT};
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(UA));
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9,pt-BR;q=0.8"),
    );
    let (jar, seeded) = match session {
        Some(content) => jar_from_netscape(content, domain),
        None => (Arc::new(reqwest::cookie::Jar::default()), 0),
    };
    let client = crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
        .default_headers(headers)
        .cookie_provider(jar)
        .timeout(Duration::from_secs(180))
        .build()?;
    Ok((client, seeded > 0))
}

/// Client with a brake, in the same shape as the Reddit `Fetcher`: one
/// request at a time, configurable wait between them and growing backoff
/// when the server complains. Letterboxd and Goodreads have no public API and
/// respond badly to bursts — the brake is what keeps the user's account out of
/// trouble.
pub struct Fetcher {
    client: reqwest::Client,
    delay: Duration,
    last: tokio::sync::Mutex<Option<Instant>>,
    requests: AtomicU32,
    has_session: bool,
}

impl Fetcher {
    pub fn new(delay_ms: u64, session: Option<&str>, domain: &str) -> Result<Self> {
        let (client, has_session) = cookie_client(session, domain)?;
        Ok(Self {
            client,
            delay: Duration::from_millis(delay_ms.clamp(200, 30_000)),
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

    pub async fn pace(&self) {
        let mut last = self.last.lock().await;
        if let Some(t) = *last {
            let since = t.elapsed();
            if since < self.delay {
                tokio::time::sleep(self.delay - since).await;
            }
        }
        *last = Some(Instant::now());
    }

    /// GET with the body as text. Retries on 429 and on server errors.
    pub async fn get_text(&self, url: &str) -> Result<String> {
        const TRIES: u32 = 4;
        let mut wait = Duration::from_secs(3);
        for attempt in 1..=TRIES {
            self.pace().await;
            self.requests.fetch_add(1, Ordering::Relaxed);
            let resp = self.client.get(url).send().await;
            match resp {
                Ok(r) if r.status().is_success() => return Ok(r.text().await?),
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
                Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => {
                    return Err(anyhow!(
                        "the site answered {} — the session saved in the cookie manager expired or has no access to this",
                        r.status()
                    ));
                }
                Ok(r) if r.status().as_u16() == 404 => {
                    return Err(anyhow!("not found ({})", url));
                }
                Ok(r) => return Err(anyhow!("HTTP {} em {}", r.status(), url)),
                Err(e) if attempt < TRIES => {
                    tokio::time::sleep(wait).await;
                    wait *= 2;
                    tracing::debug!("lists: attempt {} failed: {}", attempt, e);
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(anyhow!("could not fetch {}", url))
    }

    /// HEAD just to know whether a file already exists. It is how Goodreads says
    /// it finished generating the CSV: while it is not ready, it answers 404.
    pub async fn head_status(&self, url: &str) -> Result<u16> {
        self.pace().await;
        self.requests.fetch_add(1, Ordering::Relaxed);
        Ok(self.client.head(url).send().await?.status().as_u16())
    }

    /// GET that returns the raw bytes (the Letterboxd ZIP, the Goodreads CSV).
    pub async fn get_bytes(&self, url: &str) -> Result<(Vec<u8>, String)> {
        self.pace().await;
        self.requests.fetch_add(1, Ordering::Relaxed);
        let r = self.client.get(url).send().await?;
        if !r.status().is_success() {
            return Err(anyhow!("HTTP {} em {}", r.status(), url));
        }
        let ctype = r
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        Ok((r.bytes().await?.to_vec(), ctype))
    }
}

// ── HTML mínimo ─────────────────────────────────────────────────────────

/// Value of an attribute inside a tag, without bringing in a whole HTML parser
/// to find two numbers. Looks for `attr="value"` from
/// `anchor` and returns the first one that shows up.
pub fn attr_after(html: &str, anchor: &str, attr: &str) -> Option<String> {
    let start = html.find(anchor)?;
    let rest = &html[start..];
    let pat = format!("{}=\"", attr);
    let at = rest.find(&pat)? + pat.len();
    let end = rest[at..].find('"')? + at;
    Some(rest[at..end].to_string())
}

/// Strips the tags from a piece of HTML and resolves the most common entities.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            // A tag becomes a space: without this, a `<br />` between two sentences
            // glues them into a single word.
            '>' => {
                inside = false;
                out.push(' ');
            }
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_acento_artigo_e_subtitulo() {
        assert_eq!(
            normalize_title("O Auto da Compadecida"),
            "auto da compadecida"
        );
        assert_eq!(normalize_title("The Thing"), "thing");
        assert_eq!(normalize_title("Thing, The"), "thing");
        assert_eq!(normalize_title("Duna: Parte Dois"), "duna");
        assert_eq!(normalize_title("Dune - Part Two"), "dune");
        assert_eq!(normalize_title("Amélie"), "amelie");
        assert_eq!(
            normalize_title("  Blade   Runner 2049 "),
            "blade runner 2049"
        );
        assert_eq!(normalize_title("WALL·E"), "wall e");
        // Year and series live in their own field; in the title they are noise.
        assert_eq!(normalize_title("Duna (2021)"), "duna");
        assert_eq!(normalize_title("Duna (Duna, #1)"), "duna");
        assert_eq!(normalize_title("Blade Runner 2049"), "blade runner 2049");
    }

    #[test]
    fn normalizacao_casa_titulos_de_servicos_diferentes() {
        assert_eq!(
            normalize_title("The Lord of the Rings: The Fellowship of the Ring"),
            normalize_title("Lord of the Rings, The")
        );
        assert_eq!(
            normalize_title("Cidade de Deus"),
            normalize_title("Cidade de Deus")
        );
        assert_ne!(normalize_title("Duna"), normalize_title("Dune"));
    }

    #[test]
    fn chave_de_dedup_usa_tipo_titulo_e_ano() {
        let mut a = Entry::new("filme", "letterboxd", "Duna: Parte Dois");
        a.year = Some(2024);
        let mut b = Entry::new("filme", "trakt", "Duna");
        b.year = Some(2024);
        let mut c = Entry::new("livro", "goodreads", "Duna");
        c.year = Some(2024);
        assert_eq!(a.dedup_key(), b.dedup_key());
        assert_ne!(a.dedup_key(), c.dedup_key());
    }

    #[test]
    fn datas_em_varios_formatos_viram_iso() {
        assert_eq!(iso_date("2021-05-13"), "2021-05-13");
        assert_eq!(iso_date("2021-05-13T14:33:00.000Z"), "2021-05-13");
        assert_eq!(iso_date("2021/05/13"), "2021-05-13");
        assert_eq!(iso_date("13/05/2021"), "2021-05-13");
        assert_eq!(iso_date("May 13, 2021"), "2021-05-13");
        assert_eq!(iso_date("Sep 2021"), "2021-09-01");
        assert_eq!(iso_date(""), "");
        assert_eq!(iso_date("not a date"), "");
    }

    #[test]
    fn ano_sai_do_titulo_ou_do_campo() {
        assert_eq!(parse_year("2021"), Some(2021));
        assert_eq!(parse_year("Duna (2021)"), Some(2021));
        assert_eq!(parse_year("sem ano"), None);
        assert_eq!(parse_year("12"), None);
    }

    #[test]
    fn csv_escapa_virgula_aspas_e_quebra() {
        assert_eq!(csv_cell("simples"), "simples");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("diz \"oi\""), "\"diz \"\"oi\"\"\"");
        assert_eq!(csv_line(&["a".into(), "b,c".into()]), "a,\"b,c\"\n");
    }

    #[test]
    fn header_indexes_case_insensitively_and_accepts_alias() {
        let rows = parse_csv("Date,Name,Letterboxd URI\n2021-05-13,Duna,https://x\n");
        let idx = header_index(&rows[0]);
        assert_eq!(cell(&rows[1], &idx, &["name"]), "Duna");
        assert_eq!(cell(&rows[1], &idx, &["Título", "Name"]), "Duna");
        assert_eq!(cell(&rows[1], &idx, &["nada"]), "");
    }

    #[test]
    fn pote_de_cookie_so_aceita_o_dominio_certo() {
        let content = "# Netscape HTTP Cookie File\n\
.letterboxd.com\tTRUE\t/\tTRUE\t2000000000\tletterboxd.signed.in.as\tfulano\n\
letterboxd.com\tFALSE\t/\tFALSE\t2000000000\tcom.xk72.webparts.csrf\tabc\n\
.goodreads.com\tTRUE\t/\tTRUE\t2000000000\t_session_id2\tdeadbeef\n\
.evil.com\tTRUE\t/\tTRUE\t2000000000\troubado\t1\n";
        let (_, n) = jar_from_netscape(content, "letterboxd.com");
        assert_eq!(n, 2);
        let (_, g) = jar_from_netscape(content, "goodreads.com");
        assert_eq!(g, 1);
        let (_, vazio) = jar_from_netscape("# Netscape HTTP Cookie File\n", "letterboxd.com");
        assert_eq!(vazio, 0);
    }

    #[test]
    fn dominio_parecido_nao_entra_no_pote() {
        let content = "# Netscape HTTP Cookie File\n\
.naoletterboxd.com\tTRUE\t/\tTRUE\t2000000000\tx\t1\n\
.letterboxd.com.br.golpe.net\tTRUE\t/\tTRUE\t2000000000\ty\t2\n";
        let (_, n) = jar_from_netscape(content, "letterboxd.com");
        assert_eq!(n, 0);
    }

    #[test]
    fn html_minimo_le_atributo_e_tira_tag() {
        let html = r#"<body class="film" data-tmdb-id="438631" data-tmdb-type="movie">"#;
        assert_eq!(
            attr_after(html, "data-tmdb-id", "data-tmdb-id"),
            Some("438631".into())
        );
        assert_eq!(
            strip_html("<p>oi <b>mundo</b> &amp; cia</p>"),
            "oi mundo & cia"
        );
    }

    #[test]
    fn mes_e_ano_saem_da_data() {
        let mut e = Entry::new("filme", "letterboxd", "Duna");
        e.date = "2024-03-07".into();
        assert_eq!(e.month(), "2024-03");
        assert_eq!(e.year_of_date(), "2024");
        e.date = String::new();
        assert_eq!(e.month(), "");
    }
}
