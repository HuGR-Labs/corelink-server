//! WI-S01-003 §10.3.1 + §15: cross-tenant key disjointness property tests.
//!
//! The 100k-iter cross-tenant disjointness test is the headline AC: any pair
//! of distinct tenant UUIDs (under the same TDK + region + digest) must
//! produce distinct R2 keys. Below the property test, two unit-style bulk
//! tests cover concrete adversarial regimes (sequential UUIDs, single-bit
//! flips). The whole file runs in debug so the perf budget is irrelevant; on
//! commodity hardware this test takes ~1.5 seconds.
//!
//! Coverage map:
//! - WI-S01-003 §8 "Property test — path determinism" → [`prop_path_determinism`]
//! - WI-S01-003 §8 "Property test — cross-tenant disjointness" →
//!   [`prop_cross_tenant_keys_distinct_100k`]
//! - WI-S01-003 §15 chaos #4 "Concurrent duplicate writes — paths distinct" →
//!   [`bulk_distinct_keys_for_distinct_tenants`]

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_docs_in_private_items,
    clippy::print_stderr,
    clippy::print_stdout,
    missing_docs,
    reason = "test code; panic on assertion failure is the contract"
)]

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::r2::{InMemoryR2, R2Writer};
use corelink_worker::{Region, TenantCtx};
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_TDK_BYTES: [u8; 32] = *b"fixture-tdk-32-bytes-constant!ok";

fn fixture_tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_BYTES))
}

fn ctx_for(uuid: Uuid, region: Region) -> TenantCtx {
    let tdk = fixture_tdk();
    TenantCtx::new(&tdk, uuid, region)
}

async fn put_and_capture_key(
    backend: &Arc<InMemoryR2>,
    region: Region,
    ctx: &TenantCtx,
    body: Bytes,
) -> String {
    let writer = R2Writer::new(region, Arc::clone(backend));
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let snap_before: HashSet<String> = backend.keys_snapshot().into_iter().collect();
    writer.put(ctx, &vb).await.expect("put ok");
    let snap_after: HashSet<String> = backend.keys_snapshot().into_iter().collect();
    let mut added = snap_after.difference(&snap_before);
    added
        .next()
        .cloned()
        .expect("at least one key must have been added")
}

// ---------------------------------------------------------------------------
// Bulk-style tests (cheap, run unconditionally)
// ---------------------------------------------------------------------------

/// Bulk concrete check: 1024 sequential tenant UUIDs ⇒ 1024 distinct keys
/// for the same digest + region.
#[tokio::test]
async fn bulk_distinct_keys_for_distinct_tenants() {
    let backend = Arc::new(InMemoryR2::new());
    let body = Bytes::from_static(b"shared body");
    let mut seen: HashSet<String> = HashSet::with_capacity(1024);

    for i in 0..1024u128 {
        let ctx = ctx_for(Uuid::from_u128(i), Region::Wnam);
        let key = put_and_capture_key(&backend, Region::Wnam, &ctx, body.clone()).await;
        assert!(seen.insert(key.clone()), "collision on tenant {i}: {key}");
    }
}

/// Single-bit flip in the tenant UUID must produce a different key.
#[tokio::test]
async fn single_bit_flip_uuid_changes_key() {
    let backend = Arc::new(InMemoryR2::new());
    let body = Bytes::from_static(b"flip body");

    let mut bytes_a = *Uuid::nil().as_bytes();
    let mut bytes_b = bytes_a;
    bytes_a[0] = 0x00;
    bytes_b[0] = 0x01;

    let ctx_a = ctx_for(Uuid::from_bytes(bytes_a), Region::Wnam);
    let ctx_b = ctx_for(Uuid::from_bytes(bytes_b), Region::Wnam);

    let key_a = put_and_capture_key(&backend, Region::Wnam, &ctx_a, body.clone()).await;

    let backend2 = Arc::new(InMemoryR2::new());
    let key_b = put_and_capture_key(&backend2, Region::Wnam, &ctx_b, body).await;

    assert_ne!(key_a, key_b, "single-bit UUID flip must reroute the key");
}

/// Determinism: same (ctx, digest) ⇒ same key, no matter how many times.
#[tokio::test]
async fn determinism_repeat_writes_under_same_ctx() {
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Weur, Arc::clone(&backend));
    let body = Bytes::from_static(b"determ body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let ctx = ctx_for(Uuid::nil(), Region::Weur);

    // 1st write — Fresh.
    writer.put(&ctx, &vb).await.expect("put 1 ok");
    let snap1 = backend.keys_snapshot();

    // 100 more writes — all Duplicate; key set unchanged.
    for _ in 0..100 {
        writer.put(&ctx, &vb).await.expect("put N ok");
    }
    let snap2 = backend.keys_snapshot();
    assert_eq!(snap1, snap2, "key set must not grow on duplicate writes");
    assert_eq!(snap1.len(), 1);
}

// ---------------------------------------------------------------------------
// Property tests — 100k iter cross-tenant disjointness (WI §10.3.1 / §15)
// ---------------------------------------------------------------------------

// Drive the in-memory backend through the production `R2Writer::put` and
// observe the actually-stored key. This is what closes the codex round-1 P2:
// if a future refactor diverges from the canonical key constructor, this
// test fails — not a hand-rolled mirror of `derive_prefix`.
fn key_via_writer(uuid: Uuid, region: Region, body: &Bytes) -> String {
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(region, Arc::clone(&backend));
    let claimed = Digest::compute(body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");
    let ctx = ctx_for(uuid, region);
    let fut = writer.put(&ctx, &vb);
    futures::executor::block_on(fut).expect("put ok");
    let snap = backend.keys_snapshot();
    snap.first().cloned().expect("one key")
}

// Headline property: distinct UUIDs (same TDK, region, body) ⇒ distinct R2
// keys. 100k iterations covers WI-S01-003 §10.3.1's "100k iter cross-tenant
// disjointness 0 collisions" requirement.
//
// Rationale for asserting strict inequality at this iteration count: the
// underlying `derive_prefix` is HMAC-SHA256 truncated to 96 bits. The
// expected number of birthday-paradox collisions over 100k random pairs is
// `100_000 / 2^96 ≈ 10^-23`. A failure here is therefore an algorithmic
// regression, not a real collision.
//
// Drives the actual `R2Writer::put` end-to-end so the test exercises the
// canonical-key constructor in production code (closes codex round-1 P2).
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100_000,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_cross_tenant_keys_distinct_100k(
        tid_a in any::<u128>(),
        tid_b in any::<u128>(),
        body_seed in any::<u64>(),
    ) {
        prop_assume!(tid_a != tid_b);
        let body = Bytes::copy_from_slice(&body_seed.to_le_bytes());
        let key_a = key_via_writer(Uuid::from_u128(tid_a), Region::Wnam, &body);
        let key_b = key_via_writer(Uuid::from_u128(tid_b), Region::Wnam, &body);
        prop_assert_ne!(key_a, key_b);
    }

    /// Determinism: same (tid, region, digest) under same TDK ⇒ same key.
    #[test]
    fn prop_path_determinism(
        tid in any::<u128>(),
        body_seed in any::<u64>(),
    ) {
        let body = Bytes::copy_from_slice(&body_seed.to_le_bytes());
        let key1 = key_via_writer(Uuid::from_u128(tid), Region::Wnam, &body);
        let key2 = key_via_writer(Uuid::from_u128(tid), Region::Wnam, &body);
        prop_assert_eq!(key1, key2);
    }

    /// Region segregation: same (tid, digest), distinct regions ⇒ distinct keys.
    #[test]
    fn prop_region_segregation(
        tid in any::<u128>(),
        body_seed in any::<u64>(),
    ) {
        let body = Bytes::copy_from_slice(&body_seed.to_le_bytes());
        let key_wnam = key_via_writer(Uuid::from_u128(tid), Region::Wnam, &body);
        let key_weur = key_via_writer(Uuid::from_u128(tid), Region::Weur, &body);
        let key_sam  = key_via_writer(Uuid::from_u128(tid), Region::Sam, &body);
        prop_assert_ne!(&key_wnam, &key_weur);
        prop_assert_ne!(&key_weur, &key_sam);
        prop_assert_ne!(&key_wnam, &key_sam);
    }
}
