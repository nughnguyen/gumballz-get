//! `tt-download`: TikTok video without watermark, in batch.
//!
//! The input accepts three forms at the same time: a pasted list (one URL per
//! line), a `.txt` of links and a whole profile/collection. Everything becomes one
//! queue of clean URLs, without repetition.
//!
//! Each item is one yt-dlp process, one at a time, with a wait between them —
//! TikTok closes the door on whoever fires in bursts. What already exists in the
//! destination is skipped before touching the network, and the final summary says per item whether it
//! was ok, skipped or the reason for the failure.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::{
    base_name, canonical_url, expand_inputs, format_selector, has_stem, parse_target, short_reason,
    Pacer, Target, TempCookies,
};
use crate::core::tools::{report, ProgressFn};

const ID: &str = "tt-download";

fn def_true() -> bool {
    true
}
fn def_quality() -> String {
    "best".to_string()
}
fn def_delay() -> u64 {
    1200
}
fn def_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    /// Pasted list: one URL per line (comma and space work too).
    #[serde(default)]
    pub urls: String,
    /// `.txt` file with one URL per line.
    #[serde(default)]
    pub list_file: Option<String>,
    /// Profile (`@user` or the URL) or collection: enters the queue whole.
    #[serde(default)]
    pub profile: Option<String>,
    pub dest: String,
    /// Download the version **with** watermark (the `download_addr`). Default: no.
    #[serde(default)]
    pub watermark: bool,
    /// "best" (h265 1080p when it exists) or "h264" (compatibility).
    #[serde(default = "def_quality")]
    pub quality: String,
    /// Cap of items from the profile/collection. 0 = no cap.
    #[serde(default = "def_limit")]
    pub limit: u32,
    /// Skip what is already in the destination.
    #[serde(default = "def_true")]
    pub skip_existing: bool,
    /// Write the `.info.json` next to each video.
    #[serde(default)]
    pub write_info: bool,
    #[serde(default = "def_delay")]
    pub delay_ms: u64,
    /// A hand-picked cookies.txt (takes priority over the session).
    #[serde(default)]
    pub cookies: Option<String>,
    /// Cookie-manager account (None = `_default`).
    #[serde(default)]
    pub account_slug: Option<String>,
    /// Netscape content of the `tiktok.com` bucket. The Tauri command fills it,
    /// never the frontend — hence the `skip`.
    #[serde(skip)]
    pub session_netscape: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemResult {
    pub url: String,
    pub id: String,
    pub author: String,
    /// "ok" | "skipped" | "failed"
    pub status: String,
    pub reason: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub items: Vec<ItemResult>,
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
    pub dest: String,
    pub used_session: bool,
    /// Whether the no-watermark version came out (the inverse of `watermark`).
    pub no_watermark: bool,
}

/// Joins the three inputs into a single queue, without repeating and respecting the cap.
/// The profile/collection does not enter here (it needs network); the rest is pure.
pub fn queue_from_text(pasted: &str, list: &str, limit: u32) -> Vec<String> {
    let mut out = expand_inputs(pasted);
    for url in expand_inputs(list) {
        if !out.contains(&url) {
            out.push(url);
        }
    }
    if limit > 0 && out.len() > limit as usize {
        out.truncate(limit as usize);
    }
    out
}

/// Lists the videos of a profile or collection with a single yt-dlp call.
async fn expand_profile(
    input: &str,
    limit: u32,
    cookies: Option<&std::path::Path>,
) -> Result<Vec<String>> {
    let target = parse_target(input)
        .ok_or_else(|| anyhow!("did not recognize this profile or collection: {}", input))?;
    match target {
        Target::User { .. } | Target::Collection { .. } | Target::Music { .. } => {}
        _ => return Err(anyhow!("this is a video, not a profile: {}", input)),
    }
    let url = canonical_url(&target);
    let v = super::ytdlp_json(&super::ytdlp_list_args(&url, limit, cookies)).await?;
    let entries = super::favorites::entries_from_list(&v);
    Ok(entries.into_iter().map(|e| e.url).collect())
}

fn cookies_path(opts: &Options, session: &TempCookies) -> Option<PathBuf> {
    if let Some(c) = opts.cookies.as_deref().filter(|c| !c.trim().is_empty()) {
        return Some(PathBuf::from(c));
    }
    session.path().map(|p| p.to_path_buf())
}

pub async fn run(opts: &Options, progress: ProgressFn) -> Result<DownloadResult> {
    let dest = PathBuf::from(&opts.dest);
    if opts.dest.trim().is_empty() {
        return Err(anyhow!("choose the destination folder"));
    }
    std::fs::create_dir_all(&dest)?;

    let session = TempCookies::new(opts.session_netscape.as_deref());
    let used_session = session.is_some();
    let cookies = cookies_path(opts, &session);

    let list_text = match opts.list_file.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| anyhow!("could not read the list {}: {}", path, e))?,
        None => String::new(),
    };
    let mut queue = queue_from_text(&opts.urls, &list_text, 0);

    if let Some(profile) = opts.profile.as_deref().filter(|p| !p.trim().is_empty()) {
        report(
            &progress,
            ID,
            "progress",
            0,
            None,
            Some(format!("lendo {}", profile)),
        );
        for url in expand_profile(profile, opts.limit, cookies.as_deref()).await? {
            if !queue.contains(&url) {
                queue.push(url);
            }
        }
    }
    if opts.limit > 0 && queue.len() > opts.limit as usize {
        queue.truncate(opts.limit as usize);
    }
    if queue.is_empty() {
        return Err(anyhow!(
            "nenhum link do TikTok na entrada (cole as URLs, escolha um .txt ou informe um perfil)"
        ));
    }

    let selector = format_selector(opts.watermark, &opts.quality);
    let ffmpeg = crate::core::dependencies::find_tool("ffmpeg").await;
    let pacer = Pacer::new(opts.delay_ms);
    let total = queue.len() as u64;
    let mut items: Vec<ItemResult> = Vec::with_capacity(queue.len());

    for (i, url) in queue.iter().enumerate() {
        let target = parse_target(url);
        let (author, id) = match &target {
            Some(Target::Video { user, id }) => (user.clone().unwrap_or_default(), id.clone()),
            _ => (String::new(), String::new()),
        };
        report(
            &progress,
            ID,
            "progress",
            i as u64,
            Some(total),
            Some(url.clone()),
        );
        let stem = if id.is_empty() {
            String::new()
        } else {
            base_name(Some(author.as_str()).filter(|a| !a.is_empty()), &id)
        };
        if opts.skip_existing && !stem.is_empty() && has_stem(&dest, &stem) {
            items.push(ItemResult {
                url: url.clone(),
                id,
                author,
                status: "skipped".to_string(),
                reason: "already in the folder".to_string(),
                files: Vec::new(),
            });
            continue;
        }
        // Without an id in the link (short link), yt-dlp picks the name by its own id.
        let template = if stem.is_empty() {
            "%(uploader,channel)s-%(id)s".to_string()
        } else {
            stem
        };
        pacer.wait().await;
        let args = super::ytdlp_args(
            url,
            &dest,
            &template,
            super::Mode::Video {
                selector: &selector,
            },
            opts.write_info,
            ffmpeg.as_deref(),
            cookies.as_deref(),
        );
        let (files, tail) = match super::run_ytdlp(&args, ID, &progress).await {
            Ok(v) => v,
            Err(e) => {
                items.push(ItemResult {
                    url: url.clone(),
                    id,
                    author,
                    status: "failed".to_string(),
                    reason: e.to_string(),
                    files: Vec::new(),
                });
                continue;
            }
        };
        if files.is_empty() {
            items.push(ItemResult {
                url: url.clone(),
                id,
                author,
                status: "failed".to_string(),
                reason: short_reason(&tail),
                files: Vec::new(),
            });
        } else {
            items.push(ItemResult {
                url: url.clone(),
                id,
                author,
                status: "ok".to_string(),
                reason: String::new(),
                files,
            });
        }
    }

    let ok = items.iter().filter(|i| i.status == "ok").count();
    let failed = items.iter().filter(|i| i.status == "failed").count();
    let skipped = items.iter().filter(|i| i.status == "skipped").count();
    report(&progress, ID, "done", total, Some(total), None);
    Ok(DownloadResult {
        items,
        ok,
        failed,
        skipped,
        dest: dest.to_string_lossy().to_string(),
        used_session,
        no_watermark: !opts.watermark,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fila_junta_colado_e_txt_sem_repetir() {
        let colado = "https://www.tiktok.com/@a/video/7683195368279985438?is_from_webapp=1";
        let txt = "# links\n\
                   https://www.tiktok.com/@a/video/7683195368279985438\n\
                   https://www.tiktok.com/@b/video/7681695065927912735\n";
        assert_eq!(
            queue_from_text(colado, txt, 0),
            vec![
                "https://www.tiktok.com/@a/video/7683195368279985438".to_string(),
                "https://www.tiktok.com/@b/video/7681695065927912735".to_string(),
            ]
        );
    }

    #[test]
    fn o_teto_corta_a_fila() {
        let txt = "https://www.tiktok.com/@a/video/7683195368279985438\n\
                   https://www.tiktok.com/@b/video/7681695065927912735\n\
                   https://www.tiktok.com/@c/video/7681414892942839071\n";
        assert_eq!(queue_from_text("", txt, 2).len(), 2);
        assert_eq!(queue_from_text("", txt, 0).len(), 3);
    }

    #[test]
    fn fila_vazia_quando_nada_e_do_tiktok() {
        assert!(queue_from_text("https://youtube.com/watch?v=1\nlixo", "", 0).is_empty());
    }

    #[tokio::test]
    #[ignore = "rede: baixa um video publico do TikTok com o yt-dlp de verdade"]
    async fn ao_vivo_baixa_um_video_sem_marca_dagua() {
        let dir = crate::core::tools::temp_dir().join("tt-live-download");
        let _ = std::fs::create_dir_all(&dir);
        let opts = Options {
            urls: String::new(),
            list_file: None,
            profile: Some("https://www.tiktok.com/@tiktok".to_string()),
            dest: dir.to_string_lossy().to_string(),
            watermark: false,
            quality: "best".to_string(),
            limit: 1,
            skip_existing: true,
            write_info: false,
            delay_ms: 1200,
            cookies: None,
            account_slug: None,
            session_netscape: None,
        };
        let r = run(&opts, crate::core::tools::noop_progress())
            .await
            .expect("o download ao vivo falhou");
        assert_eq!(r.failed, 0, "{:?}", r.items);
        assert!(r.ok + r.skipped >= 1);
    }
}
