//! Canonical [`AbuseError`] taxonomy surfaced by every fallible API in
//! the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07 /
//! S-08 lesson — the variant set grows additively across S-08 follow-on
//! WIs (production cron DO + admin S-13 endpoints in WI-S08-006)
//! without breaking downstream callers.

use thiserror::Error;
use uuid::Uuid;

use super::audit::AbuseAuditSinkError;
use super::metrics::AbuseMetricsObserverError;

/// Canonical errors surfaced by [`super::scorer::AbuseScorer`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AbuseError {
    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 5xx to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The orchestrator rolls
    /// back the per-request decision on this error per WI §6.1.8
    /// fail-closed envelope (Lote 10.6bis pattern + S-07 sprint-close
    /// P1-1 fix).
    #[error("abuse audit sink error: {0}")]
    Audit(#[from] AbuseAuditSinkError),

    /// Metrics emit failure. The orchestrator fail-closes ONLY on audit
    /// emit failure, not metric; this arm is surfaced when the
    /// production wiring opts into strict-metric mode (admin S-13
    /// preference; canonical default is log-and-continue per WI
    /// §14.s08.004.10).
    #[error("abuse metrics observer error: {0}")]
    Metrics(#[from] AbuseMetricsObserverError),

    /// Auto-suspend programmatic attempt detected (LGPD Art. 20 + GDPR
    /// Art. 22 violation; sprint contract §7.10.s08.3 humane response).
    /// This arm is the canary trigger for the SEV-1 LGPD violation
    /// alert; production wiring rolls back any code path that reaches
    /// this branch (suspend ALWAYS requires human admin review per WI
    /// §1 invariant 3).
    #[error("auto-suspend forbidden by humane response (LGPD Art. 20 + GDPR Art. 22): admin must manually review tenant {tenant_id} score {score}")]
    AutoSuspendForbidden {
        /// Tenant scope.
        tenant_id: Uuid,
        /// Score that triggered the forbidden auto-suspend attempt.
        score: f64,
    },

    /// Backend transport failure (cron DO read S-09 metrics / D1 batch
    /// failure / etc).
    #[error("abuse backend error: {0}")]
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
    fn auto_suspend_forbidden_format_carries_tenant_and_score() {
        let id = Uuid::from_u128(0xdead_beef);
        let e = AbuseError::AutoSuspendForbidden {
            tenant_id: id,
            score: 0.97,
        };
        let s = format!("{e}");
        assert!(s.contains("LGPD Art. 20"));
        assert!(s.contains("GDPR Art. 22"));
        assert!(s.contains(&format!("{id}")));
        assert!(s.contains("0.97"));
    }

    #[test]
    fn audit_error_format_includes_inner_message() {
        let inner = AbuseAuditSinkError::Store("db unreachable".to_string());
        let e: AbuseError = inner.into();
        let s = format!("{e}");
        assert!(s.contains("audit"));
        assert!(s.contains("db unreachable"));
    }

    #[test]
    fn backend_format_carries_message() {
        let e = AbuseError::Backend("S-09 metrics endpoint timeout".to_string());
        let s = format!("{e}");
        assert!(s.contains("S-09 metrics endpoint timeout"));
    }
}
