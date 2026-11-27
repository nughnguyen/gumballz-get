//! Twitch: tools that only use what the site exposes without login.
//!
//! - `gql`: the public GraphQL (web player Client-ID), parsing of channel,
//!   VOD and clip links.
//! - `emotes`: bulk-download of native emotes and badges + BTTV, FFZ and 7TV.
//! - `chat`: VOD/clip chat replay exported as JSON, CSV and subtitles.

pub mod chat;
pub mod emotes;
pub mod gql;

pub use gql::{parse_channel, parse_video, ChatTarget, Gql};
