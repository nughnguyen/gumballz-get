//! A lesson's silence map, for the player to skip during playback.
//!
//! B52. `core::silence_map` knows where the silences are and where to jump;
//! what was missing was computing the map and keeping it.
//!
//! Skipping during playback, instead of cutting the file, is the difference that
//! matters: B36 cut with `silenceremove` and paid two prices — padding residue
//! and output forced to audio, because an audio filter desyncs video. Here the
//! video stays a video and the padding becomes a reversible decision.
//!
//! The probe is expensive (one ffmpeg pass over the whole file), so it runs once
//! and the map stays on disk. Nothing is reprocessed when watching again.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

use crate::core::silence_map::{
    needs_recompute, savings_secs, skip_target, SilenceMap, CURRENT_VERSION,
};

const MAPS_FILE: &str = "silence-maps.json";

fn file_path() -> Option<PathBuf> {
    crate::core::paths::app_data_dir().map(|d| d.join(MAPS_FILE))
}

fn load() -> BTreeMap<String, SilenceMap> {
    let Some(path) = file_path() else {
        return BTreeMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save(map: &BTreeMap<String, SilenceMap>) {
    let Some(path) = file_path() else { return };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(json) = serde_json::to_string(map) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Total duration of the file, read from the same ffmpeg output.
///
/// It has to come along because a `silence_start` without a `silence_end` can
/// only be interpreted knowing where the file ends.
fn parse_duration_secs(stderr: &str) -> Option<f64> {
    let idx = stderr.find("Duration:")?;
    let resto = &stderr[idx + "Duration:".len()..];
    let bruto = resto.split(',').next()?.trim();
    let mut partes = bruto.split(':');
    let h: f64 = partes.next()?.trim().parse().ok()?;
    let m: f64 = partes.next()?.trim().parse().ok()?;
    let s: f64 = partes.next()?.trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SilenceMapInfo {
    pub map: SilenceMap,
    /// How much time the jump saves, in seconds.
    pub savings_secs: f64,
    /// `true` when the map came from the cache, without probing again.
    pub from_cache: bool,
}

/// Computes (or reuses) a file's silence map.
#[tauri::command]
pub async fn compute_silence_map(
    path: String,
    force: Option<bool>,
) -> Result<SilenceMapInfo, String> {
    let force = force.unwrap_or(false);
    let arquivo = PathBuf::from(&path);
    if !arquivo.is_file() {
        return Err("The file does not exist".to_string());
    }

    if !force {
        if let Some(cached) = load().get(&path) {
            // `needs_recompute` compares the algorithm version: a map recorded
            // with other probe parameters describes something else.
            if !needs_recompute(cached) {
                return Ok(SilenceMapInfo {
                    savings_secs: savings_secs(cached),
                    map: cached.clone(),
                    from_cache: true,
                });
            }
        }
    }

    let ffmpeg = crate::core::dependencies::find_tool("ffmpeg")
        .await
        .ok_or_else(|| "FFmpeg not found. Install it in Settings → Plugins.".to_string())?;

    let alvo = arquivo.clone();
    let stderr = tokio::task::spawn_blocking(move || {
        let mut cmd = crate::core::process::std_command(&ffmpeg);
        cmd.arg("-i").arg(&alvo);
        for a in gumballz_core::core::ffmpeg_ops::silence_probe_args() {
            cmd.arg(a);
        }
        cmd.arg("-")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let texto = String::from_utf8_lossy(&stderr.stderr);
    let duracao = parse_duration_secs(&texto)
        .ok_or_else(|| "Could not read the file's duration".to_string())?;

    let mapa = SilenceMap {
        version: CURRENT_VERSION,
        media_duration_secs: duracao,
        spans: crate::core::silence_map::parse_spans(&texto),
    };

    let mut todos = load();
    todos.insert(path, mapa.clone());
    save(&todos);

    Ok(SilenceMapInfo {
        savings_secs: savings_secs(&mapa),
        map: mapa,
        from_cache: false,
    })
}

/// Where to jump, if the current position falls inside a silence.
///
/// Called by the player on every time update. It lives in the backend instead of
/// duplicating the rule in TypeScript: a second implementation of the same
/// calculation would diverge at the first padding change.
#[tauri::command]
pub fn silence_skip_target(path: String, position_secs: f64) -> Option<f64> {
    let mapa = load();
    skip_target(mapa.get(&path)?, position_secs)
}

/// Forgets a file's map.
#[tauri::command]
pub fn forget_silence_map(path: String) {
    let mut todos = load();
    if todos.remove(&path).is_some() {
        save(&todos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real ffmpeg stderr, trimmed.
    const STDERR: &str = "\
Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'aula.mp4':
  Duration: 00:41:03.52, start: 0.000000, bitrate: 1200 kb/s
[silencedetect @ 0x7f8] silence_start: 12.4213
[silencedetect @ 0x7f8] silence_end: 15.0102 | silence_duration: 2.5889
";

    #[test]
    fn a_duracao_sai_do_mesmo_stderr_da_sonda() {
        // Without it, a silence that runs to the end of the file has no way to be
        // measured — and an extra pass just for that would double the cost.
        let d = parse_duration_secs(STDERR).expect("reads duration");
        assert!((d - 2463.52).abs() < 0.01, "41:03.52 = 2463.52s, got {d}");
    }

    #[test]
    fn stderr_sem_duracao_nao_inventa_numero() {
        assert_eq!(parse_duration_secs("nada aqui"), None);
    }

    #[test]
    fn duracao_com_horas_e_lida_certo() {
        let s = "  Duration: 02:05:30.00, start: 0.0";
        let d = parse_duration_secs(s).expect("reads");
        assert!((d - 7530.0).abs() < 0.01, "2h05m30s = 7530s, got {d}");
    }

    #[test]
    fn mapa_de_versao_antiga_e_recomputado() {
        // The point of the `version` field: a map recorded with other probe
        // parameters describes something else, and reusing it would jump in the
        // wrong place.
        let antigo = SilenceMap {
            version: 0,
            media_duration_secs: 100.0,
            spans: Vec::new(),
        };
        assert!(needs_recompute(&antigo));

        let atual = SilenceMap {
            version: CURRENT_VERSION,
            media_duration_secs: 100.0,
            spans: Vec::new(),
        };
        assert!(!needs_recompute(&atual));
    }
}
