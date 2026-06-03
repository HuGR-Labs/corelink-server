//! AC-flavor negative cache wrapper around the canonical
//! [`crate::cache::negative::NegativeCache`] (WI-S04-001 §6.1.5
//! step-1 + §9.3 + WI brief).
//!
//! ## Why a thin wrapper, not a clone
//!
//! The S-02 WI-S02-005 negative cache is the canonical KV-backed
//! cross-tenant-safe `(region, tenant_prefix, digest)`-keyed cache for
//! the CAS read path. The AC handler reuses the same KV namespace key
//! shape (`ac_neg:<region>:<HMAC16>:<digest_hex>`) per
//! `remote_cache_product_profile.md §7.1` (REG-NEGATIVE-002). The only
//! S-04 deltas vs the CAS use are:
//!
//! - **TTL is shorter**: 60 s for AC vs 300 s for CAS (AC entries
//!   stale fast post-rebuild; WI §9.3). Honored by the wrapper at
//!   construction time via [`crate::cache::negative::NegativeCache::with_ttl`].
//! - **The "miss reason" is uniformly [`MissReason::NotFound`]**: AC
//!   has no "tombstoned" / "cross-tenant masked" distinction at the
//!   negative-cache layer (the row simply doesn't exist for the
//!   tenant). Mapped by [`AcNegCache::populate_miss`] internally.
//! - **The handler invalidates on every successful UPDATE** per
//!   `INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE` — same surface as the
//!   CAS write path's `invalidate_on_write`.
//!
//! Wrapping (rather than re-implementing) keeps the canonical key
//! shape, TTL discipline, and soft-fail policy uniform across CAS +
//! AC; any future hardening lands in the underlying cache module.

use core::fmt;

use corelink_hash::Digest;

use crate::cache::kv::KvBackend;
use crate::cache::miss_reason::MissReason;
use crate::cache::negative::{NegativeCache, NegativeCacheError};
use crate::Region;
use crate::TenantCtx;

/// Canonical TTL (seconds) for AC negative-cache entries. **60 s** per
/// WI §9.3 — shorter than the CAS canonical 300 s because AC entries
/// stale fast post-rebuild. Above the Cloudflare Workers KV minimum
/// (60 s).
pub const AC_NEG_CACHE_TTL_SECS: u64 = 60;

/// AC-flavor negative cache. Thin wrapper around
/// [`NegativeCache`] that:
///
/// - Pins the TTL to [`AC_NEG_CACHE_TTL_SECS`] at construction time.
/// - Maps every populate to [`MissReason::NotFound`] (AC has no
///   "tombstoned" / "cross-tenant masked" distinction at this layer
///   per WI §9.3).
/// - Exposes the same `lookup` / `populate_miss` /
///   `invalidate_on_write` surface for handler ergonomics.
pub struct AcNegCache<K: KvBackend> {
    inner: NegativeCache<K>,
}

impl<K: KvBackend> fmt::Debug for AcNegCache<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcNegCache").finish_non_exhaustive()
    }
}

impl<K: KvBackend> AcNegCache<K> {
    /// Construct a fresh AC negative cache with the canonical 60 s
    /// TTL.
    ///
    /// # Errors
    ///
    /// Surfaces [`NegativeCacheError`] from the inner constructor —
    /// in practice unreachable since 60 s == the KV floor (== the
    /// canonical AC TTL); we return the typed error rather than
    /// `unwrap()` to satisfy the crate-strict clippy lint.
    pub fn new(region: Region, kv: K) -> Result<Self, NegativeCacheError> {
        let inner = NegativeCache::with_ttl(region, kv, AC_NEG_CACHE_TTL_SECS)?;
        Ok(Self { inner })
    }

    /// Cache hit ⇒ `Some(())`; cache miss / soft-fail ⇒ `None`.
    /// Distinct from the CAS surface that returns [`MissReason`] —
    /// AC has only one canonical reason (`NotFound`).
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::RegionMismatch`] when the
    /// supplied [`TenantCtx`] is pinned to a different region than
    /// this cache instance — programmer error.
    pub async fn lookup(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<Option<()>, NegativeCacheError> {
        let r = self.inner.lookup(ctx, digest).await?;
        Ok(r.map(|_| ()))
    }

    /// Mark `(ctx.tenant_id(), digest)` as "known-missing AC entry"
    /// with the canonical [`MissReason::NotFound`].
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::RegionMismatch`] when the
    /// supplied [`TenantCtx`] is pinned to a different region than
    /// this cache instance.
    pub async fn populate_miss(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<(), NegativeCacheError> {
        self.inner.put_miss(ctx, digest, MissReason::NotFound).await
    }

    /// Invalidate the cache entry for `(ctx.tenant_id(), digest)` —
    /// called by the AC UPDATE handler on every successful upsert
    /// per `INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE`.
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::RegionMismatch`] when the
    /// supplied [`TenantCtx`] is pinned to a different region than
    /// this cache instance.
    pub async fn invalidate_on_update(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<(), NegativeCacheError> {
        self.inner.invalidate_on_write(ctx, digest).await
    }

    /// Borrow the underlying KV backend (diagnostics).
    #[must_use]
    pub fn backend(&self) -> &K {
        self.inner.backend()
    }

    /// The configured TTL — always [`AC_NEG_CACHE_TTL_SECS`].
    #[must_use]
    pub const fn ttl_secs(&self) -> u64 {
        self.inner.ttl_secs()
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
    use crate::cache::kv::InMemoryKv;
    use corelink_tenant_path::TenantDerivationKey;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn tdk_zero() -> TenantDerivationKey {
        TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
    }

    fn tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    #[tokio::test]
    async fn populate_then_lookup_hits() {
        let cache = AcNegCache::new(Region::Wnam, InMemoryKv::new()).unwrap();
        let ctx = TenantCtx::new(&tdk_zero(), tenant(), Region::Wnam);
        let d = Digest::compute(b"x");
        cache.populate_miss(&ctx, &d).await.unwrap();
        let hit = cache.lookup(&ctx, &d).await.unwrap();
        assert_eq!(hit, Some(()));
    }

    #[tokio::test]
    async fn invalidate_clears_entry() {
        let cache = AcNegCache::new(Region::Wnam, InMemoryKv::new()).unwrap();
        let ctx = TenantCtx::new(&tdk_zero(), tenant(), Region::Wnam);
        let d = Digest::compute(b"x");
        cache.populate_miss(&ctx, &d).await.unwrap();
        cache.invalidate_on_update(&ctx, &d).await.unwrap();
        let hit = cache.lookup(&ctx, &d).await.unwrap();
        assert!(hit.is_none());
    }

    #[tokio::test]
    async fn cross_tenant_isolation() {
        let cache = AcNegCache::new(Region::Wnam, InMemoryKv::new()).unwrap();
        let ctx_a = TenantCtx::new(&tdk_zero(), tenant(), Region::Wnam);
        let ctx_b = TenantCtx::new(
            &tdk_zero(),
            Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap(),
            Region::Wnam,
        );
        let d = Digest::compute(b"shared-action-digest");
        cache.populate_miss(&ctx_a, &d).await.unwrap();
        // B sees nothing.
        let hit = cache.lookup(&ctx_b, &d).await.unwrap();
        assert!(hit.is_none());
        // A still sees their own.
        let hit = cache.lookup(&ctx_a, &d).await.unwrap();
        assert_eq!(hit, Some(()));
    }

    #[tokio::test]
    async fn ttl_is_60s_canonical() {
        let cache = AcNegCache::new(Region::Wnam, InMemoryKv::new()).unwrap();
        assert_eq!(cache.ttl_secs(), AC_NEG_CACHE_TTL_SECS);
    }
}
