//! Audio tools: normalize loudness (EBU R128), remove noise and tag music
//! metadata (ID3 / Vorbis / MP4).

use gumballz_core::core::tools::{audio_clean, audio_tag};

use super::{err, progress};

#[tauri::command]
pub async fn tool_audio_clean(
    app: tauri::AppHandle,
    opts: audio_clean::CleanOptions,
) -> Result<audio_clean::CleanResult, String> {
    audio_clean::run(opts, progress(&app)).await.map_err(err)
}

// ── Music tags (ID3 / Vorbis / MP4), via lofty ──────────────────

/// Reads the tags of a folder or a list of files and already returns the diagnosis.
#[tauri::command]
pub async fn tool_audio_tag_scan(
    app: tauri::AppHandle,
    opts: audio_tag::ScanOptions,
) -> Result<audio_tag::ScanResult, String> {
    let p = progress(&app);
    tokio::task::spawn_blocking(move || audio_tag::scan(&opts, &p))
        .await
        .map_err(err)?
        .map_err(err)
}

/// Applies the batch edit. With `dry_run` (the default) it only returns the diff,
/// without touching any file.
#[tauri::command]
pub async fn tool_audio_tag_write(
    app: tauri::AppHandle,
    opts: audio_tag::EditOptions,
) -> Result<audio_tag::EditResult, String> {
    let p = progress(&app);
    tokio::task::spawn_blocking(move || audio_tag::edit(&opts, &p))
        .await
        .map_err(err)?
        .map_err(err)
}

/// Embedded cover of a file, as a `data:` URL, for the UI thumbnail.
#[tauri::command]
pub async fn tool_audio_tag_cover(path: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || audio_tag::cover_data_url(&path))
        .await
        .map_err(err)?
        .map_err(err)
}
