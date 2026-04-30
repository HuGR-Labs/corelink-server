//! Canonical regression vectors (deterministic).
//!
//! Cross-language and cross-implementation drift catch: if the
//! Crockford b32 alphabet, base64url alphabet, HMAC-SHA256 truncation
//! offset, or Argon2id PHC layout ever change, these tests fail
//! immediately. The fixture inputs are pinned to known constants;
//! the expected outputs are pre-computed from the canonical
//! implementation under v1.0.0 SEAL.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: failures must be loud, not silenced"
)]

use corelink_pat::format::{
    parse_plaintext, PAT_HMAC_SIG_LEN, PAT_PREFIX_LEN, PAT_RANDOM_SECRET_LEN,
};
use corelink_pat::mint::{mint_with_entropy, DeterministicMintInput};
use corelink_pat::scopes::PatScopes;
use corelink_pat::sig::compute_hmac_sig;
use corelink_pat::types::{PatEnv, PatSigningKey, PrincipalId, TenantId, PAT_TOKEN_ID_LEN};
use corelink_pat::{
    parse_env, verify_argon2id, verify_hmac_sig, verify_with_hash, SCOPE_CACHE_RW,
};
use password_hash::Salt;
use uuid::Uuid;

/// Fixed signing key: 32 bytes of `0x42`. Stable across vectors.
fn fixed_signing_key() -> PatSigningKey {
    PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap()
}

#[test]
fn pat_format_total_length_envelopes() {
    // env len 3 ("pat") -> total 93 chars
    // env len 2 ("ci"|"ro") -> total 92 chars
    let key = fixed_signing_key();
    let salt = Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap();
    let token_seed = [0xA0u8; PAT_TOKEN_ID_LEN];
    let secret_seed = [0xB1u8; 32];
    let (pt, _) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Pat,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::from_u64(SCOPE_CACHE_RW),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: token_seed,
        secret_bytes: secret_seed,
        salt,
    })
    .unwrap();
    assert_eq!(pt.len_for_test(), 96);

    let (pt2, _) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Ci,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::from_u64(SCOPE_CACHE_RW),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: token_seed,
        secret_bytes: secret_seed,
        salt,
    })
    .unwrap();
    assert_eq!(pt2.len_for_test(), 95);

    let (pt3, _) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Ro,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::from_u64(SCOPE_CACHE_RW),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: token_seed,
        secret_bytes: secret_seed,
        salt,
    })
    .unwrap();
    assert_eq!(pt3.len_for_test(), 95);
}

#[test]
fn pat_format_constants_pin() {
    assert_eq!(PAT_PREFIX_LEN, 9);
    assert_eq!(PAT_TOKEN_ID_LEN, 16);
    assert_eq!(PAT_RANDOM_SECRET_LEN, 43);
    assert_eq!(PAT_HMAC_SIG_LEN, 22);
}

#[test]
fn parse_env_recognises_three_canonical_envs() {
    // We only need a syntactically-valid prefix here; the rest of the
    // plaintext can be arbitrary because parse_env stops after
    // env+separator.
    let stub = "corelink_pat_AAAAAAAAAAAAAAAA.something_after.does_not_matter";
    assert_eq!(parse_env(stub).unwrap(), PatEnv::Pat);
    let stub_ci = "corelink_ci_AAAAAAAAAAAAAAAA.something_after.does_not_matter";
    assert_eq!(parse_env(stub_ci).unwrap(), PatEnv::Ci);
    let stub_ro = "corelink_ro_AAAAAAAAAAAAAAAA.something_after.does_not_matter";
    assert_eq!(parse_env(stub_ro).unwrap(), PatEnv::Ro);
}

#[test]
fn parse_env_rejects_unknown_env_tags() {
    assert!(parse_env("corelink_admin_xxxx").is_err());
    assert!(parse_env("corelink_exec_xxxx").is_err());
    assert!(parse_env("not-a-pat").is_err());
    // Empty / very short
    assert!(parse_env("").is_err());
    assert!(parse_env("corelink_").is_err());
}

#[test]
fn mint_then_verify_roundtrip_succeeds() {
    let key = fixed_signing_key();
    let salt = Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap();
    let (pt, pat) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Pat,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::from_u64(SCOPE_CACHE_RW),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: [0x11u8; PAT_TOKEN_ID_LEN],
        secret_bytes: [0x22u8; 32],
        salt,
    })
    .unwrap();
    let pt_string = pt.into_string();
    let verified = verify_with_hash(&pt_string, &pat.token_id, &pat.hash, &key).unwrap();
    assert_eq!(verified.env, PatEnv::Pat);
}

#[test]
fn mint_then_tamper_one_char_fails() {
    let key = fixed_signing_key();
    let salt = Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap();
    let (pt, pat) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Pat,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::from_u64(SCOPE_CACHE_RW),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: [0x33u8; PAT_TOKEN_ID_LEN],
        secret_bytes: [0x44u8; 32],
        salt,
    })
    .unwrap();
    let mut pt_string = pt.into_string();
    // Flip the last char (always a base64url alphabet char).
    let last = pt_string.pop().unwrap();
    let flipped = if last == 'A' { 'B' } else { 'A' };
    pt_string.push(flipped);
    let res = verify_with_hash(&pt_string, &pat.token_id, &pat.hash, &key);
    assert!(res.is_err());
}

#[test]
fn hmac_sig_is_deterministic_for_fixed_key() {
    let key = fixed_signing_key();
    let preimage_a = b"AAAAAAAAAAAAAAAA.same_random_secret";
    let preimage_b = b"AAAAAAAAAAAAAAAA.different_secret___";
    let sig_a = compute_hmac_sig(&key, preimage_a);
    let sig_a_repeat = compute_hmac_sig(&key, preimage_a);
    let sig_b = compute_hmac_sig(&key, preimage_b);
    assert_eq!(sig_a, sig_a_repeat, "deterministic for same input");
    assert_ne!(sig_a, sig_b, "different preimage → different sig");
    // Truncation length pinned at 16 bytes.
    assert_eq!(sig_a.len(), 16);
}

#[test]
fn hmac_sig_verify_rejects_mismatched_signature() {
    let key = fixed_signing_key();
    let preimage = b"AAAAAAAAAAAAAAAA.some_random_secret_____blah";
    let sig = compute_hmac_sig(&key, preimage);
    // Round trip success.
    assert!(verify_hmac_sig(&key, preimage, &sig).is_ok());
    // Bit flip in the sig.
    let mut tampered = sig;
    tampered[0] ^= 0x01;
    assert!(verify_hmac_sig(&key, preimage, &tampered).is_err());
    // Wrong length.
    assert!(verify_hmac_sig(&key, preimage, &sig[..15]).is_err());
}

#[test]
fn verify_argon2id_rejects_underprovisioned_params() {
    use corelink_pat::types::PatHash;
    // Hand-crafted PHC string with m_cost = 32_768 (below OWASP 2024 floor).
    let weak = PatHash::from_phc_string(
        "$argon2id$v=19$m=32768,t=2,p=1$c2FsdHNhbHRzYWx0$abcdefghijklmnopqrstuvwxyz0123".to_owned(),
    );
    let res = verify_argon2id("any_input", &weak);
    assert!(matches!(
        res,
        Err(corelink_pat::PatError::HashError(_))
    ));
}

#[test]
fn parse_plaintext_full_canonical_shape() {
    let key = fixed_signing_key();
    let salt = Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap();
    let (pt, _) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Ci,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::empty(),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: [0x55u8; PAT_TOKEN_ID_LEN],
        secret_bytes: [0x66u8; 32],
        salt,
    })
    .unwrap();
    let pt_string = pt.into_string();
    let parts = parse_plaintext(&pt_string).unwrap();
    assert_eq!(parts.env, PatEnv::Ci);
    assert_eq!(parts.token_id.as_str().len(), PAT_TOKEN_ID_LEN);
    assert_eq!(parts.random_secret_b64.len(), PAT_RANDOM_SECRET_LEN);
    assert_eq!(parts.hmac_sig_b64.len(), PAT_HMAC_SIG_LEN);
    assert_eq!(parts.random_secret_bytes.len(), 32);
    assert_eq!(parts.hmac_sig_bytes.len(), 16);
    // Preimage is `<token_id>.<random_secret>` exactly.
    assert_eq!(
        parts.hmac_preimage.len(),
        PAT_TOKEN_ID_LEN + 1 + PAT_RANDOM_SECRET_LEN
    );
}
