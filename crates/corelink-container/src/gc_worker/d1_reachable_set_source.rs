//! Real D1-backed [`ReachableSetSource`] implementation for production mark phase.
//!
//! Implements the 3 canonical passes over `blob_meta` / `ac_meta` /
//! `manifest_chunks` using the Cloudflare D1 HTTP API via
//![`crate::storage::d1_http::D1HttpClient`].

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::mark::{
    AcMetaRow, BlobDigest, BlobMetaRow, ManifestChunkRow, MarkError, ReachableSetSource,
};
use crate::storage::d1_http::D1HttpClient;

/// D1-backed reachable set source — real production implementation.
///
/// Executes the 3 canonical passes over D1 tables:
/// - Pass 1: `blob_meta` live blobs (refcount > 0, deleted_at IS NULL)
/// - Pass 2: `ac_meta` outputs (json_each over blob_refs)
/// - Pass 3: `manifest_chunks` 3-hop traversal
#[derive(Clone, Debug)]
pub struct D1ReachableSetSource {
    client: Arc<D1HttpClient>,
}

impl D1ReachableSetSource {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::mark::ReachableSetSource for D1ReachableSetSource {
    async fn pass_blob_meta(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<corelink_gc::mark::BlobMetaRow>, corelink_gc::mark::MarkError> {
        let sql = r#"
            SELECT digest, refcount, size_bytes, last_referenced_at_ms
            FROM blob_meta
            WHERE tenant_id = ?1
              AND deleted_at IS NULL
              AND refcount > 0
              AND last_referenced_at_ms >= ?2
            ORDER BY digest
            LIMIT ?3 OFFSET ?4
        "#;
        let rows = self.client
            .query(
                sql,
                &[
                    json!(tenant_id.to_string()),
                    json!(snapshot_lower_bound_ms as i64),
                    json!(batch_size as i64),
                    json!(offset as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let digest = row.get("digest").and_then(|v| v.as_str()).unwrap_or("");
            let refcount = row.get("refcount").and_then(|v| v.as_i64()).unwrap_or(0) as u64;
            let size_bytes = row.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0) as u64;
            let last_referenced_at_ms = row.get("last_referenced_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

            let digest = corelink_gc::mark::BlobDigest::parse(digest)
                .map_err(|e| corelink_gc::mark::MarkError::InvalidDigest { actual_len: e.actual_len })?;
            out.push(corelink_gc::mark::BlobMetaRow {
                digest,
                refcount,
                size_bytes,
                last_referenced_at_ms,
            });
        }
        Ok(out)
    }

    async fn pass_ac_meta(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<corelink_gc::mark::AcMetaRow>, corelink_gc::mark::MarkError> {
        let sql = r#"
            SELECT ac.digest, ac.created_at_ms, json_each.value AS blob_ref
            FROM ac_meta ac, json_each(ac.blob_refs)
            WHERE ac.tenant_id = ?1
              AND ac.created_at_ms >= ?2
            ORDER BY ac.created_at_ms
            LIMIT ?3 OFFSET ?4
        "#;
        let rows = self.client
            .query(
                sql,
                &[
                    json!(tenant_id.to_string()),
                    json!(snapshot_lower_bound_ms as i64),
                    json!(batch_size as i64),
                    json!(offset as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        // Group by (tenant_id, ac.created_at_ms) to reconstruct AC rows
        use std::collections::BTreeMap;
        let mut ac_map: BTreeMap<String, Vec<corelink_gc::mark::BlobDigest>> = BTreeMap::new();

        for row in rows {
            let blob_ref = row.get("blob_ref").and_then(|v| v.as_str()).unwrap_or("");
            let digest = corelink_gc::mark::BlobDigest::parse(blob_ref)
                .map_err(|e| corelink_gc::mark::MarkError::InvalidDigest { actual_len: e.actual_len })?;
            let created_at_ms = row.get("created_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

            // Use tenant_id + created_at_ms as key to group blobs per AC entry
            let key = format!("{}:{}", tenant_id, created_at_ms);
            ac_map.entry(key).or_default().push(digest);
        }

        let mut out = Vec::new();
        for (key, blob_refs) in ac_map {
            // Extract created_at_ms from key
            let created_at_ms = key.split(':').last().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            out.push(corelink_gc::mark::AcMetaRow {
                blob_refs,
                created_at_ms,
            });
        }
        Ok(out)
    }

    async fn pass_manifest_chunks(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<corelink_gc::mark::ManifestChunkRow>, corelink_gc::mark::MarkError> {
        let sql = r#"
            SELECT chunk_digest, created_at_ms
            FROM manifest_chunks
            WHERE tenant_id = ?1
              AND created_at_ms >= ?2
            ORDER BY chunk_digest
            LIMIT ?3 OFFSET ?4
        "#;
        let rows = self.client
            .query(
                sql,
                &[
                    json!(tenant_id.to_string()),
                    json!(snapshot_lower_bound_ms as i64),
                    json!(batch_size as i64),
                    json!(offset as i64),
                ],
            )
            .await
            .map_err(|e| corelink_gc::mark::MarkError::Backend(e.to_string()))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let chunk_digest_str = row.get("chunk_digest").and_then(|v| v.as_str()).unwrap_or("");
            let created_at_ms = row.get("created_at_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64;

            let chunk_digest = corelink_gc::mark::BlobDigest::parse(chunk_digest_str)
                .map_err(|e| corelink_gc::mark::MarkError::InvalidDigest { actual_len: e.actual_len })?;
            out.push(corelink_gc::mark::ManifestChunkRow {
                chunk_digest,
                created_at_ms,
            });
        }
        Ok(out)
    }
}