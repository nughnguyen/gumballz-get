//! Commands of the Twitch category. Everything here uses only public endpoints:
/// the site's GraphQL with the web player's Client-ID, plus the open BTTV, FFZ
/// and 7TV APIs. No login, no app token.

use gumballz_core::core::tools::twitch::{chat, emotes};

use super::{err, progress};

/// Bulk download of a channel's emotes and badges (and the global sets).
#[tauri::command]
pub async fn tool_tw_emotes(
    app: tauri::AppHandle,
    opts: emotes::Options,
) -> Result<emotes::Result, String> {
    emotes::run(&opts, &progress(&app)).await.map_err(err)
}

/// Chat replay of a VOD or clip exported as JSON/CSV/SRT/ASS.
#[tauri::command]
pub async fn tool_tw_chat_replay(
    app: tauri::AppHandle,
    opts: chat::Options,
) -> Result<chat::Result, String> {
    chat::export(&opts, &progress(&app)).await.map_err(err)
}
