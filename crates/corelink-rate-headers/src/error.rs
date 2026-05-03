//! Canonical [`CircuitError`] taxonomy surfaced by every fallible API
//! in the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07 /
//! S-08 lesson — the variant set grows additively across S-08 follow-on
//! WIs (production CF DO singleton + admin S-13 endpoints in
//! WI-S08-006) without breaking downstream callers.

use thiserror::Error;

use crate::audit::CircuitAuditSinkError;
use crate::metrics::CircuitMetricsObserverError;

/// Canonical errors surfaced by [`crate::circuit::GlobalCircuitBreaker`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CircuitError {
    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 5xx to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The orchestrator rolls
    /// back the per-call decision on this error per WI §6.1.11
    /// fail-closed envelope (Lote 10.6bis pattern + S-07 sprint-close
    /// P1-1 fix).
    #[error("circuit audit sink error: {0}")]
    Audit(#[from] CircuitAuditSinkError),

    /// Metrics emit failure. The orchestrator fail-closes ONLY on audit
    /// emit failure, not metric; this arm is surfaced when the
    /// production wiring opts into strict-metric mode (admin S-13
    /// preference; canonical default is log-and-continue).
    #[error("circuit metrics observer error: {0}")]
    Metrics(#[from] CircuitMetricsObserverError),

    /// Admin authorization failed on a manual override attempt
    /// (AdminCtx S-13 verification rejected the request).
    #[error("circuit admin authorization failed: {0}")]
    AdminAuthFailed(String),

    /// Backend transport failure (CF DO read / D1 batch write / etc).
    #[error("circuit backend error: {0}")]
    Backend(String),
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
    fn audit_error_format_includes_inner_message() {
        let inner = CircuitAuditSinkError::Store("d1 unreachable".to_string());
        let e: CircuitError = inner.into();
        let s = format!("{e}");
        assert!(s.contains("audit"));
        assert!(s.contains("d1 unreachable"));
    }

    #[test]
    fn admin_auth_format_carries_message() {
        let e = CircuitError::AdminAuthFailed(
            "admin token revoked".to_string(),
        );
        let s = format!("{e}");
        assert!(s.contains("admin token revoked"));
        assert!(s.contains("admin"));
    }

    #[test]
    fn backend_format_carries_message() {
        let e = CircuitError::Backend("DO unreachable".to_string());
        let s = format!("{e}");
        assert!(s.contains("DO unreachable"));
    }
}
