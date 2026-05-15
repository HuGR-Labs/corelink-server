//! `D1RowSource` — trait surface for reading the canonical 24h slice
//! of [`corelink_privacy_erasure_worker::VerificationOutcome`] rows
//! from the canonical D1 `dsr_erasure_log` table.
//!
//! The production wiring at the CF Worker cron entry binds this
//! trait against a D1 prepared statement of the form
//!
//! ```sql
//! SELECT outcome_json
//! FROM dsr_erasure_log
//! WHERE verified_at_ms >= ?1 AND verified_at_ms < ?2;
//! ```
//!
//! and rehydrates each `outcome_json` row into a
//! [`VerificationOutcome`] (the row schema canonicalises every
//! decision arm's payload).

use corelink_privacy_erasure_worker::VerificationOutcome;
use thiserror::Error;

/// Trait surface for the canonical 24h D1 row source.
pub trait D1RowSource: core::fmt::Debug + Send + Sync {
    /// Fetch every [`VerificationOutcome`] whose verification
    /// timestamp falls in the half-open window
    /// `[window_start_unix_s, window_start_unix_s + 86_400)`.
    ///
    /// The implementation MUST NOT return rows outside the window —
    /// the canonical aggregator [`corelink_privacy_erasure_worker::
    /// aggregate_24h_window`] double-checks via the embedded report
    /// timestamp, but the trait contract is that this read returns
    /// the exact slice.
    ///
    /// # Errors
    ///
    /// Returns [`D1RowSourceError`] on any D1 read failure
    /// (transport, parse, etc).
    fn fetch_window(
        &self,
        window_start_unix_s: u64,
    ) -> Result<Vec<VerificationOutcome>, D1RowSourceError>;
}

/// Error variants for D1 row reads.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum D1RowSourceError {
    /// D1 read failed at the transport / query layer.
    #[error("d1 row source: {0}")]
    Read(String),
    /// D1 read succeeded but a row failed to parse as a canonical
    /// [`VerificationOutcome`]. Aborts the publish (fail-CLOSED).
    #[error("d1 row source: parse error: {0}")]
    Parse(String),
}

/// In-memory test fake — holds a vec of outcomes + filters per
/// window on each call. The fake honours the trait's half-open window
/// contract by re-applying the canonical
/// `[window_start, window_start + 86_400)` filter on every call.
#[derive(Clone, Debug, Default)]
pub struct InMemoryD1RowSource {
    rows: Vec<VerificationOutcome>,
    fail_next: bool,
}

impl InMemoryD1RowSource {
    /// Construct an empty source.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            fail_next: false,
        }
    }

    /// Construct from a pre-populated vec.
    #[must_use]
    pub fn with_rows(rows: Vec<VerificationOutcome>) -> Self {
        Self {
            rows,
            fail_next: false,
        }
    }

    /// Configure the fake to fail the next call with
    /// [`D1RowSourceError::Read`]. Used by chaos integration tests.
    #[must_use]
    pub fn with_read_failure() -> Self {
        Self {
            rows: Vec::new(),
            fail_next: true,
        }
    }
}

impl D1RowSource for InMemoryD1RowSource {
    fn fetch_window(
        &self,
        window_start_unix_s: u64,
    ) -> Result<Vec<VerificationOutcome>, D1RowSourceError> {
        if self.fail_next {
            return Err(D1RowSourceError::Read(
                "synthetic d1 read failure (test fake)".to_owned(),
            ));
        }
        let window_end_unix_s = window_start_unix_s.saturating_add(86_400);
        let filtered: Vec<VerificationOutcome> = self
            .rows
            .iter()
            .filter(|o| match o.report.as_ref() {
                Some(rep) => {
                    let ts_s = rep.verified_at_ms / 1_000;
                    ts_s >= window_start_unix_s && ts_s < window_end_unix_s
                }
                // SlaBreached has no signed report; the aggregator
                // counts it unconditionally — the publish job is
                // responsible for window-scoping in this arm. The
                // fake's "in-window" filter passes SLA-breach rows
                // through (the aggregator + counters match wave-16
                // semantics).
                None => true,
            })
            .cloned()
            .collect();
        Ok(filtered)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn empty_in_memory_returns_zero_rows() {
        let src = InMemoryD1RowSource::new();
        assert_eq!(src.fetch_window(1_700_000_000).unwrap().len(), 0);
    }

    #[test]
    fn read_failure_fake_propagates_error() {
        let src = InMemoryD1RowSource::with_read_failure();
        let err = src.fetch_window(1_700_000_000).unwrap_err();
        let is_read = matches!(err, D1RowSourceError::Read(_));
        assert!(is_read);
    }
}
