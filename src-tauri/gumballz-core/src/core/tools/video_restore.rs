//! Fix bad video with FFmpeg's native filters: shake, flicker
//! (that light pulsing in screen and TV recordings), gradient banding and
//! noise from old or low-light cameras.
//!
//! Stabilisation is `vidstab` in two passes — the first measures the
//! movement and writes a transforms file, the second applies it. A single
//! pass does not exist: without measuring the whole clip there is no way to know what
//! is shake and what is deliberate camera movement.

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct RestoreOptions {
    pub inputs: Vec<String>,
    /// Removes luminance flicker between frames.
    #[serde(default)]
    pub deflicker: bool,
    /// Smooths a gradient that became banding.
    #[serde(default)]
    pub deband: bool,
    /// "none" | "hqdn3d" | "nlmeans" | "atadenoise"
    #[serde(default = "default_denoise")]
    pub denoise: String,
    #[serde(default)]
    pub sharpen: bool,
    /// vidstab stabilization in two passes.
    #[serde(default)]
    pub stabilize: bool,
    /// 1-10; the higher, the more shake the detector accepts.
    #[serde(default = "default_shakiness")]
    pub shakiness: u32,
    /// 1-100 smoothing frames (10 ≈ half a second at 24 fps).
    #[serde(default = "default_smoothing")]
    pub smoothing: u32,
    #[serde(default = "default_crf")]
    pub crf: u32,
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default)]
    pub suffix: String,
}

fn default_denoise() -> String {
    "none".into()
}
fn default_shakiness() -> u32 {
    5
}
fn default_smoothing() -> u32 {
    10
}
fn default_crf() -> u32 {
    20
}
fn default_preset() -> String {
    "medium".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreItem {
    pub input: String,
    pub output: Option<String>,
    pub filter: String,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub stabilized: bool,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreResult {
    pub items: Vec<RestoreItem>,
}

fn denoise_filter(kind: &str) -> Option<&'static str> {
    match kind {
        // Good default for camera noise without turning into blur.
        "hqdn3d" => Some("hqdn3d=luma_spatial=4:chroma_spatial=3:luma_tmp=6:chroma_tmp=4.5"),
        // More expensive and more precise; good for film grain.
        "nlmeans" => Some("nlmeans=s=3:p=7:r=15"),
        // Purely temporal: great for video with static noise and a still camera.
        "atadenoise" => Some("atadenoise"),
        _ => None,
    }
}

/// Restore chain (without the stabilisation, which is its own pass).
/// Returns `None` when nothing was asked for.
pub fn restore_chain(opts: &RestoreOptions) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if opts.deflicker {
        parts.push("deflicker=size=5:mode=pm".into());
    }
    if let Some(d) = denoise_filter(&opts.denoise) {
        parts.push(d.into());
    }
    if opts.deband {
        parts.push("deband=1thr=0.02:2thr=0.02:3thr=0.02:4thr=0.02:range=16".into());
    }
    if opts.sharpen {
        parts.push("unsharp=5:5:0.8:3:3:0.4".into());
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

pub fn detect_filter(shakiness: u32) -> String {
    format!(
        "vidstabdetect=shakiness={}:accuracy=15",
        shakiness.clamp(1, 10)
    )
}

/// `optzoom=1` lets the filter pick the minimum zoom that avoids a black border.
pub fn transform_filter(smoothing: u32) -> String {
    format!(
        "vidstabtransform=smoothing={}:optzoom=1:interpol=bicubic,unsharp=5:5:0.8:3:3:0.4",
        smoothing.clamp(1, 100)
    )
}

async fn restore_one(
    ffmpeg: &Path,
    opts: &RestoreOptions,
    input: &str,
) -> anyhow::Result<RestoreItem> {
    let inp = Path::new(input);
    let bytes_before = std::fs::metadata(inp).map(|m| m.len()).unwrap_or(0);
    let chain = restore_chain(opts);
    if chain.is_none() && !opts.stabilize {
        return Err(anyhow!("nenhum conserto escolhido"));
    }

    let out_dir = if opts.output_dir.trim().is_empty() {
        inp.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    } else {
        PathBuf::from(opts.output_dir.trim())
    };
    std::fs::create_dir_all(&out_dir)?;
    let stem = inp
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".into());
    let suffix = if !opts.suffix.is_empty() {
        opts.suffix.clone()
    } else if opts.stabilize {
        "-estavel".into()
    } else {
        "-restaurado".into()
    };
    let output = out_dir.join(format!("{}{}.mp4", stem, suffix));

    let mut trf: Option<PathBuf> = None;
    if opts.stabilize {
        let dir = std::env::temp_dir().join(format!("gumballz-get-vidstab-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("transforms.trf");
        let detect = format!(
            "{}:result='{}'",
            detect_filter(opts.shakiness),
            file.to_string_lossy().replace('\\', "/")
        );
        let out = crate::core::process::command(ffmpeg)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(inp)
            .args(["-vf", &detect, "-an", "-f", "null", "-"])
            .output()
            .await
            .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(anyhow!(
                "shake detection: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        trf = Some(file);
    }

    let mut chain_parts: Vec<String> = Vec::new();
    if let Some(file) = trf.as_ref() {
        chain_parts.push(format!(
            "vidstabtransform=input='{}':{}",
            file.to_string_lossy().replace('\\', "/"),
            transform_filter(opts.smoothing).trim_start_matches("vidstabtransform=")
        ));
    }
    if let Some(c) = chain {
        chain_parts.push(c);
    }
    let filter = chain_parts.join(",");

    let out = crate::core::process::command(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(inp)
        .args(["-vf", &filter])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            &opts.preset,
            "-crf",
            &opts.crf.clamp(10, 40).to_string(),
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(&output)
        .output()
        .await
        .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
    if let Some(file) = trf.as_ref() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    if !out.status.success() {
        return Err(anyhow!("{}", String::from_utf8_lossy(&out.stderr).trim()));
    }

    Ok(RestoreItem {
        input: input.to_string(),
        output: Some(output.to_string_lossy().to_string()),
        filter,
        bytes_before,
        bytes_after: std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0),
        stabilized: opts.stabilize,
        ok: true,
        error: None,
    })
}

pub async fn run(
    opts: RestoreOptions,
    progress: super::ProgressFn,
) -> anyhow::Result<RestoreResult> {
    let ffmpeg = crate::core::dependencies::ensure_ffmpeg().await?;
    let total = opts.inputs.len() as u64;
    let mut items = Vec::new();
    for (i, input) in opts.inputs.iter().enumerate() {
        super::report(
            &progress,
            "video-restore",
            "progress",
            i as u64,
            Some(total),
            Some(input.clone()),
        );
        match restore_one(&ffmpeg, &opts, input).await {
            Ok(item) => items.push(item),
            Err(e) => {
                tracing::warn!("[video-restore] {}: {}", input, e);
                items.push(RestoreItem {
                    input: input.clone(),
                    output: None,
                    filter: String::new(),
                    bytes_before: std::fs::metadata(input).map(|m| m.len()).unwrap_or(0),
                    bytes_after: 0,
                    stabilized: false,
                    ok: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    super::report(&progress, "video-restore", "done", total, Some(total), None);
    Ok(RestoreResult { items })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> RestoreOptions {
        RestoreOptions {
            inputs: vec![],
            deflicker: false,
            deband: false,
            denoise: "none".into(),
            sharpen: false,
            stabilize: false,
            shakiness: 5,
            smoothing: 10,
            crf: 20,
            preset: "medium".into(),
            output_dir: String::new(),
            suffix: String::new(),
        }
    }

    #[test]
    fn nothing_selected_means_no_chain() {
        assert_eq!(restore_chain(&opts()), None);
    }

    #[test]
    fn filters_come_in_a_sane_order() {
        let o = RestoreOptions {
            deflicker: true,
            deband: true,
            denoise: "hqdn3d".into(),
            sharpen: true,
            ..opts()
        };
        let chain = restore_chain(&o).unwrap();
        let deflicker = chain.find("deflicker").unwrap();
        let denoise = chain.find("hqdn3d").unwrap();
        let deband = chain.find("deband").unwrap();
        let sharpen = chain.find("unsharp").unwrap();
        assert!(
            deflicker < denoise && denoise < deband && deband < sharpen,
            "ordem errada: {}",
            chain
        );
    }

    #[test]
    fn denoise_modes_are_distinct_and_none_is_none() {
        assert!(denoise_filter("none").is_none());
        assert!(denoise_filter("qualquer").is_none());
        assert!(denoise_filter("nlmeans").unwrap().starts_with("nlmeans"));
        assert!(denoise_filter("atadenoise").is_some());
    }

    #[test]
    fn vidstab_params_are_clamped() {
        assert!(detect_filter(0).contains("shakiness=1"));
        assert!(detect_filter(99).contains("shakiness=10"));
        assert!(transform_filter(0).contains("smoothing=1"));
        assert!(transform_filter(999).contains("smoothing=100"));
    }

    /// vidstab is an external-library filter: if the managed FFmpeg build
    /// does not ship it, the test fails here and not in the user's hands.
    /// `cargo test -p gumballz-core --lib -- --ignored live_stabilize`
    #[tokio::test]
    #[ignore]
    async fn live_stabilize_runs_both_passes() {
        let ffmpeg = crate::core::dependencies::ensure_ffmpeg().await.unwrap();
        let dir = std::env::temp_dir().join("gumballz-get-vidstab-live");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tremido.mp4");
        // testsrc2 with a sinusoidal offset in x/y: synthetic shake.
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
                "-vf",
                "crop=280:200:10+8*sin(n/3):10+8*cos(n/4)",
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
        assert!(gen.status.success(), "did not generate the shaky clip");

        let o = RestoreOptions {
            inputs: vec![src.to_string_lossy().to_string()],
            stabilize: true,
            deflicker: true,
            denoise: "hqdn3d".into(),
            crf: 24,
            preset: "ultrafast".into(),
            output_dir: dir.to_string_lossy().to_string(),
            ..opts()
        };
        let res = run(o, crate::core::tools::noop_progress()).await.unwrap();
        let item = &res.items[0];
        assert!(item.ok, "falhou: {:?}", item.error);
        assert!(item.stabilized);
        assert!(item.filter.contains("vidstabtransform"), "{}", item.filter);
        assert!(item.filter.contains("hqdn3d"), "o restauro sumiu da cadeia");
        assert!(item.bytes_after > 1000);

        let out = item.output.as_ref().unwrap();
        let probe = crate::core::ffmpeg::probe(std::path::Path::new(out))
            .await
            .unwrap();
        assert!(
            (probe.duration_seconds - 4.0).abs() < 0.5,
            "{}",
            probe.duration_seconds
        );
        // The transforms file is temporary and must not survive.
        assert!(
            !std::fs::read_dir(std::env::temp_dir())
                .unwrap()
                .flatten()
                .any(|e| e
                    .file_name()
                    .to_string_lossy()
                    .starts_with("gumballz-get-vidstab-")
                    && e.path().join("transforms.trf").exists()),
            "temporary .trf left over"
        );
        eprintln!("{} bytes, filtro: {}", item.bytes_after, item.filter);
    }

    #[test]
    fn transform_always_sharpens_after_the_zoom() {
        let f = transform_filter(10);
        assert!(f.contains("optzoom=1"), "without optzoom a black border remains");
        assert!(f.ends_with("unsharp=5:5:0.8:3:3:0.4"));
    }
}
