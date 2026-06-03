//! [`NegativeCache`] — KV-backed negative cache for the CoreLink CAS read
//! path (WI-S02-005).
//!
//! Caches `(tenant, digest) → MissReason` results in a per-region
//! Cloudflare Workers KV namespace so probe storms short-circuit before
//! reaching D1 + R2. Per WI §6.1.3 + REG-NEGATIVE-002 (revised
//! Lote 10.2bis):
//!
//! - **`GetBlob` (single-digest read) MAY short-circuit on cache hit**
//!   — invalidation hook on the write path collapses stale negatives
//!   immediately; clients retry naturally on the rare race window
//!   bounded by TTL ≤ 300 s.
//! - **`FindMissingBlobs` (batch dedup) MUST always fall through to
//!   D1 + R2 HEAD** per REG-NEGATIVE-001 — Bazel's upload decision is
//!   driven by the response, so a stale "missing" answer is more
//!   harmful than a redundant upload. Callers wire the cache as a
//!   *hint* only: a hit may be passed alongside, but the
//!   authoritative path always walks D1 + R2.
//!
//! ## Canonical key format
//!
//! ```text
//! ac_neg:<region>:<HMAC16>:<digest_hex>
//! ```
//!
//! Constructed by [`canonical_key`] in this module. Outside callers
//! cannot construct keys directly — every code path goes through a
//! [`NegativeCache`] method that derives the key internally from the
//! supplied [`crate::TenantCtx`]. This is REG-NAMESPACE-002 enforced
//! at the type-system / module-visibility level (analogous to the R2
//! adapter; see [`crate::storage::r2`]).
//!
//! ## Invariants
//!
//! - **INV-TENANT-ISOLATION** (CRITICAL): the canonical key embeds the
//!   16-char `HMAC16(TDK, tenant_id)` prefix; cross-tenant cache
//!   poisoning is impossible by construction (a Tenant-A negative
//!   never collides with a Tenant-B key, except under the
//!   ≈ 2^-96 prefix-collision probability proved out by
//!   `corelink-tenant-path`'s property tests).
//! - **INV-CAS-IMMUTABILITY** (CRITICAL): tombstoned blobs surface as
//!   [`crate::cache::miss_reason::MissReason::Tombstoned`] in the
//!   internal audit chain; the wire response is uniform 404 per
//!   ADR-0028.
//!
//! ## Failure semantics
//!
//! KV faults are treated as **soft cache misses**: the cache layer
//! never propagates a transport error to the read handler — that would
//! collapse the entire read path under a KV outage. Instead, the
//! cache returns `Ok(None)` (cache miss) and surfaces the underlying
//! error via the metrics observer in S-09. The read handler then falls
//! through to D1 + R2 on its own.

use core::fmt;

use corelink_hash::Digest;
use corelink_tenant_path::TenantPrefix;

use super::kv::{KvBackend, KvError};
use super::miss_reason::MissReason;
use crate::tenant::TenantCtx;
use crate::Region;

/// Canonical default TTL for negative-cache entries (seconds).
///
/// Per WI §9.3 + PAT-KV-TTL-001. Values < 60 s eliminate the cost
/// benefit; values > 600 s overshoot the business stale-window
/// tolerance. The Cloudflare Workers KV runtime requires
/// `expirationTtl ≥ 60`; the default + the floor enforced by
/// [`NegativeCache::with_ttl`] keep us safely above.
pub const DEFAULT_NEGATIVE_CACHE_TTL_SECS: u64 = 300;

/// Cloudflare Workers KV minimum TTL. Values below this would be
/// rejected by the production binding. Matches the
/// `expirationTtl ≥ 60` contract.
pub const MIN_NEGATIVE_CACHE_TTL_SECS: u64 = 60;

/// Build the canonical KV key for `(region, prefix, digest)`.
///
/// Format: `ac_neg:<region>:<HMAC16>:<digest_hex>`. Per
/// `remote_cache_product_profile.md §7.1` (REG-NEGATIVE-002 revised
/// Lote 10.2bis). Cross-tenant safety derives from the embedded
/// `HMAC16` prefix; no plaintext `tenant_id` appears anywhere in
/// the key.
///
/// `pub(crate)` so outside callers cannot bypass the [`NegativeCache`]
/// surface. Tests in this module can still hit it directly for
/// shape/assertion coverage.
#[must_use]
pub(crate) fn canonical_key(region: Region, prefix: &TenantPrefix, digest: &Digest) -> String {
    let hex = digest.to_hex();
    debug_assert_eq!(
        hex.len(),
        64,
        "BLAKE3 hex must be 64 chars (Digest::to_hex contract)",
    );
    format!(
        "ac_neg:{region}:{prefix}:{hex}",
        region = region.bucket_suffix(),
        prefix = prefix.as_str(),
    )
}

/// Errors surfaced by [`NegativeCache`] for callers that explicitly
/// opt into hard error propagation. The default surface
/// ([`NegativeCache::lookup`] / [`NegativeCache::put_miss`] /
/// [`NegativeCache::invalidate_on_write`]) folds backend faults into
/// soft misses so the read handler is never collapsed by a KV
/// outage; explicit error variants exist here so admin/diag tooling
/// (S-13 forward) can surface them.
#[derive(Debug, thiserror::Error)]
pub enum NegativeCacheError {
    /// The supplied [`TenantCtx`] is pinned to a different region
    /// than this cache instance. Programmer error — never raised on
    /// a client request.
    #[error("region mismatch (cache={cache}, ctx={ctx})")]
    RegionMismatch {
        /// The region this cache instance is bound to.
        cache: Region,
        /// The region carried by the `TenantCtx` the caller passed in.
        ctx: Region,
    },

    /// The configured TTL fell below the Cloudflare Workers KV
    /// floor (60 s). Refused at construction — never raised on a
    /// hot-path call.
    #[error(
        "negative-cache TTL {ttl_secs}s is below the Cloudflare Workers KV minimum ({MIN_NEGATIVE_CACHE_TTL_SECS}s)"
    )]
    TtlBelowKvMinimum {
        /// The TTL value the caller requested (in seconds).
        ttl_secs: u64,
    },
}

/// KV-backed negative cache for the CoreLink CAS read path.
///
/// Holds an [`std::sync::Arc`]-shareable backend so a single cache instance can be
/// fanned out across the request handler stack. Construction requires
/// a [`Region`] (residency anchor) — the `lookup` /
/// `put_miss` / `invalidate_on_write` methods reject any caller whose
/// [`TenantCtx`] is pinned to a different region (defense against
/// dispatcher misrouting; analogous to
/// [`crate::storage::r2::R2Writer`]'s `RegionMismatch`).
pub struct NegativeCache<K: KvBackend> {
    region: Region,
    kv: K,
    ttl_secs: u64,
}

impl<K: KvBackend> fmt::Debug for NegativeCache<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NegativeCache")
            .field("region", &self.region)
            .field("ttl_secs", &self.ttl_secs)
            .finish_non_exhaustive()
    }
}

impl<K: KvBackend> NegativeCache<K> {
    /// Construct a cache pinned to `region` over `kv` with the
    /// canonical [`DEFAULT_NEGATIVE_CACHE_TTL_SECS`] TTL.
    #[must_use]
    pub fn new(region: Region, kv: K) -> Self {
        Self {
            region,
            kv,
            ttl_secs: DEFAULT_NEGATIVE_CACHE_TTL_SECS,
        }
    }

    /// Construct a cache with an explicit TTL. The TTL is bounded
    /// below by the Cloudflare Workers KV floor
    /// ([`MIN_NEGATIVE_CACHE_TTL_SECS`]) — values below it are
    /// rejected with [`NegativeCacheError::TtlBelowKvMinimum`].
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::TtlBelowKvMinimum`] when
    /// `ttl_secs < `[`MIN_NEGATIVE_CACHE_TTL_SECS`]. The caller MUST
    /// either drop down to [`Self::new`] (canonical 300 s) or
    /// raise the requested TTL.
    pub fn with_ttl(region: Region, kv: K, ttl_secs: u64) -> Result<Self, NegativeCacheError> {
        if ttl_secs < MIN_NEGATIVE_CACHE_TTL_SECS {
            return Err(NegativeCacheError::TtlBelowKvMinimum { ttl_secs });
        }
        Ok(Self {
            region,
            kv,
            ttl_secs,
        })
    }

    /// The [`Region`] this cache instance is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// The configured TTL (seconds).
    #[must_use]
    pub const fn ttl_secs(&self) -> u64 {
        self.ttl_secs
    }

    /// Borrow the underlying KV backend (for diagnostics / metrics).
    #[must_use]
    pub const fn backend(&self) -> &K {
        &self.kv
    }

    /// Lookup whether `(ctx.tenant_id(), digest)` is known-missing.
    ///
    /// Returns:
    /// - `Ok(Some(MissReason))` on cache hit — the read handler MAY
    ///   short-circuit on a `GetBlob` single-digest read (per
    ///   REG-NEGATIVE-002); MUST NOT short-circuit on
    ///   `FindMissingBlobs` (per REG-NEGATIVE-001).
    /// - `Ok(None)` on cache miss / TTL expiry / KV transport fault
    ///   / value corruption (soft-miss policy: cache faults never
    ///   propagate; the read handler falls through to D1 + R2).
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::RegionMismatch`] only when the
    /// supplied [`TenantCtx`] is pinned to a different region than
    /// this cache instance — a programmer / wiring error, never a
    /// client-driven fault.
    pub async fn lookup(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<Option<MissReason>, NegativeCacheError> {
        self.guard_region(ctx)?;
        let key = canonical_key(self.region, ctx.prefix(), digest);
        // Soft-miss policy: KV transport faults / corruption do
        // NOT propagate; the cache returns `None` so the read
        // handler falls through to D1 + R2.
        match self.kv.get(&key).await {
            Ok(Some(bytes)) => match decode_value(&bytes) {
                Ok(reason) => Ok(Some(reason)),
                Err(_corrupt) => Ok(None),
            },
            Ok(None) => Ok(None),
            Err(_backend_fault) => Ok(None),
        }
    }

    /// Populate the cache: mark `(ctx.tenant_id(), digest)` as known-
    /// missing with `reason`. TTL is the cache's configured value
    /// ([`Self::ttl_secs`]).
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::RegionMismatch`] only when the
    /// supplied [`TenantCtx`] is pinned to a different region than
    /// this cache instance.
    ///
    /// KV transport faults are folded into a soft-fail (the call
    /// returns `Ok(())` so the read handler is never blocked on a
    /// cache populate); production observers (S-09) surface the
    /// underlying error via the metrics pipe.
    pub async fn put_miss(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
        reason: MissReason,
    ) -> Result<(), NegativeCacheError> {
        self.guard_region(ctx)?;
        let key = canonical_key(self.region, ctx.prefix(), digest);
        // Soft-fail: a KV outage mid-populate is logged (via the
        // future S-09 observer) but never blocks the read handler.
        let _ = self
            .kv
            .put_with_ttl(&key, encode_value(reason), self.ttl_secs)
            .await;
        Ok(())
    }

    /// Invalidate the cache entry for `(ctx.tenant_id(), digest)` —
    /// called by the WI-S01-005 PutBlob handler on every successful
    /// CAS write so a stale negative does not survive past the
    /// write. Idempotent (no-op on a missing key); a write that
    /// fires repeatedly under client retry never produces an error.
    ///
    /// # Errors
    ///
    /// Returns [`NegativeCacheError::RegionMismatch`] only when the
    /// supplied [`TenantCtx`] is pinned to a different region than
    /// this cache instance. KV transport faults are soft-failed
    /// identically to [`Self::put_miss`].
    pub async fn invalidate_on_write(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<(), NegativeCacheError> {
        self.guard_region(ctx)?;
        let key = canonical_key(self.region, ctx.prefix(), digest);
        let _ = self.kv.delete(&key).await;
        Ok(())
    }

    /// Strict variant of [`Self::lookup`] that **does** propagate KV
    /// transport faults / value corruption back to the caller. Used
    /// by the property-test suite to assert behavior under
    /// fault-injection without the soft-miss fold; production
    /// callers should use [`Self::lookup`].
    ///
    /// # Errors
    ///
    /// - [`NegativeCacheError::RegionMismatch`] — region mismatch.
    /// - Returns the underlying [`KvError`] re-wrapped in
    ///   [`NegativeCacheStrictError`].
    pub async fn lookup_strict(
        &self,
        ctx: &TenantCtx,
        digest: &Digest,
    ) -> Result<Option<MissReason>, NegativeCacheStrictError> {
        self.guard_region(ctx)
            .map_err(NegativeCacheStrictError::Cache)?;
        let key = canonical_key(self.region, ctx.prefix(), digest);
        let raw = self
            .kv
            .get(&key)
            .await
            .map_err(NegativeCacheStrictError::Kv)?;
        match raw {
            Some(bytes) => decode_value(&bytes)
                .map(Some)
                .map_err(NegativeCacheStrictError::Kv),
            None => Ok(None),
        }
    }

    fn guard_region(&self, ctx: &TenantCtx) -> Result<(), NegativeCacheError> {
        if ctx.region() != self.region {
            return Err(NegativeCacheError::RegionMismatch {
                cache: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }
}

/// Strict-surface error for [`NegativeCache::lookup_strict`].
#[derive(Debug, thiserror::Error)]
pub enum NegativeCacheStrictError {
    /// Cache-side error (region mismatch / TTL config).
    #[error(transparent)]
    Cache(#[from] NegativeCacheError),
    /// KV transport / corruption error surfaced from the backend.
    #[error(transparent)]
    Kv(#[from] KvError),
}

/// Encode a [`MissReason`] for storage in KV. A single ASCII byte —
/// see [`MissReason::tag`].
#[must_use]
fn encode_value(reason: MissReason) -> Vec<u8> {
    vec![reason.tag()]
}

/// Decode the KV-stored value back into a [`MissReason`].
fn decode_value(raw: &[u8]) -> Result<MissReason, KvError> {
    match raw {
        [tag] => MissReason::from_tag(*tag),
        _ => Err(KvError::Corrupt {
            detail: format!(
                "negative-cache value must be exactly 1 byte; got {len}",
                len = raw.len()
            ),
        }),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: unwrap/panic on a failing assertion is itself a test failure"
)]
mod unit_tests {
    use super::*;
    use crate::cache::kv::{AlwaysFailingKv, FakeClock, InMemoryKv};
    use corelink_hash::Digest;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn tdk_zero() -> TenantDerivationKey {
        TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
    }

    fn ctx(region: Region, tid: Uuid) -> TenantCtx {
        TenantCtx::new(&tdk_zero(), tid, region)
    }

    fn nil_uuid_a() -> Uuid {
        // Stable test UUIDs; values are arbitrary but distinct.
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000001").expect("static uuid parses")
    }

    fn nil_uuid_b() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000002").expect("static uuid parses")
    }

    #[test]
    fn canonical_key_shape_matches_grammar() {
        let prefix = derive_prefix(&tdk_zero(), nil_uuid_a());
        let digest = Digest::compute(b"x");
        let key = canonical_key(Region::Wnam, &prefix, &digest);
        let parts: Vec<&str> = key.split(':').collect();
        assert_eq!(parts.len(), 4, "exactly 4 colon-segments: {key}");
        assert_eq!(parts.first().copied(), Some("ac_neg"));
        assert_eq!(parts.get(1).copied(), Some("wnam"));
        assert_eq!(parts.get(2).map(|s| s.len()), Some(16));
        assert_eq!(parts.get(3).map(|s| s.len()), Some(64));
    }

    #[test]
    fn canonical_key_segments_pin_to_region() {
        let prefix = derive_prefix(&tdk_zero(), nil_uuid_a());
        let digest = Digest::compute(b"y");
        for region in [Region::Wnam, Region::Weur, Region::Sam] {
            let key = canonical_key(region, &prefix, &digest);
            assert!(
                key.starts_with(&format!("ac_neg:{}:", region.bucket_suffix())),
                "key {key:?} did not start with ac_neg:{}:",
                region.bucket_suffix(),
            );
        }
    }

    #[test]
    fn canonical_key_no_plaintext_tenant_id() {
        let tid = nil_uuid_a();
        let prefix = derive_prefix(&tdk_zero(), tid);
        let digest = Digest::compute(b"z");
        let key = canonical_key(Region::Wnam, &prefix, &digest);
        assert!(
            !key.contains(&tid.to_string()),
            "key MUST NOT carry plaintext tenant_id: {key}",
        );
    }

    #[tokio::test]
    async fn lookup_miss_returns_none_when_cache_empty() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        let got = cache.lookup(&ctx, &digest).await.unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn put_then_lookup_returns_reason() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        cache
            .put_miss(&ctx, &digest, MissReason::NotFound)
            .await
            .unwrap();
        let got = cache.lookup(&ctx, &digest).await.unwrap();
        assert_eq!(got, Some(MissReason::NotFound));
    }

    #[tokio::test]
    async fn put_persists_every_miss_reason_variant() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());

        let d_nf = Digest::compute(b"a");
        let d_tomb = Digest::compute(b"b");
        let d_xt = Digest::compute(b"c");

        cache
            .put_miss(&ctx_a, &d_nf, MissReason::NotFound)
            .await
            .unwrap();
        cache
            .put_miss(&ctx_a, &d_tomb, MissReason::Tombstoned)
            .await
            .unwrap();
        cache
            .put_miss(&ctx_a, &d_xt, MissReason::CrossTenantMasked)
            .await
            .unwrap();

        assert_eq!(
            cache.lookup(&ctx_a, &d_nf).await.unwrap(),
            Some(MissReason::NotFound)
        );
        assert_eq!(
            cache.lookup(&ctx_a, &d_tomb).await.unwrap(),
            Some(MissReason::Tombstoned)
        );
        assert_eq!(
            cache.lookup(&ctx_a, &d_xt).await.unwrap(),
            Some(MissReason::CrossTenantMasked)
        );
    }

    #[tokio::test]
    async fn ttl_expiry_evicts_via_fake_clock() {
        let clock = FakeClock::new(1_000_000);
        let kv = InMemoryKv::with_clock(clock);
        let cache = NegativeCache::new(Region::Wnam, kv);
        let ctx = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");

        cache
            .put_miss(&ctx, &digest, MissReason::NotFound)
            .await
            .unwrap();
        assert!(cache.lookup(&ctx, &digest).await.unwrap().is_some());
        // Advance past TTL.
        cache
            .backend()
            .clock()
            .advance(std::time::Duration::from_secs(
                DEFAULT_NEGATIVE_CACHE_TTL_SECS + 1,
            ));
        let got = cache.lookup(&ctx, &digest).await.unwrap();
        assert_eq!(got, None, "stale entry must evict on TTL boundary");
    }

    #[tokio::test]
    async fn cross_tenant_isolation_unit() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let ctx_b = ctx(Region::Wnam, nil_uuid_b());
        let digest = Digest::compute(b"shared body");

        cache
            .put_miss(&ctx_a, &digest, MissReason::NotFound)
            .await
            .unwrap();

        // Tenant B sees no negative for the same digest.
        let got = cache.lookup(&ctx_b, &digest).await.unwrap();
        assert_eq!(
            got, None,
            "Tenant B MUST NOT observe Tenant A's cached negative",
        );

        // Tenant A still sees their own.
        let got_a = cache.lookup(&ctx_a, &digest).await.unwrap();
        assert_eq!(got_a, Some(MissReason::NotFound));
    }

    #[tokio::test]
    async fn invalidate_removes_entry() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        cache
            .put_miss(&ctx_a, &digest, MissReason::NotFound)
            .await
            .unwrap();
        assert!(cache.lookup(&ctx_a, &digest).await.unwrap().is_some());
        cache.invalidate_on_write(&ctx_a, &digest).await.unwrap();
        let got = cache.lookup(&ctx_a, &digest).await.unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn invalidate_is_idempotent_on_missing_key() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"never-cached");
        // Two consecutive invalidates with no prior put_miss must succeed.
        cache.invalidate_on_write(&ctx_a, &digest).await.unwrap();
        cache.invalidate_on_write(&ctx_a, &digest).await.unwrap();
    }

    #[tokio::test]
    async fn region_mismatch_rejected_on_lookup() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_misrouted = ctx(Region::Weur, nil_uuid_a());
        let digest = Digest::compute(b"x");
        let err = cache.lookup(&ctx_misrouted, &digest).await.unwrap_err();
        match err {
            NegativeCacheError::RegionMismatch { cache, ctx } => {
                assert_eq!(cache, Region::Wnam);
                assert_eq!(ctx, Region::Weur);
            }
            other => panic!("expected RegionMismatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn region_mismatch_rejected_on_put_miss() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_misrouted = ctx(Region::Sam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        let err = cache
            .put_miss(&ctx_misrouted, &digest, MissReason::NotFound)
            .await
            .unwrap_err();
        assert!(matches!(err, NegativeCacheError::RegionMismatch { .. }));
    }

    #[tokio::test]
    async fn region_mismatch_rejected_on_invalidate() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_misrouted = ctx(Region::Sam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        let err = cache
            .invalidate_on_write(&ctx_misrouted, &digest)
            .await
            .unwrap_err();
        assert!(matches!(err, NegativeCacheError::RegionMismatch { .. }));
    }

    #[tokio::test]
    async fn ttl_below_kv_floor_is_rejected() {
        let err = NegativeCache::with_ttl(Region::Wnam, InMemoryKv::new(), 30).unwrap_err();
        match err {
            NegativeCacheError::TtlBelowKvMinimum { ttl_secs } => assert_eq!(ttl_secs, 30),
            other => panic!("expected TtlBelowKvMinimum, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ttl_at_kv_floor_is_accepted() {
        let cache =
            NegativeCache::with_ttl(Region::Wnam, InMemoryKv::new(), MIN_NEGATIVE_CACHE_TTL_SECS)
                .expect("60s is the floor; must accept");
        assert_eq!(cache.ttl_secs(), MIN_NEGATIVE_CACHE_TTL_SECS);
    }

    #[tokio::test]
    async fn kv_outage_softfails_lookup_to_none() {
        let cache = NegativeCache::new(Region::Wnam, AlwaysFailingKv::new("simulated KV outage"));
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        // Soft-miss policy: KV outage MUST NOT block the read.
        let got = cache.lookup(&ctx_a, &digest).await.unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn kv_outage_softfails_put_miss_ok() {
        let cache = NegativeCache::new(Region::Wnam, AlwaysFailingKv::new("simulated KV outage"));
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        // put_miss soft-fails: never blocks the populate path.
        cache
            .put_miss(&ctx_a, &digest, MissReason::NotFound)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn kv_outage_softfails_invalidate_ok() {
        let cache = NegativeCache::new(Region::Wnam, AlwaysFailingKv::new("simulated KV outage"));
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        cache.invalidate_on_write(&ctx_a, &digest).await.unwrap();
    }

    #[tokio::test]
    async fn corrupt_kv_value_softfails_lookup_to_none() {
        let kv = InMemoryKv::new();
        let cache = NegativeCache::new(Region::Wnam, kv);
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        // Poison the underlying KV with a multi-byte payload (legal at
        // KV layer, illegal at our cache encoding).
        let key = canonical_key(Region::Wnam, ctx_a.prefix(), &digest);
        cache.backend().poison_for_test(&key, vec![0x00, 0x01], 60);
        let got = cache.lookup(&ctx_a, &digest).await.unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn lookup_strict_propagates_corrupt_value() {
        let kv = InMemoryKv::new();
        let cache = NegativeCache::new(Region::Wnam, kv);
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");
        let key = canonical_key(Region::Wnam, ctx_a.prefix(), &digest);
        cache.backend().poison_for_test(&key, vec![0xff], 60);
        let err = cache.lookup_strict(&ctx_a, &digest).await.unwrap_err();
        match err {
            NegativeCacheStrictError::Kv(KvError::Corrupt { detail }) => {
                assert!(detail.contains("0xff"), "diagnostic: {detail}");
            }
            other => panic!("expected Kv(Corrupt), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn last_write_wins_on_repeated_put_miss() {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_a = ctx(Region::Wnam, nil_uuid_a());
        let digest = Digest::compute(b"x");

        cache
            .put_miss(&ctx_a, &digest, MissReason::NotFound)
            .await
            .unwrap();
        cache
            .put_miss(&ctx_a, &digest, MissReason::Tombstoned)
            .await
            .unwrap();

        let got = cache.lookup(&ctx_a, &digest).await.unwrap();
        assert_eq!(
            got,
            Some(MissReason::Tombstoned),
            "second put MUST overwrite per last-write-wins semantics",
        );
    }
}
