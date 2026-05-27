//! Canonical error taxonomy for the `corelink-logpush` emit surface.
//!
//! All variants `#[non_exhaustive]` so additive growth lands without
//! breaking downstream `match` sites (Lote 10.6bis discipline).

use thiserror::Error;

/// Audit-sink failure surface (lifted into [`LogpushError::Audit`]).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LogAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("logpush audit sink store error: {0}")]
    Store(String),
}

/// Sink-side transport failure surface (lifted into [`LogpushError::Sink`]).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LogSinkError {
    /// Backend transport failure (Logpush ingest unavailable / R2 PUT
    /// failure / Loki push timeout). Production wiring records this
    /// as `corelink_logs_ingest_failures_total{reason}` SEV-3 alert
    /// per WI §6.1.10.
    #[error("logpush sink backend error: {0}")]
    Backend(String),
}

/// Canonical error surface returned by the [`crate::logpush::sink`] +
/// [`crate::logpush::redaction`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LogpushError {
    /// The redaction step failed (a pattern matched but the placeholder
    /// substitution would have produced an invalid serialized log).
    /// Production wiring maps this to a fail-closed drop (DEBUG/INFO
    /// dropped; ERROR/WARN escalates to SEV-3 alert).
    #[error("logpush redaction failure: {0}")]
    RedactionFailure(String),

    /// Audit emit failure aborts the log emit (fail-closed envelope
    /// per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    #[error("logpush audit emit failure: {0}")]
    Audit(#[from] LogAuditSinkError),

    /// Underlying log sink transport failure (production CF Logpush
    /// binding / R2 PUT / Loki push rejected; in-memory fake never
    /// returns this outside of the `FailingLogSink` adversarial
    /// fixture).
    #[error("logpush sink backend failure: {0}")]
    Sink(#[from] LogSinkError),

    /// Cardinality budget guard tripped: the log emit would have
    /// pushed the per-region log-event-type label-tuple count past
    /// the analytics cardinality budget (per WI §6.1.2 integration
    /// with `corelink-analytics::CardinalityValidator`).
    #[error(
        "logpush cardinality budget exceeded: scope={scope}, observed={observed}, budget={budget}"
    )]
    CardinalityBudgetExceeded {
        /// Currently-observed unique tuple count.
        observed: u64,
        /// Configured cardinality budget.
        budget: u64,
        /// `per_metric` or `global`.
        scope: &'static str,
    },

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// emit). Production wiring maps this to 503 fail-open per WI §6.1.9
    /// (log emit fail-OPEN distinction vs audit fail-closed).
    #[error("logpush internal state invariant violated: {0}")]
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
    fn redaction_failure_displays_diagnostic() {
        let e = LogpushError::RedactionFailure("invalid utf8".into());
        let s = format!("{e}");
        assert!(s.contains("redaction failure"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = LogAuditSinkError::Store("induced".to_string());
        let e: LogpushError = inner.into();
        assert!(matches!(e, LogpushError::Audit(_)));
    }

    #[test]
    fn from_sink_error_lifts_cleanly() {
        let inner = LogSinkError::Backend("induced".to_string());
        let e: LogpushError = inner.into();
        assert!(matches!(e, LogpushError::Sink(_)));
    }

    #[test]
    fn cardinality_budget_exceeded_displays_diagnostic() {
        let e = LogpushError::CardinalityBudgetExceeded {
            observed: 100_001,
            budget: 100_000,
            scope: "global",
        };
        let s = format!("{e}");
        assert!(s.contains("cardinality budget"));
        assert!(s.contains("global"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = LogpushError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}
