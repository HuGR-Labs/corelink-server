//! Edge-flavoured audit emit trait + InMemory test sink.
//!
//! ## Why a dedicated taxonomy
//!
//! Mirrors `corelink-ratelimit::audit` + `corelink-quota::audit`: a
//! small edge-flavoured sink trait the production wiring composes on
//! top of the `audit_outbox` (WI-S01-004) row insert. The S-09 audit
//! chain processor will lift these records into the canonical
//! `corelink-audit::AuditEnvelope` CloudEvents 1.0 envelope.
//!
//! WI-S08-002 §6.1.10 freezes the canonical 5-event taxonomy:
//!
//! - `corelink.edge.allowed` — emitted on every Allow decision (sampled
//!   at handler layer in production; in tests every decision emits one
//!   record).
//! - `corelink.edge.denied_blocklist` — emitted when the per-IP edge
//!   policy denies via the CIDR blocklist arm.
//! - `corelink.edge.denied_abuse` — emitted when the per-IP edge policy
//!   denies via the runtime abuse arm (callable from outside the
//!   blocklist; reserved for sustained-abuse + ratelimit cascade).
//! - `corelink.edge.blocklist_added` — emitted when an admin adds a
//!   CIDR to the blocklist (BEFORE the D1 INSERT; fail-closed envelope
//!   per Lote 10.6bis pattern).
//! - `corelink.edge.blocklist_removed` — emitted when an admin removes
//!   (or soft-deletes) a CIDR (BEFORE the D1 UPDATE).
//!
//! The enum is `#[non_exhaustive]` so follow-on WIs (suggest_block /
//! reconcile / appeal) can extend the taxonomy additively without
//! breaking downstream sinks.

use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::edge::cidr::Cidr;

/// Canonical edge audit taxonomy. The `#[non_exhaustive]` marker
/// reserves additive growth for WI-S08-002 follow-on work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum EdgeEventType {
    /// `corelink.edge.allowed` — Allow decision (no blocklist hit; no
    /// runtime abuse signal).
    Allowed,
    /// `corelink.edge.denied_blocklist` — denied via CIDR blocklist
    /// (longest-prefix-match hit).
    DeniedBlocklist,
    /// `corelink.edge.denied_abuse` — denied via runtime abuse arm.
    DeniedAbuse,
    /// `corelink.edge.blocklist_added` — admin add audit (BEFORE D1
    /// INSERT; fail-closed envelope).
    BlocklistAdded,
    /// `corelink.edge.blocklist_removed` — admin remove audit (BEFORE
    /// D1 UPDATE; fail-closed envelope).
    BlocklistRemoved,
}

impl EdgeEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "corelink.edge.allowed",
            Self::DeniedBlocklist => "corelink.edge.denied_blocklist",
            Self::DeniedAbuse => "corelink.edge.denied_abuse",
            Self::BlocklistAdded => "corelink.edge.blocklist_added",
            Self::BlocklistRemoved => "corelink.edge.blocklist_removed",
        }
    }

    /// Whether this variant is SEV-1 (always emit to direct SIEM in
    /// addition to the outbox). All edge variants are NOT SEV-1 at this
    /// layer; SEV-1 lives at the metric layer (cross-tenant linkability
    /// canary; not applicable at edge pre-auth boundary).
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        false
    }
}

impl core::fmt::Display for EdgeEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical event-string list for cross-component regression tests +
/// dashboard widget configuration.
#[must_use]
pub const fn canonical_audit_event_strings() -> &'static [&'static str; 5] {
    &[
        "corelink.edge.allowed",
        "corelink.edge.denied_blocklist",
        "corelink.edge.denied_abuse",
        "corelink.edge.blocklist_added",
        "corelink.edge.blocklist_removed",
    ]
}

/// Typed edge audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
///
/// `cidr` is `Option` because the per-request decision arm (`Allowed` /
/// `DeniedAbuse`) does NOT carry a CIDR (no blocklist hit OR runtime
/// abuse signal independent of blocklist).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeAuditRecord {
    /// Canonical event type.
    pub event_type: EdgeEventType,
    /// Canonical CIDR text (for `DeniedBlocklist` / `BlocklistAdded` /
    /// `BlocklistRemoved` arms; `None` for the runtime decision arms).
    pub cidr: Option<Cidr>,
    /// Source attribution: the request id (`x-request-id` header) /
    /// `"sweeper"` for snapshot maintenance / `"admin"` for admin
    /// mutations.
    pub created_by_request_id: String,
    /// Admin id for the `BlocklistAdded` / `BlocklistRemoved` arms; nil
    /// otherwise.
    pub admin_id: Option<Uuid>,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`EdgeAuditSink::emit`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EdgeAuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout / outbox INSERT rejected).
    #[error("edge audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the edge_blocklist mutation (fail-closed envelope per
///   Lote 10.6bis pattern).
/// - `MultiplexAuditSink` — fan-out to direct SIEM in addition to the
///   outbox.
pub trait EdgeAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `record` durably. Caller maps a non-`Ok` return to 503
    /// so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`EdgeAuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: EdgeAuditRecord) -> Result<(), EdgeAuditSinkError>;
}

/// In-memory test audit sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryEdgeAuditSink {
    inner: std::sync::Arc<Mutex<Vec<EdgeAuditRecord>>>,
}

impl InMemoryEdgeAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every record captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<EdgeAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: EdgeEventType) -> Vec<EdgeAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }
}

impl EdgeAuditSink for InMemoryEdgeAuditSink {
    fn emit(&self, record: EdgeAuditRecord) -> Result<(), EdgeAuditSinkError> {
        let mut guard = self.inner.lock().map_err(|_| {
            EdgeAuditSinkError::Store("audit sink mutex poisoned".to_string())
        })?;
        guard.push(record);
        Ok(())
    }
}

/// Always-failing sink for adversarial tests of the fail-closed
/// envelope (handler MUST surface 503 when the audit emit fires the
/// `Store` error).
#[derive(Debug, Default)]
pub struct FailingEdgeAuditSink;

impl FailingEdgeAuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EdgeAuditSink for FailingEdgeAuditSink {
    fn emit(&self, _record: EdgeAuditRecord) -> Result<(), EdgeAuditSinkError> {
        Err(EdgeAuditSinkError::Store(
            "induced edge audit sink failure (test fixture)".to_string(),
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

    fn rec(t: EdgeEventType) -> EdgeAuditRecord {
        EdgeAuditRecord {
            event_type: t,
            cidr: None,
            created_by_request_id: "test".to_string(),
            admin_id: None,
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = [
            EdgeEventType::Allowed,
            EdgeEventType::DeniedBlocklist,
            EdgeEventType::DeniedAbuse,
            EdgeEventType::BlocklistAdded,
            EdgeEventType::BlocklistRemoved,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.edge."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {t}");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_event_strings_match_enum_count() {
        let canonical = canonical_audit_event_strings();
        assert_eq!(canonical.len(), 5);
        for s in canonical {
            assert!(s.starts_with("corelink.edge."));
        }
    }

    #[test]
    fn no_arm_is_sev1_at_audit_layer() {
        for t in [
            EdgeEventType::Allowed,
            EdgeEventType::DeniedBlocklist,
            EdgeEventType::DeniedAbuse,
            EdgeEventType::BlocklistAdded,
            EdgeEventType::BlocklistRemoved,
        ] {
            assert!(!t.is_sev1());
        }
    }

    #[test]
    fn in_memory_sink_captures_records() {
        let sink = InMemoryEdgeAuditSink::new();
        assert!(sink.is_empty());
        sink.emit(rec(EdgeEventType::Allowed)).unwrap();
        sink.emit(rec(EdgeEventType::DeniedBlocklist)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(EdgeEventType::Allowed).len(), 1);
        assert_eq!(sink.snapshot_of(EdgeEventType::DeniedBlocklist).len(), 1);
    }

    #[test]
    fn failing_sink_returns_store_error() {
        let sink = FailingEdgeAuditSink::new();
        let err = sink.emit(rec(EdgeEventType::Allowed)).unwrap_err();
        assert!(matches!(err, EdgeAuditSinkError::Store(_)));
    }

    #[test]
    fn cloned_sink_shares_buffer() {
        let s1 = InMemoryEdgeAuditSink::new();
        let s2 = s1.clone();
        s1.emit(rec(EdgeEventType::Allowed)).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(
            format!("{}", EdgeEventType::Allowed),
            "corelink.edge.allowed"
        );
    }
}
