//! Audit-of-audit (meta-audit) trait + InMemory test sink for the
//! billing emit surface.
//!
//! Mirrors the `corelink-audit-chain::audit` discipline at one
//! meta-level higher — every emit decision arm fires its own audit event
//! BEFORE state mutation per the canonical Lote 10.6bis pattern + S-07
//! P1-1 fix. The billing emit surface is THE compliance primitive for
//! financial integrity (CTRL-BILLING-001 / SOC 2 CC1.4 / GAAP ASC 606);
//! a corrupt emit decision that didn't surface a meta-audit row would
//! mean the reconciliation worker (WI-S10-004) cannot retroactively
//! diagnose the root cause.
//!
//! WI-S10-001 §6.1 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.billing.usage_emitted` — emitted on every successful
//!   `UsageEvent` accepted by the emitter (after canonicalization +
//!   idempotency tracker insert + R2 NDJSON write).
//! - `corelink.billing.duplicate_rejected` — emitted on every
//!   replay-safe duplicate detected by the idempotency tracker (the
//!   same `idem_key` was already accepted; the caller's retry is
//!   treated as a no-op + the audit fires for visibility).
//! - `corelink.billing.sink_failure` — emitted on every R2 PutObject
//!   transport failure (production wiring fail-OPEN at the hot path
//!   per the WI-S10-001 narrative; SEV-3 source).
//! - `corelink.billing.idempotency_collision` — emitted when the
//!   tracker observed the same `idem_key` for two events whose
//!   canonical bytes DIFFER (probability < 2^-128 for BLAKE3-256 under
//!   random inputs; SEV-1 source — defensive guard against caller bugs
//!   like reused request_id across distinct billable operations).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (WI-S10-002 counter
//! aggregator / WI-S10-007 PRR) can extend the taxonomy additively
//! without breaking downstream sinks.

use std::sync::Mutex;

/// Canonical billing-emit meta-audit taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BillingAuditEventType {
    /// `corelink.billing.usage_emitted` — successful emit (the
    /// idempotency tracker accepted the event + the R2 NDJSON write
    /// committed).
    UsageEmitted,
    /// `corelink.billing.duplicate_rejected` — idempotency tracker
    /// observed a replay (same `idem_key` previously accepted; safe).
    DuplicateRejected,
    /// `corelink.billing.sink_failure` — R2 PutObject transport
    /// failure. Per the WI-S10-001 narrative this is fail-OPEN at the
    /// hot path (the customer request never blocks; staging table +
    /// retry queue handles eventual delivery).
    SinkFailure,
    /// `corelink.billing.idempotency_collision` — tracker observed the
    /// same `idem_key` for two events whose canonical bytes DIFFER.
    /// SEV-1 source; defensive guard against caller bugs.
    IdempotencyCollision,
}

impl BillingAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsageEmitted => "corelink.billing.usage_emitted",
            Self::DuplicateRejected => "corelink.billing.duplicate_rejected",
            Self::SinkFailure => "corelink.billing.sink_failure",
            Self::IdempotencyCollision => "corelink.billing.idempotency_collision",
        }
    }

    /// Whether this variant fires SEV-1 alerting per WI-S10-001 §6.1.
    /// Idempotency collision is the canonical SEV-1 trigger
    /// (probability < 2^-128 under random inputs; observation = caller
    /// bug or BLAKE3 implementation defect — both warrant immediate
    /// operations attention).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::IdempotencyCollision)
    }

    /// Whether this variant fires SEV-3 alerting per WI-S10-001 §6.1.
    /// Sink failure is SEV-3 — the production wiring is fail-OPEN at
    /// the hot path; eventual delivery via retry queue is canonical;
    /// SEV-3 covers visibility without paging on every transient hop.
    #[must_use]
    pub const fn is_sev3(self) -> bool {
        matches!(self, Self::SinkFailure)
    }
}

impl core::fmt::Display for BillingAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_billing_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.billing.usage_emitted",
        "corelink.billing.duplicate_rejected",
        "corelink.billing.sink_failure",
        "corelink.billing.idempotency_collision",
    ]
}

/// Typed billing-emit meta-audit record. Production wiring serializes
/// via a CloudEvents 1.0 envelope (mirrors the on-the-wire `UsageEvent`
/// shape); the trait surface accepts the typed shape so the sink + the
/// envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BillingAuditRecord {
    /// Canonical event type.
    pub event_type: BillingAuditEventType,
    /// Tenant id of the originating emit (per-tenant audit partition;
    /// matches the per-tenant idempotency tracker partition).
    pub tenant_id: String,
    /// Canonical 64-char hex BLAKE3-256 idempotency key (empty for
    /// taxonomies that don't carry an event context — none today, but
    /// reserved for additive growth).
    pub idem_key_hex: String,
    /// Canonical billing period (`YYYY-MM`) of the originating event.
    pub billing_period: String,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"emitter"` for orchestrator-internal arms.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Audit-of-audit emit error alias (lifted from
/// [`crate::error::BillingAuditSinkError`]).
pub use crate::error::BillingAuditSinkError as BillingAuditEmitError;

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `OutboxBillingAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the R2 PutObject (S-01 audit_outbox table mirrors;
///   fail-CLOSED envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexBillingAuditSink` — fan-out to direct SIEM in addition
///   to the outbox.
pub trait BillingAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to emit
    /// fail-CLOSED at THIS trait surface (per WI-S10-001 §6.1: the
    /// audit envelope is fail-CLOSED; the outer hot path is fail-OPEN
    /// at the customer-facing request — the wrapping production worker
    /// chooses the policy).
    ///
    /// # Errors
    ///
    /// Returns [`BillingAuditEmitError::Store`] on any backend
    /// failure.
    fn emit(&self, record: BillingAuditRecord) -> Result<(), BillingAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryBillingAuditSink {
    inner: std::sync::Arc<Mutex<Vec<BillingAuditRecord>>>,
}

impl InMemoryBillingAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<BillingAuditRecord> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of records captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Filter snapshot down to records of a single event type.
    #[must_use]
    pub fn snapshot_of(
        &self,
        event_type: BillingAuditEventType,
    ) -> Vec<BillingAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl BillingAuditSink for InMemoryBillingAuditSink {
    fn emit(&self, record: BillingAuditRecord) -> Result<(), BillingAuditEmitError> {
        let mut guard = self.inner.lock().map_err(|_| {
            BillingAuditEmitError::Store("billing audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope.
#[derive(Debug, Default)]
pub struct FailingBillingAuditSink;

impl FailingBillingAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl BillingAuditSink for FailingBillingAuditSink {
    fn emit(&self, _record: BillingAuditRecord) -> Result<(), BillingAuditEmitError> {
        Err(BillingAuditEmitError::Store(
            "induced billing audit sink failure (test fixture)".to_string(),
        ))
    }
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

    fn rec(t: BillingAuditEventType) -> BillingAuditRecord {
        BillingAuditRecord {
            event_type: t,
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            idem_key_hex: "ab".repeat(32),
            billing_period: "2026-05".to_string(),
            created_by_request_id: "test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            BillingAuditEventType::UsageEmitted,
            BillingAuditEventType::DuplicateRejected,
            BillingAuditEventType::SinkFailure,
            BillingAuditEventType::IdempotencyCollision,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.billing."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_billing_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.billing."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(BillingAuditEventType::IdempotencyCollision.is_sev1());
        assert!(!BillingAuditEventType::IdempotencyCollision.is_sev3());
        assert!(BillingAuditEventType::SinkFailure.is_sev3());
        assert!(!BillingAuditEventType::SinkFailure.is_sev1());
        assert!(!BillingAuditEventType::UsageEmitted.is_sev1());
        assert!(!BillingAuditEventType::UsageEmitted.is_sev3());
        assert!(!BillingAuditEventType::DuplicateRejected.is_sev1());
        assert!(!BillingAuditEventType::DuplicateRejected.is_sev3());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryBillingAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(BillingAuditEventType::UsageEmitted)).unwrap();
        sink.emit(rec(BillingAuditEventType::DuplicateRejected)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(BillingAuditEventType::UsageEmitted).len(),
            1
        );
        assert_eq!(
            sink.snapshot_of(BillingAuditEventType::DuplicateRejected).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingBillingAuditSink::new();
        let err = sink
            .emit(rec(BillingAuditEventType::UsageEmitted))
            .unwrap_err();
        assert!(matches!(err, BillingAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryBillingAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(BillingAuditEventType::UsageEmitted)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", BillingAuditEventType::IdempotencyCollision),
            "corelink.billing.idempotency_collision"
        );
    }
}
