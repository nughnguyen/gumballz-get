//! Finds out whether the bundled yt-dlp can impersonate a browser.
//!
//! Sites with TLS fingerprinting refuse connections from a plain HTTP client and the
//! failure reaches the user as an error they cannot interpret. `curl_cffi`
//! fixes it, but it **does not ship in every build** of yt-dlp.
//!
//! ## Premise correction
//!
//! The backlog said "not in every build — detect absence and offer
//! guided install". Verified against the binary the app bundles today
//! (yt-dlp 2026.07.23): it **has** `curl_cffi`, with Chrome, Safari,
//! Edge, Firefox and Tor targets. So detection stays correct, but the common case is
//! presence, not absence — and the value is in **knowing before trying**, so
//! the flag can be suggested instead of letting the failure leak out raw.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImpersonateTarget {
    pub client: String,
    pub os: Option<String>,
}

impl ImpersonateTarget {
    /// `--impersonate` value, in `CLIENT[:OS]` format.
    pub fn as_flag_value(&self) -> String {
        match &self.os {
            Some(os) => format!("{}:{}", self.client, os),
            None => self.client.clone(),
        }
    }
}

/// Reads the output of `yt-dlp --list-impersonate-targets`.
///
/// Real format (2026.07):
/// ```text
/// [info] Available impersonate targets
/// Client          OS           Source
/// --------------------------------------
/// Chrome-133      Macos-15     curl_cffi
/// ```
///
/// An empty list means a build without `curl_cffi`. That is different from an execution error,
/// and the caller needs to tell the two apart — that is why the function returns
/// the list, not a boolean.
pub fn parse_targets(stdout: &str) -> Vec<ImpersonateTarget> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with('[')) // [info] line
        .filter(|l| !l.starts_with('-')) // separator
        .filter(|l| !l.starts_with("Client")) // header
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let client = cols.next()?.to_string();
            let os = cols.next().map(|s| s.to_string()).filter(|s| s != "-");
            Some(ImpersonateTarget { client, os })
        })
        .collect()
}

/// Does stderr indicate a refusal by TLS fingerprint?
pub fn stderr_indicates_fingerprint_block(stderr_lower: &str) -> bool {
    const SIGNS: &[&str] = &[
        "unable to handshake",
        "ssl: sslv3_alert_handshake_failure",
        "the read operation timed out",
        "cloudflare",
        "just a moment...",
        "enable javascript and cookies to continue",
        "http error 403: forbidden",
    ];
    SIGNS.iter().any(|s| stderr_lower.contains(s))
}

/// Best target to retry with, given what the build offers.
///
/// Prefers desktop Chrome: it is what most fingerprinting sites
/// expect to see, and a mobile or Tor target changes more than the fingerprint.
pub fn preferred_target(targets: &[ImpersonateTarget]) -> Option<&ImpersonateTarget> {
    let desktop_chrome = targets.iter().find(|t| {
        t.client.starts_with("Chrome")
            && t.os
                .as_deref()
                .is_some_and(|os| !os.starts_with("Android") && !os.starts_with("Ios"))
    });
    desktop_chrome
        .or_else(|| targets.iter().find(|t| t.client.starts_with("Chrome")))
        .or_else(|| targets.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real output of the yt-dlp 2026.07.23 bundled by the app, with the order
    /// shuffled on purpose: with desktop Chrome first, the preference test
    /// would pass even if the function just took the first target.
    const REAL: &str = "\
[info] Available impersonate targets\n\
Client          OS           Source\n\
--------------------------------------\n\
Chrome-99       Android-12   curl_cffi\n\
Safari-17.2     Ios-17.2     curl_cffi\n\
Chrome-131      Android-14   curl_cffi\n\
Chrome-133      Macos-15     curl_cffi\n\
Chrome-136      Macos-15     curl_cffi\n\
Safari-18.0     Ios-18.0     curl_cffi\n\
Tor-14.5        Macos-14     curl_cffi\n";

    #[test]
    fn reads_the_real_output_ignoring_header_and_separator() {
        let t = parse_targets(REAL);
        assert_eq!(t.len(), 7, "{t:?}");
        let chrome133 = t
            .iter()
            .find(|x| x.client == "Chrome-133")
            .expect("Chrome-133");
        assert_eq!(chrome133.os.as_deref(), Some("Macos-15"));
        assert!(!t.iter().any(|x| x.client.starts_with('[')));
        assert!(!t.iter().any(|x| x.client == "Client"));
    }

    #[test]
    fn build_without_curl_cffi_gives_an_empty_list() {
        // An empty list is the signal of absence, and it is different from an error running
        // the binary — that is why the function returns a list, not a boolean.
        let none = "[info] Available impersonate targets\nClient          OS           Source\n--------------------------------------\n";
        assert!(parse_targets(none).is_empty());
        assert!(parse_targets("").is_empty());
    }

    #[test]
    fn prefers_desktop_chrome() {
        // A mobile or Tor target changes more than the fingerprint, and most
        // blocking sites expect to see a desktop Chrome.
        let t = parse_targets(REAL);
        let chosen = preferred_target(&t).unwrap();
        assert_ne!(
            chosen.client, t[0].client,
            "taking the first target must not pass this test"
        );
        assert_eq!(chosen.client, "Chrome-133");
        assert_eq!(chosen.os.as_deref(), Some("Macos-15"));
    }

    #[test]
    fn falls_back_to_mobile_chrome_when_no_desktop() {
        let mobile_only =
            "Chrome-99       Android-12   curl_cffi\nTor-14.5        Macos-14     curl_cffi\n";
        let t = parse_targets(mobile_only);
        assert_eq!(preferred_target(&t).unwrap().client, "Chrome-99");
    }

    #[test]
    fn with_no_chrome_takes_the_first_available() {
        let t = parse_targets("Safari-18.0     Ios-18.0     curl_cffi\n");
        assert_eq!(preferred_target(&t).unwrap().client, "Safari-18.0");
        assert!(preferred_target(&[]).is_none());
    }

    #[test]
    fn builds_the_flag_value_in_yt_dlp_format() {
        let t = parse_targets(REAL);
        let chrome133 = t.iter().find(|x| x.client == "Chrome-133").unwrap();
        assert_eq!(chrome133.as_flag_value(), "Chrome-133:Macos-15");
        let no_os = ImpersonateTarget {
            client: "Chrome".into(),
            os: None,
        };
        assert_eq!(no_os.as_flag_value(), "Chrome");
    }

    #[test]
    fn recognizes_a_fingerprint_block() {
        for c in [
            "error: unable to handshake with the server",
            "error: <title>just a moment...</title> cloudflare",
            "error: http error 403: forbidden",
        ] {
            assert!(stderr_indicates_fingerprint_block(c), "{c:?}");
        }
    }

    #[test]
    fn does_not_confuse_a_common_failure_with_a_fingerprint_block() {
        // Suggesting impersonation for a private video would send the user
        // chasing the wrong fix.
        for c in [
            "error: [youtube] abc: video unavailable",
            "error: http error 429: too many requests",
            "error: unable to download webpage: name or service not known",
            "",
        ] {
            assert!(
                !stderr_indicates_fingerprint_block(c),
                "false positive: {c:?}"
            );
        }
    }
}
