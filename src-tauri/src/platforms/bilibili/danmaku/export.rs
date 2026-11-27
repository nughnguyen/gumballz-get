//! Standalone export of the floating comments: resolves the requested part of
//! the URL, pulls the danmaku and writes it in the formats the engine already
//! knows how to generate.
//!
//! Nothing here downloads video. The path is the same one the download engine
//! uses as a sidecar (`url_kind` → `parser` → `fetch_elems`), only stopping at
//! the `cid` and going straight to a file, with filters and style along the way.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use gumballz_core::core::tools::{report, ProgressFn};
use serde::{Deserialize, Serialize};

use super::super::api::{ApiClient, BilibiliError};
use super::super::{active_account_slug, cookie, parser, url_kind};
use super::ass::AssRenderOptions;
use super::filter::{self, DanmakuFilter};
use super::{fetch_elems, DanmakuFormat};

/// Tool id in the `tool-progress` event.
pub const ID: &str = "bili-danmaku";

/// When the API does not say how long the video is, the segment count becomes a
/// guess. Six minutes per segment: ten segments cover an hour, and an empty
/// segment only costs one request (`fetch_elems` warns in the log and carries on).
const FALLBACK_DURATION_SECS: u64 = 3600;

#[derive(Debug, Clone, Deserialize)]
pub struct ExportOptions {
    /// Aceita link completo, `b23.tv`, `BV...`, `av...` ou `ep...`.
    pub url: String,
    /// Video part (`?p=N`); 0 uses the one in the URL, otherwise the first.
    #[serde(default)]
    pub page: u32,
    /// `xml`, `ass` and/or `json`. Empty exports all three.
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub filter: DanmakuFilter,
    #[serde(default)]
    pub style: AssRenderOptions,
    /// Output folder; empty falls back to the system's downloads folder.
    #[serde(default)]
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DanmakuPart {
    pub page: u32,
    pub title: String,
    pub cid: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportFile {
    pub format: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub title: String,
    pub part: DanmakuPart,
    /// All the parts the URL has, so the UI can build the selector.
    pub parts: Vec<DanmakuPart>,
    pub fetched: usize,
    pub kept: usize,
    pub files: Vec<ExportFile>,
    /// `true` when it went out with an account's session, not anonymous.
    pub with_account: bool,
}

pub async fn run(opts: &ExportOptions, progress: &ProgressFn) -> Result<ExportResult> {
    let url = opts.url.trim();
    if url.is_empty() {
        return Err(anyhow!("paste the Bilibili video link"));
    }
    report(progress, ID, "started", 0, Some(4), None);

    // Anonymous cookies (buvid3/buvid4/bili_ticket): without them the danmaku
    // endpoint's WBI signature is rejected on a good share of videos.
    if let Err(e) = cookie::ensure_fresh().await {
        tracing::warn!("[bili-danmaku] anonymous cookies failed: {:?}", e);
    }
    let slug = active_account_slug();
    let with_account = slug.is_some();
    let client = build_client(slug.as_deref())?;

    report(
        progress,
        ID,
        "progress",
        1,
        Some(4),
        Some("resolvendo o link".into()),
    );
    let mut effective = url.to_string();
    if url_kind::is_b23_short(&effective) {
        match url_kind::resolve_b23(&client, &effective).await {
            Ok(full) => effective = full,
            Err(e) => return Err(friendly(e, with_account)),
        }
    }
    let kind = url_kind::detect(&effective)
        .map_err(|_| anyhow!("I did not recognise a Bilibili video in: {}", url))?;
    let parsed = parser::parse(&client, &kind)
        .await
        .map_err(|e| friendly(e, with_account))?;

    let parts = collect_parts(&parsed);
    if parts.is_empty() {
        return Err(anyhow!(
            "this link has no video part with comments — open a specific part"
        ));
    }
    let wanted = if opts.page > 0 {
        opts.page
    } else {
        page_from_kind(&kind)
    };
    let part = pick_part(&parts, wanted).ok_or_else(|| {
        anyhow!(
            "part {} does not exist; this video has {}",
            wanted,
            describe_pages(&parts)
        )
    })?;

    let duration = if part.duration_secs > 0 {
        part.duration_secs
    } else {
        FALLBACK_DURATION_SECS
    };
    report(
        progress,
        ID,
        "progress",
        2,
        Some(4),
        Some(format!("downloading the comments of {}", part.title)),
    );
    let elems = fetch_elems(&client, part.cid, duration)
        .await
        .map_err(|e| friendly(e, with_account))?;
    let fetched = elems.len();

    report(
        progress,
        ID,
        "progress",
        3,
        Some(4),
        Some("filtrando".into()),
    );
    let kept_elems = filter::apply(&elems, &opts.filter)?;
    if kept_elems.is_empty() {
        return Err(anyhow!(
            "the filters left no comment standing ({} came from Bilibili)",
            fetched
        ));
    }

    let dir = output_dir(&opts.output_dir)?;
    std::fs::create_dir_all(&dir)?;
    let stem = file_stem(&parsed.title, part, parts.len());
    let mut files = Vec::new();
    for format in wanted_formats(&opts.formats) {
        let body = match format {
            DanmakuFormat::Ass => super::ass::render(&kept_elems, &opts.style),
            other => super::render(&kept_elems, other),
        };
        let path = dir.join(format!("{}.danmaku.{}", stem, format.extension()));
        std::fs::write(&path, &body)
            .map_err(|e| anyhow!("could not write {}: {}", path.display(), e))?;
        files.push(ExportFile {
            format: format.extension().to_string(),
            path: path.to_string_lossy().to_string(),
            bytes: body.len() as u64,
        });
    }

    report(progress, ID, "done", 4, Some(4), None);
    Ok(ExportResult {
        title: parsed.title.clone(),
        part: part.clone(),
        parts,
        fetched,
        kept: kept_elems.len(),
        files,
        with_account,
    })
}

fn build_client(slug: Option<&str>) -> Result<ApiClient> {
    let client = ApiClient::new().map_err(|e| anyhow!("{}", e.i18n_key()))?;
    Ok(match slug {
        Some(s) => client.with_account(s),
        None => client.with_anonymous_cookies(),
    })
}

/// API error turned into a sentence. Everything session-dependent degrades with
/// the same message — that is the case for paid content (`cheese`) and regional
/// bangumi.
fn friendly(e: BilibiliError, with_account: bool) -> anyhow::Error {
    match e {
        BilibiliError::NotLoggedIn | BilibiliError::CookieMissing => anyhow!(
            "this content asks for a Bilibili session — sign in to the account via the cookie manager and try again"
        ),
        BilibiliError::PremiumRequired if with_account => anyhow!(
            "the connected account has no access to this paid content"
        ),
        BilibiliError::PremiumRequired => anyhow!(
            "paid content: connect a Bilibili account that already has the purchase"
        ),
        BilibiliError::GeoBlocked => {
            anyhow!("Bilibili blocks this content in your region; try with a proxy")
        }
        BilibiliError::RateLimited => {
            anyhow!("Bilibili is rate-limiting the requests; wait a bit and repeat")
        }
        BilibiliError::ContentUnavailable => {
            anyhow!("I found no comments in this part — it may have been removed")
        }
        other => anyhow!("{}", other.i18n_key()),
    }
}

fn collect_parts(parsed: &parser::ParsedContent) -> Vec<DanmakuPart> {
    let mut out = Vec::new();
    for (i, item) in parsed.items.iter().enumerate() {
        let cid = match item.cid {
            Some(c) if c > 0 => c,
            _ => continue,
        };
        let page = item.page.unwrap_or((i + 1) as u32);
        let title = if item.title.trim().is_empty() {
            format!("P{}", page)
        } else {
            item.title.trim().to_string()
        };
        out.push(DanmakuPart {
            page,
            title,
            cid,
            duration_secs: item.duration_seconds.unwrap_or(0.0).max(0.0) as u64,
        });
    }
    out
}

fn page_from_kind(kind: &url_kind::UrlKind) -> u32 {
    match kind {
        url_kind::UrlKind::Video { page, .. } => page.unwrap_or(0),
        _ => 0,
    }
}

/// `wanted` 0 means "the first one that comes".
fn pick_part(parts: &[DanmakuPart], wanted: u32) -> Option<&DanmakuPart> {
    if wanted == 0 {
        return parts.first();
    }
    parts.iter().find(|p| p.page == wanted)
}

fn describe_pages(parts: &[DanmakuPart]) -> String {
    let pages: Vec<String> = parts.iter().map(|p| p.page.to_string()).collect();
    format!("{} parte(s): {}", parts.len(), pages.join(", "))
}

fn wanted_formats(raw: &[String]) -> Vec<DanmakuFormat> {
    let mut out: Vec<DanmakuFormat> = Vec::new();
    for name in raw {
        if let Some(f) = DanmakuFormat::from_name(name) {
            if !out.contains(&f) {
                out.push(f);
            }
        }
    }
    if out.is_empty() {
        out = vec![DanmakuFormat::Xml, DanmakuFormat::Ass, DanmakuFormat::Json];
    }
    out
}

fn output_dir(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        return Ok(PathBuf::from(trimmed));
    }
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow!("choose the output folder"))
}

/// `Title - P2 Part name`, already without the characters the system refuses.
/// A single-part video gets no suffix.
fn file_stem(title: &str, part: &DanmakuPart, total_parts: usize) -> String {
    let base = sanitize_filename::sanitize(title.trim());
    let base = if base.is_empty() {
        "bilibili".to_string()
    } else {
        base
    };
    if total_parts <= 1 {
        return base;
    }
    let part_name = sanitize_filename::sanitize(&part.title);
    let tail = if part_name.is_empty() || part_name == base {
        format!("P{}", part.page)
    } else {
        format!("P{} {}", part.page, part_name)
    };
    let joined = format!("{} - {}", base, tail);
    // A file name that is too long breaks on ext4 (255 bytes) and on NTFS.
    if joined.len() > 150 {
        joined.chars().take(150).collect()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::bilibili::parser::{ContentMetadata, EpisodeItem};

    fn item(page: u32, cid: u64, title: &str, dur: f64) -> EpisodeItem {
        EpisodeItem {
            title: title.to_string(),
            cid: Some(cid),
            page: Some(page),
            duration_seconds: Some(dur),
            ..EpisodeItem::default()
        }
    }

    fn parsed(items: Vec<EpisodeItem>) -> parser::ParsedContent {
        parser::ParsedContent {
            title: "Aula de Rust".into(),
            items,
            metadata: ContentMetadata::default(),
            pagination: None,
        }
    }

    #[test]
    fn parts_come_from_the_parsed_items() {
        let p = parsed(vec![
            item(1, 111, "Abertura", 300.0),
            item(2, 222, "Meio", 600.0),
        ]);
        let parts = collect_parts(&p);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].cid, 222);
        assert_eq!(parts[1].duration_secs, 600);
    }

    #[test]
    fn parts_without_cid_are_dropped() {
        let mut sem_cid = item(2, 0, "Sem cid", 10.0);
        sem_cid.cid = None;
        let p = parsed(vec![item(1, 111, "Ok", 10.0), sem_cid]);
        assert_eq!(collect_parts(&p).len(), 1);
    }

    #[test]
    fn parts_fall_back_to_the_position_when_there_is_no_page() {
        let mut a = item(1, 111, "Episódio 1", 10.0);
        a.page = None;
        let mut b = item(2, 222, "Episódio 2", 10.0);
        b.page = None;
        let parts = collect_parts(&parsed(vec![a, b]));
        assert_eq!(parts[0].page, 1);
        assert_eq!(parts[1].page, 2);
    }

    #[test]
    fn page_zero_takes_the_first_part() {
        let parts = collect_parts(&parsed(vec![
            item(1, 111, "a", 1.0),
            item(2, 222, "b", 1.0),
        ]));
        assert_eq!(pick_part(&parts, 0).unwrap().cid, 111);
        assert_eq!(pick_part(&parts, 2).unwrap().cid, 222);
        assert!(pick_part(&parts, 9).is_none());
    }

    #[test]
    fn page_comes_from_the_url_when_the_option_is_zero() {
        let kind = url_kind::detect("https://www.bilibili.com/video/BV1xx411c7mu?p=3").unwrap();
        assert_eq!(page_from_kind(&kind), 3);
        let sem = url_kind::detect("https://www.bilibili.com/video/BV1xx411c7mu").unwrap();
        assert_eq!(page_from_kind(&sem), 0);
        let ep = url_kind::detect("https://www.bilibili.com/bangumi/play/ep123456").unwrap();
        assert_eq!(page_from_kind(&ep), 0);
    }

    #[test]
    fn formats_default_to_all_three_and_ignore_junk() {
        assert_eq!(wanted_formats(&[]).len(), 3);
        assert_eq!(
            wanted_formats(&["ass".into(), "ass".into(), "srt".into()]),
            vec![DanmakuFormat::Ass]
        );
        assert_eq!(
            wanted_formats(&["JSON".into(), "xml".into()]),
            vec![DanmakuFormat::Json, DanmakuFormat::Xml]
        );
    }

    #[test]
    fn file_stem_only_names_the_part_on_multipart() {
        let part = DanmakuPart {
            page: 2,
            title: "Parte: dois".into(),
            cid: 1,
            duration_secs: 1,
        };
        assert_eq!(file_stem("Aula/Rust", &part, 1), "AulaRust");
        let multi = file_stem("Aula", &part, 4);
        assert!(multi.starts_with("Aula - P2 "), "{}", multi);
        assert!(!multi.contains('/'), "{}", multi);
    }

    /// Pulls a real public video and writes all three formats.
    /// `cargo test -p gumballz-get --lib -- --ignored live_danmaku_export`
    #[tokio::test]
    #[ignore]
    async fn live_danmaku_export_writes_the_three_formats() {
        let dir = std::env::temp_dir().join("gumballz-get-danmaku-live");
        let _ = std::fs::remove_dir_all(&dir);
        let opts = ExportOptions {
            url: "https://www.bilibili.com/video/BV1xx411c7mu".into(),
            page: 0,
            formats: Vec::new(),
            filter: DanmakuFilter::default(),
            style: AssRenderOptions::default(),
            output_dir: dir.to_string_lossy().to_string(),
        };
        let res = run(&opts, &gumballz_core::core::tools::noop_progress())
            .await
            .unwrap();
        eprintln!(
            "{} — part {} (cid {}), {} comments, account: {}",
            res.title, res.part.page, res.part.cid, res.fetched, res.with_account
        );
        assert!(res.fetched > 0);
        assert_eq!(res.kept, res.fetched, "without a filter nothing may disappear");
        assert_eq!(res.files.len(), 3);
        for f in &res.files {
            let body = std::fs::read_to_string(&f.path).unwrap();
            assert!(f.bytes > 0, "{} saiu vazio", f.format);
            match f.format.as_str() {
                "xml" => assert!(body.contains("<i>") && body.contains("<d p=")),
                "ass" => assert!(body.contains("[Events]") && body.contains("Dialogue:")),
                _ => assert!(body.trim_start().starts_with('[')),
            }
        }

        // Second pass with a filter: the density ceiling and the word search have
        // to really bite on a real video.
        let filtrado = ExportOptions {
            formats: vec!["json".into()],
            filter: DanmakuFilter {
                kinds: vec![super::super::filter::KIND_SCROLL.to_string()],
                max_per_second: 1,
                ..DanmakuFilter::default()
            },
            ..opts
        };
        let res2 = run(&filtrado, &gumballz_core::core::tools::noop_progress())
            .await
            .unwrap();
        assert_eq!(res2.files.len(), 1);
        assert!(res2.kept < res2.fetched, "the filter cut nothing");
        assert!(
            res2.kept <= res2.part.duration_secs as usize + 1,
            "{} comments for {} seconds at 1/s",
            res2.kept,
            res2.part.duration_secs
        );
        eprintln!("com filtro: {} de {}", res2.kept, res2.fetched);
    }
}
