//! GC sweep runner — the **entrypoint** that wires the existing
//! `corelink-gc` reclaim logic into a single, mode-gated sweep.
//!
//! ## Why this module exists
//!
//! The crate already ships the full reclaim logic (mark → sweep →
//! [`crate::physical_delete`] → reconcile), but nothing *invoked* one
//! end-to-end sweep over a run's candidate set. The DD finding —
//! "physical-delete GC has no entrypoint; erased/over-cap bytes are
//! never reclaimed" — is exactly that missing call site. This module is
//! that call site.
//!
//! ## DRY-RUN BY DEFAULT (the frozen policy)
//!
//! [`GcSweepRunner::run`] is driven by a [`GcSweepMode`] that **defaults
//! to [`GcSweepMode::DryRun`]**:
//!
//! - **[`GcSweepMode::DryRun`]** (default, 100% non-destructive): every
//!   candidate is classified through the read-only
//!   [`crate::physical_delete::InMemoryPhysicalDeletePhase::classify_candidate`]
//!   gate. Reclaimable objects are *counted + listed* in the
//!   [`SweepReport`]; **zero** R2 `DeleteObject` calls, **zero** D1
//!   purges, **zero** candidate transitions, **zero** audit
//!   `physical_deleted` emits happen. `SweepReport::deleted_count` and
//!   `SweepReport::deleted_bytes` are `0` by construction.
//! - **[`GcSweepMode::LiveDelete`]** (gated behind the
//!   `GC_LIVE_DELETE=true` env flag — see [`GcSweepMode::from_env`]):
//!   reclaimable candidates are purged via the **existing**
//!   [`crate::physical_delete::InMemoryPhysicalDeletePhase::step_candidate`]
//!   path, which deletes ONLY objects the gate positively classifies as
//!   reclaimable (`Swept` + post-grace + `refcount == 0`). A live blob
//!   (`refcount != 0`) or a grace-pending object is NEVER deleted.
//!
//! The mode gate **fails closed**: any value other than a literal
//! `true` / `1` (case-insensitive) resolves to [`GcSweepMode::DryRun`].
//!
//! ## Report channel
//!
//! Every sweep — dry-run or live — emits exactly one [`SweepReport`] to
//! a [`GcSweepReportSink`]. This is a dedicated report channel (NOT the
//! frozen 11-event `corelink.gc.*` audit taxonomy): production wiring
//! composes it onto a structured log line / outbox row the operator
//! reviews before enabling live delete.
//!
//! ## Production data-source wiring (flagged, out of scope here)
//!
//! The runner is generic over the same trait seams the rest of the
//! crate uses. The in-memory fakes prove every invariant. The
//! **production** sweep must inject the real Cloudflare bindings —
//! `R2 DeleteObject` adapter + the D1 `blob_meta` / `gc_candidates`
//! readers + the `gc_run` enumeration. Those bindings live in the
//! container/worker plane (NOT in this crate); wiring them is the
//! owner-gated follow-up that flips `GC_LIVE_DELETE` on after reviewing
//! a dry-run report.

use std::sync::Arc;
use std::sync::Mutex;

use thiserror::Error;
use uuid::Uuid;

use crate::audit::GcAuditSink;
use crate::mark::{GcCandidatesStore, MarkError};
use crate::metrics::GcMetricsObserver;
use crate::physical_delete::{
    BlobMetaPurgeStore, InMemoryPhysicalDeletePhase, PhysicalDeleteClock, PhysicalDeleteConfig,
    PhysicalDeleteDecision, PhysicalDeleteError, R2Delete, ReclaimClassification,
};
use crate::region::GcRegion;
use crate::run::{GcPhase, GcRunStore, GcRunStoreError, RunId};

/// Execution mode for one [`GcSweepRunner::run`] sweep.
///
/// **Defaults to [`GcSweepMode::DryRun`]** everywhere a default is
/// taken — the live-delete path is opt-in and fails closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum GcSweepMode {
    /// Non-destructive: classify + report; delete nothing.
    #[default]
    DryRun,
    /// Destructive: purge positively-classified reclaimable objects via
    /// the existing physical-delete path. Gated behind
    /// `GC_LIVE_DELETE=true`.
    LiveDelete,
}

impl GcSweepMode {
    /// The environment variable that gates live delete.
    pub const ENV_VAR: &'static str = "GC_LIVE_DELETE";

    /// Map an explicit boolean to a mode (`true` → live; `false` →
    /// dry-run).
    #[must_use]
    pub const fn from_live_flag(live: bool) -> Self {
        if live {
            Self::LiveDelete
        } else {
            Self::DryRun
        }
    }

    /// Resolve a mode from a raw env value, **failing closed**: only a
    /// literal `true` / `1` (case-insensitive, trimmed) enables live
    /// delete; `None` or anything else → [`GcSweepMode::DryRun`].
    #[must_use]
    pub fn from_env_value(raw: Option<&str>) -> Self {
        match raw {
            Some(v) => {
                let v = v.trim();
                if v.eq_ignore_ascii_case("true") || v == "1" {
                    Self::LiveDelete
                } else {
                    Self::DryRun
                }
            }
            None => Self::DryRun,
        }
    }

    /// Resolve the mode from the process environment
    /// ([`GcSweepMode::ENV_VAR`]); fails closed to
    /// [`GcSweepMode::DryRun`] when unset / unparseable.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_env_value(std::env::var(Self::ENV_VAR).ok().as_deref())
    }

    /// Whether this mode performs destructive deletes.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::LiveDelete)
    }

    /// Canonical lower-snake-case label (report / log field).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry_run",
            Self::LiveDelete => "live_delete",
        }
    }
}

/// The structured outcome of one sweep — the "report event" emitted to
/// the [`GcSweepReportSink`] and returned to the caller.
///
/// In [`GcSweepMode::DryRun`], `deleted_count` and `deleted_bytes` are
/// `0` by construction and `reclaimable_keys` lists the R2 keys a live
/// sweep WOULD have deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SweepReport {
    /// Mode the sweep ran in.
    pub mode: GcSweepMode,
    /// Run identity swept.
    pub run_id: RunId,
    /// Tenant scope.
    pub tenant_id: Uuid,
    /// Region scope.
    pub region: GcRegion,
    /// Total candidate rows inspected.
    pub candidates_scanned: u64,
    /// Candidates the reclaim gate classified as reclaimable (would be
    /// deleted by a live sweep).
    pub reclaimable_count: u64,
    /// Sum of `blob_size_bytes` across reclaimable candidates.
    pub reclaimable_bytes: u64,
    /// Objects actually deleted. **Always `0` in [`GcSweepMode::DryRun`].**
    pub deleted_count: u64,
    /// Bytes actually reclaimed. **Always `0` in [`GcSweepMode::DryRun`].**
    pub deleted_bytes: u64,
    /// Candidates skipped because the grace window has not elapsed.
    pub skipped_grace_pending: u64,
    /// Candidates skipped because a live re-reference set `refcount != 0`
    /// (protected live blobs).
    pub skipped_refcount_non_zero: u64,
    /// Already-resolved candidates (not `Swept`, or `blob_meta` row
    /// gone — idempotent no-op).
    pub already_resolved: u64,
    /// R2 object keys a live sweep would delete (dry-run listing). In
    /// live mode this lists the keys that were purged.
    pub reclaimable_keys: Vec<String>,
    /// Sweep wall-clock duration (ms).
    pub duration_ms: u64,
    /// Producer-side wall-clock instant the report was emitted (Unix ms).
    pub now_ms: u64,
}

/// Errors surfaced by [`GcSweepReportSink::emit_report`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GcSweepReportError {
    /// Backend transport failure (log / outbox write).
    #[error("gc sweep report sink store error: {0}")]
    Store(String),
}

/// Report-channel sink. Production wiring composes this onto a
/// structured log line / `audit_outbox` row; the in-memory fake
/// captures reports for tests.
pub trait GcSweepReportSink: Send + Sync + core::fmt::Debug {
    /// Persist one sweep report durably.
    ///
    /// # Errors
    ///
    /// Returns [`GcSweepReportError::Store`] on any backend failure;
    /// the runner treats this fail-closed and aborts the sweep.
    fn emit_report(&self, report: &SweepReport) -> Result<(), GcSweepReportError>;
}

/// In-memory report sink. Cloning shares the underlying buffer.
#[derive(Clone, Default, Debug)]
pub struct InMemoryGcSweepReportSink {
    inner: Arc<Mutex<Vec<SweepReport>>>,
}

impl InMemoryGcSweepReportSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every report captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<SweepReport> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of reports captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether no report was captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl GcSweepReportSink for InMemoryGcSweepReportSink {
    fn emit_report(&self, report: &SweepReport) -> Result<(), GcSweepReportError> {
        let mut g = self.inner.lock().map_err(|_| {
            GcSweepReportError::Store("sweep report sink mutex poisoned".to_owned())
        })?;
        g.push(report.clone());
        Ok(())
    }
}

/// Canonical error taxonomy for [`GcSweepRunner::run`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GcSweepError {
    /// A reclaim-logic / data-source error (run lookup, candidate
    /// snapshot, `blob_meta` lookup, R2/D1 in live mode). Fail-closed:
    /// the sweep aborts and no further deletes happen.
    #[error(transparent)]
    PhysicalDelete(#[from] PhysicalDeleteError),
    /// The report sink failed; the sweep aborts fail-closed.
    #[error("sweep report sink error: {0}")]
    ReportSink(String),
}

impl From<GcRunStoreError> for GcSweepError {
    fn from(e: GcRunStoreError) -> Self {
        Self::PhysicalDelete(PhysicalDeleteError::from(e))
    }
}

impl From<MarkError> for GcSweepError {
    fn from(e: MarkError) -> Self {
        Self::PhysicalDelete(PhysicalDeleteError::from(e))
    }
}

/// The GC sweep runner: classify (and, in live mode, reclaim) every
/// candidate of one `gc_run`, then emit a single [`SweepReport`].
///
/// Generic over the same trait seams as the rest of the crate so the
/// in-memory fakes exercise every invariant; production injects the
/// real Cloudflare bindings (flagged in the module docs).
pub struct GcSweepRunner<S, C, B, R, A, M, K, P>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
    P: GcSweepReportSink,
{
    runs: Arc<S>,
    candidates: Arc<C>,
    phase: InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>,
    metrics: Arc<M>,
    report_sink: Arc<P>,
    clock: Arc<K>,
    mode: GcSweepMode,
}

impl<S, C, B, R, A, M, K, P> core::fmt::Debug for GcSweepRunner<S, C, B, R, A, M, K, P>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
    P: GcSweepReportSink,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GcSweepRunner")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl<S, C, B, R, A, M, K, P> GcSweepRunner<S, C, B, R, A, M, K, P>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
    P: GcSweepReportSink,
{
    /// Construct a runner with the canonical [`PhysicalDeleteConfig`]
    /// defaults.
    #[allow(
        clippy::too_many_arguments,
        reason = "wires the same 7 trait deps as the physical-delete phase \
                  + the report sink + mode; a builder would obscure the \
                  call-site clarity the in-memory tests rely on."
    )]
    pub fn with_defaults(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta_purge: Arc<B>,
        r2: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        report_sink: Arc<P>,
        mode: GcSweepMode,
    ) -> Self {
        Self::new(
            runs,
            candidates,
            blob_meta_purge,
            r2,
            audit,
            metrics,
            clock,
            report_sink,
            PhysicalDeleteConfig::default(),
            mode,
        )
    }

    /// Construct a runner with an explicit [`PhysicalDeleteConfig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "wires the same 7 trait deps as the physical-delete phase \
                  + the report sink + config + mode; a builder would obscure \
                  the call-site clarity the in-memory tests rely on."
    )]
    pub fn new(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta_purge: Arc<B>,
        r2: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        report_sink: Arc<P>,
        config: PhysicalDeleteConfig,
        mode: GcSweepMode,
    ) -> Self {
        let phase = InMemoryPhysicalDeletePhase::new(
            Arc::clone(&runs),
            Arc::clone(&candidates),
            blob_meta_purge,
            r2,
            audit,
            Arc::clone(&metrics),
            Arc::clone(&clock),
            config,
        );
        Self {
            runs,
            candidates,
            phase,
            metrics,
            report_sink,
            clock,
            mode,
        }
    }

    /// The configured [`GcSweepMode`].
    #[must_use]
    pub const fn mode(&self) -> GcSweepMode {
        self.mode
    }

    /// Run one sweep over the candidate set of `(run_id, tenant, region)`.
    ///
    /// Fail-closed: any backend error (run lookup, candidate snapshot,
    /// `blob_meta` lookup, R2/D1 in live mode, or report emit) aborts
    /// the sweep before any further mutation. In [`GcSweepMode::DryRun`]
    /// the returned [`SweepReport`] is guaranteed to have
    /// `deleted_count == 0` and `deleted_bytes == 0`.
    ///
    /// # Errors
    ///
    /// Surface as [`GcSweepError`].
    pub fn run(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<SweepReport, GcSweepError> {
        // Fail-closed: the run must exist and its region must match.
        let run_row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        if run_row.region != region {
            return Err(GcSweepError::PhysicalDelete(
                PhysicalDeleteError::RegionMismatch {
                    run_region: run_row.region,
                    caller_region: region,
                },
            ));
        }

        let phase_start = self.clock.now_ms();
        let phase_budget_ms = self.phase.config().phase_budget_ms();
        let deadline_ms = phase_start.saturating_add(phase_budget_ms);
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id)?;

        let mut candidates_scanned: u64 = 0;
        let mut reclaimable_count: u64 = 0;
        let mut reclaimable_bytes: u64 = 0;
        let mut deleted_count: u64 = 0;
        let mut deleted_bytes: u64 = 0;
        let mut skipped_grace_pending: u64 = 0;
        let mut skipped_refcount_non_zero: u64 = 0;
        let mut already_resolved: u64 = 0;
        let mut reclaimable_keys: Vec<String> = Vec::new();

        for candidate in &candidates {
            // Keep the diagnostic path bounded by the same phase budget as the
            // mutating path. This is checked for both modes so a large or slow
            // dry-run cannot become an unbounded production footgun.
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(GcSweepError::PhysicalDelete(
                    PhysicalDeleteError::PhaseBudgetExceeded {
                        duration_ms: now_for_probe.saturating_sub(phase_start),
                        budget_ms: phase_budget_ms,
                    },
                ));
            }
            // The store contract is tenant/run-scoped; enforce that contract
            // at this boundary as well, failing closed if an adapter leaks a
            // row from another tenant or run.
            if candidate.tenant_id != tenant_id || candidate.mark_run_id != run_id {
                return Err(GcSweepError::PhysicalDelete(
                    PhysicalDeleteError::Backend(format!(
                        "candidate scope mismatch: expected tenant={tenant_id} run={run_id:?}, observed tenant={} run={:?}",
                        candidate.tenant_id, candidate.mark_run_id
                    )),
                ));
            }
            candidates_scanned = candidates_scanned.saturating_add(1);
            if self.mode.is_live() {
                // LIVE: run the EXISTING physical-delete path. It purges
                // ONLY positively-classified reclaimable objects; live
                // blobs / grace-pending objects are never deleted.
                match self.phase.step_candidate(candidate, region)? {
                    PhysicalDeleteDecision::Purged {
                        bytes_reclaimed, ..
                    } => {
                        reclaimable_count = reclaimable_count.saturating_add(1);
                        reclaimable_bytes = reclaimable_bytes.saturating_add(bytes_reclaimed);
                        deleted_count = deleted_count.saturating_add(1);
                        deleted_bytes = deleted_bytes.saturating_add(bytes_reclaimed);
                        reclaimable_keys.push(candidate.digest.to_string());
                    }
                    PhysicalDeleteDecision::SkippedGracePending { .. } => {
                        skipped_grace_pending = skipped_grace_pending.saturating_add(1);
                    }
                    PhysicalDeleteDecision::SkippedRefcountNonZero { .. } => {
                        skipped_refcount_non_zero = skipped_refcount_non_zero.saturating_add(1);
                    }
                    PhysicalDeleteDecision::AlreadyResolved { .. } => {
                        already_resolved = already_resolved.saturating_add(1);
                    }
                }
            } else {
                // DRY-RUN: classify ONLY (read-only). Zero deletes.
                match self.phase.classify_candidate(candidate)? {
                    ReclaimClassification::Reclaimable {
                        r2_key, size_bytes, ..
                    } => {
                        reclaimable_count = reclaimable_count.saturating_add(1);
                        reclaimable_bytes = reclaimable_bytes.saturating_add(size_bytes);
                        reclaimable_keys.push(r2_key);
                    }
                    ReclaimClassification::GracePending { .. } => {
                        skipped_grace_pending = skipped_grace_pending.saturating_add(1);
                    }
                    ReclaimClassification::RefcountNonZero { .. } => {
                        skipped_refcount_non_zero = skipped_refcount_non_zero.saturating_add(1);
                    }
                    ReclaimClassification::NotSwept { .. }
                    | ReclaimClassification::Resolved { .. } => {
                        already_resolved = already_resolved.saturating_add(1);
                    }
                }
            }
        }

        let now_ms = self.clock.now_ms();
        if now_ms > deadline_ms {
            return Err(GcSweepError::PhysicalDelete(
                PhysicalDeleteError::PhaseBudgetExceeded {
                    duration_ms: now_ms.saturating_sub(phase_start),
                    budget_ms: phase_budget_ms,
                },
            ));
        }
        let duration_ms = now_ms.saturating_sub(phase_start);

        let report = SweepReport {
            mode: self.mode,
            run_id,
            tenant_id,
            region,
            candidates_scanned,
            reclaimable_count,
            reclaimable_bytes,
            deleted_count,
            deleted_bytes,
            skipped_grace_pending,
            skipped_refcount_non_zero,
            already_resolved,
            reclaimable_keys,
            duration_ms,
            now_ms,
        };

        // Observability: phase-duration histogram (best-effort, but
        // fail-closed like the physical-delete phase).
        self.metrics
            .record_phase_duration_ms(GcPhase::PhysicalDelete, tenant_id, region, duration_ms)
            .map_err(PhysicalDeleteError::from)?;

        // Emit exactly one report event; fail-closed.
        self.report_sink
            .emit_report(&report)
            .map_err(|e| GcSweepError::ReportSink(e.to_string()))?;

        Ok(report)
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
#[path = "sweep_runner_tests.rs"]
mod sweep_runner_tests;
