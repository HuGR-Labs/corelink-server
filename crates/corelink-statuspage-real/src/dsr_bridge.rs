//! Wave-16 bridge between
//! `corelink-privacy-erasure-worker::statuspage_publish::DsrCompletionStats`
//! (pure aggregator) and this crate's
//! [`crate::report::DsrCompletionReport`] (Statuspage publish payload).
//!
//! The bridge lives here (not in the worker crate) so the worker
//! stays wasm32-clean (no `reqwest` dep creep). The publish job at
//! WI-S11-008 PRR ship gate calls
//! `aggregate_24h_window(&outcomes, window_start)` to produce the
//! aggregate + `bridge_to_report(&stats)` to produce the publish
//! payload + `StatuspageBackend::publish_dsr_metric(&report, now_ms)`
//! to fire the wire.

use corelink_privacy_erasure_worker::DsrCompletionStats;

use crate::report::{DsrCompletionReport, DsrCompletionReportError};

/// Convert a worker-emitted [`DsrCompletionStats`] aggregate into a
/// canonical Statuspage [`DsrCompletionReport`] publish payload.
///
/// # Errors
///
/// Returns [`DsrCompletionReportError`] if the aggregate window is
/// malformed or the p95 observation exceeds the canonical ceiling
/// (clock-skew guard).
pub fn bridge_to_report(
    stats: &DsrCompletionStats,
) -> Result<DsrCompletionReport, DsrCompletionReportError> {
    DsrCompletionReport::new(
        stats.window_start_unix_s,
        stats.window_end_unix_s,
        stats.verified_complete_count,
        stats.verified_partial_count,
        stats.sla_breached_count,
        stats.p95_resolution_hours,
    )
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
    fn bridge_preserves_canonical_fields() {
        let stats = DsrCompletionStats {
            window_start_unix_s: 1_700_000_000,
            window_end_unix_s: 1_700_086_400,
            verified_complete_count: 47,
            verified_partial_count: 2,
            sla_breached_count: 0,
            p95_resolution_hours: 18,
        };
        let report = bridge_to_report(&stats).unwrap();
        assert_eq!(report.window_start_unix_s, 1_700_000_000);
        assert_eq!(report.window_end_unix_s, 1_700_086_400);
        assert_eq!(report.verified_complete_count, 47);
        assert_eq!(report.verified_partial_count, 2);
        assert_eq!(report.sla_breached_count, 0);
        assert_eq!(report.p95_resolution_hours, 18);
        assert_eq!(report.total_resolved(), 49);
    }

    #[test]
    fn bridge_rejects_invalid_window() {
        let stats = DsrCompletionStats {
            window_start_unix_s: 0,
            window_end_unix_s: 1, // not a 24h window
            verified_complete_count: 0,
            verified_partial_count: 0,
            sla_breached_count: 0,
            p95_resolution_hours: 0,
        };
        let err = bridge_to_report(&stats).unwrap_err();
        let is_invalid = matches!(err, DsrCompletionReportError::InvalidWindow { .. });
        assert!(is_invalid);
    }

    #[test]
    fn empty_aggregate_bridges_to_zero_p95_report() {
        let stats = DsrCompletionStats {
            window_start_unix_s: 0,
            window_end_unix_s: 86_400,
            verified_complete_count: 0,
            verified_partial_count: 0,
            sla_breached_count: 0,
            p95_resolution_hours: 0,
        };
        let report = bridge_to_report(&stats).unwrap();
        assert_eq!(report.p95_resolution_hours, 0);
        assert_eq!(report.total_resolved(), 0);
    }
}
