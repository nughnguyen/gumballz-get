//! Music tags (ID3v2 / Vorbis Comments / MP4 ilst) in pure Rust.
//!
//! All reading and writing goes through `lofty`, which normalizes the three worlds
//! into a generic `Tag`: the same code serves MP3, FLAC, M4A, OGG, Opus and WAV,
//! with no ffmpeg and no external binary.
//!
//! There are three steps, in the order the UI uses them:
//!
//! 1. `scan` — reads a folder (or a list of files) and returns a table
//!    with format, duration, bitrate and each track's fields.
//! 2. diagnosis — `diagnose` points out what is wrong in each file:
//!    empty required field, swapped title and artist, track stored as
//!    "3/12" text, invalid year, mojibake (UTF-8 read as latin-1),
//!    missing cover, huge cover, duplicate cover, wrong-format tag.
//! 3. `edit` — applies the corrections in batch. The default is `dry_run`: the function
//!    returns the field-by-field diff and does NOT touch the file. It only writes when the
//!    user sends `dry_run: false`.
//!
//! There is no fingerprint identification (AcoustID/MusicBrainz) here:
//! that would send user data off the machine and was left out on
//! purpose.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use lofty::config::WriteOptions as LoftyWriteOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{Accessor, ItemKey};
use lofty::tag::{Tag, TagType};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::{report, ProgressFn};

/// Identifier the UI listens for in the progress event.
const TOOL_ID: &str = "audio-tag";

/// Above this, the embedded cover hurts more than it helps: it inflates each
/// album file and stalls old devices.
const COVER_HUGE_BYTES: u64 = 2 * 1024 * 1024;

/// Extensions that `lofty` knows how to open.
const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "m4b", "m4p", "mp4", "flac", "ogg", "oga", "opus", "spx", "wav", "wave", "aiff",
    "aif", "aifc", "aac", "ape", "wv", "mpc", "mp2", "mp1",
];

// ─────────────────────────── input and output ────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScanOptions {
    /// Folders to scan.
    #[serde(default)]
    pub dirs: Vec<String>,
    /// Loose files (added to the folders).
    #[serde(default)]
    pub files: Vec<String>,
    /// Go into subfolders.
    #[serde(default)]
    pub recursive: bool,
    /// File cap, so the UI does not stall on a whole library.
    #[serde(default)]
    pub max_files: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TrackTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disc: Option<u32>,
    pub disc_total: Option<u32>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    /// What is stored in the track field, uninterpreted ("3/12").
    pub track_raw: Option<String>,
    /// Same for the disc.
    pub disc_raw: Option<String>,
}

/// A problem found in a file. `fixable` says whether this same tool's `edit` fixes it on its own.
#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub code: String,
    pub field: Option<String>,
    pub severity: String,
    pub detail: String,
    pub fixable: bool,
}

impl Issue {
    fn new(code: &str, field: Option<&str>, severity: &str, detail: String, fixable: bool) -> Self {
        Self {
            code: code.to_string(),
            field: field.map(str::to_string),
            severity: severity.to_string(),
            detail,
            fixable,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverInfo {
    pub count: usize,
    pub bytes: u64,
    pub mime: Option<String>,
    /// How many front covers exist (more than one is a duplicate).
    pub front_count: usize,
    /// Two images with identical bytes inside the same tag.
    pub duplicated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackRow {
    pub path: String,
    pub file_name: String,
    pub format: String,
    /// Tag read, in lofty's vocabulary ("Id3v2", "VorbisComments"…).
    pub tag_type: Option<String>,
    /// Tag the container expects.
    pub primary_tag_type: String,
    pub bytes: u64,
    pub duration_secs: f64,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub tags: TrackTags,
    pub cover: CoverInfo,
    pub issues: Vec<Issue>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub scanned: usize,
    pub failed: usize,
    /// Files with at least one problem.
    pub with_issues: usize,
    /// Count per problem code, for the UI to show the summary.
    pub issue_counts: BTreeMap<String, usize>,
    pub tracks: Vec<TrackRow>,
}

/// A field change, the way `dry_run` shows it to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldDiff {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Which operation asked for the change ("set", "renumber", "mojibake"…).
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverDiff {
    /// "embed" | "remove" | "dedupe"
    pub action: String,
    pub from_bytes: u64,
    pub to_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub path: String,
    pub file_name: String,
    pub diffs: Vec<FieldDiff>,
    pub cover: Option<CoverDiff>,
    /// Fields the file format does not accept (e.g. year in AIFF).
    pub unsupported: Vec<String>,
    pub written: bool,
    pub backup: Option<String>,
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_one() -> u32 {
    1
}

/// Batch edit. The default is not to write: `dry_run` only becomes `false` when the
/// frontend sends the field explicitly.
#[derive(Debug, Clone, Deserialize)]
pub struct EditOptions {
    /// Order matters: it is what renumbering uses when `sort_by_name` is
    /// false.
    pub files: Vec<String>,
    /// Field → value. An empty string clears the field.
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    /// Fields to clear.
    #[serde(default)]
    pub clear: Vec<String>,
    /// Reorder the list by filename before renumbering.
    #[serde(default = "default_true")]
    pub sort_by_name: bool,
    /// Rewrite the track as 1, 2, 3… in list order.
    #[serde(default)]
    pub renumber: bool,
    #[serde(default = "default_one")]
    pub renumber_start: u32,
    /// Also write the track total.
    #[serde(default)]
    pub set_track_total: bool,
    /// Copy the artist to the album artist when the latter is empty.
    #[serde(default)]
    pub album_artist_from_artist: bool,
    /// Fix UTF-8 text that was read as latin-1.
    #[serde(default)]
    pub fix_mojibake: bool,
    /// Split "3/12" into track 3 and total 12.
    #[serde(default)]
    pub split_track_slash: bool,
    /// Swap title and artist around.
    #[serde(default)]
    pub swap_title_artist: bool,
    /// Image to embed as the front cover.
    #[serde(default)]
    pub cover_path: Option<String>,
    /// Remove all covers.
    #[serde(default)]
    pub remove_cover: bool,
    /// Keep only one cover when there are duplicates.
    #[serde(default)]
    pub dedupe_cover: bool,
    /// Copy the file to `<name>.bak` before writing.
    #[serde(default)]
    pub backup: bool,
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

impl Default for EditOptions {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            set: BTreeMap::new(),
            clear: Vec::new(),
            sort_by_name: true,
            renumber: false,
            renumber_start: 1,
            set_track_total: false,
            album_artist_from_artist: false,
            fix_mojibake: false,
            split_track_slash: false,
            swap_title_artist: false,
            cover_path: None,
            remove_cover: false,
            dedupe_cover: false,
            backup: false,
            dry_run: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EditResult {
    pub dry_run: bool,
    /// Files with at least one planned change.
    pub changed: usize,
    pub written: usize,
    pub failed: usize,
    pub files: Vec<FileChange>,
}

// ───────────────────────────── fields ───────────────────────────────────

/// Fields the tool edits, in the order the table shows them.
pub const FIELDS: &[&str] = &[
    "title",
    "artist",
    "album_artist",
    "album",
    "year",
    "track",
    "track_total",
    "disc",
    "disc_total",
    "genre",
    "comment",
];

/// Free-text fields — the ones that can suffer mojibake.
const TEXT_FIELDS: &[&str] = &[
    "title",
    "artist",
    "album_artist",
    "album",
    "genre",
    "comment",
];

fn item_key(field: &str) -> Option<ItemKey> {
    Some(match field {
        "title" => ItemKey::TrackTitle,
        "artist" => ItemKey::TrackArtist,
        "album_artist" => ItemKey::AlbumArtist,
        "album" => ItemKey::AlbumTitle,
        "year" => ItemKey::Year,
        "track" => ItemKey::TrackNumber,
        "track_total" => ItemKey::TrackTotal,
        "disc" => ItemKey::DiscNumber,
        "disc_total" => ItemKey::DiscTotal,
        "genre" => ItemKey::Genre,
        "comment" => ItemKey::Comment,
        _ => return None,
    })
}

// ───────────────────────── tested pure helpers ─────────────────────

/// Byte that produced this character when CP1252/latin-1 bytes became text.
fn cp1252_byte(c: char) -> Option<u8> {
    let code = c as u32;
    if code < 0x80 {
        return Some(code as u8);
    }
    // C1 range: in CP1252 these bytes are typographic punctuation.
    const C1: &[(char, u8)] = &[
        ('\u{20AC}', 0x80),
        ('\u{201A}', 0x82),
        ('\u{0192}', 0x83),
        ('\u{201E}', 0x84),
        ('\u{2026}', 0x85),
        ('\u{2020}', 0x86),
        ('\u{2021}', 0x87),
        ('\u{02C6}', 0x88),
        ('\u{2030}', 0x89),
        ('\u{0160}', 0x8A),
        ('\u{2039}', 0x8B),
        ('\u{0152}', 0x8C),
        ('\u{017D}', 0x8E),
        ('\u{2018}', 0x91),
        ('\u{2019}', 0x92),
        ('\u{201C}', 0x93),
        ('\u{201D}', 0x94),
        ('\u{2022}', 0x95),
        ('\u{2013}', 0x96),
        ('\u{2014}', 0x97),
        ('\u{02DC}', 0x98),
        ('\u{2122}', 0x99),
        ('\u{0161}', 0x9A),
        ('\u{203A}', 0x9B),
        ('\u{0153}', 0x9C),
        ('\u{017E}', 0x9E),
        ('\u{0178}', 0x9F),
    ];
    if let Some((_, b)) = C1.iter().find(|(ch, _)| *ch == c) {
        return Some(*b);
    }
    if (0xA0..=0xFF).contains(&code) {
        return Some(code as u8);
    }
    None
}

/// UTF-8 text that was decoded as latin-1/CP1252 ("MÃºsica").
/// Returns the fixed text, or `None` when it is not mojibake.
///
/// The test is the round-trip itself: it only returns something if every character fits
/// in one CP1252 byte **and** the resulting sequence is valid UTF-8. Text that
/// is already right ("Música") does not survive that round-trip, so there is
/// no silent false positive.
pub fn fix_mojibake(s: &str) -> Option<String> {
    if s.is_ascii() || s.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        bytes.push(cp1252_byte(c)?);
    }
    let fixed = String::from_utf8(bytes).ok()?;
    if fixed == s || fixed.is_empty() || fixed.is_ascii() {
        return None;
    }
    if fixed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return None;
    }
    Some(fixed)
}

/// Text that lost bytes in conversion (became `U+FFFD`) — it can be warned about,
/// but not fixed: the information no longer exists in the file.
pub fn has_lost_bytes(s: &str) -> bool {
    s.contains('\u{FFFD}')
}

/// "3/12", "3 of 12", "03-12", "3" → (track, total).
pub fn parse_track_pair(s: &str) -> (Option<u32>, Option<u32>) {
    let lower = s.trim().to_lowercase();
    let norm = lower.replace(" of ", "/").replace(" de ", "/");
    let mut it = norm.splitn(2, ['/', '\\', '-']);
    let first = it.next().unwrap_or_default();
    let second = it.next();
    (leading_number(first), second.and_then(leading_number))
}

fn leading_number(s: &str) -> Option<u32> {
    let digits: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

/// A plausible year inside a date field. Accepts "1998",
/// "1998-05-03", "1998-05-03T10:00:00" and "03/05/1998"; refuses "98", "0000"
/// and anything without four consecutive digits.
pub fn normalize_year(s: &str) -> Option<u16> {
    let chars: Vec<char> = s.trim().chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i - start == 4 {
            let run: String = chars[start..i].iter().collect();
            if let Ok(y) = run.parse::<u16>() {
                if (1000..=2999).contains(&y) {
                    return Some(y);
                }
            }
        }
    }
    None
}

/// Strips "01 - " / "01. " / "01 " from the start of the filename.
fn strip_leading_number(s: &str) -> &str {
    let t = s.trim_start();
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return t;
    }
    let rest = t[digits..].trim_start();
    rest.strip_prefix('-')
        .or_else(|| rest.strip_prefix('.'))
        .or_else(|| rest.strip_prefix('_'))
        .map(str::trim_start)
        .unwrap_or_else(|| if rest.len() < t.len() { rest } else { t })
}

fn loose_eq(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let (a, b) = (norm(a), norm(b));
    !a.is_empty() && a == b
}

/// A filename in the "Artist - Title" pattern that matches the tag backwards:
/// title and artist were swapped.
pub fn looks_swapped(file_stem: &str, title: Option<&str>, artist: Option<&str>) -> bool {
    let (Some(title), Some(artist)) = (title, artist) else {
        return false;
    };
    let stem = strip_leading_number(file_stem);
    let Some((left, right)) = stem.split_once(" - ") else {
        return false;
    };
    if right.contains(" - ") {
        return false;
    }
    // The name says "left = artist, right = title". If the tag stores the
    // opposite, and it is not the case that both are equal, it is swapped.
    loose_eq(left, title) && loose_eq(right, artist) && !loose_eq(title, artist)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NatPart {
    Num(u64),
    Text(String),
}

/// "Natural" sort key: "track2" comes before "track10".
fn natural_key(name: &str) -> Vec<NatPart> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut digits = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_digit() {
            if !buf.is_empty() {
                out.push(NatPart::Text(std::mem::take(&mut buf)));
            }
            digits.push(c);
        } else {
            if !digits.is_empty() {
                let n = digits.parse::<u64>().unwrap_or(u64::MAX);
                digits.clear();
                out.push(NatPart::Num(n));
            }
            buf.push(c);
        }
    }
    if !digits.is_empty() {
        out.push(NatPart::Num(digits.parse::<u64>().unwrap_or(u64::MAX)));
    }
    if !buf.is_empty() {
        out.push(NatPart::Text(buf));
    }
    out
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Sorts paths by filename, in natural order.
pub fn sort_paths_by_name(paths: &mut [String]) {
    paths.sort_by(|a, b| {
        let (ka, kb) = (natural_key(&file_name_of(a)), natural_key(&file_name_of(b)));
        ka.cmp(&kb).then_with(|| a.cmp(b))
    });
}

// ───────────────────────────── reading ──────────────────────────────────

fn format_label(ft: FileType) -> String {
    match ft {
        FileType::Aac => "AAC".into(),
        FileType::Aiff => "AIFF".into(),
        FileType::Ape => "APE".into(),
        FileType::Flac => "FLAC".into(),
        FileType::Mpeg => "MP3".into(),
        FileType::Mp4 => "MP4".into(),
        FileType::Mpc => "Musepack".into(),
        FileType::Opus => "Opus".into(),
        FileType::Vorbis => "Ogg Vorbis".into(),
        FileType::Speex => "Speex".into(),
        FileType::Wav => "WAV".into(),
        FileType::WavPack => "WavPack".into(),
        other => format!("{other:?}"),
    }
}

fn tag_label(tt: TagType) -> String {
    format!("{tt:?}")
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn text_of(tag: &Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn read_tags(tag: &Tag) -> TrackTags {
    let track_raw = text_of(tag, ItemKey::TrackNumber);
    let disc_raw = text_of(tag, ItemKey::DiscNumber);
    let (track_from_raw, total_from_raw) = track_raw
        .as_deref()
        .map(parse_track_pair)
        .unwrap_or((None, None));
    let (disc_from_raw, disc_total_from_raw) = disc_raw
        .as_deref()
        .map(parse_track_pair)
        .unwrap_or((None, None));

    TrackTags {
        title: text_of(tag, ItemKey::TrackTitle),
        artist: text_of(tag, ItemKey::TrackArtist),
        album_artist: text_of(tag, ItemKey::AlbumArtist),
        album: text_of(tag, ItemKey::AlbumTitle),
        year: text_of(tag, ItemKey::Year).or_else(|| text_of(tag, ItemKey::RecordingDate)),
        track: tag.track().or(track_from_raw),
        track_total: tag
            .track_total()
            .or_else(|| text_of(tag, ItemKey::TrackTotal).and_then(|s| leading_number(&s)))
            .or(total_from_raw),
        disc: tag.disk().or(disc_from_raw),
        disc_total: tag
            .disk_total()
            .or_else(|| text_of(tag, ItemKey::DiscTotal).and_then(|s| leading_number(&s)))
            .or(disc_total_from_raw),
        genre: text_of(tag, ItemKey::Genre),
        comment: text_of(tag, ItemKey::Comment),
        track_raw,
        disc_raw,
    }
}

fn read_cover(tag: &Tag) -> CoverInfo {
    let pics = tag.pictures();
    let biggest = pics.iter().max_by_key(|p| p.data().len());
    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut duplicated = false;
    for p in pics {
        if !seen.insert(p.data()) {
            duplicated = true;
        }
    }
    CoverInfo {
        count: pics.len(),
        bytes: biggest.map(|p| p.data().len() as u64).unwrap_or(0),
        mime: biggest
            .and_then(|p| p.mime_type())
            .map(|m| m.as_str().to_string()),
        front_count: pics
            .iter()
            .filter(|p| p.pic_type() == PictureType::CoverFront)
            .count(),
        duplicated,
    }
}

fn empty_cover() -> CoverInfo {
    CoverInfo {
        count: 0,
        bytes: 0,
        mime: None,
        front_count: 0,
        duplicated: false,
    }
}

/// Reads a file. A read error becomes a row with `error` filled in —
/// a corrupted file in the middle of a folder is a normal case, not a reason to stop.
pub fn read_track(path: &Path) -> TrackRow {
    let path_str = path.to_string_lossy().to_string();
    let file_name = file_name_of(&path_str);
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let mut row = TrackRow {
        path: path_str,
        file_name,
        format: String::new(),
        tag_type: None,
        primary_tag_type: String::new(),
        bytes,
        duration_secs: 0.0,
        bitrate_kbps: None,
        sample_rate: None,
        channels: None,
        tags: TrackTags::default(),
        cover: empty_cover(),
        issues: Vec::new(),
        error: None,
    };

    let tagged = match lofty::read_from_path(path) {
        Ok(t) => t,
        Err(e) => {
            row.error = Some(e.to_string());
            return row;
        }
    };

    let props = tagged.properties();
    row.format = format_label(tagged.file_type());
    row.primary_tag_type = tag_label(tagged.primary_tag_type());
    row.duration_secs = props.duration().as_secs_f64();
    row.bitrate_kbps = props.audio_bitrate().or_else(|| props.overall_bitrate());
    row.sample_rate = props.sample_rate();
    row.channels = props.channels();

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    if let Some(tag) = tag {
        row.tag_type = Some(tag_label(tag.tag_type()));
        row.tags = read_tags(tag);
        row.cover = read_cover(tag);
    }

    let other_tags: Vec<TagType> = tagged
        .tags()
        .iter()
        .map(Tag::tag_type)
        .filter(|t| *t != tagged.primary_tag_type())
        .collect();
    row.issues = diagnose(&row, tagged.primary_tag().is_none(), &other_tags);
    row
}

// ─────────────────────────── diagnosis ────────────────────────────────

/// List of a track's problems. `no_primary` says the file has a tag,
/// but not the one the container expects; `other_tags` are the "foreign" tags
/// left in the file.
pub fn diagnose(row: &TrackRow, no_primary: bool, other_tags: &[TagType]) -> Vec<Issue> {
    let mut out = Vec::new();
    let t = &row.tags;

    if row.tag_type.is_none() {
        out.push(Issue::new(
            "no_tag",
            None,
            "error",
            "file without any tags".into(),
            false,
        ));
        return out;
    }

    for (field, value) in [
        ("title", &t.title),
        ("artist", &t.artist),
        ("album", &t.album),
    ] {
        if value.is_none() {
            out.push(Issue::new(
                "missing_field",
                Some(field),
                if field == "album" { "warn" } else { "error" },
                format!("field {field} empty"),
                false,
            ));
        }
    }

    if t.album_artist.is_none() && t.artist.is_some() {
        out.push(Issue::new(
            "missing_album_artist",
            Some("album_artist"),
            "info",
            "empty album artist; can copy from artist".into(),
            true,
        ));
    }

    let stem = Path::new(&row.file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| row.file_name.clone());
    if looks_swapped(&stem, t.title.as_deref(), t.artist.as_deref()) {
        out.push(Issue::new(
            "swapped_title_artist",
            Some("title"),
            "warn",
            "the file name suggests title and artist are swapped".into(),
            true,
        ));
    }

    for (field, raw) in [("track", &t.track_raw), ("disc", &t.disc_raw)] {
        if let Some(raw) = raw {
            if raw.contains('/') || raw.contains('\\') {
                out.push(Issue::new(
                    "pair_in_text",
                    Some(field),
                    "warn",
                    format!("{field} stored as text \"{raw}\""),
                    true,
                ));
            }
        }
    }

    if let Some(year) = &t.year {
        if normalize_year(year).is_none() {
            out.push(Issue::new(
                "bad_year",
                Some("year"),
                "warn",
                format!("year \"{year}\" has no four-digit year"),
                false,
            ));
        }
    }

    for field in TEXT_FIELDS {
        let value = match *field {
            "title" => &t.title,
            "artist" => &t.artist,
            "album_artist" => &t.album_artist,
            "album" => &t.album,
            "genre" => &t.genre,
            _ => &t.comment,
        };
        let Some(value) = value else { continue };
        if let Some(fixed) = fix_mojibake(value) {
            out.push(Issue::new(
                "mojibake",
                Some(field),
                "warn",
                format!("\"{value}\" parece ser \"{fixed}\""),
                true,
            ));
        } else if has_lost_bytes(value) {
            out.push(Issue::new(
                "lost_bytes",
                Some(field),
                "warn",
                format!("\"{value}\" lost characters when written"),
                false,
            ));
        }
    }

    if row.cover.count == 0 {
        out.push(Issue::new(
            "no_cover",
            Some("cover"),
            "info",
            "no embedded cover".into(),
            true,
        ));
    } else {
        if row.cover.bytes > COVER_HUGE_BYTES {
            out.push(Issue::new(
                "cover_huge",
                Some("cover"),
                "warn",
                format!(
                    "a {} KB cover embedded in every track",
                    row.cover.bytes / 1024
                ),
                false,
            ));
        }
        if row.cover.duplicated || row.cover.front_count > 1 {
            out.push(Issue::new(
                "cover_dupe",
                Some("cover"),
                "warn",
                format!("{} covers in the same tag", row.cover.count),
                true,
            ));
        }
    }

    if no_primary && !other_tags.is_empty() {
        let names: Vec<String> = other_tags.iter().copied().map(tag_label).collect();
        out.push(Issue::new(
            "wrong_tag_type",
            None,
            "warn",
            format!(
                "{} stores {} instead of {}",
                row.format,
                names.join(", "),
                row.primary_tag_type
            ),
            false,
        ));
    }

    out
}

// ─────────────────────────────── scan ───────────────────────────────────

fn collect_paths(opts: &ScanOptions) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    let push = |p: PathBuf, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>| {
        let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if seen.insert(canon) {
            out.push(p);
        }
    };

    for f in &opts.files {
        let p = PathBuf::from(f);
        if p.is_file() && is_audio(&p) {
            push(p, &mut out, &mut seen);
        }
    }
    for d in &opts.dirs {
        let depth = if opts.recursive { usize::MAX } else { 1 };
        let walker = WalkDir::new(d)
            .max_depth(depth)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok);
        for entry in walker {
            if entry.file_type().is_file() && is_audio(entry.path()) {
                push(entry.path().to_path_buf(), &mut out, &mut seen);
            }
        }
    }

    let mut names: Vec<String> = out
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    sort_paths_by_name(&mut names);
    let mut sorted: Vec<PathBuf> = names.into_iter().map(PathBuf::from).collect();
    if let Some(max) = opts.max_files {
        sorted.truncate(max);
    }
    sorted
}

/// Reads everything the UI asked for and returns the table with the diagnosis attached.
pub fn scan(opts: &ScanOptions, progress: &ProgressFn) -> anyhow::Result<ScanResult> {
    let paths = collect_paths(opts);
    let total = paths.len() as u64;
    report(progress, TOOL_ID, "started", 0, Some(total), None);

    let mut tracks = Vec::with_capacity(paths.len());
    let mut issue_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut failed = 0usize;
    let mut with_issues = 0usize;

    for (i, path) in paths.iter().enumerate() {
        let row = read_track(path);
        if row.error.is_some() {
            failed += 1;
        }
        if !row.issues.is_empty() {
            with_issues += 1;
        }
        for issue in &row.issues {
            *issue_counts.entry(issue.code.clone()).or_insert(0) += 1;
        }
        report(
            progress,
            TOOL_ID,
            "progress",
            i as u64 + 1,
            Some(total),
            Some(row.file_name.clone()),
        );
        tracks.push(row);
    }

    report(progress, TOOL_ID, "done", total, Some(total), None);
    Ok(ScanResult {
        scanned: tracks.len(),
        failed,
        with_issues,
        issue_counts,
        tracks,
    })
}

// ─────────────────────────── edit plan ────────────────────────────

/// Working state: field → current value, as text.
fn state_of(row: &TrackRow) -> BTreeMap<String, Option<String>> {
    let t = &row.tags;
    let num = |v: Option<u32>| v.map(|n| n.to_string());
    let mut m = BTreeMap::new();
    m.insert("title".into(), t.title.clone());
    m.insert("artist".into(), t.artist.clone());
    m.insert("album_artist".into(), t.album_artist.clone());
    m.insert("album".into(), t.album.clone());
    m.insert("year".into(), t.year.clone());
    // Track and disc go in with the raw text: it is what the user sees in the diff
    // when the file stores "3/12".
    m.insert("track".into(), t.track_raw.clone().or_else(|| num(t.track)));
    m.insert("track_total".into(), num(t.track_total));
    m.insert("disc".into(), t.disc_raw.clone().or_else(|| num(t.disc)));
    m.insert("disc_total".into(), num(t.disc_total));
    m.insert("genre".into(), t.genre.clone());
    m.insert("comment".into(), t.comment.clone());
    m
}

/// Diff of one track, without touching disk. This is the function `dry_run`
/// shows and the writer applies — both read exactly the same plan.
///
/// `position` is the number renumbering will write; `total` is the total of
/// tracks in the batch.
pub fn plan_for(row: &TrackRow, position: u32, total: u32, opts: &EditOptions) -> Vec<FieldDiff> {
    let before = state_of(row);
    let mut after = before.clone();
    let mut reasons: BTreeMap<String, String> = BTreeMap::new();
    let put = |after: &mut BTreeMap<String, Option<String>>,
               reasons: &mut BTreeMap<String, String>,
               field: &str,
               value: Option<String>,
               reason: &str| {
        after.insert(field.to_string(), value);
        reasons.insert(field.to_string(), reason.to_string());
    };

    if opts.fix_mojibake {
        for field in TEXT_FIELDS {
            let current = after.get(*field).cloned().flatten();
            if let Some(fixed) = current.as_deref().and_then(fix_mojibake) {
                put(&mut after, &mut reasons, field, Some(fixed), "mojibake");
            }
        }
    }

    if opts.swap_title_artist {
        let title = after.get("title").cloned().flatten();
        let artist = after.get("artist").cloned().flatten();
        if title.is_some() || artist.is_some() {
            put(&mut after, &mut reasons, "title", artist, "swap");
            put(&mut after, &mut reasons, "artist", title, "swap");
        }
    }

    if opts.split_track_slash {
        for (field, total_field) in [("track", "track_total"), ("disc", "disc_total")] {
            let current = after.get(field).cloned().flatten();
            let Some(raw) = current else { continue };
            if !raw.contains('/') && !raw.contains('\\') {
                continue;
            }
            let (n, tot) = parse_track_pair(&raw);
            if let Some(n) = n {
                put(
                    &mut after,
                    &mut reasons,
                    field,
                    Some(n.to_string()),
                    "split_pair",
                );
            }
            if let Some(tot) = tot {
                put(
                    &mut after,
                    &mut reasons,
                    total_field,
                    Some(tot.to_string()),
                    "split_pair",
                );
            }
        }
    }

    for (field, value) in &opts.set {
        if !FIELDS.contains(&field.as_str()) {
            continue;
        }
        let value = value.trim();
        let value = if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        };
        put(&mut after, &mut reasons, field, value, "set");
    }

    for field in &opts.clear {
        if FIELDS.contains(&field.as_str()) {
            put(&mut after, &mut reasons, field, None, "clear");
        }
    }

    if opts.album_artist_from_artist {
        let artist = after.get("artist").cloned().flatten();
        let album_artist = after.get("album_artist").cloned().flatten();
        if album_artist.is_none() && artist.is_some() {
            put(
                &mut after,
                &mut reasons,
                "album_artist",
                artist,
                "album_artist",
            );
        }
    }

    if opts.renumber {
        put(
            &mut after,
            &mut reasons,
            "track",
            Some(position.to_string()),
            "renumber",
        );
        if opts.set_track_total {
            put(
                &mut after,
                &mut reasons,
                "track_total",
                Some(total.to_string()),
                "renumber",
            );
        }
    }

    let mut diffs = Vec::new();
    for field in FIELDS {
        let from = before.get(*field).cloned().flatten();
        let to = after.get(*field).cloned().flatten();
        if from == to {
            continue;
        }
        diffs.push(FieldDiff {
            field: (*field).to_string(),
            from,
            to,
            reason: reasons
                .get(*field)
                .cloned()
                .unwrap_or_else(|| "set".to_string()),
        });
    }
    diffs
}

// ─────────────────────────────── cover ───────────────────────────────────

fn mime_of(bytes: &[u8]) -> Option<MimeType> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(MimeType::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(MimeType::Jpeg)
    } else if bytes.starts_with(b"GIF8") {
        Some(MimeType::Gif)
    } else if bytes.starts_with(b"BM") {
        Some(MimeType::Bmp)
    } else {
        None
    }
}

/// Keeps only the first image of each content (identical bytes) and at most
/// one front cover. Returns how many were removed.
fn dedupe_pictures(tag: &mut Tag) -> usize {
    let before = tag.pictures().len();
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut front_seen = false;
    let keep: Vec<bool> = tag
        .pictures()
        .iter()
        .map(|p| {
            let is_front = p.pic_type() == PictureType::CoverFront;
            if is_front && front_seen {
                return false;
            }
            if !seen.insert(p.data().to_vec()) {
                return false;
            }
            if is_front {
                front_seen = true;
            }
            true
        })
        .collect();
    for i in (0..keep.len()).rev() {
        if !keep[i] {
            let _ = tag.remove_picture(i);
        }
    }
    before - tag.pictures().len()
}

// ────────────────────────────── writing ────────────────────────────────

fn apply_diffs(tag: &mut Tag, diffs: &[FieldDiff]) -> Vec<String> {
    let mut unsupported = Vec::new();
    for diff in diffs {
        let Some(key) = item_key(&diff.field) else {
            continue;
        };
        match &diff.to {
            None => {
                tag.remove_key(key);
                // Track and disc in ID3v2 live in the same frame as the total.
                if diff.field == "track" {
                    tag.remove_key(ItemKey::TrackNumber);
                } else if diff.field == "disc" {
                    tag.remove_key(ItemKey::DiscNumber);
                }
                if diff.field == "year" {
                    tag.remove_key(ItemKey::RecordingDate);
                }
            }
            Some(value) => {
                let ok = tag.insert_text(key, value.clone());
                if !ok {
                    // ID3v2 has no "Year" — the date goes in TDRC.
                    let fallback = if diff.field == "year" {
                        tag.insert_text(ItemKey::RecordingDate, value.clone())
                    } else {
                        false
                    };
                    if !fallback {
                        unsupported.push(diff.field.clone());
                    }
                } else if diff.field == "year" {
                    // Keeps both readings consistent when the format
                    // accepts both fields.
                    let _ = tag.insert_text(ItemKey::RecordingDate, value.clone());
                }
            }
        }
    }
    unsupported
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

fn edit_one(path: &Path, diffs: &[FieldDiff], opts: &EditOptions) -> anyhow::Result<FileChange> {
    let path_str = path.to_string_lossy().to_string();
    let mut change = FileChange {
        path: path_str.clone(),
        file_name: file_name_of(&path_str),
        diffs: diffs.to_vec(),
        cover: None,
        unsupported: Vec::new(),
        written: false,
        backup: None,
        error: None,
    };

    let cover_bytes = if let Some(cover) = opts.cover_path.as_deref().filter(|s| !s.is_empty()) {
        let data =
            std::fs::read(cover).with_context(|| format!("could not read the cover at {cover}"))?;
        let mime =
            mime_of(&data).ok_or_else(|| anyhow!("cover {cover} is not PNG, JPEG, GIF or BMP"))?;
        Some((data, mime))
    } else {
        None
    };

    let mut tagged = lofty::read_from_path(path).map_err(|e| anyhow!(e.to_string()))?;
    let tag_type = tagged.primary_tag_type();
    if tagged.primary_tag_mut().is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .primary_tag_mut()
        .ok_or_else(|| anyhow!("the format does not accept tag {tag_type:?}"))?;

    let before_cover = tag.pictures().len();
    let before_bytes: u64 = tag.pictures().iter().map(|p| p.data().len() as u64).sum();

    change.unsupported = apply_diffs(tag, diffs);

    if opts.remove_cover {
        while !tag.pictures().is_empty() {
            let _ = tag.remove_picture(0);
        }
        change.cover = Some(CoverDiff {
            action: "remove".into(),
            from_bytes: before_bytes,
            to_bytes: 0,
        });
    } else if let Some((data, mime)) = cover_bytes {
        tag.remove_picture_type(PictureType::CoverFront);
        let to_bytes = data.len() as u64;
        tag.push_picture(
            Picture::unchecked(data)
                .pic_type(PictureType::CoverFront)
                .mime_type(mime)
                .build(),
        );
        change.cover = Some(CoverDiff {
            action: "embed".into(),
            from_bytes: before_bytes,
            to_bytes,
        });
    } else if opts.dedupe_cover && before_cover > 1 {
        let removed = dedupe_pictures(tag);
        if removed > 0 {
            change.cover = Some(CoverDiff {
                action: "dedupe".into(),
                from_bytes: before_bytes,
                to_bytes: tag.pictures().iter().map(|p| p.data().len() as u64).sum(),
            });
        }
    }

    let touched = !change.diffs.is_empty() || change.cover.is_some();
    if opts.dry_run || !touched {
        return Ok(change);
    }

    if opts.backup {
        let dest = backup_path(path);
        std::fs::copy(path, &dest)
            .with_context(|| format!("could not save the copy to {}", dest.display()))?;
        change.backup = Some(dest.to_string_lossy().to_string());
    }
    tagged
        .save_to_path(path, LoftyWriteOptions::default())
        .map_err(|e| anyhow!(e.to_string()))?;
    change.written = true;
    Ok(change)
}

/// Applies (or just simulates) the batch edit. With `dry_run` — the default —
/// no byte of the file changes: the return is just the diff.
pub fn edit(opts: &EditOptions, progress: &ProgressFn) -> anyhow::Result<EditResult> {
    let mut files = opts.files.clone();
    if opts.sort_by_name {
        sort_paths_by_name(&mut files);
    }
    let total = files.len() as u64;
    report(progress, TOOL_ID, "started", 0, Some(total), None);

    let mut out = Vec::with_capacity(files.len());
    let mut changed = 0usize;
    let mut written = 0usize;
    let mut failed = 0usize;

    for (i, file) in files.iter().enumerate() {
        let path = PathBuf::from(file);
        let row = read_track(&path);
        let position = opts.renumber_start + i as u32;
        let diffs = plan_for(&row, position, files.len() as u32, opts);

        let change = if let Some(err) = row.error.clone() {
            FileChange {
                path: file.clone(),
                file_name: file_name_of(file),
                diffs: Vec::new(),
                cover: None,
                unsupported: Vec::new(),
                written: false,
                backup: None,
                error: Some(err),
            }
        } else {
            match edit_one(&path, &diffs, opts) {
                Ok(c) => c,
                Err(e) => FileChange {
                    path: file.clone(),
                    file_name: file_name_of(file),
                    diffs: diffs.clone(),
                    cover: None,
                    unsupported: Vec::new(),
                    written: false,
                    backup: None,
                    error: Some(e.to_string()),
                },
            }
        };

        if change.error.is_some() {
            failed += 1;
        } else {
            if !change.diffs.is_empty() || change.cover.is_some() {
                changed += 1;
            }
            if change.written {
                written += 1;
            }
        }
        report(
            progress,
            TOOL_ID,
            "progress",
            i as u64 + 1,
            Some(total),
            Some(change.file_name.clone()),
        );
        out.push(change);
    }

    report(progress, TOOL_ID, "done", total, Some(total), None);
    Ok(EditResult {
        dry_run: opts.dry_run,
        changed,
        written,
        failed,
        files: out,
    })
}

/// A file's cover, as a `data:` URL, for the UI to show the thumbnail.
pub fn cover_data_url(path: &str) -> anyhow::Result<Option<String>> {
    use base64::Engine as _;
    let tagged = lofty::read_from_path(path).map_err(|e| anyhow!(e.to_string()))?;
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return Ok(None);
    };
    let pic = tag
        .get_picture_type(PictureType::CoverFront)
        .or_else(|| tag.pictures().first());
    let Some(pic) = pic else { return Ok(None) };
    let mime = pic
        .mime_type()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "image/jpeg".to_string());
    let b64 = base64::engine::general_purpose::STANDARD.encode(pic.data());
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

// ─────────────────────────────── tests ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::prelude::TagExt;

    /// A minimal valid FLAC: marker + one 1-second STREAMINFO block and
    /// no audio frames. Enough for lofty to open it, read the
    /// properties and write Vorbis Comments — no binary in the repo.
    fn minimal_flac() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"fLaC");
        v.push(0x80); // last metadata block, type 0 (STREAMINFO)
        v.extend_from_slice(&[0x00, 0x00, 0x22]); // 34 bytes
        v.extend_from_slice(&4096u16.to_be_bytes()); // minimum blocksize
        v.extend_from_slice(&4096u16.to_be_bytes()); // maximum blocksize
        v.extend_from_slice(&[0, 0, 0]); // minimum framesize
        v.extend_from_slice(&[0, 0, 0]); // maximum framesize
                                         // 20 bits de taxa, 3 de canais (valor-1), 5 de bits (valor-1),
                                         // 36 de total de amostras.
        let packed: u64 = (44_100u64 << 44) | (1u64 << 41) | (15u64 << 36) | 44_100u64;
        v.extend_from_slice(&packed.to_be_bytes());
        v.extend_from_slice(&[0u8; 16]); // audio MD5: zeroed = unknown
        v
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gumballz-get-audio-tag-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test tempdir");
        dir
    }

    fn write_flac(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, minimal_flac()).expect("write the test flac");
        p
    }

    fn row_with(tags: TrackTags, file_name: &str) -> TrackRow {
        TrackRow {
            path: format!("/musica/{file_name}"),
            file_name: file_name.to_string(),
            format: "FLAC".into(),
            tag_type: Some("VorbisComments".into()),
            primary_tag_type: "VorbisComments".into(),
            bytes: 1024,
            duration_secs: 1.0,
            bitrate_kbps: Some(900),
            sample_rate: Some(44_100),
            channels: Some(2),
            tags,
            cover: empty_cover(),
            issues: Vec::new(),
            error: None,
        }
    }

    // ── mojibake ──

    #[test]
    fn mojibake_conserta_utf8_lido_como_latin1() {
        assert_eq!(fix_mojibake("MÃºsica").as_deref(), Some("Música"));
        assert_eq!(fix_mojibake("CoraÃ§Ã£o").as_deref(), Some("Coração"));
        assert_eq!(fix_mojibake("BeyoncÃ©").as_deref(), Some("Beyoncé"));
        // Typographic quotes go through the CP1252 C1 range.
        assert_eq!(fix_mojibake("Donâ€™t").as_deref(), Some("Don’t"));
    }

    #[test]
    fn mojibake_nao_mexe_em_texto_certo() {
        assert!(fix_mojibake("Música").is_none());
        assert!(fix_mojibake("Coração").is_none());
        assert!(fix_mojibake("Radiohead").is_none());
        assert!(fix_mojibake("").is_none());
        assert!(fix_mojibake("日本語").is_none());
    }

    #[test]
    fn bytes_perdidos_sao_detectados_mas_nao_consertados() {
        assert!(has_lost_bytes("Mus\u{FFFD}ica"));
        assert!(!has_lost_bytes("Musica"));
    }

    // ── track "3/12" ──

    #[test]
    fn parse_de_faixa_com_total() {
        assert_eq!(parse_track_pair("3/12"), (Some(3), Some(12)));
        assert_eq!(parse_track_pair("03/12"), (Some(3), Some(12)));
        assert_eq!(parse_track_pair(" 3 of 12 "), (Some(3), Some(12)));
        assert_eq!(parse_track_pair("3-12"), (Some(3), Some(12)));
        assert_eq!(parse_track_pair("7"), (Some(7), None));
        assert_eq!(parse_track_pair("A1"), (None, None));
        assert_eq!(parse_track_pair(""), (None, None));
    }

    #[test]
    fn ano_valido_ou_nao() {
        assert_eq!(normalize_year("1998"), Some(1998));
        assert_eq!(normalize_year("1998-05-03"), Some(1998));
        assert_eq!(normalize_year("2001-01-01T00:00:00"), Some(2001));
        assert_eq!(normalize_year("03/05/1998"), Some(1998));
        assert_eq!(normalize_year("98"), None);
        assert_eq!(normalize_year("0000"), None);
        assert_eq!(normalize_year("sem ano"), None);
    }

    // ── swapped title/artist ──

    #[test]
    fn detecta_titulo_e_artista_trocados() {
        assert!(looks_swapped(
            "Radiohead - Creep",
            Some("Radiohead"),
            Some("Creep")
        ));
        assert!(looks_swapped(
            "01 - Radiohead - Creep",
            Some("Radiohead"),
            Some("Creep")
        ));
        // Right: the name says artist on the left and the tag agrees.
        assert!(!looks_swapped(
            "Radiohead - Creep",
            Some("Creep"),
            Some("Radiohead")
        ));
        assert!(!looks_swapped("faixa01", Some("Creep"), Some("Radiohead")));
        assert!(!looks_swapped("Radiohead - Creep", None, Some("Radiohead")));
    }

    // ── natural order ──

    #[test]
    fn ordena_por_nome_em_ordem_natural() {
        let mut v = vec![
            "/m/10 dez.flac".to_string(),
            "/m/2 dois.flac".to_string(),
            "/m/1 um.flac".to_string(),
        ];
        sort_paths_by_name(&mut v);
        assert_eq!(
            v,
            vec![
                "/m/1 um.flac".to_string(),
                "/m/2 dois.flac".to_string(),
                "/m/10 dez.flac".to_string()
            ]
        );
    }

    #[test]
    fn renumeracao_segue_a_ordem_de_nome() {
        let opts = EditOptions {
            renumber: true,
            set_track_total: true,
            ..Default::default()
        };
        let rows = [
            row_with(TrackTags::default(), "01 a.flac"),
            row_with(TrackTags::default(), "02 b.flac"),
            row_with(TrackTags::default(), "03 c.flac"),
        ];
        for (i, row) in rows.iter().enumerate() {
            let diffs = plan_for(row, i as u32 + 1, 3, &opts);
            let track = diffs.iter().find(|d| d.field == "track").expect("track");
            assert_eq!(track.to.as_deref(), Some((i + 1).to_string().as_str()));
            assert_eq!(track.reason, "renumber");
            let total = diffs
                .iter()
                .find(|d| d.field == "track_total")
                .expect("total");
            assert_eq!(total.to.as_deref(), Some("3"));
        }
    }

    // ── dry_run diff ──

    #[test]
    fn diff_mostra_so_o_que_muda() {
        let mut tags = TrackTags {
            title: Some("Creep".into()),
            artist: Some("Radiohead".into()),
            ..Default::default()
        };
        tags.album = Some("Pablo Honey".into());
        let row = row_with(tags, "01 - Radiohead - Creep.flac");

        let mut set = BTreeMap::new();
        // Same value: it must not become a diff.
        set.insert("album".to_string(), "Pablo Honey".to_string());
        set.insert("genre".to_string(), "Rock".to_string());
        let opts = EditOptions {
            set,
            ..Default::default()
        };
        let diffs = plan_for(&row, 1, 1, &opts);
        assert_eq!(diffs.len(), 1, "only the genre changed: {diffs:?}");
        assert_eq!(diffs[0].field, "genre");
        assert_eq!(diffs[0].from, None);
        assert_eq!(diffs[0].to.as_deref(), Some("Rock"));
    }

    #[test]
    fn diff_quebra_faixa_em_texto_e_copia_artista_do_album() {
        let tags = TrackTags {
            title: Some("Creep".into()),
            artist: Some("Radiohead".into()),
            track_raw: Some("3/12".into()),
            ..Default::default()
        };
        let row = row_with(tags, "03 - Radiohead - Creep.flac");
        let opts = EditOptions {
            split_track_slash: true,
            album_artist_from_artist: true,
            ..Default::default()
        };
        let diffs = plan_for(&row, 3, 12, &opts);
        let by = |f: &str| diffs.iter().find(|d| d.field == f).cloned();

        let track = by("track").expect("track");
        assert_eq!(track.from.as_deref(), Some("3/12"));
        assert_eq!(track.to.as_deref(), Some("3"));
        assert_eq!(track.reason, "split_pair");
        assert_eq!(by("track_total").and_then(|d| d.to).as_deref(), Some("12"));
        assert_eq!(
            by("album_artist").and_then(|d| d.to).as_deref(),
            Some("Radiohead")
        );
    }

    #[test]
    fn diff_limpa_campo_e_troca_titulo_com_artista() {
        let tags = TrackTags {
            title: Some("Radiohead".into()),
            artist: Some("Creep".into()),
            comment: Some("rip por alguem".into()),
            ..Default::default()
        };
        let row = row_with(tags, "Radiohead - Creep.flac");
        let opts = EditOptions {
            swap_title_artist: true,
            clear: vec!["comment".into()],
            ..Default::default()
        };
        let diffs = plan_for(&row, 1, 1, &opts);
        let by = |f: &str| diffs.iter().find(|d| d.field == f).cloned();
        assert_eq!(by("title").and_then(|d| d.to).as_deref(), Some("Creep"));
        assert_eq!(
            by("artist").and_then(|d| d.to).as_deref(),
            Some("Radiohead")
        );
        let comment = by("comment").expect("comment");
        assert_eq!(comment.to, None);
        assert_eq!(comment.reason, "clear");
    }

    #[test]
    fn diff_conserta_mojibake_dos_campos_de_texto() {
        let tags = TrackTags {
            title: Some("CoraÃ§Ã£o".into()),
            artist: Some("Radiohead".into()),
            ..Default::default()
        };
        let row = row_with(tags, "a.flac");
        let opts = EditOptions {
            fix_mojibake: true,
            ..Default::default()
        };
        let diffs = plan_for(&row, 1, 1, &opts);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].to.as_deref(), Some("Coração"));
        assert_eq!(diffs[0].reason, "mojibake");
    }

    // ── diagnosis ──

    #[test]
    fn diagnostico_lista_os_problemas_da_faixa() {
        let tags = TrackTags {
            title: Some("Radiohead".into()),
            artist: Some("Creep".into()),
            year: Some("98".into()),
            track_raw: Some("3/12".into()),
            ..Default::default()
        };
        let row = row_with(tags, "Radiohead - Creep.flac");
        let issues = diagnose(&row, false, &[]);
        let codes: Vec<&str> = issues.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains(&"missing_field"), "{codes:?}"); // album
        assert!(codes.contains(&"swapped_title_artist"), "{codes:?}");
        assert!(codes.contains(&"pair_in_text"), "{codes:?}");
        assert!(codes.contains(&"bad_year"), "{codes:?}");
        assert!(codes.contains(&"no_cover"), "{codes:?}");
        assert!(codes.contains(&"missing_album_artist"), "{codes:?}");
    }

    #[test]
    fn diagnostico_acusa_tag_estrangeira() {
        let row = row_with(TrackTags::default(), "a.flac");
        let issues = diagnose(&row, true, &[TagType::Id3v2]);
        assert!(issues.iter().any(|i| i.code == "wrong_tag_type"));
    }

    // ── round trip via lofty, on a hand-built FLAC ──

    #[test]
    fn le_um_flac_montado_a_mao() {
        let dir = temp_dir("read");
        let p = write_flac(&dir, "01 vazio.flac");
        let row = read_track(&p);
        assert!(row.error.is_none(), "erro: {:?}", row.error);
        assert_eq!(row.format, "FLAC");
        assert_eq!(row.sample_rate, Some(44_100));
        assert!((row.duration_secs - 1.0).abs() < 0.05, "{row:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_diagnostica_a_pasta_inteira() {
        let dir = temp_dir("scan");
        write_flac(&dir, "01 a.flac");
        write_flac(&dir, "02 b.flac");
        std::fs::write(dir.join("leia-me.txt"), b"nao e audio").expect("write txt");
        let opts = ScanOptions {
            dirs: vec![dir.to_string_lossy().to_string()],
            ..Default::default()
        };
        let out = scan(&opts, &super::super::noop_progress()).expect("scan");
        assert_eq!(out.scanned, 2, "the .txt cannot get in");
        assert_eq!(out.failed, 0);
        // File with no tag at all.
        assert!(out.tracks.iter().all(|t| t.tag_type.is_none()));
        assert!(out.issue_counts.contains_key("no_tag"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dry_run_nao_toca_no_arquivo_e_a_gravacao_toca() {
        let dir = temp_dir("write");
        let p = write_flac(&dir, "01 faixa.flac");
        let path = p.to_string_lossy().to_string();
        let antes = std::fs::read(&p).expect("ler antes");

        let mut set = BTreeMap::new();
        set.insert("title".to_string(), "Coração".to_string());
        set.insert("artist".to_string(), "Radiohead".to_string());
        let mut opts = EditOptions {
            files: vec![path.clone()],
            set,
            renumber: true,
            set_track_total: true,
            album_artist_from_artist: true,
            ..Default::default()
        };

        let seco = edit(&opts, &super::super::noop_progress()).expect("dry run");
        assert!(seco.dry_run);
        assert_eq!(seco.written, 0);
        assert_eq!(seco.changed, 1);
        assert_eq!(
            std::fs::read(&p).expect("read after the dry run"),
            antes,
            "dry_run cannot write a single byte"
        );
        let campos: Vec<&str> = seco.files[0]
            .diffs
            .iter()
            .map(|d| d.field.as_str())
            .collect();
        assert!(campos.contains(&"title"), "{campos:?}");
        assert!(campos.contains(&"album_artist"), "{campos:?}");
        assert!(campos.contains(&"track"), "{campos:?}");

        opts.dry_run = false;
        let gravado = edit(&opts, &super::super::noop_progress()).expect("write");
        assert_eq!(gravado.written, 1, "{:?}", gravado.files[0].error);

        let row = read_track(&p);
        assert_eq!(row.tags.title.as_deref(), Some("Coração"));
        assert_eq!(row.tags.artist.as_deref(), Some("Radiohead"));
        assert_eq!(row.tags.album_artist.as_deref(), Some("Radiohead"));
        assert_eq!(row.tags.track, Some(1));
        assert_eq!(row.tags.track_total, Some(1));

        // Running again with the same plan has nothing left to change.
        let repetido = edit(&opts, &super::super::noop_progress()).expect("second pass");
        let restantes: Vec<&str> = repetido.files[0]
            .diffs
            .iter()
            .map(|d| d.field.as_str())
            .collect();
        assert!(
            !restantes.contains(&"title"),
            "title is already right: {restantes:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capa_entra_sai_e_nao_duplica() {
        let dir = temp_dir("cover");
        let p = write_flac(&dir, "01 capa.flac");
        let path = p.to_string_lossy().to_string();
        // Minimal 1x1 PNG.
        let png: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let cover = dir.join("capa.png");
        std::fs::write(&cover, &png).expect("gravar png");

        let opts = EditOptions {
            files: vec![path.clone()],
            cover_path: Some(cover.to_string_lossy().to_string()),
            dry_run: false,
            ..Default::default()
        };
        let r = edit(&opts, &super::super::noop_progress()).expect("embutir capa");
        assert_eq!(r.written, 1, "{:?}", r.files[0].error);
        let row = read_track(&p);
        assert_eq!(row.cover.count, 1);
        assert_eq!(row.cover.mime.as_deref(), Some("image/png"));

        // Embedding again swaps the front cover instead of stacking another one.
        let r = edit(&opts, &super::super::noop_progress()).expect("embed again");
        assert_eq!(r.written, 1);
        assert_eq!(read_track(&p).cover.count, 1);

        let fora = EditOptions {
            files: vec![path],
            remove_cover: true,
            dry_run: false,
            ..Default::default()
        };
        edit(&fora, &super::super::noop_progress()).expect("tirar capa");
        assert_eq!(read_track(&p).cover.count, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedupe_deixa_uma_capa_so() {
        let mut tag = Tag::new(TagType::VorbisComments);
        let data = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        for _ in 0..3 {
            tag.push_picture(
                Picture::unchecked(data.clone())
                    .pic_type(PictureType::CoverFront)
                    .mime_type(MimeType::Png)
                    .build(),
            );
        }
        let removidas = dedupe_pictures(&mut tag);
        assert_eq!(removidas, 2);
        assert_eq!(tag.pictures().len(), 1);
        assert!(!tag.is_empty() || tag.pictures().len() == 1);
    }

    #[test]
    fn backup_e_gravado_antes_de_escrever() {
        let dir = temp_dir("backup");
        let p = write_flac(&dir, "01 bkp.flac");
        let original = std::fs::read(&p).expect("ler original");
        let mut set = BTreeMap::new();
        set.insert("title".to_string(), "Teste".to_string());
        let opts = EditOptions {
            files: vec![p.to_string_lossy().to_string()],
            set,
            backup: true,
            dry_run: false,
            ..Default::default()
        };
        let r = edit(&opts, &super::super::noop_progress()).expect("gravar com backup");
        let bkp = r.files[0].backup.clone().expect("caminho do backup");
        assert_eq!(std::fs::read(&bkp).expect("ler backup"), original);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
