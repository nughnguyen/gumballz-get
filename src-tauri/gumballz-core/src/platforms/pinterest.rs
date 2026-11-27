use crate::models::progress::ProgressUpdate;
use std::sync::LazyLock;

use anyhow::anyhow;
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, REFERER};
use tokio::sync::mpsc;

use crate::core::direct_downloader;
use crate::core::redirect;
use crate::models::media::{DownloadOptions, DownloadResult, MediaInfo, MediaType, VideoQuality};
use crate::platforms::traits::PlatformDownloader;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

static PIN_NOT_FOUND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""__typename"\s*:\s*"PinNotFound""#).expect("valid PIN_NOT_FOUND_RE")
});

static VIDEO_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""url":"(https://v1\.pinimg\.com/videos/.*?)""#).expect("valid VIDEO_URL_RE")
});

static IMAGE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"src="(https://i\.pinimg\.com/[^"]+\.(jpg|jpeg|png|gif|webp))""#).expect("valid IMAGE_URL_RE")
});

// Structured video renditions from the Relay `videoList` blob. `v720P` is
// the canonical `mc/720p` MP4; some pins only carry an `expMp4` 720w file.
static VIDEO_1080P_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""v1080P":\{[^}]*?"url":"(https://v1\.pinimg\.com/videos/[^"]*\.mp4)""#)
        .expect("valid VIDEO_1080P_RE")
});
static VIDEO_720P_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""v720P":\{[^}]*?"url":"(https://v1\.pinimg\.com/videos/[^"]*\.mp4)""#)
        .expect("valid VIDEO_720P_RE")
});
static VIDEO_720P_OBJ_RE: LazyLock<Regex> = LazyLock::new(|| {
    // A rendition object, tolerating one level of nesting (the
    // captionsUrls object) so fields after it stay reachable.
    Regex::new(r#""v720P":\{((?:[^{}]|\{[^{}]*\})*)\}"#).expect("valid VIDEO_720P_OBJ_RE")
});
static VIDEO_HLS_OBJ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""vHLSV[^"]*":\{((?:[^{}]|\{[^{}]*\})*)\}"#).expect("valid VIDEO_HLS_OBJ_RE")
});
// Field readers applied to a rendition-object body, so key order inside
// the object does not matter.
static JSON_WIDTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""width":(\d+)"#).expect("valid JSON_WIDTH_RE")
});
static JSON_HEIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""height":(\d+)"#).expect("valid JSON_HEIGHT_RE")
});
static JSON_DURATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""duration":(\d+)"#).expect("valid JSON_DURATION_RE")
});
static JSON_THUMB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""thumbnail":"(https://[^"]+)""#).expect("valid JSON_THUMB_RE")
});
static VIDEO_MC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""url":"(https://v1\.pinimg\.com/videos/mc/[^"]*\.mp4)""#)
        .expect("valid VIDEO_MC_RE")
});
static VIDEO_EXP_MP4_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""url":"(https://v1\.pinimg\.com/videos/[^"]*expMp4[^"]*\.mp4)""#)
        .expect("valid VIDEO_EXP_MP4_RE")
});
static VIDEO_URLS_ARRAY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""videoUrls":\[([^\]]+)\]"#).expect("valid VIDEO_URLS_ARRAY_RE")
});
static VIDEO_ARRAY_MP4_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(https://v1\.pinimg\.com/videos/[^"]*\.mp4)"#)
        .expect("valid VIDEO_ARRAY_MP4_RE")
});
static VIDEO_POSTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<video[^>]*poster="([^"]+)""#).expect("valid VIDEO_POSTER_RE")
});

// The pin's own image. `images_orig` is the ONLY place the original-size
// URL lives: the closeup <img src="..."> is always the 736x transcode, and
// the old regex only looked at src="...", so its "prefer originals" branch
// could never fire. Relay emits images_orig twice (once with __typename,
// once with height/width); both shapes are matched.
static IMAGES_ORIG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""images_orig":\{[^}]*?"url":"(https://i\.pinimg\.com/originals/[^"]+)""#)
        .expect("valid IMAGES_ORIG_RE")
});
static IMAGES_ORIG_DIMS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""images_orig":\{"url":"[^"]+","height":(\d+),"width":(\d+)"#)
        .expect("valid IMAGES_ORIG_DIMS_RE")
});
static IMAGES_736X_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""images_736x":\{[^}]*?"url":"(https://i\.pinimg\.com/736x/[^"]+)""#)
        .expect("valid IMAGES_736X_RE")
});
static IMAGES_736X_DIMS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""images_736x":\{"width":(\d+),"height":(\d+)"#)
        .expect("valid IMAGES_736X_DIMS_RE")
});

// Fallback for the closeup <img>: extended to png/webp/jpeg (the old regex
// only accepted jpg|gif, so a pin whose closeup is a png/webp found
// nothing). og:image is emitted content-first:
// `<meta content="https://i.pinimg.com/736x/..." ... property="og:image"/>`.
static OG_IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<meta content="(https://i\.pinimg\.com/[^"]+)"[^>]*property="og:image""#)
        .expect("valid OG_IMAGE_RE")
});

pub struct PinterestDownloader {
    client: reqwest::Client,
}

impl Default for PinterestDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl PinterestDownloader {
    pub fn new() -> Self {
        let mut builder = crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(15));

        if let Some(jar) =
            crate::core::cookie_parser::load_extension_cookies_for_domain("pinterest.com")
        {
            builder = builder.cookie_provider(jar);
        }

        let client = builder.build().unwrap_or_default();
        Self { client }
    }

    /// Full browser-navigation headers for fetching pin pages, same
    /// approach as the TikTok fix: the old request carried only the client
    /// User-Agent plus Accept/Accept-Language, which is the fingerprint
    /// Pinterest's bot checks flag. A real Chrome navigation sends these.
    fn page_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, HeaderValue::from_static("https://www.pinterest.com/"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        headers.insert(
            HeaderName::from_static("sec-fetch-dest"),
            HeaderValue::from_static("document"),
        );
        headers.insert(
            HeaderName::from_static("sec-fetch-mode"),
            HeaderValue::from_static("navigate"),
        );
        headers.insert(
            HeaderName::from_static("sec-fetch-site"),
            HeaderValue::from_static("same-origin"),
        );
        headers.insert(
            HeaderName::from_static("sec-fetch-user"),
            HeaderValue::from_static("?1"),
        );
        headers.insert(
            HeaderName::from_static("sec-ch-ua"),
            HeaderValue::from_static(
                "\"Google Chrome\";v=\"141\", \"Chromium\";v=\"141\", \"Not-A/Brand\";v=\"99\"",
            ),
        );
        headers.insert(
            HeaderName::from_static("sec-ch-ua-mobile"),
            HeaderValue::from_static("?0"),
        );
        headers.insert(
            HeaderName::from_static("sec-ch-ua-platform"),
            HeaderValue::from_static("\"Windows\""),
        );
        headers.insert(
            HeaderName::from_static("upgrade-insecure-requests"),
            HeaderValue::from_static("1"),
        );
        headers
    }

    /// File extension for a media URL, so the downloaded file keeps the
    /// pin's real format (a png original must not be saved as .jpg).
    fn format_from_url(url: &str) -> &'static str {
        let base = url.split('?').next().unwrap_or(url);
        if base.ends_with(".png") {
            "png"
        } else if base.ends_with(".gif") {
            "gif"
        } else if base.ends_with(".webp") {
            "webp"
        } else {
            "jpg"
        }
    }

    /// Rank a `src="..."` image URL. Thumbnails and avatars score -1 (the
    /// old code returned the first src match, which on video pins was the
    /// 75x75 avatar); the closeup transcodes rank below the original.
    fn score_image_url(url: &str) -> i32 {
        if url.contains("/videos/thumbnails/") {
            return -1;
        }
        if url.contains("originals") || url.contains("1200x") {
            100
        } else if url.contains("/736x/") {
            80
        } else if url.contains("/564x/") {
            70
        } else if url.contains("/474x/") {
            60
        } else if url.contains("/236x/") {
            40
        } else {
            -1
        }
    }

    /// A video pin renders a <video> player and a Relay `videoList`; image
    /// pins have neither. Used to keep a video pin with no MP4 rendition
    /// from silently falling through to a wrong (related-pin/avatar) image.
    fn is_video_pin(html: &str) -> bool {
        html.contains("web-video-player")
            || html.contains("\"videoList\"")
            || html.contains("<video ")
    }

    /// HEVC preview clips in the `videoUrls` array are named
    /// `<hash>_tN.mp4` (e.g. `_t1.mp4`); they are short low-tier
    /// renditions, only a last resort when no full MP4 exists.
    fn is_hevc_preview(url: &str) -> bool {
        let file = url.rsplit('/').next().unwrap_or("");
        let bytes = file.as_bytes();
        bytes.len() >= 7
            && file.ends_with(".mp4")
            && bytes[bytes.len() - 7] == b'_'
            && bytes[bytes.len() - 6] == b't'
            && bytes[bytes.len() - 5].is_ascii_digit()
    }

    /// Read an integer field out of a JSON object body.
    fn json_int(body: &str, re: &Regex) -> Option<u32> {
        re.captures(body)
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse().ok())
    }

    fn extract_pin_id(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let segments: Vec<&str> = parsed.path().split('/').filter(|s| !s.is_empty()).collect();

        if segments.first() == Some(&"pin") {
            let raw_id = segments.get(1)?;
            if raw_id.contains("--") {
                return raw_id.split("--").last().map(|s| s.to_string());
            }
            return Some(raw_id.to_string());
        }

        if segments.first() == Some(&"url_shortener") {
            return None;
        }

        segments.last().map(|s| {
            if s.contains("--") {
                s.split("--").last().unwrap_or(s).to_string()
            } else {
                s.to_string()
            }
        })
    }

    fn is_short_link(url: &str) -> bool {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                return host == "pin.it";
            }
        }
        false
    }

    async fn resolve_pin_url(&self, url: &str) -> anyhow::Result<String> {
        if Self::is_short_link(url) {
            let canonical = redirect::resolve_redirect(&self.client, url).await?;
            return Ok(canonical);
        }
        Ok(url.to_string())
    }

    async fn fetch_pin_html(&self, pin_id: &str) -> anyhow::Result<String> {
        let url = format!("https://www.pinterest.com/pin/{}/", pin_id);

        let response = self
            .client
            .get(&url)
            .headers(Self::page_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP {} ao acessar pin {}",
                response.status(),
                pin_id
            ));
        }

        response.text().await.map_err(Into::into)
    }

    fn check_pin_not_found(html: &str) -> bool {
        PIN_NOT_FOUND_RE.is_match(html)
    }

    /// Best downloadable MP4 for a video pin, with its width/height (from
    /// the `v720P` rendition when that is the pick), poster thumbnail and
    /// duration in seconds. Priority: v1080P > v720P (`mc/720p`) > any
    /// `mc/` MP4 > `expMp4` 720w > any other `"url"` MP4 (skipping the
    /// HEVC `_tN` preview clips while a better rendition exists) > the
    /// `"videoUrls"` array. HLS `.m3u8` / DASH `.mpd` entries are never
    /// picked: the direct downloader cannot play a playlist. An HLS-only
    /// page (Idea pins, where the MP4 renditions are duration-only stubs)
    /// returns `None` here so the yt-dlp fallback gets the pin.
    /// Returns `None` when the page carries no MP4 at all.
    fn extract_video(html: &str) -> Option<(String, u32, u32, Option<String>, Option<f64>)> {
        let pick = |re: &Regex| {
            re.captures(html)
                .and_then(|cap| cap.get(1))
                .map(|m| m.as_str().to_string())
        };

        let mut url = pick(&VIDEO_1080P_RE).or_else(|| pick(&VIDEO_720P_RE));

        // Dimensions, duration and thumbnail come from the rendition
        // objects themselves, read field-by-field so key order inside
        // the object does not matter (some pins emit height before
        // width; some MP4 renditions are duration-only stubs while the
        // HLS rendition carries the real dimensions). The <video>
        // poster only exists in the fuller server-rendered variant.
        let (mut width, mut height, mut duration_seconds) = (0u32, 0u32, None);
        let mut thumbnail = VIDEO_POSTER_RE
            .captures(html)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string());
        // Every rendition object in the page, v720P first: the first
        // occurrence is often a stub (duration-only), the complete one
        // comes later.
        let mut bodies: Vec<&str> = Vec::new();
        for obj_re in [&VIDEO_720P_OBJ_RE, &VIDEO_HLS_OBJ_RE] {
            for cap in obj_re.captures_iter(html) {
                if let Some(body) = cap.get(1) {
                    bodies.push(body.as_str());
                }
            }
        }
        for body in bodies {
            if width == 0 {
                width = Self::json_int(body, &JSON_WIDTH_RE).unwrap_or(0);
                height = Self::json_int(body, &JSON_HEIGHT_RE).unwrap_or(0);
            }
            if duration_seconds.is_none() {
                duration_seconds =
                    Self::json_int(body, &JSON_DURATION_RE).map(|ms| ms as f64 / 1000.0);
            }
            if thumbnail.is_none() {
                thumbnail = JSON_THUMB_RE
                    .captures(body)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string());
            }
            if width != 0 && duration_seconds.is_some() && thumbnail.is_some() {
                break;
            }
        }
        // Last resort: the 736x still image of the pin.
        if thumbnail.is_none() {
            thumbnail = IMAGES_736X_RE
                .captures(html)
                .and_then(|cap| cap.get(1))
                .map(|m| m.as_str().to_string());
        }

        if url.is_none() {
            url = pick(&VIDEO_MC_RE).or_else(|| pick(&VIDEO_EXP_MP4_RE));
        }

        if url.is_none() {
            let all: Vec<String> = VIDEO_URL_RE
                .captures_iter(html)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .filter(|u| u.ends_with(".mp4"))
                .collect();
            url = all
                .iter()
                .find(|u| !Self::is_hevc_preview(u))
                .cloned()
                .or_else(|| all.into_iter().next());
        }

        if url.is_none() {
            if let Some(cap) = VIDEO_URLS_ARRAY_RE.captures(html) {
                if let Some(list) = cap.get(1) {
                    let urls: Vec<String> = VIDEO_ARRAY_MP4_RE
                        .captures_iter(list.as_str())
                        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                        .collect();
                    url = urls
                        .iter()
                        .find(|u| u.contains("/mc/"))
                        .cloned()
                        .or_else(|| {
                            urls.iter().find(|u| !Self::is_hevc_preview(u)).cloned()
                        })
                        .or_else(|| urls.into_iter().next());
                }
            }
        }

        url.map(|u| (u, width, height, thumbnail, duration_seconds))
    }

    /// The pin's image as (url, format, width, height). Primary source is
    /// the Relay `images_orig` URL — the only place the original lives —
    /// with dimensions from its own object, else the 736x object. Falls
    /// back to the best scored closeup `src="..."` (originals/736x/564x/
    /// 474x; avatars and video thumbnails are excluded), then og:image.
    fn extract_image_url(html: &str) -> Option<(String, String, u32, u32)> {
        if let Some(cap) = IMAGES_ORIG_RE.captures(html) {
            if let Some(url_match) = cap.get(1) {
                let url = url_match.as_str().to_string();
                let format = Self::format_from_url(&url).to_string();
                let (mut width, mut height) = (0u32, 0u32);
                if let Some(dims) = IMAGES_ORIG_DIMS_RE.captures(html) {
                    height = dims
                        .get(1)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    width = dims
                        .get(2)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                }
                if width == 0 {
                    if let Some(dims) = IMAGES_736X_DIMS_RE.captures(html) {
                        width = dims
                            .get(1)
                            .and_then(|m| m.as_str().parse().ok())
                            .unwrap_or(0);
                        height = dims
                            .get(2)
                            .and_then(|m| m.as_str().parse().ok())
                            .unwrap_or(0);
                    }
                }
                return Some((url, format, width, height));
            }
        }

        let mut best: Option<(String, i32)> = None;
        for cap in IMAGE_URL_RE.captures_iter(html) {
            if let Some(url_match) = cap.get(1) {
                let url = url_match.as_str().to_string();
                let score = Self::score_image_url(&url);
                if score > best.as_ref().map(|(_, s)| *s).unwrap_or(-1) {
                    best = Some((url, score));
                }
            }
        }
        if let Some((url, _)) = best {
            let format = Self::format_from_url(&url).to_string();
            let (mut width, mut height) = (0u32, 0u32);
            if let Some(dims) = IMAGES_736X_DIMS_RE.captures(html) {
                width = dims
                    .get(1)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                height = dims
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
            }
            return Some((url, format, width, height));
        }

        if let Some(cap) = OG_IMAGE_RE.captures(html) {
            if let Some(url_match) = cap.get(1) {
                let url = url_match.as_str().to_string();
                let format = Self::format_from_url(&url).to_string();
                return Some((url, format, 0, 0));
            }
        }

        None
    }
}

#[async_trait]
impl PlatformDownloader for PinterestDownloader {
    fn name(&self) -> &str {
        "pinterest"
    }

    fn can_handle(&self, url: &str) -> bool {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                let host = host.to_lowercase();
                return host == "pin.it"
                    || host.contains("pinterest.com")
                    || host.contains("pinterest.ca")
                    || host.contains("pinterest.co.uk")
                    || host.contains("pinterest.fr")
                    || host.contains("pinterest.de")
                    || host.contains("pinterest.es")
                    || host.contains("pinterest.it")
                    || host.contains("pinterest.pt")
                    || host.contains("pinterest.jp")
                    || host.contains("pinterest.kr")
                    || host.contains("pinterest.com.br")
                    || host.contains("pinterest.com.mx")
                    || host.contains("pinterest.co.kr")
                    || host.contains("pinterest.cl")
                    || host.contains("pinterest.at")
                    || host.contains("pinterest.ch")
                    || host.contains("pinterest.com.au")
                    || host.contains("pinterest.co.in")
                    || host.contains("pinterest.nz")
                    || host.contains("pinterest.ph")
                    || host.contains("pinterest.ru")
                    || host.contains("pinterest.se")
                    || host.contains("pinterest.dk")
                    || host.contains("pinterest.");
            }
        }
        false
    }

    async fn get_media_info(&self, url: &str) -> anyhow::Result<MediaInfo> {
        match self.native_get_media_info(url).await {
            Ok(info) => Ok(info),
            Err(native_err) => {
                tracing::warn!(
                    "[pinterest] native failed: {}, trying yt-dlp fallback",
                    native_err
                );
                self.fallback_ytdlp(url).await.map_err(|_| native_err)
            }
        }
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
                    opts.referer
                        .as_deref()
                        .or(Some("https://www.pinterest.com/")),
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

        let quality = info
            .available_qualities
            .first()
            .ok_or_else(|| anyhow!("No media URL available"))?;

        let extension = &quality.format;
        let filename = format!("{}.{}", info.title, extension);
        let safe_filename = sanitize_filename::sanitize(&filename);
        let output_path = opts.output_dir.join(&safe_filename);

        let total_bytes = direct_downloader::download_direct(
            &self.client,
            &quality.url,
            &output_path,
            progress,
            Some(&opts.cancel_token),
        )
        .await?;

        Ok(DownloadResult {
            file_path: output_path,
            file_size_bytes: total_bytes,
            duration_seconds: 0.0,
            torrent_id: None,
        })
    }
}

impl PinterestDownloader {
    async fn fallback_ytdlp(&self, url: &str) -> anyhow::Result<MediaInfo> {
        let ytdlp_path = crate::core::ytdlp::ensure_ytdlp().await?;
        let json = crate::core::ytdlp::get_video_info(&ytdlp_path, url, &[]).await?;
        crate::platforms::generic_ytdlp::GenericYtdlpDownloader::parse_video_info(&json)
    }

    async fn native_get_media_info(&self, url: &str) -> anyhow::Result<MediaInfo> {
        let canonical = self.resolve_pin_url(url).await?;

        let pin_id =
            Self::extract_pin_id(&canonical).ok_or_else(|| anyhow!("Could not extract pin ID"))?;

        let html = self.fetch_pin_html(&pin_id).await?;

        if Self::check_pin_not_found(&html) {
            return Err(anyhow!("Pin not found"));
        }

        // A login wall / skeleton page carries no pin data at all: fail
        // loudly so the yt-dlp fallback (which handles video pins) gets a
        // chance, instead of returning a wrong image below.
        if !html.contains("images_orig")
            && !html.contains("pin-closeup-image")
            && !html.contains("\"videoList\"")
        {
            return Err(anyhow!(
                "Pinterest returned a page without pin media (login wall or block) for pin {}",
                pin_id
            ));
        }

        if let Some((video_url, width, height, thumbnail, duration_seconds)) =
            Self::extract_video(&html)
        {
            return Ok(MediaInfo {
                title: format!("pinterest_{}", pin_id),
                author: String::new(),
                platform: "pinterest".to_string(),
                duration_seconds,
                thumbnail_url: thumbnail,
                available_qualities: vec![VideoQuality {
                    label: "original".to_string(),
                    width,
                    height,
                    url: video_url,
                    format: "mp4".to_string(),
                }],
                media_type: MediaType::Video,
                file_size_bytes: None,
            });
        }

        // A video pin with no downloadable MP4 must NOT fall through to
        // the image extractor: its only `src="..."` images are the
        // pinner's avatar and related pins. Let the yt-dlp fallback
        // (HLS-capable) handle it instead of saving the wrong file.
        if Self::is_video_pin(&html) {
            return Err(anyhow!(
                "Video pin {} has no downloadable MP4 in the page",
                pin_id
            ));
        }

        if let Some((image_url, fmt, width, height)) = Self::extract_image_url(&html) {
            let media_type = if fmt == "gif" {
                MediaType::Gif
            } else {
                MediaType::Photo
            };
            let thumbnail_url = IMAGES_736X_RE
                .captures(&html)
                .and_then(|cap| cap.get(1))
                .map(|m| m.as_str().to_string());

            return Ok(MediaInfo {
                title: format!("pinterest_{}", pin_id),
                author: String::new(),
                platform: "pinterest".to_string(),
                duration_seconds: None,
                thumbnail_url,
                available_qualities: vec![VideoQuality {
                    label: "original".to_string(),
                    width,
                    height,
                    url: image_url,
                    format: fmt,
                }],
                media_type,
                file_size_bytes: None,
            });
        }

        Err(anyhow!("No media found in pin {}", pin_id))
    }
}
