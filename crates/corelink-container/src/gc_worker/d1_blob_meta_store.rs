//! Real D1-backed [`BlobMetaStore`] implementation for production sweep phase.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::sweep::{BlobMetaRow, BlobMetaStore, BlobState, SoftDeleteOutcome, SweepError};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed blob meta store — real production implementation.
#[derive(Clone, Debug)]
pub struct D1BlobMetaStore {
    client: Arc<D1HttpClient>,
}

impl D1BlobMetaStore {
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::sweep::BlobMetaStore for D1BlobMetaStore {
    async fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::sweep::BlobDigest,
    ) -> Result<Option<BlobMetaRow>, corelink_gc::sweep::SweepError> {
        let rows = self.client
            .query(
                r#"
                SELECT digest, refcount, size_bytes, last_referenced_at_ms, created_at_ms, deleted_at_ms
                FROM blob_meta
                WHERE tenant_id = ?1 AND digest = ?2
                LIMIT 1
            "#,
                &[json!(tenant_id.to_string()), json!(digest.as_str())],
            )
            .await
            .map_err(|e| corelink_gc::sweep::SweepError::Backend(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }
        let row = &rows[0];
        Ok(Some(BlobMetaRow {
            digest: corelink_gc::sweep::BlobDigest::parse(
                row.get("digest").and_then(|v| v.as_str()).unwrap_or("")
            ).map_err(|e| corelink_gc::sweep::SweepError::Backend(e.to_string()))?,
            refcount: row.get("refcount").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            size_bytes: row.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            last_referenced_at_ms: row.get("last_referenced_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            created_at_ms: row.get("created_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            deleted_at_ms: row.get("deleted_at_ms").and_then(|v| v.as_i64()).map(|v| v as u64),
        }))
    }

    async fn soft_delete(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::sweep::BlobDigest,
        now_ms: u64,
    ) -> Result<Option<BlobState>, corelink_gc::sweep::SweepError> {
        let affected = self.client
            .execute(
                r#"
                UPDATE blob_meta
                SET deleted_at_ms = ?, updated_at_ms = ?
                WHERE tenant_id = ?1 AND digest = ?2 AND deleted_at_ms IS NULL
            "#,
                &[
                    json!(now_ms as i64),
                    json!(now_ms as i64),
                    json!(tenant_id.to_string()),
                    json!(digest.as_str()),
                ],
            )
            .await
            .map_err(|e| corelink_gc::sweep::SweepError::Backend(e.to_string()))?;

        if affected == 0 {
            return Ok(None);
        }
        Ok(Some(corelink_gc::sweep::BlobState {
            digest: digest.clone(),
            size_bytes: 0, // Filled by caller
            refcount: 0,
            last_referenced_at_ms: 0,
            created_at_ms: 0,
            deleted_at_ms: Some(now_ms),
        }))
    }
}