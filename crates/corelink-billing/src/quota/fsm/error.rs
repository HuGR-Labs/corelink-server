//! Canonical error taxonomy for the `corelink-quota-fsm` crate.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline +
//! S-10 inheritance pattern).
//!
//! ## Fail policy boundary
//!
//! Per WI-S10-005 §1 invariant 8 (Lote 10.6bis split-tier canonical) the
//! quota state-transition surface is **fail-CLOSED**: state-machine
//! integrity outranks availability. A partial transition (audit row
//! landed but state row not flipped, or vice-versa) corrupts the
//! customer-visible billing surface; the orchestrator aborts on any
//! audit/store backend failure so manual operator replay reconciles.
//!
//! The hot-path `is_hard_blocked()` query (consumed by the CAS write
//! handler) is the counter-pattern — fail-OPEN — and lives at the
//! Tower-layer boundary at WI-S10-007 production wiring; the orchestrator
//! here ships the canonical state-transition surface only.
//!
//! See `corelink-quota::error` (S-07 PROVISIONAL 429 fail-CLOSED at the
//! decision pipeline) + `corelink-quota-cas::error` (S-08 atomic CAS
//! quota with Retry-After) + `corelink-billing-reconcile::error`
//! (fail-CLOSED at the reconciliation layer) for the canonical
//! split-tier policy lattice.

use thiserror::Error;

/// Audit-of-audit sink failure surface (lifted into
/// [`QuotaFsmError::Audit`]).
///
/// The audit envelope (`corelink.billing_quota.{state_changed,
/// overage_telemetry_recorded, suspended, reinstated}`) emits BEFORE
/// state mutation per the canonical S-07 P1-1 fix + Lote 10.6bis
/// pattern: audit failure aborts the transition; the per-tenant state
/// remains at its pre-call value.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaFsmAuditSinkError {
    /// Backend transport failure (D1 audit_outbox INSERT rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`QuotaFsmError::Audit`] and returns to the caller;
    /// no state mutation, no telemetry emission.
    #[error("quota-fsm audit sink store error: {0}")]
    Store(String),
}

/// Per-tenant state-store failure surface (lifted into
/// [`QuotaFsmError::Store`]).
///
/// The state store persists every per-tenant `QuotaState` + invoice
/// failure counter. Production wiring at WI-S10-007 binds this to the
/// canonical D1 `quota_fsm_state` durable mirror (additive migration
/// `migrations/d1/0020_quota_fsm_state.sql`); the in-memory fake here
/// pins the same trait surface so the orchestrator's audit-fail-CLOSED
/// envelope holds at both fakes + the production wiring.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaFsmStoreError {
    /// Backend transport failure (D1 INSERT/UPDATE rejected / network
    /// partition).
    #[error("quota-fsm state store backend error: {0}")]
    Backend(String),
}

/// Canonical error surface returned by the [`crate::quota::fsm`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QuotaFsmError {
    /// Audit envelope rejection (audit sink unavailable). Fail-CLOSED
    /// per Lote 10.6bis pattern: no state mutation, no overage
    /// telemetry, no Stripe-suspension flip.
    #[error("quota-fsm audit envelope failure (state unchanged): {0}")]
    Audit(#[from] QuotaFsmAuditSinkError),

    /// State store backend failure (D1 UPSERT rejected). Fail-CLOSED
    /// per Lote 10.6bis split-tier — state-machine integrity outranks
    /// availability; the transition aborts so the customer-facing
    /// billing surface is never silently divergent from the audit
    /// trail.
    #[error("quota-fsm state store failure: {0}")]
    Store(#[from] QuotaFsmStoreError),

    /// Configuration invariant violated (utilization out of `[0, 200]`
    /// percent range, suspension threshold < 1 invoice failure, etc.).
    /// Programmer error; mapped to 5xx in the production wiring.
    #[error("quota-fsm configuration invariant violated: {0}")]
    Config(String),

    /// Internal invariant violation (e.g. a per-instance mutex poisoned
    /// by a panicking caller). Production wiring maps this to
    /// fail-CLOSED at the trait surface — the run aborts rather than
    /// risk corrupt state.
    #[error("quota-fsm internal state invariant violated: {0}")]
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
        let inner = QuotaFsmAuditSinkError::Store("induced".to_string());
        let e: QuotaFsmError = inner.into();
        let lifted = matches!(e, QuotaFsmError::Audit(_));
        assert!(lifted);
    }

    #[test]
    fn from_store_error_lifts_cleanly() {
        let inner = QuotaFsmStoreError::Backend("induced".to_string());
        let e: QuotaFsmError = inner.into();
        let lifted = matches!(e, QuotaFsmError::Store(_));
        assert!(lifted);
    }

    #[test]
    fn config_displays_diagnostic() {
        let e = QuotaFsmError::Config("utilization out of range".to_string());
        let s = format!("{e}");
        assert!(s.contains("configuration invariant"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = QuotaFsmError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }

    #[test]
    fn audit_displays_lifted_inner() {
        let e: QuotaFsmError =
            QuotaFsmAuditSinkError::Store("inner-msg".to_string()).into();
        let s = format!("{e}");
        assert!(s.contains("audit envelope failure"));
    }
}
