//! Refcount reconciliation phase (WI-S06-005) — daily refcount drift
//! detection + scale-invariant auto-fix gate + manual-review escalation
//! + audit emission per decision.
//!
//! ## Where reconcile sits in the GC phase chain
//!
//! `Idle → Mark → Sweep → PhysicalDelete → **Reconcile** → Completed`
//!
//! Reconcile recomputes the *true* refcount per `(tenant, digest)`
//! from `ac_meta.blob_refs` via the canonical SQL `json_each`
//! JSON-aware membership idiom (NOT `LIKE '%digest%'` substring match
//! — Lote 10.6bis Part 2a P0-1 highest-leverage fix), compares it
//! against `blob_meta.refcount` (denormalised counter), and either
//! emits `refcount_reconciled` (no drift), `refcount_auto_fixed`
//! (drift within dual-condition gate; UPDATE fires), or
//! `refcount_manual_review_required` (drift exceeds gate; SEV-1).
//!
//! ## Cryptographic invariants enforced (per WI §1)
//!
//! 1. **Canonical `json_each` JSON-aware membership** (Lote 10.6bis
//!    Part 2a P0-1) — `j.value = digest` *exactly* (no `LIKE`
//!    substring collisions); the [`InMemoryRefcountSource`] fake
//!    mirrors this byte-for-byte.
//! 2. **Auto-fix dual-condition gate** (Lote 10.6bis Part 2a P0-6;
//!    INV-GC-RECONCILE-AUTO-FIX-BOUNDED) — `drift_count ≤ 5 AND
//!    drift_percent ≤ 0.01%` per tenant; both required. The two-floor
//!    design is scale-invariant.
//! 3. **Audit emit fail-closed** (WI §6.1.7) — emit BEFORE the
//!    `blob_meta.refcount` UPDATE; emission failure aborts the row.
//! 4. **Tenant-scoped strict** (Lote 10.4bis) — every API takes
//!    `(tenant_id, region)` first; cross-tenant surfaces as
//!    fail-closed [`ReconcileError::Backend`].
//! 5. **Orphan-R2 detection** (WI §1.4) — `blob_meta` with
//!    `refcount > 0` + R2 NotFound surfaces via the
//!    `OrphanR2Detected` arm; NOT auto-fixed (mutation would amplify).
//!
//! See [`super::sweep`] / [`super::physical_delete`] for the upstream
//! GC phases and ADR-0028 for the audit envelope contract. The
//! production wiring (D1 `json_each` aggregate; atomic batch UPDATE
//! blob_meta + INSERT audit_outbox; Cron DO alarm) lands in
//! WI-S06-007 alongside the conformance suite; this module ships the
//! pure-logic skeleton + in-memory fakes per the
//! `trait-abstraction-defer` charter pattern.
//!
//! ## File layout (Wave 33 Stream A2.2)
//!
//! The reconcile module is structured into per-concern files under
//! `reconcile/` so each file stays ≤ 500 LOC per the techlead L2.10
//! file-size discipline. The parent module file lives at
//! `reconcile.rs` (workspace lint `clippy::mod_module_files = "deny"`):
//!
//! - `reconcile.rs` (parent) — constants, traits, materialised row
//!   types, [`ReconcileDecision`] / [`ReconcileResult`] /
//!   [`ReconcileError`] / [`ReconcilePhase`] + submodule glue.
//! - `reconcile/plan.rs` — [`ReconcileConfig`] knobs + [`sev_level_for`]
//!   / [`auto_fix_gate_fires`] pure-fn boundary helpers +
//!   [`CountingReconcileClock`] test seam.
//! - `reconcile/execute.rs` — [`InMemoryReconcilePhase`] +
//!   `step_row` per-row decision pipeline + [`ReconcilePhase`] impl.
//! - `reconcile/verify.rs` — [`InMemoryRefcountSource`] +
//!   [`InMemoryBlobMetaRefcountStore`] property-test fakes.
//! - `reconcile/tests.rs` + `reconcile/tests_scenarios.rs` — unit
//!   tests split across two files (parity with pre-split test set).

use thiserror::Error;
use uuid::Uuid;

use crate::audit::GcAuditSinkError;
use crate::error::GcError;
use crate::mark::BlobDigest;
use crate::metrics::GcMetricsObserverError;
use crate::region::GcRegion;
use crate::run::{GcRunStoreError, RunId};

mod execute;
mod plan;
mod verify;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_scenarios;

pub use execute::InMemoryReconcilePhase;
pub use plan::{auto_fix_gate_fires, sev_level_for, CountingReconcileClock, ReconcileConfig};
pub use verify::{InMemoryBlobMetaRefcountStore, InMemoryRefcountSource};

/// Canonical reconcile phase budget — 1h p99 @ 1M blobs per (tenant,
/// region) daily cron tick (sprint contract §5.5 R-S06-10.1).
///
/// Re-derived post Lote 10.6bis P0-1: `json_each` per-row extracts
/// `O(json_array_size)` joined with `idx_ac_meta_tenant_deleted`;
/// chunked iteration 1k blobs/chunk × bounded concurrency 4-8.
pub const CANONICAL_RECONCILE_PHASE_BUDGET_MS: u64 = 60 * 60 * 1000;

/// Auto-fix gate (count arm; Lote 10.6bis Part 2a P0-6
/// scale-invariant). Auto-fix fires ONLY when
/// `drift_count ≤ AUTO_FIX_MAX_RECORDS` AND
/// `drift_percent ≤ AUTO_FIX_MAX_PERCENT` per tenant. Boundary `=5`
/// auto-fixes (operative bound `≤5`); `=6` rejects → manual review.
pub const AUTO_FIX_MAX_RECORDS: u64 = 5;

/// Auto-fix gate (percentage arm; Lote 10.6bis Part 2a P0-6
/// scale-invariant). Expressed as a fractional unit (0.01 % =
/// `0.0001`).
pub const AUTO_FIX_MAX_PERCENT: f64 = 0.0001;

/// Per-tenant drift threshold above which the SEV-1 alert fires (sprint
/// contract §5.5 R-S06-10; 1 % per tenant). Expressed as a fractional
/// unit (1 % = `0.01`).
pub const SEV1_PER_TENANT_DRIFT_PERCENT: f64 = 0.01;

/// Global drift threshold above which the SEV-2 alert fires (sprint
/// contract §5.5 R-S06-10; 0.1 % global). Expressed as a fractional
/// unit (0.1 % = `0.001`).
pub const SEV2_GLOBAL_DRIFT_PERCENT: f64 = 0.001;

// ============================================================================
//  RefcountSource — canonical `json_each` membership probe.
// ============================================================================

/// Trait surfaced by every refcount-source backend (production D1
/// `json_each` aggregate / in-memory fake).
///
/// The single method [`RefcountSource::expected_refcount`] mirrors the
/// canonical SQL aggregate enforcing the `json_each` JSON-aware
/// membership idiom (NOT `LIKE '%digest%'` substring match — Lote
/// 10.6bis Part 2a P0-1 fix). The trait is tenant-scoped at the API
/// surface so a Layer 4 envelope violation surfaces as a fail-closed
/// `Backend` error.
pub trait RefcountSource: Send + Sync + core::fmt::Debug {
    /// Compute `expected_refcount` for `(tenant_id, digest)` at the
    /// snapshot instant. Mirrors:
    ///
    /// ```sql
    /// SELECT COUNT(*)
    ///   FROM ac_meta a, json_each(a.blob_refs) j
    ///  WHERE a.tenant_id = $1
    ///    AND j.value = $2
    ///    AND a.deleted_at_ms IS NULL
    ///    AND a.created_at_ms < $3
    /// ```
    ///
    /// `snapshot_at_ms` is the `gc_run.reconcile_started_at_ms` instant
    /// (mirrors WI-S06-003 mark phase pattern; bounds race window to
    /// writes-before-snapshot per Lote 10.6bis P1-6 fix).
    ///
    /// # Errors
    ///
    /// Backend transport errors (D1 throttle / parse / cross-tenant
    /// injection).
    fn expected_refcount(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        snapshot_at_ms: u64,
    ) -> Result<u32, ReconcileError>;
}

/// Materialised projection of a single `ac_meta` row used by the
/// in-memory [`RefcountSource`] fake. Mirrors the SQL columns:
/// `(action_digest, blob_refs, deleted_at_ms, created_at_ms)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcMetaReconcileRow {
    /// `ac_meta.action_digest` (forensic identity; never compared to
    /// `blob_refs` membership).
    pub action_digest: String,
    /// `ac_meta.blob_refs` — the canonical JSON array. Production
    /// wiring stores this as `TEXT` containing a JSON array literal;
    /// the fake stores it as `Vec<BlobDigest>` so `json_each` semantics
    /// fall out naturally.
    pub blob_refs: Vec<BlobDigest>,
    /// `ac_meta.deleted_at_ms` — soft-delete tombstone. The canonical
    /// query filters `deleted_at_ms IS NULL` so soft-deleted rows do
    /// NOT count toward `expected_refcount`.
    pub deleted_at_ms: Option<u64>,
    /// `ac_meta.created_at_ms` — used for the snapshot bound
    /// `created_at_ms < snapshot_at_ms` (Lote 10.6bis P1-6 fix).
    pub created_at_ms: u64,
}

// ============================================================================
//  BlobMetaRefcountStore — reconcile reads + auto-fix UPDATE surface.
// ============================================================================

/// Materialised projection of a `blob_meta` row used by the reconcile
/// phase. Mirrors the SQL columns
/// `(digest, refcount, deleted_at_ms, r2_present)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobMetaReconcileRow {
    /// Canonical hex digest.
    pub digest: BlobDigest,
    /// Stored refcount (denormalised counter under reconciliation).
    pub stored_refcount: u32,
    /// Tombstone (`Some(ms)` indicates soft-deleted; reconcile does
    /// NOT auto-fix soft-deleted rows — refcount=NULL on physical
    /// delete creates a "deletion-during-reconcile" SEV-2 informational
    /// signal per WI §6.1.10 chaos test #12).
    pub deleted_at_ms: Option<u64>,
    /// R2 object presence at reconcile time. `false` triggers the
    /// `OrphanR2Detected` decision arm (WI §1.4 cross-reference with
    /// WI-S06-004 R2→D1 ordering).
    pub r2_present: bool,
}

/// Trait surfaced by every `blob_meta` reconcile-side backend
/// (production D1 reader + conditional UPDATE / in-memory fake).
pub trait BlobMetaRefcountStore: Send + Sync + core::fmt::Debug {
    /// Snapshot every `(tenant_id)` row participating in the
    /// reconcile pass. Production wiring drives chunked iteration via
    /// pagination; the in-memory fake returns the full set for
    /// determinism.
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn snapshot_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<BlobMetaReconcileRow>, ReconcileError>;

    /// Auto-fix UPDATE: atomically replace
    /// `blob_meta.refcount = expected_refcount` for `(tenant, digest)`,
    /// guarded by the conditional predicate
    /// `WHERE refcount = stored_refcount` (Lote 10.6bis P0-7 chaos
    /// scenario 11 race protection — if a customer UpdateAR fired
    /// between snapshot and UPDATE the conditional fails and the
    /// auto-fix reverts to a no-op decision arm). Mirrors:
    ///
    /// ```sql
    /// UPDATE blob_meta
    ///    SET refcount = ?
    ///  WHERE tenant_id = ?
    ///    AND digest = ?
    ///    AND refcount = ?           -- conditional anti-ping-pong
    ///    AND deleted_at_ms IS NULL
    /// ```
    ///
    /// Returns `true` when the UPDATE fired; `false` when the
    /// conditional predicate failed (concurrent UpdateAR raced;
    /// re-evaluate next reconcile cycle).
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn conditional_set_refcount(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        stored_refcount: u32,
        new_refcount: u32,
    ) -> Result<bool, ReconcileError>;
}

// ============================================================================
//  ReconcileClock seam.
// ============================================================================

/// Wall-clock seam for the reconcile phase. Mirrors
/// [`crate::sweep::SweepClock`] / [`crate::physical_delete::PhysicalDeleteClock`]
/// / [`crate::mark::MarkClock`] so the four phases share a deterministic
/// test seam without binding directly to `Date.now()`.
pub trait ReconcileClock: Send + Sync + core::fmt::Debug {
    /// Read the current wall-clock instant (Unix ms). Each call may
    /// return a value `>=` the previous call.
    fn now_ms(&self) -> u64;
}

// `CountingReconcileClock` lives in [`plan`] (test-supporting seam,
// alongside `ReconcileConfig`).

// ============================================================================
//  ReconcileDecision + Result + Error taxonomy.
// ============================================================================

/// Per-row decision produced by
/// [`InMemoryReconcilePhase::step_row`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ReconcileDecision {
    /// `expected_refcount == stored_refcount`. No mutation; audit
    /// `corelink.gc.reconcile.refcount_reconciled` emitted (forensic
    /// trail; sustained drift < 0.1% DoD).
    NoDrift {
        /// Refcount observed at the canonical snapshot instant.
        refcount: u32,
    },
    /// Drift detected AND within the auto-fix gate
    /// (`drift_count ≤ AUTO_FIX_MAX_RECORDS AND drift_percent ≤
    /// AUTO_FIX_MAX_PERCENT`). Auto-fix UPDATE fired; audit
    /// `corelink.gc.reconcile.refcount_auto_fixed` emitted BEFORE the
    /// UPDATE (fail-closed envelope).
    AutoFixed {
        /// Refcount BEFORE the UPDATE (for forensic delta).
        from_refcount: u32,
        /// Refcount AFTER the UPDATE (= `expected_refcount`).
        to_refcount: u32,
        /// Wall-clock instant the auto-fix fired.
        fixed_at_ms: u64,
    },
    /// Drift detected AND exceeds the auto-fix gate (count > 5 OR
    /// percent > 0.01% per tenant). Auto-fix PAUSED for the row;
    /// audit `corelink.gc.reconcile.refcount_manual_review_required`
    /// (SEV-1) emitted; no mutation. Reconcile cycle re-evaluates the
    /// row on the next cron tick (24h).
    PausedForManualReview {
        /// Stored refcount.
        stored_refcount: u32,
        /// Expected refcount computed from `json_each`-equivalent
        /// scan.
        expected_refcount: u32,
        /// Per-tenant drift_count at the moment the gate fired.
        drift_count: u64,
        /// Per-tenant drift_percent at the moment the gate fired.
        drift_percent: f64,
    },
    /// Skipped: row is soft-deleted (`deleted_at_ms IS NOT NULL`) at
    /// reconcile time. WI §6.1.10 chaos test #12 — physical-delete
    /// fired mid-reconcile; refcount=NULL on physical purge would
    /// trigger a spurious drift; resolution is to skip the row +
    /// emit a `deletion-during-reconcile` informational audit (forward;
    /// not in scope for this WI).
    SkippedSoftDeleted,
    /// Orphan R2 detected: `blob_meta` row references a digest whose
    /// R2 object is missing (WI §1.4 cross-reference with WI-S06-004
    /// R2→D1 ordering). NO refcount UPDATE here (mutation would
    /// amplify the inconsistency); production wiring drives a follow-up
    /// reclaim task. Audit emit deferred to S-09 audit chain.
    OrphanR2Detected {
        /// Stored refcount (informational).
        refcount: u32,
    },
}

/// Aggregate outcome of one reconcile phase execution.
#[derive(Clone, Debug, PartialEq)]
pub struct ReconcileResult {
    /// Total rows inspected (sum across decisions).
    pub blobs_scanned: u64,
    /// Rows where `expected == stored` (no drift).
    pub no_drift_count: u64,
    /// Rows where drift was detected AND auto-fixed under the
    /// dual-condition gate.
    pub auto_fixed_count: u64,
    /// Rows where drift was detected but exceeded the auto-fix gate
    /// (paused for manual review; SEV-1).
    pub manual_review_count: u64,
    /// Rows skipped because `blob_meta.deleted_at_ms IS NOT NULL`.
    pub skipped_soft_deleted_count: u64,
    /// Rows where the R2 object is missing (orphan R2 — WI §1.4).
    pub orphan_r2_count: u64,
    /// Total drift count across `(tenant, region)`.
    pub drifts_detected: u64,
    /// Per-tenant drift percentage observed (`drifts_detected /
    /// blobs_scanned`).
    pub drift_percent: f64,
    /// SEV level computed at the end of the reconcile pass.
    pub sev_level: SevLevel,
    /// End-to-end reconcile duration (ms).
    pub reconcile_duration_ms: u64,
    /// Total audit events emitted (one per non-skipped decision).
    pub audit_events_emitted: u64,
    /// Snapshot anchor (`gc_run.checkpoint(reconcile_started_at_ms)`).
    pub reconcile_started_at_ms: u64,
}

/// SEV level computed at the end of a reconcile pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SevLevel {
    /// `drift_percent < 0.1% global` AND `per_tenant_drift < 1%`. No
    /// SEV alert; sustained drift < 0.1% gates DoD §10.s06.5.
    None,
    /// `0.1% ≤ global drift ≤ 1%`. Auto-fix small drifts; SEV-2 alert.
    Sev2,
    /// `per-tenant drift > 1%`. Auto-fix paused for the tenant; SEV-1
    /// alert (sprint contract §5.5 R-S06-10 alignment; Lote 10.6bis
    /// P2-7 fix — was previously listed as global > 1% which is
    /// unreachable).
    Sev1,
}

impl SevLevel {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sev2 => "sev2",
            Self::Sev1 => "sev1",
        }
    }
}

/// Canonical [`InMemoryReconcilePhase`] error taxonomy.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReconcileError {
    /// `gc_run` row backend / CHECK violation.
    #[error(transparent)]
    RunStore(#[from] GcRunStoreError),
    /// Audit emission failed mid-decision; reconcile ABORTED that row;
    /// no `blob_meta.refcount` mutation persisted. Per WI §6.1.7
    /// fail-closed envelope.
    #[error(transparent)]
    AuditEmissionFailed(#[from] GcAuditSinkError),
    /// Metrics emission failure (rare; downgradeable to log-and-continue
    /// in production wiring per WI §14.s06.001.6).
    #[error(transparent)]
    Metrics(#[from] GcMetricsObserverError),
    /// Reconcile phase budget exceeded (1h p99 @ 1M blobs per sprint
    /// contract §5.5 R-S06-10.1).
    #[error("reconcile phase budget exceeded: duration_ms={duration_ms} > budget_ms={budget_ms}")]
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

impl From<ReconcileError> for GcError {
    fn from(err: ReconcileError) -> Self {
        match err {
            ReconcileError::RunStore(e) => GcError::RunStore(e),
            ReconcileError::AuditEmissionFailed(e) => GcError::Audit(e),
            ReconcileError::Metrics(e) => GcError::Metrics(e),
            other => GcError::RunStore(GcRunStoreError::Backend(other.to_string())),
        }
    }
}

// ============================================================================
//  ReconcilePhase trait.
// ============================================================================

/// Trait surfaced by every reconcile phase backend (production CF Cron
/// DO handler / in-memory fake).
pub trait ReconcilePhase: Send + Sync + core::fmt::Debug {
    /// Execute the reconcile phase end-to-end for the given `(run_id,
    /// tenant, region)`. Iterates `blob_meta` rows, computes
    /// `expected_refcount` per `(tenant, digest)` via the canonical
    /// `json_each` aggregate, applies the dual-condition auto-fix gate,
    /// and emits audit + metrics.
    ///
    /// # Errors
    ///
    /// Surface as [`ReconcileError`].
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<ReconcileResult, ReconcileError>;
}
