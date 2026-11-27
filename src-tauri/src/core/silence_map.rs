//! Silence map of a lesson, for the player to skip over during playback.
//!
//! B36 cuts silence by transforming the file, and so pays two prices:
//! `silenceremove` preserves `stop_duration` of each stretch so it does not
//! clip the attack of speech, which left 1.4% of residue in the B53 measurement;
//! and since it is an audio filter, the output has to be audio, or the video
//! desyncs.
//!
//! Skipping during playback, both problems disappear. The padding becomes a
//! playback decision — adjustable and reversible — instead of an irreversible
//! cut, and the video stays a video. **This is not B36 in another layer: it is
//! the version that reaches the number B36 does not reach.**
//!
//! The map is computed once by the probe that already exists
//! (`silence_probe_args`) and kept as lesson metadata. Nothing is reprocessed
//! when watching again.
//!
//! Origin: Smart Speed from Overcast, which also acts during playback.

use serde::{Deserialize, Serialize};

/// A stretch of silence, in seconds from the start.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SilenceSpan {
    pub start: f64,
    pub end: f64,
}

impl SilenceSpan {
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// Map persisted alongside the lesson.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SilenceMap {
    /// Algorithm version. If the probe parameters change, an old map describes
    /// something else and needs recomputing instead of reusing.
    #[serde(default)]
    pub version: u32,
    pub media_duration_secs: f64,
    #[serde(default)]
    pub spans: Vec<SilenceSpan>,
}

pub const CURRENT_VERSION: u32 = 1;

/// How much of each stretch stays preserved during playback.
///
/// Smaller than B36's `stop_duration` of 0.35 s because there is no risk of
/// clipping audio here: nothing is cut, only skipped, and a jump can be adjusted
/// or switched off at any time. 0.15 s keeps speech from starting abruptly
/// without giving back the whole silence.
pub const PLAYBACK_PADDING_SECS: f64 = 0.15;

/// A stretch too short is not worth a jump: the seek costs more than the silence.
pub const MIN_SKIPPABLE_SECS: f64 = 0.4;

/// Reads `silence_start` / `silence_end` from ffmpeg stderr in pairs.
///
/// A `silence_start` without a `silence_end` means the file ended in silence;
/// without the total duration the pair cannot be closed, so it is discarded.
pub fn parse_spans(stderr: &str) -> Vec<SilenceSpan> {
    let mut spans = Vec::new();
    let mut open: Option<f64> = None;

    for line in stderr.lines() {
        if let Some(v) = field_after(line, "silence_start:") {
            open = Some(v);
        } else if let Some(end) = field_after(line, "silence_end:") {
            if let Some(start) = open.take() {
                if end > start {
                    spans.push(SilenceSpan { start, end });
                }
            }
        }
    }
    spans
}

fn field_after(line: &str, key: &str) -> Option<f64> {
    let idx = line.find(key)?;
    line[idx + key.len()..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Where to jump when playback reaches `position`.
///
/// `None` means "keep playing". Returns the destination instead of a boolean
/// so the player does not have to reimplement the padding arithmetic.
pub fn skip_target(map: &SilenceMap, position: f64) -> Option<f64> {
    map.spans
        .iter()
        .filter(|s| s.duration() >= MIN_SKIPPABLE_SECS)
        .find(|s| position >= s.start && position < s.end - PLAYBACK_PADDING_SECS)
        .map(|s| s.end - PLAYBACK_PADDING_SECS)
}

/// How much time playback saves with the map, in seconds.
///
/// This is the number B36 does not reach: here only the playback padding is
/// subtracted, and it is less than half of the `stop_duration` the cut demands.
pub fn savings_secs(map: &SilenceMap) -> f64 {
    map.spans
        .iter()
        .filter(|s| s.duration() >= MIN_SKIPPABLE_SECS)
        .map(|s| (s.duration() - PLAYBACK_PADDING_SECS).max(0.0))
        .sum()
}

/// A map recorded by an earlier version of the algorithm is no good.
pub fn needs_recompute(map: &SilenceMap) -> bool {
    map.version != CURRENT_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(spans: &[(f64, f64)]) -> SilenceMap {
        SilenceMap {
            version: CURRENT_VERSION,
            media_duration_secs: 300.0,
            spans: spans
                .iter()
                .map(|(a, b)| SilenceSpan { start: *a, end: *b })
                .collect(),
        }
    }

    const STDERR: &str = "\
[silencedetect @ 0x1] silence_start: 10.0\n\
[silencedetect @ 0x1] silence_end: 15.0 | silence_duration: 5.0\n\
[silencedetect @ 0x1] silence_start: 100.0\n\
[silencedetect @ 0x1] silence_end: 103.0 | silence_duration: 3.0\n\
[silencedetect @ 0x1] silence_start: 290.0\n";

    #[test]
    fn le_pares_e_descarta_o_trecho_aberto() {
        let s = parse_spans(STDERR);
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!(
            s[0],
            SilenceSpan {
                start: 10.0,
                end: 15.0
            }
        );
        assert_eq!(
            s[1],
            SilenceSpan {
                start: 100.0,
                end: 103.0
            }
        );
    }

    #[test]
    fn stderr_sem_silencio_da_mapa_vazio() {
        assert!(parse_spans("").is_empty());
        assert!(parse_spans("size=N/A time=00:05:00.00").is_empty());
        // A `silence_end` without a matching `start` must not become a span.
        assert!(parse_spans("silence_end: 5.0 | silence_duration: 1.0").is_empty());
    }

    #[test]
    fn salta_do_inicio_do_silencio_para_pouco_antes_do_fim() {
        let m = map(&[(10.0, 15.0)]);
        let alvo = skip_target(&m, 10.0).unwrap();
        assert!(
            (alvo - (15.0 - PLAYBACK_PADDING_SECS)).abs() < 1e-9,
            "{alvo}"
        );
        // It also jumps from inside the stretch, not only at the exact edge.
        assert!(skip_target(&m, 12.0).is_some());
    }

    #[test]
    fn nao_salta_fora_do_silencio() {
        let m = map(&[(10.0, 15.0)]);
        assert_eq!(skip_target(&m, 5.0), None);
        assert_eq!(skip_target(&m, 20.0), None);
    }

    #[test]
    fn nao_salta_dentro_do_padding_final() {
        // Jumping here would give a millisecond seek and clip the start of
        // speech — the defect the padding exists to avoid.
        let m = map(&[(10.0, 15.0)]);
        assert_eq!(skip_target(&m, 14.95), None);
    }

    #[test]
    fn trecho_curto_demais_nao_vale_o_seek() {
        let m = map(&[(10.0, 10.2)]);
        assert_eq!(skip_target(&m, 10.0), None);
        assert_eq!(savings_secs(&m), 0.0);
    }

    #[test]
    fn a_economia_supera_a_do_corte_em_arquivo() {
        // The point of the item. Twelve 5 s stretches: cutting the file, B36
        // preserves 0.35 s of each and delivers 55.8 s. Skipping during playback
        // the padding is 0.15 s, so more is left.
        let spans: Vec<(f64, f64)> = (0..12)
            .map(|i| (i as f64 * 25.0, i as f64 * 25.0 + 5.0))
            .collect();
        let m = map(&spans);

        let economia = savings_secs(&m);
        let corte_em_arquivo = 12.0 * (5.0 - 0.35);
        assert!(
            economia > corte_em_arquivo,
            "playback {economia} should beat file cut {corte_em_arquivo}"
        );
        assert!((economia - 12.0 * (5.0 - 0.15)).abs() < 1e-9, "{economia}");
    }

    #[test]
    fn mapa_de_versao_antiga_e_recomputado() {
        // If the probe parameters change, an old map describes something else.
        // Reusing it would make the player jump in the wrong places.
        let mut m = map(&[(1.0, 2.0)]);
        assert!(!needs_recompute(&m));
        m.version = 0;
        assert!(needs_recompute(&m));
        assert!(needs_recompute(&SilenceMap::default()));
    }

    #[test]
    fn mapa_sobrevive_ao_round_trip_json() {
        let m = map(&[(10.0, 15.0), (100.0, 103.0)]);
        let json = serde_json::to_string(&m).unwrap();
        let back: SilenceMap = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn mapa_antigo_sem_version_e_tratado_como_desatualizado() {
        // A new field in an already recorded metadata: missing becomes 0, which
        // is not the current version, so it recomputes instead of trusting a map
        // of unknown origin.
        let json = r#"{"media_duration_secs":300.0,"spans":[]}"#;
        let m: SilenceMap = serde_json::from_str(json).unwrap();
        assert_eq!(m.version, 0);
        assert!(needs_recompute(&m));
    }
}
