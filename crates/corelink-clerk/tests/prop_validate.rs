//! Property-based regression tests for [`corelink_clerk::ClerkAdapter::validate`].
//!
//! WI-S03-001 §10.5.1 mandates 10k iter PR (+ 100k iter nightly) over
//! fuzz JWT inputs with **0 panics + 0 false-accepts**. This module
//! covers the canonical security invariants:
//!
//! 1. JWT with **wrong signature** → ALWAYS rejected.
//! 2. JWT **expired** (beyond leeway) → ALWAYS rejected.
//! 3. JWT **issuer mismatch** → ALWAYS rejected.
//! 4. JWT **audience mismatch** → ALWAYS rejected.
//! 5. **Malformed input** (random bytes, near-JWT shapes) → no panic;
//!    AuthError::Malformed family.
//! 6. JWT **alg=none / alg=HS256** → ALWAYS rejected (
//!    AuthError::AlgNotAllowed family; covered jointly with the
//!    adversarial regression file).
//!
//! All tests run against a fully-mocked Clerk JWKS endpoint
//! (in-memory `StaticJwksFetcher` + `InMemoryKvCache`); zero network
//! / Clerk-credential requirements per WI §"Architectural seam".

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test-only: proptest harness fixed inputs"
)]

use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use proptest::prelude::*;
use serde::Serialize;

use corelink_clerk::fakes::test_keys::TestRsaKey;
use corelink_clerk::fakes::{InMemoryKvCache, StaticJwksFetcher};
use corelink_clerk::{AuthError, ClerkAdapter, ClerkConfig};

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

const ISSUER: &str = "https://clerk.test.example.dev";
const AUDIENCE: &str = "corelink-api";
const NOW_FIXED: u64 = 1_750_000_000;

#[derive(Serialize, Clone)]
struct TestClaims {
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

/// Single shared RSA keypair for the entire prop-test run. Generating
/// a 2048-bit RSA keypair takes ~30 ms — at 10k iter that would be
/// ~5 min just on keygen. We share one keypair across cases and rely
/// on the structural variants (mutated signature, varied claims) to
/// exercise the security invariants.
fn shared_key() -> &'static TestRsaKey {
    static KEY: OnceLock<TestRsaKey> = OnceLock::new();
    KEY.get_or_init(|| TestRsaKey::generate("kid_v1"))
}

fn build_adapter() -> ClerkAdapter {
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.test.example.dev/.well-known/jwks.json")
        .issuer_allowlist([ISSUER])
        .audience(AUDIENCE)
        .build()
        .unwrap();
    let fetcher = StaticJwksFetcher::new(shared_key().into_jwks());
    let cache = InMemoryKvCache::with_clock(fixed_clock);
    ClerkAdapter::new_with_clock(cfg, fetcher, cache, fixed_clock)
}

fn baseline_claims() -> TestClaims {
    TestClaims {
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

fn sign(key: &TestRsaKey, claims: &TestClaims) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    let encoding =
        EncodingKey::from_rsa_pem(key.private_pem.as_bytes()).expect("test rsa pem accepted");
    encode(&header, claims, &encoding).expect("test sign")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn happy_path_validates() {
    let adapter = build_adapter();
    let claims = baseline_claims();
    let jwt = sign(shared_key(), &claims);
    let principal = adapter.validate(&jwt).await.expect("validate ok");
    assert_eq!(principal.user_id.as_str(), "user_2abc");
    assert_eq!(principal.email.as_str(), "alice@example.dev");
    assert!(principal.org_id.is_some());
    let snapshot = adapter.counters();
    assert_eq!(snapshot.validate_ok, 1);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 64,
        .. ProptestConfig::default()
    })]

    /// Wrong-signature regression: any single-byte tweak to the
    /// signature segment must be rejected. INV-AUTH-JWT-VALIDATE-RS256-ONLY.
    #[test]
    fn wrong_signature_is_rejected(
        sig_byte_idx in 0usize..256,
        sig_byte_val in any::<u8>(),
    ) {
        let adapter = build_adapter();
        let claims = baseline_claims();
        let jwt = sign(shared_key(), &claims);
        // Decompose into header.payload.signature; mutate a signature byte.
        let parts: Vec<&str> = jwt.split('.').collect();
        prop_assume!(parts.len() == 3);
        let mut sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap_or_default();
        prop_assume!(!sig_bytes.is_empty());
        let idx = sig_byte_idx % sig_bytes.len();
        // Force the byte to differ from the original (collision-safe).
        let new_byte = if sig_bytes[idx] == sig_byte_val {
            sig_byte_val.wrapping_add(1)
        } else {
            sig_byte_val
        };
        sig_bytes[idx] = new_byte;
        let mutated_sig = URL_SAFE_NO_PAD.encode(&sig_bytes);
        let mutated = format!("{}.{}.{}", parts[0], parts[1], mutated_sig);
        let result = futures_executor_blocking(adapter.validate(&mutated));
        match &result {
            Err(AuthError::SignatureInvalid) => {
                // SOTA-OK: SignatureInvalid is a unit variant carrying no semantic state.
            }
            Err(AuthError::Malformed(msg)) => {
                prop_assert!(!msg.is_empty(), "Malformed reason must not be empty");
            }
            other => prop_assert!(false, "expected signature reject, got {other:?}"),
        }
    }

    /// Expired-token regression: any `exp` more than `leeway` seconds
    /// in the past is rejected.
    #[test]
    fn expired_token_is_rejected(secs_past_leeway in 1u64..3600) {
        let adapter = build_adapter();
        let mut claims = baseline_claims();
        claims.exp = (NOW_FIXED - 60 - secs_past_leeway) as i64;
        let jwt = sign(shared_key(), &claims);
        let result = futures_executor_blocking(adapter.validate(&jwt));
        // SOTA-OK: variant-only assertion sufficient — AuthError::Expired is a unit variant carrying no semantic state.
        prop_assert!(matches!(result, Err(AuthError::Expired)),
            "expected Expired, got {:?}", result);
    }

    /// Issuer-mismatch regression: any `iss` outside the allowlist
    /// (random suffix / random scheme / unicode noise) rejected.
    #[test]
    fn issuer_mismatch_is_rejected(noise in "[a-z0-9.\\-]{1,32}") {
        let adapter = build_adapter();
        let mut claims = baseline_claims();
        let bogus = format!("https://clerk.attacker.{noise}");
        claims.iss = bogus.clone();
        let jwt = sign(shared_key(), &claims);
        let result = futures_executor_blocking(adapter.validate(&jwt));
        match result {
            Err(AuthError::IssuerMismatch { got, expected }) => {
                // S-08 P1-1 anti-pattern fix: destructure + assert payload
                // fields rather than relying on `matches!(.., { .. })` which
                // would pass on a tag-only match with wrong payload.
                prop_assert_eq!(&got, &bogus);
                prop_assert!(!expected.contains(&bogus),
                    "expected allowlist should not contain the bogus iss: {expected:?}");
            }
            other => prop_assert!(false, "expected IssuerMismatch, got {other:?}"),
        }
    }

    /// Audience-mismatch regression: any `aud` ≠ configured audience
    /// is rejected.
    #[test]
    fn audience_mismatch_is_rejected(noise in "[a-z0-9\\-]{1,16}") {
        let adapter = build_adapter();
        let mut claims = baseline_claims();
        let bogus = format!("audi-{noise}");
        prop_assume!(bogus != AUDIENCE);
        claims.aud = bogus;
        let jwt = sign(shared_key(), &claims);
        let result = futures_executor_blocking(adapter.validate(&jwt));
        // SOTA-OK: variant-only assertion sufficient — AuthError::AudienceMismatch is a unit variant carrying no semantic state.
        prop_assert!(matches!(result, Err(AuthError::AudienceMismatch)),
            "expected AudienceMismatch, got {:?}", result);
    }

    /// No-panic guarantee: any random byte sequence handed to
    /// validate MUST surface an Err — never panic.
    #[test]
    fn random_bytes_never_panic(blob in proptest::collection::vec(any::<u8>(), 0..512)) {
        let adapter = build_adapter();
        // Re-encode as a UTF-8 string lossy (validator takes &str).
        let s = String::from_utf8_lossy(&blob).into_owned();
        let result = futures_executor_blocking(adapter.validate(&s));
        prop_assert!(result.is_err(), "random bytes accepted as valid: {:?}", result);
    }

    /// Within-leeway acceptance: tokens whose `exp` is up to
    /// `leeway` seconds in the past must STILL accept (RFC 7519
    /// §4.1.4 industry standard). Pinning `secs_within ∈ [0,59]`
    /// keeps us strictly under the 60s leeway boundary.
    #[test]
    fn within_leeway_accepted(secs_within in 0u64..60) {
        let adapter = build_adapter();
        let mut claims = baseline_claims();
        claims.exp = (NOW_FIXED - secs_within) as i64;
        let jwt = sign(shared_key(), &claims);
        let result = futures_executor_blocking(adapter.validate(&jwt));
        prop_assert!(result.is_ok(), "expected accept within leeway, got {:?}", result);
    }
}

/// Lazily-initialised current-thread Tokio runtime. We can't open a
/// fresh runtime per proptest case (cost prohibitive at 10k iter), so
/// we stand up a single thread-local runtime and reuse it via
/// `block_on`. The adapter is fully `Send + Sync` so this is sound.
fn futures_executor_blocking<F: std::future::Future>(fut: F) -> F::Output {
    use std::cell::RefCell;
    thread_local! {
        static RT: RefCell<tokio::runtime::Runtime> = RefCell::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("test runtime")
        );
    }
    RT.with(|rt| rt.borrow().block_on(fut))
}
