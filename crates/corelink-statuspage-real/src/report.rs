//! Canonical 24h-rolling DSR completion stats payload.
//!
//! The DSR completion report aggregates the canonical
//! `corelink_dsr_resolution_hours` histogram observations emitted by
//! `corelink-privacy-erasure-worker::verification_job` over a 24h
//! window into a single Statuspage Public-Metric data-point.
//!
//! Statuspage Public-Metric data shape (per Atlassian Statuspage API
//! v1): one `value` (the canonical observation we expose) + one
//! `timestamp` (Unix epoch seconds). The canonical observation we
//! publish is the **p95 resolution hours** over the 24h window — this
//! is the customer-visible "are erasures happening fast enough?"
//! number consistent with the SLO catalog §4.12 bucket bound `le=720`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 24h-rolling DSR completion stats summary.
///
/// Construct via [`DsrCompletionReport::new`] which enforces the
/// canonical invariants (window length, count consistency).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DsrCompletionReport {
    /// Unix epoch seconds — start of the canonical 24h aggregation
    /// window.
    pub window_start_unix_s: u64,
    /// Unix epoch seconds — end of the canonical 24h aggregation
    /// window (must equal `window_start_unix_s + 86_400`).
    pub window_end_unix_s: u64,
    /// Count of DSR tickets that resolved to
    /// `ErasureDecision::VerifiedComplete` in the window.
    pub verified_complete_count: u64,
    /// Count of DSR tickets that resolved to
    /// `ErasureDecision::VerifiedPartial` in the window.
    pub verified_partial_count: u64,
    /// Count of DSR tickets that resolved to
    /// `ErasureDecision::SlaBreached` in the window.
    pub sla_breached_count: u64,
    /// Canonical p95 observation of
    /// `corelink_dsr_resolution_hours` over the window. Bound to the
    /// 720h SLA window per slo_catalog §4.12.
    pub p95_resolution_hours: u64,
}

impl DsrCompletionReport {
    /// Construct + validate.
    ///
    /// # Errors
    ///
    /// - [`DsrCompletionReportError::InvalidWindow`] when the window
    ///   end does not equal start + 86_400.
    /// - [`DsrCompletionReportError::P95OutOfRange`] when the p95
    ///   observation exceeds the SLO catalog ceiling (canonical 720h
    ///   `le` bucket × 10 — a real observation above that is a clock-
    ///   skew bug, not a real DSR resolution).
    pub fn new(
        window_start_unix_s: u64,
        window_end_unix_s: u64,
        verified_complete_count: u64,
        verified_partial_count: u64,
        sla_breached_count: u64,
        p95_resolution_hours: u64,
    ) -> Result<Self, DsrCompletionReportError> {
        if window_end_unix_s != window_start_unix_s.saturating_add(86_400) {
            return Err(DsrCompletionReportError::InvalidWindow {
                start: window_start_unix_s,
                end: window_end_unix_s,
            });
        }
        if p95_resolution_hours > 7_200 {
            return Err(DsrCompletionReportError::P95OutOfRange {
                observed: p95_resolution_hours,
                ceiling: 7_200,
            });
        }
        Ok(Self {
            window_start_unix_s,
            window_end_unix_s,
            verified_complete_count,
            verified_partial_count,
            sla_breached_count,
            p95_resolution_hours,
        })
    }

    /// Total resolved tickets in the window (verified + partial + breached).
    #[must_use]
    pub const fn total_resolved(&self) -> u64 {
        self.verified_complete_count
            .saturating_add(self.verified_partial_count)
            .saturating_add(self.sla_breached_count)
    }

    /// Statuspage Public-Metric POST body. The canonical shape per
    /// Atlassian Statuspage API v1:
    /// `{"data": {"timestamp": <unix_s>, "value": <p95_hours>}}`.
    ///
    /// Statuspage expects a single numeric `value` per data point;
    /// auxiliary aggregates (counts) are tracked in the audit envelope
    /// + structured logging, not in the Statuspage data-point body.
    #[must_use]
    pub fn to_metric_body(&self) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "timestamp": self.window_end_unix_s,
                "value": self.p95_resolution_hours,
            }
        })
    }
}

/// Validation errors for [`DsrCompletionReport`].
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DsrCompletionReportError {
    /// The window end does not equal start + 86_400.
    #[error("invalid 24h window: start={start} end={end} (must be start+86_400)")]
    InvalidWindow {
        /// Canonical window start.
        start: u64,
        /// Canonical window end.
        end: u64,
    },
    /// p95 observation exceeds the canonical ceiling (clock-skew guard).
    #[error("p95 resolution hours {observed} exceeds ceiling {ceiling}")]
    P95OutOfRange {
        /// Observed value.
        observed: u64,
        /// Canonical ceiling.
        ceiling: u64,
    },
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
    fn new_accepts_canonical_24h_window() {
        let r = DsrCompletionReport::new(1_700_000_000, 1_700_086_400, 5, 1, 0, 12).unwrap();
        assert_eq!(r.total_resolved(), 6);
        assert_eq!(r.p95_resolution_hours, 12);
    }

    #[test]
    fn new_rejects_non_24h_window() {
        let err = DsrCompletionReport::new(0, 100, 0, 0, 0, 0).unwrap_err();
        let is_invalid = matches!(err, DsrCompletionReportError::InvalidWindow { .. });
        assert!(is_invalid);
    }

    #[test]
    fn new_rejects_p95_clock_skew() {
        let err = DsrCompletionReport::new(0, 86_400, 0, 0, 0, 10_000).unwrap_err();
        let is_oor = matches!(err, DsrCompletionReportError::P95OutOfRange { .. });
        assert!(is_oor);
    }

    #[test]
    fn metric_body_shape_matches_statuspage_api() {
        let r = DsrCompletionReport::new(0, 86_400, 5, 1, 0, 12).unwrap();
        let body = r.to_metric_body();
        let data = body.get("data").unwrap();
        assert_eq!(data.get("timestamp").unwrap().as_u64().unwrap(), 86_400);
        assert_eq!(data.get("value").unwrap().as_u64().unwrap(), 12);
    }

    #[test]
    fn total_resolved_saturates() {
        let r = DsrCompletionReport::new(0, 86_400, u64::MAX, 1, 0, 0).unwrap();
        assert_eq!(r.total_resolved(), u64::MAX);
    }
}
