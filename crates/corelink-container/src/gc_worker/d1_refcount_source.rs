//! Real D1-backed [`RefcountSource`] implementation for production reconcile phase.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::reconcile::{BlobDigest, ReconcileError, RefcountSource};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed refcount source — real production implementation.
///
/// Implements the canonical `json_each` aggregate query that computes
/// `expected_refcount` for `(tenant_id, digest)` at the snapshot instant.
#[derive(Clone, Debug)]
pub struct D1RefcountSource {
    client: Arc<D1HttpClient>,
}

impl D1RefcountSource {
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::reconcile::RefcountSource for D1RefcountSource {
    async fn expected_refcount(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::reconcile::BlobDigest,
        snapshot_at_ms: u64,
    ) -> Result<u32, corelink_gc::reconcile::ReconcileError> {
        let rows = self.client
            .query(
                r#"
                SELECT COUNT(*) as cnt
                FROM ac_meta a, json_each(a.blob_refs) j
                WHERE a.tenant_id = ?1
                  AND j.value = ?2
                  AND a.deleted_at_ms IS NULL
                  AND a.created_at_ms < ?3
            "#,
                &[
                    json!(tenant_id.to_string()),
                    json!(digest.as_str()),
                    json!(snapshot_at_ms as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::reconcile::ReconcileError::Backend(e.to_string()))?;

        let count = rows
            .first()
            .and_then(|r| r.get("cnt"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u32;

        Ok(count)
    }
}