//! Batch check before downloading anything.
//!
//! Today a batch of 80 links starts downloading and you find out in the morning
//! that 12 failed for lack of space or because they need a login. Pre-flight
//! resolves everything first, shows what is going to fail, and lets the user
//! decide with the information in hand.
//!
//! The decision is kept separate from the collection on purpose: what to **do**
//! with N URLs, an estimated size and the free space is pure, testable logic;
//! fetching metadata and checking disk is I/O that only integration exercises.
//!
//! Origin: the pre-flight checklist. Do not take off with a known problem on the ground.

use serde::Serialize;

/// What was found out about one URL in the batch.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ItemCheck {
    pub url: String,
    pub title: Option<String>,
    /// Estimated size in bytes, when the extractor reported it.
    pub estimated_bytes: Option<u64>,
    pub problem: Option<Problem>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    /// No downloader recognises the URL.
    Unsupported,
    /// O conteudo existe mas exige sessao — curso pago, video privado.
    NeedsAuth,
    /// The extractor answered that it does not exist.
    NotFound,
    /// Already in the queue or in the archive.
    AlreadyHave,
}

impl Problem {
    /// A problem the user can solve before starting.
    ///
    /// The distinction matters for the copy: "import your cookies" is actionable,
    /// "no downloader covers this site" is not.
    pub fn is_actionable(self) -> bool {
        matches!(self, Problem::NeedsAuth)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreflightReport {
    pub total: usize,
    pub ready: usize,
    pub estimated_bytes: u64,
    /// `None` when the free space could not be read.
    pub available_bytes: Option<u64>,
    pub problems: Vec<ItemCheck>,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Everything resolved and fits on disk.
    Go,
    /// Some items have a problem, but the rest can download.
    GoWithSkips,
    /// Nothing resolved, or it does not fit on disk.
    Stop,
}

/// Margem exigida acima do tamanho estimado.
///
/// Ten percent: the extractor's estimate often errs low (variable bitrate),
/// and filling the disk to the last byte breaks the user's own system, not just
/// the download.
pub const HEADROOM: f64 = 1.10;

/// Builds the report from what was already collected.
///
/// `available_bytes = None` means "could not measure" and does **not** block:
/// refusing the download because the disk read failed would trade a possible
/// problem for a certain one.
pub fn build_report(checks: Vec<ItemCheck>, available_bytes: Option<u64>) -> PreflightReport {
    let total = checks.len();
    let ready = checks.iter().filter(|c| c.problem.is_none()).count();

    let estimated_bytes: u64 = checks
        .iter()
        .filter(|c| c.problem.is_none())
        .filter_map(|c| c.estimated_bytes)
        .sum();

    let fits = match available_bytes {
        Some(free) => (estimated_bytes as f64 * HEADROOM) <= free as f64,
        None => true,
    };

    // Two distinct reasons to stop: nothing resolved, or what resolved does not
    // fit. The screen copy needs to tell the two apart, but the verdict is the same.
    let verdict = if ready == 0 || !fits {
        Verdict::Stop
    } else if ready < total {
        Verdict::GoWithSkips
    } else {
        Verdict::Go
    };

    PreflightReport {
        total,
        ready,
        estimated_bytes,
        available_bytes,
        problems: checks.into_iter().filter(|c| c.problem.is_some()).collect(),
        verdict,
    }
}

/// Free space on the volume of `path`, in bytes.
pub fn available_space(path: &std::path::Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(url: &str, bytes: u64) -> ItemCheck {
        ItemCheck {
            url: url.into(),
            title: Some("t".into()),
            estimated_bytes: Some(bytes),
            problem: None,
        }
    }

    fn bad(url: &str, p: Problem) -> ItemCheck {
        ItemCheck {
            url: url.into(),
            title: None,
            estimated_bytes: None,
            problem: Some(p),
        }
    }

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn lote_limpo_que_cabe_libera() {
        let r = build_report(vec![ok("a", GB), ok("b", GB)], Some(10 * GB));
        assert_eq!(r.verdict, Verdict::Go);
        assert_eq!(r.ready, 2);
        assert_eq!(r.estimated_bytes, 2 * GB);
        assert!(r.problems.is_empty());
    }

    #[test]
    fn alguns_com_problema_ainda_libera_o_resto() {
        // The batch does not stop entirely because of one private link: the user
        // downloads what works and sorts out the rest later.
        let r = build_report(
            vec![ok("a", GB), bad("b", Problem::NeedsAuth), ok("c", GB)],
            Some(10 * GB),
        );
        assert_eq!(r.verdict, Verdict::GoWithSkips);
        assert_eq!(r.ready, 2);
        assert_eq!(r.problems.len(), 1);
        // The estimated size counts only what will actually download.
        assert_eq!(r.estimated_bytes, 2 * GB);
    }

    #[test]
    fn nada_resolvendo_para_o_lote() {
        let r = build_report(
            vec![bad("a", Problem::Unsupported), bad("b", Problem::NotFound)],
            Some(100 * GB),
        );
        assert_eq!(r.verdict, Verdict::Stop);
        assert_eq!(r.ready, 0);
    }

    #[test]
    fn nao_cabe_no_disco_para_antes_de_comecar() {
        // The case that started this item: the batch must not begin only to find
        // out halfway that the disk ran out.
        // 10 GB estimated need 11 GB with the margin, and do not fit in 10 GB.
        let r = build_report(vec![ok("a", 10 * GB)], Some(10 * GB));
        assert_eq!(r.verdict, Verdict::Stop, "10GB + 10% does not fit in 10GB");

        // 9 GB need 9.9 GB and fit — the margin must not be so conservative
        // that it refuses a batch that fits.
        let r2 = build_report(vec![ok("a", 9 * GB)], Some(10 * GB));
        assert_eq!(r2.verdict, Verdict::Go, "9GB + 10% cabe em 10GB");
    }

    #[test]
    fn espaco_ilegivel_nao_bloqueia() {
        // Refusing the download because the disk read failed would trade a
        // possible problem for a certain one.
        let r = build_report(vec![ok("a", 500 * GB)], None);
        assert_eq!(r.verdict, Verdict::Go);
        assert_eq!(r.available_bytes, None);
    }

    #[test]
    fn item_sem_tamanho_estimado_nao_zera_a_conta() {
        // Live and some extractors report no size. It must not become 0 and
        // make a 50 GB batch look like it fits anywhere — but it also must
        // not invent a number. Count what it knows, and carry on.
        let sem_tamanho = ItemCheck {
            url: "live".into(),
            title: None,
            estimated_bytes: None,
            problem: None,
        };
        let r = build_report(vec![ok("a", GB), sem_tamanho], Some(10 * GB));
        assert_eq!(r.ready, 2);
        assert_eq!(r.estimated_bytes, GB);
        assert_eq!(r.verdict, Verdict::Go);
    }

    #[test]
    fn ja_baixado_conta_como_problema_e_nao_como_pronto() {
        let r = build_report(
            vec![ok("a", GB), bad("b", Problem::AlreadyHave)],
            Some(10 * GB),
        );
        assert_eq!(r.ready, 1);
        assert_eq!(r.verdict, Verdict::GoWithSkips);
    }

    #[test]
    fn so_falta_de_auth_e_acionavel_pelo_usuario() {
        // Guides the copy: "import your cookies" resolves it, "no downloader
        // covers this site" does not.
        assert!(Problem::NeedsAuth.is_actionable());
        for p in [
            Problem::Unsupported,
            Problem::NotFound,
            Problem::AlreadyHave,
        ] {
            assert!(!p.is_actionable(), "{p:?}");
        }
    }

    #[test]
    fn lote_vazio_para_em_vez_de_liberar() {
        let r = build_report(vec![], Some(GB));
        assert_eq!(r.verdict, Verdict::Stop);
        assert_eq!(r.total, 0);
    }

    #[test]
    fn espaco_livre_do_volume_atual_e_legivel() {
        // Asserts no number — only that the read works and does not return zero
        // in the temp directory, which would indicate a silent error.
        let livre = available_space(&std::env::temp_dir());
        assert!(livre.is_some(), "could not read free space");
        assert!(livre.unwrap() > 0);
    }
}
