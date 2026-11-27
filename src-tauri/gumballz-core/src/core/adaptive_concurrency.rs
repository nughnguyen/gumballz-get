//! Adjusts `-N` per host by measuring real throughput, instead of trusting a
//! fixed number in Config.
//!
//! The number in Config is a guess the user has to make without information: too
//! high gets 429s, too low wastes bandwidth, and the right value depends on the
//! host and the moment. The per-host limiter and the 429 counter already exist — what
//! was missing was closing the loop.
//!
//! It became possible now because B23 made the 429 brake really work:
//! before, `-N` and `--concurrent-fragments` fought each other and the effective value was not
//! what the code thought, so there was no reliable signal to feed back.
//!
//! The decision is pure: given throughput samples and the 429 history, what
//! should the next `-N` be. Measuring is I/O.
//!
//! Origin: CDN congestion control — climb slowly, drop fast.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Tuning limits. A host never leaves this range, no matter what the measurements
/// suggest: the ceiling protects the server and the floor keeps the download moving.
pub const MIN_N: u32 = 1;
pub const MAX_N: u32 = 16;

/// Connection budget for the whole machine, summing all active downloads.
/// The 2/4/8-download 1 GiB loopback stress test (study 66)
/// saturated above 32 connections: 4 downloads × 8 segments yielded 3.9 GB/s and
/// 8 × 8 dropped to 1.9 GB/s. Twenty-four leaves 3 downloads with a full `-N 8`
/// and shares the rest when the queue fills up.
pub const GLOBAL_CONNECTION_BUDGET: u32 = 24;

static ACTIVE_DOWNLOADS: AtomicU32 = AtomicU32::new(0);

/// How many downloads are pulling bytes right now (yt-dlp or direct fetcher).
pub fn active_downloads() -> u32 {
    ACTIVE_DOWNLOADS.load(Ordering::Relaxed)
}

/// Marks a download as active while the guard lives.
pub struct ActiveDownloadGuard(());

impl ActiveDownloadGuard {
    pub fn new() -> Self {
        ACTIVE_DOWNLOADS.fetch_add(1, Ordering::Relaxed);
        Self(())
    }
}

impl Default for ActiveDownloadGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ActiveDownloadGuard {
    fn drop(&mut self) {
        ACTIVE_DOWNLOADS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Per-download ceiling given the global budget and how many downloads share it.
///
/// `active` includes the download asking. Never below `MIN_N`:
/// a download always moves, even when the queue is full.
pub fn per_download_ceiling(budget: u32, active: u32) -> u32 {
    (budget / active.max(1)).clamp(MIN_N, MAX_N)
}

/// Minimum gain to justify climbing further.
///
/// Ten percent: below that the difference blends into normal network
/// variation, and raising `-N` chasing noise only increases the chance of 429.
pub const MEANINGFUL_GAIN: f64 = 1.10;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Sample {
    pub concurrency: u32,
    /// Bytes per second observed at this concurrency.
    pub throughput_bps: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HostStats {
    samples: Vec<Sample>,
    rate_limit_hits: u32,
}

impl HostStats {
    pub fn record(&mut self, concurrency: u32, throughput_bps: f64) {
        if throughput_bps.is_finite() && throughput_bps > 0.0 {
            self.samples.push(Sample {
                concurrency,
                throughput_bps,
            });
        }
    }

    pub fn record_rate_limit(&mut self) {
        self.rate_limit_hits = self.rate_limit_hits.saturating_add(1);
    }

    /// Best throughput observed for a given concurrency.
    ///
    /// Best, not average: a bad sample is usually local interference
    /// (another download, wifi), not the real capacity of that level.
    fn best_at(&self, concurrency: u32) -> Option<f64> {
        self.samples
            .iter()
            .filter(|s| s.concurrency == concurrency)
            .map(|s| s.throughput_bps)
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a| a.max(v)))
            })
    }
}

/// Next `-N` for the host.
///
/// Climbs slowly and drops fast, which is the right asymmetry: climbing wrong costs a
/// 429 and a retry; dropping wrong costs a little speed.
pub fn next_concurrency(stats: &HostStats, current: u32) -> u32 {
    let current = current.clamp(MIN_N, MAX_N);

    // 429 dominates any speed measurement. No point in throughput
    // looking good if the host is refusing.
    if stats.rate_limit_hits > 0 {
        let penalized = current / 2u32.pow(stats.rate_limit_hits.min(3));
        return penalized.clamp(MIN_N, MAX_N);
    }

    let Some(current_best) = stats.best_at(current) else {
        return current; // no measurement yet: do not touch
    };

    match stats.best_at(current.saturating_sub(1)) {
        // If one level below was better, climbing was a mistake — go down.
        Some(previous) if previous > current_best * MEANINGFUL_GAIN => (current - 1).clamp(MIN_N, MAX_N),
        _ => {
            // Only climb if the previous step showed real gain.
            let worth_climbing = match stats.best_at(current.saturating_sub(1)) {
                Some(previous) => current_best > previous * MEANINGFUL_GAIN,
                None => true, // first measured step: try climbing once
            };
            if worth_climbing {
                (current + 1).clamp(MIN_N, MAX_N)
            } else {
                current
            }
        }
    }
}

/// Per-host state, so the tuning survives between downloads.
#[derive(Debug, Default)]
pub struct ConcurrencyTuner {
    hosts: HashMap<String, HostStats>,
}

impl ConcurrencyTuner {
    pub fn record(&mut self, host: &str, concurrency: u32, throughput_bps: f64) {
        self.hosts
            .entry(host.to_string())
            .or_default()
            .record(concurrency, throughput_bps);
    }

    pub fn record_rate_limit(&mut self, host: &str) {
        self.hosts
            .entry(host.to_string())
            .or_default()
            .record_rate_limit();
    }

    /// `ceiling` is the Config number: it becomes a **ceiling**, not a fixed value. The ceiling
    /// is still cut by the global connection budget shared between active
    /// downloads (`GLOBAL_CONNECTION_BUDGET`).
    pub fn suggest(&self, host: &str, current: u32, ceiling: u32) -> u32 {
        let shared = per_download_ceiling(GLOBAL_CONNECTION_BUDGET, active_downloads());
        self.suggest_with_ceiling(host, current, ceiling.min(shared))
    }

    /// `suggest` without the global budget: the per-host rule, pure.
    pub fn suggest_with_ceiling(&self, host: &str, current: u32, ceiling: u32) -> u32 {
        let cap = ceiling.clamp(MIN_N, MAX_N);
        match self.hosts.get(host) {
            Some(stats) => next_concurrency(stats, current).min(cap),
            None => current.min(cap),
        }
    }
}

/// One tuner per process, shared between the direct downloader and yt-dlp.
///
/// The host is the same on both paths (a 429 from `googlevideo.com` counts for
/// either of them), so sharing the statistics is the right behavior.
/// `tokio::sync::Mutex` because it is locked inside async code.
pub fn global() -> &'static tokio::sync::Mutex<ConcurrencyTuner> {
    static TUNER: std::sync::OnceLock<tokio::sync::Mutex<ConcurrencyTuner>> =
        std::sync::OnceLock::new();
    TUNER.get_or_init(|| tokio::sync::Mutex::new(ConcurrencyTuner::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: f64 = 1_000_000.0;

    #[test]
    fn global_budget_is_shared_between_active_downloads() {
        assert_eq!(per_download_ceiling(24, 0), 16); // MAX_N
        assert_eq!(per_download_ceiling(24, 1), 16);
        assert_eq!(per_download_ceiling(24, 3), 8);
        assert_eq!(per_download_ceiling(24, 6), 4);
        assert_eq!(per_download_ceiling(24, 100), 1); // never below MIN_N
    }

    #[test]
    fn guard_counts_and_uncounts() {
        let before = active_downloads();
        {
            let _a = ActiveDownloadGuard::new();
            let _b = ActiveDownloadGuard::new();
            assert_eq!(active_downloads(), before + 2);
        }
        assert_eq!(active_downloads(), before);
    }

    #[test]
    fn no_measurement_at_all_means_no_change() {
        // Tuning without data would be guessing, which is what the feature came to avoid.
        let s = HostStats::default();
        assert_eq!(next_concurrency(&s, 4), 4);
    }

    #[test]
    fn climbs_when_the_previous_step_showed_real_gain() {
        let mut s = HostStats::default();
        s.record(3, 3.0 * MB);
        s.record(4, 5.0 * MB); // +66%, well above the noise
        assert_eq!(next_concurrency(&s, 4), 5);
    }

    #[test]
    fn does_not_climb_chasing_noise() {
        // A 5% difference blends into normal network variation; climbing for
        // that only increases the chance of 429 without gaining speed.
        let mut s = HostStats::default();
        s.record(3, 5.0 * MB);
        s.record(4, 5.25 * MB);
        assert_eq!(next_concurrency(&s, 4), 4);
    }

    #[test]
    fn drops_when_climbing_made_it_worse() {
        let mut s = HostStats::default();
        s.record(3, 8.0 * MB);
        s.record(4, 5.0 * MB); // climbing cost speed
        assert_eq!(next_concurrency(&s, 4), 3);
    }

    #[test]
    fn rate_limit_drops_fast_and_ignores_throughput() {
        // Deliberate asymmetry: climbing wrong costs a 429 and a retry, dropping wrong
        // costs a little speed. And good throughput means nothing if
        // the host is refusing.
        let mut s = HostStats::default();
        s.record(8, 50.0 * MB);
        s.record_rate_limit();
        assert_eq!(next_concurrency(&s, 8), 4);

        s.record_rate_limit();
        assert_eq!(next_concurrency(&s, 8), 2);

        s.record_rate_limit();
        assert_eq!(next_concurrency(&s, 8), 1);
    }

    #[test]
    fn never_leaves_the_range() {
        let mut s = HostStats::default();
        s.record(16, 100.0 * MB);
        s.record(15, 10.0 * MB);
        assert_eq!(next_concurrency(&s, 16), MAX_N, "does not exceed the ceiling");

        let mut low = HostStats::default();
        for _ in 0..10 {
            low.record_rate_limit();
        }
        assert_eq!(next_concurrency(&low, 1), MIN_N, "does not go below 1");
    }

    #[test]
    fn bad_sample_does_not_erase_the_good_one() {
        // Local interference (another download, wifi) is not the real capacity of
        // the level. That is why the best, not the average.
        let mut s = HostStats::default();
        s.record(3, 3.0 * MB);
        s.record(4, 5.0 * MB);
        s.record(4, 0.1 * MB); // interference spike
        assert_eq!(next_concurrency(&s, 4), 5, "the bad sample must not rule");
    }

    #[test]
    fn invalid_measurement_is_discarded() {
        let mut s = HostStats::default();
        s.record(4, f64::NAN);
        s.record(4, -1.0);
        s.record(4, 0.0);
        assert_eq!(next_concurrency(&s, 4), 4, "without a valid sample, do not touch");
    }

    #[test]
    fn the_config_number_becomes_a_ceiling_not_a_fixed_value() {
        // The point of the feature: the user stops guessing a number and starts
        // declaring a limit.
        let mut t = ConcurrencyTuner::default();
        t.record("youtube.com", 3, 3.0 * MB);
        t.record("youtube.com", 4, 6.0 * MB);
        assert_eq!(t.suggest("youtube.com", 4, 16), 5, "below the ceiling, climb");
        assert_eq!(t.suggest("youtube.com", 4, 4), 4, "the ceiling holds");
    }

    #[test]
    fn hosts_do_not_contaminate_each_other() {
        // A YouTube 429 must not lower Bilibili's concurrency.
        let mut t = ConcurrencyTuner::default();
        t.record_rate_limit("youtube.com");
        assert_eq!(t.suggest("youtube.com", 8, 16), 4);
        assert_eq!(t.suggest("bilibili.com", 8, 16), 8);
    }
}
