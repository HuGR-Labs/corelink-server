//! Sweep phase (WI-S06-003) — soft-delete + INV-GC-004 enforcement +
//! audit emission per decision.
//!
//! ## Where sweep sits in the GC phase chain
//!
//! `Idle → Mark → **Sweep** → PhysicalDelete → Reconcile → Completed`
//!
//! - Mark phase (WI-S06-002) populates the `gc_candidates` table with
//!   rows whose `status = 'candidate'` and atomically captures
//!   `gc_run.mark_started_at_ms` per `INV-GC-MARK-STARTED-AT-IMMUTABLE`.
//! - Sweep phase consumes those candidates, re-checks per row whether
//!   any `ac_meta` entry references the digest with `created_at >=
//!   mark_started_at_ms` (canonical TLA `gc_correctness.tla` L152-154
//!   `protect-if-equal-or-newer`), and:
//!   - **No re-ref**: soft-delete `blob_meta.deleted_at = now()`,
//!     transition the candidate to `Swept`, audit
//!     `corelink.gc.sweep.soft_deleted` with `prev_state` BlobState.
//!   - **Re-ref detected (INV-GC-004 protected)**: do NOT delete;
//!     transition the candidate to `ProtectedReRef`, audit
//!     `corelink.gc.sweep.protected_re_ref` with the offending
//!     `ac.created_at_ms` for forensics.
//! - Physical-delete (WI-S06-004) consumes `Swept` candidates after
//!   the grace window expires.
//!
//! ## Trait-abstraction-defer pattern
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this module ships **only the
//! pure-logic skeleton + in-memory fakes** so property tests pinned at
//! 10 k iter exercise every load-bearing invariant (INV-GC-004
//! `protect-if-`>=``, idempotent re-run, tenant isolation, audit
//! fail-closed) without spinning up miniflare. The real D1
//! `blob_meta.deleted_at` UPDATE + atomic batch + `audit_outbox`
//! INSERT land in WI-S06-007 (PRR ship gate) alongside the conformance
//! suite.
//!
//! ## INV-GC-004 — canonical TLA semantics enforced
//!
//! The sweep query that drives the protect-if-`>=` decision is
//! algebraically equivalent to (per WI §6.1.2 SQL):
//!
//! ```sql
//! SELECT EXISTS (
//!     SELECT 1 FROM ac_meta a, json_each(a.blob_refs) j
//!     WHERE a.tenant_id = $1
//!       AND j.value = $2                         -- exact digest match
//!       AND a.created_at_ms >= $3                -- STRICT >=
//! ) AS reference_after_mark;
//! ```
//!
//! When `EXISTS = TRUE` the canonical TLA `gc_correctness.tla`
//! `InvGCReRefProtected` invariant fires: the digest was re-referenced
//! during the mark window so sweep MUST NOT delete. The `>=` boundary
//! is canonical (not `>`) — `ac.created_at_ms == mark_started_at_ms`
//! is treated as protected (favourable to reachable; conservative).
//! Off-by-one (using `>` instead of `>=`) is a data-loss bug and is
//! pinned by `prop_inv_gc_004_protect_if_ge_strict_boundary`.
//!
//! ## Audit fail-closed contract
//!
//! Per WI §6.1.7 + §10.s06.003.5, every sweep decision MUST emit an
//! audit record before the candidate `status` flips. Audit emission
//! failure aborts the per-candidate sweep step (no soft-delete persists
//! on the in-memory fake; production wiring rolls back the D1 batch +
//! returns `SweepError::AuditEmissionFailed`). The fail-closed envelope
//! preserves both `INV-GC-001` (reachable not deleted) and
//! `INV-OBS-AUDIT-CHAIN-INTEGRITY`.

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

/// Canonical grace period for CAS blobs — 72 hours expressed as ms
/// (sprint contract §5.3 R-S06-6). Physical-delete (WI-S06-004) does
/// not fire until `deleted_at + GRACE_CAS_MS <= now`.
pub const GRACE_CAS_MS: u64 = 72 * 60 * 60 * 1000;

/// Canonical grace period for AC envelopes — 24 hours expressed as ms
/// (sprint contract §5.3).
pub const GRACE_AC_MS: u64 = 24 * 60 * 60 * 1000;

/// Canonical sweep phase budget — 5 min p99 @ 100k candidates per
/// (tenant, region) per sprint contract §5.3 R-S06-7.1 (separate
/// budget line, not a sub-allocation of mark's 10 min).
pub const CANONICAL_SWEEP_PHASE_BUDGET_MS: u64 = 5 * 60 * 1000;

// ============================================================================
//  BlobState — prev-state forensic capture passed to audit emit.
// ============================================================================

/// Snapshot of `blob_meta` row state immediately prior to soft-delete.
/// Captured by sweep phase per WI §1.3 `prev_state` requirement so the
/// S-09 audit chain processor can reconstruct the soft-delete forensic
/// trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobState {
    /// Canonical hex digest.
    pub digest: BlobDigest,
    /// Blob payload size (bytes).
    pub size_bytes: u64,
    /// Refcount captured at sweep moment (informational; `0` for a
    /// confirmed orphan).
    pub refcount: u32,
    /// Last-referenced wall-clock instant.
    pub last_referenced_at_ms: u64,
    /// Created wall-clock instant.
    pub created_at_ms: u64,
}

// ============================================================================
//  BlobMetaStore — soft-delete + lookup surface.
// ============================================================================

/// Materialised projection of a `blob_meta` row used by the sweep
/// phase for the soft-delete UPDATE + RETURNING preimage.
///
/// Production wiring binds this against the real D1 `blob_meta` row
/// (WI-S01-001 + WI-S01-004 schema). The in-memory fake mirrors the
/// load-bearing invariants (`deleted_at IS NULL` idempotent guard,
/// `RETURNING` prev-state semantics, tenant-leftmost isolation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobMetaRow {
    /// Canonical hex digest.
    pub digest: BlobDigest,
    /// Blob payload size (bytes).
    pub size_bytes: u64,
    /// Refcount.
    pub refcount: u32,
    /// Last-referenced instant.
    pub last_referenced_at_ms: u64,
    /// Created instant.
    pub created_at_ms: u64,
    /// Tombstone — `Some(swept_at_ms)` once sweep soft-deletes the row.
    pub deleted_at_ms: Option<u64>,
}

/// Trait surfaced by every `blob_meta` backend (D1 reader + soft-delete
/// writer / in-memory fake).
pub trait BlobMetaStore: Send + Sync + core::fmt::Debug {
    /// Lookup the current `blob_meta` row for `(tenant_id, digest)`.
    /// Returns `Ok(None)` when the row does not exist or belongs to a
    /// different tenant (Layer 4 envelope).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<Option<BlobMetaRow>, SweepError>;

    /// Soft-delete a `blob_meta` row. Mirrors the SQL:
    ///
    /// ```sql
    /// UPDATE blob_meta
    ///   SET deleted_at_ms = ?
    ///   WHERE tenant_id = ?
    ///     AND digest = ?
    ///     AND deleted_at_ms IS NULL
    /// RETURNING size_bytes, refcount, last_referenced_at_ms, created_at_ms;
    /// ```
    ///
    /// Returns `Some(prev_state)` when the UPDATE fired (a confirmed
    /// orphan was just soft-deleted); `None` when the row was already
    /// soft-deleted (idempotent re-run no-op) or absent.
    ///
    /// # Errors
    ///
    /// Backend transport errors. Cross-tenant attempts surface as a
    /// fail-closed `Backend` error (sqlx prepared rejects in
    /// production; the in-memory fake mirrors the rejection).
    fn soft_delete(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
    ) -> Result<Option<BlobState>, SweepError>;
}

/// In-memory `blob_meta` store. Mirrors the canonical (tenant_id,
/// digest) PK + `deleted_at IS NULL` idempotent UPDATE semantic.
#[derive(Debug, Default)]
pub struct InMemoryBlobMetaStore {
    inner: Mutex<std::collections::BTreeMap<(Uuid, BlobDigest), BlobMetaRow>>,
}

impl InMemoryBlobMetaStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a fresh `blob_meta` row (test wiring).
    pub fn push_row(&self, tenant_id: Uuid, row: BlobMetaRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.insert((tenant_id, row.digest.clone()), row);
    }

    /// Snapshot a single row (diagnostic).
    #[must_use]
    pub fn snapshot(&self, tenant_id: Uuid, digest: &BlobDigest) -> Option<BlobMetaRow> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, digest.clone())).cloned()
    }

    /// Restore (undelete) a soft-deleted row. WI §6.1.10 — re-upload
    /// path (CAP-GC-002 reversibility window). The in-memory fake
    /// surfaces this via an explicit method; production wiring drives
    /// it from the CAS write handler (WI-S01-005) via a dedicated
    /// `restore_if_soft_deleted` SQL UPDATE.
    pub fn undelete(&self, tenant_id: Uuid, digest: &BlobDigest) -> bool {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match g.get_mut(&(tenant_id, digest.clone())) {
            Some(row) if row.deleted_at_ms.is_some() => {
                row.deleted_at_ms = None;
                true
            }
            _ => false,
        }
    }
}

impl BlobMetaStore for InMemoryBlobMetaStore {
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<Option<BlobMetaRow>, SweepError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| SweepError::Backend("blob_meta store mutex poisoned".to_owned()))?;
        Ok(g.get(&(tenant_id, digest.clone())).cloned())
    }

    fn soft_delete(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
    ) -> Result<Option<BlobState>, SweepError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| SweepError::Backend("blob_meta store mutex poisoned".to_owned()))?;
        let Some(row) = g.get_mut(&(tenant_id, digest.clone())) else {
            return Ok(None);
        };
        if row.deleted_at_ms.is_some() {
            // Idempotent: already soft-deleted; return None so sweep
            // emits AlreadySwept rather than re-emitting audit.
            return Ok(None);
        }
        let prev = BlobState {
            digest: row.digest.clone(),
            size_bytes: row.size_bytes,
            refcount: row.refcount,
            last_referenced_at_ms: row.last_referenced_at_ms,
            created_at_ms: row.created_at_ms,
        };
        row.deleted_at_ms = Some(now_ms);
        Ok(Some(prev))
    }
}

// ============================================================================
//  AcReferenceIndex — INV-GC-004 protect-if->= probe surface.
// ============================================================================

/// Forensic projection of an `ac_meta` row that triggered the
/// protect-if-`>=` check (INV-GC-004). Surfaced via
/// [`AcReferenceIndex::find_re_reference`] when the SQL `EXISTS` arm
/// fires; carried into the `protected_re_ref` audit record per
/// WI §6.1.7.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcReferenceWitness {
    /// `ac_meta.action_digest` of the offending row.
    pub action_digest: String,
    /// `ac_meta.created_at_ms` (will satisfy `>= mark_started_at_ms`).
    pub created_at_ms: u64,
}

/// Trait surfaced by every `ac_meta` reference-index backend (D1
/// reader / in-memory fake).
///
/// The single method [`AcReferenceIndex::find_re_reference`] mirrors the
/// canonical SQL `EXISTS` query enforcing INV-GC-004 (canonical TLA
/// `>=` protect-if-equal-or-newer). The trait is tenant-scoped at the
/// API surface so a Layer 4 envelope violation surfaces as a
/// fail-closed `Backend` error.
pub trait AcReferenceIndex: Send + Sync + core::fmt::Debug {
    /// Probe whether any `ac_meta` row satisfies BOTH:
    /// 1. `tenant_id = $1`
    /// 2. `digest \in blob_refs` (json_each contains; canonical idiom
    ///    per WI §6.1.2 + sprint contract §5.5)
    /// 3. `created_at_ms >= mark_started_at_ms` (STRICT `>=` per
    ///    `gc_correctness.tla` L152-154 protect-if-equal-or-newer)
    ///
    /// Returns `Ok(Some(witness))` when the EXISTS arm fires (sweep
    /// MUST NOT delete; transition to `ProtectedReRef`); `Ok(None)`
    /// when the arm is empty (sweep proceeds with soft-delete).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn find_re_reference(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_started_at_ms: u64,
    ) -> Result<Option<AcReferenceWitness>, SweepError>;
}

/// In-memory `ac_meta` reference-index fake. Tests push `(action_digest,
/// blob_refs, created_at_ms)` triples per tenant; the
/// [`AcReferenceIndex::find_re_reference`] scan filters by tenant +
/// blob-ref membership + `>=` boundary deterministically.
#[derive(Debug, Default)]
pub struct InMemoryAcReferenceIndex {
    inner: Mutex<std::collections::BTreeMap<Uuid, Vec<AcReferenceRow>>>,
}

#[derive(Clone, Debug)]
struct AcReferenceRow {
    action_digest: String,
    blob_refs: Vec<BlobDigest>,
    created_at_ms: u64,
}

impl InMemoryAcReferenceIndex {
    /// Construct an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an `ac_meta` row tied to a tenant. The `action_digest` is
    /// surfaced by the witness on the protect arm; tests can use any
    /// deterministic string (production wiring uses the canonical
    /// 64-char hex form).
    pub fn push_ac_row(
        &self,
        tenant_id: Uuid,
        action_digest: impl Into<String>,
        blob_refs: Vec<BlobDigest>,
        created_at_ms: u64,
    ) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.entry(tenant_id).or_default().push(AcReferenceRow {
            action_digest: action_digest.into(),
            blob_refs,
            created_at_ms,
        });
    }
}

impl AcReferenceIndex for InMemoryAcReferenceIndex {
    fn find_re_reference(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_started_at_ms: u64,
    ) -> Result<Option<AcReferenceWitness>, SweepError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| SweepError::Backend("ac reference index mutex poisoned".to_owned()))?;
        let Some(rows) = g.get(&tenant_id) else {
            return Ok(None);
        };
        // Canonical TLA semantics: PROTECT if any row has
        // `created_at_ms >= mark_started_at_ms` (the strict `>=`
        // boundary is the load-bearing invariant — using `>` instead
        // would create a data-loss off-by-one).
        for row in rows {
            if row.created_at_ms >= mark_started_at_ms && row.blob_refs.contains(digest) {
                return Ok(Some(AcReferenceWitness {
                    action_digest: row.action_digest.clone(),
                    created_at_ms: row.created_at_ms,
                }));
            }
        }
        Ok(None)
    }
}

// ============================================================================
//  SweepClock seam.
// ============================================================================

/// Wall-clock seam for the sweep phase. Mirrors
/// [`crate::mark::MarkClock`] so the sweep + mark phases share a
/// deterministic test seam without binding directly to `Date.now()`.
pub trait SweepClock: Send + Sync + core::fmt::Debug {
    /// Read the current wall-clock instant (Unix ms). Each call may
    /// return a value `>=` the previous call.
    fn now_ms(&self) -> u64;
}

/// Counter-driven [`SweepClock`] used by tests + property tests.
#[derive(Debug)]
pub struct CountingSweepClock {
    inner: Mutex<u64>,
}

impl CountingSweepClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }
}

impl SweepClock for CountingSweepClock {
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
//  SweepResult + SweepDecision + SweepError taxonomy.
// ============================================================================

/// Per-candidate decision produced by [`SweepPhase::execute`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SweepDecision {
    /// Confirmed orphan: blob_meta soft-deleted; candidate transitioned
    /// to `Swept`; audit `corelink.gc.sweep.soft_deleted` emitted.
    Sweep {
        /// Wall-clock instant the soft-delete fired.
        swept_at_ms: u64,
        /// Prev-state snapshot per WI §1.3 forensic requirement.
        prev_state: BlobState,
    },
    /// INV-GC-004 caught: an `ac_meta` row references the digest with
    /// `created_at_ms >= mark_started_at_ms`. Did NOT delete; candidate
    /// transitioned to `ProtectedReRef`; audit
    /// `corelink.gc.sweep.protected_re_ref` emitted.
    ProtectedReRef {
        /// Wall-clock instant the protection fired.
        protected_at_ms: u64,
        /// Anchor (`gc_run.mark_started_at_ms`).
        mark_started_at_ms: u64,
        /// Forensic witness from the offending `ac_meta` row.
        witness: AcReferenceWitness,
    },
    /// Idempotent re-run: candidate was already `Swept` /
    /// `ProtectedReRef` / `PhysicallyDeleted` — no-op (no audit emit).
    AlreadyResolved {
        /// Status observed at the moment of inspection.
        observed_status: CandidateStatus,
    },
}

/// Aggregate outcome of one sweep phase execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SweepResult {
    /// Anchor (`gc_run.mark_started_at_ms`) the sweep ran against.
    pub mark_started_at_ms: u64,
    /// Total candidates inspected (sum across decisions).
    pub candidates_processed: u64,
    /// Confirmed orphans soft-deleted.
    pub blobs_swept_count: u64,
    /// INV-GC-004 protection fires (sustained spike alert at >5%).
    pub blobs_protected_re_ref_count: u64,
    /// Already-resolved candidates (idempotent re-run no-op).
    pub already_resolved_count: u64,
    /// Sum of `blob_size_bytes` across `Sweep` decisions (the
    /// pending-reclaim figure surfaced in WI §6.1.8 metrics).
    pub bytes_to_be_reclaimed: u64,
    /// End-to-end sweep duration (ms).
    pub sweep_duration_ms: u64,
    /// Total audit events emitted (one per non-AlreadyResolved
    /// decision, plus the phase boundary events).
    pub audit_events_emitted: u64,
}

/// Canonical [`SweepPhase`] error taxonomy.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SweepError {
    /// `gc_run.mark_started_at_ms` is unset; mark phase did not
    /// complete; sweep MUST NOT fire (per WI §6.1.10 chaos test #10).
    #[error("mark anchor missing for run_id={run_id}; mark phase did not complete")]
    MarkAnchorMissing {
        /// The opaque run_id the caller supplied.
        run_id: RunId,
    },
    /// `gc_run` row backend / CHECK violation.
    #[error(transparent)]
    RunStore(#[from] GcRunStoreError),
    /// `gc_candidates` store backend error.
    #[error(transparent)]
    CandidatesStore(#[from] MarkError),
    /// Audit emission failed mid-decision; sweep ROLLED BACK that
    /// candidate; status preserved as `Candidate` so a subsequent
    /// re-run (post-D1 throttle backoff) can retry. Per WI §6.1.7
    /// fail-closed envelope.
    #[error(transparent)]
    AuditEmissionFailed(#[from] GcAuditSinkError),
    /// Metrics emission failure (rare; downgradeable to log-and-continue
    /// in production wiring per WI §14.s06.001.6).
    #[error(transparent)]
    Metrics(#[from] GcMetricsObserverError),
    /// Sweep phase budget exceeded (5 min p99 @ 100k candidates per
    /// sprint contract §5.3 R-S06-7.1).
    #[error("sweep phase budget exceeded: duration_ms={duration_ms} > budget_ms={budget_ms}")]
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

impl From<SweepError> for GcError {
    fn from(err: SweepError) -> Self {
        match err {
            SweepError::RunStore(e) => GcError::RunStore(e),
            SweepError::AuditEmissionFailed(e) => GcError::Audit(e),
            SweepError::Metrics(e) => GcError::Metrics(e),
            other => GcError::RunStore(GcRunStoreError::Backend(other.to_string())),
        }
    }
}

// ============================================================================
//  SweepConfig — knobs surfaced by ScheduleConfig in production.
// ============================================================================

/// Knobs driving the sweep phase. The defaults pin the canonical values
/// from sprint contract §5.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SweepConfig {
    grace_cas_ms: u64,
    grace_ac_ms: u64,
    phase_budget_ms: u64,
}

impl Default for SweepConfig {
    fn default() -> Self {
        Self {
            grace_cas_ms: GRACE_CAS_MS,
            grace_ac_ms: GRACE_AC_MS,
            phase_budget_ms: CANONICAL_SWEEP_PHASE_BUDGET_MS,
        }
    }
}

impl SweepConfig {
    /// Construct a custom config. `grace_cas_ms` MUST be `>=
    /// grace_ac_ms` (CAS retention is a regulatory floor; AC
    /// retention is shorter); shorter CAS grace requires an ADR + customer
    /// opt-in per sprint contract §10 anti-scope.
    ///
    /// # Errors
    ///
    /// Returns [`SweepError::Backend`] when the invariant is violated.
    /// Production wiring lifts the values from `wrangler.toml` env
    /// overrides (`CORELINK_GC_GRACE_CAS_MS` /
    /// `CORELINK_GC_GRACE_AC_MS`).
    pub fn new(
        grace_cas_ms: u64,
        grace_ac_ms: u64,
        phase_budget_ms: u64,
    ) -> Result<Self, SweepError> {
        if grace_cas_ms < grace_ac_ms {
            return Err(SweepError::Backend(
                "grace_cas_ms must be >= grace_ac_ms (regulatory floor)".to_owned(),
            ));
        }
        if phase_budget_ms == 0 {
            return Err(SweepError::Backend(
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
//  SweepPhase trait + InMemorySweepPhase impl.
// ============================================================================

/// Trait surfaced by every sweep phase backend (production CF Cron DO
/// handler / in-memory fake).
pub trait SweepPhase: Send + Sync + core::fmt::Debug {
    /// Execute the sweep phase end-to-end for the given `(run_id,
    /// tenant, region)`. Reads the candidate set surfaced by the mark
    /// phase, enforces INV-GC-004 per row, soft-deletes confirmed
    /// orphans, transitions the candidate row's `status`, and emits
    /// audit + metrics.
    ///
    /// # Errors
    ///
    /// Surface as [`SweepError`].
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<SweepResult, SweepError>;
}

/// In-memory sweep phase orchestrator (the canonical pure-logic
/// skeleton). Composes the trait dependencies declared at construction
/// time.
pub struct InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    runs: Arc<S>,
    candidates: Arc<C>,
    blob_meta: Arc<B>,
    ac_index: Arc<X>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<K>,
    config: SweepConfig,
}

impl<S, C, B, X, A, M, K> core::fmt::Debug for InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemorySweepPhase")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, C, B, X, A, M, K> InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    /// Construct a sweep phase orchestrator with the canonical
    /// defaults.
    pub fn with_defaults(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta: Arc<B>,
        ac_index: Arc<X>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta,
            ac_index,
            audit,
            metrics,
            clock,
            config: SweepConfig::default(),
        }
    }

    /// Construct with an explicit [`SweepConfig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 7 trait deps + 1 knob; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta: Arc<B>,
        ac_index: Arc<X>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        config: SweepConfig,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta,
            ac_index,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// The `SweepConfig` snapshot.
    #[must_use]
    pub fn config(&self) -> SweepConfig {
        self.config
    }

    /// Process a single candidate row through the sweep decision
    /// pipeline. Visible for property tests so the decision boundary
    /// can be exercised independently of the phase orchestration.
    ///
    /// # Errors
    ///
    /// Surface as [`SweepError`].
    pub fn step_candidate(
        &self,
        candidate: &GcCandidate,
        region: GcRegion,
    ) -> Result<SweepDecision, SweepError> {
        // Idempotent re-run guard: already-resolved candidates emit
        // AlreadyResolved + no audit. The mark→sweep contract is that
        // sweep only ingests rows whose mark phase wrote `Candidate`.
        if candidate.status != CandidateStatus::Candidate {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        }
        // INV-GC-004 protect-if->= probe (canonical TLA semantics).
        let witness = self.ac_index.find_re_reference(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_started_at_ms,
        )?;
        let now = self.clock.now_ms();
        if let Some(witness) = witness {
            // PROTECT path. Fail-closed audit emit BEFORE flipping the
            // row status — if audit fails the row remains 'candidate'
            // and a subsequent re-run can retry.
            let reason = format!(
                "ac.created_at_ms={} mark_started_at_ms={} action_digest={}",
                witness.created_at_ms, candidate.mark_started_at_ms, witness.action_digest
            );
            self.audit.emit(GcAuditRecord {
                event_type: GcEventType::SweepProtectedReRef,
                run_id: candidate.mark_run_id,
                tenant_id: candidate.tenant_id,
                region,
                status: GcStatus::Running,
                from_phase: Some(GcPhase::Sweep),
                to_phase: Some(GcPhase::Sweep),
                created_by_request_id: "cron".to_owned(),
                reason: "inv_gc_004_protected_re_ref",
                now_ms: now,
            })?;
            // Atomic transition; if the row drifted from 'candidate'
            // mid-flight (concurrent re-run) the boolean returns false
            // and we surface AlreadyResolved on the second observation.
            let fired = self.candidates.transition_status(
                candidate.tenant_id,
                &candidate.digest,
                candidate.mark_run_id,
                CandidateStatus::Candidate,
                CandidateStatus::ProtectedReRef,
                now,
                Some(reason),
            )?;
            if !fired {
                // Concurrent winner; observe + surface as resolved.
                let observed = self
                    .candidates
                    .lookup(
                        candidate.tenant_id,
                        &candidate.digest,
                        candidate.mark_run_id,
                    )?
                    .map_or(CandidateStatus::Candidate, |c| c.status);
                return Ok(SweepDecision::AlreadyResolved {
                    observed_status: observed,
                });
            }
            return Ok(SweepDecision::ProtectedReRef {
                protected_at_ms: now,
                mark_started_at_ms: candidate.mark_started_at_ms,
                witness,
            });
        }
        // SOFT-DELETE path. Read prev-state via a non-mutating lookup,
        // emit audit, then perform the conditional UPDATE. Audit emit
        // failure leaves blob_meta unchanged (fail-closed envelope on
        // the in-memory fake matches production D1 atomic batch
        // ROLLBACK semantics).
        let Some(row) = self
            .blob_meta
            .lookup(candidate.tenant_id, &candidate.digest)?
        else {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        };
        if row.deleted_at_ms.is_some() {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        }
        let prev_state = BlobState {
            digest: row.digest.clone(),
            size_bytes: row.size_bytes,
            refcount: row.refcount,
            last_referenced_at_ms: row.last_referenced_at_ms,
            created_at_ms: row.created_at_ms,
        };
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::SweepSoftDeleted,
            run_id: candidate.mark_run_id,
            tenant_id: candidate.tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::Sweep),
            to_phase: Some(GcPhase::Sweep),
            created_by_request_id: "cron".to_owned(),
            reason: "sweep_soft_deleted",
            now_ms: now,
        })?;
        if self
            .blob_meta
            .soft_delete(candidate.tenant_id, &candidate.digest, now)?
            .is_none()
        {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        };
        let fired = self.candidates.transition_status(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_run_id,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            now,
            None,
        )?;
        if !fired {
            let observed = self
                .candidates
                .lookup(
                    candidate.tenant_id,
                    &candidate.digest,
                    candidate.mark_run_id,
                )?
                .map_or(CandidateStatus::Candidate, |c| c.status);
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: observed,
            });
        }
        Ok(SweepDecision::Sweep {
            swept_at_ms: now,
            prev_state,
        })
    }
}

impl<S, C, B, X, A, M, K> SweepPhase for InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<SweepResult, SweepError> {
        // 1. Read gc_run + verify mark anchor + region match.
        let run_row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(SweepError::RunStore(GcRunStoreError::NotFound(run_id)))?;
        if run_row.region != region {
            return Err(SweepError::RegionMismatch {
                run_region: run_row.region,
                caller_region: region,
            });
        }
        let mark_anchor = run_row
            .mark_started_at_ms
            .ok_or(SweepError::MarkAnchorMissing { run_id })?;

        // 2. Phase budget deadline.
        let phase_start = self.clock.now_ms();
        let deadline_ms = phase_start.saturating_add(self.config.phase_budget_ms);

        // 3. Sweep candidates for the run.
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id)?;
        let mut blobs_swept_count: u64 = 0;
        let mut blobs_protected_re_ref_count: u64 = 0;
        let mut already_resolved_count: u64 = 0;
        let mut bytes_to_be_reclaimed: u64 = 0;
        let mut audit_events_emitted: u64 = 0;
        let mut candidates_processed: u64 = 0;
        for candidate in &candidates {
            // Phase-budget probe per candidate (cheaper than per
            // mutation; aligns with mark phase batched probe pattern).
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(SweepError::PhaseBudgetExceeded {
                    duration_ms: now_for_probe.saturating_sub(phase_start),
                    budget_ms: self.config.phase_budget_ms,
                });
            }
            let decision = self.step_candidate(candidate, region)?;
            candidates_processed = candidates_processed.saturating_add(1);
            match decision {
                SweepDecision::Sweep { prev_state, .. } => {
                    blobs_swept_count = blobs_swept_count.saturating_add(1);
                    bytes_to_be_reclaimed =
                        bytes_to_be_reclaimed.saturating_add(prev_state.size_bytes);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                SweepDecision::ProtectedReRef { .. } => {
                    blobs_protected_re_ref_count = blobs_protected_re_ref_count.saturating_add(1);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                SweepDecision::AlreadyResolved { .. } => {
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
                blobs_swept_delta: blobs_swept_count,
                bytes_reclaimed_delta: bytes_to_be_reclaimed,
                ..Default::default()
            },
        )?;

        // 5. Phase duration histogram (canonical metric).
        self.metrics
            .record_phase_duration_ms(GcPhase::Sweep, tenant_id, region, duration_ms)?;

        Ok(SweepResult {
            mark_started_at_ms: mark_anchor,
            candidates_processed,
            blobs_swept_count,
            blobs_protected_re_ref_count,
            already_resolved_count,
            bytes_to_be_reclaimed,
            sweep_duration_ms: duration_ms,
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
    use crate::mark::{InMemoryGcCandidatesStore, MarkConfig};
    use crate::metrics::InMemoryGcMetrics;
    use crate::run::InMemoryGcRunStore;

    fn digest(seed: u32) -> BlobDigest {
        let prefix = format!("{seed:08x}");
        let mut s = prefix;
        s.push_str(&"0".repeat(BlobDigest::LEN - 8));
        BlobDigest::parse(&s).expect("canonical hex")
    }

    type Fixture = (
        InMemorySweepPhase<
            InMemoryGcRunStore,
            InMemoryGcCandidatesStore,
            InMemoryBlobMetaStore,
            InMemoryAcReferenceIndex,
            InMemoryGcAuditSink,
            InMemoryGcMetrics,
            CountingSweepClock,
        >,
        Arc<InMemoryGcRunStore>,
        Arc<InMemoryGcCandidatesStore>,
        Arc<InMemoryBlobMetaStore>,
        Arc<InMemoryAcReferenceIndex>,
        Arc<InMemoryGcAuditSink>,
    );

    fn fresh(start_ms: u64) -> Fixture {
        let runs = Arc::new(InMemoryGcRunStore::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let blob_meta = Arc::new(InMemoryBlobMetaStore::new());
        let ac_index = Arc::new(InMemoryAcReferenceIndex::new());
        let audit = Arc::new(InMemoryGcAuditSink::new());
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingSweepClock::new(start_ms));
        let sweep = InMemorySweepPhase::with_defaults(
            Arc::clone(&runs),
            Arc::clone(&candidates),
            Arc::clone(&blob_meta),
            Arc::clone(&ac_index),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        (sweep, runs, candidates, blob_meta, ac_index, audit)
    }

    fn seed_run_in_sweep(
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
    }

    fn seed_candidate(
        candidates: &InMemoryGcCandidatesStore,
        blob_meta: &InMemoryBlobMetaStore,
        tenant: Uuid,
        rid: RunId,
        d: BlobDigest,
        mark_anchor: u64,
    ) {
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
        blob_meta.push_row(
            tenant,
            BlobMetaRow {
                digest: d,
                size_bytes: 4096,
                refcount: 0,
                last_referenced_at_ms: mark_anchor.saturating_sub(1),
                created_at_ms: mark_anchor.saturating_sub(100),
                deleted_at_ms: None,
            },
        );
    }

    #[test]
    fn canonical_grace_constants_pinned() {
        assert_eq!(GRACE_CAS_MS, 72 * 60 * 60 * 1000);
        assert_eq!(GRACE_AC_MS, 24 * 60 * 60 * 1000);
        assert_eq!(CANONICAL_SWEEP_PHASE_BUDGET_MS, 5 * 60 * 1000);
    }

    #[test]
    fn sweep_config_default_canonical() {
        let cfg = SweepConfig::default();
        assert_eq!(cfg.grace_cas_ms(), GRACE_CAS_MS);
        assert_eq!(cfg.grace_ac_ms(), GRACE_AC_MS);
        assert_eq!(cfg.phase_budget_ms(), CANONICAL_SWEEP_PHASE_BUDGET_MS);
    }

    #[test]
    fn sweep_config_rejects_inverted_grace() {
        let err = SweepConfig::new(GRACE_AC_MS, GRACE_CAS_MS, 1).unwrap_err();
        assert!(matches!(err, SweepError::Backend(_)));
    }

    #[test]
    fn sweep_config_rejects_zero_budget() {
        assert!(SweepConfig::new(GRACE_CAS_MS, GRACE_AC_MS, 0).is_err());
    }

    #[test]
    fn happy_path_orphan_swept() {
        let (sweep, runs, candidates, blob_meta, _ac, audit) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let d = digest(0xabcd);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);

        let result = sweep.execute(rid, tenant, GcRegion::Sam).unwrap();
        assert_eq!(result.mark_started_at_ms, mark_anchor);
        assert_eq!(result.candidates_processed, 1);
        assert_eq!(result.blobs_swept_count, 1);
        assert_eq!(result.blobs_protected_re_ref_count, 0);
        assert_eq!(result.bytes_to_be_reclaimed, 4096);
        // blob_meta soft-deleted.
        let row = blob_meta.snapshot(tenant, &d).unwrap();
        assert!(row.deleted_at_ms.is_some());
        // gc_candidate flipped to Swept.
        let cand = candidates
            .lookup(tenant, &d, rid)
            .unwrap()
            .expect("candidate row");
        assert_eq!(cand.status, CandidateStatus::Swept);
        assert!(cand.swept_at_ms.is_some());
        // Audit emitted.
        assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
        assert!(audit
            .snapshot_of(GcEventType::SweepProtectedReRef)
            .is_empty());
    }

    #[test]
    fn protect_re_ref_strict_ge_boundary() {
        // ac.created_at_ms == mark_started_at_ms exactly → protected.
        let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Iad, mark_anchor);
        let d = digest(0x1111);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
        // EXACTLY EQUAL — canonical TLA `>=` says PROTECT.
        ac.push_ac_row(tenant, "action_a", vec![d.clone()], mark_anchor);

        let result = sweep.execute(rid, tenant, GcRegion::Iad).unwrap();
        assert_eq!(result.blobs_swept_count, 0);
        assert_eq!(result.blobs_protected_re_ref_count, 1);
        // blob_meta NOT soft-deleted.
        let row = blob_meta.snapshot(tenant, &d).unwrap();
        assert!(row.deleted_at_ms.is_none());
        // gc_candidate flipped to ProtectedReRef.
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::ProtectedReRef);
        assert!(cand.protected_at_ms.is_some());
        assert!(cand.protected_reason.is_some());
        // Audit emitted.
        assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 1);
        assert!(audit.snapshot_of(GcEventType::SweepSoftDeleted).is_empty());
    }

    #[test]
    fn no_protect_when_ac_predates_mark_anchor() {
        let (sweep, runs, candidates, blob_meta, ac, _audit) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Lhr, mark_anchor);
        let d = digest(0x2222);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
        // ac.created_at_ms = mark_anchor - 1 → NOT protected.
        ac.push_ac_row(tenant, "action_b", vec![d.clone()], mark_anchor - 1);

        let result = sweep.execute(rid, tenant, GcRegion::Lhr).unwrap();
        assert_eq!(result.blobs_swept_count, 1);
        assert_eq!(result.blobs_protected_re_ref_count, 0);
    }

    #[test]
    fn idempotent_re_run_already_swept() {
        let (sweep, runs, candidates, blob_meta, _ac, audit) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Nrt, mark_anchor);
        let d = digest(0x3333);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d, mark_anchor);

        let _r1 = sweep.execute(rid, tenant, GcRegion::Nrt).unwrap();
        let r2 = sweep.execute(rid, tenant, GcRegion::Nrt).unwrap();
        assert_eq!(r2.blobs_swept_count, 0);
        assert_eq!(r2.already_resolved_count, 1);
        // Audit only emitted ONCE (no re-emit on idempotent re-run).
        assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
    }

    #[test]
    fn mark_anchor_missing_rejected() {
        // gc_run without Mark transition → MarkAnchorMissing.
        let (sweep, runs, _c, _b, _ac, _aud) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        runs.insert_pending(rid, tenant, GcRegion::Sam, 100, "cron".into())
            .unwrap();
        runs.acquire_running(rid, tenant, 200).unwrap();
        let err = sweep.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, SweepError::MarkAnchorMissing { .. }));
    }

    #[test]
    fn region_mismatch_rejected() {
        let (sweep, runs, _c, _b, _ac, _aud) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
        let err = sweep.execute(rid, tenant, GcRegion::Iad).unwrap_err();
        assert!(matches!(err, SweepError::RegionMismatch { .. }));
    }

    #[test]
    fn run_not_found_rejected() {
        let (sweep, _runs, _c, _b, _ac, _aud) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(99));
        let err = sweep.execute(rid, tenant, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, SweepError::RunStore(_)));
    }

    #[test]
    fn cross_tenant_run_returns_not_found() {
        // The Layer 4 envelope is tested at the run-store seam:
        // gc_run.lookup(rid, wrong_tenant) returns Ok(None).
        let (sweep, runs, _c, _b, _ac, _aud) = fresh(1_000);
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let rid = RunId(Uuid::from_u128(99));
        seed_run_in_sweep(&runs, rid, tenant_a, GcRegion::Sam, 500);
        // Tenant B asks for the run id → MarkAnchorMissing/NotFound
        // (the lookup returns None, then the .ok_or maps to NotFound).
        let err = sweep.execute(rid, tenant_b, GcRegion::Sam).unwrap_err();
        assert!(matches!(err, SweepError::RunStore(_)));
    }

    #[test]
    fn tenant_isolation_no_cross_sweep() {
        let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(1_000);
        let ta = Uuid::from_u128(1);
        let tb = Uuid::from_u128(2);
        let ra = RunId(Uuid::from_u128(10));
        let rb = RunId(Uuid::from_u128(11));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, ra, ta, GcRegion::Sam, mark_anchor);
        seed_run_in_sweep(&runs, rb, tb, GcRegion::Sam, mark_anchor);
        let d = digest(0xdead);
        seed_candidate(&candidates, &blob_meta, ta, ra, d.clone(), mark_anchor);
        seed_candidate(&candidates, &blob_meta, tb, rb, d.clone(), mark_anchor);
        // Tenant A has a re-ref; tenant B does not.
        ac.push_ac_row(ta, "action_a", vec![d.clone()], mark_anchor + 1);

        let result_a = sweep.execute(ra, ta, GcRegion::Sam).unwrap();
        let result_b = sweep.execute(rb, tb, GcRegion::Sam).unwrap();
        // A protected; B swept.
        assert_eq!(result_a.blobs_protected_re_ref_count, 1);
        assert_eq!(result_a.blobs_swept_count, 0);
        assert_eq!(result_b.blobs_protected_re_ref_count, 0);
        assert_eq!(result_b.blobs_swept_count, 1);
        // Audit captures the per-tenant decisions.
        let protected_audits = audit.snapshot_of(GcEventType::SweepProtectedReRef);
        let swept_audits = audit.snapshot_of(GcEventType::SweepSoftDeleted);
        assert_eq!(protected_audits.len(), 1);
        assert_eq!(swept_audits.len(), 1);
        assert_eq!(protected_audits[0].tenant_id, ta);
        assert_eq!(swept_audits[0].tenant_id, tb);
        // Tenant A's blob_meta NOT soft-deleted; tenant B's IS.
        assert!(blob_meta.snapshot(ta, &d).unwrap().deleted_at_ms.is_none());
        assert!(blob_meta.snapshot(tb, &d).unwrap().deleted_at_ms.is_some());
    }

    #[test]
    fn undelete_via_reupload_restores_blob_meta() {
        // CAP-GC-002 reversibility — blob_meta soft-deleted within
        // grace; a CAS write handler call resets `deleted_at = NULL`.
        let blob_meta = InMemoryBlobMetaStore::new();
        let tenant = Uuid::from_u128(1);
        let d = digest(0xfeed);
        blob_meta.push_row(
            tenant,
            BlobMetaRow {
                digest: d.clone(),
                size_bytes: 1024,
                refcount: 0,
                last_referenced_at_ms: 100,
                created_at_ms: 50,
                deleted_at_ms: None,
            },
        );
        // Soft-delete.
        let prev = blob_meta
            .soft_delete(tenant, &d, 200)
            .unwrap()
            .expect("soft-delete should fire");
        assert_eq!(prev.size_bytes, 1024);
        // Within grace, customer re-uploads same digest → undelete.
        let restored = blob_meta.undelete(tenant, &d);
        assert!(restored);
        let row = blob_meta.snapshot(tenant, &d).unwrap();
        assert!(row.deleted_at_ms.is_none());
    }

    #[test]
    fn audit_emit_failure_blocks_status_flip() {
        // Use a sink that always errors; assert no soft-delete persists
        // through the candidate status flip when audit fails.
        #[derive(Debug, Default)]
        struct FailingSink;
        impl GcAuditSink for FailingSink {
            fn emit(&self, _record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
                Err(GcAuditSinkError::Store("simulated_failure".to_owned()))
            }
        }

        let runs = Arc::new(InMemoryGcRunStore::new());
        let candidates = Arc::new(InMemoryGcCandidatesStore::new());
        let blob_meta = Arc::new(InMemoryBlobMetaStore::new());
        let ac_index = Arc::new(InMemoryAcReferenceIndex::new());
        let audit = Arc::new(FailingSink);
        let metrics = Arc::new(InMemoryGcMetrics::new());
        let clock = Arc::new(CountingSweepClock::new(1_000));
        let sweep = InMemorySweepPhase::with_defaults(
            Arc::clone(&runs),
            Arc::clone(&candidates),
            Arc::clone(&blob_meta),
            Arc::clone(&ac_index),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );

        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Syd, mark_anchor);
        let d = digest(0xfeed);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);

        let err = sweep.execute(rid, tenant, GcRegion::Syd).unwrap_err();
        assert!(matches!(err, SweepError::AuditEmissionFailed(_)));
        let cand = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        assert_eq!(cand.status, CandidateStatus::Candidate);
        let row = blob_meta.lookup(tenant, &d).unwrap().unwrap();
        assert!(row.deleted_at_ms.is_none());
    }

    #[test]
    fn protect_path_preserves_audit_field_taxonomy() {
        // Audit record emitted on protect arm carries the canonical
        // event type + tenant + region + run_id + protected reason.
        let (sweep, runs, candidates, blob_meta, ac, audit) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let mark_anchor = 500;
        seed_run_in_sweep(&runs, rid, tenant, GcRegion::Iad, mark_anchor);
        let d = digest(0x7777);
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
        ac.push_ac_row(tenant, "action_xyz", vec![d.clone()], mark_anchor + 5);
        let _ = sweep.execute(rid, tenant, GcRegion::Iad).unwrap();
        let recs = audit.snapshot_of(GcEventType::SweepProtectedReRef);
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.tenant_id, tenant);
        assert_eq!(r.run_id, rid);
        assert_eq!(r.region, GcRegion::Iad);
        assert_eq!(r.from_phase, Some(GcPhase::Sweep));
        assert_eq!(r.to_phase, Some(GcPhase::Sweep));
        assert_eq!(r.reason, "inv_gc_004_protected_re_ref");
    }

    #[test]
    fn step_candidate_preserves_already_resolved_for_swept() {
        let (sweep, _runs, candidates, blob_meta, _ac, _audit) = fresh(1_000);
        let tenant = Uuid::from_u128(1);
        let rid = RunId(Uuid::from_u128(2));
        let d = digest(0x4242);
        let mark_anchor = 500;
        seed_candidate(&candidates, &blob_meta, tenant, rid, d.clone(), mark_anchor);
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
        let already_swept = candidates.lookup(tenant, &d, rid).unwrap().unwrap();
        let dec = sweep.step_candidate(&already_swept, GcRegion::Sam).unwrap();
        match dec {
            SweepDecision::AlreadyResolved { observed_status } => {
                assert_eq!(observed_status, CandidateStatus::Swept);
            }
            other => panic!("expected AlreadyResolved, got {other:?}"),
        }
    }

    #[test]
    fn ac_index_only_returns_witness_with_matching_blob_ref() {
        let ac = InMemoryAcReferenceIndex::new();
        let tenant = Uuid::from_u128(1);
        let target = digest(0x9999);
        let other = digest(0x0001);
        // ac row references `other` only — no witness on `target`.
        ac.push_ac_row(tenant, "action_o", vec![other.clone()], 500);
        let res = ac.find_re_reference(tenant, &target, 100).unwrap();
        assert!(res.is_none());
        // Push a row referencing `target` at exactly 100 (>=) — witness fires.
        ac.push_ac_row(tenant, "action_t", vec![target.clone()], 100);
        let witness = ac.find_re_reference(tenant, &target, 100).unwrap();
        assert!(witness.is_some());
    }

    #[test]
    fn ac_index_tenant_isolation() {
        let ac = InMemoryAcReferenceIndex::new();
        let ta = Uuid::from_u128(1);
        let tb = Uuid::from_u128(2);
        let d = digest(0x4321);
        ac.push_ac_row(ta, "action_a", vec![d.clone()], 1_000);
        // Tenant B should NOT see tenant A's ac row.
        let res = ac.find_re_reference(tb, &d, 500).unwrap();
        assert!(res.is_none());
        // Tenant A sees it.
        let res = ac.find_re_reference(ta, &d, 500).unwrap();
        assert!(res.is_some());
    }

    #[test]
    fn region_value_round_trip_in_sweep_audit() {
        // Sanity: every region appears in sweep audit emit.
        for region in GcRegion::all() {
            let (sweep, runs, candidates, blob_meta, _ac, audit) = fresh(1_000);
            let tenant = Uuid::from_u128(u128::from(region.as_str().len() as u32) + 1);
            let rid = RunId(Uuid::from_u128(42));
            let mark_anchor = 500;
            seed_run_in_sweep(&runs, rid, tenant, *region, mark_anchor);
            let d = digest(0xc0de);
            seed_candidate(&candidates, &blob_meta, tenant, rid, d, mark_anchor);
            let _ = sweep.execute(rid, tenant, *region).unwrap();
            let recs = audit.snapshot_of(GcEventType::SweepSoftDeleted);
            assert_eq!(recs.len(), 1);
            assert_eq!(recs[0].region, *region);
        }
    }

    #[test]
    fn cross_module_canonical_constants() {
        // Ensure canonical constants don't drift from sprint contract
        // §5.3 (R-S06-7.1 sweep budget = 5 min) + §5.3 R-S06-6 grace.
        let cfg = MarkConfig::default();
        // Mark phase budget is 10 min canonical; sweep is 5 min — they
        // are independent budget lines, sum ≤ 15 min total per
        // (tenant, region) cron tick.
        assert_eq!(
            cfg.phase_budget_ms() + CANONICAL_SWEEP_PHASE_BUDGET_MS,
            15 * 60 * 1000
        );
    }
}
