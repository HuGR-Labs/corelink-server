//! Audit-of-audit (meta-audit) trait + InMemory test sink.
//!
//! Mirrors `corelink-logpush::audit` discipline at one meta-level higher
//! — every chain emit + verify decision arm fires its own audit event
//! BEFORE state mutation per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`. The
//! audit chain itself is THE compliance primitive (SOC 2 CC7.2); a
//! corrupt chain emit that didn't surface a meta-audit row would mean
//! the verifier cannot retroactively diagnose the root cause.
//!
//! WI-S09-004 §6.1.1 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.audit_chain.event_appended` — emitted on every successful
//!   `AuditEvent` accepted by the sink (after canonicalization +
//!   chain-head advance + R2 NDJSON write).
//! - `corelink.audit_chain.chain_verified_ok` — emitted on every
//!   verifier run that walked a chain start → end without detecting a
//!   break (informational; SLO ≤ 5min p99 per WI §3 SLA).
//! - `corelink.audit_chain.chain_break_detected` — emitted on every
//!   verifier run that detected a chain break (SEV-0 source per WI
//!   §6.1.10; fires `corelink_audit_chain_break_detected_total`
//!   counter + RB-AUDIT-CHAIN-001 runbook trigger).
//! - `corelink.audit_chain.sink_failure` — emitted on every R2
//!   PutObject transport failure (production wiring fail-CLOSED per
//!   Lote 10.6bis pattern + WI §6.1.9; transaction abort canonical).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (S-13 admin / S-19
//! Stripe / S-11 DSR) can extend the taxonomy additively without
//! breaking downstream sinks.

// DEBT-013 OPT-04 phase 1 — swapped `std::sync::Mutex` →
// `parking_lot::Mutex` 2026-05-15. Infallible lock; the "audit sink
// mutex poisoned" error string returned by the in-memory sink impl
// below is now structurally unreachable. The `Store` variant remains
// the canonical surface for transport-class failures on production
// R2 / Cloudflare Queue sinks (and is still exercised by the
// adversarial `FailingAuditChainAuditSink` test fixture).
use parking_lot::Mutex;

/// Canonical audit-chain meta-audit taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AuditChainAuditEventType {
    /// `corelink.audit_chain.event_appended` — successful chain emit
    /// (the sink accepted the event + the chain head advanced + the R2
    /// NDJSON write committed).
    EventAppended,
    /// `corelink.audit_chain.chain_verified_ok` — verifier walk
    /// completed start → end without detecting a break (informational).
    ChainVerifiedOk,
    /// `corelink.audit_chain.chain_break_detected` — verifier walk
    /// detected a hash-chain break at some sequence (SEV-0 source per
    /// WI §6.1.10).
    ChainBreakDetected,
    /// `corelink.audit_chain.sink_failure` — R2 PutObject transport
    /// failure (production wiring fail-CLOSED per WI §6.1.9 + Lote
    /// 10.6bis pattern; transaction abort canonical).
    SinkFailure,
}

impl AuditChainAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EventAppended => "corelink.audit_chain.event_appended",
            Self::ChainVerifiedOk => "corelink.audit_chain.chain_verified_ok",
            Self::ChainBreakDetected => "corelink.audit_chain.chain_break_detected",
            Self::SinkFailure => "corelink.audit_chain.sink_failure",
        }
    }

    /// Whether this variant fires SEV-0 alerting per WI §6.1.10. Chain
    /// break detection is the canonical SEV-0 trigger (SOC 2 CC7.2
    /// audit-chain-integrity gap = compliance failure = customer
    /// contract loss; 4% global revenue regulatory fine risk).
    #[must_use]
    pub const fn is_sev0(self) -> bool {
        matches!(self, Self::ChainBreakDetected)
    }

    /// Whether this variant fires SEV-1 alerting per WI §6.1.10. Sink
    /// failure is SEV-1 (audit emit fail-CLOSED transaction abort —
    /// downstream impact is a 503 to the originating request, not a
    /// compliance gap, but operations need immediate visibility).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::SinkFailure)
    }
}

impl core::fmt::Display for AuditChainAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.audit_chain.event_appended",
        "corelink.audit_chain.chain_verified_ok",
        "corelink.audit_chain.chain_break_detected",
        "corelink.audit_chain.sink_failure",
    ]
}

/// Typed audit-chain meta-audit record. Production wiring serializes
/// via a CloudEvents 1.0 envelope (mirrors the on-the-wire `AuditEvent`
/// shape); the trait surface accepts the typed shape so the sink + the
/// envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditChainAuditRecord {
    /// Canonical event type.
    pub event_type: AuditChainAuditEventType,
    /// Canonical CloudEvents `subject` of the originating chain event
    /// (e.g. `cas:put`); allows downstream filtering by the event's
    /// CNCF subject.
    pub chain_subject: &'static str,
    /// Tenant id of the originating chain (per-tenant chain partition
    /// per WI §6.1.4).
    pub tenant_id: String,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"verifier"` for daily-verify routine emits.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
    /// Sequence number of the originating chain event (for chain emit
    /// arms) / first divergence sequence (for chain break detection) /
    /// `0` for sink failures with no event context.
    pub sequence_number: u64,
}

/// Errors surfaced by [`AuditChainAuditSink::emit`].
pub use crate::error::AuditChainAuditSinkError as AuditChainAuditEmitError;

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `OutboxAuditChainAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the R2 PutObject (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexAuditChainAuditSink` — fan-out to direct SIEM in addition
///   to the outbox.
pub trait AuditChainAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to chain
    /// emit fail-CLOSED per WI §6.1.9 (audit data integrity > availability;
    /// missing audit event = 4% global revenue regulatory fine risk).
    ///
    /// # Errors
    ///
    /// Returns [`AuditChainAuditEmitError::Store`] on any backend
    /// failure.
    fn emit(&self, record: AuditChainAuditRecord) -> Result<(), AuditChainAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryAuditChainAuditSink {
    inner: std::sync::Arc<Mutex<Vec<AuditChainAuditRecord>>>,
}

impl InMemoryAuditChainAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditChainAuditRecord> {
        // parking_lot lock is infallible — no PoisonError arm needed.
        self.inner.lock().clone()
    }

    /// Number of records captured.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Filter snapshot down to records of a single event type.
    #[must_use]
    pub fn snapshot_of(&self, event_type: AuditChainAuditEventType) -> Vec<AuditChainAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl AuditChainAuditSink for InMemoryAuditChainAuditSink {
    fn emit(&self, record: AuditChainAuditRecord) -> Result<(), AuditChainAuditEmitError> {
        // parking_lot lock is infallible. The `Store("audit sink
        // mutex poisoned")` arm is preserved on the trait surface
        // (FailingAuditChainAuditSink uses Store for induced
        // failures; production R2/Queue sinks need it for transport
        // errors).
        let mut guard = self.inner.lock();
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope.
#[derive(Debug, Default)]
pub struct FailingAuditChainAuditSink;

impl FailingAuditChainAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AuditChainAuditSink for FailingAuditChainAuditSink {
    fn emit(&self, _record: AuditChainAuditRecord) -> Result<(), AuditChainAuditEmitError> {
        Err(AuditChainAuditEmitError::Store(
            "induced audit-chain audit sink failure (test fixture)".to_string(),
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

    fn rec(t: AuditChainAuditEventType) -> AuditChainAuditRecord {
        AuditChainAuditRecord {
            event_type: t,
            chain_subject: "cas:put",
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            created_by_request_id: "test".to_string(),
            now_ms: 1,
            sequence_number: 0,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            AuditChainAuditEventType::EventAppended,
            AuditChainAuditEventType::ChainVerifiedOk,
            AuditChainAuditEventType::ChainBreakDetected,
            AuditChainAuditEventType::SinkFailure,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.audit_chain."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.audit_chain."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(AuditChainAuditEventType::ChainBreakDetected.is_sev0());
        assert!(!AuditChainAuditEventType::ChainBreakDetected.is_sev1());
        assert!(AuditChainAuditEventType::SinkFailure.is_sev1());
        assert!(!AuditChainAuditEventType::SinkFailure.is_sev0());
        assert!(!AuditChainAuditEventType::EventAppended.is_sev0());
        assert!(!AuditChainAuditEventType::EventAppended.is_sev1());
        assert!(!AuditChainAuditEventType::ChainVerifiedOk.is_sev0());
        assert!(!AuditChainAuditEventType::ChainVerifiedOk.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryAuditChainAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(AuditChainAuditEventType::EventAppended))
            .unwrap();
        sink.emit(rec(AuditChainAuditEventType::ChainVerifiedOk))
            .unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(AuditChainAuditEventType::EventAppended)
                .len(),
            1
        );
        assert_eq!(
            sink.snapshot_of(AuditChainAuditEventType::ChainVerifiedOk)
                .len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingAuditChainAuditSink::new();
        let err = sink
            .emit(rec(AuditChainAuditEventType::EventAppended))
            .unwrap_err();
        assert!(matches!(err, AuditChainAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryAuditChainAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(AuditChainAuditEventType::EventAppended))
            .unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", AuditChainAuditEventType::ChainBreakDetected),
            "corelink.audit_chain.chain_break_detected"
        );
    }
}
