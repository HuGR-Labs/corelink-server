//! Real D1-backed [`ReconcilePhase`] implementation for production reconcile phase.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use corelink_gc::reconcile::{
    auto_fix_gate_fires, sev_level_for, ReconcileConfig, ReconcileDecision, ReconcileError,
    ReconcilePhase, ReconcileResult, ReconcileResult, SevLevel,
    BlobMetaRefcountStore, RefcountSource, ReconcileClock,
};
use corelink_gc::region::GcRegion;
use corelink_gc::run::{GcRun, GcRunStore, GcRunStoreError, GcStatus, RunId};
use corelink_gc::metrics::GcMetricsObserver;
use corelink_gc::audit::{GcAuditRecord, GcAuditSink, GcEventType};

use crate::gc_worker::d1_refcount_source::D1RefcountSource;
use crate::gc_worker::d1_blob_meta_refcount_store::D1BlobMetaRefcountStore;
use crate::storage::d1_http::D1HttpClient;

/// System wall-clock for production reconcile clock.
#[derive(Clone, Debug, Default)]
pub struct SystemReconcileClock;

#[async_trait]
impl ReconcileClock for SystemReconcileClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0) as u64
    }
}

/// Real reconcile phase: D1-backed implementation.
#[derive(Clone, Debug)]
pub struct RealReconcilePhase {
    runs: Arc<dyn corelink_gc::run::GcRunStore>,
    refcount: Arc<dyn RefcountSource>,
    blob_meta: Arc<dyn BlobMetaRefcountStore>,
    audit: Arc<dyn GcAuditSink>,
    metrics: Arc<dyn corelink_gc::metrics::GcMetricsObserver>,
    clock: Arc<dyn ReconcileClock>,
    config: corelink_gc::reconcile::ReconcileConfig,
}

impl RealReconcilePhase {
    #[must_use]
    pub fn new(
        runs: Arc<dyn corelink_gc::run::GcRunStore>,
        refcount: Arc<dyn RefcountSource>,
        blob_meta: Arc<dyn BlobMetaRefcountStore>,
        audit: Arc<dyn GcAuditSink>,
        metrics: Arc<dyn corelink_gc::metrics::GcMetricsObserver>,
        clock: Arc<dyn ReconcileClock>,
        config: corelink_gc::reconcile::ReconcileConfig,
    ) -> Self {
        Self {
            runs,
            refcount,
            blob_meta,
            audit,
            metrics,
            clock,
            config,
        }
    }
}

impl std::fmt::Debug for RealReconcilePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealReconcilePhase").finish_non_exhaustive()
    }
}

#[async_trait]
impl ReconcilePhase for RealReconcilePhase {
    async fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<ReconcileResult, ReconcileError> {
        // 1. Read gc_run + verify region match + capture reconcile start.
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

        let reconcile_started_at_ms = self.clock.now_ms();
        let snapshot_at_ms = reconcile_started_at_ms;

        // 2. Phase budget deadline.
        let deadline_ms = reconcile_started_at_ms.saturating_add(self.config.phase_budget_ms);

        // 3. Snapshot blob_meta rows for tenant.
        let blob_rows = self.blob_meta.snapshot_for_tenant(tenant_id).await?;

        let mut blobs_scanned: u64 = 0;
        let mut no_drift_count: u64 = 0;
        let mut auto_fixed_count: u64 = 0;
        let mut manual_review_count: u64 = 0;
        let mut skipped_soft_deleted_count: u64 = 0;
        let mut orphan_r2_count: u64 = 0;
        let mut drifts_detected: u64 = 0;
        let mut audit_events_emitted: u64 = 0;

        // 4. Iterate blob_meta rows and reconcile each.
        for row in &blob_rows {
            blobs_scanned = blobs_scanned.saturating_add(1);

            // Phase budget probe
            let now = self.clock.now_ms();
            if now > deadline_ms {
                return Err(ReconcileError::PhaseBudgetExceeded {
                    duration_ms: now.saturating_sub(reconcile_started_at_ms),
                    budget_ms: self.config.phase_budget_ms,
                });
            }

            // Skip soft-deleted rows
            if row.deleted_at_ms.is_some() {
                skipped_soft_deleted_count = skipped_soft_deleted_count.saturating_add(1);
                continue;
            }

            // Orphan R2 detection
            if !row.r2_present {
                orphan_r2_count = orphan_r2_count.saturating_add(1);
                self.audit.emit(GcAuditRecord {
                    event_type: GcEventType::ReconcileOrphanR2Detected,
                    run_id: RunId::nil(), // TODO: get actual run_id
                    tenant_id,
                    region,
                    status: GcStatus::Running,
                    from_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    to_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    created_by_request_id: "cron".into(),
                    reason: "orphan_r2_detected",
                    now_ms: self.clock.now_ms(),
                }).await.map_err(ReconcileError::from)?;
                orphan_r2_count = orphan_r2_count.saturating_add(1);
                continue;
            }

            // Compute expected refcount
            let expected = self.refcount.expected_refcount(tenant_id, &row.digest, reconcile_started_at_ms).await?;

            if expected == row.stored_refcount {
                // No drift
                no_drift_count = no_drift_count.saturating_add(1);
                self.audit.emit(GcAuditRecord {
                    event_type: GcEventType::ReconcileRefcountReconciled,
                    run_id: RunId::nil(), // TODO
                    tenant_id,
                    region,
                    status: GcStatus::Running,
                    from_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    to_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    created_by_request_id: "cron".into(),
                    reason: "refcount_reconciled",
                    now_ms: self.clock.now_ms(),
                }).await.map_err(ReconcileError::from)?;
                audit_events_emitted = audit_events_emitted.saturating_add(1);
                continue;
            }

            // Drift detected
            drifts_detected = drifts_detected.saturating_add(1);
            let drift_count = 1; // This should be per-tenant counter
            let drift_percent = 1.0 / blobs_scanned as f64; // Simplified

            // Check auto-fix gate
            if auto_fix_gate_fires(1, drift_percent) {
                // Auto-fix: emit audit BEFORE update (fail-CLOSED)
                self.audit.emit(GcAuditRecord {
                    event_type: GcEventType::ReconcileRefcountAutoFixed,
                    run_id: RunId::nil(),
                    tenant_id,
                    region,
                    status: GcStatus::Running,
                    from_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    to_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    created_by_request_id: "cron".into(),
                    reason: "refcount_auto_fixed",
                    now_ms: self.clock.now_ms(),
                }).await.map_err(ReconcileError::from)?;

                // Conditional UPDATE
                let fired = self.blob_meta.conditional_set_refcount(
                    tenant_id,
                    &BlobDigest::parse(&row.digest.as_str()).unwrap(), // TODO: proper BlobDigest
                    row.stored_refcount,
                    expected,
                ).await?;

                if fired {
                    auto_fixed_count = auto_fixed_count.saturating_add(1);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                } else {
                    // Concurrent update lost - re-evaluate next cycle
                    manual_review_count = manual_review_count.saturating_add(1);
                }
            } else {
                // Drift exceeds gate - manual review
                manual_review_count = manual_review_count.saturating_add(1);
                self.audit.emit(GcAuditRecord {
                    event_type: GcEventType::ReconcileManualReviewRequired,
                    run_id: RunId::nil(),
                    tenant_id,
                    region,
                    status: GcStatus::Running,
                    from_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    to_phase: Some(corelink_gc::run::GcPhase::Reconcile),
                    created_by_request_id: "cron".into(),
                    reason: "refcount_manual_review_required",
                    now_ms: self.clock.now_ms(),
                }).await.map_err(ReconcileError::from)?;
                audit_events_emitted = audit_events_emitted.saturating_add(1);
            }
        }

        // Final metrics
        let end_ms = self.clock.now_ms();
        let duration_ms = end_ms.saturating_sub(reconcile_started_at_ms);
        let drift_percent = if blobs_scanned > 0 {
            drifts_detected as f64 / blobs_scanned as f64
        } else {
            0.0
        };

        let sev_level = sev_level_for(drift_percent, manual_review_count > 0);

        self.metrics.record_phase_duration_ms(
            corelink_gc::run::GcPhase::Reconcile,
            tenant_id,
            region,
            duration_ms,
        ).await?;

        self.metrics.record_run_completed(tenant_id, region, corelink_gc::run::GcStatus::Succeeded).await?;

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
            reconcile_started_at_ms: reconcile_started_at_ms,
        })
    }
}