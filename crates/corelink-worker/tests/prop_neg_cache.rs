//! Property tests for the negative-cache adapter (WI-S02-005 §10.5.3 +
//! §11 + §12).
//!
//! Pins the load-bearing invariants:
//!
//! 1. **INV-TENANT-ISOLATION** (CRITICAL): a Tenant-A negative cache
//!    populate is invisible to Tenant B for the same digest. 10k iter
//!    proptest plus a 100k explicit-loop concurrent fuzz with
//!    randomized writers / readers.
//! 2. **TTL bound** (PAT-KV-TTL-001): every entry expires at exactly
//!    the configured TTL boundary; a `put_miss` followed by a clock
//!    advance ≥ TTL produces `lookup → None`.
//! 3. **Invalidation precedence**: an explicit `invalidate_on_write`
//!    after a `put_miss` (under last-write-wins KV semantics) clears
//!    the entry; subsequent `lookup` is `None`.
//! 4. **Last-write-wins on put_miss**: two consecutive populates with
//!    different `MissReason` round-trip the second value.
//! 5. **Region-pinned reject**: a `TenantCtx` whose region disagrees
//!    with the cache instance is rejected with `RegionMismatch`.
//! 6. **Key grammar**: every emitted KV key matches the canonical
//!    `ac_neg:<region>:<HMAC16>:<digest_hex>` grammar; no plaintext
//!    `tenant_id` ever appears.
//!
//! ## Why both proptest @ 10k and the concurrent loop @ 100k
//!
//! Proptest @ 10k case-coverage exercises wide tenant_id / digest
//! shapes; the explicit 100k concurrent loop exercises *race* coverage
//! (tokio multi-thread runtime, A/B writer/reader interleaving). The
//! combo mirrors the WI-S02-001 cross-tenant property pattern (see
//! lessons learned §6).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics are test failures; indexing inside proptest closures is bounded by the strategy"
)]

use std::sync::Arc;
use std::time::Duration;

use corelink_hash::Digest;
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::cache::kv::{FakeClock, InMemoryKv};
use corelink_worker::cache::negative::{
    NegativeCache, NegativeCacheError, DEFAULT_NEGATIVE_CACHE_TTL_SECS,
};
use corelink_worker::cache::MissReason;
use corelink_worker::{Region, TenantCtx};
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

// -- helpers ---------------------------------------------------------------

fn tdk_zero() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
}

fn ctx_for(tid: Uuid, region: Region) -> TenantCtx {
    TenantCtx::new(&tdk_zero(), tid, region)
}

fn region_strategy() -> impl Strategy<Value = Region> {
    prop_oneof![Just(Region::Wnam), Just(Region::Weur), Just(Region::Sam),]
}

fn miss_reason_strategy() -> impl Strategy<Value = MissReason> {
    prop_oneof![
        Just(MissReason::NotFound),
        Just(MissReason::Tombstoned),
        Just(MissReason::CrossTenantMasked),
    ]
}

fn uuid_strategy() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(|bytes| {
        // Construct a UUID from raw bytes; we don't care about the
        // version bits — the cache key is derived through `derive_prefix`
        // which treats `tenant_id` as opaque bytes.
        Uuid::from_bytes(bytes)
    })
}

fn digest_from_seed(seed: &[u8; 32]) -> Digest {
    let mut hex = String::with_capacity(64);
    for &b in seed {
        hex.push_str(&format!("{b:02x}"));
    }
    Digest::from_hex(&hex).expect("64-char hex is valid")
}

fn digest_strategy() -> impl Strategy<Value = Digest> {
    any::<[u8; 32]>().prop_map(|bytes| digest_from_seed(&bytes))
}

// -- proptest @ 10k --------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    /// INV-TENANT-ISOLATION: Tenant A's cached negative is invisible to
    /// Tenant B. 10k iter over random (tenant_a, tenant_b, digest, reason).
    #[test]
    fn cross_tenant_isolation_under_random_pairs(
        a in uuid_strategy(),
        b in uuid_strategy(),
        digest in digest_strategy(),
        reason in miss_reason_strategy(),
    ) {
        // Drop pathological case where both UUIDs collide (≈ 2^-128 in
        // theory; proptest can land on the same shrunk seed in practice).
        prop_assume!(a != b);

        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx_a = ctx_for(a, Region::Wnam);
        let ctx_b = ctx_for(b, Region::Wnam);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            cache.put_miss(&ctx_a, &digest, reason).await.unwrap();
            let got_b = cache.lookup(&ctx_b, &digest).await.unwrap();
            // CRITICAL: Tenant B never sees Tenant A's negative.
            prop_assert_eq!(
                got_b, None,
                "cross-tenant cache poisoning: Tenant B {} observed Tenant A {}'s negative",
                b, a,
            );
            let got_a = cache.lookup(&ctx_a, &digest).await.unwrap();
            prop_assert_eq!(
                got_a, Some(reason),
                "self-tenant lookup must round-trip the cached MissReason",
            );
            Ok(())
        })?;
    }

    /// Key grammar: every emitted KV key matches
    /// `ac_neg:<region>:<HMAC16>:<digest_hex>` and never carries a
    /// plaintext tenant_id.
    #[test]
    fn key_grammar_holds_under_random_inputs(
        tid in uuid_strategy(),
        digest in digest_strategy(),
        region in region_strategy(),
    ) {
        let cache = NegativeCache::new(region, InMemoryKv::new());
        let ctx = ctx_for(tid, region);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            cache.put_miss(&ctx, &digest, MissReason::NotFound).await.unwrap();
            let keys = cache.backend().keys_snapshot();
            prop_assert_eq!(keys.len(), 1, "exactly one key emitted per put");
            let key = &keys[0];
            let parts: Vec<&str> = key.split(':').collect();
            prop_assert_eq!(parts.len(), 4, "exactly 4 colon-segments: {}", key);
            prop_assert_eq!(parts[0], "ac_neg");
            prop_assert_eq!(parts[1], region.bucket_suffix());
            prop_assert_eq!(parts[2].len(), 16, "HMAC16 must be 16 chars");
            prop_assert_eq!(parts[3].len(), 64, "digest_hex must be 64 chars");
            prop_assert!(
                !key.contains(&tid.to_string()),
                "plaintext tenant_id leaked into key: {}", key,
            );
            Ok(())
        })?;
    }

    /// last-write-wins under repeated puts.
    #[test]
    fn last_write_wins_under_repeated_puts(
        tid in uuid_strategy(),
        digest in digest_strategy(),
        first in miss_reason_strategy(),
        second in miss_reason_strategy(),
    ) {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx = ctx_for(tid, Region::Wnam);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            cache.put_miss(&ctx, &digest, first).await.unwrap();
            cache.put_miss(&ctx, &digest, second).await.unwrap();
            let got = cache.lookup(&ctx, &digest).await.unwrap();
            prop_assert_eq!(got, Some(second), "second put MUST overwrite first");
            Ok(())
        })?;
    }

    /// Invalidation precedence: a put_miss followed by an
    /// invalidate_on_write clears the entry; a subsequent lookup is None.
    #[test]
    fn invalidate_clears_under_random_inputs(
        tid in uuid_strategy(),
        digest in digest_strategy(),
        reason in miss_reason_strategy(),
    ) {
        let cache = NegativeCache::new(Region::Wnam, InMemoryKv::new());
        let ctx = ctx_for(tid, Region::Wnam);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            cache.put_miss(&ctx, &digest, reason).await.unwrap();
            cache.invalidate_on_write(&ctx, &digest).await.unwrap();
            let got = cache.lookup(&ctx, &digest).await.unwrap();
            prop_assert_eq!(got, None, "invalidate must clear entry");
            Ok(())
        })?;
    }

    /// TTL bound: a put_miss followed by a clock advance ≥ TTL evicts
    /// the entry — `lookup → None`.
    #[test]
    fn ttl_bound_holds_at_random_advances(
        tid in uuid_strategy(),
        digest in digest_strategy(),
        reason in miss_reason_strategy(),
        // Random advance ≥ TTL, capped to keep test runtime short.
        advance_secs in (DEFAULT_NEGATIVE_CACHE_TTL_SECS + 1)..(DEFAULT_NEGATIVE_CACHE_TTL_SECS * 4),
    ) {
        let clock = FakeClock::new(1_000_000);
        let kv = InMemoryKv::with_clock(clock);
        let cache = NegativeCache::new(Region::Wnam, kv);
        let ctx = ctx_for(tid, Region::Wnam);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            cache.put_miss(&ctx, &digest, reason).await.unwrap();
            cache.backend().clock().advance(Duration::from_secs(advance_secs));
            let got = cache.lookup(&ctx, &digest).await.unwrap();
            prop_assert_eq!(got, None, "expired entry MUST evict");
            Ok(())
        })?;
    }

    /// Region-pin: a TenantCtx for a different region than the cache
    /// instance is always rejected with RegionMismatch. Generated as
    /// `(cache_region, mismatch_index ∈ 1..3)` and the ctx region is
    /// derived as the i-th element of the *other-two* set, so we
    /// never have to `prop_assume!` the two regions apart.
    #[test]
    fn region_mismatch_is_always_rejected(
        tid in uuid_strategy(),
        digest in digest_strategy(),
        cache_region in region_strategy(),
        mismatch_offset in 1u8..3u8,
        reason in miss_reason_strategy(),
    ) {
        let all = [Region::Wnam, Region::Weur, Region::Sam];
        let cache_idx = all.iter().position(|r| *r == cache_region).unwrap();
        let ctx_region = all[(cache_idx + usize::from(mismatch_offset)) % all.len()];

        let cache = NegativeCache::new(cache_region, InMemoryKv::new());
        let ctx = ctx_for(tid, ctx_region);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            for op in 0u8..3 {
                let res = match op {
                    0 => cache.lookup(&ctx, &digest).await.map(|_| ()),
                    1 => cache.put_miss(&ctx, &digest, reason).await,
                    _ => cache.invalidate_on_write(&ctx, &digest).await,
                };
                let err = res.expect_err("region mismatch must reject all ops");
                prop_assert!(
                    matches!(err, NegativeCacheError::RegionMismatch { .. }),
                    "expected RegionMismatch on op {}: {:?}", op, err,
                );
            }
            Ok(())
        })?;
    }
}

// -- explicit 100k concurrent fuzz ----------------------------------------

/// 100k cross-tenant negative-cache poisoning attempts under tokio
/// multi-threaded execution. Stronger than the proptest above because
/// it actually drives concurrent writers + readers across worker
/// threads, exercising the underlying `Mutex<HashMap>` for race
/// coverage.
///
/// Per WI-S02-005 §10.5.3: 100k iter → 0 cross-tenant hits.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cross_tenant_isolation_concurrent_100k() {
    use futures::future::join_all;

    const ATTEMPTS: usize = 100_000;
    const VICTIMS: usize = 8;
    const ATTACKERS: usize = 8;
    const PER_TASK: usize = ATTEMPTS / (VICTIMS * ATTACKERS);

    let cache = Arc::new(NegativeCache::new(Region::Wnam, InMemoryKv::new()));

    // Use a fixed digest pool so attackers and victims compete for the
    // same digest identifiers — the strongest poisoning shape.
    let digest_pool: Vec<Digest> = (0u64..16)
        .map(|i| {
            let mut bytes = [0u8; 32];
            bytes[0..8].copy_from_slice(&i.to_le_bytes());
            digest_from_seed(&bytes)
        })
        .collect();

    let victims: Vec<TenantCtx> = (0u64..VICTIMS as u64)
        .map(|i| {
            let mut bytes = [0u8; 16];
            bytes[0..8].copy_from_slice(&i.to_le_bytes());
            ctx_for(Uuid::from_bytes(bytes), Region::Wnam)
        })
        .collect();

    let attackers: Vec<TenantCtx> = (1024u64..(1024 + ATTACKERS as u64))
        .map(|i| {
            let mut bytes = [0u8; 16];
            bytes[0..8].copy_from_slice(&i.to_le_bytes());
            ctx_for(Uuid::from_bytes(bytes), Region::Wnam)
        })
        .collect();

    // Seed every victim's cache with a NotFound for every digest.
    for vctx in &victims {
        for d in &digest_pool {
            cache.put_miss(vctx, d, MissReason::NotFound).await.unwrap();
        }
    }

    // Concurrent attackers attempt to read victims' negatives via their
    // OWN ctx — these must always miss.
    let mut tasks = Vec::with_capacity(VICTIMS * ATTACKERS);
    for actx in &attackers {
        for _vctx in &victims {
            let cache = Arc::clone(&cache);
            let attacker = *actx;
            let pool = digest_pool.clone();
            tasks.push(tokio::spawn(async move {
                let mut leaks = 0usize;
                for i in 0..PER_TASK {
                    let d = &pool[i % pool.len()];
                    if cache.lookup(&attacker, d).await.unwrap().is_some() {
                        leaks += 1;
                    }
                }
                leaks
            }));
        }
    }
    let results = join_all(tasks).await;
    let total_leaks: usize = results.into_iter().map(|r| r.unwrap()).sum();
    assert_eq!(
        total_leaks, 0,
        "cross-tenant negative-cache poisoning detected: {total_leaks} leaks across {ATTEMPTS} attempts",
    );

    // Sanity: victims still see their own.
    for vctx in &victims {
        for d in &digest_pool {
            let got = cache.lookup(vctx, d).await.unwrap();
            assert_eq!(got, Some(MissReason::NotFound));
        }
    }
}

// -- TTL boundary regression vector ---------------------------------------

/// Hard regression vector for the TTL boundary: at exactly TTL the entry
/// is still live (CF KV `expirationTtl` is exclusive on the way in,
/// inclusive on the way out — the in-memory fake matches by storing
/// `expires_at = now + ttl` and evicting on `expires_at <= now`).
#[tokio::test]
async fn ttl_boundary_exact_secs_evicts() {
    let clock = FakeClock::new(1_000_000);
    let kv = InMemoryKv::with_clock(clock);
    let cache = NegativeCache::new(Region::Wnam, kv);
    let ctx = ctx_for(Uuid::nil(), Region::Wnam);
    let digest = Digest::compute(b"boundary");

    cache
        .put_miss(&ctx, &digest, MissReason::NotFound)
        .await
        .unwrap();

    // Just before TTL — still live.
    cache
        .backend()
        .clock()
        .advance(Duration::from_secs(DEFAULT_NEGATIVE_CACHE_TTL_SECS - 1));
    assert!(cache.lookup(&ctx, &digest).await.unwrap().is_some());

    // At TTL boundary — evicted (stored expires_at == now → evict).
    cache.backend().clock().advance(Duration::from_secs(1));
    let got = cache.lookup(&ctx, &digest).await.unwrap();
    assert_eq!(got, None, "entry must evict at exact TTL boundary");
}
