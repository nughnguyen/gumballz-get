//! Burns the danmaku ASS into a local video file, with an optional cut.
//!
//! Cutting and burning at the same time is where FFmpeg falls out of sync: with
//! `-ss` before `-i` the frames reach the filter already rewound to zero, and
//! libass draws the comments from the start of the video on top of the clip.
//! Instead of fighting with `-copyts` (which fixes the drawing and scrambles the
//! output), the cut is done in the subtitle: a temporary copy of the ASS already
//! comes out shifted and trimmed to the requested window, and then `-ss`/`-t`
//! become an ordinary cut.
//!
//! The temporary copy also takes the original path out of the way: short ASCII
//! name in our own folder, with no apostrophe or accent for FFmpeg's filter
//! parser to trip over.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use gumballz_core::core::tools::{report, subtitle, video_compress, ProgressFn};
use serde::{Deserialize, Serialize};

/// Tool id in the `tool-progress` event.
pub const ID: &str = "bili-danmaku-burn";

#[derive(Debug, Clone, Deserialize)]
pub struct BurnOptions {
    pub video: String,
    /// The danmaku `.ass` (accepts `.ssa`, `.srt` and `.vtt` too).
    pub subtitle: String,
    /// Cut start in seconds; 0 takes from the beginning.
    #[serde(default)]
    pub start: f64,
    /// Cut duration in seconds; 0 goes to the end.
    #[serde(default)]
    pub duration: f64,
    /// Size target in MB; 0 encodes at fixed quality (CRF).
    #[serde(default)]
    pub target_mb: f64,
    #[serde(default = "default_crf")]
    pub crf: u32,
    #[serde(default = "default_preset")]
    pub preset: String,
    /// Maximum output height; 0 keeps the original.
    #[serde(default)]
    pub max_height: u32,
    /// Folder with the ASS fonts (danmaku asks for a CJK font that is rarely
    /// installed). Empty uses the system fonts.
    #[serde(default)]
    pub fonts_dir: String,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default)]
    pub suffix: String,
}

fn default_crf() -> u32 {
    20
}

fn default_preset() -> String {
    "veryfast".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct BurnResult {
    pub output: String,
    pub bytes: u64,
    pub duration_secs: f64,
    /// How many comments are left inside the window.
    pub dialogues: usize,
    /// 0 when encoding was by CRF.
    pub video_kbps: u32,
    pub audio_kbps: u32,
}

pub async fn run(opts: &BurnOptions, progress: &ProgressFn) -> Result<BurnResult> {
    let video = PathBuf::from(opts.video.trim());
    if !video.is_file() {
        return Err(anyhow!("video not found: {}", opts.video));
    }
    let sub = PathBuf::from(opts.subtitle.trim());
    if !sub.is_file() {
        return Err(anyhow!("subtitle not found: {}", opts.subtitle));
    }
    let ffmpeg = gumballz_core::core::dependencies::ensure_ffmpeg().await?;

    report(
        progress,
        ID,
        "progress",
        0,
        Some(3),
        Some("reading the video".into()),
    );
    let probe = gumballz_core::core::ffmpeg::probe(&video).await?;
    let has_audio = probe.streams.iter().any(|s| s.codec_type == "audio");
    let start = opts.start.max(0.0);
    let window = if opts.duration > 0.0 {
        opts.duration
    } else {
        (probe.duration_seconds - start).max(0.0)
    };
    if window <= 0.0 {
        return Err(anyhow!(
            "the cut window ended up empty — the video is {:.1}s long",
            probe.duration_seconds
        ));
    }

    let text = read_text(&sub)?;
    let (body, dialogues) = if is_ass(&sub) {
        shift_ass(&text, start, window)
    } else {
        shift_other(&text, start, window)?
    };
    if dialogues == 0 {
        return Err(anyhow!(
            "no comment falls between {:.1}s and {:.1}s",
            start,
            start + window
        ));
    }
    let work_dir = gumballz_core::core::tools::temp_dir();
    std::fs::create_dir_all(&work_dir)?;
    let work = work_dir.join(format!("danmaku-{}.ass", uuid::Uuid::new_v4()));
    std::fs::write(&work, &body)?;

    let out_dir = if opts.output_dir.trim().is_empty() {
        video.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    } else {
        PathBuf::from(opts.output_dir.trim())
    };
    std::fs::create_dir_all(&out_dir)?;
    let stem = video
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "clipe".into());
    let suffix = if opts.suffix.trim().is_empty() {
        "-danmaku"
    } else {
        opts.suffix.trim()
    };
    let output = out_dir.join(format!("{}{}.mp4", stem, suffix));

    let filters = filter_chain(&work, opts.fonts_dir.trim(), opts.max_height);
    let cut = cut_args(start, window);
    let audio = audio_args(has_audio, 0);

    let result = if opts.target_mb > 0.0 {
        let target_bytes = (opts.target_mb * 1024.0 * 1024.0) as u64;
        let (video_kbps, audio_kbps) =
            video_compress::plan_bitrates(window, target_bytes, 128, has_audio, 0.97).ok_or_else(
                || {
                    anyhow!(
                        "{:.0} MB does not fit {:.0}s of video — raise the target or shorten the clip",
                        opts.target_mb,
                        window
                    )
                },
            )?;
        let log_dir = work_dir.join(format!("danmaku-2pass-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&log_dir)?;
        let log = log_dir.join("pass");
        let common = bitrate_args(video_kbps, &opts.preset);

        report(
            progress,
            ID,
            "progress",
            1,
            Some(3),
            Some("passagem 1 de 2".into()),
        );
        run_ffmpeg(
            &ffmpeg,
            &video,
            &cut,
            &filters,
            &common,
            Some((1, &log)),
            &["-an".into(), "-f".into(), "mp4".into(), null_sink().into()],
        )
        .await?;

        report(
            progress,
            ID,
            "progress",
            2,
            Some(3),
            Some("passagem 2 de 2".into()),
        );
        let mut tail = audio_args(has_audio, audio_kbps);
        tail.extend(["-movflags".into(), "+faststart".into()]);
        tail.push(output.to_string_lossy().to_string());
        run_ffmpeg(
            &ffmpeg,
            &video,
            &cut,
            &filters,
            &common,
            Some((2, &log)),
            &tail,
        )
        .await?;
        let _ = std::fs::remove_dir_all(&log_dir);
        (video_kbps, if has_audio { audio_kbps } else { 0 })
    } else {
        report(
            progress,
            ID,
            "progress",
            1,
            Some(3),
            Some("queimando".into()),
        );
        let common = crf_args(opts.crf, &opts.preset);
        let mut tail = audio;
        tail.extend(["-movflags".into(), "+faststart".into()]);
        tail.push(output.to_string_lossy().to_string());
        run_ffmpeg(&ffmpeg, &video, &cut, &filters, &common, None, &tail).await?;
        (0, 0)
    };

    let _ = std::fs::remove_file(&work);
    let bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    if bytes == 0 {
        return Err(anyhow!("FFmpeg finished without writing anything"));
    }
    report(progress, ID, "done", 3, Some(3), None);
    Ok(BurnResult {
        output: output.to_string_lossy().to_string(),
        bytes,
        duration_secs: window,
        dialogues,
        video_kbps: result.0,
        audio_kbps: result.1,
    })
}

// ── Command assembly ────────────────────────────────────────────────

fn null_sink() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

/// `-ss`/`-t` before `-i`: FFmpeg seeks in the container instead of decoding
/// the whole video, and the ASS already comes shifted, so the math works out.
pub fn cut_args(start: f64, window: f64) -> Vec<String> {
    let mut a = Vec::new();
    if start > 0.0 {
        a.push("-ss".into());
        a.push(format!("{:.3}", start));
    }
    if window > 0.0 {
        a.push("-t".into());
        a.push(format!("{:.3}", window));
    }
    a
}

/// Scale before `ass`: libass draws at the output resolution and the text comes
/// out sharp, instead of being drawn large and then shrunk together with the video.
pub fn filter_chain(subtitle: &Path, fonts_dir: &str, max_height: u32) -> String {
    let mut chain = String::new();
    if max_height > 0 {
        chain.push_str(&format!(
            "scale=-2:'min({h},ih)':flags=lanczos,",
            h = max_height - max_height % 2
        ));
    }
    chain.push_str(&format!(
        "ass='{}'",
        subtitle::escape_filter_path(&subtitle.to_string_lossy())
    ));
    if !fonts_dir.is_empty() {
        chain.push_str(&format!(
            ":fontsdir='{}'",
            subtitle::escape_filter_path(fonts_dir)
        ));
    }
    chain
}

fn crf_args(crf: u32, preset: &str) -> Vec<String> {
    vec![
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        sane_preset(preset),
        "-crf".into(),
        crf.clamp(0, 51).to_string(),
        "-pix_fmt".into(),
        "yuv420p".into(),
    ]
}

fn bitrate_args(video_kbps: u32, preset: &str) -> Vec<String> {
    vec![
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        sane_preset(preset),
        "-b:v".into(),
        format!("{}k", video_kbps),
        "-pix_fmt".into(),
        "yuv420p".into(),
    ]
}

fn sane_preset(preset: &str) -> String {
    const KNOWN: [&str; 9] = [
        "ultrafast",
        "superfast",
        "veryfast",
        "faster",
        "fast",
        "medium",
        "slow",
        "slower",
        "veryslow",
    ];
    let p = preset.trim();
    if KNOWN.contains(&p) {
        p.to_string()
    } else {
        "veryfast".to_string()
    }
}

/// Re-encodes the audio instead of copying: a clip copy starts on a packet that
/// is not the first frame and the clip comes out with the sound ahead.
fn audio_args(has_audio: bool, kbps: u32) -> Vec<String> {
    if !has_audio {
        return vec!["-an".into()];
    }
    vec![
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", if kbps > 0 { kbps } else { 128 }),
    ]
}

async fn run_ffmpeg(
    ffmpeg: &Path,
    input: &Path,
    cut: &[String],
    filters: &str,
    common: &[String],
    pass: Option<(u8, &Path)>,
    tail: &[String],
) -> Result<()> {
    let mut cmd = gumballz_core::core::process::command(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-loglevel", "error"])
        .args(cut)
        .arg("-i")
        .arg(input)
        .args(["-vf", filters])
        .args(common);
    if let Some((n, log)) = pass {
        cmd.args(["-pass", &n.to_string()])
            .arg("-passlogfile")
            .arg(log);
    }
    let out = cmd
        .args(tail)
        .output()
        .await
        .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("{}", friendly_ffmpeg(stderr.trim())));
    }
    Ok(())
}

/// libass's raw error says nothing to anyone who does not know FFmpeg.
fn friendly_ffmpeg(stderr: &str) -> String {
    if stderr.contains("fontselect") || stderr.contains("Glyph") {
        return format!(
            "the ASS font is not installed — point at the fonts folder. ({})",
            stderr
        );
    }
    if stderr.is_empty() {
        return "ffmpeg failed without saying why".to_string();
    }
    stderr.to_string()
}

// ── Subtitle cut ─────────────────────────────────────────────────

fn is_ass(path: &Path) -> bool {
    path.extension()
        .map(|e| {
            let e = e.to_string_lossy().to_lowercase();
            e == "ass" || e == "ssa"
        })
        .unwrap_or(false)
}

/// Old subtitles in Portuguese come in Latin-1 more often than one would admit.
fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|b| *b as char).collect(),
    })
}

pub fn parse_ass_time(s: &str) -> Option<f64> {
    let mut parts = s.trim().split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let sec: f64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Shifts and cuts an ASS by touching only the time columns. The danmaku
/// position tags (`\move`, `\pos`) and the commas inside them have to come out
/// intact, so the text is the rest of the line and is not touched.
///
/// Returns the new file and how many dialogues are left.
pub fn shift_ass(text: &str, start_secs: f64, window_secs: f64) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut kept = 0usize;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("Dialogue:") && !trimmed.starts_with("Comment:") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if let Some(shifted) = shift_event(trimmed, start_secs, window_secs) {
            out.push_str(&shifted);
            out.push('\n');
            kept += 1;
        }
    }
    (out, kept)
}

fn shift_event(line: &str, start_secs: f64, window_secs: f64) -> Option<String> {
    let (head, rest) = line.split_once(':')?;
    // Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text.
    let fields: Vec<&str> = rest.splitn(10, ',').collect();
    if fields.len() < 10 {
        return None;
    }
    let s = parse_ass_time(fields[1])? - start_secs;
    let e = parse_ass_time(fields[2])? - start_secs;
    if e <= 0.0 || s >= window_secs {
        return None;
    }
    Some(format!(
        "{}:{},{},{},{}",
        head,
        fields[0],
        super::ass::ass_time(s.max(0.0)),
        super::ass::ass_time(e.min(window_secs)),
        fields[3..].join(",")
    ))
}

/// SRT/VTT: goes through the Tools section's parser, cuts the window and comes back as ASS.
fn shift_other(text: &str, start_secs: f64, window_secs: f64) -> Result<(String, usize)> {
    let mut cues = subtitle::parse(text)?;
    let offset = (start_secs * 1000.0).round() as i64;
    let limit = (window_secs * 1000.0).round() as i64;
    for c in cues.iter_mut() {
        c.start_ms -= offset;
        c.end_ms -= offset;
    }
    cues.retain(|c| c.end_ms > 0 && c.start_ms < limit);
    for c in cues.iter_mut() {
        c.start_ms = c.start_ms.max(0);
        c.end_ms = c.end_ms.min(limit);
    }
    let n = cues.len();
    Ok((subtitle::to_ass(&cues), n))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASS: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:13.00,Danmaku,,0,0,0,,{\\move(1920,50,-200,50)}comeco\nDialogue: 0,0:00:10.00,0:00:22.00,Danmaku,,0,0,0,,{\\move(1920,100,-200,100)}meio, com virgula\nDialogue: 0,0:01:00.00,0:01:12.00,Danmaku,,0,0,0,,{\\an8\\pos(960,50)}fim\n";

    #[test]
    fn time_parsing_round_trips() {
        assert_eq!(parse_ass_time("0:00:00.00"), Some(0.0));
        assert_eq!(parse_ass_time("0:01:05.50"), Some(65.5));
        assert_eq!(parse_ass_time("1:01:01.25"), Some(3661.25));
        assert_eq!(parse_ass_time("00:01:05,500"), None);
        assert_eq!(parse_ass_time("bagunca"), None);
    }

    #[test]
    fn shift_keeps_the_header_and_the_position_tags() {
        let (out, kept) = shift_ass(ASS, 8.0, 20.0);
        assert_eq!(kept, 2, "{}", out);
        assert!(out.contains("[Script Info]"));
        assert!(out.contains("Format: Layer, Start, End"));
        // The movement tag and the text's comma must not be touched.
        assert!(out.contains("{\\move(1920,100,-200,100)}meio, com virgula"));
        // What started at 10s now starts at 2s.
        assert!(
            out.contains("Dialogue: 0,0:00:02.00,0:00:14.00,Danmaku"),
            "{}",
            out
        );
    }

    #[test]
    fn shift_clamps_to_the_window() {
        // The first comment opens before the window and closes inside it.
        let (out, kept) = shift_ass(ASS, 8.0, 3.0);
        assert_eq!(kept, 2);
        // A negative start becomes zero and nothing passes the end of the window.
        for line in out.lines().filter(|l| l.starts_with("Dialogue:")) {
            let rest = line.split_once(':').unwrap().1;
            let f: Vec<&str> = rest.splitn(10, ',').collect();
            let s = parse_ass_time(f[1]).unwrap();
            let e = parse_ass_time(f[2]).unwrap();
            assert!(s >= 0.0, "{}", line);
            assert!(e <= 3.0, "{}", line);
        }
    }

    #[test]
    fn shift_drops_what_falls_outside() {
        let (out, kept) = shift_ass(ASS, 30.0, 10.0);
        assert_eq!(kept, 0, "{}", out);
        // No dialogue, but the header still stands.
        assert!(out.contains("[Events]"));
    }

    #[test]
    fn shift_without_cut_is_a_copy_of_the_events() {
        let (out, kept) = shift_ass(ASS, 0.0, 3600.0);
        assert_eq!(kept, 3);
        assert!(out.contains("0:00:01.00,0:00:13.00"));
        assert!(out.contains("0:01:00.00,0:01:12.00"));
    }

    #[test]
    fn srt_goes_through_the_tools_parser() {
        let srt = "1\n00:00:05,000 --> 00:00:07,000\nprimeira\n\n2\n00:01:00,000 --> 00:01:02,000\nsegunda\n\n";
        let (out, kept) = shift_other(srt, 4.0, 10.0).unwrap();
        assert_eq!(kept, 1);
        assert!(out.contains("[Events]"));
        assert!(out.contains("Dialogue: 0,0:00:01.00,0:00:03.00"), "{}", out);
    }

    #[test]
    fn cut_args_are_omitted_when_zero() {
        assert!(cut_args(0.0, 0.0).is_empty());
        assert_eq!(cut_args(2.5, 0.0), vec!["-ss", "2.500"]);
        assert_eq!(cut_args(0.0, 4.0), vec!["-t", "4.000"]);
    }

    #[test]
    fn filter_escapes_the_path_and_scales_before_drawing() {
        let chain = filter_chain(Path::new("C:\\temp\\a.ass"), "", 0);
        assert_eq!(chain, "ass='C\\:/temp/a.ass'");

        let scaled = filter_chain(Path::new("/tmp/a.ass"), "/tmp/fontes", 720);
        assert!(scaled.starts_with("scale=-2:'min(720,ih)'"), "{}", scaled);
        assert!(
            scaled.contains("ass='/tmp/a.ass':fontsdir='/tmp/fontes'"),
            "{}",
            scaled
        );
        // The scale has to come before ass, otherwise the text is shrunk along with it.
        assert!(scaled.find("scale=").unwrap() < scaled.find("ass=").unwrap());
    }

    #[test]
    fn odd_heights_are_rounded_down_for_h264() {
        assert!(filter_chain(Path::new("/tmp/a.ass"), "", 721).contains("min(720,ih)"));
    }

    #[test]
    fn unknown_preset_falls_back() {
        assert_eq!(sane_preset("slow"), "slow");
        assert_eq!(sane_preset("turbo"), "veryfast");
        assert_eq!(sane_preset(" medium "), "medium");
    }

    #[test]
    fn audio_is_reencoded_or_dropped() {
        assert_eq!(audio_args(false, 96), vec!["-an"]);
        assert_eq!(audio_args(true, 0), vec!["-c:a", "aac", "-b:a", "128k"]);
        assert_eq!(audio_args(true, 96), vec!["-c:a", "aac", "-b:a", "96k"]);
    }

    /// End to end with the managed FFmpeg: generates a clip, cuts it and checks
    /// the comment appears at the right time (the frame gets brighter when the
    /// white text comes on screen).
    /// `cargo test -p gumballz-get --lib -- --ignored live_danmaku_burn`
    #[tokio::test]
    #[ignore]
    async fn live_danmaku_burn_keeps_the_timing() {
        let ffmpeg = gumballz_core::core::dependencies::ensure_ffmpeg()
            .await
            .unwrap();
        let dir = std::env::temp_dir().join("gumballz-get-danmaku-burn-live");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("fonte.mp4");
        let gen = gumballz_core::core::process::command(&ffmpeg)
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:size=640x360:rate=25:duration=20",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=20",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ])
            .arg(&src)
            .output()
            .await
            .unwrap();
        assert!(gen.status.success());

        // A single comment, from 12s to 16s in the original video.
        let ass = dir.join("danmaku.ass");
        std::fs::write(
            &ass,
            "[Script Info]\nScriptType: v4.00+\nPlayResX: 640\nPlayResY: 360\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Danmaku,Arial,72,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,1.0,0.0,7,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:12.00,0:00:16.00,Danmaku,,0,0,0,,{\\pos(320,160)}WWWWWWWW\n",
        )
        .unwrap();

        let opts = BurnOptions {
            video: src.to_string_lossy().to_string(),
            subtitle: ass.to_string_lossy().to_string(),
            start: 10.0,
            duration: 8.0,
            target_mb: 0.0,
            crf: 20,
            preset: "ultrafast".into(),
            max_height: 0,
            fonts_dir: String::new(),
            output_dir: dir.to_string_lossy().to_string(),
            suffix: String::new(),
        };
        let res = run(&opts, &gumballz_core::core::tools::noop_progress())
            .await
            .unwrap();
        assert_eq!(res.dialogues, 1);
        assert!(res.bytes > 1000, "came out too small: {}", res.bytes);
        let probe = gumballz_core::core::ffmpeg::probe(Path::new(&res.output))
            .await
            .unwrap();
        assert!(
            (probe.duration_seconds - 8.0).abs() < 0.5,
            "duration: {}",
            probe.duration_seconds
        );
        assert!(probe.streams.iter().any(|s| s.codec_type == "audio"));

        // The text has to be around 2s-6s of the clip, not at the start.
        let brightness = |at: f64| {
            let ffmpeg = ffmpeg.clone();
            let out = res.output.clone();
            let dir = dir.clone();
            async move {
                let png = dir.join(format!("f{:.0}.png", at * 10.0));
                let o = gumballz_core::core::process::command(&ffmpeg)
                    .args(["-y", "-hide_banner", "-loglevel", "error", "-ss"])
                    .arg(format!("{:.2}", at))
                    .args(["-i", &out, "-frames:v", "1"])
                    .arg(&png)
                    .output()
                    .await
                    .unwrap();
                assert!(o.status.success());
                std::fs::metadata(&png).unwrap().len()
            }
        };
        let antes = brightness(0.5).await;
        let durante = brightness(3.0).await;
        assert!(
            durante > antes,
            "the comment did not appear in the right window: {} vs {}",
            antes,
            durante
        );
        eprintln!("clip: {} bytes, {} dialogue(s)", res.bytes, res.dialogues);
    }

    /// Same clip, now aiming at a file size.
    /// `cargo test -p gumballz-get --lib -- --ignored live_danmaku_burn_target`
    #[tokio::test]
    #[ignore]
    async fn live_danmaku_burn_hits_the_size_target() {
        let ffmpeg = gumballz_core::core::dependencies::ensure_ffmpeg()
            .await
            .unwrap();
        let dir = std::env::temp_dir().join("gumballz-get-danmaku-target-live");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("fonte.mp4");
        let gen = gumballz_core::core::process::command(&ffmpeg)
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=30:duration=15",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&src)
            .output()
            .await
            .unwrap();
        assert!(gen.status.success());

        let ass = dir.join("danmaku.ass");
        std::fs::write(
            &ass,
            "[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:15.00,Default,,0,0,0,,{\\pos(640,360)}danmaku\n",
        )
        .unwrap();

        let opts = BurnOptions {
            video: src.to_string_lossy().to_string(),
            subtitle: ass.to_string_lossy().to_string(),
            start: 0.0,
            duration: 0.0,
            target_mb: 1.0,
            crf: 20,
            preset: "veryfast".into(),
            max_height: 480,
            fonts_dir: String::new(),
            output_dir: dir.to_string_lossy().to_string(),
            suffix: "-alvo".into(),
        };
        let res = run(&opts, &gumballz_core::core::tools::noop_progress())
            .await
            .unwrap();
        assert!(res.video_kbps > 0);
        assert!(
            res.bytes <= 1024 * 1024,
            "{} bytes passaram de 1 MB",
            res.bytes
        );
        // The two-pass log must not be left behind.
        assert!(
            !std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains("pass")),
            "sobrou log de passagem"
        );
        eprintln!("target 1 MB → {} bytes at {} kbps", res.bytes, res.video_kbps);
    }
}
