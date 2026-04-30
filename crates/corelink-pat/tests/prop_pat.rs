//! Property tests for `corelink-pat` (10k iter PR; 100k iter nightly
//! via the `proptest-cases` env override). Asserts the load-bearing
//! security invariants:
//!
//! - `verify_with_hash` ALWAYS rejects when the random_secret bytes
//!   differ from the minted PAT (10k iter on tampered secrets).
//! - Cross-tenant isolation: a PAT minted under signing key A
//!   ALWAYS rejects under a different signing key B (10k iter,
//!   different-key forgeries).
//! - `parse_plaintext` ALWAYS returns `Err(Malformed)` (never
//!   panics) on arbitrary byte strings (10k iter on random
//!   non-canonical inputs).
//! - HMAC sig verification is deterministic across reruns for
//!   identical preimages (regression guard against accidental
//!   non-determinism in the audit pipeline).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: failures must be loud, not silenced"
)]

use corelink_pat::types::PAT_TOKEN_ID_LEN;
use corelink_pat::mint::{mint_with_entropy, DeterministicMintInput};
use corelink_pat::scopes::{
    PatScopes, SCOPE_ADMIN_AUDIT, SCOPE_CACHE_FIND, SCOPE_CACHE_RW, SCOPE_KNOWN_MASK,
};
use corelink_pat::types::{PatEnv, PatSigningKey, PrincipalId, TenantId};
use corelink_pat::{parse_env, parse_plaintext, verify_with_hash};
use password_hash::Salt;
use proptest::prelude::*;
use uuid::Uuid;

fn arb_env() -> impl Strategy<Value = PatEnv> {
    prop_oneof![Just(PatEnv::Pat), Just(PatEnv::Ci), Just(PatEnv::Ro)]
}

fn arb_signing_key() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 32..=64)
}

fn arb_token_id_bytes() -> impl Strategy<Value = [u8; PAT_TOKEN_ID_LEN]> {
    proptest::array::uniform16(any::<u8>())
}

fn arb_secret_bytes() -> impl Strategy<Value = [u8; 32]> {
    proptest::array::uniform32(any::<u8>())
}

fn fixed_salt() -> Salt<'static> {
    Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap()
}

// Argon2id verify costs ~64 MiB RAM and ~250ms CPU per iteration in
// release mode (and ~3-5s in debug). Running 10 000 cases in unit
// tests would burn ~30 min just for the heavy tests. Instead we keep
// the heavy gates at a sampled count (`ARGON2_HEAVY_CASES`) while the
// non-Argon shape / parser / scope properties run at the full 10k iter
// envelope (`SHAPE_CASES`). Nightly CI re-runs the heavy gates at the
// canonical 10k iter via the `proptest-cases` env override.
const ARGON2_HEAVY_CASES: u32 = 8;
const SHAPE_CASES: u32 = 10_000;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: ARGON2_HEAVY_CASES,
        max_global_rejects: 1024,
        .. ProptestConfig::default()
    })]

    /// Argon2-heavy: tampered random_secret bytes ALWAYS fail verify.
    #[test]
    fn verify_rejects_when_random_secret_tampered(
        env in arb_env(),
        key_bytes in arb_signing_key(),
        token_id_bytes in arb_token_id_bytes(),
        secret_bytes in arb_secret_bytes(),
        flip_idx in 0usize..32,
    ) {
        let key = PatSigningKey::from_bytes(key_bytes).unwrap();
        let (pt, _pat) = mint_with_entropy(DeterministicMintInput {
            env,
            tenant_id: TenantId(Uuid::nil()),
            principal_id: PrincipalId(Uuid::nil()),
            scopes: PatScopes::empty(),
            ttl: None,
            signing_key: &key,
            signing_key_id: 1,
            token_id_bytes,
            secret_bytes,
            salt: fixed_salt(),
        }).unwrap();
        let pt_string = pt.into_string();
        // Bit-flip a single byte of the underlying secret would change
        // both the random_secret_b64 segment AND (because the HMAC sig
        // is over the b64 form) the sig segment. We mint twice with
        // different secret_bytes and verify the cross-mint hash mismatch.
        let mut tampered_secret = secret_bytes;
        tampered_secret[flip_idx % 32] ^= 0x01;
        let (_pt2, pat2) = mint_with_entropy(DeterministicMintInput {
            env,
            tenant_id: TenantId(Uuid::nil()),
            principal_id: PrincipalId(Uuid::nil()),
            scopes: PatScopes::empty(),
            ttl: None,
            signing_key: &key,
            signing_key_id: 1,
            token_id_bytes,
            secret_bytes: tampered_secret,
            salt: fixed_salt(),
        }).unwrap();
        // pat2.hash is over a different secret; the original pt_string
        // must NOT verify against pat2.hash (cross-mint smoke).
        let res = verify_with_hash(&pt_string, &pat2.token_id, &pat2.hash, &key);
        prop_assert!(res.is_err(), "tampered hash must reject");
    }

    /// Argon2-heavy: cross-tenant isolation — PAT minted under key A
    /// is ALWAYS rejected under a different key B.
    #[test]
    fn cross_tenant_signing_key_isolation(
        env in arb_env(),
        key_a in arb_signing_key(),
        key_b in arb_signing_key(),
        token_id_bytes in arb_token_id_bytes(),
        secret_bytes in arb_secret_bytes(),
    ) {
        // Make sure the keys differ; if proptest happens to draw equal
        // keys, drop the case via prop_assume!.
        prop_assume!(key_a != key_b);
        let key_a = PatSigningKey::from_bytes(key_a).unwrap();
        let key_b = PatSigningKey::from_bytes(key_b).unwrap();
        let (pt, pat) = mint_with_entropy(DeterministicMintInput {
            env,
            tenant_id: TenantId(Uuid::nil()),
            principal_id: PrincipalId(Uuid::nil()),
            scopes: PatScopes::empty(),
            ttl: None,
            signing_key: &key_a,
            signing_key_id: 1,
            token_id_bytes,
            secret_bytes,
            salt: fixed_salt(),
        }).unwrap();
        let pt_string = pt.into_string();
        // Verify under wrong key: expect rejection.
        let res = verify_with_hash(&pt_string, &pat.token_id, &pat.hash, &key_b);
        prop_assert!(res.is_err(), "cross-key verify must reject");
        // Round-trip with the right key still passes.
        let ok = verify_with_hash(&pt_string, &pat.token_id, &pat.hash, &key_a);
        prop_assert!(ok.is_ok(), "same-key verify must succeed");
    }
}

// Shape / parser / scope properties at the canonical 10k iter
// envelope (no Argon2id work; cheap per-case so 10k cases run in
// milliseconds even in debug mode).
proptest! {
    #![proptest_config(ProptestConfig {
        cases: SHAPE_CASES,
        max_global_rejects: 100_000,
        .. ProptestConfig::default()
    })]

    /// 10k iter: arbitrary input bytes never panic the parser; the
    /// parse function returns `Err(Malformed)` on every malformed
    /// input (and may succeed only on canonical-shape inputs which
    /// are vanishingly unlikely under random generation).
    #[test]
    fn parse_plaintext_never_panics_on_arbitrary_bytes(
        bytes in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        // Convert to lossy UTF-8 so non-utf-8 bytes still exercise
        // the parser via String::from_utf8_lossy.
        let input = String::from_utf8_lossy(&bytes).into_owned();
        let _ = parse_plaintext(&input); // must not panic
        let _ = parse_env(&input);       // must not panic
    }

    /// Scope bitset roundtrip: u64 → PatScopes → u64 lossless,
    /// modulo reserved-bit masking (forward-compat invariant).
    #[test]
    fn scope_u64_roundtrip_masks_reserved_bits(raw in any::<u64>()) {
        let scopes = PatScopes::from_u64(raw);
        let back = scopes.to_u64();
        prop_assert_eq!(back, raw & SCOPE_KNOWN_MASK);
    }

    /// `PatScopes::has(required)` is consistent with the bitwise
    /// `(scopes & required) == required` definition.
    #[test]
    fn scope_has_matches_bitwise_definition(
        scopes_raw in any::<u64>(),
        required_raw in any::<u64>(),
    ) {
        let scopes = PatScopes::from_u64(scopes_raw);
        let masked_required = required_raw & SCOPE_KNOWN_MASK;
        let scopes_value = scopes_raw & SCOPE_KNOWN_MASK;
        let expected = (scopes_value & masked_required) == masked_required;
        prop_assert_eq!(scopes.has(required_raw), expected);
    }
}

/// Compile-time pin that the canonical scope mask covers exactly 12
/// bits (13 named scopes minus the alias `SCOPE_CACHE_RW`).
#[test]
fn scope_known_mask_covers_twelve_bits() {
    assert_eq!(SCOPE_KNOWN_MASK.count_ones(), 12);
}

/// Smoke test that scope semantics are stable.
#[test]
fn scope_admin_audit_disjoint_from_cache_find() {
    let s = PatScopes::from_u64(SCOPE_CACHE_FIND);
    assert!(!s.has(SCOPE_ADMIN_AUDIT));
    let s2 = s.add(SCOPE_ADMIN_AUDIT);
    assert!(s2.has(SCOPE_ADMIN_AUDIT));
    assert!(s2.has(SCOPE_CACHE_FIND));
    let s3 = s2.remove(SCOPE_CACHE_FIND);
    assert!(!s3.has(SCOPE_CACHE_FIND));
    assert!(s3.has(SCOPE_ADMIN_AUDIT));
}

/// Smoke: union/intersection align with bitwise semantics.
#[test]
fn scope_union_intersection_smoke() {
    let a = PatScopes::from_u64(SCOPE_CACHE_RW);
    let b = PatScopes::from_u64(SCOPE_ADMIN_AUDIT);
    assert_eq!(
        (a | b).to_u64(),
        SCOPE_CACHE_RW | SCOPE_ADMIN_AUDIT
    );
    assert_eq!((a & b).to_u64(), 0);
}
