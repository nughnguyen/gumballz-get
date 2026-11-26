use gumballz_core::models::progress::ProgressUpdate;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER};
use tokio::sync::mpsc;

use crate::core::direct_downloader;
use crate::models::media::{DownloadOptions, DownloadResult, MediaInfo, MediaType, VideoQuality};
use crate::platforms::traits::PlatformDownloader;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const SHORT_LINK_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)";

pub struct TikTokDownloader {
    client: reqwest::Client,
    captured_cookies: Arc<tokio::sync::Mutex<Option<String>>>,
}

impl Default for TikTokDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl TikTokDownloader {
    pub fn new() -> Self {
        let mut builder = crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30));

        if let Some(jar) =
            crate::core::cookie_parser::load_extension_cookies_for_domain("tiktok.com")
        {
            builder = builder.cookie_provider(jar);
        } else {
            builder = builder.cookie_store(true);
        }

        let client = builder.build().unwrap_or_default();
        Self {
            client,
            captured_cookies: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    fn tiktok_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, HeaderValue::from_static("https://www.tiktok.com/"));
        headers
    }

    fn download_headers(&self, cookies: &Option<String>) -> HeaderMap {
        let mut headers = Self::tiktok_headers();
        if let Some(ref cookie_str) = cookies {
            if let Ok(val) = HeaderValue::from_str(cookie_str) {
                headers.insert(COOKIE, val);
            }
        }
        headers
    }

    fn extract_post_id(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let segments: Vec<&str> = parsed.path().split('/').filter(|s| !s.is_empty()).collect();

        if segments.len() >= 3
            && segments[0].starts_with('@')
            && (segments[1] == "video" || segments[1] == "photo")
        {
            let id = segments[2];
            if id.chars().all(|c| c.is_ascii_digit()) {
                return Some(id.to_string());
            }
        }

        None
    }

    async fn resolve_short_link(&self, url: &str) -> anyhow::Result<String> {
        let redirect_client =
            crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
                .user_agent(SHORT_LINK_UA)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default();

        let response = redirect_client.get(url).send().await?;

        if let Some(location) = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
        {
            let clean = location.split('?').next().unwrap_or(location).to_string();
            return Ok(clean);
        }

        let html = response.text().await?;

        if html.starts_with("<a href=\"https://") {
            if let Some(url_part) = html.split("<a href=\"").nth(1) {
                let full_url = url_part.split('"').next().unwrap_or(url_part);
                let clean = full_url.split('?').next().unwrap_or(full_url).to_string();
                return Ok(clean);
            }
        }

        Err(anyhow!("Could not resolve short link"))
    }

    fn is_captcha_page(html: &str) -> bool {
        html.contains("verify-bar-close")
            || html.contains("captcha_verify")
            || html.contains("tiktok-verify-page")
            || html.contains("verify/page")
            || (html.contains("Verify to continue")
                && !html.contains("__UNIVERSAL_DATA_FOR_REHYDRATION__"))
    }

    fn is_valid_play_addr(url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return false;
        }
        if url.contains("verify") || url.contains("captcha") {
            return false;
        }
        true
    }

    async fn fetch_detail(&self, post_id: &str) -> anyhow::Result<serde_json::Value> {
        let url = format!("https://www.tiktok.com/@i/video/{}", post_id);

        let response = self.client.get(&url).send().await?;

        let status = response.status();

        if !status.is_success() && status.as_u16() != 302 {
            return Err(anyhow!("TikTok retornou HTTP {}", status));
        }

        let mut cookie_parts = Vec::new();
        for cookie in response.cookies() {
            cookie_parts.push(format!("{}={}", cookie.name(), cookie.value()));
        }
        if !cookie_parts.is_empty() {
            let cookie_str = cookie_parts.join("; ");
            *self.captured_cookies.lock().await = Some(cookie_str);
        }

        let html = response.text().await?;

        if Self::is_captcha_page(&html) {
            return Err(anyhow!(
                "TikTok is blocking requests. Try again in a few minutes."
            ));
        }

        let json_str = html
            .split("<script id=\"__UNIVERSAL_DATA_FOR_REHYDRATION__\" type=\"application/json\">")
            .nth(1)
            .and_then(|s| s.split("</script>").next())
            .ok_or_else(|| anyhow!("TikTok is blocking requests. Try again in a few minutes."))?;

        let data: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|_| anyhow!("Erro ao processar resposta do TikTok"))?;

        let video_detail = data
            .get("__DEFAULT_SCOPE__")
            .and_then(|s| s.get("webapp.video-detail"))
            .ok_or_else(|| anyhow!("Video data not found in TikTok response"))?;

        if let Some(status_msg) = video_detail
            .get("statusMsg")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return Err(anyhow!("Post not available: {}", status_msg));
        }

        if let Some(status_code) = video_detail.get("statusCode").and_then(|v| v.as_u64()) {
            if status_code != 0 {
                return Err(anyhow!("Post not available (status {})", status_code));
            }
        }

        let detail = video_detail
            .pointer("/itemInfo/itemStruct")
            .ok_or_else(|| anyhow!("Video data not found in TikTok response"))?
            .clone();

        if detail
            .get("isContentClassified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Err(anyhow!("Age-restricted content"));
        }

        if detail.get("author").is_none() {
            return Err(anyhow!("Post not available"));
        }

        Ok(detail)
    }

    fn extract_author(detail: &serde_json::Value) -> String {
        detail
            .pointer("/author/uniqueId")
            .or_else(|| detail.pointer("/author/unique_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    }

    fn extract_video_urls(detail: &serde_json::Value) -> (Option<String>, Option<String>) {
        (Self::extract_play_addr(detail), Self::extract_download_addr(detail))
    }

    /// Watermarked video URL (playAddr). TikTok's player URL carries the
    /// watermark; the clean URL lives in downloadAddr (see below).
    fn extract_play_addr(detail: &serde_json::Value) -> Option<String> {
        if let Some(play_addr) = detail.pointer("/video/playAddr") {
            if let Some(url) = play_addr.as_str() {
                if Self::is_valid_play_addr(url) {
                    return Some(url.to_string());
                }
            }
        }

        if let Some(play_addr) = detail.pointer("/video/play_addr/url_list") {
            if let Some(urls) = play_addr.as_array() {
                for u in urls {
                    if let Some(url) = u.as_str() {
                        if Self::is_valid_play_addr(url) {
                            return Some(url.to_string());
                        }
                    }
                }
            }
        }

        if let Some(bitrate_info) = detail
            .pointer("/video/bitrateInfo")
            .and_then(|v| v.as_array())
        {
            for bitrate in bitrate_info {
                if let Some(url_list) = bitrate
                    .pointer("/PlayAddr/UrlList")
                    .and_then(|v| v.as_array())
                {
                    for u in url_list {
                        if let Some(url) = u.as_str() {
                            if Self::is_valid_play_addr(url) {
                                return Some(url.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Watermark-free video URL (downloadAddr). Empty for most photo posts.
    fn extract_download_addr(detail: &serde_json::Value) -> Option<String> {
        detail
            .pointer("/video/downloadAddr")
            .and_then(|v| v.as_str())
            .filter(|u| Self::is_valid_play_addr(u))
            .map(|s| s.to_string())
    }

    fn extract_image_urls(detail: &serde_json::Value) -> Option<Vec<(String, u32, u32)>> {
        let images = detail
            .pointer("/imagePost/images")
            .and_then(|v| v.as_array())?;

        let urls: Vec<(String, u32, u32)> = images
            .iter()
            .filter_map(|img| {
                let url_list = img.pointer("/imageURL/urlList")?.as_array()?;
                let strs: Vec<&str> = url_list.iter().filter_map(|u| u.as_str()).collect();
                let url = strs
                    .iter()
                    .find(|u| u.contains(".jpeg"))
                    .or_else(|| strs.iter().find(|u| u.starts_with("http")))
                    .map(|u| u.to_string())?;
                let width = img
                    .get("imageWidth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let height = img
                    .get("imageHeight")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                Some((url, width, height))
            })
            .collect();

        if urls.is_empty() {
            return None;
        }

        Some(urls)
    }

    /// yt-dlp rejects /photo/ URLs ("Unsupported URL") but accepts the
    /// /video/ form of the same post id, returning its metadata + audio.
    fn ytdlp_url(url: &str) -> String {
        if let Ok(mut parsed) = url::Url::parse(url) {
            if parsed.path().contains("/photo/") {
                let new_path = parsed.path().replace("/photo/", "/video/");
                parsed.set_path(&new_path);
                return parsed.to_string();
            }
        }
        url.to_string()
    }

    async fn fallback_ytdlp(&self, url: &str) -> anyhow::Result<MediaInfo> {
        let ytdlp_path = crate::core::ytdlp::ensure_ytdlp().await?;
        let extra_flags = vec![
            "--referer".to_string(),
            "https://www.tiktok.com/".to_string(),
        ];
        let probe_url = Self::ytdlp_url(url);
        let json =
            crate::core::ytdlp::get_video_info(&ytdlp_path, &probe_url, &extra_flags).await?;
        let mut info =
            crate::platforms::generic_ytdlp::GenericYtdlpDownloader::parse_video_info(&json)?;
        for q in &mut info.available_qualities {
            q.url = probe_url.clone();
            q.format = "ytdlp".to_string();
        }
        Ok(info)
    }

    async fn get_media_info_via_ytdlp(
        &self,
        url: &str,
        post_id: &str,
    ) -> anyhow::Result<MediaInfo> {
        let ytdlp_path = crate::core::ytdlp::find_ytdlp_cached()
            .await
            .ok_or_else(|| anyhow!("yt-dlp not found — install it in Settings"))?;

        let extra_flags = vec![
            "--referer".to_string(),
            "https://www.tiktok.com/".to_string(),
        ];

        let probe_url = Self::ytdlp_url(url);
        let json =
            crate::core::ytdlp::get_video_info(&ytdlp_path, &probe_url, &extra_flags).await?;

        let title = json
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| format!("tiktok_{}", sanitize_filename::sanitize(s)))
            .unwrap_or_else(|| format!("tiktok_{}", post_id));

        let author = json
            .get("uploader")
            .or_else(|| json.get("creator"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let duration = json.get("duration").and_then(|v| v.as_f64());

        let thumbnail = json
            .get("thumbnail")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(MediaInfo {
            title,
            author,
            platform: "tiktok".to_string(),
            duration_seconds: duration,
            thumbnail_url: thumbnail,
            available_qualities: vec![VideoQuality {
                label: "original".to_string(),
                width: 0,
                height: 0,
                url: probe_url,
                format: "ytdlp".to_string(),
            }],
            media_type: MediaType::Video,
            file_size_bytes: None,
        })
    }

    fn extract_music_url(detail: &serde_json::Value) -> Option<String> {
        detail
            .pointer("/music/playUrl")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    fn extract_duration(detail: &serde_json::Value) -> Option<f64> {
        detail.pointer("/video/duration").and_then(|v| v.as_f64())
    }

    /// The quality the user picked in the format list / photo list, or the
    /// audio track for the Music preset. None = download the whole post as
    /// the media-type arms do today.
    fn selected_quality<'a>(
        info: &'a MediaInfo,
        opts: &DownloadOptions,
    ) -> Option<&'a VideoQuality> {
        if let Some(fid) = &opts.format_id {
            if let Some(q) = info.available_qualities.iter().find(|q| &q.label == fid) {
                return Some(q);
            }
        }
        if opts.download_mode.as_deref() == Some("audio") {
            if let Some(q) = info
                .available_qualities
                .iter()
                .find(|q| q.format == "mp3")
            {
                return Some(q);
            }
        }
        None
    }

    async fn download_quality(
        &self,
        info: &MediaInfo,
        quality: &VideoQuality,
        opts: &DownloadOptions,
        headers: &HeaderMap,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        let title = sanitize_filename::sanitize(&info.title);
        match quality.format.as_str() {
            "jpg" | "jpeg" | "png" | "webp" => {
                let total_photos = info
                    .available_qualities
                    .iter()
                    .filter(|q| {
                        matches!(q.format.as_str(), "jpg" | "jpeg" | "png" | "webp")
                    })
                    .count();
                let n = quality
                    .label
                    .strip_prefix("photo_")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1);
                let filename = if total_photos <= 1 {
                    format!("{}.{}", title, quality.format)
                } else {
                    format!("{}_photo_{}.{}", title, n, quality.format)
                };
                let output = opts.output_dir.join(&filename);
                let (tx, _rx) = mpsc::channel(8);
                let bytes = direct_downloader::download_direct_with_headers(
                    &self.client,
                    &quality.url,
                    &output,
                    tx,
                    Some(headers.clone()),
                    Some(&opts.cancel_token),
                )
                .await?;
                let _ = progress.send(ProgressUpdate::percent(100.0)).await;
                Ok(DownloadResult {
                    file_path: output,
                    file_size_bytes: bytes,
                    duration_seconds: 0.0,
                    torrent_id: None,
                })
            }
            "mp3" => {
                let filename = format!("{}.mp3", title);
                let output = opts.output_dir.join(&filename);
                let bytes = direct_downloader::download_direct_with_headers(
                    &self.client,
                    &quality.url,
                    &output,
                    progress,
                    Some(headers.clone()),
                    Some(&opts.cancel_token),
                )
                .await?;
                Ok(DownloadResult {
                    file_path: output,
                    file_size_bytes: bytes,
                    duration_seconds: 0.0,
                    torrent_id: None,
                })
            }
            "tiktok_direct" | "tiktok_direct_wm" | "tiktok_direct_no_wm" => {
                let filename = if quality.label.starts_with("slideshow") {
                    format!("{}_slideshow.mp4", title)
                } else {
                    format!("{}.mp4", title)
                };
                let output = opts.output_dir.join(&filename);
                self.download_direct_video(
                    &quality.url,
                    &output,
                    info.duration_seconds.unwrap_or(0.0),
                    headers,
                    opts,
                    progress,
                )
                .await
            }
            "tiktok_slideshow" => {
                self.download_slideshow(info, opts, headers, progress).await
            }
            other => Err(anyhow!("Unsupported quality format: {}", other)),
        }
    }

    /// Direct video download with TikTok headers, falling back to yt-dlp
    /// when the direct fetch fails (same as the Video arm always did).
    async fn download_direct_video(
        &self,
        url: &str,
        output: &std::path::Path,
        duration_seconds: f64,
        headers: &HeaderMap,
        opts: &DownloadOptions,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        let result = direct_downloader::download_direct_with_headers(
            &self.client,
            url,
            output,
            progress.clone(),
            Some(headers.clone()),
            Some(&opts.cancel_token),
        )
        .await;

        match result {
            Ok(bytes) => Ok(DownloadResult {
                file_path: output.to_path_buf(),
                file_size_bytes: bytes,
                duration_seconds,
                torrent_id: None,
            }),
            Err(e) => {
                tracing::warn!(
                    "[tiktok] direct download failed: {}, falling back to yt-dlp",
                    e
                );
                let _ = tokio::fs::remove_file(output).await;
                let ytdlp_path = match &opts.ytdlp_path {
                    Some(p) => p.clone(),
                    None => crate::core::ytdlp::find_ytdlp_cached()
                        .await
                        .ok_or_else(|| anyhow!("yt-dlp not found"))?,
                };
                crate::core::ytdlp::download_video(
                    &ytdlp_path,
                    url,
                    &opts.output_dir,
                    None,
                    progress,
                    opts.download_mode.as_deref(),
                    None,
                    opts.filename_template.as_deref(),
                    opts.referer.as_deref().or(Some("https://www.tiktok.com/")),
                    opts.cancel_token.clone(),
                    None,
                    opts.concurrent_fragments,
                    false,
                    &[],
                    opts.audio_format.as_deref(),
                )
                .await
            }
        }
    }

    /// Slideshow video for a photo post: fetch the photos + music, then
    /// render them to MP4 with ffmpeg (each photo an equal slice of the
    /// music). No watermark by construction — the source photos are clean.
    async fn download_slideshow(
        &self,
        info: &MediaInfo,
        opts: &DownloadOptions,
        headers: &HeaderMap,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        let photos: Vec<&VideoQuality> = info
            .available_qualities
            .iter()
            .filter(|q| matches!(q.format.as_str(), "jpg" | "jpeg" | "png" | "webp"))
            .collect();
        if photos.is_empty() {
            return Err(anyhow!("No photos to build a slideshow"));
        }
        let music = info
            .available_qualities
            .iter()
            .find(|q| q.format == "mp3");

        let tmp = std::env::temp_dir().join(format!(
            "gumballz-slideshow-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        tokio::fs::create_dir_all(&tmp).await?;
        let result = self
            .build_slideshow(info, &photos, music, &tmp, opts, headers, progress)
            .await;
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        result
    }

    async fn build_slideshow(
        &self,
        info: &MediaInfo,
        photos: &[&VideoQuality],
        music: Option<&VideoQuality>,
        tmp: &std::path::Path,
        opts: &DownloadOptions,
        headers: &HeaderMap,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        let total = photos.len() + usize::from(music.is_some());
        let mut done = 0usize;
        let mut image_paths = Vec::with_capacity(photos.len());
        for (i, q) in photos.iter().enumerate() {
            let path = tmp.join(format!("img{:03}.jpg", i + 1));
            let (tx, _rx) = mpsc::channel(8);
            direct_downloader::download_direct_with_headers(
                &self.client,
                &q.url,
                &path,
                tx,
                Some(headers.clone()),
                Some(&opts.cancel_token),
            )
            .await?;
            image_paths.push(path);
            done += 1;
            let _ = progress
                .send(ProgressUpdate::percent(done as f64 / total as f64 * 60.0))
                .await;
        }

        let mut music_path = None;
        if let Some(m) = music {
            let path = tmp.join("music.mp3");
            let (tx, _rx) = mpsc::channel(8);
            direct_downloader::download_direct_with_headers(
                &self.client,
                &m.url,
                &path,
                tx,
                Some(headers.clone()),
                Some(&opts.cancel_token),
            )
            .await?;
            music_path = Some(path);
            done += 1;
            let _ = progress
                .send(ProgressUpdate::percent(done as f64 / total as f64 * 60.0))
                .await;
        }

        let (w, h) = photos
            .first()
            .map(|q| (q.width, q.height))
            .unwrap_or((1080, 1920));
        let filename = format!(
            "{}_slideshow.mp4",
            sanitize_filename::sanitize(&info.title)
        );
        let output = opts.output_dir.join(&filename);
        let _ = progress.send(ProgressUpdate::percent(75.0)).await;
        crate::core::media_processor::MediaProcessor::slideshow_from_images(
            &image_paths,
            music_path.as_deref(),
            w,
            h,
            &output.to_string_lossy(),
        )
        .await?;
        let size = tokio::fs::metadata(&output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let _ = progress.send(ProgressUpdate::percent(100.0)).await;
        Ok(DownloadResult {
            file_path: output,
            file_size_bytes: size,
            duration_seconds: 0.0,
            torrent_id: None,
        })
    }
}

#[async_trait]
impl PlatformDownloader for TikTokDownloader {
    fn name(&self) -> &str {
        "tiktok"
    }

    fn can_handle(&self, url: &str) -> bool {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                let host = host.to_lowercase();
                return host == "tiktok.com" || host.ends_with(".tiktok.com");
            }
        }
        false
    }

    async fn get_media_info(&self, url: &str) -> anyhow::Result<MediaInfo> {
        let original_url = url.to_string();

        let post_id = match Self::extract_post_id(url) {
            Some(id) => id,
            None => {
                let canonical = self
                    .resolve_short_link(url)
                    .await
                    .unwrap_or_else(|_| url.to_string());
                Self::extract_post_id(&canonical).unwrap_or_else(|| "unknown".to_string())
            }
        };

        let detail = match self.fetch_detail(&post_id).await {
            Ok(d) => d,
            Err(first_err) => {
                tracing::warn!("[tiktok] native failed: {}, trying yt-dlp", first_err);
                return self
                    .get_media_info_via_ytdlp(&original_url, &post_id)
                    .await
                    .or_else(|_| Err(first_err));
            }
        };

        let author = Self::extract_author(&detail);
        let filename_base = format!(
            "tiktok_{}_{}",
            sanitize_filename::sanitize(&author),
            post_id
        );

        let (video_wm, video_no_wm) = Self::extract_video_urls(&detail);
        let music_url = Self::extract_music_url(&detail);

        if let Some(image_urls) = Self::extract_image_urls(&detail) {
            let (first_w, first_h) = image_urls
                .first()
                .map(|(_, w, h)| (*w, *h))
                .unwrap_or((0, 0));
            let media_type = if image_urls.len() == 1 {
                MediaType::Photo
            } else {
                MediaType::Carousel
            };

            let mut qualities: Vec<VideoQuality> = image_urls
                .iter()
                .enumerate()
                .map(|(i, (u, w, h))| VideoQuality {
                    label: format!("photo_{}", i + 1),
                    width: *w,
                    height: *h,
                    url: u.clone(),
                    format: "jpg".to_string(),
                })
                .collect();

            // Slideshow video, like TikTok plays the post: TikTok's own
            // render when it serves one, otherwise synthesized from the
            // photos + music at download time (no watermark by construction).
            if let Some(no_wm) = &video_no_wm {
                qualities.push(VideoQuality {
                    label: "slideshow".to_string(),
                    width: first_w,
                    height: first_h,
                    url: no_wm.clone(),
                    format: "tiktok_direct_no_wm".to_string(),
                });
            } else if let Some(wm) = &video_wm {
                qualities.push(VideoQuality {
                    label: "slideshow_wm".to_string(),
                    width: first_w,
                    height: first_h,
                    url: wm.clone(),
                    format: "tiktok_direct_wm".to_string(),
                });
            } else {
                qualities.push(VideoQuality {
                    label: "slideshow".to_string(),
                    width: first_w,
                    height: first_h,
                    url: String::new(),
                    format: "tiktok_slideshow".to_string(),
                });
            }

            if let Some(m) = &music_url {
                qualities.push(VideoQuality {
                    label: "audio".to_string(),
                    width: 0,
                    height: 0,
                    url: m.clone(),
                    format: "mp3".to_string(),
                });
            }

            return Ok(MediaInfo {
                title: filename_base,
                author,
                platform: "tiktok".to_string(),
                duration_seconds: None,
                thumbnail_url: image_urls.first().map(|(u, _, _)| u.clone()),
                available_qualities: qualities,
                media_type,
                file_size_bytes: None,
            });
        }

        if video_wm.is_some() || video_no_wm.is_some() {
            let mut qualities: Vec<VideoQuality> = Vec::new();
            if let Some(wm) = &video_wm {
                qualities.push(VideoQuality {
                    label: "video_wm".to_string(),
                    width: 0,
                    height: 0,
                    url: wm.clone(),
                    format: "tiktok_direct_wm".to_string(),
                });
            }
            if let Some(nw) = &video_no_wm {
                qualities.push(VideoQuality {
                    label: "video_no_wm".to_string(),
                    width: 0,
                    height: 0,
                    url: nw.clone(),
                    format: "tiktok_direct_no_wm".to_string(),
                });
            }
            if let Some(m) = &music_url {
                qualities.push(VideoQuality {
                    label: "audio".to_string(),
                    width: 0,
                    height: 0,
                    url: m.clone(),
                    format: "mp3".to_string(),
                });
            }

            return Ok(MediaInfo {
                title: filename_base,
                author,
                platform: "tiktok".to_string(),
                duration_seconds: Self::extract_duration(&detail),
                thumbnail_url: None,
                available_qualities: qualities,
                media_type: MediaType::Video,
                file_size_bytes: None,
            });
        }

        if let Some(music_url) = music_url {
            return Ok(MediaInfo {
                title: filename_base,
                author,
                platform: "tiktok".to_string(),
                duration_seconds: None,
                thumbnail_url: None,
                available_qualities: vec![VideoQuality {
                    label: "audio".to_string(),
                    width: 0,
                    height: 0,
                    url: music_url,
                    format: "mp3".to_string(),
                }],
                media_type: MediaType::Audio,
                file_size_bytes: None,
            });
        }

        tracing::warn!("[tiktok] no media extracted natively, trying yt-dlp fallback");
        self.fallback_ytdlp(&original_url).await
    }

    async fn download(
        &self,
        info: &MediaInfo,
        opts: &DownloadOptions,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        if let Some(quality) = info.available_qualities.first() {
            if quality.format == "ytdlp" {
                let ytdlp_path = crate::core::ytdlp::ensure_ytdlp().await?;
                return crate::core::ytdlp::download_video(
                    &ytdlp_path,
                    &quality.url,
                    &opts.output_dir,
                    None,
                    progress,
                    opts.download_mode.as_deref(),
                    opts.format_id.as_deref(),
                    opts.filename_template.as_deref(),
                    opts.referer.as_deref().or(Some("https://www.tiktok.com/")),
                    opts.cancel_token.clone(),
                    None,
                    opts.concurrent_fragments,
                    false,
                    &[],
                    opts.audio_format.as_deref(),
                )
                .await;
            }
        }

        let cookies = self.captured_cookies.lock().await.clone();
        let mut headers = self.download_headers(&cookies);
        crate::core::http_client::inject_ua_header(&mut headers, opts.user_agent.as_deref());

        // A quality picked in the format list / photo list (or the Music
        // preset) downloads just that item instead of the whole post.
        if let Some(quality) = Self::selected_quality(info, opts) {
            return self
                .download_quality(info, quality, opts, &headers, progress)
                .await;
        }

        match info.media_type {
            MediaType::Video => {
                let quality = info
                    .available_qualities
                    .first()
                    .ok_or_else(|| anyhow!("No video URL available"))?;

                if matches!(
                    quality.format.as_str(),
                    "tiktok_direct" | "tiktok_direct_wm" | "tiktok_direct_no_wm"
                ) {
                    let filename = format!("{}.mp4", sanitize_filename::sanitize(&info.title));
                    let output = opts.output_dir.join(&filename);
                    return self
                        .download_direct_video(
                            &quality.url,
                            &output,
                            info.duration_seconds.unwrap_or(0.0),
                            &headers,
                            opts,
                            progress,
                        )
                        .await;
                }

                let ytdlp_path = match &opts.ytdlp_path {
                    Some(p) => p.clone(),
                    None => crate::core::ytdlp::find_ytdlp_cached()
                        .await
                        .ok_or_else(|| anyhow!("yt-dlp not found"))?,
                };

                crate::core::ytdlp::download_video(
                    &ytdlp_path,
                    &quality.url,
                    &opts.output_dir,
                    None,
                    progress,
                    opts.download_mode.as_deref(),
                    None,
                    opts.filename_template.as_deref(),
                    opts.referer.as_deref().or(Some("https://www.tiktok.com/")),
                    opts.cancel_token.clone(),
                    None,
                    opts.concurrent_fragments,
                    false,
                    &[],
                    opts.audio_format.as_deref(),
                )
                .await
            }
            MediaType::Photo | MediaType::Carousel => {
                // Only the photo qualities are files here: the same info
                // also carries the slideshow video + audio track as options.
                let photos: Vec<&VideoQuality> = info
                    .available_qualities
                    .iter()
                    .filter(|q| {
                        matches!(q.format.as_str(), "jpg" | "jpeg" | "png" | "webp")
                    })
                    .collect();
                if photos.is_empty() {
                    return Err(anyhow!("No photos available"));
                }

                let mut total_bytes = 0u64;
                let count = photos.len();
                let mut last_path = opts.output_dir.clone();

                for (i, quality) in photos.iter().enumerate() {
                    let filename = if count == 1 {
                        format!("{}.jpg", sanitize_filename::sanitize(&info.title))
                    } else {
                        format!(
                            "{}_photo_{}.jpg",
                            sanitize_filename::sanitize(&info.title),
                            i + 1
                        )
                    };
                    let output = opts.output_dir.join(&filename);
                    let (tx, _rx) = mpsc::channel(8);

                    let bytes = direct_downloader::download_direct_with_headers(
                        &self.client,
                        &quality.url,
                        &output,
                        tx,
                        Some(headers.clone()),
                        Some(&opts.cancel_token),
                    )
                    .await?;

                    total_bytes += bytes;
                    last_path = output;

                    let percent = ((i + 1) as f64 / count as f64) * 100.0;
                    let _ = progress.send(ProgressUpdate::percent(percent)).await;
                }

                Ok(DownloadResult {
                    file_path: last_path,
                    file_size_bytes: total_bytes,
                    duration_seconds: 0.0,
                    torrent_id: None,
                })
            }
            MediaType::Audio => {
                let quality = info
                    .available_qualities
                    .first()
                    .ok_or_else(|| anyhow!("No audio URL available"))?;

                let filename = format!("{}.mp3", sanitize_filename::sanitize(&info.title));
                let output = opts.output_dir.join(&filename);

                let bytes = direct_downloader::download_direct_with_headers(
                    &self.client,
                    &quality.url,
                    &output,
                    progress,
                    Some(headers),
                    Some(&opts.cancel_token),
                )
                .await?;

                Ok(DownloadResult {
                    file_path: output,
                    file_size_bytes: bytes,
                    duration_seconds: 0.0,
                    torrent_id: None,
                })
            }
            _ => Err(anyhow!("Unsupported media type for download")),
        }
    }
}
