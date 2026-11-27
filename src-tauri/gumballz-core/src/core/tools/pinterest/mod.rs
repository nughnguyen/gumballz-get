//! Pinterest (study 67): the undocumented web API (`/resource/*Resource/get/`)
//! the site itself uses, read without login for public content and with the
//! user's cookies for secret boards. No password, no Selenium.
//!
//! - `api.rs`: client, URL parsing, pagination by `bookmarks`, the normalized `Pin`
//!   (original image, video, carousel, story, stats,
//!   AI and ad signals).
//! - `media.rs`: download of the original with a fallback chain, HLS video → MP4,
//!   WebP → PNG, already-downloaded file for syncing.
//! - `analysis.rs`: dHash (near-identical duplicates), k-means palette,
//!   AI heuristic (Pinterest Power Menu lists, MIT).
//! - `export.rs`: CSV, JSON, offline HTML gallery and PDF.

pub mod analysis;
pub mod api;
pub mod export;
pub mod media;

pub use api::{parse_target, Board, Person, Pin, PinClient, Section, Target, User};
