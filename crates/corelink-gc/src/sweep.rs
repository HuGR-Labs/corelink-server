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

mod phase;
pub use phase::InMemorySweepPhase;

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
