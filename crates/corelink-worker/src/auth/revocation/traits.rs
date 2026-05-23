//! Trait abstractions for the revocation orchestrator (DO storage /
//! Neon writer / KV invalidator / Queue) + the `KvSessionCacheInvalidator`
//! adapter that wraps a generic [`KvBackend`] as a dyn-compatible
//! [`SessionCacheInvalidator`].
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1):
//! this file owns the trait surface (no orchestrator, no in-memory
//! fakes, no type defs).

use core::fmt;
use std::time::SystemTime;

use async_trait::async_trait;

use corelink_pat::{PatId, PrincipalId as PatPrincipalId, TenantId as PatTenantId};

use crate::cache::kv::{KvBackend, KvError};
use crate::region::Region;

use super::types::{
    MassRevokeRow, MetaMassRevokeOutcome, MetaRevokeOutcome, PropagationOutcome, PropagationStatus,
    RevocationError, RevocationReason, RevokedEntry, SessionCacheKey,
};

// ---------------------------------------------------------------------------
// Trait abstractions (DO storage / Neon writer / KV invalidator / Queue)
// ---------------------------------------------------------------------------

/// Persistent revocation list — Durable Object equivalent.
///
/// The trait surface is deliberately small: the orchestrator only
/// needs idempotent upsert + range query + propagation-status
/// tracking. The production CF Durable Object adapter (deferred to
/// the wrangler-binding shim WI; see WI-S03-004 §6.2 trait abstraction
/// defer pattern) wires `worker::DurableObjectStorage` into these
/// methods.
#[async_trait]
pub trait RevocationStore: Send + Sync {
    /// Idempotent upsert. The implementation MUST treat
    /// `(entry.pat_id, entry.revoked_at)` as the canonical
    /// dedup key — two calls with matching keys are no-ops.
    ///
    /// Returns `true` when the upsert was a fresh insert; `false`
    /// when the entry was already present (idempotent replay).
    async fn upsert(&self, entry: RevokedEntry) -> Result<bool, RevocationError>;

    /// Lookup by PAT primary key. Returns `None` when the entry is
    /// absent. Used by the admin / audit query plane (S-13 forward);
    /// the verify hot path consults Neon `pat.revoked_at` instead
    /// (INV-AUTH-NEON-IS-SOT).
    async fn get(&self, pat_id: PatId) -> Result<Option<RevokedEntry>, RevocationError>;

    /// Record that `peer` has acknowledged the entry. Idempotent.
    async fn record_propagation_ack(
        &self,
        pat_id: PatId,
        peer: Region,
    ) -> Result<(), RevocationError>;

    /// Snapshot the propagation tracker for `pat_id`. Returns
    /// `None` if the entry is absent.
    async fn propagation_status(
        &self,
        pat_id: PatId,
    ) -> Result<Option<PropagationStatus>, RevocationError>;

    /// Range-scan the store for the reconciliation Cron.
    /// `tenant_filter` of `None` returns all entries.
    async fn list(
        &self,
        tenant_filter: Option<PatTenantId>,
    ) -> Result<Vec<RevokedEntry>, RevocationError>;
}

/// Neon transactional writer — the canonical SoT for revocation
/// state (INV-AUTH-NEON-IS-SOT).
///
/// Implementations MUST run the `pat` UPDATE and the `audit_outbox`
/// INSERT in the **same** Postgres transaction (the WI-S01-005
/// atomic batch pattern). The mass-revoke flow is two-phase: Phase 1
/// is a single atomic UPDATE; Phase 2 is the orchestrator-driven
/// chunked outbox INSERT (this trait exposes the per-chunk hook
/// [`Self::insert_outbox_batch`] for symmetry with the queue
/// producer).
#[async_trait]
pub trait MetaRevocationSink: Send + Sync {
    /// Single-row revoke. Implementation contract:
    ///
    /// ```sql
    /// BEGIN;
    /// UPDATE pat SET revoked_at = now(), reason = $reason
    ///   WHERE id = $pat_id AND revoked_at IS NULL
    ///   RETURNING revoked_at;
    /// -- if RETURNING was empty (already revoked):
    /// SELECT revoked_at FROM pat WHERE id = $pat_id;
    /// INSERT INTO audit_outbox (..., event_type = 'auth.token.revoked', ...);
    /// COMMIT;
    /// ```
    ///
    /// Returns [`MetaRevokeOutcome::Revoked`] on the fresh-revoke
    /// path (audit row inserted) or [`MetaRevokeOutcome::AlreadyRevoked`]
    /// on the idempotent replay path (audit row NOT inserted; the
    /// original event is the canonical record).
    async fn revoke(
        &self,
        pat_id: PatId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaRevokeOutcome, RevocationError>;

    /// Mass-revoke Phase 1 — atomic tenant-scoped UPDATE. Returns
    /// the list of rows that flipped from active to revoked plus the
    /// canonical `revoked_at` timestamp.
    async fn mass_revoke(
        &self,
        tenant_id: PatTenantId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaMassRevokeOutcome, RevocationError>;

    /// Mass-revoke Phase 2 chunk hook — inserts up to
    /// [`super::types::MASS_REVOKE_OUTBOX_CHUNK_SIZE`] audit_outbox rows in a single
    /// batch. Implementations SHOULD use the same INSERT batching
    /// path as WI-S01-005.
    async fn insert_outbox_batch(
        &self,
        tenant_id: PatTenantId,
        mass_revoke_id: super::types::MassRevokeId,
        revoked_at: SystemTime,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
        chunk: &[MassRevokeRow],
    ) -> Result<(), RevocationError>;
}

/// KV-namespace adapter that drops a session-cache entry. The
/// orchestrator treats KV outages as soft-degraded (the cold path
/// recovers via Neon).
///
/// The trait is dyn-compatible (`#[async_trait]`) so the
/// orchestrator can hold an `Arc<dyn SessionCacheInvalidator>`. The
/// canonical wrapper [`KvSessionCacheInvalidator`] adapts any
/// [`KvBackend`] (which is NOT dyn-compatible by design — it uses
/// RPITIT for zero-cost futures) into this trait.
#[async_trait]
pub trait SessionCacheInvalidator: Send + Sync {
    /// Drop the canonical session-cache entry. Idempotent on missing
    /// keys.
    async fn invalidate(&self, key: &SessionCacheKey) -> Result<(), KvError>;
}

/// Adapter wrapping a generic [`KvBackend`] as a dyn-compatible
/// [`SessionCacheInvalidator`]. The host-server / property-test
/// crates instantiate this with the concrete backend (in-memory
/// fake or production CF binding) and hand it to the orchestrator
/// as `Arc<dyn SessionCacheInvalidator>`.
pub struct KvSessionCacheInvalidator<K: KvBackend + Send + Sync + 'static> {
    kv: K,
}

impl<K: KvBackend + Send + Sync + 'static> KvSessionCacheInvalidator<K> {
    /// Wrap a [`KvBackend`].
    #[must_use]
    pub const fn new(kv: K) -> Self {
        Self { kv }
    }

    /// Borrow the inner backend.
    #[must_use]
    pub const fn inner(&self) -> &K {
        &self.kv
    }
}

impl<K: KvBackend + Send + Sync + fmt::Debug + 'static> fmt::Debug
    for KvSessionCacheInvalidator<K>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KvSessionCacheInvalidator")
            .field("kv", &self.kv)
            .finish()
    }
}

#[async_trait]
impl<K: KvBackend + Send + Sync + 'static> SessionCacheInvalidator
    for KvSessionCacheInvalidator<K>
{
    async fn invalidate(&self, key: &SessionCacheKey) -> Result<(), KvError> {
        self.kv.delete(key.as_str()).await
    }
}

/// Cross-region broadcast producer (CF Queue equivalent). Returns a
/// [`PropagationOutcome::Enqueued`] when the message was accepted,
/// or [`PropagationOutcome::DlqFallback`] when the producer was
/// unavailable and the orchestrator MUST persist to the DLQ.
#[async_trait]
pub trait RevocationBroadcast: Send + Sync {
    /// Enqueue a single-revoke broadcast. The implementation
    /// derives the peer-region set internally (the orchestrator's
    /// `peers` config is opaque from the producer's POV).
    async fn enqueue_single(&self, entry: &RevokedEntry) -> PropagationOutcome;

    /// Enqueue a batched mass-revoke broadcast. Implementations
    /// MUST chunk into ≤ [`super::types::MASS_REVOKE_BROADCAST_BATCH_SIZE`]-entry
    /// messages internally; the orchestrator passes the full slice.
    async fn enqueue_mass(&self, entries: &[RevokedEntry]) -> PropagationOutcome;
}
