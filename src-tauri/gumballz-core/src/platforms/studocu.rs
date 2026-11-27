use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::core::tools::studocu;
use crate::models::media::{DownloadOptions, DownloadResult, MediaInfo, MediaType, VideoQuality};
use crate::models::progress::ProgressUpdate;
use crate::platforms::traits::PlatformDownloader;

pub struct StudocuDownloader;

impl Default for StudocuDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl StudocuDownloader {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformDownloader for StudocuDownloader {
    fn name(&self) -> &str {
        "studocu"
    }

    fn can_handle(&self, url: &str) -> bool {
        studocu::parse_studocu_url(url).is_some()
    }

    async fn get_media_info(&self, url: &str) -> anyhow::Result<MediaInfo> {
        let info =
            studocu::parse_studocu_url(url).ok_or_else(|| anyhow!("Không phải link Studocu"))?;
        let client = crate::core::tools::client()?;
        let html = studocu::fetch_document_html(&client, &info.url).await?;
        let doc = studocu::extract_document(&html, &info.id)
            .ok_or_else(|| anyhow!("Không đọc được thông tin tài liệu Studocu"))?;
        Ok(MediaInfo {
            title: doc.title,
            author: "Studocu".to_string(),
            platform: "studocu".to_string(),
            duration_seconds: None,
            // Studocu's blurred preview of page 1 works as a thumbnail.
            thumbnail_url: Some(doc.blurred_page_url(1)).filter(|u| !u.is_empty()),
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
            // A document link: the frontend offers only PDF / DOCX,
            // never video/audio choices (AGENTS.md rule).
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
            .ok_or_else(|| anyhow!("Không có link Studocu"))?;

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

        let outcome = studocu::download_document(
            target_url,
            &opts.output_dir.to_string_lossy(),
            tool_progress,
            opts.format_id.as_deref().unwrap_or("pdf"),
        )
        .await?;
        let out_path = PathBuf::from(&outcome.output_path);
        let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

        // Studocu only serves the free preview for locked documents, so
        // the page counts are part of the result, not just a log line.
        let mut phase = format!(
            "Đã tải {}/{} trang",
            outcome.pages_got, outcome.pages_total
        );
        if outcome.pages_blurred > 0 {
            phase.push_str(&format!(
                " ({} trang chỉ có bản xem trước mờ)",
                outcome.pages_blurred
            ));
        }
        let _ = progress
            .send(ProgressUpdate::phase(&phase, 100.0))
            .await;

        Ok(DownloadResult {
            file_path: out_path,
            file_size_bytes: size,
            duration_seconds: start.elapsed().as_secs_f64(),
            torrent_id: None,
        })
    }
}
