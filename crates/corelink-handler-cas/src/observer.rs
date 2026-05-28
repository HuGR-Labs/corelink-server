//! `SliObserver` — handler-layer SLI emit point feeding the
//! `corelink-slo` multi-burn-rate alert evaluator.
//!
//! Each handler entry emits exactly one [`SliObservation`] per
//! relevant SLI before returning. The fake captures every
//! observation so tests can pin `INV-HANDLER-SLI-EMIT-ENTRY`.

use std::sync::Mutex;

pub use corelink_slo::definition::Sli;

/// One handler-layer SLI emission. The shape is shared across
/// CAS / AC / Admin handlers so a future `corelink-slo::Aggregator`
/// can read it uniformly.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SliObservation {
    /// Canonical SLI being observed.
    pub sli: Sli,
    /// Whether the observation counts as an "error" toward the SLI
    /// numerator (e.g. CAS GET 5xx, CAS PUT correctness violation).
    pub is_error: bool,
    /// Wall-clock latency of the handler entry, in microseconds.
    /// For non-latency SLIs the field is still populated so a
    /// histogram side-channel can ingest it for free.
    pub latency_us: u64,
}

impl SliObservation {
    /// Construct an [`SliObservation`] from its fields.
    ///
    /// Provided because the struct is `#[non_exhaustive]`, which
    /// prevents struct-literal construction from outside this crate.
    #[must_use]
    pub fn new(sli: Sli, is_error: bool, latency_us: u64) -> Self {
        Self {
            sli,
            is_error,
            latency_us,
        }
    }
}

/// Sink the handler emits SLI observations to. Production wiring
/// adapts this to the `corelink-slo::BurnRateCalculator` input
/// stream + prometheus histogram registry.
pub trait SliObserver: Send + Sync + core::fmt::Debug {
    /// Record one SLI observation. Infallible by design — observer
    /// failures MUST NOT take the handler down. Production wiring
    /// uses a lock-free ring buffer.
    fn observe(&self, obs: SliObservation);
}

/// Capture-everything in-process observer for tests + apps/server
/// wire-up.
#[derive(Debug, Default)]
pub struct InMemorySliObserver {
    rows: Mutex<Vec<SliObservation>>,
}

impl InMemorySliObserver {
    /// Construct an empty observer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all observations recorded so far (in emit order).
    ///
    /// # Errors
    ///
    /// Returns an `"sli observer poisoned"` string if the internal
    /// lock is poisoned.
    pub fn snapshot(&self) -> Result<Vec<SliObservation>, String> {
        self.rows
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "sli observer poisoned".to_string())
    }

    /// Count observations matching a specific `Sli`.
    ///
    /// # Errors
    ///
    /// Returns an `"sli observer poisoned"` string if the internal
    /// lock is poisoned.
    pub fn count(&self, sli: Sli) -> Result<usize, String> {
        Ok(self.snapshot()?.iter().filter(|o| o.sli == sli).count())
    }
}

impl SliObserver for InMemorySliObserver {
    fn observe(&self, obs: SliObservation) {
        // Best-effort: lock poisoning means a panic in a prior
        // observer caller; we drop the observation in that case
        // rather than panicking (forbid-panic discipline).
        if let Ok(mut g) = self.rows.lock() {
            g.push(obs);
        }
    }
}
