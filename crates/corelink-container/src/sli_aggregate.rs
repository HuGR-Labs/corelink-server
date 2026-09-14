//! Constant-memory SLI sink for the deployed CAS + AC storage handlers.
//!
//! # Why this exists (B-057)
//!
//! `corelink_handler_cas::InMemorySliObserver` — and its AC twin — is
//! documented as a "capture-everything in-process observer for tests +
//! apps/server wire-up", and it does exactly that: it pushes every
//! [`corelink_handler_cas::SliObservation`] into a `Mutex<Vec<..>>` and
//! keeps it. The deployed builders in [`crate::storage::r2_s3`] wired
//! that type, and both handlers are constructed ONCE at process start
//! (`routes/cas.rs::build_handlers`, called from `routes.rs`, and again
//! from `routes/public_mirror.rs` for the `_public` moat). So the Vec
//! grew monotonically for the life of the container, and nothing in
//! production ever called `snapshot()` or `count()` — every caller of
//! either is a test.
//!
//! Retaining the observations was never necessary. The evaluator this
//! feeds, `corelink_slo::BurnRateCalculator`, consumes a
//! `BurnRateSample { errors, total }` per window — a pair of counts,
//! not a log of events. So this keeps a bounded five-minute bucket ring per
//! [`Sli`] variant, retaining at most the canonical three-day range regardless
//! of traffic.
//!
//! # Why it lives in the container and not in the handler crates
//!
//! It emits `tracing`, and the handler crates are deliberately free of
//! that dependency. Implementing BOTH handler traits here keeps them
//! that way, and puts the sink next to the code that decides what
//! production wiring means.
//!
//! # Consumer
//!
//! Every bounded aggregate is consumed by the canonical
//! `corelink_slo::BurnRateCalculator` and its structured decision is emitted
//! on the tracing stream. This keeps the SLO decision path local and
//! fail-open for request handling; a log/metrics collector can route a
//! non-quiet decision to the operational alert channel without retaining
//! request-level observations in the container.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use corelink_slo::definition::Sli;
use corelink_slo::{
    canonical_burn_rate_windows, AlertDecision, BurnRateCalculator, BurnRateSample, BurnRateWindow,
    SloDefinition,
};

/// How many observations of one SLI pass before its aggregate is
/// logged.
///
/// A power of two keeps the modulo cheap; 256 is frequent enough to
/// watch a burn develop and rare enough that the log is not itself a
/// hot path.
const LOG_EVERY: u64 = 256;

/// Five-minute buckets retain enough resolution for every canonical range
/// vector while keeping the in-process ring bounded to three days.
const BUCKET_MS: u64 = 5 * 60 * 1_000;
const MAX_WINDOW_MS: u64 = 3 * 24 * 60 * 60 * 1_000;
const MAX_BUCKETS: usize = (MAX_WINDOW_MS / BUCKET_MS) as usize + 1;

/// Per-SLI aggregate: exactly what `corelink_slo::BurnRateCalculator`
/// consumes, plus the latency summary the p99 SLIs need.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliCounters {
    /// Observations recorded for this SLI.
    pub total: u64,
    /// Of those, how many were errors — the burn-rate numerator.
    pub errors: u64,
    /// Sum of observed latencies, microseconds.
    pub latency_us_sum: u64,
    /// Largest single observed latency, microseconds.
    pub latency_us_max: u64,
}

impl SliCounters {
    /// Fold one observation in.
    fn record(&mut self, is_error: bool, latency_us: u64) {
        self.total = self.total.saturating_add(1);
        if is_error {
            self.errors = self.errors.saturating_add(1);
        }
        self.latency_us_sum = self.latency_us_sum.saturating_add(latency_us);
        self.latency_us_max = self.latency_us_max.max(latency_us);
    }

    fn merge(&mut self, other: Self) {
        self.total = self.total.saturating_add(other.total);
        self.errors = self.errors.saturating_add(other.errors);
        self.latency_us_sum = self.latency_us_sum.saturating_add(other.latency_us_sum);
        self.latency_us_max = self.latency_us_max.max(other.latency_us_max);
    }
}

#[derive(Debug, Default)]
struct TimedBucket {
    start_ms: u64,
    counters: BTreeMap<Sli, SliCounters>,
}

#[derive(Debug, Default)]
struct WindowedCounters {
    buckets: VecDeque<TimedBucket>,
}

impl WindowedCounters {
    fn evict(&mut self, now_ms: u64) {
        while self
            .buckets
            .front()
            .is_some_and(|bucket| now_ms.saturating_sub(bucket.start_ms) >= MAX_WINDOW_MS)
        {
            self.buckets.pop_front();
        }
        while self.buckets.len() > MAX_BUCKETS {
            self.buckets.pop_front();
        }
    }

    fn record_at(&mut self, now_ms: u64, sli: Sli, is_error: bool, latency_us: u64) -> SliCounters {
        let bucket_start = now_ms / BUCKET_MS * BUCKET_MS;
        if self
            .buckets
            .back()
            .is_none_or(|bucket| bucket.start_ms != bucket_start)
        {
            self.buckets.push_back(TimedBucket {
                start_ms: bucket_start,
                counters: BTreeMap::new(),
            });
        }
        self.evict(now_ms);
        let Some(bucket) = self.buckets.back_mut() else {
            return SliCounters::default();
        };
        let counters = bucket.counters.entry(sli).or_default();
        counters.record(is_error, latency_us);
        *counters
    }

    fn counters_at(&self, now_ms: u64) -> BTreeMap<Sli, SliCounters> {
        let mut result: BTreeMap<Sli, SliCounters> = BTreeMap::new();
        for bucket in &self.buckets {
            if now_ms.saturating_sub(bucket.start_ms) >= MAX_WINDOW_MS {
                continue;
            }
            for (sli, counters) in &bucket.counters {
                result.entry(*sli).or_default().merge(*counters);
            }
        }
        result
    }

    fn window_counters_at(&self, sli: Sli, now_ms: u64, window: BurnRateWindow) -> SliCounters {
        let duration_ms = window.window_duration_seconds().saturating_mul(1_000);
        let mut result = SliCounters::default();
        for bucket in &self.buckets {
            if now_ms.saturating_sub(bucket.start_ms) < duration_ms {
                if let Some(counters) = bucket.counters.get(&sli) {
                    result.merge(*counters);
                }
            }
        }
        result
    }
}

/// The production SLO targets for the CAS/AC stream. Latency observations are
/// retained as latency summaries, not treated as availability errors.
fn availability_target(sli: Sli) -> Option<f64> {
    match sli {
        Sli::AvailCasGet | Sli::AvailCasPut | Sli::AvailAcLookup => Some(0.999),
        Sli::CorrectnessCas => Some(1.0),
        _ => None,
    }
}

/// Consume one bounded aggregate with the canonical burn-rate calculator.
///
/// This is deliberately pure and synchronous: the observer's hot path can
/// evaluate a sample without starting a task or depending on a remote alert
/// service. The caller chooses the evaluation window.
#[must_use]
pub fn evaluate_burn_rate(
    sli: Sli,
    counters: SliCounters,
    window: BurnRateWindow,
) -> Option<AlertDecision> {
    let target = availability_target(sli)?;
    let slo = SloDefinition::new(sli, target).ok()?;
    let sample = BurnRateSample::new(counters.errors, counters.total);
    Some(BurnRateCalculator::new().decide(slo, window, sample))
}

/// Constant-memory SLI observer shared by the CAS and AC handlers.
#[derive(Debug, Default)]
pub struct CountingSliObserver {
    counters: Mutex<WindowedCounters>,
}

impl CountingSliObserver {
    /// Construct an observer with every counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current counters for every SLI observed at least once.
    ///
    /// # Errors
    ///
    /// Returns `"sli observer poisoned"` if the internal lock is
    /// poisoned.
    pub fn counters(&self) -> Result<BTreeMap<Sli, SliCounters>, String> {
        self.counters
            .lock()
            .map(|g| g.counters_at(now_ms()))
            .map_err(|_| "sli observer poisoned".to_string())
    }

    #[cfg(test)]
    fn counters_at(&self, now_ms: u64) -> Result<BTreeMap<Sli, SliCounters>, String> {
        self.counters
            .lock()
            .map(|g| g.counters_at(now_ms))
            .map_err(|_| "sli observer poisoned".to_string())
    }

    #[cfg(test)]
    fn window_counters_at(
        &self,
        sli: Sli,
        now_ms: u64,
        window: BurnRateWindow,
    ) -> Result<SliCounters, String> {
        self.counters
            .lock()
            .map(|g| g.window_counters_at(sli, now_ms, window))
            .map_err(|_| "sli observer poisoned".to_string())
    }

    /// Fold one observation in and, every [`LOG_EVERY`] observations of
    /// that SLI, publish the aggregate.
    ///
    /// Best-effort on a poisoned lock, mirroring the observer it
    /// replaces: a prior caller panicked, and dropping an observation
    /// is strictly better than taking the data plane down for it.
    fn record(&self, sli: Sli, is_error: bool, latency_us: u64) {
        self.record_at(sli, is_error, latency_us, now_ms());
    }

    fn record_at(&self, sli: Sli, is_error: bool, latency_us: u64, now: u64) {
        let snapshot = {
            let Ok(mut g) = self.counters.lock() else {
                return;
            };
            let c = g.record_at(now, sli, is_error, latency_us);
            if c.total % LOG_EVERY == 0 {
                Some((c, g.window_counters_at(sli, now, BurnRateWindow::Fast1h)))
            } else {
                None
            }
        };
        // The guard is dropped above: the tracing subscriber is out of
        // our control and must never run while this lock is held.
        if let Some((s, fast)) = snapshot {
            for window in canonical_burn_rate_windows() {
                let sample = if *window == BurnRateWindow::Fast1h {
                    fast
                } else {
                    let Ok(g) = self.counters.lock() else { return };
                    g.window_counters_at(sli, now, *window)
                };
                if let Some(decision) = evaluate_burn_rate(sli, sample, *window) {
                    tracing::info!(
                        sli = ?sli,
                        window = %window,
                        decision = decision.slug(),
                        total = sample.total,
                        errors = sample.errors,
                        "sli burn-rate evaluation"
                    );
                }
            }
            tracing::info!(
                sli = ?sli,
                total = fast.total,
                errors = fast.errors,
                latency_us_sum = s.latency_us_sum,
                latency_us_max = s.latency_us_max,
                "sli aggregate"
            );
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// The ONE observer the whole process shares.
///
/// The CAS handler, the AC handler and the edge's audit-emit route
/// (`routes/audit_cas_attempted`) must all fold into the SAME counters:
/// `AvailCasGet` served by the container and `AvailCasGet` served in-colo
/// by the Worker are the same SLI, and two disjoint observers would give
/// two partial views of one SLO — which is the shape of blindness B-055
/// exists to remove, not a new one to add.
static SHARED: OnceLock<Arc<CountingSliObserver>> = OnceLock::new();

/// Handle to the process-wide observer, created on first use.
#[must_use]
pub fn shared() -> Arc<CountingSliObserver> {
    Arc::clone(SHARED.get_or_init(|| Arc::new(CountingSliObserver::new())))
}

impl corelink_handler_cas::SliObserver for CountingSliObserver {
    fn observe(&self, obs: corelink_handler_cas::SliObservation) {
        self.record(obs.sli, obs.is_error, obs.latency_us);
    }
}

impl corelink_handler_ac::SliObserver for CountingSliObserver {
    fn observe(&self, obs: corelink_handler_ac::SliObservation) {
        self.record(obs.sli, obs.is_error, obs.latency_us);
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_burn_rate, CountingSliObserver, LOG_EVERY};
    use corelink_handler_cas::{SliObservation, SliObserver as _};
    use corelink_slo::definition::Sli;

    /// The whole point of B-057's third defect: memory must not track
    /// traffic. The observer this replaced pushed every observation
    /// into a `Vec` that was never drained, on a handler built once at
    /// process start.
    #[test]
    fn memory_is_bounded_by_the_taxonomy_not_by_traffic() {
        let obs = CountingSliObserver::new();
        for i in 0..10_000_u64 {
            obs.observe(SliObservation::new(Sli::AvailCasGet, i % 7 == 0, i));
            obs.observe(SliObservation::new(Sli::AvailCasPut, false, i));
        }
        let c = obs.counters().unwrap_or_default();
        assert_eq!(
            c.len(),
            2,
            "one entry per DISTINCT Sli observed, never per observation"
        );
        let get = c.get(&Sli::AvailCasGet).copied().unwrap_or_default();
        assert_eq!(get.total, 10_000);
        assert_eq!(
            get.errors, 1429,
            "errors is the burn-rate numerator, counted not stored"
        );
    }

    /// `latency_us_sum` / `latency_us_max` must carry what the callers
    /// measured — the half that stays broken if only the transport is
    /// fixed.
    #[test]
    fn latency_is_summed_and_maxed() {
        let obs = CountingSliObserver::new();
        for us in [5_u64, 900, 12] {
            obs.observe(SliObservation::new(Sli::LatencyCasGetP99, false, us));
        }
        let c = obs
            .counters()
            .unwrap_or_default()
            .get(&Sli::LatencyCasGetP99)
            .copied()
            .unwrap_or_default();
        assert_eq!(c.total, 3);
        assert_eq!(c.errors, 0);
        assert_eq!(c.latency_us_sum, 917);
        assert_eq!(c.latency_us_max, 900);
    }

    /// A zero-latency observation still counts toward `total`, so an
    /// operator can tell "fast" from "never measured" by comparing the
    /// sum against the count — the exact confusion that let
    /// `latency_us: 0` survive at every call site.
    #[test]
    fn zero_latency_still_counts_as_an_observation() {
        let obs = CountingSliObserver::new();
        obs.observe(SliObservation::new(Sli::AvailAcLookup, false, 0));
        let c = obs
            .counters()
            .unwrap_or_default()
            .get(&Sli::AvailAcLookup)
            .copied()
            .unwrap_or_default();
        assert_eq!(c.total, 1);
        assert_eq!(c.latency_us_sum, 0);
    }

    /// The aggregate is published on the tracing stream every
    /// `LOG_EVERY` observations. Pinned as a CONSTANT so a future edit
    /// that silences the only reader has to say so out loud.
    #[test]
    fn log_cadence_is_a_power_of_two() {
        assert!(LOG_EVERY.is_power_of_two(), "cheap modulo");
        assert_eq!(LOG_EVERY, 256);
    }

    #[test]
    fn bounded_aggregate_has_a_real_burn_rate_consumer() {
        let decision = evaluate_burn_rate(
            Sli::AvailCasGet,
            super::SliCounters {
                total: 1_000,
                errors: 20,
                latency_us_sum: 0,
                latency_us_max: 0,
            },
            corelink_slo::BurnRateWindow::Fast1h,
        );
        assert_eq!(decision, Some(corelink_slo::AlertDecision::PageSev0));
    }

    #[test]
    fn latency_slis_are_not_misclassified_as_error_burn() {
        let decision = evaluate_burn_rate(
            Sli::LatencyCasGetP99,
            super::SliCounters {
                total: 1_000,
                errors: 20,
                latency_us_sum: 10_000,
                latency_us_max: 50,
            },
            corelink_slo::BurnRateWindow::Fast1h,
        );
        assert_eq!(decision, None);
    }

    #[test]
    fn time_advance_uses_bounded_windows_and_evicts_old_buckets() {
        let obs = CountingSliObserver::new();
        let t0 = super::BUCKET_MS;
        obs.record_at(Sli::AvailCasGet, true, 10, t0);
        obs.record_at(Sli::AvailCasGet, false, 10, t0 + 2 * 60 * 60 * 1_000);

        let fast = obs
            .window_counters_at(
                Sli::AvailCasGet,
                t0 + 2 * 60 * 60 * 1_000,
                corelink_slo::BurnRateWindow::Fast1h,
            )
            .unwrap_or_default();
        let medium = obs
            .window_counters_at(
                Sli::AvailCasGet,
                t0 + 2 * 60 * 60 * 1_000,
                corelink_slo::BurnRateWindow::Medium6h,
            )
            .unwrap_or_default();
        assert_eq!(fast.errors, 0, "the old error is outside the 1h range");
        assert_eq!(
            medium.errors, 1,
            "the old error remains inside the 6h range"
        );

        let future = t0 + 4 * 24 * 60 * 60 * 1_000;
        obs.record_at(Sli::AvailCasGet, false, 10, future);
        let retained = obs.counters_at(future).unwrap_or_default();
        let retained_get = retained.get(&Sli::AvailCasGet).copied().unwrap_or_default();
        assert_eq!(retained_get.errors, 0);
        assert!(retained_get.total <= 1);
    }

    #[test]
    fn one_error_is_diluted_by_successes_inside_the_same_window() {
        let obs = CountingSliObserver::new();
        let t0 = super::BUCKET_MS;
        obs.record_at(Sli::AvailCasGet, true, 1, t0);
        for _ in 0..999 {
            obs.record_at(Sli::AvailCasGet, false, 1, t0 + 1_000);
        }
        let sample = obs
            .window_counters_at(
                Sli::AvailCasGet,
                t0 + 1_000,
                corelink_slo::BurnRateWindow::Fast1h,
            )
            .unwrap_or_default();
        assert_eq!(sample.total, 1_000);
        assert_eq!(sample.errors, 1);
        assert_eq!(
            evaluate_burn_rate(
                Sli::AvailCasGet,
                sample,
                corelink_slo::BurnRateWindow::Fast1h,
            ),
            Some(corelink_slo::AlertDecision::Quiet)
        );
    }

    #[test]
    fn counters_at_merges_each_sli_value_without_cross_keying() {
        let obs = CountingSliObserver::new();
        let t0 = super::BUCKET_MS;
        obs.record_at(Sli::AvailCasGet, true, 11, t0);
        obs.record_at(Sli::AvailCasGet, false, 29, t0 + super::BUCKET_MS);
        obs.record_at(Sli::AvailCasPut, false, 7, t0 + super::BUCKET_MS);

        let counters = obs.counters_at(t0 + super::BUCKET_MS).unwrap_or_default();
        assert_eq!(
            counters.get(&Sli::AvailCasGet).copied(),
            Some(super::SliCounters {
                total: 2,
                errors: 1,
                latency_us_sum: 40,
                latency_us_max: 29,
            })
        );
        assert_eq!(
            counters.get(&Sli::AvailCasPut).copied(),
            Some(super::SliCounters {
                total: 1,
                errors: 0,
                latency_us_sum: 7,
                latency_us_max: 7,
            })
        );
        assert_eq!(counters.len(), 2);
    }
}
