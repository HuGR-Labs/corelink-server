//! [`GcRun`] row + canonical phase / status state machines + the
//! [`GcRunStore`] trait every backend satisfies + the
//! [`InMemoryGcRunStore`] fake whose semantics mirror the SQL
//! migration `migrations/d1/0006_gc_run.sql` byte-for-byte.
//!
//! ## Invariants enforced by [`InMemoryGcRunStore`] (same as SQL)
//!
//! - **`INV-GC-SINGLE-RUNNING-PER-TENANT-REGION` (HIGH)**: partial
//!   UNIQUE on `(tenant_id, region)` `WHERE status = 'running'` —
//!   second insert with status='running' for the same `(tenant,
//!   region)` returns [`GcRunStoreError::AlreadyRunning`].
//! - **`INV-GC-PHASE-MONOTONIC` (HIGH)**: phase transitions follow
//!   `idle → mark → sweep → physical_delete → reconcile → completed`
//!   with optional `* → failed` terminal. Reverse / skip transitions
//!   reject.
//! - **`INV-GC-MARK-STARTED-AT-IMMUTABLE` (CRITICAL)**:
//!   `mark_started_at_ms` set ONCE on Mark phase entry; every
//!   subsequent transition / checkpoint MUST preserve it. Attempts to
//!   re-set surface as [`GcRunStoreError::MarkStartedAtImmutable`].
//! - **Tenant-isolation envelope**: every read API takes
//!   `(tenant_id, run_id)` first, so a Tenant B query for a Tenant A
//!   row returns `Ok(None)` (Layer 4 of the 5-layer defence at
//!   storage level).
//! - **`INV-GC-IDEMPOTENT-RERUN` (HIGH)**: a row in `Crashed` /
//!   `Aborted` / `Failed` state is preserved as forensic trail; a new
//!   run can be inserted for the same `(tenant, region)` because the
//!   partial UNIQUE only restricts `status = 'running'`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::region::GcRegion;

/// Sentinel value indicating `mark_started_at_ms` has not been set.
/// `0` is unreachable as a real Unix-ms timestamp (1970-01-01) and the
/// SQL column is `NULL`-able; we use `0` in-memory + `Option` at the
/// API surface to keep the row layout straightforward.
pub const NULL_MARK_STARTED_AT_MS: u64 = 0;

/// Canonical GC phase domain. WI §1 enumerates the 7 phases; the
/// `#[non_exhaustive]` marker reserves the right to add S-06 follow-on
/// variants additively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum GcPhase {
    /// Phase 0 — pre-Mark.
    Idle,
    /// Phase 1 — Mark phase: scan reachable set + persist
    /// `mark_started_at_ms` (WI-S06-002).
    Mark,
    /// Phase 2 — Sweep phase: soft-delete unreachable blobs +
    /// `INV-GC-004` enforcement (WI-S06-003).
    Sweep,
    /// Phase 3 — Physical delete: post-grace R2 DeleteObject + row
    /// purge (WI-S06-004).
    PhysicalDelete,
    /// Phase 4 — Reconcile: refcount drift detection + auto-fix
    /// (WI-S06-005).
    Reconcile,
    /// Phase 5 — Completed: terminal success state.
    Completed,
    /// Phase 6 — Failed: terminal failure state. Lote 10.6bis P0-4 fix
    /// — `WI-S06-002 §6.1.10` PhaseBudgetExceeded transitions here.
    Failed,
}

/// `Display`-friendly string mnemonic for each phase. Matches the SQL
/// CHECK literal list byte-for-byte (`'idle'`, `'mark'`, …) so the
/// SQL column + the in-memory enum cannot drift.
impl GcPhase {
    /// Canonical lower-snake-case mnemonic matching the SQL CHECK
    /// literal.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Mark => "mark",
            Self::Sweep => "sweep",
            Self::PhysicalDelete => "physical_delete",
            Self::Reconcile => "reconcile",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Whether `self → next` is a legal transition.
    ///
    /// Canonical graph (WI §6.1.10 + spec contract §5.2..§5.5):
    ///
    /// ```text
    ///  Idle → Mark → Sweep → PhysicalDelete → Reconcile → Completed
    ///   ↓      ↓       ↓           ↓               ↓
    ///   └──────┴───────┴───────────┴───────────────┴───→ Failed
    /// ```
    ///
    /// `Failed` and `Completed` are terminal; no outbound transitions.
    /// Reverse transitions and skip-ahead transitions reject.
    #[must_use]
    pub const fn can_transition_to(self, next: GcPhase) -> bool {
        match (self, next) {
            // Forward progression along the canonical chain.
            (Self::Idle, Self::Mark)
            | (Self::Mark, Self::Sweep)
            | (Self::Sweep, Self::PhysicalDelete)
            | (Self::PhysicalDelete, Self::Reconcile)
            | (Self::Reconcile, Self::Completed) => true,
            // Failure can occur at any pre-terminal phase.
            (Self::Idle, Self::Failed)
            | (Self::Mark, Self::Failed)
            | (Self::Sweep, Self::Failed)
            | (Self::PhysicalDelete, Self::Failed)
            | (Self::Reconcile, Self::Failed) => true,
            // Everything else: reject.
            _ => false,
        }
    }
}

/// Coarser kind for phase classification (used by metric labels +
/// degrade-mode gating).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GcPhaseKind {
    /// Pre-execution (Idle).
    PreExec,
    /// Active execution phase (Mark / Sweep / PhysicalDelete /
    /// Reconcile).
    Active,
    /// Terminal (Completed / Failed).
    Terminal,
}

impl GcPhase {
    /// Coarser kind for metric labels.
    #[must_use]
    pub const fn kind(self) -> GcPhaseKind {
        match self {
            Self::Idle => GcPhaseKind::PreExec,
            Self::Mark | Self::Sweep | Self::PhysicalDelete | Self::Reconcile => {
                GcPhaseKind::Active
            }
            Self::Completed | Self::Failed => GcPhaseKind::Terminal,
        }
    }
}

/// Canonical run status domain (Lote 10.6bis P0-4 includes `Failed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GcStatus {
    /// Pre-cron-tick: row scheduled but not picked up by worker.
    Pending,
    /// Worker actively executing.
    Running,
    /// Phases all completed cleanly.
    Succeeded,
    /// Worker crashed mid-phase; `last_checkpoint_at_ms` anchors
    /// resume. WI §6.1.5 — chaos test #1 covers idempotent resume.
    Crashed,
    /// Aborted via degrade-mode `gc-pause`. WI §6.1.5 — chaos test
    /// #4.
    Aborted,
    /// Terminal failure (PhaseBudgetExceeded / non-recoverable
    /// PhaseFailure). Lote 10.6bis P0-4 fix.
    Failed,
}

impl GcStatus {
    /// Canonical lower-snake-case mnemonic matching the SQL CHECK
    /// literal.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Crashed => "crashed",
            Self::Aborted => "aborted",
            Self::Failed => "failed",
        }
    }

    /// Whether the status is terminal (no further transitions
    /// expected on this run; partial UNIQUE on
    /// `WHERE status='running'` does NOT exclude this row from the
    /// per-(tenant, region) lock release semantic).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Crashed | Self::Aborted | Self::Failed
        )
    }
}

/// Canonical run identity. UUIDv7-derived monotonic per WI §1; we
/// wrap a `Uuid` so the SQL column type (TEXT) and the in-memory
/// representation share an unambiguous shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(pub Uuid);

impl RunId {
    /// Canonical hyphenated lower-case TEXT form (matches the SQL
    /// column representation — `data_model.md §2.1 L91-93`).
    #[must_use]
    pub fn as_text(&self) -> String {
        self.0
            .as_hyphenated()
            .encode_lower(&mut Uuid::encode_buffer())
            .to_string()
    }
}

impl core::fmt::Display for RunId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.as_text())
    }
}

/// Materialised `gc_run` row (mirror of the SQL row layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcRun {
    /// Run identity (PK).
    pub run_id: RunId,
    /// Tenant scope (NOT NULL).
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: GcRegion,
    /// Current phase.
    pub phase: GcPhase,
    /// Current status.
    pub status: GcStatus,
    /// Lifecycle timestamps (unix ms).
    pub started_at_ms: u64,
    /// Updated per checkpoint; idempotent re-run anchor.
    pub last_checkpoint_at_ms: u64,
    /// Set when status transitions to `Succeeded`.
    pub completed_at_ms: Option<u64>,
    /// Set when status transitions to `Failed` / `Aborted`.
    pub failed_at_ms: Option<u64>,
    /// `INV-GC-MARK-STARTED-AT-IMMUTABLE` anchor — `None` until Mark
    /// phase begins; `Some(_)` once captured + immutable thereafter.
    pub mark_started_at_ms: Option<u64>,
    /// Counters (NOT NULL with default 0; CHECK >= 0).
    pub blobs_marked_count: u64,
    /// Sweep counter.
    pub blobs_swept_count: u64,
    /// Physical delete counter.
    pub blobs_physically_deleted_count: u64,
    /// Bytes reclaimed (sum across all phases).
    pub bytes_reclaimed: u64,
    /// Source attribution: `"cron"` for scheduled tick OR admin PAT id
    /// hex prefix.
    pub created_by_request_id: String,
    /// Failure forensics — phase at which the run failed (NULL on
    /// success).
    pub failed_phase: Option<String>,
    /// Failure forensics — short reason code (NULL on success).
    pub failed_reason: Option<String>,
}

/// Errors surfaced by [`GcRunStore`] backends (SQL and in-memory
/// fake).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum GcRunStoreError {
    /// Partial UNIQUE rejection: a `Running` gc_run already exists
    /// for `(tenant_id, region)`. Handler returns 409
    /// `COR_GC_ALREADY_RUNNING` per WI §6.1 Gherkin scenario 3.
    #[error("gc_run already running for tenant={tenant_id} region={region:?}: existing run_id={existing_run_id}")]
    AlreadyRunning {
        /// Tenant scope.
        tenant_id: Uuid,
        /// Region scope.
        region: GcRegion,
        /// Existing run id occupying the per-(tenant, region) lock.
        existing_run_id: RunId,
    },
    /// `mark_started_at_ms` immutable per
    /// `INV-GC-MARK-STARTED-AT-IMMUTABLE`.
    #[error("mark_started_at_ms is immutable: existing={existing} attempted={attempted}")]
    MarkStartedAtImmutable {
        /// Existing immutable anchor.
        existing: u64,
        /// Attempted overwrite.
        attempted: u64,
    },
    /// Phase transition violates [`GcPhase::can_transition_to`].
    #[error("invalid phase transition: {from} -> {to}")]
    InvalidPhaseTransition {
        /// Previous phase.
        from: &'static str,
        /// Attempted next phase.
        to: &'static str,
    },
    /// CHECK constraint violation — `last_checkpoint_at_ms <
    /// started_at_ms` or counter < 0 (counters are `u64` in-memory,
    /// so the SQL semantic is preserved by construction).
    #[error("check constraint violated: {0}")]
    CheckViolation(&'static str),
    /// Run not found — caller passed an unknown `run_id`.
    #[error("gc_run not found: run_id={0}")]
    NotFound(RunId),
    /// Cross-tenant access attempt — the run_id exists but its
    /// tenant_id does not match the calling context. Surface this
    /// distinct from `NotFound` so tests can prove the row was NOT
    /// touched (defense-in-depth Layer 4 enforcement).
    #[error("cross_tenant_run: run_id={run_id} belongs to a different tenant")]
    CrossTenantRun {
        /// The opaque run_id the caller supplied.
        run_id: RunId,
    },
    /// Backend transport failure (D1 / connection error).
    #[error("backend error: {0}")]
    Backend(String),
}

/// Trait surfaced by every gc_run backend (SQL D1 / in-memory fake).
///
/// The trait is sync because the production D1 binding is itself sync
/// at the row-mutation layer (single-row `INSERT` / `UPDATE`); this
/// matches the canonical `corelink-audit::Emitter` shape +
/// `reapi::ac::audit::AuditSink` shape.
pub trait GcRunStore: Send + Sync + core::fmt::Debug {
    /// Insert a fresh `gc_run` row with `status = 'pending'`. Returns
    /// the row's canonical `run_id`.
    ///
    /// # Errors
    ///
    /// Backend-class via [`GcRunStoreError::Backend`] / CHECK
    /// violation. The partial UNIQUE on `WHERE status='running'` does
    /// NOT fire on `Pending` insert — the lock is acquired only when
    /// status transitions to `Running`.
    fn insert_pending(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
        started_at_ms: u64,
        created_by_request_id: String,
    ) -> Result<(), GcRunStoreError>;

    /// Transition `Pending → Running` (acquires per-(tenant, region)
    /// lock). Sweep + physical delete + reconcile do NOT call this —
    /// they continue on the existing `Running` row.
    ///
    /// # Errors
    ///
    /// - [`GcRunStoreError::AlreadyRunning`] if another gc_run is
    ///   already `Running` for the same `(tenant_id, region)`.
    /// - [`GcRunStoreError::NotFound`] if `run_id` does not exist.
    /// - [`GcRunStoreError::CrossTenantRun`] if the row exists under
    ///   a different tenant.
    fn acquire_running(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<(), GcRunStoreError>;

    /// Transition phase (within `Running` status). Validates
    /// monotonic graph + sets `mark_started_at_ms` if `to ==
    /// GcPhase::Mark` (and `mark_started_at_ms` was previously
    /// unset).
    ///
    /// # Errors
    ///
    /// - [`GcRunStoreError::InvalidPhaseTransition`] on illegal graph
    ///   move.
    /// - [`GcRunStoreError::MarkStartedAtImmutable`] if Mark phase
    ///   is re-entered (programmer error; production wiring must not
    ///   re-set the anchor).
    fn transition_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        to: GcPhase,
        now_ms: u64,
    ) -> Result<(), GcRunStoreError>;

    /// Update `last_checkpoint_at_ms` (idempotent resume anchor) +
    /// optional counter increments.
    fn checkpoint(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
        deltas: CheckpointDeltas,
    ) -> Result<(), GcRunStoreError>;

    /// Mark the run as terminal.
    fn finalize(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        terminal: GcStatus,
        now_ms: u64,
        failure: Option<FailureContext>,
    ) -> Result<(), GcRunStoreError>;

    /// Lookup a row by run_id (tenant-scoped).
    fn lookup(&self, run_id: RunId, tenant_id: Uuid) -> Result<Option<GcRun>, GcRunStoreError>;

    /// Return the `Running` gc_run row for `(tenant_id, region)` if
    /// any — the partial UNIQUE guarantees at most one.
    fn current_running(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<Option<GcRun>, GcRunStoreError>;
}

/// Optional counter increments piggy-backed on a single
/// [`GcRunStore::checkpoint`] call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckpointDeltas {
    /// Increment to `blobs_marked_count`.
    pub blobs_marked_delta: u64,
    /// Increment to `blobs_swept_count`.
    pub blobs_swept_delta: u64,
    /// Increment to `blobs_physically_deleted_count`.
    pub blobs_physically_deleted_delta: u64,
    /// Increment to `bytes_reclaimed`.
    pub bytes_reclaimed_delta: u64,
}

/// Failure forensics passed to [`GcRunStore::finalize`] when the
/// terminal status is `Failed` / `Crashed` / `Aborted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureContext {
    /// Phase at which the run failed.
    pub failed_phase: GcPhase,
    /// Short reason code (e.g. `"PhaseBudgetExceeded"`,
    /// `"degrade_mode_gc_pause"`).
    pub failed_reason: String,
}

// =========================================================================
//  In-memory simulator
// =========================================================================

/// In-memory `gc_run` store whose semantics mirror the SQL migration
/// byte-for-byte.
#[derive(Debug, Default)]
pub struct InMemoryGcRunStore {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    rows: BTreeMap<RunId, GcRun>,
}

impl InMemoryGcRunStore {
    /// Construct a fresh store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (for property tests + diagnostics).
    #[must_use]
    pub fn snapshot(&self) -> Vec<GcRun> {
        match self.inner.lock() {
            Ok(g) => g.rows.values().cloned().collect(),
            Err(p) => p.into_inner().rows.values().cloned().collect(),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, GcRunStoreError> {
        self.inner
            .lock()
            .map_err(|_| GcRunStoreError::Backend("gc_run store mutex poisoned".to_string()))
    }
}

impl GcRunStore for InMemoryGcRunStore {
    fn insert_pending(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
        started_at_ms: u64,
        created_by_request_id: String,
    ) -> Result<(), GcRunStoreError> {
        let mut guard = self.lock()?;
        if guard.rows.contains_key(&run_id) {
            return Err(GcRunStoreError::CheckViolation(
                "primary_key_violation: run_id already exists",
            ));
        }
        let row = GcRun {
            run_id,
            tenant_id,
            region,
            phase: GcPhase::Idle,
            status: GcStatus::Pending,
            started_at_ms,
            last_checkpoint_at_ms: started_at_ms,
            completed_at_ms: None,
            failed_at_ms: None,
            mark_started_at_ms: None,
            blobs_marked_count: 0,
            blobs_swept_count: 0,
            blobs_physically_deleted_count: 0,
            bytes_reclaimed: 0,
            created_by_request_id,
            failed_phase: None,
            failed_reason: None,
        };
        guard.rows.insert(run_id, row);
        Ok(())
    }

    fn acquire_running(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<(), GcRunStoreError> {
        let mut guard = self.lock()?;
        // Partial UNIQUE check: another `Running` row for
        // (tenant_id, region) is rejected.
        let region = match guard.rows.get(&run_id) {
            Some(row) => {
                if row.tenant_id != tenant_id {
                    return Err(GcRunStoreError::CrossTenantRun { run_id });
                }
                row.region
            }
            None => return Err(GcRunStoreError::NotFound(run_id)),
        };
        if let Some(existing) = guard.rows.values().find(|row| {
            row.tenant_id == tenant_id
                && row.region == region
                && row.status == GcStatus::Running
                && row.run_id != run_id
        }) {
            return Err(GcRunStoreError::AlreadyRunning {
                tenant_id,
                region,
                existing_run_id: existing.run_id,
            });
        }
        let row = guard
            .rows
            .get_mut(&run_id)
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        // CHECK: last_checkpoint_at_ms >= started_at_ms.
        if now_ms < row.started_at_ms {
            return Err(GcRunStoreError::CheckViolation(
                "chk_gc_run_lifecycle_checkpoint",
            ));
        }
        row.status = GcStatus::Running;
        row.last_checkpoint_at_ms = now_ms;
        Ok(())
    }

    fn transition_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        to: GcPhase,
        now_ms: u64,
    ) -> Result<(), GcRunStoreError> {
        let mut guard = self.lock()?;
        let row = guard
            .rows
            .get_mut(&run_id)
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        if row.tenant_id != tenant_id {
            return Err(GcRunStoreError::CrossTenantRun { run_id });
        }
        if !row.phase.can_transition_to(to) {
            return Err(GcRunStoreError::InvalidPhaseTransition {
                from: row.phase.as_str(),
                to: to.as_str(),
            });
        }
        if to == GcPhase::Mark {
            // INV-GC-MARK-STARTED-AT-IMMUTABLE: set ONCE.
            if let Some(existing) = row.mark_started_at_ms {
                return Err(GcRunStoreError::MarkStartedAtImmutable {
                    existing,
                    attempted: now_ms,
                });
            }
            // CHECK: mark_started_at_ms >= started_at_ms.
            if now_ms < row.started_at_ms {
                return Err(GcRunStoreError::CheckViolation("chk_gc_run_mark_started"));
            }
            row.mark_started_at_ms = Some(now_ms);
        }
        if now_ms < row.last_checkpoint_at_ms {
            return Err(GcRunStoreError::CheckViolation(
                "chk_gc_run_lifecycle_checkpoint",
            ));
        }
        row.phase = to;
        row.last_checkpoint_at_ms = now_ms;
        Ok(())
    }

    fn checkpoint(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
        deltas: CheckpointDeltas,
    ) -> Result<(), GcRunStoreError> {
        let mut guard = self.lock()?;
        let row = guard
            .rows
            .get_mut(&run_id)
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        if row.tenant_id != tenant_id {
            return Err(GcRunStoreError::CrossTenantRun { run_id });
        }
        if now_ms < row.last_checkpoint_at_ms {
            return Err(GcRunStoreError::CheckViolation(
                "chk_gc_run_lifecycle_checkpoint",
            ));
        }
        row.last_checkpoint_at_ms = now_ms;
        row.blobs_marked_count = row
            .blobs_marked_count
            .saturating_add(deltas.blobs_marked_delta);
        row.blobs_swept_count = row
            .blobs_swept_count
            .saturating_add(deltas.blobs_swept_delta);
        row.blobs_physically_deleted_count = row
            .blobs_physically_deleted_count
            .saturating_add(deltas.blobs_physically_deleted_delta);
        row.bytes_reclaimed = row
            .bytes_reclaimed
            .saturating_add(deltas.bytes_reclaimed_delta);
        Ok(())
    }

    fn finalize(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        terminal: GcStatus,
        now_ms: u64,
        failure: Option<FailureContext>,
    ) -> Result<(), GcRunStoreError> {
        if !terminal.is_terminal() {
            return Err(GcRunStoreError::CheckViolation(
                "finalize_called_with_non_terminal_status",
            ));
        }
        let mut guard = self.lock()?;
        let row = guard
            .rows
            .get_mut(&run_id)
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        if row.tenant_id != tenant_id {
            return Err(GcRunStoreError::CrossTenantRun { run_id });
        }
        if now_ms < row.started_at_ms {
            return Err(GcRunStoreError::CheckViolation(
                "chk_gc_run_lifecycle_completed",
            ));
        }
        if now_ms < row.last_checkpoint_at_ms {
            return Err(GcRunStoreError::CheckViolation(
                "chk_gc_run_lifecycle_checkpoint",
            ));
        }
        row.status = terminal;
        row.last_checkpoint_at_ms = now_ms;
        match terminal {
            GcStatus::Succeeded => {
                row.phase = GcPhase::Completed;
                row.completed_at_ms = Some(now_ms);
            }
            GcStatus::Crashed | GcStatus::Aborted | GcStatus::Failed => {
                row.failed_at_ms = Some(now_ms);
                if let Some(ctx) = failure {
                    row.failed_phase = Some(ctx.failed_phase.as_str().to_owned());
                    row.failed_reason = Some(ctx.failed_reason);
                    if terminal == GcStatus::Failed {
                        row.phase = GcPhase::Failed;
                    }
                }
            }
            // Other terminal variants exhaust above.
            _ => {}
        }
        Ok(())
    }

    fn lookup(&self, run_id: RunId, tenant_id: Uuid) -> Result<Option<GcRun>, GcRunStoreError> {
        let guard = self.lock()?;
        match guard.rows.get(&run_id) {
            Some(row) if row.tenant_id == tenant_id => Ok(Some(row.clone())),
            // Cross-tenant: surface as `Ok(None)` per Layer 4 envelope
            // (spec contract §5.1 "tenant-isolation envelope").
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }

    fn current_running(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<Option<GcRun>, GcRunStoreError> {
        let guard = self.lock()?;
        Ok(guard
            .rows
            .values()
            .find(|row| {
                row.tenant_id == tenant_id
                    && row.region == region
                    && row.status == GcStatus::Running
            })
            .cloned())
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

    fn run_id(seed: u128) -> RunId {
        RunId(Uuid::from_u128(seed))
    }
    fn tenant(seed: u128) -> Uuid {
        Uuid::from_u128(seed)
    }

    #[test]
    fn phase_canonical_strings() {
        assert_eq!(GcPhase::Idle.as_str(), "idle");
        assert_eq!(GcPhase::Mark.as_str(), "mark");
        assert_eq!(GcPhase::Sweep.as_str(), "sweep");
        assert_eq!(GcPhase::PhysicalDelete.as_str(), "physical_delete");
        assert_eq!(GcPhase::Reconcile.as_str(), "reconcile");
        assert_eq!(GcPhase::Completed.as_str(), "completed");
        assert_eq!(GcPhase::Failed.as_str(), "failed");
    }

    #[test]
    fn status_canonical_strings() {
        assert_eq!(GcStatus::Pending.as_str(), "pending");
        assert_eq!(GcStatus::Running.as_str(), "running");
        assert_eq!(GcStatus::Succeeded.as_str(), "succeeded");
        assert_eq!(GcStatus::Crashed.as_str(), "crashed");
        assert_eq!(GcStatus::Aborted.as_str(), "aborted");
        assert_eq!(GcStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn legal_phase_chain() {
        assert!(GcPhase::Idle.can_transition_to(GcPhase::Mark));
        assert!(GcPhase::Mark.can_transition_to(GcPhase::Sweep));
        assert!(GcPhase::Sweep.can_transition_to(GcPhase::PhysicalDelete));
        assert!(GcPhase::PhysicalDelete.can_transition_to(GcPhase::Reconcile));
        assert!(GcPhase::Reconcile.can_transition_to(GcPhase::Completed));
    }

    #[test]
    fn any_active_phase_can_fail() {
        for p in [
            GcPhase::Idle,
            GcPhase::Mark,
            GcPhase::Sweep,
            GcPhase::PhysicalDelete,
            GcPhase::Reconcile,
        ] {
            assert!(p.can_transition_to(GcPhase::Failed), "{p:?}");
        }
    }

    #[test]
    fn terminal_phases_have_no_outbound() {
        for terminal in [GcPhase::Completed, GcPhase::Failed] {
            for next in [
                GcPhase::Idle,
                GcPhase::Mark,
                GcPhase::Sweep,
                GcPhase::PhysicalDelete,
                GcPhase::Reconcile,
                GcPhase::Completed,
                GcPhase::Failed,
            ] {
                assert!(!terminal.can_transition_to(next), "{terminal:?}->{next:?}");
            }
        }
    }

    #[test]
    fn skip_ahead_phase_rejected() {
        assert!(!GcPhase::Idle.can_transition_to(GcPhase::Sweep));
        assert!(!GcPhase::Mark.can_transition_to(GcPhase::Reconcile));
        assert!(!GcPhase::Sweep.can_transition_to(GcPhase::Completed));
    }

    #[test]
    fn reverse_phase_rejected() {
        assert!(!GcPhase::Mark.can_transition_to(GcPhase::Idle));
        assert!(!GcPhase::Sweep.can_transition_to(GcPhase::Mark));
        assert!(!GcPhase::Completed.can_transition_to(GcPhase::Reconcile));
    }

    #[test]
    fn insert_pending_then_lookup() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
        assert_eq!(row.status, GcStatus::Pending);
        assert_eq!(row.phase, GcPhase::Idle);
        assert_eq!(row.mark_started_at_ms, None);
    }

    #[test]
    fn cross_tenant_lookup_returns_none() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        // Tenant B sees nothing — Layer 4 envelope.
        assert!(store.lookup(run_id(1), tenant(2)).unwrap().is_none());
    }

    #[test]
    fn acquire_running_succeeds_then_blocks_second() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        // Second pending row for the same (tenant, region) ; insert
        // succeeds (Pending allowed) but acquire fails:
        store
            .insert_pending(run_id(2), tenant(1), GcRegion::Sam, 300, "cron".into())
            .unwrap();
        let err = store
            .acquire_running(run_id(2), tenant(1), 400)
            .unwrap_err();
        assert!(matches!(err, GcRunStoreError::AlreadyRunning { .. }));
    }

    #[test]
    fn other_region_can_acquire_in_parallel() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store
            .insert_pending(run_id(2), tenant(1), GcRegion::Iad, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        // Same tenant, different region — MUST succeed (per-region
        // partial UNIQUE).
        store.acquire_running(run_id(2), tenant(1), 200).unwrap();
    }

    #[test]
    fn other_tenant_can_acquire_same_region() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store
            .insert_pending(run_id(2), tenant(2), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        // Different tenant, same region — partial UNIQUE on
        // (tenant_id, region) so it's per-(tenant, region).
        store.acquire_running(run_id(2), tenant(2), 200).unwrap();
    }

    #[test]
    fn mark_started_at_is_immutable() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        store
            .transition_phase(run_id(1), tenant(1), GcPhase::Mark, 300)
            .unwrap();
        let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
        assert_eq!(row.mark_started_at_ms, Some(300));
        // Reverse phase to Mark is rejected by transition graph; even
        // if we could, the immutable mark_started_at_ms guard fires.
        let err = store
            .transition_phase(run_id(1), tenant(1), GcPhase::Mark, 999)
            .unwrap_err();
        assert!(matches!(
            err,
            GcRunStoreError::InvalidPhaseTransition { .. }
        ));
    }

    #[test]
    fn full_canonical_chain_run() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        for (to, t) in [
            (GcPhase::Mark, 300),
            (GcPhase::Sweep, 400),
            (GcPhase::PhysicalDelete, 500),
            (GcPhase::Reconcile, 600),
            (GcPhase::Completed, 700),
        ] {
            store
                .transition_phase(run_id(1), tenant(1), to, t)
                .unwrap();
        }
        store
            .finalize(run_id(1), tenant(1), GcStatus::Succeeded, 800, None)
            .unwrap();
        let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
        assert_eq!(row.status, GcStatus::Succeeded);
        assert_eq!(row.phase, GcPhase::Completed);
        assert_eq!(row.completed_at_ms, Some(800));
        assert_eq!(row.mark_started_at_ms, Some(300));
    }

    #[test]
    fn checkpoint_idempotent_re_run() {
        // After Crash, partial UNIQUE permits a NEW pending+running
        // row for the same (tenant, region) because the crashed row
        // is no longer Running.
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        store
            .finalize(
                run_id(1),
                tenant(1),
                GcStatus::Crashed,
                400,
                Some(FailureContext {
                    failed_phase: GcPhase::Mark,
                    failed_reason: "worker_panic".into(),
                }),
            )
            .unwrap();
        // New run can take the lock.
        store
            .insert_pending(run_id(2), tenant(1), GcRegion::Sam, 500, "cron".into())
            .unwrap();
        store.acquire_running(run_id(2), tenant(1), 600).unwrap();
        let new_row = store.current_running(tenant(1), GcRegion::Sam).unwrap();
        assert!(new_row.is_some());
        assert_eq!(new_row.unwrap().run_id, run_id(2));
    }

    #[test]
    fn reverse_lifecycle_timestamp_rejected() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        // acquire_running with now < started_at_ms must reject.
        let err = store.acquire_running(run_id(1), tenant(1), 50).unwrap_err();
        assert!(matches!(err, GcRunStoreError::CheckViolation(_)));
    }

    #[test]
    fn checkpoint_increments_counters() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        store.acquire_running(run_id(1), tenant(1), 200).unwrap();
        store
            .checkpoint(
                run_id(1),
                tenant(1),
                300,
                CheckpointDeltas {
                    blobs_marked_delta: 1000,
                    blobs_swept_delta: 0,
                    blobs_physically_deleted_delta: 0,
                    bytes_reclaimed_delta: 1024 * 1024,
                },
            )
            .unwrap();
        let row = store.lookup(run_id(1), tenant(1)).unwrap().unwrap();
        assert_eq!(row.blobs_marked_count, 1000);
        assert_eq!(row.bytes_reclaimed, 1024 * 1024);
    }

    #[test]
    fn finalize_with_non_terminal_rejected() {
        let store = InMemoryGcRunStore::new();
        store
            .insert_pending(run_id(1), tenant(1), GcRegion::Sam, 100, "cron".into())
            .unwrap();
        let err = store
            .finalize(run_id(1), tenant(1), GcStatus::Running, 200, None)
            .unwrap_err();
        assert!(matches!(err, GcRunStoreError::CheckViolation(_)));
    }

    #[test]
    fn run_id_text_form_is_canonical_uuid() {
        let r = RunId(Uuid::from_u128(0xdeadbeef_cafe_babe_1234_567890abcdef));
        let t = r.as_text();
        assert_eq!(t.len(), 36);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }
}
