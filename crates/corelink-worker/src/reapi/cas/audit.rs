//! Multipart-blob audit emit trait + InMemory test sink (WI-S05-001
//! §6.1.7).
//!
//! ## Why a thin trait surface (mirrors `reapi::ac::audit`)
//!
//! The S-03 `corelink-audit::Emitter` taxonomy covers the `auth.*`
//! family; the AC handler's `corelink.ac.*` events live behind a
//! parallel sink in `reapi::ac::audit`. The SplitBlob/SpliceBlob flow
//! emits **CAS-multipart** events (`corelink.blob.split.*` /
//! `corelink.blob.splice.*`) which belong to the **CAS audit family**.
//! We follow the same pattern: a small AC-flavor sink trait the
//! production wiring composes on top of `audit_outbox` (WI-S01-004).

use core::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use thiserror::Error;
use uuid::Uuid;

use super::types::{BlobDigest, ChunkIndex, ManifestDigest, SessionId};
use crate::Region;

/// Canonical 5-variant SplitBlob/SpliceBlob event-type taxonomy per
/// WI-S05-001 brief.
///
/// The 5 canonical variants (`blob.split.start` /
/// `blob.split.chunk_appended` / `blob.split.finalized` /
/// `blob.split.aborted` / `blob.splice.ok`) are explicitly enumerated
/// per the WI brief. Additional sub-variants (`blob.splice.sig_invalid`,
/// `blob.splice.chunk_missing`, etc.) land in WI-S05-005 / -006 along
/// with the manifest verifier wiring; the enum is `#[non_exhaustive]`
/// so additive growth is binary-additive only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BlobEventType {
    /// `corelink.blob.split.start` — multipart upload session opened.
    SplitStart,
    /// `corelink.blob.split.chunk_appended` — chunk slot bound to the
    /// session.
    SplitChunkAppended,
    /// `corelink.blob.split.finalized` — manifest sealed. Idempotent
    /// re-finalize is also reported with this event.
    SplitFinalized,
    /// `corelink.blob.split.aborted` — session aborted (idempotent
    /// retry on Aborted is also reported with this event; reason
    /// distinguishes via canonical reason code).
    SplitAborted,
    /// `corelink.blob.splice.ok` — successful streaming reassembly.
    SpliceOk,
}

impl BlobEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SplitStart => "corelink.blob.split.start",
            Self::SplitChunkAppended => "corelink.blob.split.chunk_appended",
            Self::SplitFinalized => "corelink.blob.split.finalized",
            Self::SplitAborted => "corelink.blob.split.aborted",
            Self::SpliceOk => "corelink.blob.splice.ok",
        }
    }

    /// Whether this variant maps to a SEV-1 alert. None of the current
    /// 5 variants are SEV-1 (tampering / verify-fail variants land
    /// alongside the manifest verifier in WI-S05-005); kept for
    /// forward compatibility.
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        false
    }
}

impl fmt::Display for BlobEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Typed multipart audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope (mirrors `corelink-audit::AuditEnvelope`);
/// the trait surface accepts the typed shape so the sink + the envelope
/// serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobAuditRecord {
    /// Canonical event type.
    pub event_type: BlobEventType,
    /// Verified tenant id.
    pub tenant_id: Uuid,
    /// Region the request was issued under.
    pub region: Region,
    /// Source blob digest the flow targets, when known. `None` on
    /// path that addresses by `session_id` only (e.g.
    /// `chunk_appended`).
    pub blob_digest: Option<BlobDigest>,
    /// Manifest digest, when known (post-finalize / splice).
    pub manifest_digest: Option<ManifestDigest>,
    /// Server-minted session id, when known.
    pub session_id: Option<SessionId>,
    /// Chunk index, when known (canonical on `chunk_appended`
    /// records).
    pub chunk_index: Option<ChunkIndex>,
    /// Correlation id from the gRPC `x-request-id` header (canonical
    /// REAPI propagation).
    pub request_id: String,
    /// Per-WI extra context — short canonical reason code (e.g.
    /// `"started"` / `"already_chunked"` / `"out_of_order"` /
    /// `"chunk_too_large"`). Empty when no extra context applies.
    pub reason: &'static str,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`AuditSink::emit`].
#[derive(Debug, Error)]
pub enum AuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout).
    #[error("blob audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the session-row mutation (WI-S05-006).
/// - `MultiplexAuditSink` — fan-outs SEV-1 to direct SIEM in addition
///   to the outbox (forward; no SEV-1 variants in this WI).
///
/// Production wiring is sync (D1 batch is itself sync at the trait
/// surface), so this trait is sync to match the
/// `corelink-audit::Emitter` shape + the `reapi::ac::audit::AuditSink`
/// canonical shape.
pub trait AuditSink: Send + Sync {
    /// Persist `record` durably. The handler maps a non-`Ok` return to
    /// 503 so the audit gap doesn't leak to the client as a 200 (per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`AuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: BlobAuditRecord) -> Result<(), AuditSinkError>;
}

/// Test-only audit sink that captures every emitted record in an
/// in-memory `Vec`. Cloning shares the underlying buffer
/// (`Arc<Mutex<Vec<…>>>`).
#[derive(Clone, Default)]
pub struct InMemoryAuditSink {
    inner: Arc<Mutex<Vec<BlobAuditRecord>>>,
}

impl fmt::Debug for InMemoryAuditSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryAuditSink").finish_non_exhaustive()
    }
}

impl InMemoryAuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Snapshot every record captured so far. Returns a clone.
    #[must_use]
    pub fn snapshot(&self) -> Vec<BlobAuditRecord> {
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

    /// Filter snapshot down to records of a single canonical event
    /// type — convenience for property tests asserting per-flow audit
    /// emission.
    #[must_use]
    pub fn snapshot_of(&self, event_type: BlobEventType) -> Vec<BlobAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }

    fn lock(&self) -> Result<MutexGuard<'_, Vec<BlobAuditRecord>>, AuditSinkError> {
        self.inner
            .lock()
            .map_err(|_| AuditSinkError::Store("blob audit sink mutex poisoned".to_string()))
    }
}

impl AuditSink for InMemoryAuditSink {
    fn emit(&self, record: BlobAuditRecord) -> Result<(), AuditSinkError> {
        let mut guard = self.lock()?;
        guard.push(record);
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use corelink_hash::Digest;

    fn rec(t: BlobEventType) -> BlobAuditRecord {
        BlobAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            region: Region::Wnam,
            blob_digest: Some(BlobDigest::new(Digest::compute(b"x"), 1)),
            manifest_digest: None,
            session_id: None,
            chunk_index: None,
            request_id: "req-1".to_string(),
            reason: "",
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = vec![
            BlobEventType::SplitStart,
            BlobEventType::SplitChunkAppended,
            BlobEventType::SplitFinalized,
            BlobEventType::SplitAborted,
            BlobEventType::SpliceOk,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.blob."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {}", t);
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn no_variants_are_sev1_in_this_wi() {
        for t in [
            BlobEventType::SplitStart,
            BlobEventType::SplitChunkAppended,
            BlobEventType::SplitFinalized,
            BlobEventType::SplitAborted,
            BlobEventType::SpliceOk,
        ] {
            assert!(!t.is_sev1());
        }
    }

    #[test]
    fn in_memory_sink_captures_emitted_records() {
        let sink = InMemoryAuditSink::new();
        sink.emit(rec(BlobEventType::SplitStart)).unwrap();
        sink.emit(rec(BlobEventType::SplitFinalized)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(BlobEventType::SplitStart).len(), 1);
        assert_eq!(sink.snapshot_of(BlobEventType::SplitFinalized).len(), 1);
    }

    #[test]
    fn clone_shares_buffer() {
        let a = InMemoryAuditSink::new();
        let b = a.clone();
        a.emit(rec(BlobEventType::SplitStart)).unwrap();
        assert_eq!(b.len(), 1);
    }
}
