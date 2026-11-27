//! Spotify track downloader — no login, no API keys, no client secret.
//!
//! Evidence (VM simulation, 2026-11-27): yt-dlp refuses Spotify outright —
//! `[DRM] The requested site is known to use DRM protection`, exit 1, no JSON
//! at all — so the full track audio cannot be fetched by any means this app
//! has. What Spotify does serve without login is the public embed page
//! (`open.spotify.com/embed/track/<id>`), whose `__NEXT_DATA__` JSON carries
//! the track title, artists, duration, cover art (up to 640px) and a
//! 30-second MP3 preview (`p.scdn.co/mp3-preview/...`). This downloader
//! offers exactly those two honest artifacts:
//!
//! * `"30s preview"` — the real Spotify 30-second preview audio (MP3).
//!   It is labelled as a preview everywhere; it is never the full track.
//! * `"cover art"` — the track's cover image (JPEG, largest size offered).
//!
//! Only track links are supported (`open.spotify.com/track/<id>`,
//! `spotify.link` short links, `spotify:track:<id>` URIs). Album / playlist /
//! artist / episode links are not handled here and stay with the generic path.
//!
//! The embed page also embeds an anonymous session token; this module never
//! reads, logs or sends it — only the `state.data.entity` metadata subtree
//! is parsed.

use crate::core::direct_downloader;
use crate::core::redirect;
use crate::models::media::{DownloadOptions, DownloadResult, MediaInfo, MediaType, VideoQuality};
use crate::models::progress::ProgressUpdate;
use crate::platforms::traits::PlatformDownloader;
use std::sync::LazyLock;

use anyhow::anyhow;
use async_trait::async_trait;
use regex::Regex;
use tokio::sync::mpsc;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Labels shown to the user for the two downloadable artifacts. The preview
/// label says "30s" on purpose: the file is a 30-second sample, not the song.
pub const PREVIEW_LABEL: &str = "30s preview";
pub const COVER_LABEL: &str = "cover art";

/// Spotify previews are 30 seconds by design; the preview file carries no
/// duration of its own in the metadata, so the download result reports this.
const PREVIEW_DURATION_SECONDS: f64 = 30.0;

static NEXT_DATA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<script id="__NEXT_DATA__"[^>]*>(.*?)</script>"#)
        .expect("valid NEXT_DATA_RE")
});

/// Metadata parsed from the public embed page. No album name is available
/// there (the entity object carries no album field), and the full-track
/// duration is the track's length, not the preview's.
#[derive(Debug, Clone, PartialEq)]
struct TrackMeta {
    title: String,
    artists: Vec<String>,
    /// Full track length in seconds, when the page reports it.
    duration_seconds: Option<f64>,
    cover_url: Option<String>,
    cover_width: u32,
    cover_height: u32,
    /// 30-second preview audio URL, when Spotify provides one.
    preview_url: Option<String>,
}

pub struct SpotifyDownloader {
    client: reqwest::Client,
}

impl Default for SpotifyDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl SpotifyDownloader {
    pub fn new() -> Self {
        let client = crate::core::http_client::apply_global_proxy(reqwest::Client::builder())
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Track ID from a URL or `spotify:track:<id>` URI, without any network.
    /// Accepts `/track/<id>` under `open.spotify.com` (including locale
    /// prefixes such as `/intl-de/track/<id>` and `/embed/track/<id>`).
    /// `spotify.link` short links cannot be read this way — they need a
    /// redirect resolution first (see [`Self::resolve_track_id`]).
    fn extract_track_id(url: &str) -> Option<String> {
        if let Some(rest) = url.strip_prefix("spotify:track:") {
            let id = rest.split([':', '?', '#', '/']).next()?;
            return if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            };
        }
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.to_lowercase();
        if host != "open.spotify.com" && !host.ends_with(".open.spotify.com") {
            return None;
        }
        let segments: Vec<&str> = parsed
            .path()
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        segments
            .iter()
            .position(|s| *s == "track")
            .and_then(|i| segments.get(i + 1))
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string())
    }

    fn is_short_link(url: &str) -> bool {
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
            .map(|h| h == "spotify.link")
            .unwrap_or(false)
    }

    async fn resolve_track_id(&self, url: &str) -> anyhow::Result<String> {
        if let Some(id) = Self::extract_track_id(url) {
            return Ok(id);
        }
        if Self::is_short_link(url) {
            let canonical = redirect::resolve_redirect(&self.client, url).await?;
            if let Some(id) = Self::extract_track_id(&canonical) {
                return Ok(id);
            }
            return Err(anyhow!(
                "Spotify short link did not resolve to a track: {}",
                canonical
            ));
        }
        Err(anyhow!(
            "Only Spotify track links are supported (open.spotify.com/track/..., spotify.link, spotify:track:...)"
        ))
    }

    async fn fetch_embed_html(&self, track_id: &str) -> anyhow::Result<String> {
        let url = format!("https://open.spotify.com/embed/track/{}", track_id);
        let response = self
            .client
            .get(&url)
            .header("Accept", "text/html")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Spotify returned HTTP {} for track {}",
                response.status(),
                track_id
            ));
        }
        response.text().await.map_err(Into::into)
    }

    /// Parse the embed page's `__NEXT_DATA__` JSON. Only the
    /// `props.pageProps.state.data.entity` subtree is read — the page also
    /// embeds an anonymous session token, which is deliberately ignored.
    fn parse_embed_entity(html: &str) -> anyhow::Result<TrackMeta> {
        let json_text = NEXT_DATA_RE
            .captures(html)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| anyhow!("Spotify embed page has no track data"))?;
        let root: serde_json::Value = serde_json::from_str(json_text)
            .map_err(|_| anyhow!("Spotify embed page data is not valid JSON"))?;
        let entity = root
            .pointer("/props/pageProps/state/data/entity")
            .ok_or_else(|| anyhow!("Spotify embed page has no track data"))?;

        let entity_type = entity
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if entity_type != "track" {
            return Err(anyhow!(
                "Only Spotify track links are supported (got {})",
                if entity_type.is_empty() {
                    "unknown"
                } else {
                    entity_type
                }
            ));
        }

        let title = entity
            .get("title")
            .or_else(|| entity.get("name"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("Spotify track has no title"))?
            .to_string();

        let artists: Vec<String> = entity
            .get("artists")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                    .filter(|n| !n.is_empty())
                    .map(|n| n.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let duration_seconds = entity
            .get("duration")
            .and_then(|v| v.as_u64())
            .map(|ms| ms as f64 / 1000.0);

        // Cover art: largest image the page offers (usually 640px).
        let mut cover_url: Option<String> = None;
        let mut cover_width = 0u32;
        let mut cover_height = 0u32;
        if let Some(images) = entity
            .pointer("/visualIdentity/image")
            .and_then(|v| v.as_array())
        {
            for img in images {
                let width = img
                    .get("maxWidth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let height = img
                    .get("maxHeight")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let url = img.get("url").and_then(|v| v.as_str());
                if let Some(url) = url {
                    if width >= cover_width {
                        cover_url = Some(url.to_string());
                        cover_width = width;
                        cover_height = height;
                    }
                }
            }
        }

        let preview_url = entity
            .pointer("/audioPreview/url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        Ok(TrackMeta {
            title,
            artists,
            duration_seconds,
            cover_url,
            cover_width,
            cover_height,
            preview_url,
        })
    }

    fn build_media_info(meta: &TrackMeta) -> anyhow::Result<MediaInfo> {
        let mut qualities: Vec<VideoQuality> = Vec::new();
        if let Some(preview_url) = &meta.preview_url {
            qualities.push(VideoQuality {
                label: PREVIEW_LABEL.to_string(),
                width: 0,
                height: 0,
                url: preview_url.clone(),
                format: "mp3".to_string(),
            });
        }
        if let Some(cover_url) = &meta.cover_url {
            qualities.push(VideoQuality {
                label: COVER_LABEL.to_string(),
                width: meta.cover_width,
                height: meta.cover_height,
                url: cover_url.clone(),
                format: "jpg".to_string(),
            });
        }
        if qualities.is_empty() {
            return Err(anyhow!(
                "Spotify has no preview audio or cover art for this track"
            ));
        }
        Ok(MediaInfo {
            title: meta.title.clone(),
            author: meta.artists.join(", "),
            platform: "spotify".to_string(),
            duration_seconds: meta.duration_seconds,
            thumbnail_url: meta.cover_url.clone(),
            available_qualities: qualities,
            media_type: MediaType::Audio,
            file_size_bytes: None,
        })
    }

    fn file_stem(info: &MediaInfo) -> String {
        if info.author.is_empty() {
            info.title.clone()
        } else {
            format!("{} - {}", info.author, info.title)
        }
    }
}

#[async_trait]
impl PlatformDownloader for SpotifyDownloader {
    fn name(&self) -> &str {
        "spotify"
    }

    fn can_handle(&self, url: &str) -> bool {
        Self::extract_track_id(url).is_some() || Self::is_short_link(url)
    }

    async fn get_media_info(&self, url: &str) -> anyhow::Result<MediaInfo> {
        // No yt-dlp fallback: yt-dlp refuses Spotify outright (DRM), so a
        // fallback would only repeat the same failure.
        let track_id = self.resolve_track_id(url).await?;
        let html = self.fetch_embed_html(&track_id).await?;
        let meta = Self::parse_embed_entity(&html)?;
        Self::build_media_info(&meta)
    }

    async fn download(
        &self,
        info: &MediaInfo,
        opts: &DownloadOptions,
        progress: mpsc::Sender<ProgressUpdate>,
    ) -> anyhow::Result<DownloadResult> {
        let selected = opts
            .quality
            .as_deref()
            .and_then(|wanted| {
                info.available_qualities
                    .iter()
                    .find(|q| q.label == *wanted)
            })
            .or_else(|| info.available_qualities.first())
            .ok_or_else(|| anyhow!("No media URL available"))?;

        let stem = sanitize_filename::sanitize(&Self::file_stem(info));
        let (filename, duration_seconds) = match selected.format.as_str() {
            "mp3" => (
                format!("{} (30s preview).mp3", stem),
                PREVIEW_DURATION_SECONDS,
            ),
            "jpg" => (format!("{} (cover).jpg", stem), 0.0),
            other => {
                return Err(anyhow!("Unsupported Spotify quality format: {}", other));
            }
        };
        let output_path = opts.output_dir.join(&filename);

        let total_bytes = direct_downloader::download_direct(
            &self.client,
            &selected.url,
            &output_path,
            progress,
            Some(&opts.cancel_token),
        )
        .await?;

        Ok(DownloadResult {
            file_path: output_path,
            file_size_bytes: total_bytes,
            duration_seconds,
            torrent_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::Platform;

    #[test]
    fn extract_track_id_from_open_links() {
        assert_eq!(
            SpotifyDownloader::extract_track_id(
                "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"
            ),
            Some("4uLU6hMCjMI75M1A2tKUQC".to_string())
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id(
                "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=abc123"
            ),
            Some("4uLU6hMCjMI75M1A2tKUQC".to_string())
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id(
                "https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC"
            ),
            Some("4uLU6hMCjMI75M1A2tKUQC".to_string())
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id(
                "https://open.spotify.com/embed/track/4uLU6hMCjMI75M1A2tKUQC"
            ),
            Some("4uLU6hMCjMI75M1A2tKUQC".to_string())
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id("spotify:track:4uLU6hMCjMI75M1A2tKUQC"),
            Some("4uLU6hMCjMI75M1A2tKUQC".to_string())
        );
    }

    #[test]
    fn extract_track_id_rejects_non_tracks() {
        assert_eq!(
            SpotifyDownloader::extract_track_id(
                "https://open.spotify.com/album/1zCNyM9y5o2iK4XxXW7QyG"
            ),
            None
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id("https://open.spotify.com/"),
            None
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id("https://spotify.link/abc123"),
            None
        );
        assert_eq!(
            SpotifyDownloader::extract_track_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            None
        );
    }

    #[test]
    fn can_handle_accepts_tracks_and_short_links() {
        let dl = SpotifyDownloader::new();
        assert!(dl.can_handle("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"));
        assert!(dl.can_handle("https://spotify.link/abc123"));
        assert!(dl.can_handle("spotify:track:4uLU6hMCjMI75M1A2tKUQC"));
        assert!(!dl.can_handle(
            "https://open.spotify.com/album/1zCNyM9y5o2iK4XxXW7QyG"
        ));
        assert!(!dl.can_handle("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
    }

    fn sample_embed_html() -> String {
        // Shape mirrors the real embed page's __NEXT_DATA__ (track
        // 4uLU6hMCjMI75M1A2tKUQC, fetched 2026-11-27); URLs are placeholders.
        // The anonymous session token the real page also embeds is
        // deliberately absent here and never read by the parser.
        format!(
            r#"<!DOCTYPE html><html><head></head><body><script id="__NEXT_DATA__" type="application/json">{{"props":{{"pageProps":{{"state":{{"data":{{"entity":{{"type":"track","name":"Never Gonna Give You Up","title":"Never Gonna Give You Up","artists":[{{"name":"Rick Astley"}}],"duration":213573,"audioPreview":{{"url":"https://example.invalid/mp3-preview/abc123"}},"visualIdentity":{{"image":[{{"url":"https://example.invalid/image/small","maxWidth":64,"maxHeight":64}},{{"url":"https://example.invalid/image/large","maxWidth":640,"maxHeight":640}}]}}}}}}}}}}}}}}</script></body></html>"#
        )
    }

    #[test]
    fn parse_embed_entity_extracts_track_metadata() {
        let meta = SpotifyDownloader::parse_embed_entity(&sample_embed_html())
            .expect("parse should succeed");
        assert_eq!(meta.title, "Never Gonna Give You Up");
        assert_eq!(meta.artists, vec!["Rick Astley".to_string()]);
        assert_eq!(meta.duration_seconds, Some(213.573));
        assert_eq!(
            meta.cover_url.as_deref(),
            Some("https://example.invalid/image/large")
        );
        assert_eq!(meta.cover_width, 640);
        assert_eq!(meta.cover_height, 640);
        assert_eq!(
            meta.preview_url.as_deref(),
            Some("https://example.invalid/mp3-preview/abc123")
        );
    }

    #[test]
    fn build_media_info_offers_preview_then_cover() {
        let meta = SpotifyDownloader::parse_embed_entity(&sample_embed_html())
            .expect("parse should succeed");
        let info = SpotifyDownloader::build_media_info(&meta).expect("build should succeed");
        assert_eq!(info.title, "Never Gonna Give You Up");
        assert_eq!(info.author, "Rick Astley");
        assert_eq!(info.platform, "spotify");
        assert_eq!(info.media_type, MediaType::Audio);
        assert_eq!(info.duration_seconds, Some(213.573));
        assert_eq!(
            info.thumbnail_url.as_deref(),
            Some("https://example.invalid/image/large")
        );
        assert_eq!(info.available_qualities.len(), 2);
        assert_eq!(info.available_qualities[0].label, PREVIEW_LABEL);
        assert_eq!(info.available_qualities[0].format, "mp3");
        assert_eq!(info.available_qualities[1].label, COVER_LABEL);
        assert_eq!(info.available_qualities[1].format, "jpg");
        assert_eq!(info.available_qualities[1].width, 640);
    }

    #[test]
    fn parse_embed_entity_rejects_non_track() {
        let html = r#"<!DOCTYPE html><html><body><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"type":"album","name":"Whenever You Need Somebody"}}}}}}</script></body></html>"#;
        let err = SpotifyDownloader::parse_embed_entity(html).expect_err("album must fail");
        assert!(err.to_string().contains("Only Spotify track links"));
    }

    #[test]
    fn parse_embed_entity_without_preview_offers_cover_only() {
        let html = r#"<!DOCTYPE html><html><body><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"type":"track","title":"No Preview Song","artists":[{"name":"Some Artist"}],"duration":180000,"audioPreview":null,"visualIdentity":{"image":[{"url":"https://example.invalid/image/cover","maxWidth":300,"maxHeight":300}]}}}}}}}</script></body></html>"#;
        let meta = SpotifyDownloader::parse_embed_entity(html).expect("parse should succeed");
        assert_eq!(meta.preview_url, None);
        let info = SpotifyDownloader::build_media_info(&meta).expect("build should succeed");
        assert_eq!(info.available_qualities.len(), 1);
        assert_eq!(info.available_qualities[0].label, COVER_LABEL);
    }

    #[test]
    fn platform_from_url_detects_spotify_tracks() {
        assert_eq!(
            Platform::from_url("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"),
            Some(Platform::Spotify)
        );
        assert_eq!(
            Platform::from_url("https://spotify.link/abc123"),
            Some(Platform::Spotify)
        );
        // Album links stay generic: only tracks are supported.
        assert_eq!(
            Platform::from_url("https://open.spotify.com/album/1zCNyM9y5o2iK4XxXW7QyG"),
            None
        );
    }
}
