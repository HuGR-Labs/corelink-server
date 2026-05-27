//! Canonical error taxonomy for the `corelink-billing-replay` crate.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline +
//! S-09 / S-10 inheritance).
//!
//! ## Fail policy boundary
//!
//! Per WI-S10-006 §1 invariant 7 (Lote 10.6bis split-tier canonical) the
//! replay forensic surface is **fail-CLOSED**: forensic integrity
//! outranks availability. Audit envelope failure aborts the replay; no
//! idempotency ledger UPSERT; no append to the production audit chain.
//! Distinct from `corelink-billing-emit` (fail-OPEN at the customer hot
//! path) — the replay forensic engine is an admin-only background
//! primitive with NO customer SLA at risk.
//!
//! See `corelink-billing-emit::error` (fail-OPEN at hot path) +
//! `corelink-billing-aggregator::error` (fail-CLOSED at aggregation
//! layer) + `corelink-billing-stripe::error` (fail-CLOSED at Stripe
//! adapter layer) + `corelink-billing-reconcile::error` (fail-CLOSED at
//! the reconciliation layer) for the canonical split-tier policy
//! lattice.

use thiserror::Error;

/// Audit-of-audit sink failure surface (lifted into
/// [`ReplayError::Audit`]).
///
/// The audit envelope (`corelink.billing_replay.{request_authorized,
/// request_denied, dry_run_planned, executed, layer_diverged}`) emits
/// BEFORE state mutation per the canonical S-07 P1-1 fix + Lote 10.6bis
/// pattern: audit failure aborts the replay; the idempotency ledger
/// remains at its pre-call state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReplayAuditSinkError {
    /// Backend transport failure (S-09 audit chain append rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`ReplayError::Audit`] and returns to the caller;
    /// no idempotency ledger UPSERT, no state mutation past the point
    /// of the audit failure.
    #[error("billing-replay audit sink store error: {0}")]
    Store(String),
}

/// Idempotency ledger failure surface (lifted into
/// [`ReplayError::Idempotency`]).
///
/// The idempotency ledger persists every replay outcome per
/// `request_id` UNIQUE so duplicate re-submissions reuse the prior
/// outcome. Production wiring at WI-S10-007 binds this to the
/// canonical D1 `billing_replay_audit` table (additive migration
/// `migrations/d1/0021_billing_replay_audit.sql`).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReplayIdempotencyError {
    /// Backend transport failure (D1 INSERT rejected / network
    /// partition / batch failure).
    #[error("billing-replay idempotency ledger backend error: {0}")]
    Backend(String),

    /// A prior outcome with the same canonical `request_id` exists
    /// but with a divergent payload (request body differs while the
    /// `request_id` matches). Surfaces as a tampering signal: the
    /// canonical idempotency contract is "same `request_id` → same
    /// outcome"; a replayed `request_id` paired with a different
    /// `(tenant, billing_period, reason)` tuple is a SEV-1 forensic
    /// anomaly.
    #[error("billing-replay idempotency ledger detected divergent payload for request_id={0}")]
    DivergentPayload(String),
}

/// Canonical error surface returned by the [`crate::replay::engine`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReplayError {
    /// Audit envelope rejection (audit sink unavailable). Fail-CLOSED
    /// per Lote 10.6bis pattern: no idempotency ledger UPSERT, no
    /// state mutation.
    #[error("billing-replay audit envelope failure (state unchanged): {0}")]
    Audit(#[from] ReplayAuditSinkError),

    /// Idempotency ledger backend failure (D1 INSERT rejected).
    /// Fail-CLOSED per Lote 10.6bis split-tier — forensic integrity
    /// outranks availability; the run aborts so the auditor trail is
    /// never silently truncated.
    #[error("billing-replay idempotency ledger failure: {0}")]
    Idempotency(#[from] ReplayIdempotencyError),

    /// Configuration invariant violated (e.g. tenant_id mismatch
    /// between request body + authenticated context). Programmer
    /// error; mapped to 5xx in production wiring.
    #[error("billing-replay configuration invariant violated: {0}")]
    Config(String),

    /// Internal invariant violation (e.g. a per-instance mutex
    /// poisoned by a panicking run). Production wiring maps this to
    /// fail-CLOSED at the trait surface — the run aborts rather than
    /// risk corrupt state.
    #[error("billing-replay internal state invariant violated: {0}")]
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
        let inner = ReplayAuditSinkError::Store("induced".to_string());
        let e: ReplayError = inner.into();
        assert!(matches!(e, ReplayError::Audit(_)));
    }

    #[test]
    fn from_idempotency_error_lifts_cleanly() {
        let inner = ReplayIdempotencyError::Backend("induced".to_string());
        let e: ReplayError = inner.into();
        assert!(matches!(e, ReplayError::Idempotency(_)));
    }

    #[test]
    fn from_idempotency_divergent_lifts_cleanly() {
        let inner = ReplayIdempotencyError::DivergentPayload("rid".to_string());
        let e: ReplayError = inner.into();
        assert!(matches!(e, ReplayError::Idempotency(_)));
    }

    #[test]
    fn config_displays_diagnostic() {
        let e = ReplayError::Config("tenant mismatch".to_string());
        let s = format!("{e}");
        assert!(s.contains("configuration invariant"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = ReplayError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }

    #[test]
    fn audit_displays_lifted_inner() {
        let e: ReplayError = ReplayAuditSinkError::Store("inner-msg".to_string()).into();
        let s = format!("{e}");
        assert!(s.contains("audit envelope failure"));
    }
}
