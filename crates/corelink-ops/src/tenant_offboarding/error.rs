//! Canonical error taxonomy for `corelink-tenant-offboarding`.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites.
//!
//! ## Fail policy boundary (ADR-S11-002 inheritance)
//!
//! The tenant-offboarding surface is **fail-CLOSED**: regulatory
//! integrity outranks availability. Audit envelope failure aborts
//! the transition; no store mutation. Distinct from
//! `corelink-billing-emit` (fail-OPEN at the customer hot path).
//! Silent loss of a tenant-offboarding transition is a LGPD Art. 18
//! / GDPR Art. 17 (tenant-equivalent) violation.

use thiserror::Error;

use super::state::{TenantOffboardingState, TransitionTrigger};

/// Audit sink failure surface (lifted into
/// [`TenantOffboardingError::Audit`]).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TenantOffboardingAuditSinkError {
    /// Backend transport failure (S-09 audit chain append rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`TenantOffboardingError::Audit`] and aborts;
    /// no store mutation.
    #[error("tenant offboarding audit sink store error: {0}")]
    Store(String),
}

/// Durable store failure surface (lifted into
/// [`TenantOffboardingError::Store`]).
///
/// Production wiring at PRR ship gate binds this to the canonical D1
/// `tenant_offboarding_state` table per
/// `migrations/d1/0046_tenant_offboarding_state.sql`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TenantOffboardingStoreError {
    /// Backend transport failure (D1 UPDATE rejected / network
    /// partition / batch failure).
    #[error("tenant offboarding store backend error: {0}")]
    Backend(String),

    /// The tenant_id does not have an offboarding record (caller
    /// tried to advance a tenant that was never marked for
    /// offboarding; the canonical orchestrator pre-check ensures
    /// this is unreachable through the trait surface).
    #[error("tenant offboarding record not found: tenant_id={0}")]
    NotFound(String),
}

/// Canonical error surface returned by the orchestrator + state
/// machine.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TenantOffboardingError {
    /// Audit envelope rejection. Fail-CLOSED: no store mutation, no
    /// state advance.
    #[error("tenant offboarding audit envelope failure (state unchanged): {0}")]
    Audit(#[from] TenantOffboardingAuditSinkError),

    /// Durable store failure. Fail-CLOSED.
    #[error("tenant offboarding store failure: {0}")]
    Store(#[from] TenantOffboardingStoreError),

    /// The requested `(from, trigger)` transition is not a legal
    /// transition in the canonical state machine table. The error
    /// carries both for forensic clarity.
    #[error("tenant offboarding illegal transition: from={from} trigger={trigger}")]
    IllegalTransition {
        /// Source state at the time of the rejected request.
        from: TenantOffboardingState,
        /// Trigger that was illegal for the source state.
        trigger: TransitionTrigger,
    },

    /// The requested transition is structurally legal but the
    /// canonical wall-clock grace threshold has not yet been
    /// reached. Carries the canonical deadline (ms since epoch)
    /// for diagnostic display.
    #[error(
        "tenant offboarding grace not yet elapsed: state={state} not_before_ms={not_before_ms}"
    )]
    GraceNotElapsed {
        /// Source state at the time of the rejected request.
        state: TenantOffboardingState,
        /// Wall-clock instant (ms since epoch) at which the
        /// transition becomes legal.
        not_before_ms: i64,
    },

    /// The canonical final-erasure commit was requested without a
    /// fresh dry-run preview having been generated + confirmed.
    /// Production wiring at PRR ship gate enforces this at the
    /// runbook layer; the trait-level surface here ensures the
    /// orchestrator REJECTS bare `AdminCommitErasure` calls without
    /// the confirmation token.
    #[error("tenant offboarding final erasure requires dry-run preview confirmation")]
    DryRunPreviewMissing,

    /// Configuration invariant violated (e.g. empty tenant_id).
    /// Programmer error; mapped to 5xx in production wiring.
    #[error("tenant offboarding configuration invariant violated: {0}")]
    Config(String),

    /// Internal invariant violation (e.g. a per-instance mutex
    /// poisoned by a panicking run). Fail-CLOSED.
    #[error("tenant offboarding internal state invariant violated: {0}")]
    Internal(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = TenantOffboardingAuditSinkError::Store("induced".to_string());
        let e: TenantOffboardingError = inner.into();
        assert!(matches!(e, TenantOffboardingError::Audit(_)));
    }

    #[test]
    fn from_store_error_lifts_cleanly() {
        let inner = TenantOffboardingStoreError::Backend("induced".to_string());
        let e: TenantOffboardingError = inner.into();
        assert!(matches!(e, TenantOffboardingError::Store(_)));
    }

    #[test]
    fn illegal_transition_renders_diagnostic() {
        let e = TenantOffboardingError::IllegalTransition {
            from: TenantOffboardingState::Erased,
            trigger: TransitionTrigger::CustomerInitiated,
        };
        let s = format!("{e}");
        assert!(s.contains("illegal transition"));
        assert!(s.contains("erased"));
        assert!(s.contains("customer_initiated"));
    }

    #[test]
    fn grace_not_elapsed_renders_deadline() {
        let e = TenantOffboardingError::GraceNotElapsed {
            state: TenantOffboardingState::GracePeriod,
            not_before_ms: 12345,
        };
        let s = format!("{e}");
        assert!(s.contains("12345"));
    }

    #[test]
    fn dry_run_missing_renders_human_message() {
        let e = TenantOffboardingError::DryRunPreviewMissing;
        let s = format!("{e}");
        assert!(s.contains("dry-run preview"));
    }
}
