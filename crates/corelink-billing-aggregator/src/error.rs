//! Canonical error taxonomy for the `corelink-billing-aggregator`
//! counter-aggregator surface.
//!
//! All error enums are `#[non_exhaustive]` so additive growth lands
//! without breaking downstream `match` sites (Lote 10.6bis discipline).
//!
//! ## Fail policy boundary
//!
//! Per WI-S10-002 §1 invariant 7 + sprint contract §6 DoD, the counter
//! aggregator surface is **fail-CLOSED at the aggregation layer** — vs
//! the WI-S10-001 hot path which is fail-OPEN at the customer-facing
//! CAS request. Counter integrity is non-negotiable: a partial counter
//! row would cause Layer 1 reconciliation drift > 0.1% = customer
//! invoice dispute. Therefore aggregation halts on ANY error; SEV-1
//! alert; manual replay via the watermark resume primitive after
//! root-cause fix.
//!
//! See `corelink-billing-emit::error` for the dual fail-OPEN policy at
//! the hot-path emit surface; the two policies are deliberately
//! split-tier per Lote 10.6bis canonical lesson.

use thiserror::Error;

/// Audit-of-audit sink failure surface (lifted into
/// [`AggregatorError::Audit`]).
///
/// The audit-of-audit envelope (`corelink.billing_aggregator.{run_started,
/// run_completed, chain_break_detected, sink_failure}`) emits BEFORE
/// state mutation per the canonical S-07 P1-1 fix + Lote 10.6bis
/// pattern: audit failure aborts the run; the counter store + the chain
/// head remain at their pre-run state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AggregatorAuditSinkError {
    /// Backend transport failure (D1 audit_outbox INSERT rejected /
    /// SIEM webhook timeout / outbox batch failure). The orchestrator
    /// maps this to [`AggregatorError::Audit`] and returns to the
    /// caller; no chain advance, no counter write.
    #[error("billing-aggregator audit sink store error: {0}")]
    Store(String),
}

/// Aggregated-counter store failure surface (lifted into
/// [`AggregatorError::Store`]).
///
/// Production wiring binds this to D1 atomic transaction (counter row
/// UPSERT + `hash_chain_head` UPDATE in the same `db.batch`); the
/// in-memory fake here pins INV-BILLING-NO-LOSS Layer 1 + INV-BILLING-NO-DUP
/// at the trait surface so adversarial tests can falsify both
/// invariants independently of the production D1 binding (deferred to
/// WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter
/// pattern).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AggregatedCounterStoreError {
    /// Backend transport failure (D1 transaction rollback / network
    /// partition / IAM denial). Production wiring at WI-S10-007
    /// surfaces a SEV-1 alert + halts subsequent hours; manual replay
    /// via the watermark resume primitive after operator triage.
    #[error("billing-aggregator counter store backend error: {0}")]
    Backend(String),

    /// Digest mismatch on UPSERT replay path: a counter row already
    /// exists for the canonical (tenant, billing_period, event_kind)
    /// coordinate but the recomputed `own_digest` diverges from the
    /// stored digest. Per WI-S10-002 §6.1.9 watermark replay protocol,
    /// re-aggregation of the same period MUST produce the same digest
    /// (deterministic input ordering); a divergence is the canonical
    /// SEV-1 source — replay corruption.
    #[error(
        "billing-aggregator counter digest mismatch (replay corruption) at tenant={tenant_id} period={billing_period} kind={event_kind}: stored={stored_digest_hex} recomputed={recomputed_digest_hex}"
    )]
    DigestMismatch {
        /// Tenant id of the colliding aggregate.
        tenant_id: String,
        /// Canonical billing period (`YYYY-MM`).
        billing_period: String,
        /// Canonical event kind (the stringified
        /// `UsageEventKind::as_str()`).
        event_kind: String,
        /// Stored digest hex (64 chars BLAKE3-256).
        stored_digest_hex: String,
        /// Recomputed digest hex (64 chars BLAKE3-256).
        recomputed_digest_hex: String,
    },
}

/// Canonical error surface returned by the [`crate::aggregator`] APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AggregatorError {
    /// JCS canonicalization (RFC 8785) failed for the aggregate body
    /// (a non-finite float / non-string map key / etc.). Fail-CLOSED
    /// at the trait surface — the canonical chain digest cannot be
    /// derived without canonical bytes; the production wiring queues
    /// the aggregation for retry only after the underlying
    /// serialization issue is resolved (typically a code bug).
    #[error("billing-aggregator JCS canonicalization failed: {0}")]
    Canonicalization(String),

    /// Audit-of-audit emit failure aborts the run (fail-CLOSED envelope
    /// per the S-07 P1-1 lesson + Lote 10.6bis pattern). No counter
    /// store advance, no chain advance.
    #[error("billing-aggregator audit envelope failure (state unchanged): {0}")]
    Audit(#[from] AggregatorAuditSinkError),

    /// Underlying counter store failure (production D1 atomic
    /// transaction rejected / digest mismatch on replay). Production
    /// wiring is fail-CLOSED: the aggregator halts the current run +
    /// SEV-1 alert.
    #[error("billing-aggregator counter store failure: {0}")]
    Store(#[from] AggregatedCounterStoreError),

    /// Hash chain integrity violation detected during aggregate append:
    /// the new aggregate's `prev_hash` does not match the per-(tenant,
    /// billing_period) chain head OR the recomputed link diverged from
    /// what the verifier expected. Per WI-S10-002 §1 invariant 3
    /// (INV-AUDIT-APPEND-ONLY chain integrity), this is the canonical
    /// CRITICAL post-mortem trigger — admin INSERT bypass, R2 raw
    /// retroactive deletion, or BLAKE3 implementation defect.
    #[error("billing-aggregator chain break detected (CRITICAL tampering signal): {0}")]
    ChainBreak(String),

    /// Internal invariant violation (e.g. mutex poisoned by a panicking
    /// run; sequence_number overflow at u64::MAX which is structurally
    /// unreachable; etc.). Production wiring maps this to fail-CLOSED at
    /// the trait surface — the run aborts rather than risk corrupt
    /// state.
    #[error("billing-aggregator internal state invariant violated: {0}")]
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
        let e = AggregatorError::Canonicalization("non-finite float".into());
        let s = format!("{e}");
        assert!(s.contains("JCS canonicalization"));
    }

    #[test]
    fn from_audit_sink_error_lifts_cleanly() {
        let inner = AggregatorAuditSinkError::Store("induced".to_string());
        let e: AggregatorError = inner.into();
        assert!(matches!(e, AggregatorError::Audit(_)));
    }

    #[test]
    fn from_store_error_lifts_cleanly() {
        let inner = AggregatedCounterStoreError::Backend("induced".to_string());
        let e: AggregatorError = inner.into();
        assert!(matches!(e, AggregatorError::Store(_)));
    }

    #[test]
    fn chain_break_displays_diagnostic() {
        let e = AggregatorError::ChainBreak(
            "expected head abcdef..; observed 000000..".to_string(),
        );
        let s = format!("{e}");
        assert!(s.contains("chain break"));
        assert!(s.contains("CRITICAL"));
    }

    #[test]
    fn digest_mismatch_displays_diagnostic() {
        let e = AggregatedCounterStoreError::DigestMismatch {
            tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
            billing_period: "2026-05".to_string(),
            event_kind: "cas_put".to_string(),
            stored_digest_hex: "ab".repeat(32),
            recomputed_digest_hex: "cd".repeat(32),
        };
        let s = format!("{e}");
        assert!(s.contains("digest mismatch"));
        assert!(s.contains("replay corruption"));
        assert!(s.contains("2026-05"));
    }

    #[test]
    fn internal_displays_diagnostic() {
        let e = AggregatorError::Internal("mutex poisoned".to_string());
        let s = format!("{e}");
        assert!(s.contains("mutex poisoned"));
    }
}
