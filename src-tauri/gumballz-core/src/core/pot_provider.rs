//! YouTube PO Token provider client.
//!
//! YouTube started requiring a Proof-of-Origin Token on some clients, tied
//! to the video and the session, with a short expiry. Without a token, the download degrades to a
//! worse format or is refused — and the message that reaches the user says nothing
//! about any of this.
//!
//! ## What this module does NOT do, and why
//!
//! The backlog described "running bgutil-ytdlp-pot-provider as a side service
//! managed by the app". Checked against the real project (1.3.1, March/2026):
//! it runs as a **Docker container or Node server**, and on top of that requires a
//! **Python plugin installed inside yt-dlp**. Packaging that would mean the
//! app managing a Node runtime with native dependencies and a yt-dlp
//! plugin directory — another order of magnitude, and a failure surface the
//! user cannot diagnose.
//!
//! So what stays here is the app's side: **finding out whether an accessible provider
//! exists, pointing yt-dlp at it, and naming the failure when there is none.**
//! Packaging the provider stays recorded as debt, with the reason.

use serde::Serialize;

/// Where bgutil listens by default when run locally.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4416";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProviderHealth {
    /// Answered and is ready.
    Ready { version: Option<String> },
    /// Address configured but nobody answers.
    Unreachable { base_url: String },
    /// Answered but returned an error — incompatible version, normally.
    Unhealthy { detail: String },
    /// No provider configured. Not an error: it is the default state.
    NotConfigured,
}

impl ProviderHealth {
    pub fn is_usable(&self) -> bool {
        matches!(self, ProviderHealth::Ready { .. })
    }
}

/// Normalizes the provider address.
///
/// Accepts what the user probably types (`localhost:4416`, with or without
/// a trailing slash) and refuses what would become a broken argument in yt-dlp: whitespace
/// inside the `--extractor-args` string splits the argument in two.
pub fn normalize_base_url(raw: &str) -> Option<String> {
    let t = raw.trim().trim_end_matches('/');
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    let with_scheme = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else {
        format!("http://{t}")
    };
    Some(with_scheme)
}

/// Arguments that point yt-dlp at the provider.
///
/// Empty when there is no usable provider: sending the flag pointing at a
/// dead address makes yt-dlp wait out the timeout on **every** download, which is
/// worse than having no provider at all.
pub fn extractor_args(health: &ProviderHealth, base_url: &str) -> Vec<String> {
    if !health.is_usable() {
        return Vec::new();
    }
    let Some(url) = normalize_base_url(base_url) else {
        return Vec::new();
    };
    vec![
        "--extractor-args".to_string(),
        format!("youtubepot-bgutilhttp:base_url={url}"),
    ]
}

/// Does stderr indicate a missing PO Token?
///
/// Matches the phrases yt-dlp actually emits when the token is missing or
/// expired, not a bare "po_token" — the string also shows up in success logs
/// when the token was provided.
pub fn stderr_indicates_missing_token(stderr_lower: &str) -> bool {
    const SIGNS: &[&str] = &[
        "a po token is required",
        "po token is missing",
        "missing a po token",
        "content is not available on this app",
        "please sign in to confirm you're not a bot",
        "sign in to confirm you're not a bot",
    ];
    SIGNS.iter().any(|s| stderr_lower.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_what_the_user_types() {
        for (input, expected) in [
            ("127.0.0.1:4416", "http://127.0.0.1:4416"),
            ("http://localhost:4416/", "http://localhost:4416"),
            ("  https://pot.example.com/ ", "https://pot.example.com"),
        ] {
            assert_eq!(normalize_base_url(input).as_deref(), Some(expected));
        }
    }

    #[test]
    fn rejects_an_address_that_would_break_the_argument() {
        // A space inside the --extractor-args string splits the argument in
        // two, which is a silent failure that is hard to diagnose.
        for bad in ["", "   ", "http://a b:4416", "localhost 4416"] {
            assert_eq!(normalize_base_url(bad), None, "accepted {bad:?}");
        }
    }

    #[test]
    fn unavailable_provider_generates_no_flag() {
        // Pointing at a dead address makes yt-dlp wait out the timeout on EVERY
        // download — worse than having no provider.
        for h in [
            ProviderHealth::NotConfigured,
            ProviderHealth::Unreachable {
                base_url: DEFAULT_BASE_URL.into(),
            },
            ProviderHealth::Unhealthy {
                detail: "500".into(),
            },
        ] {
            assert!(
                extractor_args(&h, DEFAULT_BASE_URL).is_empty(),
                "generated a flag for {h:?}"
            );
        }
    }

    #[test]
    fn ready_provider_generates_the_bgutil_flag() {
        let h = ProviderHealth::Ready {
            version: Some("1.3.1".into()),
        };
        let args = extractor_args(&h, "localhost:4416");
        assert_eq!(args[0], "--extractor-args");
        assert_eq!(
            args[1],
            "youtubepot-bgutilhttp:base_url=http://localhost:4416"
        );
    }

    #[test]
    fn ready_provider_with_invalid_address_still_generates_no_flag() {
        let h = ProviderHealth::Ready { version: None };
        assert!(extractor_args(&h, "  ").is_empty());
    }

    #[test]
    fn recognizes_the_real_missing_token_phrases() {
        let cases = [
            "error: [youtube] abc: a po token is required for this client",
            "warning: [youtube] abc: the content is not available on this app",
            "error: sign in to confirm you're not a bot",
        ];
        for c in cases {
            assert!(stderr_indicates_missing_token(c), "did not recognize {c:?}");
        }
    }

    #[test]
    fn does_not_confuse_a_success_log_with_a_missing_token() {
        // "po_token" shows up in a normal log when the token WAS provided;
        // matching the bare substring would false-positive on every
        // successful download with a provider on.
        let cases = [
            "[debug] [youtube] fetched po_token from provider",
            "[youtube] extracting player po_token: ok",
            "error: http error 429: too many requests",
            "",
        ];
        for c in cases {
            assert!(
                !stderr_indicates_missing_token(c),
                "false positive on {c:?}"
            );
        }
    }

    #[test]
    fn only_ready_is_usable() {
        assert!(ProviderHealth::Ready { version: None }.is_usable());
        assert!(!ProviderHealth::NotConfigured.is_usable());
        assert!(!ProviderHealth::Unhealthy { detail: "x".into() }.is_usable());
    }
}
