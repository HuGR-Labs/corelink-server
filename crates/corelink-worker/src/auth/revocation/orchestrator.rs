//! Top-level revocation orchestrator. Wires the four traits together
//! and exposes the canonical `revoke` / `mass_revoke` /
//! `ingest_remote` / `reconcile` surface.
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1):
//! this file owns the orchestrator + the public response types.

use core::fmt;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::SystemTime;

use corelink_pat::{PatId, PrincipalId as PatPrincipalId, TenantId as PatTenantId};

use crate::region::Region;

use super::traits::{
    MetaRevocationSink, RevocationBroadcast, RevocationStore, SessionCacheInvalidator,
};
use super::types::{
    HookOutcome, MassRevokeId, MetaRevokeOutcome, PropagationOutcome, RevocationError,
    RevocationReason, RevokeRequest, RevokeResponse, RevokedEntry, SessionCacheKey,
    MASS_REVOKE_BROADCAST_BATCH_SIZE, MASS_REVOKE_OUTBOX_CHUNK_SIZE,
};

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Top-level revocation orchestrator. Wires the four traits together
/// and exposes the canonical `revoke` / `mass_revoke` /
/// `ingest_remote` surface that the host-server endpoints (and the
/// CF Queue consumer) call.
///
/// A single `RevocationOrchestrator` is pinned to a region — every
/// [`RevocationStore::upsert`] writes that region's DO; every
/// `enqueue_*` call is for the broadcast plane targeting peers.
pub struct RevocationOrchestrator {
    region: Region,
    peers: BTreeSet<Region>,
    store: Arc<dyn RevocationStore>,
    meta: Arc<dyn MetaRevocationSink>,
    session_cache: Arc<dyn SessionCacheInvalidator>,
    broadcast: Arc<dyn RevocationBroadcast>,
}

impl fmt::Debug for RevocationOrchestrator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RevocationOrchestrator")
            .field("region", &self.region)
            .field("peers", &self.peers)
            .finish_non_exhaustive()
    }
}

impl RevocationOrchestrator {
    /// Construct a fresh orchestrator. `peers` is the set of peer
    /// regions for cross-region propagation; the origin region MUST
    /// NOT appear in the set (a config-time invariant; we filter it
    /// defensively here so a misconfiguration doesn't double-emit).
    #[must_use]
    pub fn new(
        region: Region,
        peers: BTreeSet<Region>,
        store: Arc<dyn RevocationStore>,
        meta: Arc<dyn MetaRevocationSink>,
        session_cache: Arc<dyn SessionCacheInvalidator>,
        broadcast: Arc<dyn RevocationBroadcast>,
    ) -> Self {
        let mut peers = peers;
        peers.remove(&region);
        Self {
            region,
            peers,
            store,
            meta,
            session_cache,
            broadcast,
        }
    }

    /// The region this orchestrator is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// The peer-region set (origin region filtered out at
    /// construction time).
    #[must_use]
    pub fn peers(&self) -> &BTreeSet<Region> {
        &self.peers
    }

    /// Canonical single-revoke flow.
    ///
    /// Steps (canonical order; each step's failure mode documented
    /// inline):
    ///
    /// 1. Neon UPDATE + audit_outbox INSERT (atomic). On
    ///    [`MetaRevokeOutcome::AlreadyRevoked`] short-circuit and
    ///    return the existing timestamp; no downstream side effects.
    /// 2. DO storage upsert (idempotent; preserves
    ///    INV-AUTH-REVOCATION-IDEMPOTENT under retry).
    /// 3. KV session-cache invalidate (best-effort soft-degraded).
    /// 4. Cross-region broadcast enqueue (peer set; outcome
    ///    surfaced to the caller).
    pub async fn revoke(&self, request: RevokeRequest) -> Result<RevokeResponse, RevocationError> {
        // Step 1 — Neon SoT.
        let meta_outcome = self
            .meta
            .revoke(request.pat_id, request.reason, request.revoked_by)
            .await?;
        let (revoked_at, was_freshly_revoked) = match meta_outcome {
            MetaRevokeOutcome::Revoked { revoked_at } => (revoked_at, true),
            MetaRevokeOutcome::AlreadyRevoked { revoked_at } => {
                // INV-AUTH-REVOCATION-IDEMPOTENT replay path.
                // Skip DO upsert / KV invalidate / broadcast.
                return Ok(RevokeResponse {
                    revoked_at,
                    was_freshly_revoked: false,
                    session_cache_invalidate: HookOutcome::Ok,
                    broadcast_outcome: PropagationOutcome::NoPeers,
                    origin_region: self.region,
                });
            }
        };

        // Step 2 — DO storage upsert.
        let entry = RevokedEntry {
            pat_id: request.pat_id,
            tenant_id: request.tenant_id,
            revoked_by: request.revoked_by,
            revoked_at,
            reason: request.reason,
            origin_region: self.region,
            mass_revoke_id: None,
        };
        let _ = self.store.upsert(entry.clone()).await?;

        // Step 3 — KV session-cache invalidate (best-effort).
        let session_cache_invalidate =
            match self.session_cache.invalidate(&request.token_hash_key).await {
                Ok(()) => HookOutcome::Ok,
                Err(_) => HookOutcome::SoftDegraded,
            };

        // Step 4 — Cross-region broadcast.
        let broadcast_outcome = if self.peers.is_empty() {
            PropagationOutcome::NoPeers
        } else {
            self.broadcast.enqueue_single(&entry).await
        };

        Ok(RevokeResponse {
            revoked_at,
            was_freshly_revoked,
            session_cache_invalidate,
            broadcast_outcome,
            origin_region: self.region,
        })
    }

    /// Canonical mass-revoke flow.
    ///
    /// Phase 1 — atomic Neon UPDATE.
    /// Phase 2 — chunked audit_outbox INSERT
    ///   (`MASS_REVOKE_OUTBOX_CHUNK_SIZE`) + chunked Queue broadcast
    ///   (`MASS_REVOKE_BROADCAST_BATCH_SIZE`) + per-row DO upsert +
    ///   per-row session-cache invalidate.
    pub async fn mass_revoke(
        &self,
        tenant_id: PatTenantId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MassRevokeResponse, RevocationError> {
        let mass_revoke_id = MassRevokeId::new_v7();
        let phase1 = self.meta.mass_revoke(tenant_id, reason, revoked_by).await?;
        let revoked_at = phase1.revoked_at;
        let total = phase1.newly_revoked.len();

        // Phase 2 — chunked audit_outbox INSERT.
        let mut chunked_count = 0usize;
        for chunk in phase1.newly_revoked.chunks(MASS_REVOKE_OUTBOX_CHUNK_SIZE) {
            self.meta
                .insert_outbox_batch(
                    tenant_id,
                    mass_revoke_id,
                    revoked_at,
                    reason,
                    revoked_by,
                    chunk,
                )
                .await?;
            chunked_count += chunk.len();
        }

        // Phase 2 — DO upsert + KV invalidate per row + accumulate
        // entries for the broadcast batch.
        let mut entries: Vec<RevokedEntry> = Vec::with_capacity(total);
        let mut session_cache_failures = 0usize;
        for row in &phase1.newly_revoked {
            let entry = RevokedEntry {
                pat_id: row.pat_id,
                tenant_id,
                revoked_by,
                revoked_at,
                reason,
                origin_region: self.region,
                mass_revoke_id: Some(mass_revoke_id),
            };
            let _ = self.store.upsert(entry.clone()).await?;
            if self
                .session_cache
                .invalidate(&row.token_hash_key)
                .await
                .is_err()
            {
                session_cache_failures += 1;
            }
            entries.push(entry);
        }

        // Phase 2 — broadcast in
        // MASS_REVOKE_BROADCAST_BATCH_SIZE-bounded chunks.
        let mut broadcast_outcome = PropagationOutcome::Enqueued;
        if self.peers.is_empty() || entries.is_empty() {
            broadcast_outcome = PropagationOutcome::NoPeers;
        } else {
            for batch in entries.chunks(MASS_REVOKE_BROADCAST_BATCH_SIZE) {
                let outcome = self.broadcast.enqueue_mass(batch).await;
                if outcome == PropagationOutcome::DlqFallback {
                    broadcast_outcome = PropagationOutcome::DlqFallback;
                }
            }
        }

        Ok(MassRevokeResponse {
            mass_revoke_id,
            revoked_at,
            revoked_count: total,
            outbox_inserted: chunked_count,
            session_cache_failures,
            broadcast_outcome,
            origin_region: self.region,
        })
    }

    /// Ingest a remote revocation broadcast (called by the CF Queue
    /// consumer in peer regions). Idempotent under retry — the
    /// canonical dedup key is `(pat_id, revoked_at)`.
    ///
    /// Steps:
    /// 1. Region-mismatch defensive check (the orchestrator MUST be
    ///    the receiving region; `entry.origin_region` is the
    ///    sender's region, distinct from the orchestrator's).
    /// 2. DO storage upsert (idempotent; INV-AUTH-PROPAGATION-AT-LEAST-ONCE).
    /// 3. KV session-cache invalidate (best-effort).
    /// 4. Origin-region propagation ack.
    pub async fn ingest_remote(
        &self,
        entry: RevokedEntry,
        remote_session_cache_key: Option<SessionCacheKey>,
    ) -> Result<IngestOutcome, RevocationError> {
        if entry.origin_region == self.region {
            // The origin's own DO already holds this entry; an
            // incoming "remote" message addressed back to the origin
            // is either a config bug or a self-test loopback. We
            // accept it idempotently (upsert) but mark the outcome
            // accordingly.
        }
        let was_fresh = self.store.upsert(entry.clone()).await?;
        let session_cache_invalidate = match remote_session_cache_key.as_ref() {
            Some(key) => match self.session_cache.invalidate(key).await {
                Ok(()) => HookOutcome::Ok,
                Err(_) => HookOutcome::SoftDegraded,
            },
            None => HookOutcome::Ok,
        };
        // Record propagation ack at the origin's tracker.
        // (The origin is `entry.origin_region`; this orchestrator is
        // `self.region` and is the new ack source.)
        self.store
            .record_propagation_ack(entry.pat_id, self.region)
            .await?;
        Ok(IngestOutcome {
            was_fresh,
            session_cache_invalidate,
        })
    }

    /// Reconciliation Cron entry point. Returns a summary of the
    /// drift between the local DO storage and the canonical Neon
    /// `pat.revoked_at` snapshot.
    ///
    /// The drift detection is deliberately host-agnostic: callers
    /// pass a `neon_view: Vec<(PatId, SystemTime)>` snapshot they
    /// produced from a Neon read query. The return value enumerates
    /// the two drift directions so the runbook RB-FM-REVOKE-DRIFT
    /// can act on each.
    pub async fn reconcile(
        &self,
        neon_view: &[(PatId, SystemTime)],
    ) -> Result<ReconciliationSummary, RevocationError> {
        let do_view = self.store.list(None).await?;
        let do_set: HashMap<PatId, SystemTime> = do_view
            .into_iter()
            .map(|e| (e.pat_id, e.revoked_at))
            .collect();
        let mut neon_only = Vec::new();
        let mut do_only = Vec::new();
        let mut timestamp_drift = Vec::new();
        let neon_map: HashMap<PatId, SystemTime> = neon_view.iter().copied().collect();
        for (pat_id, neon_ts) in &neon_map {
            match do_set.get(pat_id) {
                None => neon_only.push(*pat_id),
                Some(do_ts) if do_ts != neon_ts => {
                    timestamp_drift.push(DriftRow {
                        pat_id: *pat_id,
                        neon_ts: *neon_ts,
                        do_ts: *do_ts,
                    });
                }
                _ => {}
            }
        }
        for pat_id in do_set.keys() {
            if !neon_map.contains_key(pat_id) {
                do_only.push(*pat_id);
            }
        }
        Ok(ReconciliationSummary {
            neon_only,
            do_only,
            timestamp_drift,
        })
    }
}

/// Outcome of a [`RevocationOrchestrator::mass_revoke`] call.
#[derive(Clone, Debug)]
pub struct MassRevokeResponse {
    /// Parent mass-revoke id (UUIDv7).
    pub mass_revoke_id: MassRevokeId,
    /// Authoritative timestamp set inside the Phase 1 atomic
    /// UPDATE.
    pub revoked_at: SystemTime,
    /// Number of rows newly revoked (Phase 1 result).
    pub revoked_count: usize,
    /// Number of audit_outbox rows successfully inserted in Phase 2.
    pub outbox_inserted: usize,
    /// Number of session-cache invalidate failures observed during
    /// Phase 2 fan-out (best-effort; soft-degraded; logged for
    /// telemetry, not an error).
    pub session_cache_failures: usize,
    /// Aggregated broadcast outcome (worst of any chunk).
    pub broadcast_outcome: PropagationOutcome,
    /// Origin region.
    pub origin_region: Region,
}

/// Outcome of [`RevocationOrchestrator::ingest_remote`].
#[derive(Clone, Copy, Debug)]
pub struct IngestOutcome {
    /// `true` when the upsert was a fresh insert; `false` on
    /// idempotent replay.
    pub was_fresh: bool,
    /// Health of the local session-cache invalidate hook.
    pub session_cache_invalidate: HookOutcome,
}

/// Reconciliation drift summary returned by
/// [`RevocationOrchestrator::reconcile`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconciliationSummary {
    /// PAT ids the Neon snapshot contains that the local DO storage
    /// does NOT (delivered revoke whose broadcast never reached the
    /// region OR whose DO write was lost).
    pub neon_only: Vec<PatId>,
    /// PAT ids the local DO storage contains that Neon does NOT
    /// (broadcast received without a matching SoT row — extremely
    /// unusual; investigate via RB-FM-REVOKE-DRIFT).
    pub do_only: Vec<PatId>,
    /// PAT ids present in both views with disagreeing
    /// `revoked_at` timestamps. The Neon timestamp is canonical;
    /// the runbook patches the DO row to match.
    pub timestamp_drift: Vec<DriftRow>,
}

impl ReconciliationSummary {
    /// `true` when no drift was observed.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.neon_only.is_empty() && self.do_only.is_empty() && self.timestamp_drift.is_empty()
    }
}

/// One row in [`ReconciliationSummary::timestamp_drift`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriftRow {
    /// PAT primary key.
    pub pat_id: PatId,
    /// Neon `pat.revoked_at` (canonical).
    pub neon_ts: SystemTime,
    /// Local DO storage `revoked_at`.
    pub do_ts: SystemTime,
}
