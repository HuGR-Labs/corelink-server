//! Unit tests for `VaultRealProvider` — the GA-hardened HashiCorp Vault
//! Transit adapter (BYOK wave 14 R-prep).
//!
//! Coverage (≥ 14 unit + 1 prop test, mirroring the AWS reference):
//!
//! 1. `auth_method_label_static`           — `auth_method()` reports the
//!    injected backend label.
//! 2. `resolved_fips_endpoint_returns_vault_addr` — Vault FIPS is operator-
//!    config, not endpoint-routable; the accessor surfaces the configured
//!    `VAULT_ADDR` for spec validation.
//! 3. `tls_strict_enforced_is_true`        — `tls_strict_enforced()` is
//!    `true` in real-mode builds.
//! 4. `wrap_unwrap_roundtrip_via_mock`     — `encrypt` then `decrypt`
//!    against a `wiremock` server recovers the DEK.
//! 5. `aad_canonicalization_is_order_independent` — Two AAD JSON values
//!    with same keys in different order produce the same canonical bytes.
//! 6. `aad_canonicalization_encoded_as_vault_context` — The `context`
//!    parameter on the wire equals `base64(JCS(aad))` byte-for-byte
//!    (verified by intercepting the wiremock request body).
//! 7. `aad_canonicalization_context_order_independent_on_wire` — Same
//!    logical AAD with different input ordering produces byte-identical
//!    `context` parameter strings on the wire.
//! 8. `context_mismatch_surfaces_as_aad_error` — Vault HTTP 400 with
//!    "context" / "mismatch" maps to `BYOKError::AadMismatch`.
//! 9. `check_access_enabled_returns_ok`    — `Ok` for an active CMK.
//! 10. `check_access_destruction_scheduled_returns_revoked` —
//!     `deletion_time` ⇒ `Revoked`.
//! 11. `check_access_403_returns_revoked`  — 403 ⇒ `Revoked`.
//! 12. `check_access_404_returns_not_found` — 404 ⇒ `NotFound`.
//! 13. `check_access_429_returns_throttled` — 429 ⇒ `Throttled`.
//! 14. `wrap_rejects_malformed_key_name`   — Pre-flight rejection (no HTTP).
//! 15. `wrap_rejects_missing_encryption_context` — Hard fail (`EncryptionContextMissing`).
//! 16. `non_string_aad_value_rejected`     — Cross-provider portability;
//!     numeric AAD values rejected as `EnvelopeError`.
//! 17. `wrong_provider_rejected_unwrap`    — Cross-provider WrappedDek
//!     rejected (`EnvelopeError`).
//! 18. `wrap_403_surfaces_as_cmk_revoked`  — 403 on encrypt ⇒ `CmkRevoked`.
//! 19. `wrap_429_surfaces_as_throttled`    — 429 on encrypt ⇒ `Provider`
//!     with "throttled".
//! 20. `unwrap_rejects_short_plaintext`    — `DekLengthInvalid { got: 16 }`.
//! 21. `vault_token_never_leaks_in_error`  — Error messages never contain
//!     the bearer token.
//! 22. (prop) `prop_aad_jcs_roundtrip_deterministic` — Same logical AAD
//!     always produces same canonical context bytes regardless of order
//!     (encoded as Vault `context` parameter).

#![cfg(feature = "real-vault")]
#![cfg(not(target_arch = "wasm32"))]
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::uninlined_format_args,
    clippy::format_in_format_args
)]

use base64::Engine as _;
use corelink_byok::{
    BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};
use corelink_byok::vault::__test_support::VaultAuth;
use corelink_byok::vault::{canonicalize_aad_to_string_map, VaultRealProvider};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_KEY: &str = "customer-cmk";
const TEST_MOUNT: &str = "transit";
const TEST_TOKEN: &str = "hvs.test-token-DO-NOT-LOG";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64d(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

fn proptest_cases() -> u32 {
    // Runtime-configurable proptest budget (charter constraint).
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(96)
}

fn provider_with_endpoint(endpoint: &str) -> VaultRealProvider {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let auth = VaultAuth::for_test_static(TEST_TOKEN);
    VaultRealProvider::for_test(http, auth, endpoint, TEST_MOUNT, "us-east-1")
}

fn test_key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::HashicorpVault,
        key_arn_or_id: format!("transit/keys/{TEST_KEY}"),
        region: "us-east-1".to_string(),
    }
}

// ── 1. auth method label ────────────────────────────────────────────────────

#[tokio::test]
async fn auth_method_label_static() {
    let server = MockServer::start().await;
    let p = provider_with_endpoint(&server.uri());
    assert_eq!(p.auth_method(), "static");
}

// ── 2. resolved FIPS endpoint = VAULT_ADDR ──────────────────────────────────

#[tokio::test]
async fn resolved_fips_endpoint_returns_vault_addr() {
    let server = MockServer::start().await;
    let p = provider_with_endpoint(&server.uri());
    // Vault FIPS is operator-config; the accessor surfaces the configured
    // VAULT_ADDR (trimmed) for spec validation cross-reference.
    let expected = server.uri();
    let expected = expected.trim_end_matches('/');
    assert_eq!(p.resolved_fips_endpoint(), expected);
    // Trailing slash normalised away.
    let p2 = provider_with_endpoint(&format!("{}/", server.uri()));
    assert_eq!(p2.resolved_fips_endpoint(), expected);
}

// ── 3. TLS strict enforcement ───────────────────────────────────────────────

#[tokio::test]
async fn tls_strict_enforced_is_true() {
    let server = MockServer::start().await;
    let p = provider_with_endpoint(&server.uri());
    // Production real-mode: TLS 1.3 + server X.509 verification ON.
    assert!(p.tls_strict_enforced());
    assert_eq!(p.transit_mount(), TEST_MOUNT);
    assert_eq!(p.provider_kind(), KmsProviderKind::HashicorpVault);
    assert_eq!(p.region(), "us-east-1");
    assert_eq!(p.fips_level(), FipsLevel::Fips140_3_L1);
}

// ── 4. Roundtrip wrap → unwrap ──────────────────────────────────────────────

#[tokio::test]
async fn wrap_unwrap_roundtrip_via_mock() {
    let server = MockServer::start().await;

    let dek = Dek::generate().unwrap();
    let original = dek.bytes;
    let canned_vault_ct = "vault:v1:abcdEFGH==";

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/encrypt/{TEST_KEY}")))
        .and(header("x-vault-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "ciphertext": canned_vault_ct, "key_version": 1 }
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/decrypt/{TEST_KEY}")))
        .and(header("x-vault-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "plaintext": b64(&original) }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let ctx = json!({"tenant_id": "T1", "blob_hash": "H1"});

    let wrapped = provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .expect("wrap");
    assert_eq!(wrapped.provider, KmsProviderKind::HashicorpVault);
    assert_eq!(wrapped.ciphertext, canned_vault_ct.as_bytes());

    let unwrapped = provider.unwrap_dek(&wrapped).await.expect("unwrap");
    assert_eq!(unwrapped.bytes, original);
}

// ── 5. AAD JCS canonicalization is order independent (helper-level) ─────────

#[test]
fn aad_canonicalization_is_order_independent() {
    let a = json!({"tenant_id": "t-1", "blob_hash": "sha256:abc"});
    let b = json!({"blob_hash": "sha256:abc", "tenant_id": "t-1"});
    let (_, ba) = canonicalize_aad_to_string_map(&a).unwrap();
    let (_, bb) = canonicalize_aad_to_string_map(&b).unwrap();
    assert_eq!(ba, bb);
}

// ── 6. Wire-level: context = base64(JCS(aad)) ───────────────────────────────

#[tokio::test]
async fn aad_canonicalization_encoded_as_vault_context() {
    let server = MockServer::start().await;

    let dek = Dek::generate().unwrap();
    let expected_plaintext = b64(&dek.bytes);
    let ctx = json!({"tenant_id": "T-CTX", "blob_hash": "H-CTX"});
    let (_, canonical_aad) = canonicalize_aad_to_string_map(&ctx).unwrap();
    let expected_context = b64(&canonical_aad);

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/encrypt/{TEST_KEY}")))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value =
                serde_json::from_slice(&req.body).expect("json body");
            assert_eq!(body["plaintext"].as_str().unwrap(), expected_plaintext);
            assert_eq!(
                body["context"].as_str().unwrap(),
                expected_context,
                "Vault `context` MUST equal base64(JCS(aad))"
            );
            ResponseTemplate::new(200).set_body_json(json!({
                "data": { "ciphertext": "vault:v1:OK==", "key_version": 1 }
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .expect("wrap");
}

// ── 7. Wire-level: context ordering invariance ──────────────────────────────

#[tokio::test]
async fn aad_canonicalization_context_order_independent_on_wire() {
    let server = MockServer::start().await;

    // Build two AAD JSON objects with same logical content, opposite order.
    let aad_forward = json!({"a_key": "v-a", "b_key": "v-b", "c_key": "v-c"});
    let aad_reverse = json!({"c_key": "v-c", "b_key": "v-b", "a_key": "v-a"});

    let (_, canonical_forward) = canonicalize_aad_to_string_map(&aad_forward).unwrap();
    let (_, canonical_reverse) = canonicalize_aad_to_string_map(&aad_reverse).unwrap();
    let expected_context = b64(&canonical_forward);
    assert_eq!(canonical_forward, canonical_reverse);

    // Wiremock intercepts both encrypt requests and asserts they carry
    // the SAME `context` string regardless of input ordering.
    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/encrypt/{TEST_KEY}")))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value =
                serde_json::from_slice(&req.body).expect("json body");
            assert_eq!(body["context"].as_str().unwrap(), expected_context);
            ResponseTemplate::new(200).set_body_json(json!({
                "data": { "ciphertext": "vault:v1:OK==", "key_version": 1 }
            }))
        })
        .expect(2)
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let dek = Dek::generate().unwrap();
    provider
        .wrap_dek(&dek, &test_key_id(), Some(&aad_forward))
        .await
        .expect("wrap forward");
    provider
        .wrap_dek(&dek, &test_key_id(), Some(&aad_reverse))
        .await
        .expect("wrap reverse");
}

// ── 8. Context mismatch surfaces as AadMismatch ─────────────────────────────

#[tokio::test]
async fn context_mismatch_surfaces_as_aad_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/decrypt/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errors": ["cipher: message authentication failed (context mismatch)"]
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let wrapped = WrappedDek {
        provider: KmsProviderKind::HashicorpVault,
        key_id: test_key_id(),
        ciphertext: b"vault:v1:ZZZ==".to_vec(),
        encryption_context: Some(json!({"tenant_id": "TAMPERED"})),
    };
    let err = provider.unwrap_dek(&wrapped).await.unwrap_err();
    assert!(matches!(err, BYOKError::AadMismatch), "got: {err:?}");
}

// ── 9. check_access ENABLED key → Ok ───────────────────────────────────────

#[tokio::test]
async fn check_access_enabled_returns_ok() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/{TEST_MOUNT}/keys/{TEST_KEY}")))
        .and(header("x-vault-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "name": TEST_KEY,
                "deletion_allowed": false,
                "min_decryption_version": 1,
                "min_encryption_version": 0,
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Ok);
}

// ── 10. check_access scheduled-for-destruction → Revoked ───────────────────

#[tokio::test]
async fn check_access_destruction_scheduled_returns_revoked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/{TEST_MOUNT}/keys/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "name": TEST_KEY,
                "deletion_allowed": true,
                "deletion_time": "2026-06-01T00:00:00Z",
            }
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Revoked);
}

// ── 11. check_access 403 → Revoked (permission denied) ─────────────────────

#[tokio::test]
async fn check_access_403_returns_revoked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/{TEST_MOUNT}/keys/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "errors": ["permission denied"]
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Revoked);
}

// ── 12. check_access 404 → NotFound ────────────────────────────────────────

#[tokio::test]
async fn check_access_404_returns_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/{TEST_MOUNT}/keys/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "errors": []
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::NotFound);
}

// ── 13. check_access 429 → Throttled ───────────────────────────────────────

#[tokio::test]
async fn check_access_429_returns_throttled() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/{TEST_MOUNT}/keys/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "errors": ["rate limit exceeded"]
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Throttled);
}

// ── 14. wrap rejects malformed key name (pre-flight) ───────────────────────

#[tokio::test]
async fn wrap_rejects_malformed_key_name() {
    let server = MockServer::start().await;
    let provider = provider_with_endpoint(&server.uri());

    let bad = KmsKeyId {
        provider: KmsProviderKind::HashicorpVault,
        key_arn_or_id: "../etc/passwd".to_string(),
        region: "us-east-1".to_string(),
    };
    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T", "blob_hash": "H"});
    let err = provider.wrap_dek(&dek, &bad, Some(&ctx)).await.unwrap_err();
    match err {
        BYOKError::Provider(msg) => assert!(msg.contains("malformed Vault Transit")),
        other => panic!("expected Provider, got {other:?}"),
    }
}

// ── 15. wrap rejects missing encryption_context ────────────────────────────

#[tokio::test]
async fn wrap_rejects_missing_encryption_context() {
    let server = MockServer::start().await;
    let provider = provider_with_endpoint(&server.uri());

    let dek = Dek::generate().unwrap();
    let err = provider
        .wrap_dek(&dek, &test_key_id(), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BYOKError::EncryptionContextMissing),
        "got: {err:?}"
    );
}

// ── 16. Non-string AAD value rejected pre-HTTP ─────────────────────────────

#[tokio::test]
async fn non_string_aad_value_rejected() {
    let server = MockServer::start().await;
    let provider = provider_with_endpoint(&server.uri());

    let bad_aad = json!({"tenant_id": "T", "weight": 42});
    let dek = Dek::generate().unwrap();
    let err = provider
        .wrap_dek(&dek, &test_key_id(), Some(&bad_aad))
        .await
        .unwrap_err();
    assert!(matches!(err, BYOKError::EnvelopeError(_)), "got: {err:?}");
}

// ── 17. unwrap rejects WrappedDek with wrong provider ──────────────────────

#[tokio::test]
async fn wrong_provider_rejected_unwrap() {
    let server = MockServer::start().await;
    let provider = provider_with_endpoint(&server.uri());

    let wrapped = WrappedDek {
        provider: KmsProviderKind::AwsKms,
        key_id: KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: "arn:aws:kms:us-east-1:0:key/00000000-0000-0000-0000-000000000000"
                .to_string(),
            region: "us-east-1".to_string(),
        },
        ciphertext: b"vault:v1:OK==".to_vec(),
        encryption_context: Some(json!({"tenant_id": "T"})),
    };
    let err = provider.unwrap_dek(&wrapped).await.unwrap_err();
    assert!(matches!(err, BYOKError::EnvelopeError(_)), "got: {err:?}");
}

// ── 18. wrap 403 → CmkRevoked ──────────────────────────────────────────────

#[tokio::test]
async fn wrap_403_surfaces_as_cmk_revoked() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/encrypt/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "errors": ["permission denied"]
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let dek = Dek::generate().unwrap();
    let err = provider
        .wrap_dek(
            &dek,
            &test_key_id(),
            Some(&json!({"tenant_id": "T", "blob_hash": "H"})),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, BYOKError::CmkRevoked { provider: KmsProviderKind::HashicorpVault, .. }),
        "got: {err:?}"
    );
}

// ── 19. wrap 429 → Provider("... throttled") ───────────────────────────────

#[tokio::test]
async fn wrap_429_surfaces_as_throttled() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/encrypt/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "errors": ["rate limit exceeded"]
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let dek = Dek::generate().unwrap();
    let err = provider
        .wrap_dek(
            &dek,
            &test_key_id(),
            Some(&json!({"tenant_id": "T", "blob_hash": "H"})),
        )
        .await
        .unwrap_err();
    match err {
        BYOKError::Provider(msg) => assert!(msg.contains("throttled"), "msg={msg}"),
        other => panic!("expected Provider(throttled), got {other:?}"),
    }
}

// ── 20. unwrap rejects DEK length != 32 ────────────────────────────────────

#[tokio::test]
async fn unwrap_rejects_short_plaintext() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/decrypt/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "plaintext": b64(&[0u8; 16]) }
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let wrapped = WrappedDek {
        provider: KmsProviderKind::HashicorpVault,
        key_id: test_key_id(),
        ciphertext: b"vault:v1:OK==".to_vec(),
        encryption_context: Some(json!({"tenant_id": "T", "blob_hash": "H"})),
    };
    let err = provider.unwrap_dek(&wrapped).await.unwrap_err();
    assert!(
        matches!(err, BYOKError::DekLengthInvalid { got: 16 }),
        "got: {err:?}"
    );
}

// ── 21. Vault tokens never appear in error messages ────────────────────────

#[tokio::test]
async fn vault_token_never_leaks_in_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/{TEST_MOUNT}/encrypt/{TEST_KEY}")))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "errors": ["internal server error"]
        })))
        .mount(&server)
        .await;

    let provider = provider_with_endpoint(&server.uri());
    let dek = Dek::generate().unwrap();
    let err = provider
        .wrap_dek(
            &dek,
            &test_key_id(),
            Some(&json!({"tenant_id": "T", "blob_hash": "H"})),
        )
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(!msg.contains(TEST_TOKEN), "token leaked: {msg}");
}

// ── 22. b64 roundtrip sanity ───────────────────────────────────────────────

#[test]
fn b64_decode_roundtrip() {
    let data = b"vault transit test";
    assert_eq!(b64d(&b64(data)), data);
}

// ── 23. Property test: context bytes deterministic across orderings ────────

mod prop {
    use super::*;
    use proptest::prelude::*;

    fn arb_kv() -> impl Strategy<Value = (String, String)> {
        let key = "[a-z][a-z0-9_]{0,15}";
        let val = "[a-zA-Z0-9_:.-]{1,32}";
        (key, val).prop_map(|(k, v)| (k, v))
    }

    fn arb_ctx_pair() -> impl Strategy<Value = (serde_json::Value, serde_json::Value)> {
        proptest::collection::vec(arb_kv(), 1..=6).prop_map(|mut kvs| {
            let mut m1 = serde_json::Map::new();
            for (k, v) in &kvs {
                m1.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            kvs.reverse();
            let mut m2 = serde_json::Map::new();
            for (k, v) in &kvs {
                m2.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            (serde_json::Value::Object(m1), serde_json::Value::Object(m2))
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

        /// Same logical AAD (same key/value set, any order) ⇒ same canonical
        /// bytes ⇒ same `base64` Vault `context` parameter. This is the
        /// determinism invariant per INV-BYOK-CRYPTO-SOVEREIGNTY, encoded
        /// at the Vault Transit `context` slot.
        #[test]
        fn prop_aad_jcs_roundtrip_deterministic((a, b) in arb_ctx_pair()) {
            let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
            let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
            prop_assert_eq!(&bytes_a, &bytes_b);
            // And the base64-encoded form (what Vault sees) is also stable.
            let ctx_a = base64::engine::general_purpose::STANDARD.encode(&bytes_a);
            let ctx_b = base64::engine::general_purpose::STANDARD.encode(&bytes_b);
            prop_assert_eq!(ctx_a, ctx_b);
        }
    }
}
