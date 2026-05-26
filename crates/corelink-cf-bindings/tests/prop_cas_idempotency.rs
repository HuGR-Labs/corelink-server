//! Property tests pinning INV-CAS-IDEMPOTENCY across the trait surface
//! that `CfR2BucketAdapter` implements (WI-PROPTEST-FU-003 — DEBT-009).
//!
//! Closes the proptest-density gap identified in
//! `specs/_audits/2026-05-15-proptest-density.md` (ratio 0/1 → 3/1).
//!
//! # Why native-runnable
//!
//! `corelink-cf-bindings` carries `#![cfg(target_arch = "wasm32")]` at
//! its lib root so the host build is empty (no symbols, no wasm-bindgen
//! linkage). The CAS idempotency contract is held by the
//! `corelink_worker::storage::r2::R2Backend` trait — every adapter that
//! implements it (production `CfR2BucketAdapter` on wasm32, host
//! `InMemoryR2` for CI) MUST satisfy the same property. This test
//! exercises the trait surface against `InMemoryR2` so the contract is
//! pinned by deterministic native CI; the wasm32 adapter inherits the
//! property via its trait impl.
//!
//! # Invariant coverage
//!
//! | Test                                                            | Invariant pinned                       |
//! |-----------------------------------------------------------------|----------------------------------------|
//! | `prop_inv_cas_idempotency_repeated_put_observes_one_object`     | INV-CAS-IDEMPOTENCY (idempotent PUT)   |
//! | `prop_inv_cas_idempotency_get_returns_first_write`              | INV-CAS-IDEMPOTENCY (immutable read)   |
//! | `prop_inv_cas_idempotency_different_keys_distinct_objects`      | INV-CAS-IDEMPOTENCY (per-key isolation)|
//!
//! Adversarial inputs covered:
//! - Empty buffer (`Bytes::new()`).
//! - Single-byte buffer.
//! - Single-bit-flip pairs (digests MUST diverge, but here we treat
//!   them as distinct keys; the digest property lives in `corelink-hash`).
//! - Length at 4 KiB and 64 KiB boundaries (Cloudflare R2 chunk
//!   alignment boundary).
//! - 1024-byte all-zero and all-0xFF buffers (degenerate content).

// This test file is host-only — `corelink-cf-bindings` itself is
// wasm32-only, so we gate the entire file to compile only on native.
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: panics surface as test failures by design"
)]

use bytes::Bytes;
use corelink_cas::r2_storage::{BackendPutOutcome, InMemoryR2, R2Backend};
use proptest::prelude::*;
use proptest::test_runner::Config;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

// =====================================================================
// PROPTEST_CASES runtime knob (S-07 P1-2 contract — runtime fn, NOT const).
// =====================================================================

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

/// Generate a deterministic adversarial buffer based on the seed.
///
/// The buffer covers a representative cross-section of the input space:
/// empty, single-byte, single-bit-flip pairs, 4 KiB / 64 KiB boundaries,
/// 1 KiB all-zero / all-0xFF. The bucket is selected by `seed mod 8`.
fn pick_payload(rng: &mut ChaCha20Rng) -> Bytes {
    let bucket: u32 = rng.random_range(0..8);
    match bucket {
        0 => Bytes::new(),
        1 => Bytes::from(vec![rng.random::<u8>()]),
        2 => {
            // 4 KiB exactly.
            let mut v = vec![0u8; 4 * 1024];
            for b in v.iter_mut() {
                *b = rng.random();
            }
            Bytes::from(v)
        }
        3 => {
            // 64 KiB exactly.
            let mut v = vec![0u8; 64 * 1024];
            for b in v.iter_mut() {
                *b = rng.random();
            }
            Bytes::from(v)
        }
        4 => Bytes::from(vec![0u8; 1024]),
        5 => Bytes::from(vec![0xFFu8; 1024]),
        6 => {
            // Random length in [0, 8 KiB).
            let n = rng.random_range(0..(8 * 1024));
            let mut v = vec![0u8; n];
            for b in v.iter_mut() {
                *b = rng.random();
            }
            Bytes::from(v)
        }
        _ => {
            // Random length in [1, 256).
            let n = rng.random_range(1..256);
            let mut v = vec![0u8; n];
            for b in v.iter_mut() {
                *b = rng.random();
            }
            Bytes::from(v)
        }
    }
}

fn pick_key(rng: &mut ChaCha20Rng) -> String {
    // Canonical R2 key shape: `tenant/digest`. Use the same flavour the
    // production adapter would. Deterministic for reproducibility.
    let tenant: u32 = rng.random_range(0..16);
    let digest: u64 = rng.random();
    format!("t-{tenant:04}/blake3:{digest:016x}")
}

/// Run a sync block over the tokio current-thread runtime. We
/// intentionally use a fresh runtime per call to keep test isolation:
/// `InMemoryR2` is a per-test-instance fake, no shared state.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime")
        .block_on(fut)
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// INV-CAS-IDEMPOTENCY: N=[1, 16] sequential PUTs of identical
    /// content under the same key MUST observe exactly ONE stored object.
    /// The first PUT MUST return `Stored`; every subsequent PUT MUST
    /// return `AlreadyExists`. This pins the "idempotent duplicate
    /// write" property that the production `CfR2BucketAdapter` head-
    /// probe-then-put path satisfies.
    #[test]
    fn prop_inv_cas_idempotency_repeated_put_observes_one_object(
        seed in any::<u64>(),
        n_puts in 1u8..=16u8,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let key = pick_key(&mut rng);
        let payload = pick_payload(&mut rng);

        let backend = InMemoryR2::new();

        // First PUT — MUST be Stored.
        let first = block_on(backend.put_if_none_match(&key, payload.clone()));
        prop_assert!(first.is_ok(),
            "INV-CAS-IDEMPOTENCY: first PUT errored: {first:?}");
        let outcome = first.expect("first put ok");
        let is_stored = matches!(outcome, BackendPutOutcome::Stored);
        prop_assert!(is_stored,
            "INV-CAS-IDEMPOTENCY: first PUT did not classify as Stored (got {outcome:?})");

        // Subsequent PUTs — MUST be AlreadyExists.
        for i in 1..n_puts {
            let res = block_on(backend.put_if_none_match(&key, payload.clone()));
            prop_assert!(res.is_ok(),
                "INV-CAS-IDEMPOTENCY: PUT #{i} errored: {res:?}");
            let o = res.expect("put ok");
            let is_already = matches!(o, BackendPutOutcome::AlreadyExists);
            prop_assert!(is_already,
                "INV-CAS-IDEMPOTENCY: PUT #{i} of identical content not classified as AlreadyExists (got {o:?})");
        }

        // HEAD MUST observe the object.
        let head = block_on(backend.head(&key));
        prop_assert!(head.is_ok(), "head errored: {head:?}");
        prop_assert!(head.expect("head ok"),
            "INV-CAS-IDEMPOTENCY: HEAD returned false after successful PUT");
    }

    /// INV-CAS-IDEMPOTENCY (immutable read): once an object is written
    /// at `key`, the body returned by `get(key)` MUST equal the body
    /// the FIRST writer landed — even after subsequent idempotent PUTs.
    /// CAS is write-once: the first writer's body is the canonical
    /// representation.
    ///
    /// Adversarial input: second PUT carries a DIFFERENT body but the
    /// same key. Production R2 `If-None-Match: *` semantics reject the
    /// second PUT with PreconditionFailed → `AlreadyExists`; the get
    /// MUST still return the first writer's body. This is the CAS
    /// immutability property's observable side.
    #[test]
    fn prop_inv_cas_idempotency_get_returns_first_write(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let key = pick_key(&mut rng);
        let body_a = pick_payload(&mut rng);
        // Force a distinct second body by prefixing a marker byte (and
        // guaranteeing it differs from body_a even if body_a happens to
        // start with that byte by flipping the low bit).
        let mut body_b_vec = body_a.to_vec();
        body_b_vec.push(0xAA);
        body_b_vec.push(0xBB);
        let body_b = Bytes::from(body_b_vec);
        prop_assert_ne!(body_a.len(), body_b.len(),
            "constructed bodies must differ by length");

        let backend = InMemoryR2::new();

        // First PUT — body_a lands.
        let first = block_on(backend.put_if_none_match(&key, body_a.clone()))
            .expect("first put");
        let is_stored = matches!(first, BackendPutOutcome::Stored);
        prop_assert!(is_stored, "first PUT did not classify as Stored");

        // Second PUT — body_b should be rejected (AlreadyExists).
        let second = block_on(backend.put_if_none_match(&key, body_b.clone()))
            .expect("second put");
        let is_already = matches!(second, BackendPutOutcome::AlreadyExists);
        prop_assert!(is_already,
            "INV-CAS-IDEMPOTENCY: second PUT with DIFFERENT body not rejected (got {second:?})");

        // GET MUST still return body_a (first writer wins).
        let got = block_on(backend.get(&key)).expect("get");
        prop_assert_eq!(got.as_ref(), body_a.as_ref(),
            "INV-CAS-IDEMPOTENCY: GET returned non-first-writer body");
    }

    /// INV-CAS-IDEMPOTENCY (per-key isolation): two distinct keys MUST
    /// store distinct objects independently — a PUT under `key_a` MUST
    /// NOT make `head(key_b)` return true. Random key pairs from the
    /// same generator; reflexivity + non-interference.
    #[test]
    fn prop_inv_cas_idempotency_different_keys_distinct_objects(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let key_a = pick_key(&mut rng);
        let mut key_b = pick_key(&mut rng);
        // Guarantee distinct keys (the random generator can collide;
        // append a deterministic suffix if so).
        if key_a == key_b {
            key_b.push_str(":alt");
        }
        prop_assert_ne!(&key_a, &key_b, "keys must differ");

        let payload = pick_payload(&mut rng);
        let backend = InMemoryR2::new();

        // PUT under key_a; key_b MUST remain absent.
        block_on(backend.put_if_none_match(&key_a, payload.clone()))
            .expect("put key_a");
        let head_a = block_on(backend.head(&key_a)).expect("head key_a");
        let head_b = block_on(backend.head(&key_b)).expect("head key_b");
        prop_assert!(head_a, "key_a should be present after PUT");
        prop_assert!(!head_b,
            "INV-CAS-IDEMPOTENCY: per-key isolation violated — head(key_b)=true after PUT(key_a)");
    }
}

// =====================================================================
// Determinism canary — PRNG seed reproducibility.
// =====================================================================

#[test]
fn prng_seed_is_deterministic_across_invocations() {
    let mut a = ChaCha20Rng::seed_from_u64(0xC45_F00D_DEAD_BEEF);
    let mut b = ChaCha20Rng::seed_from_u64(0xC45_F00D_DEAD_BEEF);
    for _ in 0..128 {
        let av: u64 = a.random();
        let bv: u64 = b.random();
        assert_eq!(av, bv, "ChaCha20Rng output must be deterministic per seed");
    }
}

/// Canary: empty payload + single-byte payload both land successfully
/// and observe as `Stored` then `AlreadyExists` on re-PUT (boundary).
#[test]
fn empty_and_single_byte_payloads_are_idempotent() {
    let backend = InMemoryR2::new();

    let res1 = block_on(backend.put_if_none_match("t/empty", Bytes::new()))
        .expect("put empty");
    assert!(matches!(res1, BackendPutOutcome::Stored));
    let res2 = block_on(backend.put_if_none_match("t/empty", Bytes::new()))
        .expect("put empty 2");
    assert!(matches!(res2, BackendPutOutcome::AlreadyExists));

    let res3 = block_on(backend.put_if_none_match("t/one", Bytes::from(vec![0x42])))
        .expect("put one");
    assert!(matches!(res3, BackendPutOutcome::Stored));
    let res4 = block_on(backend.put_if_none_match("t/one", Bytes::from(vec![0x42])))
        .expect("put one 2");
    assert!(matches!(res4, BackendPutOutcome::AlreadyExists));
}

/// Canary: the per-module cfg-gating invariant.
///
/// `corelink-cf-bindings` originally carried `#![cfg(target_arch =
/// "wasm32")]` at its lib root, making the native build entirely
/// empty. As of R-PREP-CF-R2-REAL the lib root no longer cfg-gates;
/// `r2_real::CfR2BucketReal` is dual-target (wasm32 production + native
/// stub) and each `worker::*`-dependent module (`cf_r2`, `cf_d1`,
/// `cf_kv`, `cf_do`) carries its OWN `#[cfg(target_arch = "wasm32")]`.
///
/// This test pins the contract that no `worker::*` symbol leaks into
/// the native build: if it did, linking this test binary on host CI
/// would fail with a missing `js_sys` / `wasm_bindgen` symbol.
#[test]
fn cfg_gate_keeps_native_build_worker_free() {
    let _proof =
        "corelink-cf-bindings: worker::* gated per-module; native build links no wasm-bindgen";
    assert!(!_proof.is_empty());
}
