//! Music category: what to do with music that is already yours.
//!
//! - `playlist`: matches an exported playlist with the disk files and
//!   writes `.m3u8`/`.pls` for the local player.
//! - `lyrics`: synced lyrics (`.lrc`) via LRCLIB.
//! - `history`: analysis of the official Spotify export, no network.
//!
//! `norm` is what the three share: normalize artist and title in one
//! single way, so "Song (Remastered 2011)" and "song" are the same thing.

pub mod history;
pub mod lyrics;
pub mod norm;
pub mod playlist;
