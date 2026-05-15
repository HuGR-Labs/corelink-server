//! Chunk content-addressable store trait + InMemory fake (WI-S05-001
//! §6.1.5).
//!
//! Mirrors the trait-abstraction-defer pattern. Production wiring
//! lands in WI-S05-003 (R2 multipart adapter for chunk PUTs) +
//! WI-S05-004 (D1 schema for refcount tracking) + WI-S05-006
//! (binding mount). The pure-logic handler depends on the
//! [`ChunkStore`] trait surface only.
//!
//! ## Tenant isolation seam
//!
//! [`ChunkKey::new`] is the only legal way to address a chunk row; it
//! takes `(tenant_id, chunk_digest)` by value, mirroring sprint contract
//! §5.3 UNIQUE constraint and `INV-TENANT-ISOLATION`. Cross-tenant
//! probing is structurally unreachable.
//!
//! ## Idempotent UPSERT contract (`CAP-CAS-010` foundation)
//!
//! [`ChunkStore::upsert`] is idempotent: a re-PUT for the same
//! `(tenant_id, chunk_digest)` is a no-op on the bytes (content-
//! addressable) and increments the refcount by 1 on the row. The
//! refcount column matures in WI-S05-004 schema; the in-memory fake
//! tracks it inline so the property tests can assert it directly.
//!
//! ## Anti-scope
//!
//! - The trait surface intentionally does **not** expose
//!   `find_by_chunk_digest` without a tenant_id; cross-tenant dedup is
//!   anti-scope per sprint contract §10.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site"
)]

use core::fmt;
use core::future::Future;
use std::collections::HashMap;
// DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (infallible lock).
// `ChunkStoreError::Backend("…mutex poisoned")` is unreachable here.
use parking_lot::{Mutex, MutexGuard};

use bytes::Bytes;
use corelink_tenant_path::TenantPrefix;
use thiserror::Error;
use uuid::Uuid;

use super::types::ChunkDigest;
use crate::region::Region;

/// Composite primary key for the chunk content store: `(tenant_id,
/// chunk_digest)`. Constructing one is the only legal way to address a
/// chunk through the [`ChunkStore`] trait surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    tenant_id: Uuid,
    chunk_digest: ChunkDigest,
}

impl ChunkKey {
    /// Build a fresh [`ChunkKey`].
    #[must_use]
    pub const fn new(tenant_id: Uuid, chunk_digest: ChunkDigest) -> Self {
        Self {
            tenant_id,
            chunk_digest,
        }
    }

    /// Borrow the tenant id.
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// Borrow the chunk digest.
    #[must_use]
    pub const fn chunk_digest(&self) -> &ChunkDigest {
        &self.chunk_digest
    }
}

/// Snapshot of a chunk row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkRecord {
    /// Composite PK.
    pub key: ChunkKey,
    /// Content bytes.
    pub bytes: Bytes,
    /// Region the chunk was minted under (residency anchor).
    pub region: Region,
    /// Tenant prefix materialized at INSERT time.
    pub tenant_prefix: TenantPrefix,
    /// Refcount column — number of distinct manifests referencing this
    /// chunk. Incremented on every [`ChunkStore::upsert`] call.
    pub refcount: u32,
}

/// Errors surfaced by [`ChunkStore`] methods.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ChunkStoreError {
    /// Backend storage failure.
    #[error("chunk backend unavailable: {0}")]
    Backend(String),
}

/// Trait surface for the chunk content store.
pub trait ChunkStore: Send + Sync {
    /// Idempotent UPSERT — content-addressable insert (no-op on
    /// duplicate bytes); increments refcount on every call.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn upsert<'a>(
        &'a self,
        key: ChunkKey,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        bytes: Bytes,
    ) -> impl Future<Output = Result<u32, ChunkStoreError>> + Send + 'a;

    /// Look up the chunk record. Returns `None` for unknown rows.
    /// Cross-tenant lookups MUST mask as `None`.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn lookup<'a>(
        &'a self,
        key: ChunkKey,
    ) -> impl Future<Output = Result<Option<ChunkRecord>, ChunkStoreError>> + Send + 'a;

    /// Test-only mutation knob — flip a single byte of a stored chunk
    /// to simulate envelope tampering / bit-rot. Return `false` if
    /// the row is absent.
    ///
    /// # Errors
    ///
    /// Backend-class only.
    fn tamper<'a>(
        &'a self,
        key: ChunkKey,
    ) -> impl Future<Output = Result<bool, ChunkStoreError>> + Send + 'a;
}

/// In-memory chunk store fake. Per-instance — no global state (F-001
/// closure 2026-05-01).
pub struct InMemoryChunkStore {
    inner: Mutex<HashMap<ChunkKey, ChunkRecord>>,
}

impl Default for InMemoryChunkStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InMemoryChunkStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryChunkStore").finish_non_exhaustive()
    }
}

impl InMemoryChunkStore {
    /// Construct a fresh in-memory chunk store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Number of distinct chunks present.
    pub fn len(&self) -> Result<usize, ChunkStoreError> {
        Ok(self.lock()?.len())
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> Result<bool, ChunkStoreError> {
        Ok(self.lock()?.is_empty())
    }

    /// Synchronous test-only tamper helper — equivalent to
    /// [`ChunkStore::tamper`] but does not require an async runtime.
    /// Used by handler unit tests that already hold the chunk store
    /// `Arc`.
    pub fn tamper_for_test(&self, key: ChunkKey) -> bool {
        let mut g = match self.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if let Some(rec) = g.get_mut(&key) {
            // Flip the first byte of the chunk so BLAKE3 over the
            // tampered slice diverges from `key.chunk_digest()`.
            let mut new_bytes = rec.bytes.to_vec();
            if let Some(first) = new_bytes.first_mut() {
                *first ^= 0xFF;
            } else {
                // Empty chunk — append a byte to force divergence.
                new_bytes.push(0xAA);
            }
            rec.bytes = Bytes::from(new_bytes);
            true
        } else {
            false
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, HashMap<ChunkKey, ChunkRecord>>, ChunkStoreError> {
        // parking_lot lock is infallible; Err arm unreachable.
        Ok(self.inner.lock())
    }
}

impl ChunkStore for InMemoryChunkStore {
    fn upsert<'a>(
        &'a self,
        key: ChunkKey,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        bytes: Bytes,
    ) -> impl Future<Output = Result<u32, ChunkStoreError>> + Send + 'a {
        async move {
            let mut g = self.lock()?;
            let entry = g.entry(key).or_insert_with(|| ChunkRecord {
                key,
                bytes: bytes.clone(),
                region,
                tenant_prefix: *tenant_prefix,
                refcount: 0,
            });
            // Refcount += 1 on every UPSERT (mirrors WI-S05-004 schema
            // ON CONFLICT DO UPDATE SET refcount = refcount + 1).
            entry.refcount = entry.refcount.saturating_add(1);
            Ok(entry.refcount)
        }
    }

    fn lookup<'a>(
        &'a self,
        key: ChunkKey,
    ) -> impl Future<Output = Result<Option<ChunkRecord>, ChunkStoreError>> + Send + 'a {
        async move {
            let g = self.lock()?;
            Ok(g.get(&key).cloned())
        }
    }

    fn tamper<'a>(
        &'a self,
        key: ChunkKey,
    ) -> impl Future<Output = Result<bool, ChunkStoreError>> + Send + 'a {
        async move { Ok(self.tamper_for_test(key)) }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use zeroize::Zeroizing;

    fn fixed_prefix(tenant: Uuid) -> TenantPrefix {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        derive_prefix(&tdk, tenant)
    }

    #[tokio::test]
    async fn upsert_idempotent_increments_refcount() {
        let store = InMemoryChunkStore::new();
        let tenant = Uuid::nil();
        let prefix = fixed_prefix(tenant);
        let cd = ChunkDigest::compute(b"x");
        let key = ChunkKey::new(tenant, cd);
        let r1 = store
            .upsert(key, Region::Wnam, &prefix, Bytes::from_static(b"x"))
            .await
            .unwrap();
        let r2 = store
            .upsert(key, Region::Wnam, &prefix, Bytes::from_static(b"x"))
            .await
            .unwrap();
        assert_eq!(r1, 1);
        assert_eq!(r2, 2);
    }

    #[tokio::test]
    async fn cross_tenant_lookup_masks_to_none() {
        let store = InMemoryChunkStore::new();
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let prefix_a = fixed_prefix(tenant_a);
        let cd = ChunkDigest::compute(b"x");
        store
            .upsert(
                ChunkKey::new(tenant_a, cd),
                Region::Wnam,
                &prefix_a,
                Bytes::from_static(b"x"),
            )
            .await
            .unwrap();
        let r = store
            .lookup(ChunkKey::new(tenant_b, cd))
            .await
            .unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn tamper_flips_bytes() {
        let store = InMemoryChunkStore::new();
        let tenant = Uuid::nil();
        let prefix = fixed_prefix(tenant);
        let cd = ChunkDigest::compute(b"abcd");
        let key = ChunkKey::new(tenant, cd);
        store
            .upsert(key, Region::Wnam, &prefix, Bytes::from_static(b"abcd"))
            .await
            .unwrap();
        assert!(store.tamper(key).await.unwrap());
        let r = store.lookup(key).await.unwrap().unwrap();
        // Bytes flipped — recomputed digest no longer matches the
        // claimed key.
        let recomputed = ChunkDigest::compute(&r.bytes);
        assert_ne!(&recomputed, key.chunk_digest());
    }
}
