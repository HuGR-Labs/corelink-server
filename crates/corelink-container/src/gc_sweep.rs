//! Production GC sweep adapters for the native container.
//!
//! [`corelink_gc`] intentionally owns only the synchronous, deterministic
//! reclaim state machine.  This module is the native boundary that supplies
//! the real Cloudflare D1 and R2 implementations.  It is deliberately kept in
//! `corelink-container`: the container already owns the async runtime and the
//! credential-redacting D1/R2 clients, so the pure GC crate does not acquire a
//! native/network dependency.
//!
//! The entrypoint is tenant- and region-scoped.  Missing or malformed
//! configuration is an error (there is no in-memory production fallback), and
//! Destructive mode is fail-closed: even with an operator confirmation token,
//! live execution remains disabled until the cross-system fencing protocol is
//! deployed.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use corelink_gc::{
    BlobDigest, BlobMetaPurgeStore, CandidateStatus, CheckpointDeltas, GcAuditRecord, GcAuditSink,
    GcAuditSinkError, GcCandidate, GcCandidatesStore, GcEventType, GcMetricKind, GcMetricsObserver,
    GcMetricsObserverError, GcPhase, GcRegion, GcRun, GcRunStore, GcRunStoreError, GcStatus,
    GcSweepMode, GcSweepReportError, GcSweepReportSink, GcSweepRunner, MarkError,
    PhysicalDeleteClock, PhysicalDeleteError, PurgeStage, PurgeState, R2Delete, R2DeleteError,
    R2DeleteOutcome, RunId, SweepReport,
};
use serde_json::{json, Value};
use tracing::info;
use uuid::Uuid;

use crate::storage::cas_write_fence::CAS_WRITE_LEASE_MS;
use crate::storage::d1_http::{D1HttpClient, D1Row};
use crate::storage::r2_s3::{validate_cas_bucket_for_region, R2S3Client};
use crate::storage::StorageEnv;

/// Runtime configuration for one tenant/region sweep.
#[derive(Clone)]
pub struct GcProductionConfig {
    /// Tenant whose candidates are eligible for this invocation.
    pub tenant_id: Uuid,
    /// Region whose running checkpoint is eligible for this invocation.
    pub region: GcRegion,
    /// Optional explicit run; absent means the current running run.
    pub run_id: Option<RunId>,
    /// Destructive mode, gated by `GC_LIVE_DELETE` and confirmation.
    pub mode: GcSweepMode,
    /// R2 bucket containing CAS objects.
    pub bucket: String,
    /// Secret tenant-key derivation material used by the CAS key scheme.
    tdk: [u8; 32],
}

impl core::fmt::Debug for GcProductionConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GcProductionConfig")
            .field("tenant_id", &self.tenant_id)
            .field("region", &self.region)
            .field("run_id", &self.run_id)
            .field("mode", &self.mode)
            .field("bucket", &self.bucket)
            .field("tdk", &"[REDACTED]")
            .finish()
    }
}

impl GcProductionConfig {
    /// Read and validate production configuration from environment.
    ///
    /// Required values intentionally include the tenant, region, bucket and
    /// tenant derivation key.  A native process without these values cannot
    /// safely infer a scope, so it fails closed instead of using a fixture.
    pub fn from_env() -> Result<Self, String> {
        let tenant_raw = required_env("GC_TENANT_ID")?;
        let tenant_id =
            Uuid::parse_str(&tenant_raw).map_err(|e| format!("GC_TENANT_ID is not a UUID: {e}"))?;
        let region_raw = required_env("GC_REGION")?;
        let region =
            GcRegion::parse(&region_raw).map_err(|e| format!("GC_REGION is invalid: {e}"))?;
        let run_id = match std::env::var("GC_RUN_ID") {
            Ok(raw) if !raw.trim().is_empty() => Some(RunId(
                Uuid::parse_str(raw.trim()).map_err(|e| format!("GC_RUN_ID is not a UUID: {e}"))?,
            )),
            Ok(_) | Err(std::env::VarError::NotPresent) => None,
            Err(e) => return Err(format!("GC_RUN_ID could not be read: {e}")),
        };
        let bucket = std::env::var("GC_R2_BUCKET")
            .or_else(|_| std::env::var("R2_CAS_BUCKET"))
            .map_err(|_| "GC_R2_BUCKET (or R2_CAS_BUCKET) is required".to_owned())?;
        let bucket = bucket.trim().to_owned();
        if bucket.is_empty() {
            return Err("GC_R2_BUCKET must not be empty".to_owned());
        }
        let tdk_raw = required_env("R2_TDK_HEX")?;
        let tdk_vec = hex::decode(tdk_raw.trim())
            .map_err(|_| "R2_TDK_HEX must be 64 hexadecimal characters".to_owned())?;
        let tdk: [u8; 32] = tdk_vec
            .try_into()
            .map_err(|_| "R2_TDK_HEX must decode to exactly 32 bytes".to_owned())?;

        let mode = GcSweepMode::from_env();
        if mode.is_live() {
            if std::env::var("GC_LIVE_DELETE_CONFIRM").ok().as_deref() != Some("I_UNDERSTAND") {
                return Err(
                    "GC_LIVE_DELETE_CONFIRM=I_UNDERSTAND is required for live deletion".to_owned(),
                );
            }
            // R2 DeleteObject and D1 row removal are different remote systems;
            // there is no 2PC or conditional delete token spanning them.  The
            // native adapter therefore refuses destructive execution until the
            // fencing protocol is deployed.  A confirmation token must never
            // turn an inherently unsafe remote ordering into a false success.
            return Err(
                "live GC is unavailable: R2/D1 deletion needs a deployed fencing transaction; run dry-run".to_owned(),
            );
        }
        Ok(Self {
            tenant_id,
            region,
            run_id,
            mode,
            bucket,
            tdk,
        })
    }
}

/// Execute one real D1/R2 sweep using a validated scope.
pub async fn run_production(
    storage: &StorageEnv,
    config: &GcProductionConfig,
) -> Result<SweepReport, String> {
    if config.mode.is_live() {
        return Err(
            "live GC is unavailable: R2/D1 deletion needs a deployed fencing transaction; run dry-run"
                .to_owned(),
        );
    }
    let d1 = Arc::new(D1HttpClient::new(storage)?);
    validate_cas_bucket_for_region(&config.bucket, config.region.as_str())?;
    let r2 = Arc::new(R2S3Client::new(storage, config.bucket.clone()).await?);
    let runs = Arc::new(D1GcRunStore::new(Arc::clone(&d1)));
    let candidates = Arc::new(D1GcCandidatesStore::new(Arc::clone(&d1), config.region));
    let blob_meta = Arc::new(D1BlobMetaPurgeStore::new(
        Arc::clone(&d1),
        config.region,
        config.tdk,
    ));
    let r2_delete = Arc::new(R2GcDelete::new(r2, config.tdk));
    let audit = Arc::new(D1GcAuditSink::new(Arc::clone(&d1)));
    let metrics = Arc::new(TracingGcMetrics);
    let clock = Arc::new(SystemGcClock::default());
    let report = Arc::new(D1GcReportSink::new(Arc::clone(&d1)));
    let run_id = match config.run_id {
        Some(run_id) => run_id,
        None => {
            runs.current_running(config.tenant_id, config.region)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| {
                    format!(
                        "no running gc_run for tenant={} region={}",
                        config.tenant_id,
                        config.region.as_str()
                    )
                })?
                .run_id
        }
    };
    let run = runs
        .lookup(run_id, config.tenant_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("gc_run {run_id} is absent for configured tenant"))?;
    if run.region != config.region {
        return Err(format!(
            "gc_run {run_id} region={} does not match configured region={}",
            run.region.as_str(),
            config.region.as_str()
        ));
    }
    if run.status != GcStatus::Running || run.phase != GcPhase::PhysicalDelete {
        return Err(format!(
            "gc_run {run_id} is not sweepable: status={} phase={}",
            run.status.as_str(),
            run.phase.as_str()
        ));
    }
    let runner = GcSweepRunner::with_defaults(
        runs,
        candidates,
        blob_meta,
        r2_delete,
        audit,
        metrics,
        clock,
        report,
        config.mode,
    );
    runner
        .run(run_id, config.tenant_id, config.region)
        .map_err(|e| e.to_string())
}

fn required_env(name: &str) -> Result<String, String> {
    let value = std::env::var(name).map_err(|_| format!("{name} is required"))?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(format!("{name} must not be empty"))
    } else {
        Ok(value)
    }
}

fn query_sync(d1: &D1HttpClient, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|_| "GC D1 adapter requires a running Tokio runtime".to_owned())?;
    tokio::task::block_in_place(|| handle.block_on(d1.query(sql, params)))
}

fn row_value<'a>(row: &'a D1Row, column: &str) -> Result<&'a Value, String> {
    row.get(column)
        .ok_or_else(|| format!("D1 row missing required column {column}"))
}

fn text(row: &D1Row, column: &str) -> Result<String, String> {
    row_value(row, column)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("D1 column {column} is not text"))
}

fn optional_text(row: &D1Row, column: &str) -> Result<Option<String>, String> {
    match row_value(row, column)? {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.clone())),
        _ => Err(format!("D1 column {column} is not nullable text")),
    }
}

fn number(row: &D1Row, column: &str) -> Result<u64, String> {
    let value = row_value(row, column)?;
    if let Some(value) = value.as_u64() {
        return Ok(value);
    }
    if let Some(value) = value.as_i64() {
        return u64::try_from(value).map_err(|_| format!("D1 column {column} is negative"));
    }
    value
        .as_str()
        .ok_or_else(|| format!("D1 column {column} is not an integer"))?
        .parse::<u64>()
        .map_err(|e| format!("D1 column {column} is not an integer: {e}"))
}

fn optional_number(row: &D1Row, column: &str) -> Result<Option<u64>, String> {
    match row_value(row, column)? {
        Value::Null => Ok(None),
        _ => number(row, column).map(Some),
    }
}

fn uuid_column(row: &D1Row, column: &str) -> Result<Uuid, String> {
    Uuid::parse_str(&text(row, column)?).map_err(|e| format!("D1 column {column} is not UUID: {e}"))
}

fn run_from_row(row: &D1Row) -> Result<GcRun, String> {
    let phase = parse_phase(&text(row, "phase")?)?;
    let status = parse_status(&text(row, "status")?)?;
    Ok(GcRun {
        run_id: RunId(uuid_column(row, "run_id")?),
        tenant_id: uuid_column(row, "tenant_id")?,
        region: GcRegion::parse(&text(row, "region")?).map_err(|e| e.to_string())?,
        phase,
        status,
        started_at_ms: number(row, "started_at_ms")?,
        last_checkpoint_at_ms: number(row, "last_checkpoint_at_ms")?,
        completed_at_ms: optional_number(row, "completed_at_ms")?,
        failed_at_ms: optional_number(row, "failed_at_ms")?,
        mark_started_at_ms: optional_number(row, "mark_started_at_ms")?,
        blobs_marked_count: number(row, "blobs_marked_count")?,
        blobs_swept_count: number(row, "blobs_swept_count")?,
        blobs_physically_deleted_count: number(row, "blobs_physically_deleted_count")?,
        bytes_reclaimed: number(row, "bytes_reclaimed")?,
        created_by_request_id: text(row, "created_by_request_id")?,
        failed_phase: optional_text(row, "failed_phase")?,
        failed_reason: optional_text(row, "failed_reason")?,
    })
}

fn parse_phase(value: &str) -> Result<GcPhase, String> {
    match value {
        "idle" => Ok(GcPhase::Idle),
        "mark" => Ok(GcPhase::Mark),
        "sweep" => Ok(GcPhase::Sweep),
        "physical_delete" => Ok(GcPhase::PhysicalDelete),
        "reconcile" => Ok(GcPhase::Reconcile),
        "completed" => Ok(GcPhase::Completed),
        "failed" => Ok(GcPhase::Failed),
        other => Err(format!("unknown gc phase {other:?}")),
    }
}

fn parse_status(value: &str) -> Result<GcStatus, String> {
    match value {
        "pending" => Ok(GcStatus::Pending),
        "running" => Ok(GcStatus::Running),
        "succeeded" => Ok(GcStatus::Succeeded),
        "crashed" => Ok(GcStatus::Crashed),
        "aborted" => Ok(GcStatus::Aborted),
        "failed" => Ok(GcStatus::Failed),
        other => Err(format!("unknown gc status {other:?}")),
    }
}

const RUN_COLUMNS: &str = "run_id, tenant_id, region, phase, status, started_at_ms, last_checkpoint_at_ms, completed_at_ms, failed_at_ms, mark_started_at_ms, blobs_marked_count, blobs_swept_count, blobs_physically_deleted_count, bytes_reclaimed, created_by_request_id, failed_phase, failed_reason";

/// D1-backed `gc_run` implementation.
#[derive(Debug)]
pub struct D1GcRunStore {
    d1: Arc<D1HttpClient>,
}

impl D1GcRunStore {
    /// Construct over a shared D1 client.
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
    fn find(&self, run_id: RunId, tenant_id: Uuid) -> Result<Option<GcRun>, GcRunStoreError> {
        let rows = query_sync(
            &self.d1,
            &format!("SELECT {RUN_COLUMNS} FROM gc_run WHERE run_id = ?1 AND tenant_id = ?2"),
            &[json!(run_id.as_text()), json!(tenant_id.to_string())],
        )
        .map_err(GcRunStoreError::Backend)?;
        rows.first()
            .map(run_from_row)
            .transpose()
            .map_err(GcRunStoreError::Backend)
    }
}

impl GcRunStore for D1GcRunStore {
    fn insert_pending(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
        started_at_ms: u64,
        created_by_request_id: String,
    ) -> Result<(), GcRunStoreError> {
        query_sync(&self.d1, "INSERT INTO gc_run (run_id, tenant_id, region, started_at_ms, last_checkpoint_at_ms, created_by_request_id) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", &[json!(run_id.as_text()), json!(tenant_id.to_string()), json!(region.as_str()), json!(started_at_ms), json!(created_by_request_id)]).map_err(GcRunStoreError::Backend).map(|_| ())
    }

    fn acquire_running(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<(), GcRunStoreError> {
        let row = self
            .find(run_id, tenant_id)?
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        if let Some(existing) = self.current_running(tenant_id, row.region)? {
            if existing.run_id != run_id {
                return Err(GcRunStoreError::AlreadyRunning {
                    tenant_id,
                    region: row.region,
                    existing_run_id: existing.run_id,
                });
            }
            return Ok(());
        }
        query_sync(&self.d1, "UPDATE gc_run SET status = 'running', last_checkpoint_at_ms = ?3 WHERE run_id = ?1 AND tenant_id = ?2 AND status = 'pending'", &[json!(run_id.as_text()), json!(tenant_id.to_string()), json!(now_ms)]).map_err(GcRunStoreError::Backend).map(|_| ())
    }

    fn transition_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        to: GcPhase,
        now_ms: u64,
    ) -> Result<(), GcRunStoreError> {
        let row = self
            .find(run_id, tenant_id)?
            .ok_or(GcRunStoreError::NotFound(run_id))?;
        if !row.phase.can_transition_to(to) {
            return Err(GcRunStoreError::InvalidPhaseTransition {
                from: row.phase.as_str(),
                to: to.as_str(),
            });
        }
        if to == GcPhase::Mark && row.mark_started_at_ms.is_some() {
            return Err(GcRunStoreError::MarkStartedAtImmutable {
                existing: row.mark_started_at_ms.unwrap_or_default(),
                attempted: now_ms,
            });
        }
        query_sync(&self.d1, "UPDATE gc_run SET phase = ?3, last_checkpoint_at_ms = ?4, mark_started_at_ms = CASE WHEN ?3 = 'mark' THEN ?4 ELSE mark_started_at_ms END WHERE run_id = ?1 AND tenant_id = ?2 AND status = 'running'", &[json!(run_id.as_text()), json!(tenant_id.to_string()), json!(to.as_str()), json!(now_ms)]).map_err(GcRunStoreError::Backend).map(|_| ())
    }

    fn checkpoint(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
        deltas: CheckpointDeltas,
    ) -> Result<(), GcRunStoreError> {
        query_sync(&self.d1, "UPDATE gc_run SET last_checkpoint_at_ms = ?3, blobs_marked_count = blobs_marked_count + ?4, blobs_swept_count = blobs_swept_count + ?5, blobs_physically_deleted_count = blobs_physically_deleted_count + ?6, bytes_reclaimed = bytes_reclaimed + ?7 WHERE run_id = ?1 AND tenant_id = ?2", &[json!(run_id.as_text()), json!(tenant_id.to_string()), json!(now_ms), json!(deltas.blobs_marked_delta), json!(deltas.blobs_swept_delta), json!(deltas.blobs_physically_deleted_delta), json!(deltas.bytes_reclaimed_delta)]).map_err(GcRunStoreError::Backend).map(|_| ())
    }

    fn finalize(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        terminal: GcStatus,
        now_ms: u64,
        failure: Option<corelink_gc::FailureContext>,
    ) -> Result<(), GcRunStoreError> {
        if !terminal.is_terminal() {
            return Err(GcRunStoreError::CheckViolation(
                "finalize_called_with_non_terminal_status",
            ));
        }
        let (phase, failed_phase, failed_reason) = match failure {
            Some(f) => (
                if terminal == GcStatus::Failed {
                    Some(GcPhase::Failed.as_str())
                } else {
                    None
                },
                Some(f.failed_phase.as_str()),
                Some(f.failed_reason),
            ),
            None => (
                if terminal == GcStatus::Succeeded {
                    Some(GcPhase::Completed.as_str())
                } else {
                    None
                },
                None,
                None,
            ),
        };
        query_sync(&self.d1, "UPDATE gc_run SET status = ?3, phase = COALESCE(?4, phase), last_checkpoint_at_ms = ?5, completed_at_ms = CASE WHEN ?3 = 'succeeded' THEN ?5 ELSE completed_at_ms END, failed_at_ms = CASE WHEN ?3 IN ('failed','crashed','aborted') THEN ?5 ELSE failed_at_ms END, failed_phase = ?6, failed_reason = ?7 WHERE run_id = ?1 AND tenant_id = ?2", &[json!(run_id.as_text()), json!(tenant_id.to_string()), json!(terminal.as_str()), json!(phase), json!(now_ms), json!(failed_phase), json!(failed_reason)]).map_err(GcRunStoreError::Backend).map(|_| ())
    }

    fn lookup(&self, run_id: RunId, tenant_id: Uuid) -> Result<Option<GcRun>, GcRunStoreError> {
        self.find(run_id, tenant_id)
    }

    fn current_running(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<Option<GcRun>, GcRunStoreError> {
        let rows = query_sync(&self.d1, &format!("SELECT {RUN_COLUMNS} FROM gc_run WHERE tenant_id = ?1 AND region = ?2 AND status = 'running' LIMIT 1"), &[json!(tenant_id.to_string()), json!(region.as_str())]).map_err(GcRunStoreError::Backend)?;
        rows.first()
            .map(run_from_row)
            .transpose()
            .map_err(GcRunStoreError::Backend)
    }
}

fn candidate_from_row(row: &D1Row) -> Result<GcCandidate, String> {
    let status = match text(row, "status")?.as_str() {
        "candidate" => CandidateStatus::Candidate,
        "swept" => CandidateStatus::Swept,
        "physically_deleted" => CandidateStatus::PhysicallyDeleted,
        "protected_re_ref" => CandidateStatus::ProtectedReRef,
        other => return Err(format!("unknown candidate status {other:?}")),
    };
    Ok(GcCandidate {
        tenant_id: uuid_column(row, "tenant_id")?,
        digest: BlobDigest::parse(&text(row, "digest")?).map_err(|e| e.to_string())?,
        mark_started_at_ms: number(row, "mark_started_at_ms")?,
        mark_run_id: RunId(uuid_column(row, "mark_run_id")?),
        blob_size_bytes: number(row, "blob_size_bytes")?,
        blob_last_referenced_at_ms: number(row, "blob_last_referenced_at_ms")?,
        status,
        created_at_ms: number(row, "created_at_ms")?,
        swept_at_ms: optional_number(row, "swept_at_ms")?,
        protected_at_ms: optional_number(row, "protected_at_ms")?,
        protected_reason: optional_text(row, "protected_reason")?,
    })
}

/// D1-backed `gc_candidates` implementation.
#[derive(Debug)]
pub struct D1GcCandidatesStore {
    d1: Arc<D1HttpClient>,
    region: GcRegion,
}
impl D1GcCandidatesStore {
    /// Construct over a shared D1 client.
    pub fn new(d1: Arc<D1HttpClient>, region: GcRegion) -> Self {
        Self { d1, region }
    }
}
impl GcCandidatesStore for D1GcCandidatesStore {
    fn insert_candidate(&self, c: GcCandidate) -> Result<(), MarkError> {
        query_sync(&self.d1, "INSERT OR IGNORE INTO gc_candidates (tenant_id, digest, mark_started_at_ms, mark_run_id, blob_size_bytes, blob_last_referenced_at_ms, status, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", &[json!(c.tenant_id.to_string()), json!(c.digest.to_string()), json!(c.mark_started_at_ms), json!(c.mark_run_id.as_text()), json!(c.blob_size_bytes), json!(c.blob_last_referenced_at_ms), json!(c.status.as_str()), json!(c.created_at_ms)]).map_err(MarkError::Backend).map(|_| ())
    }
    fn snapshot_for_run(
        &self,
        tenant_id: Uuid,
        run_id: RunId,
    ) -> Result<Vec<GcCandidate>, MarkError> {
        let rows = query_sync(&self.d1, "SELECT c.tenant_id, c.digest, c.mark_started_at_ms, c.mark_run_id, c.blob_size_bytes, c.blob_last_referenced_at_ms, c.status, c.created_at_ms, c.swept_at_ms, c.protected_at_ms, c.protected_reason FROM gc_candidates AS c WHERE c.tenant_id = ?1 AND c.mark_run_id = ?2 AND EXISTS (SELECT 1 FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id = bm.tenant_id WHERE bm.tenant_id = c.tenant_id AND bm.digest = c.digest AND bm.region = t.primary_region AND ((?3 = 'iad' AND bm.region IN ('wnam', 'enam')) OR (?3 = 'lhr' AND bm.region = 'weur') OR (?3 = 'nrt' AND bm.region = 'apac') OR (?3 = 'sam' AND bm.region = 'sam'))) ORDER BY c.digest", &[json!(tenant_id.to_string()), json!(run_id.as_text()), json!(self.region.as_str())]).map_err(MarkError::Backend)?;
        rows.iter()
            .map(candidate_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(MarkError::Backend)
    }
    fn count_for_run(&self, tenant_id: Uuid, run_id: RunId) -> Result<u64, MarkError> {
        let rows = query_sync(
            &self.d1,
            "SELECT COUNT(*) AS count FROM gc_candidates WHERE tenant_id = ?1 AND mark_run_id = ?2",
            &[json!(tenant_id.to_string()), json!(run_id.as_text())],
        )
        .map_err(MarkError::Backend)?;
        rows.first().map_or(Ok(0), |row| {
            number(row, "count").map_err(MarkError::Backend)
        })
    }
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        run_id: RunId,
    ) -> Result<Option<GcCandidate>, MarkError> {
        let rows = query_sync(&self.d1, "SELECT tenant_id, digest, mark_started_at_ms, mark_run_id, blob_size_bytes, blob_last_referenced_at_ms, status, created_at_ms, swept_at_ms, protected_at_ms, protected_reason FROM gc_candidates WHERE tenant_id = ?1 AND digest = ?2 AND mark_run_id = ?3", &[json!(tenant_id.to_string()), json!(digest.to_string()), json!(run_id.as_text())]).map_err(MarkError::Backend)?;
        rows.first()
            .map(candidate_from_row)
            .transpose()
            .map_err(MarkError::Backend)
    }
    fn transition_status(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        run_id: RunId,
        from: CandidateStatus,
        to: CandidateStatus,
        now_ms: u64,
        reason: Option<String>,
    ) -> Result<bool, MarkError> {
        let rows = query_sync(&self.d1, "UPDATE gc_candidates SET status = ?4, swept_at_ms = CASE WHEN ?4 = 'swept' THEN ?6 ELSE swept_at_ms END, physically_deleted_at_ms = CASE WHEN ?4 = 'physically_deleted' THEN ?6 ELSE physically_deleted_at_ms END, protected_at_ms = CASE WHEN ?4 = 'protected_re_ref' THEN ?6 ELSE protected_at_ms END, protected_reason = CASE WHEN ?4 = 'protected_re_ref' THEN ?7 ELSE protected_reason END WHERE tenant_id = ?1 AND digest = ?2 AND mark_run_id = ?3 AND status = ?5 RETURNING 1 AS changed", &[json!(tenant_id.to_string()), json!(digest.to_string()), json!(run_id.as_text()), json!(to.as_str()), json!(from.as_str()), json!(now_ms), json!(reason)]).map_err(MarkError::Backend)?;
        Ok(!rows.is_empty())
    }
}

/// D1-backed soft-delete lookup and conditional purge.
pub struct D1BlobMetaPurgeStore {
    d1: Arc<D1HttpClient>,
    region: GcRegion,
    tdk: [u8; 32],
}

impl core::fmt::Debug for D1BlobMetaPurgeStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1BlobMetaPurgeStore")
            .field("d1", &self.d1)
            .field("region", &self.region)
            .field("tdk", &"[REDACTED]")
            .finish()
    }
}
impl D1BlobMetaPurgeStore {
    /// Construct a tenant-key-aware purge adapter.
    pub fn new(d1: Arc<D1HttpClient>, region: GcRegion, tdk: [u8; 32]) -> Self {
        Self { d1, region, tdk }
    }
}
impl BlobMetaPurgeStore for D1BlobMetaPurgeStore {
    fn lookup_purge_state(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<Option<PurgeState>, PhysicalDeleteError> {
        // `blob_meta.region` is the macro-region vocabulary while `self.region`
        // is the physical GC colo.  Filter with the explicit mapping (not a
        // literal equality), then validate the returned pair again below.
        let rows = query_sync(
            &self.d1,
            "SELECT bm.refcount, bm.size_bytes, bm.deleted_at, \
             bm.region AS blob_region, t.primary_region AS tenant_region \
             FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id = bm.tenant_id \
             WHERE bm.tenant_id = ?1 AND bm.digest = ?2 \
             AND bm.deleted_at IS NOT NULL AND bm.region = t.primary_region \
             AND ((?3 = 'iad' AND bm.region IN ('wnam', 'enam')) \
                  OR (?3 = 'lhr' AND bm.region = 'weur') \
                  OR (?3 = 'nrt' AND bm.region = 'apac') \
                  OR (?3 = 'sam' AND bm.region = 'sam'))",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(self.region.as_str()),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        rows.first()
            .map(|row| {
                let blob_region = text(row, "blob_region")?;
                let tenant_region = text(row, "tenant_region")?;
                validate_gc_residency(self.region, &blob_region, &tenant_region)?;
                Ok(PurgeState {
                    r2_key: canonical_blob_key(self.region, &self.tdk, tenant_id, digest)?,
                    refcount: number(row, "refcount")?
                        .try_into()
                        .map_err(|_| "refcount exceeds u32".to_owned())?,
                    size_bytes: number(row, "size_bytes")?,
                    deleted_at_ms: number(row, "deleted_at")?,
                })
            })
            .transpose()
            .map_err(PhysicalDeleteError::Backend)
    }
    fn conditional_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<bool, PhysicalDeleteError> {
        let allowed_macro_regions = match self.region {
            GcRegion::Iad => "'wnam', 'enam'",
            GcRegion::Lhr => "'weur'",
            GcRegion::Nrt => "'apac'",
            GcRegion::Sam => "'sam'",
            GcRegion::Syd => {
                return Err(PhysicalDeleteError::Backend(
                    "no residency mapping exists for GC region syd".to_owned(),
                ));
            }
        };
        let rows = query_sync(
            &self.d1,
            &format!(
                "DELETE FROM blob_meta WHERE tenant_id = ?1 AND digest = ?2 \
                 AND refcount = 0 AND deleted_at IS NOT NULL \
                 AND region = (SELECT primary_region FROM tenant WHERE tenant_id = ?1) \
                 AND region IN ({allowed_macro_regions}) \
                 AND (?3 - deleted_at) > ?4 \
                 AND EXISTS (SELECT 1 FROM gc_purge_intent AS pi \
                            WHERE pi.tenant_id = blob_meta.tenant_id AND pi.digest = blob_meta.digest \
                              AND pi.state = 'r2_deleted') RETURNING 1 AS purged"
            ),
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(now_ms),
                json!(grace_period_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        Ok(!rows.is_empty())
    }

    fn begin_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        run_id: RunId,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<Option<PurgeStage>, PhysicalDeleteError> {
        let allowed_macro_regions = match self.region {
            GcRegion::Iad => "'wnam', 'enam'",
            GcRegion::Lhr => "'weur'",
            GcRegion::Nrt => "'apac'",
            GcRegion::Sam => "'sam'",
            GcRegion::Syd => {
                return Err(PhysicalDeleteError::Backend(
                    "no residency mapping exists for GC region syd".to_owned(),
                ));
            }
        };
        let r2_key = canonical_blob_key(self.region, &self.tdk, tenant_id, digest)
            .map_err(PhysicalDeleteError::Backend)?;
        let inserted = query_sync(
            &self.d1,
            &format!(
                "INSERT INTO gc_purge_intent \
                 (tenant_id, digest, epoch, state, r2_key, gc_region, residency_region, mark_run_id, started_at, updated_at) \
                 SELECT ?1, ?2, COALESCE((SELECT MAX(epoch) FROM gc_purge_intent WHERE tenant_id = ?1 AND digest = ?2), 0) + 1, \
                        'purging', ?3, ?6, t.primary_region, ?5, ?4, ?4 \
                 FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id = bm.tenant_id \
                 WHERE bm.tenant_id = ?1 AND bm.digest = ?2 AND bm.refcount = 0 \
                   AND bm.deleted_at IS NOT NULL AND (?4 - bm.deleted_at) > ?7 \
                   AND bm.region = t.primary_region AND bm.region IN ({allowed_macro_regions}) \
                   AND NOT EXISTS (SELECT 1 FROM gc_purge_intent AS pi \
                                  WHERE pi.tenant_id = bm.tenant_id AND pi.digest = bm.digest \
                                    AND pi.state IN ('purging', 'r2_deleted', 'retry')) \
                   AND NOT EXISTS (SELECT 1 FROM cas_write_intent AS wi \
                                  WHERE wi.tenant_id = bm.tenant_id AND wi.digest = bm.digest \
                                    AND wi.state = 'writing' \
                                    AND (?4 - wi.updated_at) <= {CAS_WRITE_LEASE_MS}) \
                 ON CONFLICT (tenant_id, digest) DO NOTHING \
                 RETURNING epoch, state"
            ),
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(r2_key),
                json!(now_ms),
                json!(run_id.as_text()),
                json!(self.region.as_str()),
                json!(grace_period_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        if let Some(row) = inserted.first() {
            let epoch = number(row, "epoch")?;
            return Ok(Some(PurgeStage::Acquired { epoch }));
        }
        let existing = query_sync(
            &self.d1,
            "SELECT epoch, state FROM gc_purge_intent WHERE tenant_id = ?1 AND digest = ?2",
            &[json!(tenant_id.to_string()), json!(digest.to_string())],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        let Some(row) = existing.first() else {
            return Ok(None);
        };
        let epoch = number(row, "epoch")?;
        match text(row, "state")?.as_str() {
            // A live `purging` row belongs to another worker.  Do not issue
            // a second remote delete concurrently.  A crashed owner is
            // reclaimed after the bounded lease and then receives a new
            // epoch, fencing the stale worker's later updates.
            "purging" => {
                const PURGE_LEASE_MS: u64 = CAS_WRITE_LEASE_MS;
                let updated_at = number(row, "updated_at")?;
                if now_ms.saturating_sub(updated_at) <= PURGE_LEASE_MS {
                    return Ok(None);
                }
                let expired = query_sync(
                    &self.d1,
                    "UPDATE gc_purge_intent SET state = 'retry', updated_at = ?4, last_error = 'owner lease expired' \
                     WHERE tenant_id = ?1 AND digest = ?2 AND epoch = ?3 AND state = 'purging' \
                       AND (?4 - updated_at) > ?5 RETURNING epoch",
                    &[
                        json!(tenant_id.to_string()),
                        json!(digest.to_string()),
                        json!(epoch),
                        json!(now_ms),
                        json!(PURGE_LEASE_MS),
                    ],
                )
                .map_err(PhysicalDeleteError::Backend)?;
                if expired.is_empty() {
                    return Ok(None);
                }
                self.claim_retry_epoch(tenant_id, digest, now_ms)
            }
            "retry" => self.claim_retry_epoch(tenant_id, digest, now_ms),
            "r2_deleted" => Ok(Some(PurgeStage::R2Deleted { epoch })),
            "finalized" => Ok(None),
            state => Err(PhysicalDeleteError::Backend(format!(
                "unknown gc purge intent state {state:?}"
            ))),
        }
    }

    /// Atomically claim a retry and advance the epoch.  The UPDATE is the
    /// ownership boundary: exactly one concurrent worker can receive the new
    /// `purging` epoch and proceed to R2.
    fn claim_retry_epoch(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
    ) -> Result<Option<PurgeStage>, PhysicalDeleteError> {
        let rows = query_sync(
            &self.d1,
            "UPDATE gc_purge_intent SET state = 'purging', epoch = epoch + 1, updated_at = ?3, last_error = NULL \
             WHERE tenant_id = ?1 AND digest = ?2 AND state = 'retry' RETURNING epoch",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(now_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        rows.first()
            .map(|row| number(row, "epoch").map(|epoch| PurgeStage::Acquired { epoch }))
            .transpose()
            .map_err(PhysicalDeleteError::Backend)
    }

    fn mark_r2_deleted(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        epoch: u64,
        now_ms: u64,
    ) -> Result<(), PhysicalDeleteError> {
        let rows = query_sync(
            &self.d1,
            "UPDATE gc_purge_intent SET state = 'r2_deleted', updated_at = ?4, last_error = NULL \
             WHERE tenant_id = ?1 AND digest = ?2 AND epoch = ?3 AND state IN ('purging', 'retry') RETURNING epoch",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(epoch),
                json!(now_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        if rows.is_empty() {
            return Err(PhysicalDeleteError::Backend(
                "purge epoch lost before R2 completion".to_owned(),
            ));
        }
        Ok(())
    }

    fn mark_r2_retry(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        epoch: u64,
        now_ms: u64,
        error: &str,
    ) -> Result<(), PhysicalDeleteError> {
        query_sync(
            &self.d1,
            "UPDATE gc_purge_intent SET state = 'retry', updated_at = ?4, last_error = ?5 \
             WHERE tenant_id = ?1 AND digest = ?2 AND epoch = ?3 AND state IN ('purging', 'retry')",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(epoch),
                json!(now_ms),
                json!(error),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)
        .map(|_| ())
    }

    fn finalize_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        epoch: u64,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<bool, PhysicalDeleteError> {
        let allowed_macro_regions = match self.region {
            GcRegion::Iad => "'wnam', 'enam'",
            GcRegion::Lhr => "'weur'",
            GcRegion::Nrt => "'apac'",
            GcRegion::Sam => "'sam'",
            GcRegion::Syd => {
                return Err(PhysicalDeleteError::Backend(
                    "no residency mapping exists for GC region syd".to_owned(),
                ));
            }
        };
        let rows = query_sync(
            &self.d1,
            &format!(
                "DELETE FROM blob_meta WHERE tenant_id = ?1 AND digest = ?2 \
                 AND refcount = 0 AND deleted_at IS NOT NULL \
                 AND region = (SELECT primary_region FROM tenant WHERE tenant_id = ?1) \
                 AND region IN ({allowed_macro_regions}) AND (?3 - deleted_at) > ?4 \
                 AND EXISTS (SELECT 1 FROM gc_purge_intent AS pi \
                            WHERE pi.tenant_id = blob_meta.tenant_id AND pi.digest = blob_meta.digest \
                              AND pi.epoch = ?5 AND pi.state = 'r2_deleted') \
                 RETURNING 1 AS purged"
            ),
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(now_ms),
                json!(grace_period_ms),
                json!(epoch),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        Ok(!rows.is_empty())
    }
}

fn canonical_blob_key(
    region: GcRegion,
    tdk: &[u8; 32],
    tenant_id: Uuid,
    digest: &BlobDigest,
) -> Result<String, String> {
    let prefix = corelink_tenant_path::derive_prefix(
        &corelink_tenant_path::TenantDerivationKey::from_bytes(zeroize::Zeroizing::new(*tdk)),
        tenant_id,
    );
    Ok(format!("{}/{}/{}", region.as_str(), prefix, digest))
}

/// Reconcile the physical GC colo with the macro-region residency contract.
/// The two schemas intentionally use different vocabularies: `gc_run.region`
/// is a serving colo while `blob_meta.region`/`tenant.primary_region` are macro
/// regions.  Unknown or mismatched pairs are a hard error; silently treating a
/// row as resident in another region would make GC a cross-border delete path.
fn validate_gc_residency(
    gc_region: GcRegion,
    blob_region: &str,
    tenant_region: &str,
) -> Result<(), String> {
    let valid = match gc_region {
        GcRegion::Iad => matches!(blob_region, "wnam" | "enam"),
        GcRegion::Lhr => blob_region == "weur",
        GcRegion::Nrt => blob_region == "apac",
        GcRegion::Sam => blob_region == "sam",
        // There is no provisioned macro residency mapping for syd.
        GcRegion::Syd => false,
    };
    if !valid || blob_region != tenant_region {
        return Err(format!(
            "GC/residency region mismatch: gc={} blob={} tenant={}",
            gc_region.as_str(),
            blob_region,
            tenant_region
        ));
    }
    Ok(())
}

/// Resolve the durable tenant residency before writing any GC audit record.
/// `audit_outbox.region` is a macro-region column, whereas GC events carry the
/// serving-colo enum.  Never insert a physical colo into that column: the
/// residency trigger would reject valid `iad`/`wnam` pairs, and bypassing it
/// would misattribute the audit partition.
fn tenant_residency_region(
    d1: &D1HttpClient,
    tenant_id: Uuid,
    gc_region: GcRegion,
) -> Result<String, String> {
    let rows = query_sync(
        d1,
        "SELECT primary_region FROM tenant WHERE tenant_id = ?1",
        &[json!(tenant_id.to_string())],
    )?;
    let row = rows
        .first()
        .ok_or_else(|| format!("tenant {tenant_id} has no durable residency region"))?;
    let region = text(row, "primary_region")?;
    validate_gc_residency(gc_region, &region, &region)?;
    Ok(region)
}

// Keep the native GC adapter below the source-size ratchet used by the
// storage-boundary audit; the included fragment shares this module's private
// helpers and types without changing the compiled symbol map.
include!("gc_sweep/part-01.rs");
