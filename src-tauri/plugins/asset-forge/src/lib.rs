use gumballz_plugin_sdk::{GumballzPlugin, PluginHost};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeProgress {
    pub completed: usize,
    pub total: usize,
    pub file: String,
    pub success: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeResult {
    pub input: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

pub fn supported_format(format: &str) -> bool {
    matches!(format, "webp" | "ogg" | "mp3" | "flac")
}

pub fn collect_media(paths: &[PathBuf], max_files: usize) -> Result<Vec<PathBuf>, String> {
    fn walk(path: &Path, out: &mut Vec<PathBuf>, limit: usize, depth: usize) -> Result<(), String> {
        if out.len() >= limit {
            return Ok(());
        }
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "mp4"
                    | "mkv"
                    | "webm"
                    | "mov"
                    | "avi"
                    | "mp3"
                    | "m4a"
                    | "aac"
                    | "ogg"
                    | "flac"
                    | "wav"
                    | "jpg"
                    | "jpeg"
                    | "png"
                    | "gif"
                    | "bmp"
                    | "webp"
            ) {
                out.push(path.to_path_buf());
            }
        } else if path.is_dir() && depth < 8 {
            let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                walk(&entry.path(), out, limit, depth + 1)?;
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    for path in paths {
        walk(path, &mut files, max_files, 0)?;
        if files.len() >= max_files {
            break;
        }
    }
    if files.is_empty() {
        return Err("No supported media files were found".into());
    }
    Ok(files)
}

pub fn convert_batch<F>(
    inputs: &[PathBuf],
    output_dir: &Path,
    format: &str,
    ffmpeg: &Path,
    progress: F,
) -> Result<Vec<ForgeResult>, String>
where
    F: Fn(ForgeProgress) + Send + Sync,
{
    if !supported_format(format) {
        return Err("Output format must be webp, ogg, mp3, or flac".into());
    }
    if inputs.len() > 256 {
        return Err("A batch is limited to 256 files".into());
    }
    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    let complete = AtomicUsize::new(0);
    let results = inputs
        .par_iter()
        .enumerate()
        .map(|(index, input)| {
            let stem = sanitize_filename::sanitize(
                input
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .unwrap_or("media"),
            );
            let output = output_dir.join(format!("{stem}-{:03}.{format}", index + 1));
            let mut command = Command::new(ffmpeg);
            command
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(input);
            match format {
                "webp" => {
                    command.args(["-an", "-c:v", "libwebp", "-q:v", "80"]);
                }
                "ogg" => {
                    command.args(["-vn", "-c:a", "libvorbis", "-q:a", "5"]);
                }
                "mp3" => {
                    command.args(["-vn", "-c:a", "libmp3lame", "-q:a", "2"]);
                }
                "flac" => {
                    command.args(["-vn", "-c:a", "flac"]);
                }
                _ => unreachable!(),
            }
            let result = match command.arg(&output).output() {
                Ok(output) if output.status.success() => ForgeResult {
                    input: input.to_string_lossy().into_owned(),
                    output: Some(output_path(&output, &output_dir, &stem, index, format)),
                    error: None,
                },
                Ok(_) => ForgeResult {
                    input: input.to_string_lossy().into_owned(),
                    output: None,
                    error: Some("FFmpeg conversion failed".into()),
                },
                Err(_) => ForgeResult {
                    input: input.to_string_lossy().into_owned(),
                    output: None,
                    error: Some("Could not start FFmpeg".into()),
                },
            };
            let done = complete.fetch_add(1, Ordering::Relaxed) + 1;
            progress(ForgeProgress {
                completed: done,
                total: inputs.len(),
                file: input
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("media")
                    .to_owned(),
                success: result.error.is_none(),
            });
            result
        })
        .collect();
    Ok(results)
}

fn output_path(
    _ffmpeg: &std::process::Output,
    directory: &Path,
    stem: &str,
    index: usize,
    format: &str,
) -> String {
    directory
        .join(format!("{stem}-{:03}.{format}", index + 1))
        .to_string_lossy()
        .into_owned()
}

pub fn manifest() -> gumballz_plugin_sdk::PluginManifest {
    serde_json::from_str(include_str!("../plugin.manifest.json"))
        .expect("valid Asset Forge manifest")
}

pub struct AssetForge(Option<Arc<dyn PluginHost>>);
impl AssetForge {
    pub fn new() -> Self {
        Self(None)
    }
}
impl GumballzPlugin for AssetForge {
    fn id(&self) -> &str {
        "asset-forge"
    }
    fn name(&self) -> &str {
        "Batch Media & Asset Forge"
    }
    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
    fn initialize(&mut self, host: Arc<dyn PluginHost>) -> anyhow::Result<()> {
        self.0 = Some(host);
        Ok(())
    }
    fn handle_command(
        &self,
        command: String,
        _args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + 'static>> {
        Box::pin(async move { Err(format!("Unknown Asset Forge command: {command}")) })
    }
    fn commands(&self) -> Vec<String> {
        Vec::new()
    }
}
// gumballz_plugin_sdk::export_plugin!(AssetForge::new());
