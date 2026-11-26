use gumballz_plugin_sdk::{GumballzPlugin, PluginHost};
use serde::{Deserialize, Serialize};
use std::{future::Future, path::Path, pin::Pin, process::Command, sync::Arc};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubtitleSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub fn parse_subtitles(source: &str) -> Result<Vec<SubtitleSegment>, String> {
    let mut segments = Vec::new();
    for block in source.replace("\r\n", "\n").split("\n\n") {
        let mut lines = block.lines();
        let Some(timing) = lines.find(|line| line.contains("-->")) else {
            continue;
        };
        let (start, end) = timing.split_once("-->").ok_or("Invalid subtitle timing")?;
        let end = end.split_whitespace().next().unwrap_or(end);
        let text = lines
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }
        let start = parse_timestamp(start.trim())?;
        let end = parse_timestamp(end.trim())?;
        if end <= start {
            return Err("Subtitle end must be after its start".into());
        }
        segments.push(SubtitleSegment {
            start,
            end,
            text: strip_markup(&text),
        });
        if segments.len() > 10_000 {
            return Err("Subtitle file exceeds 10,000 cues".into());
        }
    }
    Ok(segments)
}

fn parse_timestamp(value: &str) -> Result<f64, String> {
    let value = value.replace(',', ".");
    let parts: Vec<_> = value.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (
            0.0,
            minutes.parse::<f64>().map_err(|_| "Invalid timestamp")?,
            seconds.parse::<f64>().map_err(|_| "Invalid timestamp")?,
        ),
        [hours, minutes, seconds] => (
            hours.parse::<f64>().map_err(|_| "Invalid timestamp")?,
            minutes.parse::<f64>().map_err(|_| "Invalid timestamp")?,
            seconds.parse::<f64>().map_err(|_| "Invalid timestamp")?,
        ),
        _ => return Err("Invalid timestamp".into()),
    };
    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn strip_markup(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut inside_tag = false;
    for character in text.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    output
}

pub fn split_audio(
    audio: &Path,
    output_dir: &Path,
    segments: &[SubtitleSegment],
    ffmpeg: &Path,
) -> Result<Vec<String>, String> {
    if !audio.is_file() {
        return Err("Audio or video input does not exist".into());
    }
    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    let stem =
        sanitize_filename::sanitize(audio.file_stem().and_then(|s| s.to_str()).unwrap_or("clip"));
    let extension = audio
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("m4a");
    let mut created = Vec::new();
    for (index, cue) in segments.iter().enumerate() {
        let name = format!("{stem}-{:04}.{}", index + 1, extension);
        let output = output_dir.join(name);
        let status = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-ss"])
            .arg(format!("{:.3}", cue.start))
            .arg("-to")
            .arg(format!("{:.3}", cue.end))
            .arg("-i")
            .arg(audio)
            .args(["-c", "copy"])
            .arg(&output)
            .status()
            .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
        if status.success() {
            created.push(output.to_string_lossy().into_owned());
        } else {
            return Err(format!("FFmpeg failed on subtitle cue {}", index + 1));
        }
    }
    Ok(created)
}

pub fn cloze(segments: &[SubtitleSegment], every_nth_word: usize) -> Vec<String> {
    let stride = every_nth_word.max(1);
    segments
        .iter()
        .map(|cue| {
            let mut word_index = 0;
            cue.text
                .split_whitespace()
                .map(|word| {
                    word_index += 1;
                    if word_index % stride == 0 {
                        "_____"
                    } else {
                        word
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

pub fn export_markdown(segments: &[SubtitleSegment], blanks: &[String]) -> String {
    segments
        .iter()
        .zip(blanks)
        .enumerate()
        .map(|(i, (cue, blank))| {
            format!(
                "{}. **{}–{}**\n\n   {}\n\n",
                i + 1,
                cue.start,
                cue.end,
                blank
            )
        })
        .collect()
}

pub fn export_csv(segments: &[SubtitleSegment], blanks: &[String]) -> String {
    let mut csv = String::from("start,end,text,cloze\n");
    for (cue, blank) in segments.iter().zip(blanks) {
        let escape = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
        csv.push_str(&format!(
            "{:.3},{:.3},{},{}\n",
            cue.start,
            cue.end,
            escape(&cue.text),
            escape(blank)
        ));
    }
    csv
}

pub fn manifest() -> gumballz_plugin_sdk::PluginManifest {
    serde_json::from_str(include_str!("../plugin.manifest.json"))
        .expect("valid Listening Studio manifest")
}

pub struct ListeningStudio(Option<Arc<dyn PluginHost>>);
impl ListeningStudio {
    pub fn new() -> Self {
        Self(None)
    }
}
impl GumballzPlugin for ListeningStudio {
    fn id(&self) -> &str {
        "listening-studio"
    }
    fn name(&self) -> &str {
        "Listening & Vocab Studio"
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
        args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + 'static>> {
        Box::pin(async move {
            match command.as_str() {
                "parse_subtitles" => {
                    let text = args
                        .get("text")
                        .and_then(|v| v.as_str())
                        .ok_or("text is required")?;
                    serde_json::to_value(parse_subtitles(text)?).map_err(|e| e.to_string())
                }
                _ => Err(format!("Unknown Listening Studio command: {command}")),
            }
        })
    }
    fn commands(&self) -> Vec<String> {
        vec!["parse_subtitles".into()]
    }
}
// gumballz_plugin_sdk::export_plugin!(ListeningStudio::new());
