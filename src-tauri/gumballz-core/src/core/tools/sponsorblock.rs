//! SponsorBlock (study 43): segments by hash prefix, as the extension
//! does so it does not send the video ID to the server.

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SERVER: &str = "https://sponsor.ajay.app";
pub const CATEGORIES: &[&str] = &[
    "sponsor",
    "selfpromo",
    "interaction",
    "intro",
    "outro",
    "preview",
    "music_offtopic",
    "filler",
    "exclusive_access",
    "poi_highlight",
    "chapter",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    pub segment: [f64; 2],
    pub category: String,
    #[serde(rename = "actionType", default)]
    pub action_type: String,
    #[serde(default)]
    pub votes: i64,
    #[serde(default)]
    pub locked: i64,
    #[serde(rename = "videoDuration", default)]
    pub video_duration: f64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SponsorResult {
    pub video_id: String,
    pub segments: Vec<Segment>,
    pub skipped_seconds: f64,
    /// Equivalent yt-dlp arguments to download without the segments.
    pub ytdlp_args: String,
}

/// Accepts a full URL, `youtu.be/ID`, `shorts/ID` or the ID itself.
pub fn video_id(input: &str) -> Option<String> {
    let s = input.trim();
    if s.len() == 11
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(s.to_string());
    }
    let re =
        regex::Regex::new(r"(?:v=|youtu\.be/|shorts/|embed/|live/)([A-Za-z0-9_-]{11})").ok()?;
    re.captures(s)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

pub async fn segments(input: &str, categories: &[String]) -> anyhow::Result<SponsorResult> {
    let id = video_id(input)
        .ok_or_else(|| anyhow!("did not recognise a YouTube video in: {}", input))?;
    let cats: Vec<&str> = if categories.is_empty() {
        CATEGORIES.to_vec()
    } else {
        categories.iter().map(|s| s.as_str()).collect()
    };
    let mut h = Sha256::new();
    h.update(id.as_bytes());
    let prefix = &hex::encode(h.finalize())[..4];
    let url = format!(
        "{}/api/skipSegments/{}?categories={}&actionTypes={}",
        SERVER,
        prefix,
        urlencoding::encode(&serde_json::to_string(&cats)?),
        urlencoding::encode(r#"["skip","mute","full","poi","chapter"]"#)
    );
    let client = super::client()?;
    let resp = client.get(&url).send().await?;
    let mut segs: Vec<Segment> = Vec::new();
    if resp.status().as_u16() == 404 {
        // no segments for this prefix
    } else if resp.status().is_success() {
        let arr: Vec<serde_json::Value> = resp.json().await?;
        for v in arr {
            if v["videoID"].as_str() == Some(id.as_str()) {
                if let Some(list) = v["segments"].as_array() {
                    segs.extend(
                        list.iter()
                            .filter_map(|s| serde_json::from_value(s.clone()).ok()),
                    );
                }
            }
        }
    } else {
        return Err(anyhow!("SponsorBlock: HTTP {}", resp.status()));
    }
    segs.sort_by(|a, b| {
        a.segment[0]
            .partial_cmp(&b.segment[0])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let skipped: f64 = segs
        .iter()
        .filter(|s| s.action_type == "skip" || s.action_type.is_empty())
        .map(|s| (s.segment[1] - s.segment[0]).max(0.0))
        .sum();
    let mut used: Vec<&str> = segs
        .iter()
        .filter(|s| s.action_type != "chapter" && s.action_type != "poi")
        .map(|s| s.category.as_str())
        .collect();
    used.sort();
    used.dedup();
    let ytdlp_args = if used.is_empty() {
        String::new()
    } else {
        format!("--sponsorblock-remove {}", used.join(","))
    };
    Ok(SponsorResult {
        video_id: id,
        segments: segs,
        skipped_seconds: skipped,
        ytdlp_args,
    })
}

// ── Envio de segmentos ─────────────────────────────────────────────────
//
// SponsorBlockServer is AGPL: none of it comes in here. What exists is the public
// API `POST /api/skipSegments`, the same one the extension uses.
//
// The `userID` is a random private key stored on the machine, and nothing else. The
// server derives the public identifier from it — which is why it must never be
// born from the name, the email or the hardware of whoever uses the app.

pub const USER_AGENT_TAG: &str = concat!("GumballZ Get/", env!("CARGO_PKG_VERSION"));
pub const USER_FILE: &str = "sponsorblock-user.txt";
/// One submission at a time; above that the server returns a rate limit.
pub const MAX_SEGMENTS_PER_SUBMIT: usize = 10;
pub const ACTION_TYPES: &[&str] = &["skip", "mute", "full", "poi", "chapter"];

/// A new private key: 256 bits of randomness in hexadecimal. It derives from
/// nothing on the machine or the account.
pub fn generate_private_id() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// SHA-256 of the private key, only so the UI can show a stable identifier
/// without displaying the key. The real public identifier is derived by
/// the SponsorBlock server, not by here.
pub fn public_fingerprint(private: &str) -> String {
    let mut h = Sha256::new();
    h.update(private.as_bytes());
    hex::encode(h.finalize())
}

fn user_file() -> anyhow::Result<std::path::PathBuf> {
    let dir = super::tools_dir().ok_or_else(|| anyhow!("could not find the app data folder"))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(USER_FILE))
}

/// The machine's private key, created the first time and reused afterwards.
pub fn local_user_id() -> anyhow::Result<String> {
    let path = user_file()?;
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if existing.len() >= 30 {
            return Ok(existing);
        }
    }
    let fresh = generate_private_id();
    std::fs::write(&path, fresh.as_bytes())?;
    Ok(fresh)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct NewSegment {
    pub start: f64,
    pub end: f64,
    pub category: String,
    #[serde(default = "default_action")]
    pub action_type: String,
    /// Only the chapter uses it: it is the title that appears on the bar.
    #[serde(default)]
    pub description: String,
}

fn default_action() -> String {
    "skip".to_string()
}

/// Refuses what the server would refuse, with the message in English and before
/// spending a submission (the rate limit is per account, not per valid attempt).
pub fn validate_segments(segments: &[NewSegment], duration: f64) -> anyhow::Result<()> {
    if segments.is_empty() {
        return Err(anyhow!("mark at least one segment before submitting"));
    }
    if segments.len() > MAX_SEGMENTS_PER_SUBMIT {
        return Err(anyhow!(
            "at most {} segments per submission",
            MAX_SEGMENTS_PER_SUBMIT
        ));
    }
    for (i, s) in segments.iter().enumerate() {
        let n = i + 1;
        if !CATEGORIES.contains(&s.category.as_str()) {
            return Err(anyhow!(
                "segment {}: unknown category ({})",
                n,
                s.category
            ));
        }
        if !ACTION_TYPES.contains(&s.action_type.as_str()) {
            return Err(anyhow!(
                "segment {}: unknown action type ({})",
                n,
                s.action_type
            ));
        }
        if !s.start.is_finite() || !s.end.is_finite() || s.start < 0.0 || s.end < 0.0 {
            return Err(anyhow!("segment {}: invalid time", n));
        }
        let pontual = s.action_type == "poi" || s.action_type == "full";
        if !pontual && s.end <= s.start {
            return Err(anyhow!("segment {}: the end must come after the start", n));
        }
        if s.action_type == "chapter" && s.description.trim().is_empty() {
            return Err(anyhow!("segment {}: chapter needs a title", n));
        }
        if duration > 0.0 && s.end > duration + 1.0 {
            return Err(anyhow!("segment {}: past the end of the video", n));
        }
    }
    // Overlap only matters between segments that skip a chunk.
    let mut cortes: Vec<&NewSegment> = segments
        .iter()
        .filter(|s| s.action_type == "skip" || s.action_type == "mute")
        .collect();
    cortes.sort_by(|a, b| a.start.total_cmp(&b.start));
    for par in cortes.windows(2) {
        if par[1].start < par[0].end {
            return Err(anyhow!("two segments overlap at {:.1}s", par[1].start));
        }
    }
    Ok(())
}

/// The exact body of `POST /api/skipSegments`.
pub fn build_payload(
    video_id: &str,
    user_id: &str,
    duration: f64,
    segments: &[NewSegment],
) -> anyhow::Result<serde_json::Value> {
    if video_id.len() != 11 {
        return Err(anyhow!("did not recognize the YouTube video"));
    }
    if user_id.trim().len() < 30 {
        return Err(anyhow!("the local SponsorBlock key is corrupted"));
    }
    validate_segments(segments, duration)?;
    let segs: Vec<serde_json::Value> = segments
        .iter()
        .map(|s| {
            let mut o = serde_json::json!({
                "segment": [s.start, s.end],
                "category": s.category,
                "actionType": s.action_type,
            });
            if !s.description.trim().is_empty() {
                o["description"] = serde_json::Value::String(s.description.trim().to_string());
            }
            o
        })
        .collect();
    let mut body = serde_json::json!({
        "videoID": video_id,
        "userID": user_id.trim(),
        "userAgent": USER_AGENT_TAG,
        "segments": segs,
    });
    if duration > 0.0 {
        body["videoDuration"] = serde_json::json!(duration);
    }
    Ok(body)
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitOptions {
    pub url: String,
    pub segments: Vec<NewSegment>,
    #[serde(default)]
    pub video_duration: f64,
    /// The UI must check this after the user confirms: the submission is
    /// public and changes the video for everyone.
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmitResult {
    pub video_id: String,
    pub accepted: usize,
    pub uuids: Vec<String>,
    pub fingerprint: String,
    pub message: String,
}

/// Translates what the server returns into something readable on screen.
pub fn submit_error(status: u16, body: &str) -> String {
    let corpo = body.trim();
    let curto = if corpo.len() > 200 {
        &corpo[..200]
    } else {
        corpo
    };
    match status {
        400 => format!("SponsorBlock refused the submission: {}", curto),
        403 => format!("submission blocked by SponsorBlock: {}", curto),
        409 => "this segment had already been submitted".to_string(),
        429 => "too many submissions in a row; wait a few minutes and try again".to_string(),
        _ => format!("SponsorBlock: HTTP {} {}", status, curto),
    }
}

pub async fn submit(opts: SubmitOptions) -> anyhow::Result<SubmitResult> {
    if !opts.confirmed {
        return Err(anyhow!(
            "the submission is public and applies to everyone: confirm first"
        ));
    }
    let id = video_id(&opts.url)
        .ok_or_else(|| anyhow!("did not recognise a YouTube video in: {}", opts.url))?;
    let user = local_user_id()?;
    let body = build_payload(&id, &user, opts.video_duration, &opts.segments)?;

    let client = super::client()?;
    let resp = client
        .post(format!("{}/api/skipSegments", SERVER))
        .json(&body)
        .send()
        .await?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(anyhow!("{}", submit_error(status, &text)));
    }
    let uuids: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&text)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["UUID"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Ok(SubmitResult {
        accepted: if uuids.is_empty() {
            opts.segments.len()
        } else {
            uuids.len()
        },
        video_id: id,
        uuids,
        fingerprint: public_fingerprint(&user),
        message: text.trim().chars().take(200).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::video_id;

    #[test]
    fn extracts_ids() {
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(video_id("dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://example.com"), None);
    }
}

#[cfg(test)]
mod submit_tests {
    use super::*;

    fn seg(start: f64, end: f64, cat: &str) -> NewSegment {
        NewSegment {
            start,
            end,
            category: cat.to_string(),
            action_type: "skip".to_string(),
            description: String::new(),
        }
    }

    #[test]
    fn the_private_key_is_random_hex_and_leaks_nothing() {
        let a = generate_private_id();
        let b = generate_private_id();
        assert_eq!(a.len(), 64, "256 bits em hexadecimal");
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b, "each key is born on its own");
        let baixo = a.to_lowercase();
        for var in ["USER", "USERNAME", "LOGNAME", "HOME", "HOSTNAME"] {
            if let Ok(v) = std::env::var(var) {
                let v = v.trim().to_lowercase();
                if v.len() >= 3 {
                    assert!(
                        !baixo.contains(&v),
                        "the key cannot load {} from the machine",
                        var
                    );
                }
            }
        }
    }

    #[test]
    fn the_fingerprint_is_a_stable_sha256() {
        assert_eq!(
            public_fingerprint("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let k = generate_private_id();
        assert_eq!(public_fingerprint(&k), public_fingerprint(&k));
        assert_ne!(public_fingerprint(&k), k, "the fingerprint does not return the key");
    }

    #[test]
    fn the_payload_is_what_the_api_expects() {
        let chave = "0".repeat(64);
        let body = build_payload("dQw4w9WgXcQ", &chave, 212.0, &[seg(30.0, 45.5, "sponsor")])
            .expect("valid payload");
        assert_eq!(body["videoID"], "dQw4w9WgXcQ");
        assert_eq!(body["userID"], chave.as_str());
        assert_eq!(body["videoDuration"], 212.0);
        assert!(body["userAgent"]
            .as_str()
            .unwrap_or_default()
            .starts_with("GumballZ Get/"));
        let s = &body["segments"][0];
        assert_eq!(s["segment"][0], 30.0);
        assert_eq!(s["segment"][1], 45.5);
        assert_eq!(s["category"], "sponsor");
        assert_eq!(s["actionType"], "skip");
        assert!(s.get("description").is_none(), "empty description does not go");
    }

    #[test]
    fn the_payload_carries_nothing_that_identifies_the_person() {
        let chave = "a".repeat(64);
        let body = build_payload("dQw4w9WgXcQ", &chave, 0.0, &[seg(1.0, 2.0, "intro")])
            .expect("valid payload");
        let obj = body.as_object().expect("objeto");
        let mut chaves: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        chaves.sort_unstable();
        assert_eq!(
            chaves,
            vec!["segments", "userAgent", "userID", "videoID"],
            "no field beyond the four of the protocol"
        );
        assert!(
            body.get("videoDuration").is_none(),
            "duration 0 does not go along"
        );
        let texto = body.to_string();
        for var in ["USER", "USERNAME", "LOGNAME", "HOSTNAME"] {
            if let Ok(v) = std::env::var(var) {
                if v.trim().len() >= 3 {
                    assert!(!texto.contains(v.trim()), "the body leaked {}", var);
                }
            }
        }
    }

    #[test]
    fn a_chapter_needs_a_title() {
        let mut s = seg(10.0, 20.0, "chapter");
        s.action_type = "chapter".into();
        assert!(validate_segments(&[s.clone()], 100.0).is_err());
        s.description = "Receita".into();
        assert!(validate_segments(&[s.clone()], 100.0).is_ok());
        let body =
            build_payload("dQw4w9WgXcQ", &"1".repeat(64), 100.0, &[s]).expect("valid payload");
        assert_eq!(body["segments"][0]["description"], "Receita");
    }

    #[test]
    fn bad_segments_are_refused_before_the_network() {
        assert!(validate_segments(&[], 100.0).is_err(), "empty list");
        assert!(
            validate_segments(&[seg(50.0, 20.0, "sponsor")], 100.0).is_err(),
            "end before start"
        );
        assert!(
            validate_segments(&[seg(10.0, 20.0, "propaganda")], 100.0).is_err(),
            "made-up category"
        );
        assert!(
            validate_segments(&[seg(10.0, 500.0, "sponsor")], 100.0).is_err(),
            "past the end of the video"
        );
        assert!(
            validate_segments(
                &[seg(10.0, 30.0, "sponsor"), seg(20.0, 40.0, "intro")],
                100.0
            )
            .is_err(),
            "overlap"
        );
        let muitos: Vec<NewSegment> = (0..MAX_SEGMENTS_PER_SUBMIT + 1)
            .map(|i| seg(i as f64 * 10.0, i as f64 * 10.0 + 5.0, "sponsor"))
            .collect();
        assert!(
            validate_segments(&muitos, 1000.0).is_err(),
            "submission too large"
        );
    }

    #[test]
    fn a_point_of_interest_may_have_no_length() {
        let mut s = seg(42.0, 42.0, "poi_highlight");
        s.action_type = "poi".into();
        assert!(validate_segments(&[s], 100.0).is_ok());
    }

    #[test]
    fn a_broken_video_or_key_never_becomes_a_payload() {
        assert!(build_payload("curto", &"0".repeat(64), 10.0, &[seg(1.0, 2.0, "intro")]).is_err());
        assert!(build_payload("dQw4w9WgXcQ", "curta", 10.0, &[seg(1.0, 2.0, "intro")]).is_err());
    }

    #[test]
    fn server_errors_become_readable_messages() {
        assert!(submit_error(429, "").contains("espere"));
        assert!(submit_error(409, "").contains("had already been"));
        assert!(submit_error(403, "banned").contains("bloqueado"));
        assert!(submit_error(400, "Duplicate segment").contains("Duplicate segment"));
        assert!(submit_error(500, "boom").contains("500"));
        let enorme = "x".repeat(1000);
        assert!(
            submit_error(400, &enorme).len() < 300,
            "message cannot be a wall"
        );
    }

    #[tokio::test]
    async fn nothing_is_sent_without_an_explicit_confirmation() {
        let e = submit(SubmitOptions {
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            segments: vec![seg(10.0, 20.0, "sponsor")],
            video_duration: 100.0,
            confirmed: false,
        })
        .await
        .expect_err("without confirmation it does not go out");
        assert!(e.to_string().contains("confirme"));
    }
}
