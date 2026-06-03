//! AC handler SLI observer surface.

use std::sync::Mutex;

pub use corelink_slo::definition::Sli;

/// One AC handler SLI observation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SliObservation {
    /// Canonical SLI being observed.
    pub sli: Sli,
    /// Whether the observation is an error toward the numerator.
    pub is_error: bool,
    /// Latency in microseconds.
    pub latency_us: u64,
}

impl SliObservation {
    /// Construct from fields. The struct is `#[non_exhaustive]` so this is the
    /// only out-of-crate construction path (required by out-of-crate AC handler
    /// impls like the R2-backed one in `corelink-container`).
    #[must_use]
    pub const fn new(sli: Sli, is_error: bool, latency_us: u64) -> Self {
        Self {
            sli,
            is_error,
            latency_us,
        }
    }
}

/// SLI observer trait (infallible).
pub trait SliObserver: Send + Sync + core::fmt::Debug {
    /// Record one observation.
    fn observe(&self, obs: SliObservation);
}

/// Capture-everything in-process observer.
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

    /// Snapshot of recorded observations.
    ///
    /// # Errors
    ///
    /// Returns a string on lock poisoning.
    pub fn snapshot(&self) -> Result<Vec<SliObservation>, String> {
        self.rows
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "sli observer poisoned".to_string())
    }

    /// Count observations matching `sli`.
    ///
    /// # Errors
    ///
    /// Returns a string on lock poisoning.
    pub fn count(&self, sli: Sli) -> Result<usize, String> {
        Ok(self.snapshot()?.iter().filter(|o| o.sli == sli).count())
    }
}

impl SliObserver for InMemorySliObserver {
    fn observe(&self, obs: SliObservation) {
        if let Ok(mut g) = self.rows.lock() {
            g.push(obs);
        }
    }
}
