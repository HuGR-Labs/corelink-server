//! Real D1-backed [`BlobMetaRefcountStore`] implementation for production reconcile phase.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::reconcile::{BlobDigest, BlobMetaReconcileRow, BlobMetaRefcountStore, ReconcileError};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed blob meta refcount store — real production implementation.
#[derive(Clone, Debug)]
pub struct D1BlobMetaRefcountStore {
    client: Arc<D1HttpClient>,
}

impl D1BlobMetaRefcountStore {
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::reconcile::BlobMetaRefcountStore for D1BlobMetaRefcountStore {
    async fn snapshot_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<corelink_gc::reconcile::BlobMetaReconcileRow>, corelink_gc::reconcile::ReconcileError> {
        let rows = self.client
            .query(
                r#"
                SELECT digest, refcount, deleted_at_ms, r2_present
                FROM blob_meta
                WHERE tenant_id = ?1
                ORDER BY digest
            "#,
                &[json!(tenant_id.to_string())],
            )
            .await
            .map_err(|e| corelink_gc::reconcile::ReconcileError::Backend(e.to_string()))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let digest = row.get("digest").and_then(|v| v.as_str()).unwrap_or("");
            let stored_refcount = row.get("refcount").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let deleted_at_ms = row.get("deleted_at_ms").and_then(|v| v.as_i64()).map(|v| v as u64);
            let r2_present = row.get("r2_present").and_then(|v| v.as_bool()).unwrap_or(false);

            let digest = corelink_gc::reconcile::BlobDigest::parse(
                row.get("digest").and_then(|v| v.as_str()).unwrap_or("")
            ).map_err(|e| corelink_gc::reconcile::ReconcileError::Backend(e.to_string()))?;

            out.push(corelink_gc::reconcile::BlobMetaReconcileRow {
                digest,
                stored_refcount,
                deleted_at_ms,
                r2_present,
            });
        }
        Ok(out)
    }

    async fn conditional_set_refcount(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::reconcile::BlobDigest,
        stored_refcount: u32,
        new_refcount: u32,
    ) -> Result<bool, corelink_gc::reconcile::ReconcileError> {
        let sql = r#"
            UPDATE blob_meta
            SET refcount = ?, updated_at_ms = ?
            WHERE tenant_id = ?1 AND digest = ?2
              AND refcount = ?3
              AND deleted_at_ms IS NULL
        "#;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0) as i64;

        let affected = self.client
            .execute(
                r#"
                UPDATE blob_meta
                SET refcount = ?4, updated_at_ms = ?5
                WHERE tenant_id = ?1 AND digest = ?2
                  AND refcount = ?3
                  AND deleted_at_ms IS NULL
            "#,
                &[
                    json!(tenant_id.to_string()),
                    json!(digest.as_str()),
                    json!(stored_refcount as i64),
                    json!(new_refcount as i64),
                    json!(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::reconcile::ReconcileError::Backend(e.to_string()))?;

        Ok(affected > 0)
    }
}

use std::time::{SystemTime, UNIX_EPOCH};