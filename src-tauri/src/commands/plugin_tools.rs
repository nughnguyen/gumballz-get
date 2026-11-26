use std::path::{Path, PathBuf};
use tauri::Emitter;

use gumballz_get_plugin_listening_studio::SubtitleSegment;

fn read_subtitles(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "srt" | "vtt") {
        return Err("Choose an .srt or .vtt subtitle file".into());
    }
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > 5 * 1024 * 1024 {
        return Err("Subtitle files are limited to 5 MB".into());
    }
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn listening_studio_parse_subtitles(path: String) -> Result<Vec<SubtitleSegment>, String> {
    let content = read_subtitles(Path::new(&path))?;
    gumballz_get_plugin_listening_studio::parse_subtitles(&content)
}

#[tauri::command]
pub async fn listening_studio_split_audio(
    audio_path: String,
    subtitle_path: String,
    output_dir: String,
) -> Result<Vec<String>, String> {
    let audio = PathBuf::from(audio_path);
    if !audio.is_file() {
        return Err("Audio/video input does not exist".into());
    }
    let subtitles = read_subtitles(Path::new(&subtitle_path))?;
    let segments = gumballz_get_plugin_listening_studio::parse_subtitles(&subtitles)?;
    let (ffmpeg, _) = gumballz_core::core::dependencies::find_tool_with_source("ffmpeg")
        .await
        .ok_or("FFmpeg is not installed")?;
    gumballz_get_plugin_listening_studio::split_audio(
        &audio,
        Path::new(&output_dir),
        &segments,
        &ffmpeg,
    )
}

#[tauri::command]
pub fn listening_studio_export(path: String, format: String) -> Result<String, String> {
    let content = read_subtitles(Path::new(&path))?;
    let segments = gumballz_get_plugin_listening_studio::parse_subtitles(&content)?;
    let blanks = gumballz_get_plugin_listening_studio::cloze(&segments, 7);
    match format.as_str() {
        "markdown" => Ok(gumballz_get_plugin_listening_studio::export_markdown(
            &segments, &blanks,
        )),
        "csv" => Ok(gumballz_get_plugin_listening_studio::export_csv(
            &segments, &blanks,
        )),
        _ => Err("Export format must be markdown or csv".into()),
    }
}

#[tauri::command]
pub async fn asset_forge_convert(
    app: tauri::AppHandle,
    inputs: Vec<String>,
    output_dir: String,
    format: String,
) -> Result<Vec<gumballz_get_plugin_asset_forge::ForgeResult>, String> {
    let files = gumballz_get_plugin_asset_forge::collect_media(
        &inputs.iter().map(PathBuf::from).collect::<Vec<_>>(),
        256,
    )?;
    let (ffmpeg, _) = gumballz_core::core::dependencies::find_tool_with_source("ffmpeg")
        .await
        .ok_or("FFmpeg is not installed")?;
    let output = PathBuf::from(output_dir);
    let callback = move |progress| {
        let _ = app.emit("asset-forge-progress", progress);
    };
    tauri::async_runtime::spawn_blocking(move || {
        gumballz_get_plugin_asset_forge::convert_batch(&files, &output, &format, &ffmpeg, callback)
    })
    .await
    .map_err(|error| error.to_string())?
}
