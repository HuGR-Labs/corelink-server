//! Canonical error taxonomy for the `corelink-billing-emit` emit
//! surface.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline).
//!
//! ## Fail policy boundary
//!
//! Per the WI-S10-001 narrative, the billing emit surface is **fail-OPEN
//! at the customer-facing CAS hot path** (the customer request never
//! blocks on a billing emit failure — staging table + retry queue
//! handles eventual delivery within SLO-FRESH-BILLING ≤ 15min). The
//! audit-of-audit envelope itself is **fail-CLOSED at this trait
//! surface** — the orchestrator returns `BillingEmitError::Audit` and
//! the wrapping production worker decides whether to retry / queue /
//! surface a 5xx (the trait does not make that decision). The emit
//! orchestrator NEVER advances idempotency tracker state nor R2 sink
//! state when an audit emit at a decision arm fails.
//!
//! See also `corelink-audit-chain::error` for the canonical fail-CLOSED
//! audit pattern (S-09 inheritance).

use thiserror::Error;

/// Audit-of-audit sink failure surface (lifted into
/// [`BillingEmitError::Audit`]).
///
/// The audit-of-audit envelope (`corelink.billing.{usage_emitted,
/// duplicate_rejected, sink_failure, idempotency_collision}`) emits
/// BEFORE state mutation per the canonical S-07 P1-1 fix + Lote 10.6bis
/// pattern (audit failure aborts the emit; idempotency tracker + R2 sink
/// remain at their pre-emit state).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BillingAuditSinkError {
    /// Backend transport failure (D1 audit_outbox INSERT rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`BillingEmitError::Audit`] and returns to the
    /// caller; no idempotency / R2 / chain state advance.
    #[error("billing-emit audit sink store error: {0}")]
    Store(String),
}

/// R2 NDJSON sink failure surface (lifted into
/// [`BillingEmitError::Sink`]).
///
/// Production wiring binds this to the CF R2 PutObject endpoint with an
/// append-only key prefix
/// (`usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson`). The
/// orchestrator surfaces overwrite attempts at the trait surface as
/// [`R2UsageSinkError::AppendOnlyViolation`] so the test suite can pin
/// INV-BILLING-APPEND-ONLY independently of the production R2 Object
/// Lock retention enforcement (which lands alongside WI-S10-007).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum R2UsageSinkError {
    /// Backend transport failure (R2 PutObject rejected / network
    /// partition / IAM denial). Per the WI-S10-001 narrative this is
    /// fail-OPEN at the hot path: the orchestrator returns the error +
    /// the production worker queues for retry; the customer request
    /// never blocks on this.
    #[error("billing-emit R2 sink backend error: {0}")]
    Backend(String),

    /// Append-only invariant violation: an emit attempted to write at
    /// `(tenant, billing_period, seq)` whose canonical key already
    /// exists in the sink. Pins INV-BILLING-APPEND-ONLY (analogous to
    /// INV-AUDIT-APPEND-ONLY from S-09 audit chain). Production wiring
    /// at the R2 PutObject layer enforces this via Object Lock
    /// Governance Mode + the canonical key pattern (existing key →
    /// PutObject rejected); the in-memory fake here pins it at the
    /// trait surface.
    #[error("billing-emit R2 sink append-only violation at key {key} (INV-BILLING-APPEND-ONLY)")]
    AppendOnlyViolation {
        /// Canonical R2 object key the orchestrator attempted to
        /// overwrite.
        key: String,
    },
}

/// Canonical error surface returned by the [`crate::emitter`] +
/// [`crate::idempotency`] + [`crate::sink`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BillingEmitError {
    /// JCS canonicalization (RFC 8785) failed for the event body
    /// (a non-finite float / non-string map key / etc.). Fail-CLOSED
    /// at the trait surface — the canonical idem_key cannot be derived
    /// without canonical bytes; the production wiring queues the event
    /// for retry only after the underlying serialization issue is
    /// resolved (typically a code bug).
    #[error("billing-emit JCS canonicalization failed: {0}")]
    Canonicalization(String),

    /// Audit-of-audit emit failure aborts the chain emit (fail-CLOSED
    /// envelope per the S-07 P1-1 lesson + Lote 10.6bis pattern). No
    /// idempotency tracker or R2 sink state advances.
    #[error("billing-emit audit envelope failure (state unchanged): {0}")]
    Audit(#[from] BillingAuditSinkError),

    /// Underlying R2 sink transport / append-only failure (production
    /// CF R2 PutObject rejected). Production wiring is fail-OPEN at the
    /// hot path: caller queues for retry + customer request never
    /// blocks.
    #[error("billing-emit R2 sink failure: {0}")]
    Sink(#[from] R2UsageSinkError),

    /// Idempotency tracker observed a duplicate `idem_key` for the
    /// (tenant, request) pair: the event was already accepted; the
    /// caller's retry is replay-safe (idempotent). The audit
    /// `corelink.billing.duplicate_rejected` fires BEFORE this error
    /// surfaces. Production wiring maps this to a
    /// `corelink_billing_idempotency_dedup_total{kind}` counter
    /// increment (informational; replay-safety canary).
    #[error(
        "billing-emit duplicate rejected (idempotent replay): tenant={tenant_id} idem_key={idem_key}"
    )]
    DuplicateRejected {
        /// Tenant id of the duplicate emit.
        tenant_id: String,
        /// Canonical 64-char hex BLAKE3-256 idem_key.
        idem_key: String,
    },

    /// Idempotency collision detected: the same `idem_key` was observed
    /// across two events whose canonical bytes DIFFER. Per the BLAKE3
    /// 256-bit security level this is < 2^-128 probability under random
    /// inputs; the orchestrator surfaces it as a defensive guard
    /// against caller bugs (e.g. a request_id was reused across two
    /// distinct billable operations). Audit
    /// `corelink.billing.idempotency_collision` fires BEFORE this error
    /// surfaces (SEV-1 source).
    #[error(
        "billing-emit idempotency collision: tenant={tenant_id} idem_key={idem_key} canonical bytes diverged"
    )]
    IdempotencyCollision {
        /// Tenant id of the colliding emit.
        tenant_id: String,
        /// Canonical 64-char hex BLAKE3-256 idem_key.
        idem_key: String,
    },

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// emit). Production wiring maps this to fail-CLOSED at the trait
    /// surface — the chain must abort rather than risk corrupt state.
    #[error("billing-emit internal state invariant violated: {0}")]
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
    fn canonicalization_displays_diagnostic() {
        let e = BillingEmitError::Canonicalization("non-finite float".into());
        let s = format!("{e}");
        assert!(s.contains("JCS canonicalization"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = BillingAuditSinkError::Store("induced".to_string());
        let e: BillingEmitError = inner.into();
        assert!(matches!(e, BillingEmitError::Audit(_)));
    }

    #[test]
    fn from_r2_sink_error_lifts_cleanly() {
        let inner = R2UsageSinkError::Backend("induced".to_string());
        let e: BillingEmitError = inner.into();
        assert!(matches!(e, BillingEmitError::Sink(_)));
    }

    #[test]
    fn append_only_violation_displays_key() {
        let e = R2UsageSinkError::AppendOnlyViolation {
            key: "usage/abc/2026-05/00000000.usage.ndjson".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("INV-BILLING-APPEND-ONLY"));
        assert!(s.contains("usage/abc/2026-05/00000000.usage.ndjson"));
    }

    #[test]
    fn duplicate_rejected_displays_diagnostic() {
        let e = BillingEmitError::DuplicateRejected {
            tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
            idem_key: "ab".repeat(32),
        };
        let s = format!("{e}");
        assert!(s.contains("duplicate rejected"));
        assert!(s.contains("idempotent"));
    }

    #[test]
    fn idempotency_collision_displays_diagnostic() {
        let e = BillingEmitError::IdempotencyCollision {
            tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
            idem_key: "cd".repeat(32),
        };
        let s = format!("{e}");
        assert!(s.contains("idempotency collision"));
        assert!(s.contains("canonical bytes diverged"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = BillingEmitError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}
