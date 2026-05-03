//! Audit-of-audit (meta-audit) trait + InMemory test sink for the
//! billing-aggregator surface.
//!
//! Mirrors the `corelink-billing-emit::audit` discipline at the
//! aggregator layer — every run decision arm fires its own audit event
//! BEFORE state mutation per the canonical Lote 10.6bis pattern + S-07
//! P1-1 fix. The aggregator is THE counter-integrity primitive between
//! raw events (R2 immutable archive) and Stripe invoice generation
//! (WI-S10-003); a corrupt aggregation decision that didn't surface a
//! meta-audit row would mean the reconciliation worker (WI-S10-004)
//! cannot retroactively diagnose the root cause.
//!
//! WI-S10-002 §6.1 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.billing_aggregator.run_started` — fires at the start of
//!   every aggregator run (BEFORE any chain state observation +
//!   counter store read; pins the run-start watermark for forensic
//!   reconstruction).
//! - `corelink.billing_aggregator.run_completed` — fires after the
//!   counter store + chain head advance, OR on the no-op /
//!   skipped-duplicate arm. The audit carries the `AggregationDecision`
//!   discriminant so the dashboard can bucket runs by outcome.
//! - `corelink.billing_aggregator.chain_break_detected` — fires when
//!   the chain head observed at run-start does NOT match the chain head
//!   recomputed from the counter store. CRITICAL post-mortem trigger;
//!   admin INSERT bypass / R2 raw retroactive deletion / BLAKE3
//!   implementation defect.
//! - `corelink.billing_aggregator.sink_failure` — fires when the
//!   counter store rejects the UPSERT (D1 transaction rollback /
//!   network partition / IAM denial). Per WI-S10-002 §1 invariant 7
//!   the aggregator is fail-CLOSED: the audit fires BEFORE the error
//!   propagates so observability is complete.
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (WI-S10-004
//! reconciliation / WI-S10-007 PRR) can extend the taxonomy additively
//! without breaking downstream sinks.

use std::sync::Mutex;
use uuid::Uuid;

use corelink_billing_emit::UsageEventKind;

/// Canonical billing-aggregator meta-audit taxonomy. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AggregatorAuditEventType {
    /// `corelink.billing_aggregator.run_started` — aggregator run
    /// commenced; pins the run-start watermark.
    RunStarted,
    /// `corelink.billing_aggregator.run_completed` — aggregator run
    /// completed (any decision arm); the audit carries the
    /// `AggregationDecision` discriminant.
    RunCompleted,
    /// `corelink.billing_aggregator.chain_break_detected` — chain head
    /// integrity violation observed. CRITICAL post-mortem trigger.
    ChainBreakDetected,
    /// `corelink.billing_aggregator.sink_failure` — counter store
    /// rejected the UPSERT (D1 transaction rollback / network partition).
    SinkFailure,
}

impl AggregatorAuditEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "corelink.billing_aggregator.run_started",
            Self::RunCompleted => "corelink.billing_aggregator.run_completed",
            Self::ChainBreakDetected => "corelink.billing_aggregator.chain_break_detected",
            Self::SinkFailure => "corelink.billing_aggregator.sink_failure",
        }
    }

    /// Whether this variant fires SEV-1 alerting per WI-S10-002 §6.1.17.
    /// Chain break + sink failure are the canonical SEV-1 triggers
    /// (chain break = tampering signal; sink failure = aggregation halt
    /// = invoice generation blocked downstream).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::ChainBreakDetected | Self::SinkFailure)
    }
}

impl core::fmt::Display for AggregatorAuditEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_aggregator_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.billing_aggregator.run_started",
        "corelink.billing_aggregator.run_completed",
        "corelink.billing_aggregator.chain_break_detected",
        "corelink.billing_aggregator.sink_failure",
    ]
}

/// Typed billing-aggregator meta-audit record. Production wiring
/// serializes via a CloudEvents 1.0 envelope (mirrors the on-the-wire
/// `AggregatedCounter` shape); the trait surface accepts the typed
/// shape so the sink + the envelope serializer share an unambiguous
/// contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatorAuditRecord {
    /// Canonical event type.
    pub event_type: AggregatorAuditEventType,
    /// Tenant id of the originating run (per-tenant audit partition).
    pub tenant_id: Uuid,
    /// Canonical billing period (`YYYY-MM`).
    pub billing_period: String,
    /// Canonical event kind (the bucketed
    /// `UsageEventKind::as_str()` discriminant).
    pub event_kind: UsageEventKind,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
    /// Free-form context (e.g. observed chain head hex, recomputed
    /// chain head hex, error string from the failure path). Reserved
    /// for adversarial debug + dashboard widget grouping.
    pub context: String,
}

/// Audit-of-audit emit error alias (lifted from
/// [`AggregatorAuditEmitError`]).
pub use crate::error::AggregatorAuditSinkError as AggregatorAuditEmitError;

/// Audit-of-audit sink trait. Production wiring composes:
///
/// - `OutboxAggregatorAuditSink` — D1 INSERT into `audit_outbox` in the
///   same batch as the counter UPSERT (S-01 audit_outbox table mirrors;
///   fail-CLOSED envelope per Lote 10.6bis pattern + S-07 P1-1 fix).
/// - `MultiplexAggregatorAuditSink` — fan-out to direct SIEM in
///   addition to the outbox.
pub trait AggregatorAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to abort
    /// the run fail-CLOSED at THIS trait surface (per WI-S10-002 §1
    /// invariant 7: the aggregator is fail-CLOSED — no downstream
    /// fail-OPEN policy override).
    ///
    /// # Errors
    ///
    /// Returns [`AggregatorAuditEmitError::Store`] on any backend
    /// failure.
    fn emit(
        &self,
        record: AggregatorAuditRecord,
    ) -> Result<(), AggregatorAuditEmitError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryAggregatorAuditSink {
    inner: std::sync::Arc<Mutex<Vec<AggregatorAuditRecord>>>,
}

impl InMemoryAggregatorAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AggregatorAuditRecord> {
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
        event_type: AggregatorAuditEventType,
    ) -> Vec<AggregatorAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl AggregatorAuditSink for InMemoryAggregatorAuditSink {
    fn emit(
        &self,
        record: AggregatorAuditRecord,
    ) -> Result<(), AggregatorAuditEmitError> {
        let mut guard = self.inner.lock().map_err(|_| {
            AggregatorAuditEmitError::Store(
                "billing-aggregator audit sink mutex poisoned".to_string(),
            )
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingAggregatorAuditSink;

impl FailingAggregatorAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AggregatorAuditSink for FailingAggregatorAuditSink {
    fn emit(
        &self,
        _record: AggregatorAuditRecord,
    ) -> Result<(), AggregatorAuditEmitError> {
        Err(AggregatorAuditEmitError::Store(
            "induced billing-aggregator audit sink failure (test fixture)".to_string(),
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

    fn rec(t: AggregatorAuditEventType) -> AggregatorAuditRecord {
        AggregatorAuditRecord {
            event_type: t,
            tenant_id: Uuid::now_v7(),
            billing_period: "2026-05".to_string(),
            event_kind: UsageEventKind::CasPut,
            now_ms: 1,
            context: "test".to_string(),
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            AggregatorAuditEventType::RunStarted,
            AggregatorAuditEventType::RunCompleted,
            AggregatorAuditEventType::ChainBreakDetected,
            AggregatorAuditEventType::SinkFailure,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.billing_aggregator."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_aggregator_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.billing_aggregator."));
        }
    }

    #[test]
    fn sev_classification_pinned() {
        assert!(AggregatorAuditEventType::ChainBreakDetected.is_sev1());
        assert!(AggregatorAuditEventType::SinkFailure.is_sev1());
        assert!(!AggregatorAuditEventType::RunStarted.is_sev1());
        assert!(!AggregatorAuditEventType::RunCompleted.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryAggregatorAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(AggregatorAuditEventType::RunStarted)).unwrap();
        sink.emit(rec(AggregatorAuditEventType::RunCompleted)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(AggregatorAuditEventType::RunStarted).len(),
            1
        );
        assert_eq!(
            sink.snapshot_of(AggregatorAuditEventType::RunCompleted).len(),
            1
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingAggregatorAuditSink::new();
        let err = sink
            .emit(rec(AggregatorAuditEventType::RunStarted))
            .unwrap_err();
        assert!(matches!(err, AggregatorAuditEmitError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryAggregatorAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(AggregatorAuditEventType::RunStarted)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", AggregatorAuditEventType::ChainBreakDetected),
            "corelink.billing_aggregator.chain_break_detected"
        );
    }
}
