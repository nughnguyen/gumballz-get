//! Video → GIF or animated WebP with a decent palette.
//!
//! A direct FFmpeg GIF comes out dirty because the default palette is a fixed 216 colours.
//! The right way is two passes: `palettegen` looks at the whole clip and
//! picks the colours, `paletteuse` applies them with dithering. Animated WebP does not
//! need this — it has true colour — and usually comes out 3-5× smaller.

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct GifOptions {
    pub inputs: Vec<String>,
    /// Seconds from the start; 0 takes from the beginning.
    #[serde(default)]
    pub start: f64,
    /// Duration in seconds; 0 takes to the end.
    #[serde(default)]
    pub duration: f64,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Width in pixels; 0 keeps the original.
    #[serde(default)]
    pub width: u32,
    /// "gif" | "webp"
    #[serde(default = "default_format")]
    pub format: String,
    /// "sierra2_4a" | "bayer" | "none"
    #[serde(default = "default_dither")]
    pub dither: String,
    #[serde(default = "default_colors")]
    pub max_colors: u32,
    /// WebP quality (0-100); ignored for GIF.
    #[serde(default = "default_quality")]
    pub quality: u32,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default)]
    pub suffix: String,
}

fn default_fps() -> u32 {
    12
}
fn default_format() -> String {
    "gif".into()
}
fn default_dither() -> String {
    "sierra2_4a".into()
}
fn default_colors() -> u32 {
    256
}
fn default_quality() -> u32 {
    75
}

#[derive(Debug, Clone, Serialize)]
pub struct GifItem {
    pub input: String,
    pub output: Option<String>,
    pub bytes: u64,
    pub frames: u64,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GifResult {
    pub items: Vec<GifItem>,
}

/// `fps` and `scale` — the same chain must hold in both passes, otherwise the
/// palette is computed over frames that are not the ones going into the file.
pub fn vf_chain(fps: u32, width: u32) -> String {
    let mut v = format!("fps={}", fps.clamp(1, 60));
    if width > 0 {
        v.push_str(&format!(",scale={}:-2:flags=lanczos", width - width % 2));
    }
    v
}

pub fn output_ext(format: &str) -> &'static str {
    if format == "webp" {
        "webp"
    } else {
        "gif"
    }
}

fn dither_arg(dither: &str) -> String {
    match dither {
        "none" => "dither=none".into(),
        "bayer" => "dither=bayer:bayer_scale=3".into(),
        other => format!("dither={}", other),
    }
}

fn cut_args(start: f64, duration: f64) -> Vec<String> {
    let mut a = Vec::new();
    if start > 0.0 {
        a.push("-ss".into());
        a.push(format!("{:.3}", start));
    }
    if duration > 0.0 {
        a.push("-t".into());
        a.push(format!("{:.3}", duration));
    }
    a
}

async fn convert_one(ffmpeg: &Path, opts: &GifOptions, input: &str) -> anyhow::Result<GifItem> {
    let inp = Path::new(input);
    let out_dir = if opts.output_dir.trim().is_empty() {
        inp.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    } else {
        PathBuf::from(opts.output_dir.trim())
    };
    std::fs::create_dir_all(&out_dir)?;
    let stem = inp
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "clipe".into());
    let output = out_dir.join(format!(
        "{}{}.{}",
        stem,
        opts.suffix,
        output_ext(&opts.format)
    ));
    let chain = vf_chain(opts.fps, opts.width);
    let cut = cut_args(opts.start, opts.duration);

    if output_ext(&opts.format) == "webp" {
        let out = crate::core::process::command(ffmpeg)
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(&cut)
            .arg("-i")
            .arg(inp)
            .args(["-vf", &chain, "-c:v", "libwebp_anim", "-lossless", "0"])
            .args(["-q:v", &opts.quality.clamp(1, 100).to_string()])
            .args(["-loop", "0", "-an"])
            .arg(&output)
            .output()
            .await
            .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
        if !out.status.success() {
            return Err(anyhow!("{}", String::from_utf8_lossy(&out.stderr).trim()));
        }
    } else {
        let tmp =
            std::env::temp_dir().join(format!("gumballz-get-palette-{}.png", uuid::Uuid::new_v4()));
        let pass1 = crate::core::process::command(ffmpeg)
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(&cut)
            .arg("-i")
            .arg(inp)
            .args([
                "-vf",
                &format!(
                    "{},palettegen=max_colors={}:stats_mode=diff",
                    chain,
                    opts.max_colors.clamp(4, 256)
                ),
            ])
            .arg(&tmp)
            .output()
            .await
            .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
        if !pass1.status.success() {
            let _ = std::fs::remove_file(&tmp);
            return Err(anyhow!(
                "paleta: {}",
                String::from_utf8_lossy(&pass1.stderr).trim()
            ));
        }
        let pass2 = crate::core::process::command(ffmpeg)
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(&cut)
            .arg("-i")
            .arg(inp)
            .arg("-i")
            .arg(&tmp)
            .args([
                "-lavfi",
                &format!(
                    "{}[x];[x][1:v]paletteuse={}",
                    chain,
                    dither_arg(&opts.dither)
                ),
            ])
            .args(["-loop", "0", "-an"])
            .arg(&output)
            .output()
            .await
            .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
        let _ = std::fs::remove_file(&tmp);
        if !pass2.status.success() {
            return Err(anyhow!("{}", String::from_utf8_lossy(&pass2.stderr).trim()));
        }
    }

    let bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    let secs = if opts.duration > 0.0 {
        opts.duration
    } else {
        crate::core::ffmpeg::probe(inp)
            .await
            .map(|p| (p.duration_seconds - opts.start).max(0.0))
            .unwrap_or(0.0)
    };
    Ok(GifItem {
        input: input.to_string(),
        output: Some(output.to_string_lossy().to_string()),
        bytes,
        frames: (secs * opts.fps as f64).round() as u64,
        ok: true,
        error: None,
    })
}

pub async fn run(opts: GifOptions, progress: super::ProgressFn) -> anyhow::Result<GifResult> {
    let ffmpeg = crate::core::dependencies::ensure_ffmpeg().await?;
    let total = opts.inputs.len() as u64;
    let mut items = Vec::new();
    for (i, input) in opts.inputs.iter().enumerate() {
        super::report(
            &progress,
            "video-gif",
            "progress",
            i as u64,
            Some(total),
            Some(input.clone()),
        );
        match convert_one(&ffmpeg, &opts, input).await {
            Ok(item) => items.push(item),
            Err(e) => {
                tracing::warn!("[video-gif] {}: {}", input, e);
                items.push(GifItem {
                    input: input.clone(),
                    output: None,
                    bytes: 0,
                    frames: 0,
                    ok: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    super::report(&progress, "video-gif", "done", total, Some(total), None);
    Ok(GifResult { items })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_the_same_in_both_passes() {
        assert_eq!(vf_chain(12, 0), "fps=12");
        let c = vf_chain(15, 481);
        assert!(c.starts_with("fps=15"), "{}", c);
        assert!(c.contains("scale=480:-2"), "largura tem que ser par: {}", c);
    }

    #[test]
    fn fps_is_clamped_to_something_sane() {
        assert_eq!(vf_chain(0, 0), "fps=1");
        assert_eq!(vf_chain(999, 0), "fps=60");
    }

    #[test]
    fn extension_follows_the_format() {
        assert_eq!(output_ext("webp"), "webp");
        assert_eq!(output_ext("gif"), "gif");
        assert_eq!(output_ext("qualquer"), "gif");
    }

    #[test]
    fn cut_args_are_omitted_when_zero() {
        assert!(cut_args(0.0, 0.0).is_empty());
        assert_eq!(cut_args(1.5, 0.0), vec!["-ss", "1.500"]);
        assert_eq!(cut_args(0.0, 3.0), vec!["-t", "3.000"]);
    }

    /// End to end with the managed FFmpeg, in both formats.
    /// `cargo test -p gumballz-core --lib -- --ignored live_gif`
    #[tokio::test]
    #[ignore]
    async fn live_gif_and_webp_come_out_readable() {
        let ffmpeg = crate::core::dependencies::ensure_ffmpeg().await.unwrap();
        let dir = std::env::temp_dir().join("gumballz-get-gif-live");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("fonte.mp4");
        let gen = crate::core::process::command(&ffmpeg)
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=25:duration=4",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&src)
            .output()
            .await
            .unwrap();
        assert!(gen.status.success());

        for format in ["gif", "webp"] {
            let opts = GifOptions {
                inputs: vec![src.to_string_lossy().to_string()],
                start: 1.0,
                duration: 2.0,
                fps: 12,
                width: 240,
                format: format.into(),
                dither: "sierra2_4a".into(),
                max_colors: 128,
                quality: 75,
                output_dir: dir.to_string_lossy().to_string(),
                suffix: format!("-{}", format),
            };
            let res = run(opts, crate::core::tools::noop_progress())
                .await
                .unwrap();
            let item = &res.items[0];
            assert!(item.ok, "{} failed: {:?}", format, item.error);
            assert!(
                item.bytes > 1000,
                "{} came out too small: {}",
                format,
                item.bytes
            );
            assert_eq!(item.frames, 24);
            let out = item.output.as_ref().unwrap();
            let probe = crate::core::ffmpeg::probe(std::path::Path::new(out))
                .await
                .unwrap();
            assert!(
                probe.streams.iter().any(|s| s.codec_type == "video"),
                "{} did not open as a video",
                format
            );
            eprintln!("{}: {} bytes", format, item.bytes);
        }
        // The temporary palette must not be left behind.
        assert!(
            !std::fs::read_dir(std::env::temp_dir())
                .unwrap()
                .flatten()
                .any(|e| e
                    .file_name()
                    .to_string_lossy()
                    .starts_with("gumballz-get-palette-")),
            "temporary palette left over"
        );
    }

    #[test]
    fn dither_none_is_explicit() {
        assert_eq!(dither_arg("none"), "dither=none");
        assert!(dither_arg("bayer").contains("bayer_scale"));
        assert_eq!(dither_arg("sierra2_4a"), "dither=sierra2_4a");
    }
}
