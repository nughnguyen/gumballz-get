//! Automatic chapters for publishers: where the topic changes there is usually
//! a pause in the speech and an image cut at the same time.
//!
//! Two signals go in — `silencedetect` (the same detector as the silence cut)
//! and `select='gt(scene,N)'` with `showinfo` — and they are merged: a mark
//! that appears in both weighs more than a mark from only one. Then comes the minimum
//! chapter duration, because YouTube ignores chapters that are too short and nobody
//! wants a forty-line outline.

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use super::silence_cut::Span;
use super::subtitle::Cue;
use super::yt_notes::fmt_stamp;

/// YouTube only builds the outline with three chapters or more, the first at
/// 0:00 and none shorter than ten seconds.
pub const YT_MIN_CHAPTERS: usize = 3;
pub const YT_MIN_SECONDS: f64 = 10.0;
const ID: &str = "yt-chapters";

// ── Sinais ─────────────────────────────────────────────────────────────

/// Reads the `pts_time` values `showinfo` prints after the scene filter.
pub fn parse_scene_times(stderr: &str) -> Vec<f64> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        if !line.contains("Parsed_showinfo") {
            continue;
        }
        let Some(idx) = line.find("pts_time:") else {
            continue;
        };
        let token = line[idx + "pts_time:".len()..]
            .split_whitespace()
            .next()
            .unwrap_or("");
        if let Ok(v) = token.parse::<f64>() {
            if v.is_finite() {
                out.push(v);
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub time: f64,
    pub score: f64,
    /// "silencio" | "cena" | "ambos"
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FuseOptions {
    /// Marks closer than this to each other are the same mark.
    #[serde(default = "default_window")]
    pub window: f64,
    /// No chapter comes out shorter than this.
    #[serde(default = "default_min_chapter")]
    pub min_chapter: f64,
    /// Chapter cap (0 = no cap).
    #[serde(default)]
    pub max_chapters: u32,
}

fn default_window() -> f64 {
    2.0
}
fn default_min_chapter() -> f64 {
    45.0
}

impl Default for FuseOptions {
    fn default() -> Self {
        Self {
            window: default_window(),
            min_chapter: default_min_chapter(),
            max_chapters: 0,
        }
    }
}

/// A long silence is a strong signal; the mark sits where the speech returns.
fn from_silences(silences: &[Span]) -> Vec<Candidate> {
    silences
        .iter()
        .filter(|s| s.start > 0.0)
        .map(|s| Candidate {
            time: s.end,
            score: (s.duration() / 3.0).clamp(0.2, 1.0),
            source: "silencio".to_string(),
        })
        .collect()
}

fn from_scenes(scenes: &[f64]) -> Vec<Candidate> {
    scenes
        .iter()
        .filter(|t| **t > 0.0)
        .map(|t| Candidate {
            time: *t,
            score: 0.55,
            source: "cena".to_string(),
        })
        .collect()
}

/// Joins what is close and returns the marks strong enough, in order of
/// time, respecting the minimum chapter duration.
pub fn merge_signals(silences: &[Span], scenes: &[f64], opts: &FuseOptions) -> Vec<Candidate> {
    let mut raw = from_silences(silences);
    raw.extend(from_scenes(scenes));
    raw.sort_by(|a, b| a.time.total_cmp(&b.time));

    // 1. Groups neighbouring marks into one.
    let window = opts.window.max(0.0);
    let mut clusters: Vec<Candidate> = Vec::new();
    for c in raw {
        match clusters.last_mut() {
            Some(last) if c.time - last.time <= window => {
                let confirma = last.source != c.source && last.source != "ambos";
                // Between two signals stuck together, the speech returning is the better edge.
                if c.source == "silencio" && last.source == "cena" {
                    last.time = c.time;
                }
                last.score = last.score.max(c.score);
                if confirma {
                    last.source = "ambos".to_string();
                    last.score = (last.score + 0.4).min(1.5);
                }
            }
            _ => clusters.push(c),
        }
    }

    // 2. The strongest first; each accepted one blocks its neighbourhood.
    let min = opts.min_chapter.max(1.0);
    let mut order: Vec<usize> = (0..clusters.len()).collect();
    order.sort_by(|a, b| {
        clusters[*b]
            .score
            .total_cmp(&clusters[*a].score)
            .then(clusters[*a].time.total_cmp(&clusters[*b].time))
    });
    let cap = if opts.max_chapters == 0 {
        usize::MAX
    } else {
        // The first chapter is 0:00, which does not come from here.
        (opts.max_chapters as usize).saturating_sub(1)
    };
    let mut kept: Vec<Candidate> = Vec::new();
    for i in order {
        if kept.len() >= cap {
            break;
        }
        let c = &clusters[i];
        if c.time < min {
            continue;
        }
        if kept.iter().any(|k| (k.time - c.time).abs() < min) {
            continue;
        }
        kept.push(c.clone());
    }
    kept.sort_by(|a, b| a.time.total_cmp(&b.time));
    kept
}

// ── Capítulos ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterOut {
    pub start: f64,
    pub end: f64,
    pub title: String,
    pub source: String,
}

fn shorten(text: &str, max_chars: usize) -> String {
    let clean = super::yt_notes::clean_line(text);
    let mut out = String::new();
    for word in clean.split_whitespace() {
        if !out.is_empty() && out.chars().count() + 1 + word.chars().count() > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    let out = out.trim_end_matches([',', '.', ';', ':', '-', '…']).trim();
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Title taken from the speech that opens the chapter. Without a transcript, "Part N".
pub fn title_at(cues: &[Cue], start: f64, index: usize) -> String {
    let start_ms = (start * 1000.0).round() as i64;
    let found = cues
        .iter()
        .find(|c| c.end_ms >= start_ms)
        .map(|c| shorten(&c.text, 58))
        .unwrap_or_default();
    if found.is_empty() {
        format!("Part {}", index + 1)
    } else {
        found
    }
}

/// Closes the chapters: the first always at 0:00, each one ending where the
/// next starts, and a tail too short goes back to the previous one.
pub fn build_chapters(
    marks: &[Candidate],
    cues: &[Cue],
    total: f64,
    min_chapter: f64,
) -> Vec<ChapterOut> {
    let total = total.max(0.0);
    let mut starts: Vec<(f64, String)> = vec![(0.0, "inicio".to_string())];
    for m in marks {
        if m.time > 0.0 && m.time < total {
            starts.push((m.time, m.source.clone()));
        }
    }
    let mut out: Vec<ChapterOut> = Vec::new();
    for (i, (start, source)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map(|(t, _)| *t).unwrap_or(total);
        out.push(ChapterOut {
            start: *start,
            end,
            title: title_at(cues, *start, i),
            source: source.clone(),
        });
    }
    // A short last chapter is leftover intro, not a chapter.
    if out.len() > 1 {
        let last_short = out
            .last()
            .map(|c| c.end - c.start < min_chapter.max(YT_MIN_SECONDS))
            .unwrap_or(false);
        if last_short {
            if let Some(c) = out.pop() {
                if let Some(prev) = out.last_mut() {
                    prev.end = c.end;
                }
            }
        }
    }
    out
}

/// The block that gets pasted into the video description.
pub fn description_block(chapters: &[ChapterOut]) -> String {
    chapters
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                fmt_stamp((c.start * 1000.0).round() as i64),
                c.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_meta(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '=' | ';' | '#' | '\\' | '\n') {
            out.push('\\');
        }
        if ch == '\n' {
            out.push('n');
        } else {
            out.push(ch);
        }
    }
    out
}

/// FFmpeg chapter metadata (`-i meta.txt -map_metadata 1`).
pub fn ffmetadata(chapters: &[ChapterOut]) -> String {
    let mut out = String::from(";FFMETADATA1\n");
    for c in chapters {
        out.push_str("\n[CHAPTER]\nTIMEBASE=1/1000\n");
        out.push_str(&format!("START={}\n", (c.start * 1000.0).round() as i64));
        out.push_str(&format!("END={}\n", (c.end * 1000.0).round() as i64));
        out.push_str(&format!("title={}\n", escape_meta(&c.title)));
    }
    out
}

/// What is still missing for YouTube to accept the outline.
pub fn youtube_note(chapters: &[ChapterOut]) -> Option<String> {
    if chapters.len() < YT_MIN_CHAPTERS {
        return Some(format!(
            "YouTube only builds the outline with {} chapters or more",
            YT_MIN_CHAPTERS
        ));
    }
    match chapters.first() {
        Some(c) if c.start > 0.0 => {
            return Some("the first chapter must start at 0:00".into())
        }
        None => return Some("no chapters".into()),
        _ => {}
    }
    if chapters
        .iter()
        .any(|c| c.end - c.start < YT_MIN_SECONDS - 1e-6)
    {
        return Some(format!(
            "each chapter needs at least {:.0} seconds",
            YT_MIN_SECONDS
        ));
    }
    None
}

// ── Execução ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    /// The creator's own video file.
    pub input: String,
    /// Subtitle or transcript, only to name the chapters.
    #[serde(default)]
    pub subtitle_path: String,
    #[serde(default = "default_true")]
    pub use_silence: bool,
    #[serde(default = "default_true")]
    pub use_scene: bool,
    #[serde(default = "default_db")]
    pub silence_db: f64,
    #[serde(default = "default_min_silence")]
    pub min_silence: f64,
    #[serde(default = "default_scene")]
    pub scene_threshold: f64,
    #[serde(default)]
    pub fuse: FuseOptions,
    /// Write `capitulos.txt` and `capitulos.ffmeta` next to the video.
    #[serde(default)]
    pub write_files: bool,
    #[serde(default)]
    pub output_dir: String,
}

fn default_true() -> bool {
    true
}
fn default_db() -> f64 {
    -32.0
}
fn default_min_silence() -> f64 {
    1.2
}
fn default_scene() -> f64 {
    0.4
}

#[derive(Debug, Clone, Serialize)]
pub struct ChaptersResult {
    pub input: String,
    pub duration_seconds: f64,
    pub chapters: Vec<ChapterOut>,
    pub description: String,
    pub ffmetadata: String,
    pub description_path: String,
    pub metadata_path: String,
    pub silence_marks: usize,
    pub scene_marks: usize,
    pub titled_from_transcript: bool,
    pub youtube_ready: bool,
    pub youtube_note: Option<String>,
}

async fn detect_silence(
    ffmpeg: &Path,
    input: &Path,
    db: f64,
    min: f64,
    total: f64,
) -> anyhow::Result<Vec<Span>> {
    let out = crate::core::process::command(ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(input)
        .args([
            "-af",
            &format!("silencedetect=noise={}dB:d={}", db, min.max(0.05)),
            "-vn",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok(super::silence_cut::parse_spans(&stderr, total))
}

async fn detect_scenes(ffmpeg: &Path, input: &Path, threshold: f64) -> anyhow::Result<Vec<f64>> {
    let out = crate::core::process::command(ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(input)
        .args([
            "-vf",
            &format!(
                "select='gt(scene,{})',showinfo",
                threshold.clamp(0.05, 0.95)
            ),
            "-an",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .map_err(|e| anyhow!("ffmpeg did not start: {}", e))?;
    Ok(parse_scene_times(&String::from_utf8_lossy(&out.stderr)))
}

pub async fn run(opts: Options, progress: super::ProgressFn) -> anyhow::Result<ChaptersResult> {
    let input = PathBuf::from(opts.input.trim());
    if !input.exists() {
        return Err(anyhow!("file not found: {}", opts.input));
    }
    let ffmpeg = crate::core::dependencies::ensure_ffmpeg().await?;
    super::report(&progress, ID, "progress", 0, Some(3), None);
    let probe = crate::core::ffmpeg::probe(&input).await?;
    let total = probe.duration_seconds;
    if total <= 0.0 {
        return Err(anyhow!("could not measure the video duration"));
    }

    let mut cues: Vec<Cue> = Vec::new();
    if !opts.subtitle_path.trim().is_empty() {
        if let Ok(text) = std::fs::read_to_string(opts.subtitle_path.trim()) {
            cues = super::subtitle::parse(&text).unwrap_or_default();
            cues = super::yt_notes::dedup_cues(&cues);
        }
    }
    let titled = !cues.is_empty();

    let silences = if opts.use_silence {
        super::report(&progress, ID, "progress", 1, Some(3), None);
        detect_silence(&ffmpeg, &input, opts.silence_db, opts.min_silence, total).await?
    } else {
        Vec::new()
    };
    let scenes = if opts.use_scene && probe.streams.iter().any(|s| s.codec_type == "video") {
        super::report(&progress, ID, "progress", 2, Some(3), None);
        detect_scenes(&ffmpeg, &input, opts.scene_threshold).await?
    } else {
        Vec::new()
    };
    if silences.is_empty() && scenes.is_empty() {
        return Err(anyhow!(
            "nenhuma pausa nem corte de cena encontrado; afrouxe os limiares"
        ));
    }

    let marks = merge_signals(&silences, &scenes, &opts.fuse);
    let chapters = build_chapters(&marks, &cues, total, opts.fuse.min_chapter);
    let description = description_block(&chapters);
    let meta = ffmetadata(&chapters);

    let (mut desc_path, mut meta_path) = (String::new(), String::new());
    if opts.write_files {
        let dir = if opts.output_dir.trim().is_empty() {
            input.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        } else {
            PathBuf::from(opts.output_dir.trim())
        };
        std::fs::create_dir_all(&dir)?;
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "video".into());
        let d = dir.join(format!("{}-capitulos.txt", stem));
        let m = dir.join(format!("{}-capitulos.ffmeta", stem));
        std::fs::write(&d, description.as_bytes())?;
        std::fs::write(&m, meta.as_bytes())?;
        desc_path = d.to_string_lossy().to_string();
        meta_path = m.to_string_lossy().to_string();
    }

    let note = youtube_note(&chapters);
    super::report(&progress, ID, "done", 3, Some(3), None);
    Ok(ChaptersResult {
        input: input.to_string_lossy().to_string(),
        duration_seconds: total,
        chapters,
        description,
        ffmetadata: meta,
        description_path: desc_path,
        metadata_path: meta_path,
        silence_marks: silences.len(),
        scene_marks: scenes.len(),
        titled_from_transcript: titled,
        youtube_ready: note.is_none(),
        youtube_note: note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOWINFO: &str = "\
[Parsed_showinfo_1 @ 0x1] n:0 pts:0 pts_time:0 duration:1
[Parsed_showinfo_1 @ 0x1] n:1 pts:1500 pts_time:60.5 duration:1
frame= 2 fps=0.0 q=-0.0 size=N/A time=00:00:00.08
[Parsed_showinfo_1 @ 0x1] n:2 pts:3000 pts_time:121.25 duration:1
[Parsed_showinfo_1 @ 0x1] n:3 pts:9000 pts_time:nan duration:1
";

    fn span(start: f64, end: f64) -> Span {
        Span { start, end }
    }

    #[test]
    fn reads_the_scene_times() {
        let t = parse_scene_times(SHOWINFO);
        assert_eq!(t, vec![0.0, 60.5, 121.25], "the invalid pts_time drops out");
        assert!(parse_scene_times("nada aqui").is_empty());
    }

    #[test]
    fn a_mark_seen_by_both_signals_beats_one_seen_by_a_single_one() {
        // 60 s has silence and a scene cut; 130 s only a scene; 200 s only silence.
        let silences = [span(57.0, 60.0), span(199.0, 200.0)];
        let scenes = [60.4, 130.0];
        let marks = merge_signals(&silences, &scenes, &FuseOptions::default());
        let em_60 = marks
            .iter()
            .find(|m| (m.time - 60.0).abs() < 1.0)
            .expect("the 60 s mark should have been there");
        assert_eq!(em_60.source, "ambos");
        let so_cena = marks
            .iter()
            .find(|m| (m.time - 130.0).abs() < 0.1)
            .expect("the 130 s mark should have been there");
        assert!(
            em_60.score > so_cena.score,
            "the confirmed one must weigh more"
        );
        assert_eq!(marks.len(), 3);
    }

    #[test]
    fn minimum_chapter_length_wins_over_the_weaker_mark() {
        let silences = [span(58.0, 60.0), span(64.0, 65.0)];
        let scenes = [60.5];
        let opts = FuseOptions {
            window: 2.0,
            min_chapter: 45.0,
            max_chapters: 0,
        };
        let marks = merge_signals(&silences, &scenes, &opts);
        assert_eq!(marks.len(), 1, "the two marks are 5 s apart");
        assert_eq!(marks[0].source, "ambos", "the one confirmed by both remained");
    }

    #[test]
    fn a_mark_before_the_minimum_never_becomes_a_chapter() {
        let silences = [span(5.0, 8.0)];
        let marks = merge_signals(&silences, &[], &FuseOptions::default());
        assert!(marks.is_empty(), "8 s does not open a chapter with a 45 s minimum");
    }

    #[test]
    fn the_cap_keeps_the_strongest_marks() {
        let silences: Vec<Span> = (1..=8)
            .map(|i| span(i as f64 * 60.0 - 1.0, i as f64 * 60.0))
            .collect();
        // The 300 s one is confirmed by the scene, so it has to survive the cap.
        let scenes = [300.2];
        let opts = FuseOptions {
            window: 2.0,
            min_chapter: 45.0,
            max_chapters: 3,
        };
        let marks = merge_signals(&silences, &scenes, &opts);
        assert_eq!(marks.len(), 2, "cap of 3 chapters = 0:00 + 2 marks");
        assert!(marks.iter().any(|m| m.source == "ambos"));
    }

    #[test]
    fn no_signal_means_no_mark() {
        assert!(merge_signals(&[], &[], &FuseOptions::default()).is_empty());
    }

    fn cues_de_teste() -> Vec<Cue> {
        vec![
            Cue {
                start_ms: 500,
                end_ms: 3000,
                text: "bem-vindo ao canal.".into(),
            },
            Cue {
                start_ms: 60_000,
                end_ms: 63_000,
                text: "a moagem certa,".into(),
            },
        ]
    }

    #[test]
    fn chapters_start_at_zero_and_chain_up_to_the_end() {
        let marks = vec![Candidate {
            time: 60.0,
            score: 1.0,
            source: "ambos".into(),
        }];
        let chs = build_chapters(&marks, &cues_de_teste(), 300.0, 45.0);
        assert_eq!(chs.len(), 2);
        assert_eq!(chs[0].start, 0.0);
        assert_eq!(chs[0].end, 60.0);
        assert_eq!(chs[1].end, 300.0);
        assert_eq!(chs[0].title, "Bem-vindo ao canal", "the final period comes off");
        assert_eq!(chs[1].title, "A moagem certa", "trailing comma drops out");
    }

    #[test]
    fn a_long_first_sentence_is_cut_on_a_word_boundary() {
        let cues = vec![Cue {
            start_ms: 0,
            end_ms: 1000,
            text: "palavra ".repeat(12).trim().to_string(),
        }];
        let titulo = title_at(&cues, 0.0, 0);
        assert!(titulo.chars().count() <= 58, "ended up with {}", titulo.len());
        assert_eq!(titulo.split_whitespace().count(), 7);
        assert!(titulo.starts_with("Palavra palavra"));
    }

    #[test]
    fn without_a_transcript_the_titles_are_numbered() {
        let marks = vec![Candidate {
            time: 60.0,
            score: 1.0,
            source: "cena".into(),
        }];
        let chs = build_chapters(&marks, &[], 200.0, 45.0);
        assert_eq!(chs[0].title, "Parte 1");
        assert_eq!(chs[1].title, "Parte 2");
    }

    #[test]
    fn a_stub_of_a_last_chapter_goes_back_into_the_previous_one() {
        let marks = vec![
            Candidate {
                time: 60.0,
                score: 1.0,
                source: "cena".into(),
            },
            Candidate {
                time: 297.0,
                score: 1.0,
                source: "cena".into(),
            },
        ];
        let chs = build_chapters(&marks, &[], 300.0, 45.0);
        assert_eq!(chs.len(), 2, "the 3 s tail is not a chapter");
        assert_eq!(chs[1].end, 300.0);
    }

    #[test]
    fn the_description_is_in_the_format_youtube_reads() {
        let chs = build_chapters(
            &[
                Candidate {
                    time: 65.0,
                    score: 1.0,
                    source: "cena".into(),
                },
                Candidate {
                    time: 3725.0,
                    score: 1.0,
                    source: "cena".into(),
                },
            ],
            &[],
            7200.0,
            45.0,
        );
        let block = description_block(&chs);
        assert_eq!(block, "0:00 Parte 1\n1:05 Parte 2\n1:02:05 Parte 3");
        assert!(
            block
                .lines()
                .next()
                .map(|l| l.starts_with("0:00"))
                .unwrap_or(false),
            "a primeira linha tem de ser 0:00"
        );
    }

    #[test]
    fn ffmetadata_has_one_block_per_chapter_in_milliseconds() {
        let chs = build_chapters(
            &[Candidate {
                time: 60.0,
                score: 1.0,
                source: "cena".into(),
            }],
            &[],
            300.0,
            45.0,
        );
        let meta = ffmetadata(&chs);
        assert!(meta.starts_with(";FFMETADATA1"));
        assert_eq!(meta.matches("[CHAPTER]").count(), 2);
        assert!(meta.contains("TIMEBASE=1/1000"));
        assert!(meta.contains("START=0\nEND=60000\n"));
        assert!(meta.contains("START=60000\nEND=300000\n"));
    }

    #[test]
    fn ffmetadata_escapes_what_would_break_the_file() {
        let chs = vec![ChapterOut {
            start: 0.0,
            end: 10.0,
            title: "preço = R$5; #1".into(),
            source: "cena".into(),
        }];
        assert!(ffmetadata(&chs).contains("title=preço \\= R$5\\; \\#1"));
    }

    #[test]
    fn youtube_rules_are_checked() {
        let poucos = build_chapters(&[], &[], 300.0, 45.0);
        assert!(youtube_note(&poucos).is_some(), "a single chapter is not enough");

        let bons = build_chapters(
            &[
                Candidate {
                    time: 60.0,
                    score: 1.0,
                    source: "cena".into(),
                },
                Candidate {
                    time: 120.0,
                    score: 1.0,
                    source: "cena".into(),
                },
            ],
            &[],
            300.0,
            45.0,
        );
        assert_eq!(bons.len(), 3);
        assert_eq!(youtube_note(&bons), None);

        let curto = vec![
            ChapterOut {
                start: 0.0,
                end: 5.0,
                title: "a".into(),
                source: "cena".into(),
            },
            ChapterOut {
                start: 5.0,
                end: 60.0,
                title: "b".into(),
                source: "cena".into(),
            },
            ChapterOut {
                start: 60.0,
                end: 120.0,
                title: "c".into(),
                source: "cena".into(),
            },
        ];
        assert!(youtube_note(&curto)
            .unwrap_or_default()
            .contains("10 segundos"));
    }

    /// Generates a clip with two scene cuts and silence in the middle and checks
    /// the chapters land where they should.
    /// `cargo test -p gumballz-core --lib -- --ignored live_yt_chapters`
    #[tokio::test]
    #[ignore]
    async fn live_yt_chapters_finds_the_planted_cut() {
        let ffmpeg = crate::core::dependencies::ensure_ffmpeg()
            .await
            .expect("managed ffmpeg");
        let dir = super::super::temp_dir().join("yt-chapters-live");
        std::fs::create_dir_all(&dir).expect("folder");
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
                "testsrc2=size=320x240:rate=15:duration=60",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=60,volume=enable='between(t,28,34)':volume=0",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&src)
            .output()
            .await
            .expect("gerou o clipe");
        assert!(gen.status.success());

        let opts = Options {
            input: src.to_string_lossy().to_string(),
            subtitle_path: String::new(),
            use_silence: true,
            use_scene: false,
            silence_db: -30.0,
            min_silence: 1.0,
            scene_threshold: 0.4,
            fuse: FuseOptions {
                window: 2.0,
                min_chapter: 15.0,
                max_chapters: 0,
            },
            write_files: false,
            output_dir: String::new(),
        };
        let r = run(opts, super::super::noop_progress())
            .await
            .expect("chapters");
        eprintln!("{}", r.description);
        assert!(r.chapters.len() >= 2, "it was supposed to find the cut at ~34 s");
        assert!((r.chapters[1].start - 34.0).abs() < 2.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
