//! Real D1-backed [`GcCandidatesStore`] implementation for production mark/sweep phases.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use corelink_gc::mark::{
    CandidateStatus, BlobDigest, GcCandidate, GcCandidatesStore, GcRunStoreError, MarkError, RunId,
};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed GC candidates store.
#[derive(Clone, Debug)]
pub struct D1GcCandidatesStore {
    client: Arc<D1HttpClient>,
}

impl D1GcCandidatesStore {
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::mark::GcCandidatesStore for D1GcCandidatesStore {
    async fn insert_candidate(
        &self,
        candidate: corelink_gc::mark::GcCandidate,
    ) -> Result<(), corelink_gc::mark::MarkError> {
        let sql = r#"
            INSERT INTO gc_candidates (
                tenant_id, digest, mark_started_at_ms, mark_run_id,
                blob_size_bytes, blob_last_referenced_at_ms, status,
                created_at_ms, swept_at_ms, protected_at_ms, protected_reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL)
            ON CONFLICT(tenant_id, digest, mark_run_id) DO NOTHING
        "#;
        self.client
            .execute(
                r#"
                INSERT INTO gc_candidates (
                    tenant_id, digest, mark_started_at_ms, mark_run_id,
                    blob_size_bytes, blob_last_referenced_at_ms, status,
                    created_at_ms, swept_at_ms, protected_at_ms, protected_reason
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL)
                ON CONFLICT(tenant_id, digest, mark_run_id) DO NOTHING
            "#,
                &[
                    json!(candidate.tenant_id.to_string()),
                    json!(candidate.digest.as_str()),
                    json!(candidate.mark_started_at_ms as i64),
                    json!(candidate.mark_run_id.to_string()),
                    json!(candidate.blob_size_bytes as i64),
                    json!(candidate.blob_last_referenced_at_ms as i64),
                    json!(candidate.status.as_str()),
                    json!(candidate.created_at_ms as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn snapshot_for_run(
        &self,
        tenant_id: Uuid,
        mark_run_id: RunId,
    ) -> Result<Vec<corelink_gc::mark::GcCandidate>, corelink_gc::mark::MarkError> {
        let rows = self.client
            .query(
                r#"
                SELECT tenant_id, digest, mark_started_at_ms, mark_run_id,
                       blob_size_bytes, blob_last_referenced_at_ms,
                       status, created_at_ms, swept_at_ms, protected_at_ms,
                       protected_reason
                FROM gc_candidates
                WHERE tenant_id = ?1 AND mark_run_id = ?2
                ORDER BY digest
            "#,
                &[json!(tenant_id.to_string()), json!(mark_run_id.to_string())],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let candidate = self.row_to_candidate(row)?;
            out.push(candidate);
        }
        Ok(out)
    }

    async fn count_for_run(
        &self,
        tenant_id: Uuid,
        mark_run_id: RunId,
    ) -> Result<u64, corelink_gc::mark::MarkError> {
        let rows = self.client
            .query(
                "SELECT COUNT(*) as cnt FROM gc_candidates WHERE tenant_id = ?1 AND mark_run_id = ?2",
                &[json!(tenant_id.to_string()), json!(mark_run_id.to_string())],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        let count = rows
            .first()
            .and_then(|r| r.get("cnt"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;
        Ok(count)
    }

    async fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::mark::BlobDigest,
        mark_run_id: RunId,
    ) -> Result<Option<corelink_gc::mark::GcCandidate>, corelink_gc::mark::MarkError> {
        let rows = self.client
            .query(
                r#"
                SELECT tenant_id, digest, mark_started_at_ms, mark_run_id,
                       blob_size_bytes, blob_last_referenced_at_ms,
                       status, created_at_ms, swept_at_ms, protected_at_ms,
                       protected_reason
                FROM gc_candidates
                WHERE tenant_id = ?1 AND digest = ?2 AND mark_run_id = ?3
                LIMIT 1
            "#,
                &[
                    json!(tenant_id.to_string()),
                    json!(digest.as_str()),
                    json!(mark_run_id.to_string()),
                ],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }
        let candidate = self.row_to_candidate(&rows[0])?;
        Ok(Some(candidate))
    }

    async fn transition_status(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::mark::BlobDigest,
        mark_run_id: RunId,
        from_status: corelink_gc::mark::CandidateStatus,
        to_status: corelink_gc::mark::CandidateStatus,
        now_ms: u64,
        protected_reason: Option<String>,
    ) -> Result<bool, corelink_gc::mark::MarkError> {
        let now_ms_i64 = now_ms as i64;
        let from_str = from_status.as_str();
        let to_str = to_status.as_str();

        let (extra_set, extra_params) = match to_status {
            corelink_gc::mark::CandidateStatus::Swept => {
                (", swept_at_ms = ?", vec![json!(now_ms as i64)])
            }
            corelink_gc::mark::CandidateStatus::ProtectedReRef => {
                let reason = protected_reason.unwrap_or_default();
                (", protected_at_ms = ?, protected_reason = ?", vec![json!(now_ms as i64), json!(reason)])
            }
            corelink_gc::mark::CandidateStatus::PhysicallyDeleted => {
                (", protected_at_ms = ?, protected_reason = ?", vec![json!(now_ms as i64), json!("physically_deleted")])
            }
            _ => (Vec::new(), vec![]),
        };

        let mut sql = String::from(
            r#"
            UPDATE gc_candidates
            SET status = ?
        "#);

        if !extra_set.is_empty() {
            sql.push_str(&extra_set);
        }

        sql.push_str(
            r#"
            WHERE tenant_id = ? AND digest = ? AND mark_run_id = ? AND status = ?
            "#,
        );

        let mut params = vec![
            json!(to_status.as_str()),
            json!(tenant_id.to_string()),
            json!(digest.as_str()),
            json!(mark_run_id.to_string()),
            json!(from_status.as_str()),
        ];
        params.extend(extra_params);

        let affected = self.client
            .execute(&sql, &params)
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        Ok(affected > 0)
    }
}