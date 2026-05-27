//! Audit sink trait + implementations for privacy-notice emit operations.
//!
//! ## Canonical audit ordering (INV-AUDIT-APPEND-ONLY / AC-008)
//!
//! Per Lote 10.6bis split-tier discipline + ADR-S11-002: the notice emit
//! pipeline MUST follow `lookup → emit_audit → mutate_state`. Audit failure
//! aborts the operation fail-CLOSED (state UNCHANGED). This is **regulatory-
//! grade fail-CLOSED** — distinct from billing fail-OPEN at the customer hot
//! path.
//!
//! The [`FailingNoticeAuditSink`] forces the audit-fail-CLOSED test path.
//! AC-008 chaos test: `cargo test -- chaos_audit_emit_failure` verifies state
//! unchanged on audit failure.

use super::error::NoticeAuditSinkError;
use super::event::{
    NoticeCloudEventType, NoticeDeprecatedPayload, NoticePublishedPayload,
};
use std::sync::{Arc, Mutex};

/// 2-arm canonical audit record for privacy notice emit.
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies typed-enum discipline.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoticeAuditRecord {
    /// Audit record for `privacy_notice.published.v1` emission attempt.
    Published {
        /// The payload that was emitted (or attempted).
        payload: NoticePublishedPayload,
        /// CloudEvents type string (canonical).
        cloud_event_type: &'static str,
    },
    /// Audit record for `privacy_notice.deprecated.v1` emission attempt.
    Deprecated {
        /// The payload that was emitted (or attempted).
        payload: NoticeDeprecatedPayload,
        /// CloudEvents type string (canonical).
        cloud_event_type: &'static str,
    },
}

impl NoticeAuditRecord {
    /// Construct a Published audit record.
    #[must_use]
    pub fn published(payload: NoticePublishedPayload) -> Self {
        Self::Published {
            cloud_event_type: NoticeCloudEventType::Published.as_cloud_event_type(),
            payload,
        }
    }

    /// Construct a Deprecated audit record.
    #[must_use]
    pub fn deprecated(payload: NoticeDeprecatedPayload) -> Self {
        Self::Deprecated {
            cloud_event_type: NoticeCloudEventType::Deprecated.as_cloud_event_type(),
            payload,
        }
    }

    /// Canonical CloudEvents type string for this record.
    #[must_use]
    pub const fn cloud_event_type(&self) -> &'static str {
        match self {
            Self::Published { cloud_event_type, .. } => cloud_event_type,
            Self::Deprecated { cloud_event_type, .. } => cloud_event_type,
        }
    }
}

/// Trait for emitting audit records for notice publish + deprecate events.
/// Implementations MUST be fail-CLOSED: if `emit` returns `Err`, the caller
/// MUST abort the state mutation (no Cloudflare Pages deploy, no published
/// notice state update).
pub trait NoticeAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit a single audit record. Called BEFORE state mutation. Failure
    /// MUST cause the caller to abort the deploy (AC-008).
    ///
    /// # Errors
    ///
    /// Returns [`NoticeAuditSinkError`] when audit infrastructure is
    /// unavailable (R2 audit write failure, etc.).
    fn emit(&self, record: NoticeAuditRecord) -> Result<(), NoticeAuditSinkError>;
}

/// In-memory capture sink for tests. Captures all records in a
/// per-instance `Arc<Mutex<Vec<_>>>` (F-001 closure — NEVER
/// `static LazyLock`).
#[derive(Clone, Debug, Default)]
pub struct InMemoryNoticeAuditSink {
    records: Arc<Mutex<Vec<NoticeAuditRecord>>>,
}

impl InMemoryNoticeAuditSink {
    /// Construct a fresh capture sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Drain all captured records (for test assertion).
    ///
    /// # Panics (test only)
    ///
    /// Panics on Mutex poison — acceptable in test context.
    #[cfg(test)]
    #[must_use]
    pub fn drain(&self) -> Vec<NoticeAuditRecord> {
        self.records
            .lock()
            .map(|mut g| std::mem::take(&mut *g))
            .unwrap_or_default()
    }

    /// Snapshot the current record count without draining.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records
            .lock()
            .map(|g| g.len())
            .unwrap_or(0)
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl NoticeAuditSink for InMemoryNoticeAuditSink {
    fn emit(&self, record: NoticeAuditRecord) -> Result<(), NoticeAuditSinkError> {
        self.records
            .lock()
            .map_err(|_| NoticeAuditSinkError::Internal {
                reason: "mutex poisoned".into(),
            })?
            .push(record);
        Ok(())
    }
}

/// Always-failing audit sink — the canonical chaos test fixture for
/// AC-008 (audit emit failure → CD pipeline aborts deploy; state UNCHANGED).
#[derive(Clone, Debug)]
pub struct FailingNoticeAuditSink;

impl NoticeAuditSink for FailingNoticeAuditSink {
    fn emit(&self, _record: NoticeAuditRecord) -> Result<(), NoticeAuditSinkError> {
        Err(NoticeAuditSinkError::Infrastructure {
            reason: "R2 audit-<region> unavailable (chaos simulation)".into(),
        })
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
    use super::super::event::{NoticeVersion, NoticePublishedPayload, VersionBump};
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn make_published_record() -> NoticeAuditRecord {
        let payload = NoticePublishedPayload::new(
            Uuid::nil(),
            NoticeVersion::new(1, 0),
            VersionBump::Minor,
            BTreeMap::new(),
            0,
        );
        NoticeAuditRecord::published(payload)
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryNoticeAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(make_published_record()).unwrap();
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn in_memory_sink_drain_clears() {
        let sink = InMemoryNoticeAuditSink::new();
        sink.emit(make_published_record()).unwrap();
        let drained = sink.drain();
        assert_eq!(drained.len(), 1);
        assert!(sink.is_empty());
    }

    #[test]
    fn failing_sink_returns_err() {
        let sink = FailingNoticeAuditSink;
        let result = sink.emit(make_published_record());
        assert!(result.is_err());
    }

    #[test]
    fn audit_record_cloud_event_type_pin() {
        let rec = make_published_record();
        assert_eq!(
            rec.cloud_event_type(),
            "dev.hugr.corelink.privacy_notice.published.v1"
        );
    }
}
