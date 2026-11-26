use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::core::tools::scribd;
use crate::models::media::{DownloadOptions, DownloadResult, MediaInfo, MediaType, VideoQuality};
use crate::models::progress::ProgressUpdate;
use crate::platforms::traits::PlatformDownloader;

pub struct ScribdDownloader;

impl Default for ScribdDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl ScribdDownloader {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformDownloader for ScribdDownloader {
    fn name(&self) -> &str {
        "scribd"
    }

    fn can_handle(&self, url: &str) -> bool {
        scribd::parse_scribd_url(url).is_some()
    }

    async fn get_media_info(&self, url: &str) -> anyhow::Result<MediaInfo> {
        let info =
            scribd::parse_scribd_url(url).ok_or_else(|| anyhow!("Không phải link Scribd"))?;
        let client = crate::core::tools::client()?;
        let html = scribd::fetch_embed_html(&client, &info.embed_url).await?;
        let doc = scribd::extract_document_json(&html)
            .ok_or_else(|| anyhow!("Không đọc được thông tin tài liệu Scribd"))?;
        Ok(MediaInfo {
            title: doc.title,
            author: "Scribd".to_string(),
            platform: "scribd".to_string(),
            duration_seconds: None,
            thumbnail_url: scribd::first_page_thumbnail_url(&html),
            available_qualities: vec![
                VideoQuality {
                    label: "PDF".to_string(),
                    width: 0,
                    height: 0,
                    url: url.to_string(),
                    format: "pdf".to_string(),
                },
                VideoQuality {
                    label: "DOCX".to_string(),
                    width: 0,
                    height: 0,
                    url: url.to_string(),
                    format: "docx".to_string(),
                },
            ],
            media_type: MediaType::File,
            file_size_bytes: None,
        })
    }

    async fn download(
        &self,
        info: &MediaInfo,
        opts: &DownloadOptions,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        let start = std::time::Instant::now();
        let _ = progress.send(ProgressUpdate::percent(1.0)).await;

        let target_url = opts
            .page_url
            .as_deref()
            .or_else(|| info.available_qualities.first().map(|q| q.url.as_str()))
            .ok_or_else(|| anyhow!("Không có link Scribd"))?;

        let tx = progress.clone();
        let tool_progress: crate::core::tools::ProgressFn =
            Arc::new(move |tp: crate::core::tools::ToolProgress| {
                let pct = match (tp.stage.as_str(), tp.total) {
                    ("fetch", Some(t)) if t > 0 => {
                        5.0 + ((tp.done as f64 / t as f64) * 55.0).clamp(0.0, 55.0)
                    }
                    ("fetch", _) => 5.0,
                    ("compile", Some(t)) if t > 0 => {
                        60.0 + ((tp.done as f64 / t as f64) * 35.0).clamp(0.0, 35.0)
                    }
                    ("compile", _) => 60.0,
                    ("done", _) => 100.0,
                    (_, Some(t)) if t > 0 => {
                        ((tp.done as f64 / t as f64) * 95.0).clamp(0.0, 95.0)
                    }
                    _ => 10.0,
                };
                let _ = tx.try_send(ProgressUpdate::rich(
                    pct,
                    tp.downloaded_bytes,
                    None,
                    None,
                    None,
                ));
            });

        let outcome = scribd::download_document(
            target_url,
            &opts.output_dir.to_string_lossy(),
            tool_progress,
            opts.format_id.as_deref().unwrap_or("pdf"),
        )
        .await?;
        let out_path = PathBuf::from(&outcome.output_path);
        let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

        // Scribd only serves preview pages for paywalled documents, so
        // the page count is part of the result, not just a log line.
        let _ = progress
            .send(ProgressUpdate::phase(
                &format!(
                    "Đã tải {}/{} trang",
                    outcome.pages_got, outcome.pages_total
                ),
                100.0,
            ))
            .await;

        Ok(DownloadResult {
            file_path: out_path,
            file_size_bytes: size,
            duration_seconds: start.elapsed().as_secs_f64(),
            torrent_id: None,
        })
    }
}
