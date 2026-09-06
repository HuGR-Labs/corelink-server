//! Physical delete phase (WI-S06-004) — post-grace R2 idempotent
//! DeleteObject + D1 row purge under a conditional `refcount = 0`
//! predicate.
//!
//! ## Where physical-delete sits in the GC phase chain
//!
//! `Idle → Mark → Sweep → **PhysicalDelete** → Reconcile → Completed`
//!
//! - Mark phase (WI-S06-002) populates `gc_candidates` (`status =
//!   'candidate'`) and atomically captures `gc_run.mark_started_at_ms`.
//! - Sweep phase (WI-S06-003) soft-deletes orphan rows (`blob_meta
//!   .deleted_at_ms = now`), transitions candidates to `Swept`, emits
//!   `corelink.gc.sweep.soft_deleted`.
//! - **Physical-delete phase consumes `Swept` candidates whose
//!   `blob_meta.deleted_at_ms` is older than the canonical grace window
//!   (`now - deleted_at_ms > grace_period_ms`)**, calls R2 DeleteObject
//!   (idempotent — re-call on already-deleted blob is `Ok`), purges the
//!   D1 row guarded by a *race-protection* `refcount = 0` predicate
//!   (Lote 10.6bis P0-4), transitions the candidate to
//!   `PhysicallyDeleted`, and emits `corelink.gc.physical_deleted` per
//!   purged blob.
//!
//! ## Cripto-driven invariants enforced (per WI §1)
//!
//! 1. **Grace period boundary STRICT enforcement** (WI §6.1.3):
//!    `now - blob_meta.deleted_at_ms > grace_period_ms` (strict `>` —
//!    delete fires only AFTER the full grace window expires; equality
//!    means `1 ms` short of grace and MUST be skipped). Off-by-one (`>=`
//!    instead of `>`) is a data-loss bug; pinned by
//!    `prop_post_grace_gate`.
//! 2. **R2 DeleteObject idempotent** (PAT-RETRY-IDEMPOTENT-001):
//!    re-calling the trait on an already-deleted key returns
//!    [`R2DeleteOutcome::NotFound`] and the orchestrator treats it as a
//!    successful delete; the next D1 purge proceeds normally
//!    (idempotent re-run safe).
//! 3. **Eventually-consistent R2→D1 ordering with idempotent
//!    crash-recovery** (Lote 10.6bis P0-2 framing fix; NOT 2PC
//!    ACID-atomic): R2 DeleteObject FIRST → on success, D1 row purge
//!    under conditional `WHERE refcount = 0` predicate. R2 fail →
//!    preserve D1; next hourly tick retries idempotently. R2 success +
//!    D1 fail → R2 deleted, D1 row remains (recovered by reconcile
//!    WI-S06-005 within 24h).
//! 4. **Conditional `refcount = 0` predicate** (Lote 10.6bis P0-4 race
//!    protection): immediately before the D1 row removal, the
//!    orchestrator re-reads the row's refcount via
//!    [`BlobMetaPurgeStore::lookup_purge_state`]; if a customer CAS
//!    write has incremented the refcount in the race window between the
//!    sweep tick and the physical-delete tick, the purge is skipped
//!    (the row is preserved; the candidate is observed as
//!    `AlreadyResolved` on a subsequent tick because the next
//!    reconcile-mark cycle re-evaluates the refcount). This converts an
//!    irreversible data-loss race into a recoverable benign no-op.
//! 5. **Tenant-scoped strict** (Lote 10.4bis lesson): every API takes
//!    `(tenant_id, region)` first; cross-tenant injection surfaces as a
//!    fail-closed [`PhysicalDeleteError::Backend`].
//! 6. **Audit emit fail-closed** (WI §6.1.7 envelope; same pattern as
//!    sweep): `corelink.gc.physical_deleted` is emitted BEFORE the
//!    candidate row's status flip; emission failure aborts the
//!    per-candidate step and surfaces
//!    [`PhysicalDeleteError::AuditEmissionFailed`].
//!
//! ## Trait-abstraction-defer pattern
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this module ships **only the
//! pure-logic skeleton + in-memory fakes** so property tests at 10 k
//! iter exercise every load-bearing invariant (idempotent retry,
//! refcount=0 race protection, post-grace gate, tenant isolation)
//! without spinning up miniflare. The real Cloudflare R2 DeleteObject
//! binding + the real D1 conditional-DELETE batch + the production
//! Cron Durable Object alarm wiring + the DSR Ed25519 bypass signal
//! handler land in WI-S06-007 (PRR ship gate) alongside the conformance
//! suite.
//!
//! ## Phase budget
//!
//! Sprint contract §5.4 R-S06-9.1 pins the canonical phase budget to
//! `≤30 min p99 @ 100 k candidates` per (tenant, region) hourly tick.
//! The skeleton enforces the ceiling via a per-candidate budget probe
//! against a deterministic clock (mirrors the sweep phase pattern); the
//! production wiring layers on bounded concurrency 8 + D1 batch ≤250
//! rows per Lote 10.6bis P0-5 derivation.

use std::sync::Arc;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::audit::{GcAuditRecord, GcAuditSink, GcAuditSinkError, GcEventType};
use crate::error::GcError;
use crate::mark::{BlobDigest, CandidateStatus, GcCandidate, GcCandidatesStore, MarkError};
use crate::metrics::{GcMetricsObserver, GcMetricsObserverError};
use crate::region::GcRegion;
use crate::run::{CheckpointDeltas, GcPhase, GcRunStore, GcRunStoreError, GcStatus, RunId};
use crate::sweep::{GRACE_AC_MS, GRACE_CAS_MS};

/// Canonical physical-delete phase budget — 30 min p99 @ 100 k
/// candidates per (tenant, region) hourly tick (sprint contract §5.4
/// R-S06-9.1; Lote 10.6bis P0-5 arithmetic re-derivation).
pub const CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS: u64 = 30 * 60 * 1000;

// ============================================================================
//  R2Delete trait + InMemory fake.
// ============================================================================

/// Outcome of a single [`R2Delete::delete`] call.
///
/// The two arms are *both* successes from the orchestrator's point of
/// view (PAT-RETRY-IDEMPOTENT-001): a fresh delete and a no-op on an
/// already-deleted key carry identical liveness for the D1 purge step.
/// The split exists so metrics + audit can record fresh-delete vs
/// already-absent counts independently (operator-visible signal:
/// sustained `NotFound` rate > expected ≈ likelihood of duplicate
/// candidate emission upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum R2DeleteOutcome {
    /// R2 DeleteObject returned 200/204 — the key existed and was
    /// removed.
    Deleted,
    /// R2 DeleteObject returned 404 (or S3-equivalent) — idempotent
    /// no-op (the key was already absent). Treated as success by the
    /// orchestrator per WI §1.2.
    NotFound,
}

/// Trait surfaced by every R2 backend (production CF R2 binding /
/// in-memory fake). The single method [`R2Delete::delete`] mirrors the
/// canonical idempotent S3 DeleteObject semantic.
///
/// # Errors
///
/// Backend transport errors (5xx / network drop) surface as
/// [`PhysicalDeleteError::R2BackendError`]. The orchestrator preserves
/// the D1 row on R2 failure (no orphan reference); the next hourly tick
/// retries idempotently.
pub trait R2Delete: Send + Sync + core::fmt::Debug {
    /// Delete a single R2 key under `(tenant_id, region)` scope.
    ///
    /// `key` is the canonical R2 object key for the digest (production:
    /// `cas/<tenant_prefix>/<digest>`). Tenant scope is asserted at the
    /// adapter boundary; cross-tenant injection surfaces as fail-closed
    /// [`R2DeleteError::Backend`].
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn delete(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
        key: &str,
    ) -> Result<R2DeleteOutcome, R2DeleteError>;
}

/// Errors surfaced by [`R2Delete`] backends.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum R2DeleteError {
    /// Backend transport failure (network / 5xx / parse). Orchestrator
    /// maps this to [`PhysicalDeleteError::R2BackendError`] and
    /// preserves the D1 row.
    #[error("r2 backend error: {0}")]
    Backend(String),
}

/// In-memory R2 delete fake. Tests push canonical R2 keys via
/// [`InMemoryR2Delete::seed`]; the [`R2Delete::delete`] method removes
/// the key from the per-`(tenant, region)` set and returns
/// [`R2DeleteOutcome::Deleted`] on first call, [`R2DeleteOutcome::NotFound`]
/// thereafter.
#[derive(Debug, Default)]
pub struct InMemoryR2Delete {
    inner: Mutex<std::collections::BTreeMap<(Uuid, GcRegion), std::collections::BTreeSet<String>>>,
}

impl InMemoryR2Delete {
    /// Construct an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a canonical R2 key under `(tenant_id, region)` so a
    /// subsequent [`R2Delete::delete`] call returns
    /// [`R2DeleteOutcome::Deleted`] (mirrors the production
    /// "object exists in R2" pre-condition).
    pub fn seed(&self, tenant_id: Uuid, region: GcRegion, key: impl Into<String>) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.entry((tenant_id, region)).or_default().insert(key.into());
    }

    /// Snapshot the present keys for a `(tenant, region)` scope (test
    /// diagnostics).
    #[must_use]
    pub fn snapshot(&self, tenant_id: Uuid, region: GcRegion) -> Vec<String> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, region))
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether a key is present under `(tenant, region)` scope.
    #[must_use]
    pub fn contains(&self, tenant_id: Uuid, region: GcRegion, key: &str) -> bool {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, region))
            .map(|s| s.contains(key))
            .unwrap_or(false)
    }
}

impl R2Delete for InMemoryR2Delete {
    fn delete(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
        key: &str,
    ) -> Result<R2DeleteOutcome, R2DeleteError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| R2DeleteError::Backend("r2 fake mutex poisoned".to_owned()))?;
        let bucket = g.entry((tenant_id, region)).or_default();
        if bucket.remove(key) {
            Ok(R2DeleteOutcome::Deleted)
        } else {
            // Idempotent: PAT-RETRY-IDEMPOTENT-001 — re-call on
            // already-deleted key returns NotFound, treated as a
            // successful delete by the orchestrator.
            Ok(R2DeleteOutcome::NotFound)
        }
    }
}

// ============================================================================
//  BlobMetaPurgeStore — post-grace lookup + conditional row purge.
// ============================================================================

/// Pre-purge state snapshot of a `blob_meta` row used by the
/// physical-delete phase to enforce (a) the post-grace gate
/// (`deleted_at_ms` known) AND (b) the conditional `refcount = 0`
/// predicate immediately before D1 row removal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgeState {
    /// Canonical R2 object key (production: `cas/<tenant_prefix>/<digest>`).
    /// The orchestrator passes this verbatim to [`R2Delete::delete`].
    pub r2_key: String,
    /// Refcount captured at lookup time. Conditional D1 DELETE fires
    /// only when `refcount == 0` re-checked at purge time (Lote
    /// 10.6bis P0-4 race protection).
    pub refcount: u32,
    /// Blob payload size (counted into `bytes_reclaimed` on a
    /// confirmed purge).
    pub size_bytes: u64,
    /// Soft-delete instant. The post-grace gate is `now -
    /// deleted_at_ms > grace_period_ms`.
    pub deleted_at_ms: u64,
}

/// Trait surfaced by every `blob_meta` purge backend (production D1
/// reader + conditional DELETE / in-memory fake).
pub trait BlobMetaPurgeStore: Send + Sync + core::fmt::Debug {
    /// Lookup the soft-deleted `blob_meta` row's purge state.
    ///
    /// Returns `Ok(None)` when the row does not exist, belongs to a
    /// different tenant (Layer 4 envelope), or is NOT soft-deleted
    /// (`deleted_at_ms IS NULL`). The orchestrator interprets `None`
    /// as `AlreadyResolved` (idempotent path).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn lookup_purge_state(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<Option<PurgeState>, PhysicalDeleteError>;

    /// Conditional row removal: purge the `blob_meta` row IFF the
    /// refcount is *still* zero AND the row is *still* soft-deleted at
    /// purge time. Mirrors:
    ///
    /// ```sql
    /// DELETE FROM blob_meta
    ///   WHERE tenant_id = ?
    ///     AND digest = ?
    ///     AND refcount = 0
    ///     AND deleted_at_ms IS NOT NULL
    ///     AND (? - deleted_at_ms) > ?  -- post-grace re-check
    /// ```
    ///
    /// Returns `true` when the DELETE fired (the row was atomically
    /// removed); `false` when the predicate was *not* satisfied at the
    /// SQL evaluation moment (a customer CAS write incremented the
    /// refcount in the race window, OR the grace gate flipped, OR the
    /// row was already absent). The `false` arm is a benign no-op —
    /// the orchestrator surfaces it as
    /// [`PhysicalDeleteDecision::SkippedRefcountNonZero`] /
    /// [`PhysicalDeleteDecision::AlreadyResolved`].
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn conditional_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<bool, PhysicalDeleteError>;
}

/// In-memory `blob_meta` purge store. Mirrors the canonical
/// `(tenant_id, digest)` PK + the conditional DELETE predicate
/// described in [`BlobMetaPurgeStore::conditional_purge`].
#[derive(Debug, Default)]
pub struct InMemoryBlobMetaPurgeStore {
    inner: Mutex<std::collections::BTreeMap<(Uuid, BlobDigest), PurgeRow>>,
}

#[derive(Clone, Debug)]
struct PurgeRow {
    r2_key: String,
    refcount: u32,
    size_bytes: u64,
    deleted_at_ms: Option<u64>,
}

impl InMemoryBlobMetaPurgeStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a soft-deleted row ready for physical-delete (the canonical
    /// fixture: `refcount = 0` AND `deleted_at_ms = Some(_)`).
    pub fn push_soft_deleted(
        &self,
        tenant_id: Uuid,
        digest: BlobDigest,
        r2_key: impl Into<String>,
        size_bytes: u64,
        deleted_at_ms: u64,
    ) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.insert(
            (tenant_id, digest),
            PurgeRow {
                r2_key: r2_key.into(),
                refcount: 0,
                size_bytes,
                deleted_at_ms: Some(deleted_at_ms),
            },
        );
    }

    /// Test mutator: simulate a customer CAS write that incremented the
    /// refcount in the race window between sweep and physical-delete.
    /// The conditional DELETE will then skip the row.
    pub fn set_refcount(&self, tenant_id: Uuid, digest: &BlobDigest, refcount: u32) -> bool {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match g.get_mut(&(tenant_id, digest.clone())) {
            Some(row) => {
                row.refcount = refcount;
                true
            }
            None => false,
        }
    }

    /// Whether a row exists under the `(tenant, digest)` key.
    #[must_use]
    pub fn contains(&self, tenant_id: Uuid, digest: &BlobDigest) -> bool {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.contains_key(&(tenant_id, digest.clone()))
    }

    /// Snapshot a row's [`PurgeState`] (diagnostic).
    #[must_use]
    pub fn snapshot(&self, tenant_id: Uuid, digest: &BlobDigest) -> Option<PurgeState> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, digest.clone())).and_then(|row| {
            row.deleted_at_ms.map(|ts| PurgeState {
                r2_key: row.r2_key.clone(),
                refcount: row.refcount,
                size_bytes: row.size_bytes,
                deleted_at_ms: ts,
            })
        })
    }
}

impl BlobMetaPurgeStore for InMemoryBlobMetaPurgeStore {
    fn lookup_purge_state(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<Option<PurgeState>, PhysicalDeleteError> {
        let g = self.inner.lock().map_err(|_| {
            PhysicalDeleteError::Backend("blob_meta purge mutex poisoned".to_owned())
        })?;
        Ok(g.get(&(tenant_id, digest.clone())).and_then(|row| {
            row.deleted_at_ms.map(|ts| PurgeState {
                r2_key: row.r2_key.clone(),
                refcount: row.refcount,
                size_bytes: row.size_bytes,
                deleted_at_ms: ts,
            })
        }))
    }

    fn conditional_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<bool, PhysicalDeleteError> {
        let mut g = self.inner.lock().map_err(|_| {
            PhysicalDeleteError::Backend("blob_meta purge mutex poisoned".to_owned())
        })?;
        let key = (tenant_id, digest.clone());
        let Some(row) = g.get(&key) else {
            return Ok(false);
        };
        // Predicate 1: row is soft-deleted.
        let Some(deleted_at_ms) = row.deleted_at_ms else {
            return Ok(false);
        };
        // Predicate 2: refcount = 0 (re-checked at purge time).
        if row.refcount != 0 {
            return Ok(false);
        }
        // Predicate 3: post-grace gate (strict `>` per WI §6.1.3).
        if now_ms.saturating_sub(deleted_at_ms) <= grace_period_ms {
            return Ok(false);
        }
        g.remove(&key);
        Ok(true)
    }
}

// ============================================================================
//  PhysicalDeleteClock seam.
// ============================================================================

/// Wall-clock seam for the physical-delete phase. Mirrors
/// [`crate::sweep::SweepClock`] / [`crate::mark::MarkClock`] so the
/// three phases share a deterministic test seam without binding
/// directly to `Date.now()`.
pub trait PhysicalDeleteClock: Send + Sync + core::fmt::Debug {
    /// Read the current wall-clock instant (Unix ms). Each call may
    /// return a value `>=` the previous call.
    fn now_ms(&self) -> u64;
}

/// Counter-driven [`PhysicalDeleteClock`] used by tests + property
/// tests.
#[derive(Debug)]
pub struct CountingPhysicalDeleteClock {
    inner: Mutex<u64>,
}

impl CountingPhysicalDeleteClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }
}

impl PhysicalDeleteClock for CountingPhysicalDeleteClock {
    fn now_ms(&self) -> u64 {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = *g;
        *g = g.saturating_add(1);
        now
    }
}

// ============================================================================
//  PhysicalDeleteDecision + Result + Error taxonomy.
// ============================================================================

/// Per-candidate decision produced by
/// [`InMemoryPhysicalDeletePhase::step_candidate`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PhysicalDeleteDecision {
    /// Confirmed purge: R2 DeleteObject succeeded (or returned 404
    /// idempotent), D1 row removed under the conditional `refcount = 0`
    /// predicate, candidate transitioned to `PhysicallyDeleted`, audit
    /// `corelink.gc.physical_deleted` emitted.
    Purged {
        /// Wall-clock instant the purge fired.
        purged_at_ms: u64,
        /// Bytes reclaimed (`PurgeState.size_bytes` from the pre-purge
        /// lookup).
        bytes_reclaimed: u64,
        /// R2 outcome — fresh delete vs idempotent NotFound.
        r2_outcome: R2DeleteOutcome,
    },
    /// Skipped: the soft-delete instant has not yet passed the grace
    /// window (`now - deleted_at_ms <= grace_period_ms`). No R2 call;
    /// no D1 mutation; candidate row preserved for the next hourly tick.
    SkippedGracePending {
        /// `now_ms` observed at the gate.
        now_ms: u64,
        /// Soft-delete instant captured from `blob_meta`.
        deleted_at_ms: u64,
        /// Effective grace window for the candidate.
        grace_period_ms: u64,
    },
    /// Skipped: the conditional `refcount = 0` predicate failed at
    /// purge time. A customer CAS write incremented the refcount in
    /// the race window between the sweep tick and the physical-delete
    /// tick (Lote 10.6bis P0-4). Row preserved; reconcile (WI-S06-005)
    /// re-evaluates on the next 24h cron cycle.
    SkippedRefcountNonZero {
        /// Refcount observed at purge time.
        refcount: u32,
    },
    /// Idempotent re-run: the candidate row is already in
    /// `PhysicallyDeleted` (or any non-`Swept`) state; no R2 call; no
    /// D1 mutation; no audit re-emit.
    AlreadyResolved {
        /// Status observed at the moment of inspection.
        observed_status: CandidateStatus,
    },
}

/// Read-only classification of a single candidate against the reclaim
/// gate, with **zero side effects** (no R2 DeleteObject, no D1 purge,
/// no candidate-status transition, no audit emit).
///
/// This is the single source of truth for "is this object reclaimable?"
/// — both the mutating [`InMemoryPhysicalDeletePhase::step_candidate`]
/// live path AND the non-destructive dry-run sweep
/// ([`crate::sweep_runner::GcSweepRunner`]) route through
/// [`InMemoryPhysicalDeletePhase::classify_candidate`], so the dry-run
/// report provably reflects exactly what a live delete WOULD remove.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReclaimClassification {
    /// Positively reclaimable: candidate is `Swept`, the `blob_meta`
    /// row is soft-deleted past the grace window, AND `refcount == 0`.
    /// The live path purges it; the dry-run path REPORTS it and does
    /// nothing.
    Reclaimable {
        /// Canonical R2 object key that WOULD be deleted.
        r2_key: String,
        /// Bytes that WOULD be reclaimed.
        size_bytes: u64,
        /// Soft-delete instant captured from `blob_meta`.
        deleted_at_ms: u64,
        /// `now_ms` observed at the gate (one clock tick).
        now_ms: u64,
        /// Effective grace window applied.
        grace_period_ms: u64,
    },
    /// Candidate is not in `Swept` state (idempotent re-run / not yet
    /// swept) — never reclaimable on this tick.
    NotSwept {
        /// Status observed at inspection.
        observed_status: CandidateStatus,
    },
    /// Candidate is `Swept` but the `blob_meta` row is absent / no
    /// longer soft-deleted (re-uploaded within the grace window —
    /// CAP-GC-002 reversibility). Not reclaimable.
    Resolved {
        /// Status observed at inspection (always `Swept`).
        observed_status: CandidateStatus,
    },
    /// Grace window has NOT yet elapsed (`now - deleted_at_ms <=
    /// grace_period_ms`). Not reclaimable on this tick.
    GracePending {
        /// `now_ms` observed at the gate.
        now_ms: u64,
        /// Soft-delete instant captured from `blob_meta`.
        deleted_at_ms: u64,
        /// Effective grace window applied.
        grace_period_ms: u64,
    },
    /// A live re-reference incremented `refcount` above zero in the race
    /// window. NEVER reclaimable — this is the guard that protects live
    /// blobs from deletion.
    RefcountNonZero {
        /// Refcount observed at the gate.
        refcount: u32,
    },
}

/// Aggregate outcome of one physical-delete phase execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalDeleteResult {
    /// Total candidates inspected (sum across decisions).
    pub candidates_processed: u64,
    /// Confirmed purges (R2 + D1 successful).
    pub blobs_deleted_count: u64,
    /// Candidates skipped because grace has not yet expired.
    pub blobs_skipped_grace_pending: u64,
    /// Candidates skipped because the conditional `refcount = 0`
    /// predicate failed at purge time (race window).
    pub blobs_skipped_refcount_non_zero: u64,
    /// Already-resolved candidates (idempotent re-run no-op).
    pub already_resolved_count: u64,
    /// Sum of `blob_size_bytes` across `Purged` decisions.
    pub bytes_reclaimed: u64,
    /// End-to-end physical-delete duration (ms).
    pub phase_duration_ms: u64,
    /// Total audit events emitted (one per `Purged` decision; phase
    /// boundary events are NOT counted here).
    pub audit_events_emitted: u64,
}

/// Canonical [`InMemoryPhysicalDeletePhase`] error taxonomy.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PhysicalDeleteError {
    /// `gc_run` row backend / CHECK violation.
    #[error(transparent)]
    RunStore(#[from] GcRunStoreError),
    /// `gc_candidates` store backend error.
    #[error(transparent)]
    CandidatesStore(#[from] MarkError),
    /// R2 DeleteObject backend error. Orchestrator preserves the D1
    /// row; the next hourly tick retries idempotently.
    #[error("r2 backend error: {0}")]
    R2BackendError(String),
    /// Audit emission failed mid-decision; physical-delete ABORTED
    /// that candidate; status preserved as `Swept` so a subsequent
    /// re-run (post-D1 throttle backoff) can retry. Per WI §6.1.7
    /// fail-closed envelope.
    #[error(transparent)]
    AuditEmissionFailed(#[from] GcAuditSinkError),
    /// Metrics emission failure (rare; downgradeable to log-and-continue
    /// in production wiring per WI §14.s06.001.6).
    #[error(transparent)]
    Metrics(#[from] GcMetricsObserverError),
    /// Physical-delete phase budget exceeded (30 min p99 @ 100 k
    /// candidates per sprint contract §5.4 R-S06-9.1).
    #[error(
        "physical-delete phase budget exceeded: duration_ms={duration_ms} > budget_ms={budget_ms}"
    )]
    PhaseBudgetExceeded {
        /// Duration observed at the moment the budget was checked.
        duration_ms: u64,
        /// Budget ceiling.
        budget_ms: u64,
    },
    /// Region mismatch between caller context and `gc_run.region`
    /// (programmer error; mapped to 5xx).
    #[error("region mismatch: run.region={run_region} caller.region={caller_region}")]
    RegionMismatch {
        /// Region recorded on the gc_run row.
        run_region: GcRegion,
        /// Region passed by the caller.
        caller_region: GcRegion,
    },
    /// Backend transport failure (D1 throttle / parse / cross-tenant
    /// injection).
    #[error("backend error: {0}")]
    Backend(String),
}

impl From<PhysicalDeleteError> for GcError {
    fn from(err: PhysicalDeleteError) -> Self {
        match err {
            PhysicalDeleteError::RunStore(e) => GcError::RunStore(e),
            PhysicalDeleteError::AuditEmissionFailed(e) => GcError::Audit(e),
            PhysicalDeleteError::Metrics(e) => GcError::Metrics(e),
            other => GcError::RunStore(GcRunStoreError::Backend(other.to_string())),
        }
    }
}

impl From<R2DeleteError> for PhysicalDeleteError {
    fn from(err: R2DeleteError) -> Self {
        match err {
            R2DeleteError::Backend(msg) => PhysicalDeleteError::R2BackendError(msg),
        }
    }
}

// ============================================================================
//  PhysicalDeleteConfig — knobs surfaced by ScheduleConfig in production.
// ============================================================================

/// Knobs driving the physical-delete phase. The defaults pin the
/// canonical values from sprint contract §5.3 + §5.4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalDeleteConfig {
    grace_cas_ms: u64,
    grace_ac_ms: u64,
    phase_budget_ms: u64,
}

impl Default for PhysicalDeleteConfig {
    fn default() -> Self {
        Self {
            grace_cas_ms: GRACE_CAS_MS,
            grace_ac_ms: GRACE_AC_MS,
            phase_budget_ms: CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS,
        }
    }
}

impl PhysicalDeleteConfig {
    /// Construct a custom config. `grace_cas_ms` MUST be `>= grace_ac_ms`
    /// (CAS retention is a regulatory floor; AC retention is shorter);
    /// shorter CAS grace requires an ADR + customer opt-in per sprint
    /// contract §10 anti-scope.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalDeleteError::Backend`] when the invariant is
    /// violated. Production wiring lifts the values from `wrangler.toml`
    /// env overrides (`CORELINK_GC_GRACE_CAS_MS` / `CORELINK_GC_GRACE_AC_MS`).
    pub fn new(
        grace_cas_ms: u64,
        grace_ac_ms: u64,
        phase_budget_ms: u64,
    ) -> Result<Self, PhysicalDeleteError> {
        if grace_cas_ms < grace_ac_ms {
            return Err(PhysicalDeleteError::Backend(
                "grace_cas_ms must be >= grace_ac_ms (regulatory floor)".to_owned(),
            ));
        }
        if phase_budget_ms == 0 {
            return Err(PhysicalDeleteError::Backend(
                "phase_budget_ms must be > 0".to_owned(),
            ));
        }
        Ok(Self {
            grace_cas_ms,
            grace_ac_ms,
            phase_budget_ms,
        })
    }

    /// CAS grace period (ms).
    #[must_use]
    pub const fn grace_cas_ms(self) -> u64 {
        self.grace_cas_ms
    }

    /// AC grace period (ms).
    #[must_use]
    pub const fn grace_ac_ms(self) -> u64 {
        self.grace_ac_ms
    }

    /// Phase budget ceiling (ms).
    #[must_use]
    pub const fn phase_budget_ms(self) -> u64 {
        self.phase_budget_ms
    }
}

// ============================================================================
//  PhysicalDeletePhase trait + InMemoryPhysicalDeletePhase impl.
// ============================================================================

/// Trait surfaced by every physical-delete phase backend (production CF
/// Cron DO handler / in-memory fake).
pub trait PhysicalDeletePhase: Send + Sync + core::fmt::Debug {
    /// Execute the physical-delete phase end-to-end for the given
    /// `(run_id, tenant, region)`. Reads the candidate set surfaced by
    /// the sweep phase (status = `Swept`), enforces the post-grace
    /// gate per row, calls R2 DeleteObject (idempotent), purges the D1
    /// row under the conditional `refcount = 0` predicate, transitions
    /// the candidate row's `status`, and emits audit + metrics.
    ///
    /// # Errors
    ///
    /// Surface as [`PhysicalDeleteError`].
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<PhysicalDeleteResult, PhysicalDeleteError>;
}

mod phase;
pub use phase::InMemoryPhysicalDeletePhase;

// ============================================================================
//  Tests (unit + cross-component sanity).
// ============================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests;
