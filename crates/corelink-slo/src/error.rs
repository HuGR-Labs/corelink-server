//! `corelink-slo` canonical error taxonomy.

use thiserror::Error;

/// Canonical PagerDuty dispatcher transport error. Hoisted to a typed
/// error so the alert orchestrator can map dispatcher transport
/// failures to the canonical `SloError::Dispatcher` arm without
/// stringifying the cause.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SloPagerDutyDispatchError {
    /// PagerDuty Events API v2 transport failure (HTTPS POST rejected
    /// / network timeout / 5xx). Production wiring sets this when the
    /// API call surface returns a non-202 status.
    #[error("PagerDuty Events API v2 transport failure: {0}")]
    Transport(String),
}

/// Canonical `corelink-slo` error taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs (e.g. S-13 admin
/// custom SEV-tier forward / Twilio fallback).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SloError {
    /// `SloDefinition` failed input validation (e.g. `target_pct`
    /// outside `(0.0, 1.0]` / `window_days == 0`).
    #[error("invalid SLO definition: {0}")]
    InvalidSloDefinition(String),
    /// Audit-of-audit emit failed; the alert path aborts per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern +
    /// S-07 P1-1 fix). Caller MUST NOT retry the alert without first
    /// resolving the audit failure.
    #[error("SLO audit emit failed: {0}")]
    Audit(String),
    /// PagerDuty dispatcher transport failure. Production wiring sets
    /// this when the PagerDuty Events API v2 HTTPS POST is rejected /
    /// times out / returns 5xx. The alert is fail-OPEN per WI §6.1.10
    /// (alert-of-alert SEV-2 source `corelink_alerts_dispatched_total`
    /// counter increment with `dispatch="fail"` label).
    #[error("PagerDuty dispatcher failure: {0}")]
    Dispatcher(#[from] SloPagerDutyDispatchError),
    /// Internal invariant violation surfaced via `Mutex` poisoning or
    /// state corruption. Treat as a non-recoverable fault: the caller
    /// MUST tear down the orchestrator instance + reconstruct from
    /// the durable mirror.
    #[error("internal SLO orchestrator fault: {0}")]
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
    fn invalid_slo_definition_renders() {
        let e = SloError::InvalidSloDefinition("target_pct=2.0".to_string());
        assert!(format!("{e}").contains("invalid SLO definition"));
    }

    #[test]
    fn audit_renders() {
        let e = SloError::Audit("sink down".to_string());
        assert!(format!("{e}").contains("audit emit failed"));
    }

    #[test]
    fn dispatcher_renders() {
        let e = SloError::Dispatcher(SloPagerDutyDispatchError::Transport("503".to_string()));
        assert!(format!("{e}").contains("PagerDuty dispatcher failure"));
    }

    #[test]
    fn internal_renders() {
        let e = SloError::Internal("mutex poisoned".to_string());
        assert!(format!("{e}").contains("internal SLO orchestrator fault"));
    }

    #[test]
    fn dispatcher_from_pagerduty_arm() {
        let inner = SloPagerDutyDispatchError::Transport("timeout".to_string());
        let outer: SloError = inner.into();
        assert!(matches!(outer, SloError::Dispatcher(_)));
    }
}
