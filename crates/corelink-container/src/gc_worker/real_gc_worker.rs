//! Production GC worker implementations.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use corelink_gc::{
    audit::{GcAuditRecord, GcAuditSink, GcEventType},
    degrade::{DegradeKind, DegradeProbe},
    error::GcError,
    mark::{InMemoryMarkPhase, MarkConfig, MarkPhase},
    metrics::{GcMetricsObserver, NoOpGcMetrics},
    reconcile::{InMemoryReconcilePhase, ReconcileConfig},
    run::{
        CheckpointDeltas, FailureContext, GcPhase, GcRun, GcRunStore, GcRunStoreError,
        GcRegion, GcRun as GcRunStruct, GcStatus, RunId,
    },
    sweep::{InMemorySweepPhase, SweepConfig},
    physical_delete::{InMemoryPhysicalDeletePhase, PhysicalDeleteConfig},
    reconcile::{InMemoryReconcilePhase, ReconcileConfig as ReconcileConfig2},
    worker::GcWorker,
};

use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;

use crate::gc_worker::d1_gc_run_store::D1GcRunStore;
use crate::gc_worker::d1_degrade_probe::D1DegradeProbe;
use crate::gc_worker::d1_gc_audit_sink::D1GcAuditSink;
use crate::gc_worker::d1_reachable_set_source::D1ReachableSetSource;
use crate::gc_worker::d1_gc_candidates_store::D1GcCandidatesStore;
use crate::gc_worker::d1_blob_meta_store::D1BlobMetaStore;
use crate::gc_worker::d1_ac_reference_index::D1AcReferenceIndex;
use crate::gc_worker::d1_blob_meta_purge_store::D1BlobMetaPurgeStore;
use crate::gc_worker::d1_refcount_source::D1RefcountSource;
use crate::gc_worker::d1_blob_meta_refcount_store::D1BlobMetaRefcountStore;

/// Real mark phase: D1-backed implementation.
///
/// Scans reachable set (blob_meta + ac_meta + manifest_chunks),
/// computes candidates, and persists gc_candidates rows.
#[derive(Clone, Debug)]
pub struct RealMarkPhase {
    runs: Arc<D1GcRunStore>,
    source: Arc<D1ReachableSetSource>,
    candidates: Arc<D1GcCandidatesStore>,
    audit: Arc<D1GcAuditSink>,
    metrics: Arc<dyn GcMetricsObserver>,
}

impl RealMarkPhase {
    #[must_use]
    pub fn new(
        runs: Arc<D1GcRunStore>,
        source: Arc<D1ReachableSetSource>,
        candidates: Arc<D1GcCandidatesStore>,
        audit: Arc<D1GcAuditSink>,
        metrics: Arc<dyn GcMetricsObserver>,
    ) -> Self {
        Self {
            runs,
            source,
            candidates,
            audit,
            metrics,
        }
    }
}

#[async_trait]
impl MarkPhase for RealMarkPhase {
    async fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<corelink_gc::mark::MarkResult, corelink_gc::mark::MarkError> {
        let mark_config = MarkConfig::default();
        let clock = Arc::new(SystemMarkClock);
        let mark = InMemoryMarkPhase::new(
            self.runs.clone(),
            self.source.clone(),
            self.candidates.clone(),
            self.audit.clone(),
            self.metrics.clone(),
            clock,
            mark_config,
            24 * 60 * 60 * 1000,
        );
        mark.execute(run_id, tenant_id, region).await
    }
}

/// System wall-clock for production mark phase.
#[derive(Clone, Debug, Default)]
pub struct SystemMarkClock;

impl corelink_gc::mark::MarkClock for SystemMarkClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
    fn advance_jitter(&self, _ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(_ms));
    }
}

/// Production GC worker: real D1 + R2 implementation.
#[derive(Clone)]
pub struct RealGcWorker {
    runs: Arc<D1GcRunStore>,
    degrade: Arc<D1DegradeProbe>,
    audit: Arc<D1GcAuditSink>,
    metrics: Arc<dyn GcMetricsObserver>,
}

impl RealGcWorker {
    #[must_use]
    pub fn new(
        runs: Arc<D1GcRunStore>,
        degrade: Arc<D1DegradeProbe>,
        audit: Arc<D1GcAuditSink>,
        metrics: Arc<dyn GcMetricsObserver>,
    ) -> Self {
        Self {
            runs,
            degrade,
            audit,
            metrics,
        }
    }
}

impl std::fmt::Debug for RealGcWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealGcWorker").finish_non_exhaustive()
    }
}

#[async_trait]
impl GcWorker for RealGcWorker {
    async fn execute_run(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
        now_ms: u64,
    ) -> Result<corelink_gc::worker::WorkerStepOutcome, GcError> {
        let chain: [(GcPhase, GcPhase); 5] = [
            (GcPhase::Idle, GcPhase::Mark),
            (GcPhase::Mark, GcPhase::Sweep),
            (GcPhase::Sweep, GcPhase::PhysicalDelete),
            (GcPhase::PhysicalDelete, GcPhase::Reconcile),
            (GcPhase::Reconcile, GcPhase::Completed),
        ];
        let mut wall_clock = now_ms;
        for (from, to) in chain {
            wall_clock = wall_clock.saturating_add(1);
            if self.degrade.probe().await?.kind.requires_abort() {
                return self.finalize_aborted(run_id, tenant_id, config, wall_clock, from).await;
            }
            self.runs.transition_phase(run_id, tenant_id, to, wall_clock).await?;
            let deltas = self.run_phase(&to, run_id, tenant_id, config, wall_clock).await?;
            if deltas != CheckpointDeltas::default() {
                self.runs.checkpoint(run_id, tenant_id, wall_clock, deltas).await?;
            }
            self.metrics.record_phase_duration_ms(to, tenant_id, config.region(), 0).await?;
            wall_clock = wall_clock.saturating_add(1);
        }
        if self.degrade.probe().await?.kind.requires_abort() {
            return self.finalize_aborted(run_id, tenant_id, config, wall_clock, GcPhase::Reconcile).await;
        }
        self.runs.finalize(run_id, tenant_id, GcStatus::Succeeded, wall_clock, None).await?;
        self.metrics.record_run_completed(tenant_id, config.region(), GcStatus::Succeeded).await?;
        let final_run = self.runs.lookup(run_id, tenant_id).await?.ok_or_else(|| {
            GcError::RunStore(GcRunStoreError::Backend("lookup failed post-finalize".to_owned()))
        })?;
        Ok(corelink_gc::worker::WorkerStepOutcome {
            final_run,
            succeeded: true,
            aborted_via_degrade: false,
        })
    }
}

impl RealGcWorker {
    async fn finalize_aborted(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
        now_ms: u64,
        from_phase: GcPhase,
    ) -> Result<corelink_gc::worker::WorkerStepOutcome, GcError> {
        self.runs.finalize(run_id, tenant_id, GcStatus::Aborted, now_ms, Some(FailureContext {
            failed_phase: from_phase,
            failed_reason: "degrade_mode_gc_pause".to_owned(),
        })).await?;
        self.metrics.record_run_completed(tenant_id, config.region(), GcStatus::Aborted).await?;
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::RunAborted,
            run_id,
            tenant_id,
            region: config.region(),
            status: GcStatus::Aborted,
            from_phase: Some(from_phase),
            to_phase: None,
            created_by_request_id: "cron".into(),
            reason: "degrade_mode_gc_pause",
            now_ms,
        }).await.map_err(GcError::from)?;
        let final_run = self.runs.lookup(run_id, tenant_id).await?.ok_or_else(|| {
            GcError::RunStore(GcRunStoreError::Backend("lookup failed post-finalize".to_owned()))
        })?;
        Ok(corelink_gc::worker::WorkerStepOutcome {
            final_run,
            succeeded: false,
            aborted_via_degrade: true,
        })
    }

    async fn run_phase(
        &self,
        phase: &GcPhase,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
        now_ms: u64,
    ) -> Result<CheckpointDeltas, GcError> {
        match phase {
            GcPhase::Mark => {
                self.run_mark_phase(run_id, tenant_id, config).await
            }
            GcPhase::Sweep => {
                self.run_sweep_phase(run_id, tenant_id, config).await
            }
            GcPhase::PhysicalDelete => {
                self.run_physical_delete_phase(run_id, tenant_id, config).await
            }
            GcPhase::Reconcile => {
                self.run_reconcile_phase(run_id, tenant_id, config).await
            }
            _ => Ok(CheckpointDeltas::default()),
        }
    }

    async fn run_mark_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
    ) -> Result<CheckpointDeltas, GcError> {
        let phase = RealMarkPhase::new(
            self.runs.clone(),
            Arc::new(D1ReachableSetSource::new(self.runs.get_d1_client())),
            Arc::new(D1GcCandidatesStore::new(self.runs.get_d1_client())),
            self.audit.clone(),
            self.metrics.clone(),
        );
        let result = phase.execute(run_id, tenant_id, config.region()).await?;
        Ok(CheckpointDeltas {
            blobs_marked_delta: result.reachable_blobs_count,
            ..Default::default()
        })
    }

    async fn run_sweep_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
    ) -> Result<CheckpointDeltas, GcError> {
        // Sweep phase: read candidates, check AC refs, soft-delete orphans
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id).await?;
        let mut blobs_swept = 0u64;
        let mut bytes_reclaimed = 0u64;
        for candidate in &candidates {
            if candidate.status != corelink_gc::mark::CandidateStatus::Candidate {
                continue;
            }
            let witness = self.ac_index.find_re_reference(
                tenant_id,
                &candidate.digest,
                candidate.mark_started_at_ms,
            ).await?;
            if witness.is_some() {
                // Protected by recent AC ref — skip
                continue;
            }
            // Check if blob_meta row exists and is not already deleted
            let blob_row = self.blob_meta.lookup(tenant_id, &candidate.digest).await?;
            if let Some(row) = blob_row {
                if row.deleted_at_ms.is_some() {
                    continue;
                }
                // Soft-delete
                self.blob_meta.soft_delete(tenant_id, &candidate.digest, now_ms).await?;
                self.candidates.transition_status(
                    tenant_id,
                    &candidate.digest,
                    run_id,
                    corelink_gc::mark::CandidateStatus::Candidate,
                    corelink_gc::mark::CandidateStatus::Swept,
                    now_ms,
                    None,
                ).await?;
                blobs_swept += 1;
                bytes_reclaimed += row.size_bytes;
            }
        }
        Ok(CheckpointDeltas {
            blobs_swept_delta: blobs_swept,
            bytes_reclaimed_delta: bytes_reclaimed,
            ..Default::default()
        })
    }

    async fn run_physical_delete_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
    ) -> Result<CheckpointDeltas, GcError> {
        // Physical delete: post-grace R2 DeleteObject + blob_meta purge
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id).await?;
        let mut blobs_deleted = 0u64;
        let mut bytes_reclaimed = 0u64;
        for candidate in &candidates {
            if candidate.status != corelink_gc::mark::CandidateStatus::Swept {
                continue;
            }
            let purge_state = self.purge.get_purge_state(tenant_id, &candidate.digest).await?;
            if let Some(state) = purge_state {
                if state.refcount > 0 {
                    continue;
                }
                // Physical delete from R2
                self.r2.delete(tenant_id, &candidate.digest, config.region()).await?;
                // Purge blob_meta row
                self.purge.purge(tenant_id, &candidate.digest, now_ms).await?;
                self.candidates.transition_status(
                    tenant_id,
                    &candidate.digest,
                    run_id,
                    corelink_gc::mark::CandidateStatus::Swept,
                    corelink_gc::mark::CandidateStatus::PhysicallyDeleted,
                    now_ms,
                    Some("physically_deleted".to_string()),
                ).await?;
                blobs_deleted += 1;
                bytes_reclaimed += state.size_bytes;
            }
        }
        Ok(CheckpointDeltas {
            blobs_physically_deleted_delta: blobs_deleted,
            bytes_reclaimed_delta: bytes_reclaimed,
            ..Default::default()
        })
    }

    async fn run_reconcile_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        config: &corelink_gc::schedule::ScheduleConfig,
    ) -> Result<CheckpointDeltas, GcError> {
        // Reconcile phase: refcount drift detection + auto-fix
        let phase = RealReconcilePhase::new(
            self.runs.clone(),
            Arc::new(D1RefcountSource::new(self.runs.get_d1_client())),
            Arc::new(D1BlobMetaRefcountStore::new(self.runs.get_d1_client())),
            self.audit.clone(),
            self.metrics.clone(),
            Arc::new(SystemReconcileClock),
            ReconcileConfig::default(),
        );
        let result = phase.execute(run_id, tenant_id, config.region()).await?;
        Ok(CheckpointDeltas {
            bytes_reclaimed_delta: result.auto_fixed_count,
            ..Default::default()
        })
    }
}

use uuid::Uuid;
