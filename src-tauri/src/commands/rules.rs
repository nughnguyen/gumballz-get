//! User rules: "link from this channel -> this folder, at this quality".
//!
//! B40. The decision engine (`core::rules`) existed, tested and without a
//! caller. Here it gains persistence, commands and the point where it is
//! consulted: enqueueing.
//!
//! Its own file instead of a field in `settings.json`, for the same reason as
//! the binary overrides (#222): it does not force a migration on everyone
//! because of a few, and a malformed rule must not stop the settings from loading.

use std::path::PathBuf;

use crate::core::rules::{first_match, Rule};

const RULES_FILE: &str = "rules.json";

fn file_path() -> Option<PathBuf> {
    crate::core::paths::app_data_dir().map(|d| d.join(RULES_FILE))
}

/// Reads the rules from disk. An unreadable file counts as "no rules": losing
/// the rules is an annoyance, not being able to download is a problem.
pub fn load_rules() -> Vec<Rule> {
    let Some(path) = file_path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<Rule>>(&raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[rules] {} unreadable, ignoring: {}", path.display(), e);
            Vec::new()
        }
    }
}

fn save_to_disk(rules: &[Rule]) -> Result<(), String> {
    let path = file_path().ok_or_else(|| "no data directory".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_rules() -> Vec<Rule> {
    load_rules()
}

/// Writes the whole list. Order matters: the first rule that matches wins, and
/// that is why the UI lets you reorder instead of only switching on and off.
#[tauri::command]
pub fn save_rules(rules: Vec<Rule>) -> Result<(), String> {
    for r in &rules {
        if r.name.trim().is_empty() {
            return Err("A rule needs a name so you can recognise it later".to_string());
        }
    }
    save_to_disk(&rules)
}

/// Which rule would match this URL, without downloading anything.
///
/// Exists so the UI can answer "does this rule catch what you expect?" before
/// the user finds out on the wrong download.
#[tauri::command]
pub fn preview_rule_match(url: String, platform: Option<String>) -> Option<Rule> {
    let rules = load_rules();
    first_match(&rules, &url, platform.as_deref()).cloned()
}

/// What the rule decided, for enqueueing to apply.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RuleOutcome {
    pub rule_name: Option<String>,
    pub output_dir: Option<String>,
    pub quality: Option<String>,
    pub audio_only: Option<bool>,
    pub subtitles: Option<bool>,
    pub tags: Vec<String>,
}

/// Resolves the rule for a URL.
///
/// Returns empty when nothing matches — the path of someone who never created
/// any rule, which is the majority, and who must pay nothing for it.
pub fn resolve_for(url: &str, platform: Option<&str>) -> RuleOutcome {
    let rules = load_rules();
    match first_match(&rules, url, platform) {
        Some(r) => RuleOutcome {
            rule_name: Some(r.name.clone()),
            output_dir: r.then.output_dir.clone(),
            quality: r.then.quality.clone(),
            audio_only: r.then.audio_only,
            subtitles: r.then.subtitles,
            tags: r.then.tags.clone(),
        },
        None => RuleOutcome::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rules::{Actions, Condition};

    fn regra(nome: &str, host: &str, pasta: &str) -> Rule {
        Rule {
            enabled: true,
            name: nome.to_string(),
            when: Condition::HostIs {
                value: host.to_string(),
            },
            then: Actions {
                output_dir: Some(pasta.to_string()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn sem_regra_nada_e_decidido() {
        // The path most users take. It has to cost zero and must not invent any
        // folder.
        let out = RuleOutcome::default();
        assert!(out.rule_name.is_none());
        assert!(out.output_dir.is_none());
    }

    #[test]
    fn a_primeira_regra_que_casa_vence() {
        // Order is priority — and that is why the UI needs to let you reorder.
        let rules = vec![
            regra("primeira", "youtube.com", "/a"),
            regra("segunda", "youtube.com", "/b"),
        ];
        let m = first_match(&rules, "https://youtube.com/watch?v=x", None).expect("matches");
        assert_eq!(m.name, "primeira");
        assert_eq!(m.then.output_dir.as_deref(), Some("/a"));
    }

    #[test]
    fn regra_desligada_e_pulada() {
        let mut desligada = regra("desligada", "youtube.com", "/a");
        desligada.enabled = false;
        let rules = vec![desligada, regra("ligada", "youtube.com", "/b")];
        let m = first_match(&rules, "https://youtube.com/watch?v=x", None).expect("matches");
        assert_eq!(m.name, "ligada", "the disabled one must not win");
    }

    #[test]
    fn regra_sem_nome_e_recusada_ao_salvar() {
        // Without a name, the user cannot recognise the rule later, and the list
        // becomes a pile of identical lines.
        let mut r = regra("  ", "youtube.com", "/a");
        r.name = "   ".to_string();
        let erro = save_rules(vec![r]).unwrap_err();
        assert!(erro.contains("nome"));
    }
}
