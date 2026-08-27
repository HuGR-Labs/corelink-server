//! Real D1-backed [`BlobMetaPurgeStore`] implementation for production physical_delete phase.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::physical_delete::{BlobMetaPurgeStore, PurgeState, PhysicalDeleteError};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed blob meta purge store — real production implementation.
#[derive(Clone, Debug)]
pub struct D1BlobMetaPurgeStore {
    client: Arc<D1HttpClient>,
}

impl D1BlobMetaPurgeStore {
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::physical_delete::BlobMetaPurgeStore for D1BlobMetaPurgeStore {
    async fn get_purge_state(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::physical_delete::BlobDigest,
    ) -> Result<Option<corelink_gc::physical_delete::PurgeState>, corelink_gc::physical_delete::PhysicalDeleteError> {
        let rows = self.client
            .query(
                r#"
                SELECT deleted_at_ms, refcount, size_bytes
                FROM blob_meta
                WHERE tenant_id = ?1 AND digest = ?2
                LIMIT 1
            "#,
                &[json!(tenant_id.to_string()), json!(digest.as_str())],
            )
            .await
            .map_err(|e| corelink_gc::physical_delete::PhysicalDeleteError::Backend(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }

        let row = &rows[0];
        let deleted_at_ms = row.get("deleted_at_ms").and_then(|v| v.as_i64()).map(|v| v as u64);
        let refcount = row.get("refcount").and_then(|v| v.as_i64()).unwrap_or(0) as u64;
        let size_bytes = row.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

        Ok(Some(corelink_gc::physical_delete::PurgeState {
            deleted_at_ms,
            refcount,
            size_bytes,
        }))
    }

    async fn purge(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::physical_delete::BlobDigest,
        now_ms: u64,
    ) -> Result<(), corelink_gc::physical_delete::PhysicalDeleteError> {
        // Physical purge: delete the blob_meta row entirely
        let affected = self.client
            .execute(
                r#"
                DELETE FROM blob_meta
                WHERE tenant_id = ?1 AND digest = ?2
            "#,
                &[json!(tenant_id.to_string()), json!(digest.as_str())],
            )
            .await
            .map_err(|e| corelink_gc::physical_delete::PhysicalDeleteError::Backend(e.to_string()))?;

        if affected == 0 {
            return Err(corelink_gc::physical_delete::PhysicalDeleteError::Backend(
                "purge: no row deleted".to_string(),
            ));
        }
        Ok(())
    }
}