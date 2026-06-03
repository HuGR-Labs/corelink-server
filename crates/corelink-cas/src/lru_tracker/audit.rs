//! LRU tracker audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-eviction::audit` + `corelink-quota::audit`: a small
//! lru-flavoured sink trait the production wiring composes on top of
//! the `audit_outbox` (WI-S01-004) row insert. The S-09 audit chain
//! processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S07-004 §6.1.8 freezes the canonical 4-event taxonomy:
//!
//! - `corelink.lru.access_recorded` — emitted on a `record_access` that
//!   actually mutates the in-memory queue (Recorded decision arm).
//! - `corelink.lru.batch_flushed` — emitted on a successful flush pass
//!   (carries the count flushed + drift_ms observed).
//! - `corelink.lru.batch_failed` — emitted on a flush that fired the
//!   per-row backend error path (production rolls back D1 batch).
//! - `corelink.lru.consistency_violation_detected` — emitted when the
//!   tracker observes `last_accessed_at_ms in D1` is OLDER than the most
//!   recent recorded access by more than the configured drift threshold
//!   (R-S07-3 + spec contract §6 DoD; WI §6.1.9 cycle 6 added metric).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (DASH-EVICT widget,
//! `corelink.lru.access_dedup_total` counter) can extend the taxonomy
//! additively without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use corelink_eviction::EvictionRegion;

/// Canonical LRU audit taxonomy. The `#[non_exhaustive]` marker reserves
/// additive growth for S-07 follow-on WIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LruEventType {
    /// `corelink.lru.access_recorded` — a `record_access` call enqueued
    /// (or coalesced) a (tenant, digest) entry into the in-memory queue.
    AccessRecorded,
    /// `corelink.lru.batch_flushed` — a flush pass committed N rows to
    /// the conditional monotone UPDATE.
    BatchFlushed,
    /// `corelink.lru.batch_failed` — a flush pass fired the per-row
    /// backend error (production rolls back D1 batch + retries on next
    /// cadence).
    BatchFailed,
    /// `corelink.lru.consistency_violation_detected` — INV-LRU-CONSISTENCY
    /// guard fired (drift > threshold). Alert SEV-1 per
    /// `corelink_lru_consistency_violation_total > 0` in WI-S07-005.
    ConsistencyViolationDetected,
}

impl LruEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccessRecorded => "corelink.lru.access_recorded",
            Self::BatchFlushed => "corelink.lru.batch_flushed",
            Self::BatchFailed => "corelink.lru.batch_failed",
            Self::ConsistencyViolationDetected => "corelink.lru.consistency_violation_detected",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `ConsistencyViolationDetected` IS SEV-1 (operator pager wake-up;
    /// tenant data-loss-vector signal).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::ConsistencyViolationDetected)
    }
}

impl core::fmt::Display for LruEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 4] {
    &[
        "corelink.lru.access_recorded",
        "corelink.lru.batch_flushed",
        "corelink.lru.batch_failed",
        "corelink.lru.consistency_violation_detected",
    ]
}

/// Typed LRU audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LruAuditRecord {
    /// Canonical event type.
    pub event_type: LruEventType,
    /// Verified tenant id (extracted from the `AuthCtx` Tower
    /// middleware; never from the request body / headers per Lote
    /// 10.4bis lesson). `None` for cross-tenant events (BatchFlushed
    /// where the batch spans tenants).
    pub tenant_id: Option<Uuid>,
    /// Region scope.
    pub region: EvictionRegion,
    /// Count of rows touched by THIS event (Recorded = 1; Flushed /
    /// Failed = batch size; ConsistencyViolation = the offending row
    /// count).
    pub rows: u64,
    /// Drift observed (Unix ms) — `Some(n)` for `BatchFlushed` /
    /// `ConsistencyViolationDetected`; `None` otherwise.
    pub drift_ms: Option<u64>,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`LruAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LruAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("lru audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the conditional UPDATE (S-01 audit_outbox table;
///   fail-closed envelope per Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (forward; `ConsistencyViolationDetected` IS SEV-1).
pub trait LruAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`LruAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: LruAuditRecord) -> Result<(), LruAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryLruAuditSink {
    inner: std::sync::Arc<Mutex<Vec<LruAuditRecord>>>,
}

impl InMemoryLruAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<LruAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: LruEventType) -> Vec<LruAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl LruAuditSink for InMemoryLruAuditSink {
    fn emit(&self, record: LruAuditRecord) -> Result<(), LruAuditSinkError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LruAuditSinkError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope.
#[derive(Debug, Default)]
pub struct FailingLruAuditSink;

impl FailingLruAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl LruAuditSink for FailingLruAuditSink {
    fn emit(&self, _record: LruAuditRecord) -> Result<(), LruAuditSinkError> {
        Err(LruAuditSinkError::Store(
            "induced lru audit sink failure (test fixture)".to_string(),
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

    fn rec(t: LruEventType) -> LruAuditRecord {
        LruAuditRecord {
            event_type: t,
            tenant_id: Some(Uuid::nil()),
            region: EvictionRegion::Sam,
            rows: 0,
            drift_ms: None,
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            LruEventType::AccessRecorded,
            LruEventType::BatchFlushed,
            LruEventType::BatchFailed,
            LruEventType::ConsistencyViolationDetected,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.lru."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 4);
        for s in canonical {
            assert!(s.starts_with("corelink.lru."));
        }
    }

    #[test]
    fn sev1_subset_only_consistency_violation() {
        assert!(LruEventType::ConsistencyViolationDetected.is_sev1());
        assert!(!LruEventType::AccessRecorded.is_sev1());
        assert!(!LruEventType::BatchFlushed.is_sev1());
        assert!(!LruEventType::BatchFailed.is_sev1());
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryLruAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(LruEventType::AccessRecorded)).unwrap();
        sink.emit(rec(LruEventType::BatchFlushed)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(LruEventType::AccessRecorded).len(), 1);
        assert_eq!(sink.snapshot_of(LruEventType::BatchFlushed).len(), 1);
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingLruAuditSink::new();
        let err = sink.emit(rec(LruEventType::BatchFlushed)).unwrap_err();
        assert!(matches!(err, LruAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryLruAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(LruEventType::AccessRecorded)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", LruEventType::AccessRecorded),
            "corelink.lru.access_recorded"
        );
        assert_eq!(
            format!("{}", LruEventType::ConsistencyViolationDetected),
            "corelink.lru.consistency_violation_detected"
        );
    }
}
