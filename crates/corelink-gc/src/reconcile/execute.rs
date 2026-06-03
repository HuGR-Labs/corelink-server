//! `InMemoryReconcilePhase` — the canonical pure-logic reconcile phase
//! orchestrator. Composes the trait dependencies declared at
//! construction time + drives the per-row decision pipeline
//! (`step_row`) + the end-to-end `ReconcilePhase` impl.
//!
//! Extracted from the monolithic `reconcile.rs` per Wave 33 Stream A2.2
//! file-size discipline.

use std::sync::Arc;

use uuid::Uuid;

use crate::audit::{GcAuditRecord, GcAuditSink, GcEventType};
use crate::metrics::GcMetricsObserver;
use crate::region::GcRegion;
use crate::run::{CheckpointDeltas, GcPhase, GcRunStore, GcRunStoreError, GcStatus, RunId};

use super::plan::auto_fix_gate_fires;
use super::{
    sev_level_for, BlobMetaReconcileRow, BlobMetaRefcountStore, ReconcileClock, ReconcileConfig,
    ReconcileDecision, ReconcileError, ReconcilePhase, ReconcileResult, RefcountSource,
};

/// In-memory reconcile phase orchestrator (the canonical pure-logic
/// skeleton). Composes the trait dependencies declared at construction
/// time.
pub struct InMemoryReconcilePhase<S, R, B, A, M, K>
where
    S: GcRunStore,
    R: RefcountSource,
    B: BlobMetaRefcountStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: ReconcileClock,
{
    runs: Arc<S>,
    refcount_source: Arc<R>,
    blob_meta: Arc<B>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<K>,
    config: ReconcileConfig,
}

impl<S, R, B, A, M, K> core::fmt::Debug for InMemoryReconcilePhase<S, R, B, A, M, K>
where
    S: GcRunStore,
    R: RefcountSource,
    B: BlobMetaRefcountStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: ReconcileClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryReconcilePhase")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, R, B, A, M, K> InMemoryReconcilePhase<S, R, B, A, M, K>
where
    S: GcRunStore,
    R: RefcountSource,
    B: BlobMetaRefcountStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: ReconcileClock,
{
    /// Construct a reconcile phase orchestrator with the canonical
    /// defaults.
    pub fn with_defaults(
        runs: Arc<S>,
        refcount_source: Arc<R>,
        blob_meta: Arc<B>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
    ) -> Self {
        Self {
            runs,
            refcount_source,
            blob_meta,
            audit,
            metrics,
            clock,
            config: ReconcileConfig::default(),
        }
    }

    /// Construct with an explicit [`ReconcileConfig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 6 trait deps + 1 knob; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        runs: Arc<S>,
        refcount_source: Arc<R>,
        blob_meta: Arc<B>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        config: ReconcileConfig,
    ) -> Self {
        Self {
            runs,
            refcount_source,
            blob_meta,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// The [`ReconcileConfig`] snapshot.
    #[must_use]
    pub fn config(&self) -> ReconcileConfig {
        self.config
    }

    /// Process a single `blob_meta` row through the reconcile decision
    /// pipeline. Visible for property tests so the decision boundary
    /// can be exercised independently of the phase orchestration.
    ///
    /// `tenant_drift_count_so_far` + `tenant_blobs_scanned_so_far` feed
    /// the dual-condition auto-fix gate (the percentage arm divides
    /// `(drift_count_so_far + 1) / (blobs_scanned_so_far + 1)` to
    /// account for the row currently under inspection).
    ///
    /// `snapshot_at_ms` is the canonical `reconcile_started_at_ms`
    /// anchor.
    ///
    /// `run_id` + `region` propagate into the audit envelope.
    ///
    /// # Errors
    ///
    /// Surface as [`ReconcileError`].
    #[allow(
        clippy::too_many_arguments,
        reason = "step pipeline carries 7 explicit decision inputs; collapsing \
                  obscures the per-row predicate boundary tested by \
                  property tests."
    )]
    pub fn step_row(
        &self,
        tenant_id: Uuid,
        run_id: RunId,
        region: GcRegion,
        row: &BlobMetaReconcileRow,
        snapshot_at_ms: u64,
        tenant_drift_count_so_far: u64,
        tenant_blobs_scanned_so_far: u64,
    ) -> Result<ReconcileDecision, ReconcileError> {
        // Skipped soft-deleted: avoid spurious drift on a
        // physical-delete-during-reconcile race (WI §6.1.10 chaos #12).
        if row.deleted_at_ms.is_some() {
            return Ok(ReconcileDecision::SkippedSoftDeleted);
        }
        // Compute expected via json_each-equivalent membership.
        let expected =
            self.refcount_source
                .expected_refcount(tenant_id, &row.digest, snapshot_at_ms)?;

        // Orphan R2 detection (WI §1.4). Surface BEFORE the drift
        // comparison so refcount mutation never amplifies an
        // already-inconsistent state. The cross-reference check
        // (R2 missing AND blob_meta row present) is the canonical
        // signal forwarded to the reclaim task.
        if !row.r2_present && row.deleted_at_ms.is_none() {
            return Ok(ReconcileDecision::OrphanR2Detected {
                refcount: row.stored_refcount,
            });
        }

        let now = self.clock.now_ms();

        if expected == row.stored_refcount {
            // No drift path. Audit emit (forensic trail) + return.
            self.audit.emit(GcAuditRecord {
                event_type: GcEventType::RefcountReconciled,
                run_id,
                tenant_id,
                region,
                status: GcStatus::Running,
                from_phase: Some(GcPhase::Reconcile),
                to_phase: Some(GcPhase::Reconcile),
                created_by_request_id: "cron".to_owned(),
                reason: "no_drift",
                now_ms: now,
            })?;
            return Ok(ReconcileDecision::NoDrift {
                refcount: row.stored_refcount,
            });
        }

        // Drift detected. Compute the per-tenant drift count + percent
        // including the row under inspection (`+ 1` on both sides).
        let drift_count = tenant_drift_count_so_far.saturating_add(1);
        let blobs_scanned = tenant_blobs_scanned_so_far.saturating_add(1);
        // Safe: blobs_scanned >= 1 by construction.
        #[allow(
            clippy::cast_precision_loss,
            reason = "drift counts are bounded by tenant blob count which \
                      fits comfortably within f64 mantissa precision for \
                      practical tenants ≤ 2^53 blobs."
        )]
        let drift_percent = drift_count as f64 / blobs_scanned as f64;

        // Auto-fix gate (Lote 10.6bis P0-6 dual-condition).
        if auto_fix_gate_fires(drift_count, drift_percent, &self.config) {
            // Fail-closed audit emit BEFORE mutation.
            self.audit.emit(GcAuditRecord {
                event_type: GcEventType::RefcountAutoFixed,
                run_id,
                tenant_id,
                region,
                status: GcStatus::Running,
                from_phase: Some(GcPhase::Reconcile),
                to_phase: Some(GcPhase::Reconcile),
                created_by_request_id: "cron".to_owned(),
                reason: "auto_fix_dual_condition_gate",
                now_ms: now,
            })?;
            // Conditional UPDATE — anti-ping-pong predicate
            // `WHERE refcount = stored_refcount` re-checks the row to
            // protect against a concurrent UpdateAR.
            let fired = self.blob_meta.conditional_set_refcount(
                tenant_id,
                &row.digest,
                row.stored_refcount,
                expected,
            )?;
            if !fired {
                // Concurrent winner: a customer UpdateAR raced and the
                // stored refcount has already changed. The conditional
                // failed; leave the row alone — next reconcile cycle
                // re-evaluates against the new stored refcount.
                return Ok(ReconcileDecision::PausedForManualReview {
                    stored_refcount: row.stored_refcount,
                    expected_refcount: expected,
                    drift_count,
                    drift_percent,
                });
            }
            return Ok(ReconcileDecision::AutoFixed {
                from_refcount: row.stored_refcount,
                to_refcount: expected,
                fixed_at_ms: now,
            });
        }

        // Auto-fix gate rejected → manual review (SEV-1 audit emit).
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::RefcountManualReviewRequired,
            run_id,
            tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::Reconcile),
            to_phase: Some(GcPhase::Reconcile),
            created_by_request_id: "cron".to_owned(),
            reason: "drift_exceeds_auto_fix_gate",
            now_ms: now,
        })?;
        Ok(ReconcileDecision::PausedForManualReview {
            stored_refcount: row.stored_refcount,
            expected_refcount: expected,
            drift_count,
            drift_percent,
        })
    }
}

impl<S, R, B, A, M, K> ReconcilePhase for InMemoryReconcilePhase<S, R, B, A, M, K>
where
    S: GcRunStore,
    R: RefcountSource,
    B: BlobMetaRefcountStore,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: ReconcileClock,
{
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<ReconcileResult, ReconcileError> {
        // 1. Read gc_run + verify region match.
        let run_row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(ReconcileError::RunStore(GcRunStoreError::NotFound(run_id)))?;
        if run_row.region != region {
            return Err(ReconcileError::RegionMismatch {
                run_region: run_row.region,
                caller_region: region,
            });
        }

        // 2. Phase budget deadline + canonical snapshot anchor.
        let phase_start = self.clock.now_ms();
        let deadline_ms = phase_start.saturating_add(self.config.phase_budget_ms());
        // Canonical snapshot_at_ms anchor: an instant strictly greater
        // than the phase_start so writes-up-to-snapshot are visible to
        // the json_each scan; mirrors WI-S06-003 mark phase pattern.
        let snapshot_at_ms = phase_start.saturating_add(1);

        // 3. Iterate blob_meta rows for the tenant.
        let rows = self.blob_meta.snapshot_for_tenant(tenant_id)?;
        let mut blobs_scanned: u64 = 0;
        let mut no_drift_count: u64 = 0;
        let mut auto_fixed_count: u64 = 0;
        let mut manual_review_count: u64 = 0;
        let mut skipped_soft_deleted_count: u64 = 0;
        let mut orphan_r2_count: u64 = 0;
        let mut drifts_detected: u64 = 0;
        let mut audit_events_emitted: u64 = 0;
        for row in &rows {
            // Phase-budget probe per row (cheaper than per mutation;
            // aligns with sweep + physical-delete pattern).
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(ReconcileError::PhaseBudgetExceeded {
                    duration_ms: now_for_probe.saturating_sub(phase_start),
                    budget_ms: self.config.phase_budget_ms(),
                });
            }
            let decision = self.step_row(
                tenant_id,
                run_id,
                region,
                row,
                snapshot_at_ms,
                drifts_detected,
                blobs_scanned,
            )?;
            blobs_scanned = blobs_scanned.saturating_add(1);
            match decision {
                ReconcileDecision::NoDrift { .. } => {
                    no_drift_count = no_drift_count.saturating_add(1);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                ReconcileDecision::AutoFixed { .. } => {
                    auto_fixed_count = auto_fixed_count.saturating_add(1);
                    drifts_detected = drifts_detected.saturating_add(1);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                ReconcileDecision::PausedForManualReview { .. } => {
                    manual_review_count = manual_review_count.saturating_add(1);
                    drifts_detected = drifts_detected.saturating_add(1);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                ReconcileDecision::SkippedSoftDeleted => {
                    skipped_soft_deleted_count = skipped_soft_deleted_count.saturating_add(1);
                }
                ReconcileDecision::OrphanR2Detected { .. } => {
                    orphan_r2_count = orphan_r2_count.saturating_add(1);
                }
            }
        }
        // 4. Final aggregate drift % + SEV computation.
        #[allow(
            clippy::cast_precision_loss,
            reason = "drift counts are bounded by tenant blob count which \
                      fits comfortably within f64 mantissa precision for \
                      practical tenants ≤ 2^53 blobs."
        )]
        let drift_percent = if blobs_scanned == 0 {
            0.0
        } else {
            drifts_detected as f64 / blobs_scanned as f64
        };
        // SEV-2 (global > 0.1%) cannot be computed at the per-tenant
        // layer; the caller aggregates `drift_percent` across tenants
        // and emits the `corelink_gc_refcount_drift_percent{scope="global"}`
        // metric + SEV-2 alert. Pass 0.0 as the global term so this
        // layer only surfaces SEV-1 (per-tenant > 1%) when applicable.
        let sev_level = sev_level_for(0.0, drift_percent, &self.config);

        // 5. Checkpoint counters in gc_run.
        let phase_end = self.clock.now_ms();
        let duration_ms = phase_end.saturating_sub(phase_start);
        self.runs
            .checkpoint(run_id, tenant_id, phase_end, CheckpointDeltas::default())?;

        // 6. Phase duration histogram (canonical metric).
        self.metrics.record_phase_duration_ms(
            GcPhase::Reconcile,
            tenant_id,
            region,
            duration_ms,
        )?;

        Ok(ReconcileResult {
            blobs_scanned,
            no_drift_count,
            auto_fixed_count,
            manual_review_count,
            skipped_soft_deleted_count,
            orphan_r2_count,
            drifts_detected,
            drift_percent,
            sev_level,
            reconcile_duration_ms: duration_ms,
            audit_events_emitted,
            reconcile_started_at_ms: snapshot_at_ms,
        })
    }
}
