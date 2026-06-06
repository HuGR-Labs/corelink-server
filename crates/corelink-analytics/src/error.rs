//! Canonical error taxonomy for the `corelink-analytics` emit surface.
//!
//! All variants `#[non_exhaustive]` so additive growth lands without
//! breaking downstream `match` sites (Lote 10.6bis discipline).

use thiserror::Error;

use crate::audit::AnalyticsAuditSinkError;
use crate::observer::RedMetricsObserverError;

/// Canonical error surface returned by the [`crate::observer`] +
/// [`crate::validator`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AnalyticsError {
    /// Cardinality budget for a metric was exceeded; the emit was
    /// fail-closed (unique-tuple count would push the metric over its
    /// per-metric or global budget). The validator + observer surface
    /// this as the canonical SEV-2 alert source per WI §6.1.11.
    #[error(
        "cardinality budget exceeded: metric={metric}, observed={observed}, \
         budget={budget}, scope={scope}"
    )]
    CardinalityBudgetExceeded {
        /// Canonical metric name slug.
        metric: &'static str,
        /// Currently-observed unique tuple count for the metric.
        observed: u64,
        /// Configured cardinality budget for the metric (or global).
        budget: u64,
        /// `per_metric` or `global`.
        scope: &'static str,
    },

    /// Audit emit failure aborts the metric emit (fail-closed envelope
    /// per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    #[error("analytics audit emit failure: {0}")]
    Audit(#[from] AnalyticsAuditSinkError),

    /// Underlying metrics observer transport failure (production
    /// Cloudflare Workers Analytics Engine binding write
    /// `write_data_point` rejected; in-memory fake never returns this
    /// outside of the `FailingRedMetrics` adversarial fixture).
    #[error("analytics observer backend failure: {0}")]
    Observer(#[from] RedMetricsObserverError),

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// emit). Production wiring maps this to 503; the in-memory fake
    /// surfaces this only when adversarial tests artificially poison
    /// state.
    #[error("analytics internal state invariant violated: {0}")]
    Internal(String),
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
    fn cardinality_budget_exceeded_displays_diagnostic() {
        let e = AnalyticsError::CardinalityBudgetExceeded {
            metric: "corelink_cas_put_requests_total",
            observed: 20_001,
            budget: 20_000,
            scope: "per_metric",
        };
        let s = format!("{e}");
        assert!(s.contains("cardinality budget"));
        assert!(s.contains("20001"));
        assert!(s.contains("20000"));
        assert!(s.contains("per_metric"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = AnalyticsAuditSinkError::Store("induced".to_string());
        let e: AnalyticsError = inner.into();
        assert!(matches!(e, AnalyticsError::Audit(_)));
    }

    #[test]
    fn from_observer_error_lifts_cleanly() {
        let inner = RedMetricsObserverError::Backend("induced".to_string());
        let e: AnalyticsError = inner.into();
        assert!(matches!(e, AnalyticsError::Observer(_)));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = AnalyticsError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}
