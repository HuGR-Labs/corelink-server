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
    GcSweepMode, GcSweepReportError, GcSweepReportSink, GcSweepRunner, InMemoryGcSweepReportSink,
    MarkError, PhysicalDeleteClock, PhysicalDeleteError, PurgeStage, PurgeState, R2Delete,
    R2DeleteError, R2DeleteOutcome, RunId, SweepReport,
};
use serde_json::{json, Value};
use tracing::info;
use uuid::Uuid;

use crate::storage::cas_write_fence::CAS_WRITE_LEASE_MS;
use crate::storage::d1_http::{D1HttpClient, D1Row};
use crate::storage::r2_s3::{validate_cas_bucket_for_region, R2S3Client};

/// Runtime configuration for one tenant/region sweep.
#[derive(Clone)]
#[non_exhaustive]
pub struct GcProductionConfig {
    /// Tenant whose candidates are eligible for this invocation.
    pub tenant_id: Uuid,
    /// Region whose running checkpoint is eligible for this invocation.
    pub region: GcRegion,
    /// Optional explicit run; absent means the current running run.
    pub run_id: Option<RunId>,
    /// Destructive mode, gated by `GC_LIVE_DELETE` and confirmation.
    pub mode: GcSweepMode,
    /// Explicit read-only observation mode, requiring `GC_LIVE_DELETE=false`.
    pub observation_only: bool,
    /// Hard ceiling for one observation; a larger population fails closed.
    pub max_candidates: u32,
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
            .field("observation_only", &self.observation_only)
            .field("max_candidates", &self.max_candidates)
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
        validate_observation_region(region)?;
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

        let observation_only = observation_only_from_env(
            std::env::var("GC_OBSERVATION_ONLY").ok().as_deref(),
            std::env::var("GC_LIVE_DELETE").ok().as_deref(),
        )?;
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
        if !observation_only {
            return Err(
                "production GC requires GC_OBSERVATION_ONLY=true and GC_LIVE_DELETE=false"
                    .to_owned(),
            );
        }
        Ok(Self {
            tenant_id,
            region,
            run_id,
            mode,
            observation_only,
            max_candidates: DEFAULT_OBSERVATION_MAX_CANDIDATES,
            bucket,
            tdk,
        })
    }
}

const DEFAULT_OBSERVATION_MAX_CANDIDATES: u32 = 250;

fn validate_observation_candidate_limit(max_candidates: u32) -> Result<(), String> {
    if !(1..=DEFAULT_OBSERVATION_MAX_CANDIDATES).contains(&max_candidates) {
        return Err(format!(
            "GC observation candidate limit must be in 1..={DEFAULT_OBSERVATION_MAX_CANDIDATES}"
        ));
    }
    Ok(())
}

fn validate_observation_region(region: GcRegion) -> Result<(), String> {
    if region == GcRegion::Syd {
        return Err(
            "GC_REGION=syd has no provisioned macro-residency mapping; observation refused"
                .to_owned(),
        );
    }
    Ok(())
}

/// Resolve the opt-in observation contract without relying on the permissive
/// sweep-mode parser.  A caller asking for observation must explicitly set
/// `GC_LIVE_DELETE=false`; unset, malformed, or live values are rejected so a
/// deployment cannot accidentally turn an observation invocation into an
/// ambiguous mode.
fn observation_only_from_env(
    observation_raw: Option<&str>,
    live_raw: Option<&str>,
) -> Result<bool, String> {
    let Some(observation_raw) = observation_raw else {
        return Ok(false);
    };
    let observation_raw = observation_raw.trim();
    if observation_raw.eq_ignore_ascii_case("false") || observation_raw == "0" {
        return Ok(false);
    }
    if !(observation_raw.eq_ignore_ascii_case("true") || observation_raw == "1") {
        return Err("GC_OBSERVATION_ONLY must be true or false".to_owned());
    }
    if live_raw
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("false"))
    {
        return Ok(true);
    }
    Err("GC_OBSERVATION_ONLY=true requires GC_LIVE_DELETE=false".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        observation_only_from_env, validate_observation_candidate_limit,
        validate_observation_region, GcAuditRecord, GcAuditSink, GcEventType, GcPhase, GcRegion,
        GcStatus, ObservationOnlyAuditSink, ObservationOnlyR2Delete, R2Delete, RunId, Uuid,
    };

    #[test]
    fn observation_requires_explicit_false_live_delete() {
        assert_eq!(
            observation_only_from_env(Some("true"), Some("false")),
            Ok(true)
        );
        assert!(observation_only_from_env(Some("true"), None).is_err());
        assert!(observation_only_from_env(Some("true"), Some("true")).is_err());
        assert!(observation_only_from_env(Some("true"), Some("maybe")).is_err());
    }

    #[test]
    fn observation_is_opt_in_and_rejects_malformed_flag() {
        assert_eq!(observation_only_from_env(None, None), Ok(false));
        assert_eq!(
            observation_only_from_env(Some("false"), Some("true")),
            Ok(false)
        );
        assert!(observation_only_from_env(Some("maybe"), Some("false")).is_err());
    }

    #[test]
    fn observation_rejects_unmapped_syd_region() {
        assert!(validate_observation_region(GcRegion::Syd).is_err());
        for region in [GcRegion::Iad, GcRegion::Lhr, GcRegion::Nrt, GcRegion::Sam] {
            assert_eq!(validate_observation_region(region), Ok(()));
        }
    }

    #[test]
    fn observation_candidate_limit_accepts_250_and_rejects_251() {
        assert_eq!(validate_observation_candidate_limit(250), Ok(()));
        assert!(validate_observation_candidate_limit(251).is_err());
        assert!(validate_observation_candidate_limit(0).is_err());
    }

    #[test]
    fn observation_mutation_boundaries_reject_r2_and_audit_calls() {
        let r2_result = R2Delete::delete(
            &ObservationOnlyR2Delete,
            Uuid::nil(),
            GcRegion::Iad,
            "iad/tenant/digest",
        );
        assert!(
            r2_result.is_err(),
            "observation R2 adapter must reject delete"
        );
        let Some(r2_error) = r2_result.err() else {
            unreachable!("observation R2 adapter unexpectedly accepted delete");
        };
        assert!(r2_error
            .to_string()
            .contains("disabled for production GC observation"));

        let audit_result = GcAuditSink::emit(
            &ObservationOnlyAuditSink,
            GcAuditRecord {
                event_type: GcEventType::PhysicalDeleted,
                run_id: RunId(Uuid::nil()),
                tenant_id: Uuid::nil(),
                region: GcRegion::Iad,
                status: GcStatus::Running,
                from_phase: Some(GcPhase::PhysicalDelete),
                to_phase: None,
                created_by_request_id: "test-observation".to_owned(),
                reason: "mutation_probe",
                now_ms: 1,
            },
        );
        assert!(
            audit_result.is_err(),
            "observation audit adapter must reject mutation event"
        );
        let Some(audit_error) = audit_result.err() else {
            unreachable!("observation audit adapter unexpectedly accepted mutation event");
        };
        assert!(audit_error
            .to_string()
            .contains("disabled for production observation"));
    }
}

/// Execute one real, read-only D1 observation using a validated scope.
///
/// This path reads the durable `gc_run`, `gc_candidates`, `blob_meta`, and
/// tenant residency rows. It deliberately constructs neither an R2 client nor
/// a durable report sink: the returned report/stdout is the evidence boundary,
/// and the reject-only R2 adapter makes an accidental delete call fail closed.
pub fn run_production(config: &GcProductionConfig) -> Result<SweepReport, String> {
    if config.mode.is_live() {
        return Err(
            "live GC is unavailable: R2/D1 deletion needs a deployed fencing transaction; run dry-run"
                .to_owned(),
        );
    }
    if !config.observation_only {
        return Err("production GC observation gate is not armed".to_owned());
    }
    validate_observation_region(config.region)?;
    validate_observation_candidate_limit(config.max_candidates)?;
    let d1 = Arc::new(D1HttpClient::from_d1_env()?);
    validate_cas_bucket_for_region(&config.bucket, config.region.as_str())?;
    let runs = Arc::new(D1GcRunStore::new(Arc::clone(&d1)));
    let candidates = Arc::new(D1GcCandidatesStore::new(
        Arc::clone(&d1),
        config.region,
        config.max_candidates,
    )?);
    let blob_meta = Arc::new(D1BlobMetaPurgeStore::new(
        Arc::clone(&d1),
        config.region,
        config.tdk,
    ));
    let r2_delete = Arc::new(ObservationOnlyR2Delete);
    let audit = Arc::new(ObservationOnlyAuditSink);
    let metrics = Arc::new(TracingGcMetrics);
    let clock = Arc::new(SystemGcClock::default());
    let report = Arc::new(InMemoryGcSweepReportSink::new());
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

/// R2 boundary for a production observation. The runner's dry-run branch must
/// never call it; retaining a rejecting implementation makes that invariant a
/// runtime fence rather than an assumption.
#[derive(Debug)]
struct ObservationOnlyR2Delete;

impl R2Delete for ObservationOnlyR2Delete {
    fn delete(
        &self,
        _tenant_id: Uuid,
        _region: GcRegion,
        _key: &str,
    ) -> Result<R2DeleteOutcome, R2DeleteError> {
        Err(R2DeleteError::Backend(
            "R2 delete is disabled for production GC observation".to_owned(),
        ))
    }
}

/// Audit boundary for a production observation. Dry-run emits no GC mutation
/// events; any future regression that attempts one is rejected before D1.
#[derive(Debug)]
struct ObservationOnlyAuditSink;

impl GcAuditSink for ObservationOnlyAuditSink {
    fn emit(&self, _record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
        Err(GcAuditSinkError::Store(
            "GC mutation audit is disabled for production observation".to_owned(),
        ))
    }
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
#[non_exhaustive]
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

// Keep the native GC adapter below the source-size ratchet used by the
// storage-boundary audit; each included fragment shares this module's
// private helpers and types without changing the compiled symbol map.
include!("gc_sweep/part-02.rs");
include!("gc_sweep/part-03.rs");
include!("gc_sweep/part-04.rs");
include!("gc_sweep/part-01.rs");
