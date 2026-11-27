pub use crate::platforms::magnet::MagnetDownloader;
pub use gumballz_core::platforms::traits;
pub use gumballz_core::platforms::BlueskyDownloader;
pub use gumballz_core::platforms::DirectFileDownloader;
pub use gumballz_core::platforms::DouyinDownloader;
pub use gumballz_core::platforms::GdocsDownloader;
pub use gumballz_core::platforms::ScribdDownloader;
pub use gumballz_core::platforms::SpotifyDownloader;
pub use gumballz_core::platforms::StudocuDownloader;
pub use gumballz_core::platforms::GenericYtdlpDownloader;
pub use gumballz_core::platforms::InstagramDownloader;
pub use gumballz_core::platforms::P2pDownloader;
pub use gumballz_core::platforms::PinterestDownloader;
pub use gumballz_core::platforms::Platform;
pub use gumballz_core::platforms::RedditDownloader;
pub use gumballz_core::platforms::ThreadsDownloader;
pub use gumballz_core::platforms::TikTokDownloader;
pub use gumballz_core::platforms::TwitchClipsDownloader;
pub use gumballz_core::platforms::TwitterDownloader;
pub use gumballz_core::platforms::VimeoDownloader;
pub use gumballz_core::platforms::YouTubeDownloader;

pub mod magnet;
pub mod noop;
pub mod twitter;

#[cfg(not(target_os = "android"))]
pub mod bilibili;
#[cfg(not(target_os = "android"))]
pub mod gallerydl;
#[cfg(not(target_os = "android"))]
pub mod generic_ytdlp;
// Ported to gumballz-core: bluesky, direct_file, douyin, instagram, pinterest, p2p, reddit, tiktok, twitch, twitter, vimeo, youtube
