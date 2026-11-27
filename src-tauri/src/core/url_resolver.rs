//! Choosing the downloader for a URL, with direct-file probing (B4).
//!
//! `Platform::from_url` and `registry.find_platform` decide by URL alone. A
//! `https://host/100MB.bin` with an extension nobody knows fell into generic
//! and sat for minutes in `connecting` waiting for the yt-dlp extractor. Here,
//! if the URL is not from a known platform and has an unknown extension, a short
//! `HEAD` decides: it is not a page and it has a file size ⇒ `direct_file`,
//! which uses the segmented `HttpFetcher`.

use std::sync::Arc;

use gumballz_core::core::registry::PlatformRegistry;
use gumballz_core::platforms::{direct_file, Platform, PlatformDownloader};

pub struct Resolved {
    pub downloader: Arc<dyn PlatformDownloader>,
    pub platform_name: String,
}

pub async fn resolve_downloader(registry: &PlatformRegistry, url: &str) -> Option<Resolved> {
    let platform = Platform::from_url(url);
    if platform.is_none() && direct_file::looks_like_direct_file(url).await {
        if let Some(downloader) = registry.find_by_name("direct_file") {
            return Some(Resolved {
                downloader,
                platform_name: "direct_file".to_string(),
            });
        }
    }
    let downloader = registry.find_platform(url)?;
    let platform_name = platform
        .map(|p| p.to_string())
        .unwrap_or_else(|| "generic".to_string());
    Some(Resolved {
        downloader,
        platform_name,
    })
}
