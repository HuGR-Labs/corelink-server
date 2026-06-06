//! Canonical error taxonomy for the `corelink-billing-reconcile` crate.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline +
//! S-09 / S-10 inheritance).
//!
//! ## Fail policy boundary
//!
//! Per WI-S10-004 §1 invariant 7 (Lote 10.6bis split-tier canonical) the
//! reconciliation surface is **fail-CLOSED**: reconciliation integrity
//! outranks availability. Any layer query failure halts the run; partial
//! reconciliation result = false confidence; SEV-1 alert fires; manual
//! replay after operator triage. Distinct from `corelink-billing-emit`
//! (fail-OPEN at the customer hot path) — the reconciliation worker is
//! a background cron with NO customer SLA at risk.
//!
//! See `corelink-billing-emit::error` (fail-OPEN at hot path) +
//! `corelink-billing-aggregator::error` (fail-CLOSED at aggregation
//! layer) + `corelink-billing-stripe::error` (fail-CLOSED at Stripe
//! adapter layer) for the canonical split-tier policy lattice.

use thiserror::Error;

/// Audit-of-audit sink failure surface (lifted into
/// [`ReconcileError::Audit`]).
///
/// The audit envelope (`corelink.billing_reconcile.{run_started,
/// no_drift, auto_fixed, ticket_filed, page_dispatched, stripe_paused}`)
/// emits BEFORE state mutation per the canonical S-07 P1-1 fix +
/// Lote 10.6bis pattern: audit failure aborts the run; the drift-history
/// ledger + the Stripe-pause flag remain at their pre-call state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReconcileAuditSinkError {
    /// Backend transport failure (D1 audit_outbox INSERT rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`ReconcileError::Audit`] and returns to the caller;
    /// no drift-history INSERT, no Stripe-submission pause, no state
    /// mutation.
    #[error("billing-reconcile audit sink store error: {0}")]
    Store(String),
}

/// Drift-history ledger failure surface (lifted into
/// [`ReconcileError::DriftHistory`]).
///
/// The drift-history ledger persists every reconciliation decision per
/// (tenant, billing_period) for the SOC 2 CC1.4 + GAAP ASC 606
/// 7-year evidence trail. Production wiring at WI-S10-007 binds this to
/// the canonical D1 `billing_reconciliation_drift` table (additive
/// migration `migrations/d1/0019_billing_reconciliation_drift.sql`).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReconcileDriftHistoryError {
    /// Backend transport failure (D1 INSERT rejected / network
    /// partition / batch failure).
    #[error("billing-reconcile drift history backend error: {0}")]
    Backend(String),
}

/// Stripe-submission control surface failure (lifted into
/// [`ReconcileError::StripePause`]).
///
/// The SEV-1-arm pause primitive halts further usage_record submissions
/// to Stripe until operator clearance. Production wiring at WI-S10-007
/// binds this to the canonical D1 `stripe_submission_state` flag table
/// (read by the WI-S10-003 adapter at every record_usage call). The
/// in-memory fake here pins the same trait surface so the orchestrator's
/// SEV-1 arm exercises the same fail-CLOSED envelope discipline.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReconcileStripePauseError {
    /// Backend transport failure (D1 UPDATE rejected / network
    /// partition).
    #[error("billing-reconcile Stripe-pause backend error: {0}")]
    Backend(String),
}

/// Canonical error surface returned by the [`crate::reconciler`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReconcileError {
    /// Audit envelope rejection (audit sink unavailable). Fail-CLOSED
    /// per Lote 10.6bis pattern: no drift-history INSERT, no
    /// Stripe-submission pause, no state mutation.
    #[error("billing-reconcile audit envelope failure (state unchanged): {0}")]
    Audit(#[from] ReconcileAuditSinkError),

    /// Drift-history ledger backend failure (D1 INSERT rejected).
    /// Fail-CLOSED per Lote 10.6bis split-tier — reconciliation
    /// integrity outranks availability; the run aborts so the auditor
    /// trail is never silently truncated.
    #[error("billing-reconcile drift-history ledger failure: {0}")]
    DriftHistory(#[from] ReconcileDriftHistoryError),

    /// Stripe-pause control-surface backend failure (SEV-1 arm could
    /// not flip the submission flag). Fail-CLOSED — the SEV-1 alert
    /// still fires via the audit row that emitted BEFORE this attempt
    /// per the canonical envelope discipline.
    #[error("billing-reconcile Stripe-pause control surface failure: {0}")]
    StripePause(#[from] ReconcileStripePauseError),

    /// Configuration invariant violated (drift threshold ladder
    /// non-monotonic, auto-fix gate inverted, non-finite tolerance,
    /// negative percentage, etc.). Programmer error; mapped to 5xx in
    /// the production wiring.
    #[error("billing-reconcile configuration invariant violated: {0}")]
    Config(String),

    /// Internal invariant violation (e.g. a per-instance mutex poisoned
    /// by a panicking run). Production wiring maps this to fail-CLOSED
    /// at the trait surface — the run aborts rather than risk corrupt
    /// state.
    #[error("billing-reconcile internal state invariant violated: {0}")]
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
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = ReconcileAuditSinkError::Store("induced".to_string());
        let e: ReconcileError = inner.into();
        let lifted = matches!(e, ReconcileError::Audit(_));
        assert!(lifted);
    }

    #[test]
    fn from_drift_history_error_lifts_cleanly() {
        let inner = ReconcileDriftHistoryError::Backend("induced".to_string());
        let e: ReconcileError = inner.into();
        let lifted = matches!(e, ReconcileError::DriftHistory(_));
        assert!(lifted);
    }

    #[test]
    fn from_stripe_pause_error_lifts_cleanly() {
        let inner = ReconcileStripePauseError::Backend("induced".to_string());
        let e: ReconcileError = inner.into();
        let lifted = matches!(e, ReconcileError::StripePause(_));
        assert!(lifted);
    }

    #[test]
    fn config_displays_diagnostic() {
        let e = ReconcileError::Config("non-monotonic ladder".to_string());
        let s = format!("{e}");
        assert!(s.contains("configuration invariant"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = ReconcileError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }

    #[test]
    fn audit_displays_lifted_inner() {
        let e: ReconcileError = ReconcileAuditSinkError::Store("inner-msg".to_string()).into();
        let s = format!("{e}");
        assert!(s.contains("audit envelope failure"));
    }
}
