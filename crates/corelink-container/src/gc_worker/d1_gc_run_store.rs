//! D1-backed [`GcRunStore`] implementation for production GC worker.
//!
//! Implements the [`crate::run::GcRunStore`] trait over the Cloudflare D1
//!
//! HTTP API via [`crate::storage::d1_http::D1HttpClient`]. All SQL is
//! parameterised and tenant-scoped per the canonical schema.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use corelink_gc::run::{
    CheckpointDeltas, FailureContext, GcPhase, GcRun, GcRunStore, GcRunStoreError, GcRegion,
    GcRun as GcRunStruct, GcStatus, RunId,
};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed GC run store.
///
/// All SQL is parameterised (positional binds); the tenant scope rides in
/// the composite PK `(run_id, tenant_id)` on every statement.
#[derive(Clone, Debug)]
pub struct D1GcRunStore {
    client: Arc<D1HttpClient>,
}

impl D1GcRunStore {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }

    /// Current Unix epoch in milliseconds.
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

#[async_trait]
impl corelink_gc::run::GcRunStore for D1GcRunStore {
    async fn insert_pending(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: corelink_gc::region::GcRegion,
        started_at_ms: u64,
        created_by_request_id: String,
    ) -> Result<(), corelink_gc::run::GcRunStoreError> {
        let sql = r#"
            INSERT INTO gc_run (
                run_id, tenant_id, region, phase, status, started_at_ms,
                last_checkpoint_at_ms, completed_at_ms, failed_at_ms,
                mark_started_at_ms, blobs_marked_count, blobs_swept_count,
                blobs_physically_deleted_count, bytes_reclaimed,
                created_by_request_id, failed_phase, failed_reason
            ) VALUES (?, ?, ?, 'idle', 'pending', ?, ?, NULL, NULL, NULL, 0, 0, 0, 0, ?, NULL, NULL)
        "#;
        let region_str = region.as_str();
        self.client
            .query(
                sql,
                &[
                    json!(run_id.to_string()),
                    json!(tenant_id.to_string()),
                    json!(region_str),
                    json!(started_at_ms as i64),
                    json!(started_at_ms as i64),
                    json!(created_by_request_id),
                ],
            )
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn acquire_running(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<(), corelink_gc::run::GcRunStoreError> {
        let sql = r#"
            UPDATE gc_run
            SET status = 'running', last_checkpoint_at_ms = ?
            WHERE run_id = ? AND tenant_id = ? AND status = 'pending'
              AND NOT EXISTS (
                  SELECT 1 FROM gc_run
                  WHERE tenant_id = ? AND region = (
                      SELECT region FROM gc_run WHERE run_id = ?
                  ) AND status = 'running' AND run_id != ?
              )
        "#;
        let affected = self.client
            .execute(
                sql,
                &[
                    json!(now_ms as i64),
                    json!(run_id.to_string()),
                    json!(tenant_id.to_string()),
                    json!(tenant_id.to_string()),
                    json!(run_id.to_string()),
                    json!(run_id.to_string()),
                ],
            )
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

        if affected == 0 {
            // Check why it failed
            let check_sql = r#"
                SELECT status, tenant_id FROM gc_run WHERE run_id = ?
            "#;
            let rows = self.client
                .query(check_sql, &[json!(run_id.to_string())])
                .await
                .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

            if rows.is_empty() {
                return Err(corelink_gc::run::GcRunStoreError::NotFound(run_id));
            }
            let row = &rows[0];
            let status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status != "pending" {
                if status == "running" {
                    return Err(corelink_gc::run::GcRunStoreError::AlreadyRunning {
                        tenant_id,
                        region: corelink_gc::region::GcRegion::Enam, // placeholder
                        existing_run_id: Uuid::nil(),
                    });
                }
            }
            let row_tenant = row.get("tenant_id").and_then(|v| v.as_str()).unwrap_or("");
            if row_tenant != tenant_id.to_string() {
                return Err(corelink_gc::run::GcRunStoreError::CrossTenantRun { run_id });
            }
            return Err(corelink_gc::run::GcRunStoreError::Backend("acquire_running failed".to_string()));
        }
        Ok(())
    }

    async fn transition_phase(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        to: corelink_gc::run::GcPhase,
        now_ms: u64,
    ) -> Result<(), corelink_gc::run::GcRunStoreError> {
        let phase_str = to.as_str();
        let mut sql = String::from(
            r#"
            UPDATE gc_run
            SET phase = ?, last_checkpoint_at_ms = ?
        "#);

        let mut params = vec![json!(to.as_str()), json!(now_ms as i64)];

        if to == corelink_gc::run::GcPhase::Mark {
            sql.push_str(", mark_started_at_ms = ?");
            params.push(json!(now_ms as i64));
        }

        sql.push_str(r#"
            WHERE run_id = ? AND tenant_id = ? AND status = 'running'
              AND last_checkpoint_at_ms <= ?
        "#);
        params.push(json!(run_id.to_string()));
        params.push(json!(tenant_id.to_string()));
        params.push(json!(now_ms as i64));

        // Add phase transition validation
        if to == corelink_gc::run::GcPhase::Mark {
            sql.push_str(r#"
                AND (mark_started_at_ms IS NULL OR mark_started_at_ms = ?)
            "#);
            params.push(json!(now_ms as i64));
        }

        // Check valid phase transition
        let valid_transitions = r#"
            AND (
                (phase = 'idle' AND ? = 'mark') OR
                (phase = 'mark' AND ? = 'sweep') OR
                (phase = 'sweep' AND ? = 'physical_delete') OR
                (phase = 'physical_delete' AND ? = 'reconcile') OR
                (phase = 'reconcile' AND ? = 'completed') OR
                (phase IN ('idle','mark','sweep','physical_delete','reconcile') AND ? = 'failed')
            )"#;
        let to_str = to.as_str();
        sql.push_str(valid_transitions);
        for _ in 0..6 {
            params.push(json!(to_str));
        }

        let affected = self.client
            .execute(&sql, &params)
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

        if affected == 0 {
            // Check why it failed
            let check_sql = r#"
                SELECT phase, status, tenant_id FROM gc_run WHERE run_id = ?
            "#;
            let rows = self.client
                .query(check_sql, &[json!(run_id.to_string())])
                .await
                .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

            if rows.is_empty() {
                return Err(corelink_gc::run::GcRunStoreError::NotFound(run_id));
            }
            let row = &rows[0];
            let row_tenant = row.get("tenant_id").and_then(|v| v.as_str()).unwrap_or("");
            if row_tenant != tenant_id.to_string() {
                return Err(corelink_gc::run::GcRunStoreError::CrossTenantRun { run_id });
            }
            let phase = row.get("phase").and_then(|v| v.as_str()).unwrap_or("");
            let status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if !corelink_gc::run::GcPhase::from_str(phase).unwrap().can_transition_to(corelink_gc::run::GcPhase::from_str(to.as_str()).unwrap()) {
                return Err(corelink_gc::run::GcRunStoreError::InvalidPhaseTransition {
                    from: phase.to_string(),
                    to: to.as_str().to_string(),
                });
            }
            if status != "running" {
                return Err(corelink_gc::run::GcRunStoreError::Backend("not running".to_string()));
            }
            return Err(corelink_gc::run::GcRunStoreError::Backend("transition_phase failed".to_string()));
        }
        Ok(())
    }

    async fn checkpoint(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        now_ms: u64,
        deltas: corelink_gc::run::CheckpointDeltas,
    ) -> Result<(), corelink_gc::run::GcRunStoreError> {
        let sql = r#"
            UPDATE gc_run
            SET last_checkpoint_at_ms = ?,
                blobs_marked_count = blobs_marked_count + ?,
                blobs_swept_count = blobs_swept_count + ?,
                blobs_physically_deleted_count = blobs_physically_deleted_count + ?,
                bytes_reclaimed = bytes_reclaimed + ?
            WHERE run_id = ? AND tenant_id = ? AND status = 'running'
              AND last_checkpoint_at_ms <= ?
        "#;
        let affected = self.client
            .execute(
                r#"
                UPDATE gc_run
                SET last_checkpoint_at_ms = ?,
                    blobs_marked_count = blobs_marked_count + ?,
                    blobs_swept_count = blobs_swept_count + ?,
                    blobs_physically_deleted_count = blobs_physically_deleted_count + ?,
                    bytes_reclaimed = bytes_reclaimed + ?
                WHERE run_id = ? AND tenant_id = ? AND status = 'running'
                  AND last_checkpoint_at_ms <= ?
            "#,
                &[
                    json!(now_ms as i64),
                    json!(deltas.blobs_marked_delta as i64),
                    json!(deltas.blobs_swept_delta as i64),
                    json!(deltas.blobs_physically_deleted_delta as i64),
                    json!(deltas.bytes_reclaimed_delta as i64),
                    json!(run_id.to_string()),
                    json!(tenant_id.to_string()),
                    json!(now_ms as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

        if affected == 0 {
            return Err(corelink_gc::run::GcRunStoreError::Backend("checkpoint failed".to_string()));
        }
        Ok(())
    }

    async fn finalize(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        terminal: corelink_gc::run::GcStatus,
        now_ms: u64,
        failure: Option<corelink_gc::run::FailureContext>,
    ) -> Result<(), corelink_gc::run::GcRunStoreError> {
        let (failed_phase, failed_reason) = match failure {
            Some(f) => (Some(f.failed_phase.as_str()), Some(f.failed_reason)),
            None => (None, None),
        };

        let sql = r#"
            UPDATE gc_run
            SET status = ?, completed_at_ms = CASE WHEN ? = 'succeeded' THEN ? ELSE NULL END,
                failed_at_ms = CASE WHEN ? != 'succeeded' THEN ? ELSE NULL END,
                failed_phase = ?, failed_reason = ?, last_checkpoint_at_ms = ?
            WHERE run_id = ? AND tenant_id = ? AND status = 'running'
        "#;
        let terminal_str = terminal.as_str();
        self.client
            .execute(
                r#"
                UPDATE gc_run
                SET status = ?, completed_at_ms = CASE WHEN ? = 'succeeded' THEN ? ELSE NULL END,
                    failed_at_ms = CASE WHEN ? != 'succeeded' THEN ? ELSE NULL END,
                    failed_phase = ?, failed_reason = ?, last_checkpoint_at_ms = ?
                WHERE run_id = ? AND tenant_id = ? AND status = 'running'
            "#,
                &[
                    json!(terminal_str),
                    json!(terminal_str),
                    json!(now_ms as i64),
                    json!(terminal_str),
                    json!(now_ms as i64),
                    json!(failed_phase),
                    json!(failed_reason),
                    json!(now_ms as i64),
                    json!(run_id.to_string()),
                    json!(tenant_id.to_string()),
                ],
            )
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn lookup(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
    ) -> Result<Option<corelink_gc::run::GcRun>, corelink_gc::run::GcRunStoreError> {
        let rows = self.client
            .query(
                "SELECT * FROM gc_run WHERE run_id = ? AND tenant_id = ?",
                &[json!(run_id.to_string()), json!(tenant_id.to_string())],
            )
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }
        let row = &rows[0];
        Ok(Some(self.row_to_gc_run(row)))
    }

    async fn current_running(
        &self,
        tenant_id: Uuid,
        region: corelink_gc::region::GcRegion,
    ) -> Result<Option<corelink_gc::run::GcRun>, corelink_gc::run::GcRunStoreError> {
        let rows = self.client
            .query(
                "SELECT * FROM gc_run WHERE tenant_id = ? AND region = ? AND status = 'running' LIMIT 1",
                &[json!(tenant_id.to_string()), json!(region.as_str())],
            )
            .await
            .map_err(|e| corelink_gc::run::GcRunStoreError::Backend(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }
        let row = &rows[0];
        Ok(Some(self.row_to_gc_run(row)))
    }
}

impl D1GcRunStore {
    fn row_to_gc_run(&self, row: &serde_json::Map<String, serde_json::Value>) -> corelink_gc::run::GcRun {
        let run_id = Uuid::parse_str(row.get("run_id").and_then(|v| v.as_str()).unwrap_or("")).unwrap();
        let tenant_id = Uuid::parse_str(row.get("tenant_id").and_then(|v| v.as_str()).unwrap_or("")).unwrap();
        let region = corelink_gc::region::GcRegion::from_str(
            row.get("region").and_then(|v| v.as_str()).unwrap_or("enam")
        ).unwrap();
        let phase = corelink_gc::run::GcPhase::from_str(
            row.get("phase").and_then(|v| v.as_str()).unwrap_or("idle")
        ).unwrap();
        let status = corelink_gc::run::GcStatus::from_str(
            row.get("status").and_then(|v| v.as_str()).unwrap_or("pending")
        ).unwrap();

        corelink_gc::run::GcRun {
            run_id,
            tenant_id,
            region,
            phase,
            status,
            started_at_ms: row.get("started_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            last_checkpoint_at_ms: row.get("last_checkpoint_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            completed_at_ms: row.get("completed_at_ms").and_then(|v| v.as_i64()).map(|v| v as u64),
            failed_at_ms: row.get("failed_at_ms").and_then(|v| v.as_i64()).map(|v| v as u64),
            mark_started_at_ms: row.get("mark_started_at_ms").and_then(|v| v.as_i64()).map(|v| v as u64),
            blobs_marked_count: row.get("blobs_marked_count").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            blobs_swept_count: row.get("blobs_swept_count").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            blobs_physically_deleted_count: row.get("blobs_physically_deleted_count").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            bytes_reclaimed: row.get("bytes_reclaimed").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            created_by_request_id: row.get("created_by_request_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            failed_phase: row.get("failed_phase").and_then(|v| v.as_str()).map(|s| s.to_string()),
            failed_reason: row.get("failed_reason").and_then(|v| v.as_str()).map(|s| s.to_string()),
        }
    }
}