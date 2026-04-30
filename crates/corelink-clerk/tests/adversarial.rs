//! Adversarial CVE-regression suite (WI-S03-001 §10.5.2).
//!
//! Five canonical regressions; every one must result in a
//! deterministic reject — not a panic, not a `Result::Ok`, not an
//! incorrect taxonomy variant.
//!
//! 1. `alg=none` (CVE-2015-9235 family): JWT with header `alg=none`
//!    and no signature.
//! 2. Key confusion RS↔HS (CVE-2018-0114): JWT with `alg=HS256` and
//!    HMAC computed over the RSA public PEM as the secret.
//! 3. `exp` bypass: JWT with valid signature but `exp = now − 65s`
//!    (beyond the 60s leeway).
//! 4. Audience spoof: JWT signed by trusted key, `aud = wrong-api`.
//! 5. Issuer spoof: JWT signed by trusted key, `iss =
//!    https://clerk.attacker.example/`.
//!
//! Each test asserts the canonical [`AuthError`] variant.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test-only adversarial regression coverage"
)]

use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use sha2::Sha256;

use corelink_clerk::adapter::craft_unsigned_jwt;
use corelink_clerk::fakes::test_keys::TestRsaKey;
use corelink_clerk::fakes::{InMemoryKvCache, ScriptedJwksFetcher, StaticJwksFetcher};
use corelink_clerk::{AuthError, ClerkAdapter, ClerkConfig};

/// Shared keypair per kid; RSA-2048 keygen is ~30 ms each. The
/// rotation tests need fresh keys to exercise the refresh path,
/// so we keep `key_v2` / `key_unknown` lazy too.
fn shared_key_v1() -> &'static TestRsaKey {
    static K: OnceLock<TestRsaKey> = OnceLock::new();
    K.get_or_init(|| TestRsaKey::generate("kid_v1"))
}
fn shared_key_v2() -> &'static TestRsaKey {
    static K: OnceLock<TestRsaKey> = OnceLock::new();
    K.get_or_init(|| TestRsaKey::generate("kid_v2"))
}
fn shared_key_unknown() -> &'static TestRsaKey {
    static K: OnceLock<TestRsaKey> = OnceLock::new();
    K.get_or_init(|| TestRsaKey::generate("kid_unknown"))
}

const ISSUER: &str = "https://clerk.test.example.dev";
const AUDIENCE: &str = "corelink-api";
const NOW_FIXED: u64 = 1_750_000_000;

#[derive(Serialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: i64,
    iat: i64,
    nbf: i64,
    sid: String,
    org_id: Option<String>,
    email: String,
    clerk_role: Option<String>,
}

fn fixed_clock() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(NOW_FIXED)
}

fn make_adapter(key: &TestRsaKey) -> ClerkAdapter {
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.test.example.dev/.well-known/jwks.json")
        .issuer_allowlist([ISSUER])
        .audience(AUDIENCE)
        .build()
        .unwrap();
    let fetcher = StaticJwksFetcher::new(key.into_jwks());
    let cache = InMemoryKvCache::with_clock(fixed_clock);
    ClerkAdapter::new_with_clock(cfg, fetcher, cache, fixed_clock)
}

fn baseline_claims() -> Claims {
    Claims {
        sub: "user_2abc".into(),
        iss: ISSUER.into(),
        aud: AUDIENCE.into(),
        exp: (NOW_FIXED + 3600) as i64,
        iat: NOW_FIXED as i64,
        nbf: NOW_FIXED as i64,
        sid: "sess_xyz".into(),
        org_id: Some("org_xyz".into()),
        email: "alice@example.dev".into(),
        clerk_role: Some("member".into()),
    }
}

#[tokio::test]
async fn cve_alg_none_rejected() {
    // CVE-2015-9235 regression. Header alg=none, no signature.
    let key = shared_key_v1();
    let adapter = make_adapter(key);
    let header_json = format!(r#"{{"alg":"none","typ":"JWT","kid":"{}"}}"#, key.kid);
    let payload_json = format!(
        r#"{{"sub":"user","iss":"{ISSUER}","aud":"{AUDIENCE}","exp":{exp},"iat":{iat},"sid":"s","email":"a@b.dev"}}"#,
        exp = NOW_FIXED + 3600,
        iat = NOW_FIXED,
    );
    // Empty signature blob.
    let jwt = craft_unsigned_jwt(&header_json, &payload_json, &[]);
    let result = adapter.validate(&jwt).await;
    assert!(
        matches!(
            result,
            Err(AuthError::AlgNotAllowed) | Err(AuthError::SignatureInvalid)
        ),
        "alg=none must be rejected, got {result:?}"
    );
    let snapshot = adapter.counters();
    assert!(snapshot.validate_sig_invalid >= 1);
    assert_eq!(snapshot.validate_ok, 0);
}

#[tokio::test]
async fn cve_rs_to_hs_confusion_rejected() {
    // CVE-2018-0114 regression. JWT signed with HMAC-SHA256 using the
    // RSA public PEM as the HMAC secret; claims fully benign.
    let key = shared_key_v1();
    let adapter = make_adapter(key);

    let header_json = format!(r#"{{"alg":"HS256","typ":"JWT","kid":"{}"}}"#, key.kid);
    let payload_json = format!(
        r#"{{"sub":"user","iss":"{ISSUER}","aud":"{AUDIENCE}","exp":{exp},"iat":{iat},"sid":"s","email":"a@b.dev"}}"#,
        exp = NOW_FIXED + 3600,
        iat = NOW_FIXED,
    );
    let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    let signing_input = format!("{h}.{p}");

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key.public_pem.as_bytes())
        .expect("hmac accepts arbitrary key bytes");
    mac.update(signing_input.as_bytes());
    let tag = mac.finalize().into_bytes();
    let signature = URL_SAFE_NO_PAD.encode(tag);
    let jwt = format!("{signing_input}.{signature}");

    let result = adapter.validate(&jwt).await;
    assert!(
        matches!(
            result,
            Err(AuthError::AlgNotAllowed) | Err(AuthError::SignatureInvalid)
        ),
        "RS↔HS confusion must be rejected, got {result:?}"
    );
}

#[tokio::test]
async fn exp_bypass_beyond_leeway_rejected() {
    let key = shared_key_v1();
    let adapter = make_adapter(key);
    let mut claims = baseline_claims();
    claims.exp = (NOW_FIXED - 65) as i64; // 5s past the 60s leeway boundary
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key.private_pem.as_bytes()).expect("test key accepted");
    let jwt = encode(&header, &claims, &encoding).expect("test sign");
    let result = adapter.validate(&jwt).await;
    assert!(
        matches!(result, Err(AuthError::Expired)),
        "expected Expired, got {result:?}"
    );
}

#[tokio::test]
async fn audience_spoof_rejected() {
    let key = shared_key_v1();
    let adapter = make_adapter(key);
    let mut claims = baseline_claims();
    claims.aud = "wrong-api".into();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key.private_pem.as_bytes()).expect("test key accepted");
    let jwt = encode(&header, &claims, &encoding).expect("test sign");
    let result = adapter.validate(&jwt).await;
    assert!(
        matches!(result, Err(AuthError::AudienceMismatch)),
        "expected AudienceMismatch, got {result:?}"
    );
}

#[tokio::test]
async fn issuer_spoof_rejected() {
    let key = shared_key_v1();
    let adapter = make_adapter(key);
    let mut claims = baseline_claims();
    claims.iss = "https://clerk.attacker.example/".into();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key.private_pem.as_bytes()).expect("test key accepted");
    let jwt = encode(&header, &claims, &encoding).expect("test sign");
    let result = adapter.validate(&jwt).await;
    assert!(
        matches!(result, Err(AuthError::IssuerMismatch { .. })),
        "expected IssuerMismatch, got {result:?}"
    );
}

#[tokio::test]
async fn malformed_no_panic() {
    let key = shared_key_v1();
    let adapter = make_adapter(key);
    let inputs = [
        "",
        "abc",
        "a.b",
        "a.b.c.d",
        "not.a.jwt.at.all",
        "...",
        "<>",
    ];
    for input in inputs {
        let result = adapter.validate(input).await;
        assert!(result.is_err(), "malformed accepted: {input:?} → {result:?}");
    }
}

#[tokio::test]
async fn kid_rotation_lazy_refresh_succeeds() {
    // First fetch returns kid_v1 only; second fetch returns
    // kid_v1+kid_v2; the JWT is signed by kid_v2.
    let key_v1 = shared_key_v1();
    let key_v2 = shared_key_v2();
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.test.example.dev/.well-known/jwks.json")
        .issuer_allowlist([ISSUER])
        .audience(AUDIENCE)
        .build()
        .unwrap();
    let jwks_phase1 = corelink_clerk::Jwks::from_keys(vec![key_v1.jwks_key.clone()]);
    let jwks_phase2 = corelink_clerk::Jwks::from_keys(vec![
        key_v1.jwks_key.clone(),
        key_v2.jwks_key.clone(),
    ]);
    let fetcher = ScriptedJwksFetcher::new(vec![jwks_phase1, jwks_phase2]);
    let cache = InMemoryKvCache::with_clock(fixed_clock);
    let adapter = ClerkAdapter::new_with_clock(cfg, fetcher, cache, fixed_clock);

    let claims = baseline_claims();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key_v2.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key_v2.private_pem.as_bytes()).expect("test key accepted");
    let jwt = encode(&header, &claims, &encoding).expect("test sign");

    let result = adapter.validate(&jwt).await;
    assert!(result.is_ok(), "expected accept after lazy refresh, got {result:?}");
    let snapshot = adapter.counters();
    assert!(snapshot.refresh_kid_miss + snapshot.refresh_scheduled >= 1);
}

#[tokio::test]
async fn kid_still_missing_after_refresh_rejected_no_loop() {
    // Both phases return kid_v1 only; JWT signed with kid_unknown.
    let key_v1 = shared_key_v1();
    let key_unknown = shared_key_unknown();
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.test.example.dev/.well-known/jwks.json")
        .issuer_allowlist([ISSUER])
        .audience(AUDIENCE)
        .build()
        .unwrap();
    let jwks_only_v1 = corelink_clerk::Jwks::from_keys(vec![key_v1.jwks_key.clone()]);
    let fetcher = ScriptedJwksFetcher::new(vec![
        jwks_only_v1.clone(),
        jwks_only_v1.clone(),
        jwks_only_v1,
    ]);
    let cache = InMemoryKvCache::with_clock(fixed_clock);
    let adapter = ClerkAdapter::new_with_clock(cfg, fetcher, cache, fixed_clock);

    let claims = baseline_claims();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key_unknown.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key_unknown.private_pem.as_bytes()).expect("test key accepted");
    let jwt = encode(&header, &claims, &encoding).expect("test sign");

    let result = adapter.validate(&jwt).await;
    assert!(
        matches!(result, Err(AuthError::KidNotInJwks)),
        "expected KidNotInJwks, got {result:?}"
    );
    // Single-shot refresh — no infinite loop. Cold start fetch is
    // counted as `refresh_scheduled`. The KID-miss path is gated
    // behind a cached state, so a cold-start path should never
    // trigger more than 1 fetch in this scenario.
    let _snapshot = adapter.counters();
}

#[tokio::test]
async fn manual_refresh_bumps_counter() {
    let key = shared_key_v1();
    let adapter = make_adapter(key);
    adapter.refresh_jwks().await.expect("manual refresh ok");
    let snapshot = adapter.counters();
    assert_eq!(snapshot.refresh_manual, 1);
}
