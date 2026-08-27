//! Real D1-backed [`AcReferenceIndex`] implementation for production sweep phase.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::sweep::{AcReferenceIndex, AcReferenceWitness, SweepError};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed AC reference index — real production implementation.
///
/// Finds AC entries that reference a given blob digest and were created
/// after the mark anchor (INV-GC-004 protect-if->= gate).
#[derive(Clone, Debug)]
pub struct D1AcReferenceIndex {
    client: Arc<D1HttpClient>,
}

impl D1AcReferenceIndex {
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::sweep::AcReferenceIndex for D1AcReferenceIndex {
    async fn find_re_reference(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::sweep::BlobDigest,
        mark_started_at_ms: u64,
    ) -> Result<Option<AcReferenceWitness>, corelink_gc::sweep::SweepError> {
        let rows = self.client
            .query(
                r#"
                SELECT ac.digest AS action_digest, ac.created_at_ms
                FROM ac_meta ac, json_each(ac.blob_refs)
                WHERE ac.tenant_id = ?1
                  AND json_each.value = ?2
                  AND ac.created_at_ms >= ?3
                ORDER BY ac.created_at_ms DESC
                LIMIT 1
            "#,
                &[
                    json!(tenant_id.to_string()),
                    json!(digest.as_str()),
                    json!(mark_started_at_ms as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::sweep::SweepError::Backend(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }

        let row = &rows[0];
        let action_digest = row
            .get("action_digest")
            .and_then(|v| v.as_str())
            .ok_or_else(|| corelink_gc::sweep::SweepError::Backend("action_digest missing".to_string()))?
            .to_string();

        let created_at_ms = row
            .get("created_at_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;

        Ok(Some(AcReferenceWitness {
            action_digest,
            created_at_ms,
        }))
    }
}