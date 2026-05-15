//! Admin handler SLI observer surface.

use std::sync::Mutex;

pub use corelink_slo::definition::Sli;

/// One admin SLI observation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SliObservation {
    /// Canonical SLI being observed.
    pub sli: Sli,
    /// Whether the observation counts as an error.
    pub is_error: bool,
    /// Latency in microseconds.
    pub latency_us: u64,
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

    /// Snapshot.
    ///
    /// # Errors
    ///
    /// Lock-poisoning string.
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
    /// Lock-poisoning string.
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
