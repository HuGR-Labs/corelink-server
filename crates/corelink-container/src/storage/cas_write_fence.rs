//! D1-backed write lease for the native R2 CAS path.
//!
//! GC's `gc_purge_intent` row is a durable reader/writer fence, but a D1
//! request cannot remain open while the container waits for R2.  The native
//! CAS writer therefore claims a short-lived `cas_write_intent` row before
//! the first R2 mutation.  GC acquisition excludes that row, and the guarded
//! metadata commit consumes only the exact request token.  The owner then
//! releases that token; a crashed writer is reclaimed only after the bounded
//! lease, and its old token cannot commit after a newer writer takes over.

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use super::d1_http::D1HttpClient;

/// Maximum interval a crashed native writer can hold the D1 lease.
pub const CAS_WRITE_LEASE_MS: u64 = 15 * 60 * 1000;

/// A D1 lease authorising one tenant/digest CAS write attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasWriteLease {
    tenant_id: String,
    digest: String,
    request_id: String,
}

impl CasWriteLease {
    /// Tenant scope bound into every subsequent lease operation.
    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Plain canonical digest bound into every subsequent lease operation.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Opaque lease token used to fence stale writers.
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

/// Result of committing the metadata side of a fenced CAS write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CasWriteCommit {
    /// A new `blob_meta` row was inserted with `refcount = 1`.
    Inserted,
    /// An existing live row was re-referenced atomically.
    Referenced,
}

/// Explicit dependency required by the live R2 CAS writer.
pub trait CasWriteFence: Send + Sync + core::fmt::Debug {
    /// Claim the tenant/digest write lease before touching R2.
    fn begin(&self, tenant_id: &str, digest: &str, now_ms: u64) -> Result<CasWriteLease, String>;

    /// Commit/re-reference `blob_meta` while consuming the exact lease token.
    fn commit(
        &self,
        lease: &CasWriteLease,
        size_bytes: u64,
        now_ms: u64,
    ) -> Result<CasWriteCommit, String>;

    /// Release a lease after an R2 or pre-commit failure. Lease expiry is the
    /// recovery fallback if this cleanup request itself cannot reach D1.
    fn abort(&self, lease: &CasWriteLease) -> Result<(), String>;
}

/// Production implementation over the same D1 client used by GC.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct D1CasWriteFence {
    d1: Arc<D1HttpClient>,
}

impl D1CasWriteFence {
    /// Construct a fence over a shared D1 HTTP client.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    fn query_sync(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<super::d1_http::D1Row>, String> {
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| "CAS write fence requires a running Tokio runtime".to_owned())?;
        tokio::task::block_in_place(|| handle.block_on(self.d1.query(sql, params)))
    }

    fn validate_scope(tenant_id: &str, digest: &str) -> Result<(), String> {
        Uuid::parse_str(tenant_id)
            .map_err(|_| "CAS write fence requires a canonical tenant UUID".to_owned())?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("CAS write fence received a non-canonical digest".to_owned());
        }
        Ok(())
    }
}

impl CasWriteFence for D1CasWriteFence {
    fn begin(&self, tenant_id: &str, digest: &str, now_ms: u64) -> Result<CasWriteLease, String> {
        Self::validate_scope(tenant_id, digest)?;
        // A timed-out owner is reclaimed by the GC acquisition transaction;
        // this writer-side claim remains conservative and refuses any existing
        // lease so a stale completion can never overwrite a newer owner.
        let request_id = format!("cas-write:{tenant_id}:{digest}:{now_ms}");
        let rows = self.query_sync(
            "INSERT INTO cas_write_intent
                 (tenant_id, digest, request_id, state, started_at, updated_at)
             SELECT ?1, ?2, ?3, 'writing', ?4, ?4
             WHERE NOT EXISTS (
                 SELECT 1 FROM gc_purge_intent AS pi
                 WHERE pi.tenant_id = ?1 AND pi.digest = ?2
                   AND pi.state IN ('purging', 'r2_deleted', 'retry')
             )
               AND NOT EXISTS (
                 SELECT 1 FROM blob_meta AS bm
                 WHERE bm.tenant_id = ?1 AND bm.digest = ?2
                   AND bm.deleted_at IS NOT NULL
               )
               AND NOT EXISTS (
                 SELECT 1 FROM cas_write_intent AS wi
                 WHERE wi.tenant_id = ?1 AND wi.digest = ?2
                   AND wi.state = 'writing'
               )
             RETURNING request_id",
            &[
                json!(tenant_id),
                json!(digest),
                json!(request_id),
                json!(now_ms),
            ],
        )?;
        if rows.is_empty() {
            return Err(
                "CAS write refused: active GC purge or another writer owns the D1 fence".to_owned(),
            );
        }
        Ok(CasWriteLease {
            tenant_id: tenant_id.to_owned(),
            digest: digest.to_owned(),
            request_id,
        })
    }

    fn commit(
        &self,
        lease: &CasWriteLease,
        size_bytes: u64,
        now_ms: u64,
    ) -> Result<CasWriteCommit, String> {
        if size_bytes == 0 {
            return Err("CAS write metadata rejects an empty blob".to_owned());
        }
        let existing = self.query_sync(
            "SELECT deleted_at FROM blob_meta
             WHERE tenant_id = ?1 AND digest = ?2 LIMIT 1",
            &[json!(lease.tenant_id()), json!(lease.digest())],
        )?;
        if existing
            .first()
            .and_then(|row| row.get("deleted_at"))
            .is_some_and(|v| !v.is_null())
        {
            return Err("CAS write refused: blob metadata is tombstoned".to_owned());
        }
        let rows = self.query_sync(
            "INSERT INTO blob_meta
                 (tenant_id, digest, size_bytes, refcount, created_at,
                  last_accessed_at, region)
             SELECT ?1, ?2, ?3, 1, ?4, ?4,
                    (SELECT primary_region FROM tenant WHERE tenant_id = ?1)
             WHERE EXISTS (
                 SELECT 1 FROM cas_write_intent AS wi
                 WHERE wi.tenant_id = ?1 AND wi.digest = ?2
                   AND wi.request_id = ?5 AND wi.state = 'writing'
             )
               AND NOT EXISTS (
                 SELECT 1 FROM gc_purge_intent AS pi
                 WHERE pi.tenant_id = ?1 AND pi.digest = ?2
                   AND pi.state IN ('purging', 'r2_deleted', 'retry')
             )
             ON CONFLICT (tenant_id, digest) DO UPDATE SET
                 refcount = blob_meta.refcount + 1,
                 last_accessed_at = excluded.last_accessed_at
             WHERE blob_meta.deleted_at IS NULL
               AND NOT EXISTS (
                 SELECT 1 FROM gc_purge_intent AS pi
                 WHERE pi.tenant_id = blob_meta.tenant_id
                   AND pi.digest = blob_meta.digest
                   AND pi.state IN ('purging', 'r2_deleted', 'retry')
               )
             RETURNING refcount",
            &[
                json!(lease.tenant_id()),
                json!(lease.digest()),
                json!(size_bytes),
                json!(now_ms),
                json!(lease.request_id()),
            ],
        )?;
        if rows.is_empty() {
            return Err("CAS write metadata commit lost its D1 fence".to_owned());
        }
        if existing.is_empty() {
            Ok(CasWriteCommit::Inserted)
        } else {
            Ok(CasWriteCommit::Referenced)
        }
    }

    fn abort(&self, lease: &CasWriteLease) -> Result<(), String> {
        let _ = self.query_sync(
            "DELETE FROM cas_write_intent
             WHERE tenant_id = ?1 AND digest = ?2
               AND request_id = ?3 AND state = 'writing'",
            &[
                json!(lease.tenant_id()),
                json!(lease.digest()),
                json!(lease.request_id()),
            ],
        )?;
        Ok(())
    }
}
