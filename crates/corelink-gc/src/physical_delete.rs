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
use crate::mark::{
    BlobDigest, CandidateStatus, GcCandidate, GcCandidatesStore, MarkError,
};
use crate::metrics::{GcMetricsObserver, GcMetricsObserverError};
use crate::region::GcRegion;
use crate::run::{
    CheckpointDeltas, GcPhase, GcRunStore, GcRunStoreError, GcStatus, RunId,
};
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
        g.get(&(tenant_id, digest.clone()))
            .and_then(|row| {
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
        let g = self
            .inner
            .lock()
            .map_err(|_| PhysicalDeleteError::Backend("blob_meta purge mutex poisoned".to_owned()))?;
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
        let mut g = self
            .inner
            .lock()
            .map_err(|_| PhysicalDeleteError::Backend("blob_meta purge mutex poisoned".to_owned()))?;
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

/// In-memory physical-delete phase orchestrator (the canonical
/// pure-logic skeleton). Composes the trait dependencies declared at
/// construction time.
pub struct InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    runs: Arc<S>,
    candidates: Arc<C>,
    blob_meta_purge: Arc<B>,
    r2: Arc<R>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<K>,
    config: PhysicalDeleteConfig,
}

impl<S, C, B, R, A, M, K> core::fmt::Debug for InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryPhysicalDeletePhase")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, C, B, R, A, M, K> InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    /// Construct a physical-delete phase orchestrator with the
    /// canonical defaults.
    pub fn with_defaults(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta_purge: Arc<B>,
        r2: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta_purge,
            r2,
            audit,
            metrics,
            clock,
            config: PhysicalDeleteConfig::default(),
        }
    }

    /// Construct with an explicit [`PhysicalDeleteConfig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 7 trait deps + 1 knob; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta_purge: Arc<B>,
        r2: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        config: PhysicalDeleteConfig,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta_purge,
            r2,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// The [`PhysicalDeleteConfig`] snapshot.
    #[must_use]
    pub fn config(&self) -> PhysicalDeleteConfig {
        self.config
    }

    /// Process a single candidate row through the physical-delete
    /// decision pipeline. Visible for property tests so the decision
    /// boundary can be exercised independently of the phase
    /// orchestration.
    ///
    /// # Errors
    ///
    /// Surface as [`PhysicalDeleteError`].
    pub fn step_candidate(
        &self,
        candidate: &GcCandidate,
        region: GcRegion,
    ) -> Result<PhysicalDeleteDecision, PhysicalDeleteError> {
        // Idempotent re-run guard: only `Swept` candidates progress;
        // any other status is a no-op (mark→sweep→physical-delete
        // monotone status graph).
        if candidate.status != CandidateStatus::Swept {
            return Ok(PhysicalDeleteDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        }
        // 1. Lookup the soft-deleted blob_meta row's purge state. A
        // missing row means the sweep pre-condition is no longer
        // satisfied (e.g. customer re-uploaded same digest within the
        // grace window — CAP-GC-002 reversibility — and the CAS write
        // handler reset `deleted_at_ms = NULL`). Surface as
        // AlreadyResolved.
        let Some(purge_state) = self
            .blob_meta_purge
            .lookup_purge_state(candidate.tenant_id, &candidate.digest)?
        else {
            return Ok(PhysicalDeleteDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        };
        let now = self.clock.now_ms();
        let grace_period_ms = self.config.grace_cas_ms;

        // 2. Post-grace gate (strict `>` per WI §6.1.3).
        if now.saturating_sub(purge_state.deleted_at_ms) <= grace_period_ms {
            return Ok(PhysicalDeleteDecision::SkippedGracePending {
                now_ms: now,
                deleted_at_ms: purge_state.deleted_at_ms,
                grace_period_ms,
            });
        }

        // 3. Conditional refcount = 0 re-check (Lote 10.6bis P0-4 race
        // protection). The orchestrator already saw refcount=0 in
        // `purge_state` but the predicate is re-evaluated atomically
        // at SQL DELETE time by the production wiring; we mirror that
        // by passing the condition into `conditional_purge`.
        if purge_state.refcount != 0 {
            return Ok(PhysicalDeleteDecision::SkippedRefcountNonZero {
                refcount: purge_state.refcount,
            });
        }

        // 4. R2 DeleteObject FIRST (Lote 10.6bis P0-2 ordering;
        // PAT-RETRY-IDEMPOTENT-001 semantics — both `Deleted` and
        // `NotFound` are successes).
        let r2_outcome = self
            .r2
            .delete(candidate.tenant_id, region, &purge_state.r2_key)
            .map_err(PhysicalDeleteError::from)?;

        // 5. Audit emit BEFORE flipping the row status — fail-closed
        // envelope (mirrors sweep). Production wiring atomically rolls
        // back the D1 batch (DELETE blob_meta + DELETE gc_candidate +
        // INSERT audit_outbox) on emit failure; the in-memory fake's
        // lower fidelity is documented in the module-level rustdoc.
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::PhysicalDeleted,
            run_id: candidate.mark_run_id,
            tenant_id: candidate.tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::PhysicalDelete),
            to_phase: Some(GcPhase::PhysicalDelete),
            created_by_request_id: "cron".to_owned(),
            reason: "physical_deleted",
            now_ms: now,
        })?;

        // 6. D1 conditional row purge (re-checks refcount=0 + grace
        // gate atomically per the SQL predicate).
        let purged = self.blob_meta_purge.conditional_purge(
            candidate.tenant_id,
            &candidate.digest,
            now,
            grace_period_ms,
        )?;
        if !purged {
            // Race window: the predicate was true at lookup time but
            // false at SQL evaluation time (customer CAS write
            // re-incremented refcount post-lookup, OR clock ran
            // backwards). Surface as a skipped decision so the
            // candidate row is preserved.
            return Ok(PhysicalDeleteDecision::SkippedRefcountNonZero {
                refcount: purge_state.refcount,
            });
        }

        // 7. Atomic candidate transition; if drift detected (concurrent
        // re-run), observe and surface AlreadyResolved.
        let fired = self.candidates.transition_status(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_run_id,
            CandidateStatus::Swept,
            CandidateStatus::PhysicallyDeleted,
            now,
            None,
        )?;
        if !fired {
            // Concurrent winner; observe and surface AlreadyResolved.
            let observed = self
                .candidates
                .lookup(candidate.tenant_id, &candidate.digest, candidate.mark_run_id)?
                .map_or(CandidateStatus::Swept, |c| c.status);
            return Ok(PhysicalDeleteDecision::AlreadyResolved {
                observed_status: observed,
            });
        }
        Ok(PhysicalDeleteDecision::Purged {
            purged_at_ms: now,
            bytes_reclaimed: purge_state.size_bytes,
            r2_outcome,
        })
    }
}

impl<S, C, B, R, A, M, K> PhysicalDeletePhase for InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<PhysicalDeleteResult, PhysicalDeleteError> {
        // 1. Read gc_run + verify region match.
        let run_row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(PhysicalDeleteError::RunStore(GcRunStoreError::NotFound(
                run_id,
            )))?;
        if run_row.region != region {
            return Err(PhysicalDeleteError::RegionMismatch {
                run_region: run_row.region,
                caller_region: region,
            });
        }

        // 2. Phase budget deadline.
        let phase_start = self.clock.now_ms();
        let deadline_ms = phase_start.saturating_add(self.config.phase_budget_ms);

        // 3. Iterate candidates for the run.
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id)?;
        let mut blobs_deleted_count: u64 = 0;
        let mut blobs_skipped_grace_pending: u64 = 0;
        let mut blobs_skipped_refcount_non_zero: u64 = 0;
        let mut already_resolved_count: u64 = 0;
        let mut bytes_reclaimed: u64 = 0;
        let mut audit_events_emitted: u64 = 0;
        let mut candidates_processed: u64 = 0;
        for candidate in &candidates {
            // Phase-budget probe per candidate (cheaper than per
            // mutation; aligns with sweep phase pattern).
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(PhysicalDeleteError::PhaseBudgetExceeded {
                    duration_ms: now_for_probe.saturating_sub(phase_start),
                    budget_ms: self.config.phase_budget_ms,
                });
            }
            let decision = self.step_candidate(candidate, region)?;
            candidates_processed = candidates_processed.saturating_add(1);
            match decision {
                PhysicalDeleteDecision::Purged {
                    bytes_reclaimed: br,
                    ..
                } => {
                    blobs_deleted_count = blobs_deleted_count.saturating_add(1);
                    bytes_reclaimed = bytes_reclaimed.saturating_add(br);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                PhysicalDeleteDecision::SkippedGracePending { .. } => {
                    blobs_skipped_grace_pending =
                        blobs_skipped_grace_pending.saturating_add(1);
                }
                PhysicalDeleteDecision::SkippedRefcountNonZero { .. } => {
                    blobs_skipped_refcount_non_zero =
                        blobs_skipped_refcount_non_zero.saturating_add(1);
                }
                PhysicalDeleteDecision::AlreadyResolved { .. } => {
                    already_resolved_count = already_resolved_count.saturating_add(1);
                }
            }
        }

        // 4. Checkpoint counters in gc_run.
        let phase_end = self.clock.now_ms();
        let duration_ms = phase_end.saturating_sub(phase_start);
        self.runs.checkpoint(
            run_id,
            tenant_id,
            phase_end,
            CheckpointDeltas {
                blobs_physically_deleted_delta: blobs_deleted_count,
                bytes_reclaimed_delta: bytes_reclaimed,
                ..Default::default()
            },
        )?;

        // 5. Phase duration histogram (canonical metric).
        self.metrics.record_phase_duration_ms(
            GcPhase::PhysicalDelete,
            tenant_id,
            region,
            duration_ms,
        )?;

        Ok(PhysicalDeleteResult {
            candidates_processed,
            blobs_deleted_count,
            blobs_skipped_grace_pending,
            blobs_skipped_refcount_non_zero,
            already_resolved_count,
            bytes_reclaimed,
            phase_duration_ms: duration_ms,
            audit_events_emitted,
        })
    }
}

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
mod tests {
    use super::*;

    use crate::audit::InMemoryGcAuditSink;
    use crate::mark::InMemoryGcCandidatesStore;
    use crate::metrics::InMemoryGcMetrics;
    use crate::run::InMemoryGcRunStore;

    fn digest(seed: u32) -> BlobDigest {
        let prefix = format!("{seed:08x}");
        let mut s = prefix;
        s.push_str(&"0".repeat(BlobDigest::LEN - 8));
        BlobDigest::parse(&s).expect("canonical hex")
    }

    fn r2_key_for(tenant_id: Uuid, digest: &BlobDigest) -> String {
        format!("cas/{tenant_id}/{digest}")
    }

    type Fixture = (
        InMemoryPhysicalDeletePhase<
            InMemoryGcRunStore,
            InMemoryGcCandidatesStore,
            InMemoryBlobMetaPurgeStore,
            InMemoryR2Delete,
            InMemoryGcAuditSink,
            InMemoryGcMetrics,
            CountingPhysicalDeleteClock,
        >,
        Arc<InMemoryGcRunStore>,
        Arc<InMemoryGcCandidatesStore>,
        Arc<InMemoryBlobMetaPurgeStore>,
        Arc<InMemoryR2Delete>,
        Arc<InMemoryGcAuditSink>,
    );

    fn fresh(start_ms: u64) -> Fixture {
        let runs = Arc::new(InMemoryGcRunStore::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let blob_meta_purge = Arc::new(InMemoryBlobMetaPurgeStore::new());
        let r2 = Arc::new(InMemoryR2Delete::new());
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingPhysicalDeleteClock::new(start_ms));
        let phase = InMemoryPhysicalDeletePhase::with_defaults(
            Arc::clone(&runs),
            Arc::clone(&candidates),
            Arc::clone(&blob_meta_purge),
            Arc::clone(&r2),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        (phase, runs, candidates, blob_meta_purge, r2, audit)
    }

    fn seed_run_in_physical_delete(
        runs: &InMemoryGcRunStore,
        rid: RunId,
        tenant: Uuid,
        region: GcRegion,
        mark_anchor: u64,
    ) {
        runs.insert_pending(rid, tenant, region, 100, "cron".into())
            .unwrap();
        runs.acquire_running(rid, tenant, 200).unwrap();
        runs.transition_phase(rid, tenant, GcPhase::Mark, mark_anchor)
            .unwrap();
        runs.transition_phase(rid, tenant, GcPhase::Sweep, mark_anchor + 10)
            .unwrap();
        runs.transition_phase(rid, tenant, GcPhase::PhysicalDelete, mark_anchor + 20)
            .unwrap();
    }

    /// Seed a `Swept` candidate with the matching `blob_meta` purge row
    /// (refcount=0, soft-deleted at `deleted_at_ms`) and the matching
    /// R2 key. Mirrors the sweep→physical-delete handoff.
    #[allow(clippy::too_many_arguments, reason = "test fixture: 10 fixture-state arguments collapsing to a single setup helper kept readable inline rather than via a builder type")]
    fn seed_swept_candidate(
        candidates: &InMemoryGcCandidatesStore,
        blob_meta: &InMemoryBlobMetaPurgeStore,
        r2: &InMemoryR2Delete,
        tenant: Uuid,
        region: GcRegion,
        rid: RunId,
        d: BlobDigest,
        mark_anchor: u64,
        size_bytes: u64,
        deleted_at_ms: u64,
    ) {
        candidates
            .insert_candidate(GcCandidate {
                tenant_id: tenant,
                digest: d.clone(),
                mark_started_at_ms: mark_anchor,
                mark_run_id: rid,
                blob_size_bytes: size_bytes,
                blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
                status: CandidateStatus::Candidate,
                created_at_ms: mark_anchor,
                swept_at_ms: None,
                protected_at_ms: None,
                protected_reason: None,
            })
            .unwrap();
        // Sweep would have flipped the candidate to Swept; we drive the
        // transition here for fixture clarity.
        candidates
            .transition_status(
                tenant,
                &d,
                rid,
                CandidateStatus::Candidate,
                CandidateStatus::Swept,
                deleted_at_ms,
                None,
            )
            .unwrap();
        let key = r2_key_for(tenant, &d);
        r2.seed(tenant, region, key.clone());
        blob_meta.push_soft_deleted(tenant, d, key, size_bytes, deleted_at_ms);
    }

    #[test]
    fn canonical_phase_budget_pinned() {
        // 30 min p99 @ 100k candidates per (tenant, region) hourly tick
        // — sprint contract §5.4 R-S06-9.1.
        assert_eq!(CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS, 30 * 60 * 1000);
    }

    #[test]
    fn config_default_canonical() {
        let cfg = PhysicalDeleteConfig::default();
        assert_eq!(cfg.grace_cas_ms(), GRACE_CAS_MS);
        assert_eq!(cfg.grace_ac_ms(), GRACE_AC_MS);
        assert_eq!(cfg.phase_budget_ms(), CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS);
    }

    #[test]
    fn config_rejects_inverted_grace() {
        let err = PhysicalDeleteConfig::new(GRACE_AC_MS, GRACE_CAS_MS, 1).unwrap_err();
        assert!(matches!(err, PhysicalDeleteError::Backend(_)));
    }

    #[test]
    fn config_rejects_zero_budget() {
        assert!(PhysicalDeleteConfig::new(GRACE_CAS_MS, GRACE_AC_MS, 0).is_err());
    }

    #[test]
    fn r2_delete_idempotent_returns_not_found_on_replay() {
        let r2 = InMemoryR2Delete::new();
        let tenant = Uuid::from_u128(1);
        let key = "cas/abc/d";
        r2.seed(tenant, GcRegion::Sam, key);
        assert_eq!(
            r2.delete(tenant, GcRegion::Sam, key).unwrap(),
            R2DeleteOutcome::Deleted
        );
        // Idempotent re-call: NotFound; orchestrator treats as success.
        assert_eq!(
            r2.delete(tenant, GcRegion::Sam, key).unwrap(),
            R2DeleteOutcome::NotFound
        );
        // Third re-call: still NotFound (idempotent retry-safe).
        assert_eq!(
            r2.delete(tenant, GcRegion::Sam, key).unwrap(),
            R2DeleteOutcome::NotFound
        );
    }

    #[test]
    fn r2_delete_tenant_isolation() {
        // Tenant A and Tenant B both seed the same key under the same
        // region; deleting under Tenant A's scope MUST NOT remove
        // Tenant B's key.
        let r2 = InMemoryR2Delete::new();
        let ta = Uuid::from_u128(1);
        let tb = Uuid::from_u128(2);
        let key = "cas/x/y";
        r2.seed(ta, GcRegion::Sam, key);
        r2.seed(tb, GcRegion::Sam, key);
        assert_eq!(
            r2.delete(ta, GcRegion::Sam, key).unwrap(),
            R2DeleteOutcome::Deleted
        );
        // Tenant B's key still present.
        assert!(r2.contains(tb, GcRegion::Sam, key));
        // Tenant A's key absent.
        assert!(!r2.contains(ta, GcRegion::Sam, key));
    }

    #[test]
    fn happy_path_post_grace_purge() {
        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        // Now is well past grace.
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xabcd);
        let key = r2_key_for(tenant, &d);
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            GcRegion::Sam,
            rid,
            d.clone(),
            mark_anchor,
            4096,
            deleted_at_ms,
        );

        let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.candidates_processed, 1);
        assert_eq!(result.blobs_deleted_count, 1);
        assert_eq!(result.blobs_skipped_grace_pending, 0);
        assert_eq!(result.blobs_skipped_refcount_non_zero, 0);
        assert_eq!(result.bytes_reclaimed, 4096);
        // R2 key removed.
        assert!(!r2.contains(tenant, GcRegion::Sam, &key));
        // blob_meta row removed.
        assert!(!blob_meta.contains(tenant, &d));
        // gc_candidate flipped to PhysicallyDeleted.
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::PhysicallyDeleted);
        // Audit emitted.
        assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 1);
    }

    #[test]
    fn skipped_grace_pending_strict_boundary() {
        // EXACTLY at boundary: now - deleted_at_ms == grace_period_ms
        // — strict `>` says SKIP (not yet expired).
        // CountingPhysicalDeleteClock advances +1 per call: phase_start
        // + now_for_probe + step_candidate's now = 3 ticks consumed
        // before the gate evaluates. Set start so that the
        // step_candidate tick lands EXACTLY at grace boundary
        // (= deleted_at_ms + GRACE_CAS_MS), where strict `>` skips.
        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS - 2; // step_candidate tick = boundary.
        let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xbb);
        let key = r2_key_for(tenant, &d);
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            GcRegion::Sam,
            rid,
            d.clone(),
            mark_anchor,
            512,
            deleted_at_ms,
        );

        let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.blobs_deleted_count, 0);
        assert_eq!(result.blobs_skipped_grace_pending, 1);
        assert_eq!(result.bytes_reclaimed, 0);
        // R2 key still present.
        assert!(r2.contains(tenant, GcRegion::Sam, &key));
        // blob_meta row still present.
        assert!(blob_meta.contains(tenant, &d));
        // gc_candidate still Swept.
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::Swept);
        // No audit emit on skipped path.
        assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
    }

    #[test]
    fn skipped_refcount_non_zero_race_protection() {
        // Customer CAS write incremented refcount in the race window
        // between sweep and physical-delete; the conditional predicate
        // skips the row.
        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xabba);
        let key = r2_key_for(tenant, &d);
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            GcRegion::Sam,
            rid,
            d.clone(),
            mark_anchor,
            2048,
            deleted_at_ms,
        );
        // Race: customer CAS write bumped refcount to 1.
        assert!(blob_meta.set_refcount(tenant, &d, 1));

        let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.blobs_deleted_count, 0);
        assert_eq!(result.blobs_skipped_refcount_non_zero, 1);
        // R2 NOT called (orchestrator skips before R2 step on refcount
        // pre-check).
        assert!(r2.contains(tenant, GcRegion::Sam, &key));
        // blob_meta row still present.
        assert!(blob_meta.contains(tenant, &d));
        // gc_candidate still Swept (no transition fired).
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::Swept);
        // No audit emit.
        assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
    }

    #[test]
    fn idempotent_re_run_no_double_purge() {
        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xcafe);
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            GcRegion::Sam,
            rid,
            d,
            mark_anchor,
            1024,
            deleted_at_ms,
        );

        let r1 = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
        let r2_result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(r1.blobs_deleted_count, 1);
        assert_eq!(r2_result.blobs_deleted_count, 0);
        assert_eq!(r2_result.already_resolved_count, 1);
        // Audit emitted ONCE.
        assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 1);
    }

    #[test]
    fn tenant_isolation_no_cross_purge() {
        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
        let ta = Uuid::from_u128(1);
        let tb = Uuid::from_u128(2);
        let ra = RunId(Uuid::from_u128(10));
        let rb = RunId(Uuid::from_u128(11));
        seed_run_in_physical_delete(&runs, ra, ta, GcRegion::Sam, mark_anchor);
        seed_run_in_physical_delete(&runs, rb, tb, GcRegion::Sam, mark_anchor);
        let d = digest(0xdead);
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            ta,
            GcRegion::Sam,
            ra,
            d.clone(),
            mark_anchor,
            128,
            deleted_at_ms,
        );
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tb,
            GcRegion::Sam,
            rb,
            d.clone(),
            mark_anchor,
            128,
            deleted_at_ms,
        );
        // Tenant B has a customer re-upload (refcount = 1) — tenant A
        // is unaffected.
        assert!(blob_meta.set_refcount(tb, &d, 1));

        let result_a = phase.execute(ra, ta, GcRegion::Sam).unwrap();
        let result_b = phase.execute(rb, tb, GcRegion::Sam).unwrap();
        assert_eq!(result_a.blobs_deleted_count, 1);
        assert_eq!(result_b.blobs_deleted_count, 0);
        assert_eq!(result_b.blobs_skipped_refcount_non_zero, 1);
        // Audit captures the per-tenant decisions.
        let purged = audit.snapshot_of(GcEventType::PhysicalDeleted);
        assert_eq!(purged.len(), 1);
        assert_eq!(purged[0].tenant_id, ta);
        // Tenant A's row removed; tenant B's row preserved.
        assert!(!blob_meta.contains(ta, &d));
        assert!(blob_meta.contains(tb, &d));
    }

    #[test]
    fn region_mismatch_rejected() {
        let (phase, runs, _c, _b, _r, _a) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let err = phase.execute(rid, tenant, GcRegion::Iad).unwrap_err();
        assert!(matches!(err, PhysicalDeleteError::RegionMismatch { .. }));
    }

    #[test]
    fn run_not_found_rejected() {
        let (phase, _runs, _c, _b, _r, _a) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(99));
        let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, PhysicalDeleteError::RunStore(_)));
    }

    #[test]
    fn cross_tenant_run_returns_not_found() {
        // The Layer 4 envelope is tested at the run-store seam:
        // gc_run.lookup(rid, wrong_tenant) returns Ok(None) → NotFound.
        let (phase, runs, _c, _b, _r, _a) = fresh(1_000);
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let rid = RunId(Uuid::from_u128(99));
        seed_run_in_physical_delete(&runs, rid, tenant_a, GcRegion::Sam, 500);
        let err = phase.execute(rid, tenant_b, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, PhysicalDeleteError::RunStore(_)));
    }

    #[test]
    fn already_resolved_for_physically_deleted_candidate() {
        // A candidate already in PhysicallyDeleted state is observed
        // as AlreadyResolved by step_candidate.
        let (phase, _runs, candidates, blob_meta, r2, _audit) = fresh(1_000_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let d = digest(0x4242);
        let mark_anchor = 500;
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            GcRegion::Sam,
            rid,
            d.clone(),
            mark_anchor,
            64,
            mark_anchor + 100,
        );
        candidates
            .transition_status(
                tenant,
                &d,
                rid,
                CandidateStatus::Swept,
                CandidateStatus::PhysicallyDeleted,
                mark_anchor + 200,
                None,
            )
            .unwrap();
        let row = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        let dec = phase.step_candidate(&row, GcRegion::Sam).unwrap();
        match dec {
            PhysicalDeleteDecision::AlreadyResolved { observed_status } => {
                assert_eq!(observed_status, CandidateStatus::PhysicallyDeleted);
            }
            other => panic!("expected AlreadyResolved, got {other:?}"),
        }
    }

    #[test]
    fn already_resolved_for_protected_re_ref() {
        // A candidate in ProtectedReRef state (sweep INV-GC-004) is
        // never physical-deleted; observed as AlreadyResolved.
        let (phase, _runs, candidates, blob_meta, r2, _audit) = fresh(1_000_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let d = digest(0x5555);
        let mark_anchor = 500;
        seed_swept_candidate(
            &candidates,
            &blob_meta,
            &r2,
            tenant,
            GcRegion::Sam,
            rid,
            d.clone(),
            mark_anchor,
            64,
            mark_anchor + 100,
        );
        candidates
            .transition_status(
                tenant,
                &d,
                rid,
                CandidateStatus::Swept,
                CandidateStatus::ProtectedReRef,
                mark_anchor + 200,
                Some("test_protected".to_owned()),
            )
            .unwrap();
        let row = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        let dec = phase.step_candidate(&row, GcRegion::Sam).unwrap();
        match dec {
            PhysicalDeleteDecision::AlreadyResolved { observed_status } => {
                assert_eq!(observed_status, CandidateStatus::ProtectedReRef);
            }
            other => panic!("expected AlreadyResolved, got {other:?}"),
        }
    }

    #[test]
    fn already_resolved_when_blob_meta_undeleted() {
        // CAP-GC-002 reversibility: customer re-uploaded same digest;
        // CAS write handler reset deleted_at_ms = NULL. Physical-delete
        // sees no purge state → AlreadyResolved.
        let mark_anchor = 1_000;
        let now_start = mark_anchor + GRACE_CAS_MS + 10_000;
        let (phase, runs, candidates, _blob_meta, _r2, audit) = fresh(now_start);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xfeed);
        // Insert a Swept candidate but DO NOT seed a soft-deleted
        // blob_meta row → the CAP-GC-002 undelete fast-path.
        candidates
            .insert_candidate(GcCandidate {
                tenant_id: tenant,
                digest: d.clone(),
                mark_started_at_ms: mark_anchor,
                mark_run_id: rid,
                blob_size_bytes: 64,
                blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
                status: CandidateStatus::Candidate,
                created_at_ms: mark_anchor,
                swept_at_ms: None,
                protected_at_ms: None,
                protected_reason: None,
            })
            .unwrap();
        candidates
            .transition_status(
                tenant,
                &d,
                rid,
                CandidateStatus::Candidate,
                CandidateStatus::Swept,
                mark_anchor + 100,
                None,
            )
            .unwrap();
        let result = phase.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.blobs_deleted_count, 0);
        assert_eq!(result.already_resolved_count, 1);
        assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
    }

    #[test]
    fn r2_backend_failure_preserves_d1_row() {
        // Custom R2 trait that always errors; assert no D1 mutation
        // and no audit emit (orchestrator preserves the candidate row
        // for next-tick retry).
        #[derive(Debug, Default)]
        struct FailingR2;
        impl R2Delete for FailingR2 {
            fn delete(
                &self,
                _tenant_id: Uuid,
                _region: GcRegion,
                _key: &str,
            ) -> Result<R2DeleteOutcome, R2DeleteError> {
                Err(R2DeleteError::Backend("simulated_503".to_owned()))
            }
        }

        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let runs = Arc::new(InMemoryGcRunStore::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let blob_meta = Arc::new(InMemoryBlobMetaPurgeStore::new());
        let r2: Arc<FailingR2> = Arc::new(FailingR2);
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingPhysicalDeleteClock::new(now_start));
        let phase = InMemoryPhysicalDeletePhase::with_defaults(
            Arc::clone(&runs),
            Arc::clone(&candidates),
            Arc::clone(&blob_meta),
            Arc::clone(&r2),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xdf01);
        let key = r2_key_for(tenant, &d);
        candidates
            .insert_candidate(GcCandidate {
                tenant_id: tenant,
                digest: d.clone(),
                mark_started_at_ms: mark_anchor,
                mark_run_id: rid,
                blob_size_bytes: 4096,
                blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
                status: CandidateStatus::Candidate,
                created_at_ms: mark_anchor,
                swept_at_ms: None,
                protected_at_ms: None,
                protected_reason: None,
            })
            .unwrap();
        candidates
            .transition_status(
                tenant,
                &d,
                rid,
                CandidateStatus::Candidate,
                CandidateStatus::Swept,
                deleted_at_ms,
                None,
            )
            .unwrap();
        blob_meta.push_soft_deleted(tenant, d.clone(), key, 4096, deleted_at_ms);

        let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, PhysicalDeleteError::R2BackendError(_)));
        // gc_candidate row preserved as Swept.
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::Swept);
        // blob_meta row preserved.
        assert!(blob_meta.contains(tenant, &d));
        // No audit emit.
        assert_eq!(audit.snapshot_of(GcEventType::PhysicalDeleted).len(), 0);
    }

    #[test]
    fn audit_emit_failure_aborts_step() {
        // Use a sink that always errors; assert the orchestrator
        // surfaces AuditEmissionFailed and the candidate is preserved.
        #[derive(Debug, Default)]
        struct FailingSink;
        impl GcAuditSink for FailingSink {
            fn emit(&self, _record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
                Err(GcAuditSinkError::Store("simulated_audit_fail".to_owned()))
            }
        }

        let mark_anchor = 1_000;
        let deleted_at_ms = mark_anchor + 100;
        let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
        let runs = Arc::new(InMemoryGcRunStore::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let blob_meta = Arc::new(InMemoryBlobMetaPurgeStore::new());
        let r2 = Arc::new(InMemoryR2Delete::new());
        let audit = Arc::new(FailingSink);
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingPhysicalDeleteClock::new(now_start));
        let phase = InMemoryPhysicalDeletePhase::with_defaults(
            Arc::clone(&runs),
            Arc::clone(&candidates),
            Arc::clone(&blob_meta),
            Arc::clone(&r2),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        seed_run_in_physical_delete(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xa11d);
        let key = r2_key_for(tenant, &d);
        r2.seed(tenant, GcRegion::Sam, key.clone());
        blob_meta.push_soft_deleted(tenant, d.clone(), key, 4096, deleted_at_ms);
        candidates
            .insert_candidate(GcCandidate {
                tenant_id: tenant,
                digest: d.clone(),
                mark_started_at_ms: mark_anchor,
                mark_run_id: rid,
                blob_size_bytes: 4096,
                blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
                status: CandidateStatus::Candidate,
                created_at_ms: mark_anchor,
                swept_at_ms: None,
                protected_at_ms: None,
                protected_reason: None,
            })
            .unwrap();
        candidates
            .transition_status(
                tenant,
                &d,
                rid,
                CandidateStatus::Candidate,
                CandidateStatus::Swept,
                deleted_at_ms,
                None,
            )
            .unwrap();

        let err = phase.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, PhysicalDeleteError::AuditEmissionFailed(_)));
        // gc_candidate row preserved as Swept.
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::Swept);
        // R2 was called BEFORE audit emit (Lote 10.6bis P0-2 ordering),
        // so the R2 key is removed; production wiring's reconcile job
        // (WI-S06-005) detects the orphan blob_meta row in the next 24h.
    }

    #[test]
    fn region_value_round_trip_in_audit() {
        // Sanity: every region appears in the audit emit.
        for region in GcRegion::all() {
            let mark_anchor = 1_000;
            let deleted_at_ms = mark_anchor + 100;
            let now_start = deleted_at_ms + GRACE_CAS_MS + 10_000;
            let (phase, runs, candidates, blob_meta, r2, audit) = fresh(now_start);
            let tenant = Uuid::from_u128(u128::from(region.as_str().len() as u32) + 1);
            let rid = RunId(Uuid::from_u128(42));
            seed_run_in_physical_delete(&runs, rid, tenant, *region, mark_anchor);
            let d = digest(0xc0de);
            seed_swept_candidate(
                &candidates,
                &blob_meta,
                &r2,
                tenant,
                *region,
                rid,
                d,
                mark_anchor,
                256,
                deleted_at_ms,
            );
            let _ = phase.execute(rid, tenant, *region).unwrap();
            let recs = audit.snapshot_of(GcEventType::PhysicalDeleted);
            assert_eq!(recs.len(), 1);
            assert_eq!(recs[0].region, *region);
        }
    }

    #[test]
    fn purge_state_lookup_returns_none_when_not_soft_deleted() {
        // A row whose deleted_at_ms = None → lookup_purge_state returns
        // None.
        let store = InMemoryBlobMetaPurgeStore::new();
        let tenant = Uuid::from_u128(1);
        let d = digest(0x9999);
        // No soft-delete; lookup returns None.
        let res = store.lookup_purge_state(tenant, &d).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn conditional_purge_skips_when_grace_pending() {
        let store = InMemoryBlobMetaPurgeStore::new();
        let tenant = Uuid::from_u128(1);
        let d = digest(0x1234);
        let deleted_at_ms = 1_000;
        store.push_soft_deleted(tenant, d.clone(), "cas/key", 256, deleted_at_ms);
        // now - deleted_at_ms == grace_period_ms exactly → strict `>` SKIP.
        let now = deleted_at_ms + GRACE_CAS_MS;
        let purged = store
            .conditional_purge(tenant, &d, now, GRACE_CAS_MS)
            .unwrap();
        assert!(!purged);
        assert!(store.contains(tenant, &d));
    }

    #[test]
    fn conditional_purge_fires_when_one_ms_past_grace() {
        let store = InMemoryBlobMetaPurgeStore::new();
        let tenant = Uuid::from_u128(1);
        let d = digest(0x5678);
        let deleted_at_ms = 1_000;
        store.push_soft_deleted(tenant, d.clone(), "cas/key", 256, deleted_at_ms);
        // now - deleted_at_ms == grace_period_ms + 1 → strict `>` FIRES.
        let now = deleted_at_ms + GRACE_CAS_MS + 1;
        let purged = store
            .conditional_purge(tenant, &d, now, GRACE_CAS_MS)
            .unwrap();
        assert!(purged);
        assert!(!store.contains(tenant, &d));
    }

    #[test]
    fn conditional_purge_skips_when_refcount_non_zero() {
        let store = InMemoryBlobMetaPurgeStore::new();
        let tenant = Uuid::from_u128(1);
        let d = digest(0xab12);
        let deleted_at_ms = 1_000;
        store.push_soft_deleted(tenant, d.clone(), "cas/key", 256, deleted_at_ms);
        assert!(store.set_refcount(tenant, &d, 1));
        // Past grace + refcount=1 → skip.
        let now = deleted_at_ms + GRACE_CAS_MS + 100_000;
        let purged = store
            .conditional_purge(tenant, &d, now, GRACE_CAS_MS)
            .unwrap();
        assert!(!purged);
        assert!(store.contains(tenant, &d));
    }
}
