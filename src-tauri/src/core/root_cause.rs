//! Translates yt-dlp stderr into a named cause plus the matching fix.
//!
//! The user does not read stderr. They read "failed" and open an issue, or give
//! up. This module is what turns B41, B42 and B43 into something actionable:
//! without it, each one delivers capability the user cannot trigger.
//!
//! Each cause carries **one action**, and the action is what the screen turns
//! into a button. A cause with no matching action is just a better-written
//! error, and that was not what was asked for.
//!
//! Detection order matters: the first match wins, and the most specific ones
//! come first. A 403 for fingerprinting and a 403 for private content look the
//! same at the end of the message, and sending the user after the wrong fix is
//! worse than suggesting nothing.
//!
//! Origin: the single root-cause panel in air-traffic control — one screen
//! where every alarm has its cause and action beside it, instead of a list of
//! symptoms.

use serde::Serialize;

/// What the user can do. Becomes a button on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Remedy {
    /// Import browser cookies.
    ImportCookies,
    /// Try again imitating a browser (`--impersonate`).
    UseImpersonation,
    /// Set up or configure the PO Token provider.
    SetUpPotProvider,
    /// Switch the YouTube client — the B41 cascade already does that on its own.
    SwitchPlayerClient,
    /// Wait and try again; nothing to configure.
    WaitAndRetry,
    /// Liberar espaco em disco.
    FreeDiskSpace,
    /// Update yt-dlp.
    UpdateYtdlp,
    /// Nothing the app knows how to suggest.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnosis {
    /// i18n key. Never ready-made text: the message is translated on screen.
    pub cause_key: &'static str,
    pub remedy: Remedy,
    /// Technical detail, for the debug panel. Does not go to the toast.
    pub detail: Option<String>,
}

/// Diagnoses a download failure from stderr.
///
/// `None` means the app does not know — better to admit it than to invent a
/// cause, because a wrong diagnosis sends the user to the wrong fix and they
/// lose more time than if there had been no diagnosis at all.
pub fn diagnose(stderr: &str) -> Option<Diagnosis> {
    let s = stderr.to_lowercase();

    // Most specific first. Several of these end in 403 or in "unavailable",
    // and the order is what avoids matching for the wrong reason.
    if crate::core::pot_provider::stderr_indicates_missing_token(&s) {
        return Some(Diagnosis {
            cause_key: "error.cause.po_token_required",
            remedy: Remedy::SetUpPotProvider,
            detail: Some("youtube requires a proof-of-origin token".into()),
        });
    }

    if s.contains("sabr") {
        return Some(Diagnosis {
            cause_key: "error.cause.sabr_only",
            remedy: Remedy::SwitchPlayerClient,
            detail: Some("web client returned SABR-only formats".into()),
        });
    }

    if s.contains("http error 429") || s.contains("too many requests") {
        return Some(Diagnosis {
            cause_key: "error.cause.rate_limited",
            remedy: Remedy::WaitAndRetry,
            detail: None,
        });
    }

    if s.contains("no space left") || s.contains("disk full") || s.contains("os error 28") {
        return Some(Diagnosis {
            cause_key: "error.cause.disk_full",
            remedy: Remedy::FreeDiskSpace,
            detail: None,
        });
    }

    // Auth before fingerprint: "private video" and "members-only" are
    // unambiguous, while a bare 403 is ambiguous.
    if s.contains("private video")
        || s.contains("video is private")
        || s.contains("members-only")
        || s.contains("login required")
        || s.contains("this video is available to this channel's members")
        || s.contains("requires purchase")
        || s.contains("sign in")
    {
        return Some(Diagnosis {
            cause_key: "error.cause.needs_login",
            remedy: Remedy::ImportCookies,
            detail: None,
        });
    }

    if crate::core::impersonation::stderr_indicates_fingerprint_block(&s) {
        return Some(Diagnosis {
            cause_key: "error.cause.tls_fingerprint",
            remedy: Remedy::UseImpersonation,
            detail: Some("site rejected the connection before any content".into()),
        });
    }

    if s.contains("nsig extraction failed") || s.contains("unable to extract") {
        return Some(Diagnosis {
            cause_key: "error.cause.extractor_outdated",
            remedy: Remedy::UpdateYtdlp,
            detail: None,
        });
    }

    None
}

/// Every i18n key the screen needs translated.
///
/// Exists so the test can pin it down: a new cause without a matching key
/// becomes raw text in the UI, which is the most visible kind of i18n
/// regression.
pub const ALL_CAUSE_KEYS: &[&str] = &[
    "error.cause.po_token_required",
    "error.cause.sabr_only",
    "error.cause.rate_limited",
    "error.cause.disk_full",
    "error.cause.needs_login",
    "error.cause.tls_fingerprint",
    "error.cause.extractor_outdated",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn causa(stderr: &str) -> Option<(&'static str, Remedy)> {
        diagnose(stderr).map(|d| (d.cause_key, d.remedy))
    }

    #[test]
    fn as_cinco_falhas_mais_frequentes_tem_causa_e_acao() {
        // Acceptance criterion of the item.
        let casos = [
            (
                "ERROR: [youtube] abc: Sign in to confirm you're not a bot",
                "error.cause.po_token_required",
                Remedy::SetUpPotProvider,
            ),
            (
                "WARNING: [youtube] abc: YouTube is forcing SABR streaming",
                "error.cause.sabr_only",
                Remedy::SwitchPlayerClient,
            ),
            (
                "ERROR: HTTP Error 429: Too Many Requests",
                "error.cause.rate_limited",
                Remedy::WaitAndRetry,
            ),
            (
                "ERROR: unable to write data: [Errno 28] No space left on device",
                "error.cause.disk_full",
                Remedy::FreeDiskSpace,
            ),
            (
                "ERROR: [youtube] abc: Private video. Sign in if you've been granted access",
                "error.cause.needs_login",
                Remedy::ImportCookies,
            ),
        ];
        for (stderr, chave, acao) in casos {
            let d = diagnose(stderr).unwrap_or_else(|| panic!("no diagnosis: {stderr:?}"));
            assert_eq!(d.cause_key, chave, "stderr: {stderr:?}");
            assert_eq!(d.remedy, acao, "stderr: {stderr:?}");
        }
    }

    #[test]
    fn video_privado_sem_frase_de_bot_manda_importar_cookies() {
        let d = diagnose("ERROR: [youtube] abc: This video is private").unwrap();
        assert_eq!(d.cause_key, "error.cause.needs_login");
        assert_eq!(d.remedy, Remedy::ImportCookies);
    }

    #[test]
    fn a_ordem_evita_mandar_o_usuario_para_a_solucao_errada() {
        // A bare 403 is ambiguous and becomes fingerprinting; a 403 accompanied
        // by "members-only" is paid content and becomes cookies. Swapping the
        // order would make the user install impersonation to fix a missing login.
        assert_eq!(
            causa("ERROR: HTTP Error 403: Forbidden"),
            Some(("error.cause.tls_fingerprint", Remedy::UseImpersonation))
        );
        assert_eq!(
            causa("ERROR: members-only content. HTTP Error 403: Forbidden"),
            Some(("error.cause.needs_login", Remedy::ImportCookies))
        );
    }

    #[test]
    fn falha_desconhecida_admite_que_nao_sabe() {
        // Inventing a cause is worse than not diagnosing: it sends the user after
        // the wrong fix and they lose more time than they would have with nothing.
        for desconhecido in [
            "ERROR: something nobody has seen before",
            "ERROR: ffmpeg exited with code 1",
            "",
        ] {
            assert_eq!(
                diagnose(desconhecido),
                None,
                "inventou causa: {desconhecido:?}"
            );
        }
    }

    #[test]
    fn sabr_vence_rate_limit_quando_os_dois_aparecem() {
        // The 429 is a frequent consequence of the client cascade retrying;
        // reporting a rate limit would hide the real cause.
        let d = diagnose("YouTube is forcing SABR streaming\nHTTP Error 429").unwrap();
        assert_eq!(d.cause_key, "error.cause.sabr_only");
    }

    #[test]
    fn toda_causa_esta_na_lista_de_chaves_de_i18n() {
        // A new cause without a key becomes raw text in the UI.
        let amostras = [
            "sign in to confirm you're not a bot",
            "forcing sabr streaming",
            "http error 429",
            "no space left on device",
            "this video is private",
            "unable to handshake",
            "nsig extraction failed",
        ];
        for a in amostras {
            let d = diagnose(a).unwrap_or_else(|| panic!("no diagnosis: {a:?}"));
            assert!(
                ALL_CAUSE_KEYS.contains(&d.cause_key),
                "{} is not in ALL_CAUSE_KEYS",
                d.cause_key
            );
        }
    }

    #[test]
    fn detalhe_tecnico_nao_vaza_para_a_causa() {
        // The detail goes to the debug panel; the screen shows the translated
        // key. Separate fields on purpose.
        let d = diagnose("YouTube is forcing SABR streaming").unwrap();
        assert!(d.cause_key.starts_with("error.cause."));
        assert!(d.detail.is_some());
        assert!(!d.cause_key.contains(' '));
    }
}
