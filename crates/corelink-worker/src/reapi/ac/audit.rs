//! AC audit emit trait + InMemory test sink (WI-S04-001 §6.1.7 +
//! WI brief).
//!
//! ## Why a thin trait surface (not direct `corelink-audit::Emitter`)
//!
//! `corelink-audit::Emitter` (S-03 WI-S03-007) is the canonical
//! `auth.*` event surface — its `AuthEventType` (from `corelink_audit`)
//! taxonomy covers token / session / membership / WebAuthn / DSR
//! events. AC events (`ac.get.ok` / `ac.get.miss` / `ac.update.ok` /
//! `ac.update.merkle_invalid` / `ac.update.outputs_missing` + the
//! sig_invalid / result_mismatch / outputs_check warning shapes)
//! belong to the **CAS audit family** (`corelink.cas.*` / `corelink.ac.*`)
//! whose canonical taxonomy is owned by `corelink-reapi::audit`
//! (WI-S01-005).
//!
//! Rather than expand the `auth.*` taxonomy with AC-flavor variants
//! (which would couple the auth chain consumer to the AC handler),
//! this module ships a small AC-flavor sink trait that the production
//! wiring composes:
//!
//! - For S-04 in-memory + property test path: [`InMemoryAuditSink`]
//!   captures every emitted [`AcAuditRecord`] in a `Vec` for
//!   per-test inspection.
//! - For WI-S04-006 production wiring: a real `OutboxAuditSink` writes
//!   the canonical `corelink.ac.*` envelope into the same `audit_outbox`
//!   D1 batch as the `ac_meta` upsert (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//!
//! The sink consumes the typed [`AcAuditRecord`] structurally (no JSON
//! parsing) so a future WI that adds new fields / event types is a
//! compile-time ripple, not a silent envelope drift.

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
use core::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use corelink_hash::Digest;
use thiserror::Error;
use uuid::Uuid;

use super::types::{ActionDigest, ResultHash};
use crate::Region;

/// Canonical 11-variant AC event-type taxonomy per WI-S04-001 brief +
/// WI-S04-005 §6.1.5 step 6.
///
/// The 5 "core" variants enumerated in the WI brief
/// (`ac.get.ok` / `ac.get.miss` / `ac.update.ok` /
/// `ac.update.merkle_invalid` / `ac.update.outputs_missing`) plus 3
/// canonical sub-variants the Gherkin scenarios in WI §8 require
/// (`ac.get.sig_invalid` / `ac.update.result_mismatch` /
/// `ac.update.sig_invalid`) plus the 3 TTL eviction event types
/// landed in WI-S04-005 (`ac.evict.ttl_expired` /
/// `ac.evict.r2_failed` / `ac.evict.d1_failed`). Marked
/// `#[non_exhaustive]` so additive growth (e.g., `ac.get.expired`,
/// `ac.get.backend_unavailable`) lands without breaking downstream
/// chain consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AcEventType {
    /// `corelink.ac.get.ok` — successful `GetActionResult`.
    GetOk,
    /// `corelink.ac.get.miss` — `GetActionResult` 404 (action not
    /// found OR cross-tenant masked).
    GetMiss,
    /// `corelink.ac.get.sig_invalid` — sig verify failed mid-flight
    /// (CRITICAL severity; tampering signal).
    GetSigInvalid,
    /// `corelink.ac.update.ok` — successful `UpdateActionResult`
    /// (Inserted OR IdempotentRefresh).
    UpdateOk,
    /// `corelink.ac.update.merkle_invalid` — Merkle verify failed
    /// pre-persist.
    UpdateMerkleInvalid,
    /// `corelink.ac.update.outputs_missing` — `INV-AC-OUTPUTS-VALID`
    /// rejected pre-persist.
    UpdateOutputsMissing,
    /// `corelink.ac.update.result_mismatch` — same `(tenant_id,
    /// action_digest)` upsert with mismatched `result_hash`.
    UpdateResultMismatch,
    /// `corelink.ac.update.sig_invalid` — sig sign / verify failed
    /// mid-flight (CRITICAL severity).
    UpdateSigInvalid,
    /// `corelink.ac.evict.ttl_expired` — TTL cron worker successfully
    /// evicted an `ac_meta` row whose `expires_at < now_ms`. Emitted
    /// per row by the WI-S04-005 cron tick.
    EvictTtlExpired,
    /// `corelink.ac.evict.r2_failed` — TTL cron worker R2 envelope
    /// DELETE failed; D1 row preserved; next cron tick retries.
    EvictR2Failed,
    /// `corelink.ac.evict.d1_failed` — TTL cron worker D1 row DELETE
    /// failed post-R2-success; orphan-R2 window opens; next cron tick
    /// retries idempotently.
    EvictD1Failed,
}

impl AcEventType {
    /// Canonical CloudEvents `type` attribute string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GetOk => "corelink.ac.get.ok",
            Self::GetMiss => "corelink.ac.get.miss",
            Self::GetSigInvalid => "corelink.ac.get.sig_invalid",
            Self::UpdateOk => "corelink.ac.update.ok",
            Self::UpdateMerkleInvalid => "corelink.ac.update.merkle_invalid",
            Self::UpdateOutputsMissing => "corelink.ac.update.outputs_missing",
            Self::UpdateResultMismatch => "corelink.ac.update.result_mismatch",
            Self::UpdateSigInvalid => "corelink.ac.update.sig_invalid",
            Self::EvictTtlExpired => "corelink.ac.evict.ttl_expired",
            Self::EvictR2Failed => "corelink.ac.evict.r2_failed",
            Self::EvictD1Failed => "corelink.ac.evict.d1_failed",
        }
    }

    /// Whether this variant maps to a SEV-1 alert (tampering /
    /// integrity violation). The production multiplex emitter
    /// (WI-S04-006) fans these out to direct SIEM webhook in addition
    /// to the outbox.
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(self, Self::GetSigInvalid | Self::UpdateSigInvalid)
    }
}

impl fmt::Display for AcEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Typed AC audit record. Production wiring serializes via a
/// CloudEvents 1.0 envelope analogous to `corelink-reapi::audit::AuditEnvelope`;
/// the trait surface accepts the typed shape so the sink + the
/// envelope serializer share an unambiguous contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcAuditRecord {
    /// Canonical event type.
    pub event_type: AcEventType,
    /// Verified tenant id.
    pub tenant_id: Uuid,
    /// Region the request was issued under.
    pub region: Region,
    /// Action proto digest the request targeted.
    pub action_digest: ActionDigest,
    /// `Some(...)` on UPDATE flow paths (carries the result hash).
    /// `None` on GET miss / GET sig_invalid.
    pub result_hash: Option<ResultHash>,
    /// Correlation id from the gRPC `x-request-id` header (canonical
    /// REAPI propagation).
    pub request_id: String,
    /// Per-WI extra context — short canonical reason code (e.g.,
    /// `"depth_exceeded"` from [`super::merkle::MerkleError::audit_code`]).
    /// Empty when no extra context applies.
    pub reason: &'static str,
    /// `Some(missing_digest)` for outputs-missing audits. Empty
    /// otherwise.
    pub missing_outputs: Vec<Digest>,
    /// Producer-side wall-clock instant (Unix epoch ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`AuditSink::emit`].
#[derive(Debug, Error)]
pub enum AuditSinkError {
    /// Backend transport failure (D1 batch failure / SIEM webhook
    /// timeout). Maps to 503-class on the handler.
    #[error("audit sink store error: {0}")]
    Store(String),
}

/// Audit-sink trait. Production wiring composes:
///
/// - `OutboxAuditSink` — D1 INSERT into `audit_outbox` in the same
///   batch as the `ac_meta` upsert (WI-S04-006).
/// - `MultiplexAuditSink` — fan-outs SEV-1 to direct SIEM in addition
///   to the outbox.
///
/// Production wiring is sync (D1 batch is itself sync at the trait
/// surface), so this trait is sync to match the
/// `corelink-audit::Emitter` shape.
pub trait AuditSink: Send + Sync {
    /// Persist `record` durably. The handler maps a non-`Ok` return to
    /// 503 `COR_SERVICE_DEGRADED` so the audit gap doesn't leak to
    /// the client as a 200 (per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns [`AuditSinkError::Store`] on any backend failure.
    fn emit(&self, record: AcAuditRecord) -> Result<(), AuditSinkError>;
}

/// Test-only audit sink that captures every emitted record in an
/// in-memory `Vec`. Cloning shares the underlying buffer
/// (`Arc<Mutex<Vec<…>>>`).
#[derive(Clone, Default)]
pub struct InMemoryAuditSink {
    inner: Arc<Mutex<Vec<AcAuditRecord>>>,
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
    pub fn snapshot(&self) -> Vec<AcAuditRecord> {
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
    pub fn snapshot_of(&self, event_type: AcEventType) -> Vec<AcAuditRecord> {
        self.snapshot()
            .into_iter()
            .filter(|r| r.event_type == event_type)
            .collect()
    }

    fn lock(&self) -> Result<MutexGuard<'_, Vec<AcAuditRecord>>, AuditSinkError> {
        self.inner
            .lock()
            .map_err(|_| AuditSinkError::Store("audit sink mutex poisoned".to_string()))
    }
}

impl AuditSink for InMemoryAuditSink {
    fn emit(&self, record: AcAuditRecord) -> Result<(), AuditSinkError> {
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

    fn rec(t: AcEventType) -> AcAuditRecord {
        AcAuditRecord {
            event_type: t,
            tenant_id: Uuid::nil(),
            region: Region::Wnam,
            action_digest: ActionDigest::new(Digest::compute(b"a"), 1),
            result_hash: None,
            request_id: "req-1".to_string(),
            reason: "",
            missing_outputs: Vec::new(),
            now_ms: 1,
        }
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = vec![
            AcEventType::GetOk,
            AcEventType::GetMiss,
            AcEventType::GetSigInvalid,
            AcEventType::UpdateOk,
            AcEventType::UpdateMerkleInvalid,
            AcEventType::UpdateOutputsMissing,
            AcEventType::UpdateResultMismatch,
            AcEventType::UpdateSigInvalid,
            AcEventType::EvictTtlExpired,
            AcEventType::EvictR2Failed,
            AcEventType::EvictD1Failed,
        ];
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(t.as_str().starts_with("corelink.ac."));
            assert!(set.insert(t.as_str()), "duplicate canonical: {}", t);
        }
        assert_eq!(set.len(), 11);
    }

    #[test]
    fn sev1_set_is_sig_invalid_only() {
        assert!(AcEventType::GetSigInvalid.is_sev1());
        assert!(AcEventType::UpdateSigInvalid.is_sev1());
        for t in [
            AcEventType::GetOk,
            AcEventType::GetMiss,
            AcEventType::UpdateOk,
            AcEventType::UpdateMerkleInvalid,
            AcEventType::UpdateOutputsMissing,
            AcEventType::UpdateResultMismatch,
            AcEventType::EvictTtlExpired,
            AcEventType::EvictR2Failed,
            AcEventType::EvictD1Failed,
        ] {
            assert!(!t.is_sev1(), "{} must not be SEV-1", t);
        }
    }

    #[test]
    fn in_memory_sink_captures_emitted_records() {
        let sink = InMemoryAuditSink::new();
        sink.emit(rec(AcEventType::GetOk)).unwrap();
        sink.emit(rec(AcEventType::GetMiss)).unwrap();
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.snapshot_of(AcEventType::GetOk).len(), 1);
        assert_eq!(sink.snapshot_of(AcEventType::GetMiss).len(), 1);
    }

    #[test]
    fn clone_shares_buffer() {
        let a = InMemoryAuditSink::new();
        let b = a.clone();
        a.emit(rec(AcEventType::GetOk)).unwrap();
        assert_eq!(b.len(), 1);
    }
}
