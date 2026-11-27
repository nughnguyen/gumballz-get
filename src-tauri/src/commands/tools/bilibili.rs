//! Commands of the Bilibili category: export of the floating comments (danmaku)
/// and burning the ASS into a clip. The engine lives in
/// `crate::platforms::bilibili::danmaku`; only the bridge to Tauri comes in here.

use crate::platforms::bilibili::danmaku::{burn, export};

use super::{err, progress};

#[tauri::command]
pub async fn tool_bili_danmaku_export(
    app: tauri::AppHandle,
    opts: export::ExportOptions,
) -> Result<export::ExportResult, String> {
    export::run(&opts, &progress(&app)).await.map_err(err)
}

#[tauri::command]
pub async fn tool_bili_danmaku_burn(
    app: tauri::AppHandle,
    opts: burn::BurnOptions,
) -> Result<burn::BurnResult, String> {
    burn::run(&opts, &progress(&app)).await.map_err(err)
}
