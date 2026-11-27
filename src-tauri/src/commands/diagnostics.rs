#[tauri::command]
pub async fn get_hwaccel_info() -> gumballz_core::core::hwaccel::HwAccelInfo {
    gumballz_core::core::hwaccel::detect_hwaccel().await
}

#[tauri::command]
pub fn diagnose_download_error(stderr: String) -> Option<crate::core::root_cause::Diagnosis> {
    crate::core::root_cause::diagnose(&stderr)
}

/// Black-box dump, to paste into a bug report.
///
/// The lines already come redacted by `flight_recorder::redact` — tokens, cookies
/// and paths with a username never reach here.
#[tauri::command]
pub fn flight_recorder_dump() -> Vec<String> {
    crate::core::flight_recorder::dump()
}

/// Clears what the black box kept.
///
/// Exists because the user has to be able to decide to send nothing, even after
/// opening the report.
#[tauri::command]
pub fn flight_recorder_clear() {
    crate::core::flight_recorder::clear();
}

/// Checks a batch before enqueueing: what can download, what cannot, and
/// whether it fits on disk.
///
/// B34. Exists because finding out the disk filled up halfway through a batch of
/// 40 lessons costs far more than the check. It does not consult the network: the
/// expensive questions (the real size of each item) stay out on purpose — the
/// useful answer is "is it worth starting?", not "how much exactly".
#[tauri::command]
pub async fn preflight_batch(
    state: tauri::State<'_, crate::AppState>,
    urls: Vec<String>,
    output_dir: Option<String>,
) -> Result<crate::core::preflight::PreflightReport, String> {
    let checks = classificar_lote(urls, |u| state.registry.find_platform(u).is_some());

    let livre = output_dir
        .map(std::path::PathBuf::from)
        .as_deref()
        .and_then(crate::core::preflight::available_space);

    Ok(crate::core::preflight::build_report(checks, livre))
}

/// Classifies each URL in the batch. Kept separate from the command so it can
/// be tested without building an `AppState`.
fn classificar_lote(
    urls: Vec<String>,
    suportada: impl Fn(&str) -> bool,
) -> Vec<crate::core::preflight::ItemCheck> {
    use crate::core::preflight::{ItemCheck, Problem};

    let mut vistos = std::collections::HashSet::new();
    urls.into_iter()
        .map(|url| {
            let problem = if !vistos.insert(url.clone()) {
                // A duplicate inside the batch itself counts as "already have": the
                // user pasted the same URL twice.
                Some(Problem::AlreadyHave)
            } else if !suportada(&url) {
                Some(Problem::Unsupported)
            } else {
                None
            };
            ItemCheck {
                url,
                title: None,
                estimated_bytes: None,
                problem,
            }
        })
        .collect()
}

#[cfg(test)]
mod preflight_lote_tests {
    use super::*;
    use crate::core::preflight::Problem;

    #[test]
    fn url_repetida_no_lote_nao_baixa_duas_vezes() {
        let urls = vec![
            "https://a.com/1".to_string(),
            "https://a.com/1".to_string(),
            "https://a.com/2".to_string(),
        ];
        let checks = classificar_lote(urls, |_| true);
        assert_eq!(checks[0].problem, None, "the first occurrence downloads");
        assert_eq!(
            checks[1].problem,
            Some(Problem::AlreadyHave),
            "the second one is the same thing"
        );
        assert_eq!(checks[2].problem, None);
    }

    #[test]
    fn site_sem_downloader_e_marcado_em_vez_de_enfileirado() {
        let checks = classificar_lote(vec!["https://desconhecido.tld/x".to_string()], |_| false);
        assert_eq!(checks[0].problem, Some(Problem::Unsupported));
    }

    #[test]
    fn lote_limpo_nao_inventa_problema() {
        let checks = classificar_lote(
            vec!["https://a.com/1".to_string(), "https://a.com/2".to_string()],
            |_| true,
        );
        assert!(checks.iter().all(|c| c.problem.is_none()));
    }
}
