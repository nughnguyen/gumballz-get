//! Declarative rules engine: "channel X's URL -> folder Y, quality Z".
//!
//! Written once by the user and evaluated at enqueue, so they stop repeating the
//! same five choices on every download from the same channel.
//!
//! Deliberately **not** a scripting language. It is a list of rules with
//! combos, because an expression editor becomes its own support surface and the
//! gain over "contains / equals" is small for this case.
//!
//! Origin: Sieve, from email clients.

use serde::{Deserialize, Serialize};

/// What the rule looks at. First match wins, so the list order is the
/// priority — like an email filter, and for the same reason: predictability
/// is worth more than expressiveness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    /// Host exactly equal, ignoring `www.` and case.
    HostIs { value: String },
    /// Substring anywhere in the URL, case-insensitive.
    UrlContains { value: String },
    /// Platform resolved by the registry (`youtube`, `bilibili`, ...).
    PlatformIs { value: String },
}

impl Condition {
    pub fn matches(&self, url: &str, platform: Option<&str>) -> bool {
        match self {
            Condition::HostIs { value } => host_of(url)
                .is_some_and(|h| h.eq_ignore_ascii_case(value.trim_start_matches("www."))),
            Condition::UrlContains { value } => url.to_lowercase().contains(&value.to_lowercase()),
            Condition::PlatformIs { value } => {
                platform.is_some_and(|p| p.eq_ignore_ascii_case(value))
            }
        }
    }
}

/// What the rule does. A missing field means "do not touch", not "use the
/// default" — the distinction matters so a rule can change only the folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Actions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitles: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub name: String,
    pub when: Condition,
    pub then: Actions,
}

fn default_enabled() -> bool {
    true
}

/// Host of a URL, lowercase and without `www.`.
///
/// Manual parsing instead of the `url` crate: it accepts malformed input without
/// error, which is what is wanted here — a rule that does not match is better
/// than an aborted enqueue.
pub fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()?
        .split('@')
        .next_back()?
        .split(':')
        .next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.trim_start_matches("www.").to_lowercase())
}

/// First enabled rule that matches.
pub fn first_match<'a>(rules: &'a [Rule], url: &str, platform: Option<&str>) -> Option<&'a Rule> {
    rules
        .iter()
        .filter(|r| r.enabled)
        .find(|r| r.when.matches(url, platform))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str, when: Condition, then: Actions) -> Rule {
        Rule {
            enabled: true,
            name: name.to_string(),
            when,
            then,
        }
    }

    #[test]
    fn host_normaliza_www_porta_e_caixa() {
        let casos = [
            ("https://www.YouTube.com/watch?v=1", "youtube.com"),
            ("http://youtube.com:8080/x", "youtube.com"),
            ("https://user@bilibili.com/video", "bilibili.com"),
            ("bilibili.com/video/BV1", "bilibili.com"),
        ];
        for (url, esperado) in casos {
            assert_eq!(host_of(url).as_deref(), Some(esperado), "url {url}");
        }
        assert_eq!(host_of("").as_deref(), None);
        assert_eq!(host_of("https://").as_deref(), None);
    }

    #[test]
    fn a_primeira_regra_que_casa_vence() {
        // Order is priority. Without it, two matching rules become unpredictable
        // behaviour and the user cannot debug it.
        let rules = vec![
            rule(
                "especifica",
                Condition::UrlContains {
                    value: "/@canal-x".into(),
                },
                Actions {
                    output_dir: Some("/cursos/canal-x".into()),
                    ..Default::default()
                },
            ),
            rule(
                "geral",
                Condition::HostIs {
                    value: "youtube.com".into(),
                },
                Actions {
                    output_dir: Some("/videos".into()),
                    ..Default::default()
                },
            ),
        ];
        let m = first_match(&rules, "https://youtube.com/@canal-x/videos", None).unwrap();
        assert_eq!(m.name, "especifica");

        let m2 = first_match(&rules, "https://youtube.com/watch?v=9", None).unwrap();
        assert_eq!(m2.name, "geral");
    }

    #[test]
    fn regra_desligada_e_pulada_sem_bloquear_a_seguinte() {
        let mut rules = vec![
            rule(
                "desligada",
                Condition::HostIs {
                    value: "youtube.com".into(),
                },
                Actions {
                    quality: Some("360".into()),
                    ..Default::default()
                },
            ),
            rule(
                "ativa",
                Condition::HostIs {
                    value: "youtube.com".into(),
                },
                Actions {
                    quality: Some("1080".into()),
                    ..Default::default()
                },
            ),
        ];
        rules[0].enabled = false;
        let m = first_match(&rules, "https://youtube.com/watch?v=1", None).unwrap();
        assert_eq!(m.name, "ativa");
    }

    #[test]
    fn nenhuma_regra_casando_devolve_none() {
        let rules = vec![rule(
            "so bilibili",
            Condition::PlatformIs {
                value: "bilibili".into(),
            },
            Actions::default(),
        )];
        assert!(first_match(&rules, "https://youtube.com/x", Some("youtube")).is_none());
        assert!(first_match(&rules, "https://youtube.com/x", None).is_none());
        assert!(first_match(&[], "https://qualquer.com", None).is_none());
    }

    #[test]
    fn plataforma_casa_sem_depender_da_url() {
        let rules = vec![rule(
            "cursos",
            Condition::PlatformIs {
                value: "Hotmart".into(),
            },
            Actions {
                subtitles: Some(true),
                tags: vec!["curso".into()],
                ..Default::default()
            },
        )];
        let m = first_match(&rules, "https://algum-cdn.example/x", Some("hotmart")).unwrap();
        assert_eq!(m.then.tags, vec!["curso"]);
        assert_eq!(m.then.subtitles, Some(true));
    }

    #[test]
    fn acoes_ausentes_significam_nao_mexer() {
        // A distinction that matters: `None` must not become "use the default",
        // or a rule that only changes the folder would zero the chosen quality.
        let a = Actions {
            output_dir: Some("/x".into()),
            ..Default::default()
        };
        assert_eq!(a.quality, None);
        assert_eq!(a.audio_only, None);
        assert!(a.tags.is_empty());
    }

    #[test]
    fn regra_sobrevive_ao_round_trip_json() {
        let r = rule(
            "canal x",
            Condition::UrlContains { value: "@x".into() },
            Actions {
                output_dir: Some("/a".into()),
                quality: Some("1080".into()),
                tags: vec!["aula".into()],
                ..Default::default()
            },
        );
        let json = serde_json::to_string(&r).unwrap();
        let back: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn regra_antiga_sem_enabled_carrega_habilitada() {
        // A new field must not silently switch off the rules the user already
        // had — it is the same class of bug that zeroed settings.json.
        let json = r#"{"name":"velha","when":{"kind":"host_is","value":"x.com"},"then":{}}"#;
        let r: Rule = serde_json::from_str(json).unwrap();
        assert!(r.enabled);
    }
}
