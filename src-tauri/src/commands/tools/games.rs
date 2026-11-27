//! Games group: import the Switch album, organize PC clips and query ProtonDB.
//! The logic lives in `gumballz_core::core::tools::games`.

use gumballz_core::core::tools::games::{clip_organizer, protondb, switch_album};

use super::{err, progress};

#[tauri::command]
pub async fn tool_switch_album(
    app: tauri::AppHandle,
    opts: switch_album::SwitchOptions,
) -> Result<switch_album::SwitchResult, String> {
    switch_album::run(opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_clip_organizer(
    app: tauri::AppHandle,
    opts: clip_organizer::ClipOptions,
) -> Result<clip_organizer::ClipResult, String> {
    clip_organizer::run(opts, progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_protondb(
    app: tauri::AppHandle,
    opts: protondb::ProtonOptions,
) -> Result<protondb::ProtonResult, String> {
    protondb::run(opts, progress(&app)).await.map_err(err)
}
