//! Dedup-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a thin trait surface
//!
//! Mirrors `corelink-gc::audit` + `corelink-worker::reapi::cas::audit`:
//! a small dedup-flavoured sink trait the production wiring composes on
//! top of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S07-001 §6.1.7 freezes the canonical 1-event taxonomy:
//!
//! - `corelink.dedup.find_missing_blobs_executed` — emitted ONCE per
//!   handler invocation, after the set-difference is computed and
//!   BEFORE the response is returned. The fail-closed envelope ensures
//!   that an audit emission failure aborts the handler with 503 (per
//!   `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (eviction, quota,
//! cross-tenant policy decisions) can extend the taxonomy additively
//! without breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

/// Canonical Dedup audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for S-07 follow-on WIs (eviction +
/// quota events lift here when the handler emits structured records).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DedupEventType {
    /// `corelink.dedup.find_missing_blobs_executed` — one record per
    /// handler invocation. Captures `tenant_id`, queried-batch size,
    /// missing-set size, duration_ms, source attribution.
    FindMissingBlobsExecuted,
}

impl DedupEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FindMissingBlobsExecuted => "corelink.dedup.find_missing_blobs_executed",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox per `corelink-audit::Emitter` fan-out).
    /// `FindMissingBlobsExecuted` is NOT SEV-1 — the per-request volume
    /// would saturate SIEM ingest; alerts fire at the metric layer
    /// instead (>0% `cross_tenant_blocked_total` — see
    /// `corelink.dedup.find_missing_blobs_total{result=denied}`
    /// dimension).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        match self {
            Self::FindMissingBlobsExecuted => false,
        }
    }
}

impl core::fmt::Display for DedupEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 1] {
    &["corelink.dedup.find_missing_blobs_executed"]
}

/// Outcome category surfaced in the audit record `outcome` field — the
/// S-09 chain processor projects this onto the
/// `corelink.dedup.find_missing_blobs_total{result}` Prometheus
/// dimension. Three categories cover every legal terminal state of the
/// handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DedupAuditOutcome {
    /// Handler returned a successful set-difference (the missing
    /// digests subset; possibly empty).
    Ok,
    /// Handler aborted on cross-tenant attempt (CTRL-ISO-005).
    Denied,
    /// Handler aborted on a transport / programmer error
    /// (BatchTooLarge / IndexBackend / InvalidChunkDigest).
    Error,
}

impl DedupAuditOutcome {
    /// Canonical lower-snake-case label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Denied => "denied",
            Self::Error => "error",
        }
    }
}

/// Typed Dedup audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedupAuditRecord {
    /// Canonical event type.
    pub event_type: DedupEventType,
    /// Verified tenant id (extracted from the `TenantCtx` Tower
    /// middleware; never from the request body / headers).
    pub tenant_id: Uuid,
    /// Number of digests in the handler input batch.
    pub queried_count: u32,
    /// Number of digests that were missing in the tenant's chunks
    /// table (set-difference size; <= queried_count).
    pub missing_count: u32,
    /// Handler-observed wall-clock duration (ms).
    pub duration_ms: u64,
    /// Outcome category — projects onto the
    /// `corelink.dedup.find_missing_blobs_total{result}` dimension.
    pub outcome: DedupAuditOutcome,
    /// Source attribution: the request id (`x-request-id` header or
    /// equivalent) so the S-09 chain processor can correlate this
    /// record with the upstream gRPC trace.
    pub created_by_request_id: String,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`DedupAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DedupAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("dedup audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the handler response (S-01 audit_outbox table).
/// - `MultiplexAuditSink` — fan-out SEV-1 to direct SIEM in addition
///   to the outbox (forward; no SEV-1 events in the dedup taxonomy yet
///   per WI §6.1.7).
pub trait DedupAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`DedupAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: DedupAuditRecord) -> Result<(), DedupAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryDedupAuditSink {
    inner: std::sync::Arc<Mutex<Vec<DedupAuditRecord>>>,
}

impl InMemoryDedupAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<DedupAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: DedupEventType) -> Vec<DedupAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl DedupAuditSink for InMemoryDedupAuditSink {
    fn emit(&self, record: DedupAuditRecord) -> Result<(), DedupAuditSinkError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DedupAuditSinkError::Store("audit sink mutex poisoned".to_string()))?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 503 when the audit emit fires the
/// `Store` error).
#[derive(Debug, Default)]
pub struct FailingDedupAuditSink;

impl FailingDedupAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DedupAuditSink for FailingDedupAuditSink {
    fn emit(&self, _record: DedupAuditRecord) -> Result<(), DedupAuditSinkError> {
        Err(DedupAuditSinkError::Store(
            "induced audit sink failure (test fixture)".to_string(),
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

    fn rec(t: DedupEventType) -> DedupAuditRecord {
        DedupAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            queried_count: 0,
            missing_count: 0,
            duration_ms: 0,
            outcome: DedupAuditOutcome::Ok,
            created_by_request_id: "req-test".to_string(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_canonical_string() {
        let v = [DedupEventType::FindMissingBlobsExecuted];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.dedup."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        assert_eq!(canonical_audit_event_strings().len(), 1);
        assert_eq!(
            canonical_audit_event_strings()[0],
            DedupEventType::FindMissingBlobsExecuted.as_str()
        );
    }

    #[test]
    fn sev1_taxonomy_currently_empty() {
        // No SEV-1 events in the dedup taxonomy at WI-S07-001 ship.
        // Per-request volume of FindMissingBlobs would saturate SIEM
        // ingest; alerts fire at the metric layer instead.
        assert!(!DedupEventType::FindMissingBlobsExecuted.is_sev1());
    }

    #[test]
    fn outcome_canonical_strings() {
        assert_eq!(DedupAuditOutcome::Ok.as_str(), "ok");
        assert_eq!(DedupAuditOutcome::Denied.as_str(), "denied");
        assert_eq!(DedupAuditOutcome::Error.as_str(), "error");
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryDedupAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(DedupEventType::FindMissingBlobsExecuted))
            .unwrap();
        sink.emit(rec(DedupEventType::FindMissingBlobsExecuted))
            .unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(
            sink.snapshot_of(DedupEventType::FindMissingBlobsExecuted)
                .len(),
            2
        );
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingDedupAuditSink::new();
        let err = sink
            .emit(rec(DedupEventType::FindMissingBlobsExecuted))
            .unwrap_err();
        assert!(matches!(err, DedupAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryDedupAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(DedupEventType::FindMissingBlobsExecuted))
            .unwrap();
        assert_eq!(s2.len(), 1);
    }
}
