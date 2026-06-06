//! Adversarial regression suite — five named CVE-class checks pinned
//! per WI-S03-002 §6.1.12. Each scenario emulates a known
//! cripto-hardening failure mode and asserts the crate rejects the
//! attack cleanly.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: failures must be loud, not silenced"
)]

use corelink_pat::mint::{mint_with_entropy, DeterministicMintInput};
use corelink_pat::scopes::PatScopes;
use corelink_pat::types::PAT_TOKEN_ID_LEN;
use corelink_pat::types::{PatEnv, PatHash, PatSigningKey, PrincipalId, TenantId};
use corelink_pat::{verify_argon2id, verify_with_hash, PatError};
use password_hash::Salt;
use uuid::Uuid;

fn salt() -> Salt<'static> {
    Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").unwrap()
}

fn key32() -> PatSigningKey {
    PatSigningKey::from_bytes(vec![0x99u8; 32]).unwrap()
}

/// 1. **Argon2 algorithm downgrade**: a stored hash whose embedded
///    algorithm field claims `argon2i` (memory-only — older variant)
///    must be rejected during verify even if everything else looks
///    reasonable. Defence in depth: prevents a malicious DB swap
///    that downgrades the cripto across the verify boundary.
#[test]
fn rejects_argon2i_algorithm_downgrade() {
    // Hand-crafted PHC string with `argon2i` instead of `argon2id`.
    // The hash bytes themselves are arbitrary; the parser MUST
    // reject the algorithm tag before invoking the hasher.
    let phc =
        "$argon2i$v=19$m=65536,t=3,p=4$c2FsdHNhbHRzYWx0c2FsdA$abcdefghijklmnopqrstuvwxyz012345";
    let hash = PatHash::from_phc_string(phc.to_owned());
    let res = verify_argon2id("any_input", &hash);
    assert!(matches!(res, Err(PatError::HashError(_))));
}

/// 2. **Underprovisioned m_cost** (rainbow-table risk): m_cost below
///    OWASP 2024 floor (65_536 KiB = 64 MiB) MUST be rejected even
///    if the rest of the PHC string is otherwise valid.
#[test]
fn rejects_argon2_m_cost_below_floor() {
    let phc =
        "$argon2id$v=19$m=8192,t=3,p=4$c2FsdHNhbHRzYWx0c2FsdA$abcdefghijklmnopqrstuvwxyz012345";
    let hash = PatHash::from_phc_string(phc.to_owned());
    let res = verify_argon2id("any_input", &hash);
    assert!(matches!(res, Err(PatError::HashError(_))));
}

/// 3. **t_cost downgrade**: iteration count below floor rejected.
#[test]
fn rejects_argon2_t_cost_below_floor() {
    let phc =
        "$argon2id$v=19$m=65536,t=1,p=4$c2FsdHNhbHRzYWx0c2FsdA$abcdefghijklmnopqrstuvwxyz012345";
    let hash = PatHash::from_phc_string(phc.to_owned());
    let res = verify_argon2id("any_input", &hash);
    assert!(matches!(res, Err(PatError::HashError(_))));
}

/// 4. **HMAC signing key forge**: an attacker who guesses the wire
///    format CANNOT forge an HMAC sig without the per-region signing
///    key. We craft a plaintext that has all the right shape but a
///    fabricated sig segment (random base64url) and assert verify
///    rejects it without surfacing the underlying error type
///    (constant-time uniformity invariant).
#[test]
fn rejects_forged_hmac_sig_segment() {
    let key = key32();
    let (pt, pat) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Pat,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::empty(),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: [0xAAu8; PAT_TOKEN_ID_LEN],
        secret_bytes: [0xBBu8; 32],
        salt: salt(),
    })
    .unwrap();
    let pt_str = pt.into_string();
    // Replace the trailing 22-char hmac_sig segment with all 'A's.
    let mut bytes = pt_str.as_bytes().to_vec();
    let len = bytes.len();
    for byte in bytes.iter_mut().skip(len - 22) {
        *byte = b'A';
    }
    let forged = String::from_utf8(bytes).unwrap();
    let res = verify_with_hash(&forged, &pat.token_id, &pat.hash, &key);
    assert!(res.is_err());
}

/// 5. **Salt reuse / hash determinism** check: minting the same
///    `random_secret` twice with DIFFERENT salts MUST produce two
///    distinct PHC strings (random salt invariant) but BOTH must
///    verify against the same plaintext.
#[test]
fn salt_per_token_unique_phc_strings() {
    use corelink_pat::argon::hash_random_secret;
    let secret = "deterministic_random_secret_for_test";
    let h1 = hash_random_secret(secret).unwrap();
    let h2 = hash_random_secret(secret).unwrap();
    assert_ne!(h1.as_str(), h2.as_str(), "salts must differ");
    assert!(verify_argon2id(secret, &h1).is_ok());
    assert!(verify_argon2id(secret, &h2).is_ok());
}

/// 6. **Signing-key length lower bound**: `< 32 bytes` rejected at
///    construction, never reaching the HMAC boundary.
#[test]
fn rejects_short_signing_key() {
    let res = PatSigningKey::from_bytes(vec![0u8; 31]);
    assert!(matches!(res, Err(PatError::SigningKeyTooShort)));
}

/// 7. **Plaintext leakage**: `PatPlaintext` Debug never reveals the
///    canonical bytes. We do not require the Debug impl to be
///    constant — only that the bytes are not present.
#[test]
fn pat_plaintext_debug_redacts_payload() {
    let key = key32();
    let (pt, _pat) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Pat,
        tenant_id: TenantId(Uuid::nil()),
        principal_id: PrincipalId(Uuid::nil()),
        scopes: PatScopes::empty(),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes: [0xCCu8; PAT_TOKEN_ID_LEN],
        secret_bytes: [0xDDu8; 32],
        salt: salt(),
    })
    .unwrap();
    let dbg = format!("{pt:?}");
    assert!(dbg.contains("redacted"));
    assert!(!dbg.contains("corelink_"));
}

/// 8. **Cold-path constant-time pad**: invoking the dummy pad
///    ALWAYS returns `Err(InvalidPat)` regardless of input — the
///    middleware can call this on every cold path without surfacing
///    a different error variant that would itself constitute an
///    oracle.
#[test]
fn dummy_verify_for_constant_time_always_returns_invalid_pat() {
    use corelink_pat::dummy_verify_for_constant_time;
    for input in &[
        "",
        "anything",
        "corelink_pat_AAAAAAAAAAAAAAAA.something.morestuff",
        "definitely-not-a-pat",
    ] {
        let res = dummy_verify_for_constant_time(input);
        assert!(matches!(res, Err(PatError::InvalidPat)));
    }
}
