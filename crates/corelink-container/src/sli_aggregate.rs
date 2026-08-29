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
//! not a log of events. So this keeps one [`SliCounters`] per `Sli`
//! variant, in a map bounded by the closed `Sli` taxonomy (18 variants
//! today) regardless of traffic, forever.
//!
//! # Why it lives in the container and not in the handler crates
//!
//! It emits `tracing`, and the handler crates are deliberately free of
//! that dependency. Implementing BOTH handler traits here keeps them
//! that way, and puts the sink next to the code that decides what
//! production wiring means.
//!
//! # What this does NOT do
//!
//! It does not close the SLO loop. Nothing yet reads these counters to
//! compute a burn rate or raise an alert; the periodic log line below
//! is what makes them observable at all, which is strictly more than
//! the zero readers they had. Wiring `BurnRateCalculator` + alerting is
//! tracked separately.

use std::collections::BTreeMap;
use std::sync::Mutex;

use corelink_slo::definition::Sli;

/// How many observations of one SLI pass before its aggregate is
/// logged.
///
/// A power of two keeps the modulo cheap; 256 is frequent enough to
/// watch a burn develop and rare enough that the log is not itself a
/// hot path.
const LOG_EVERY: u64 = 256;

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
}

/// Constant-memory SLI observer shared by the CAS and AC handlers.
#[derive(Debug, Default)]
pub struct CountingSliObserver {
    counters: Mutex<BTreeMap<Sli, SliCounters>>,
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
            .map(|g| g.clone())
            .map_err(|_| "sli observer poisoned".to_string())
    }

    /// Fold one observation in and, every [`LOG_EVERY`] observations of
    /// that SLI, publish the aggregate.
    ///
    /// Best-effort on a poisoned lock, mirroring the observer it
    /// replaces: a prior caller panicked, and dropping an observation
    /// is strictly better than taking the data plane down for it.
    fn record(&self, sli: Sli, is_error: bool, latency_us: u64) {
        let snapshot = {
            let Ok(mut g) = self.counters.lock() else {
                return;
            };
            let c = g.entry(sli).or_default();
            c.record(is_error, latency_us);
            if c.total % LOG_EVERY == 0 {
                Some(*c)
            } else {
                None
            }
        };
        // The guard is dropped above: the tracing subscriber is out of
        // our control and must never run while this lock is held.
        if let Some(s) = snapshot {
            tracing::info!(
                sli = ?sli,
                total = s.total,
                errors = s.errors,
                latency_us_sum = s.latency_us_sum,
                latency_us_max = s.latency_us_max,
                "sli aggregate"
            );
        }
    }
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
    use super::{CountingSliObserver, LOG_EVERY};
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
}
