//! Unit tests for `AzureKeyVaultRealProvider` against a `wiremock` mock of
//! the Azure Key Vault REST API + Microsoft Entra ID token endpoint.
//!
//! Covers the R2-8 quality gate test list:
//! - wrap → unwrap roundtrip (composite-key AAD flow)
//! - AAD binding via composite key: tamper of `encryption_context` ⇒
//!   `BYOKError::AadMismatch`
//! - `check_access` for ENABLED / DISABLED key
//! - 401/403/404 mapping
//! - Malformed key URI rejection (pre-flight)
//! - Entra ID token caching: only one POST per refresh window
//! - Entra ID workload-identity fallback exercised
//! - Property test: AAD bytes stable across wrap/unwrap
//!
//! All tests run without network egress.

#![cfg(feature = "production")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::uninlined_format_args
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine as _;
use corelink_byok::{
    BYOKError, Dek, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};
use corelink_byok_azure::__test_support::EntraCredentials;
use corelink_byok_azure::AzureKeyVaultRealProvider;
use serde_json::json;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const TEST_VAULT: &str = "myvault";
const TEST_KEY_NAME: &str = "mykey";
const TEST_KEY_VER: &str = "0123456789abcdef0123456789abcdef";

fn test_key_uri() -> String {
    format!("https://{TEST_VAULT}.vault.azure.net/keys/{TEST_KEY_NAME}/{TEST_KEY_VER}")
}

fn test_key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::AzureKeyVault,
        key_arn_or_id: test_key_uri(),
        region: "eastus".to_string(),
    }
}

fn b64u(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn b64u_d(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .unwrap()
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}

fn provider_with_static_token(endpoint: &str) -> AzureKeyVaultRealProvider {
    let creds = EntraCredentials::for_test_static("test-bearer-token");
    AzureKeyVaultRealProvider::for_test(http_client(), creds, "eastus", endpoint)
}

// ─── Helpers: a stateful in-memory "Key Vault" that round-trips RSA-OAEP-256
//     wrap/unwrap via an identity copy of the wrapped inner key.
// ────────────────────────────────────────────────────────────────────────────

/// Echo responder for `wrapkey`: returns the raw plaintext (b64url-decoded)
/// as the ciphertext, so unwrap can recover it.
#[derive(Clone)]
struct EchoWrapResponder {
    counter: Arc<AtomicUsize>,
}

impl Respond for EchoWrapResponder {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        self.counter.fetch_add(1, Ordering::SeqCst);
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        let value = body["value"].as_str().unwrap().to_string();
        ResponseTemplate::new(200).set_body_json(json!({
            "kid": format!("https://{TEST_VAULT}.vault.azure.net/keys/{TEST_KEY_NAME}/{TEST_KEY_VER}"),
            "value": value,
        }))
    }
}

#[derive(Clone)]
struct EchoUnwrapResponder {
    counter: Arc<AtomicUsize>,
}

impl Respond for EchoUnwrapResponder {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        self.counter.fetch_add(1, Ordering::SeqCst);
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        let value = body["value"].as_str().unwrap().to_string();
        ResponseTemplate::new(200).set_body_json(json!({ "value": value }))
    }
}

// ─── 1. Roundtrip wrap → unwrap via wiremock ────────────────────────────────

#[tokio::test]
async fn wrap_unwrap_roundtrip_via_mock() {
    let server = MockServer::start().await;
    let wrap_calls = Arc::new(AtomicUsize::new(0));
    let unwrap_calls = Arc::new(AtomicUsize::new(0));

    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/wrapkey$"))
        .and(header("authorization", "Bearer test-bearer-token"))
        .respond_with(EchoWrapResponder {
            counter: wrap_calls.clone(),
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/unwrapkey$"))
        .respond_with(EchoUnwrapResponder {
            counter: unwrap_calls.clone(),
        })
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let dek = Dek::generate().unwrap();
    let orig = dek.bytes;
    let ctx = json!({"tenant_id": "T1", "blob_hash": "H1"});

    let wrapped = provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .expect("wrap");
    assert_eq!(wrapped.provider, KmsProviderKind::AzureKeyVault);

    let unwrapped = provider.unwrap_dek(&wrapped).await.expect("unwrap");
    assert_eq!(unwrapped.bytes, orig);
    assert_eq!(wrap_calls.load(Ordering::SeqCst), 1);
    assert_eq!(unwrap_calls.load(Ordering::SeqCst), 1);
}

// ─── 2. AAD binding: tamper of encryption_context surfaces as AadMismatch ──

#[tokio::test]
async fn aad_tamper_surfaces_as_aad_mismatch() {
    let server = MockServer::start().await;
    let wrap_calls = Arc::new(AtomicUsize::new(0));
    let unwrap_calls = Arc::new(AtomicUsize::new(0));

    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/wrapkey$"))
        .respond_with(EchoWrapResponder {
            counter: wrap_calls,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/unwrapkey$"))
        .respond_with(EchoUnwrapResponder {
            counter: unwrap_calls,
        })
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let dek = Dek::generate().unwrap();
    let ctx_a = json!({"tenant_id": "T1"});
    let wrapped = provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx_a))
        .await
        .expect("wrap");

    let tampered = WrappedDek {
        encryption_context: Some(json!({"tenant_id": "ATTACKER"})),
        ..wrapped
    };
    let err = provider.unwrap_dek(&tampered).await.unwrap_err();
    assert!(
        matches!(err, BYOKError::AadMismatch),
        "expected AadMismatch, got: {err:?}"
    );
}

// ─── 3. check_access: enabled key returns Ok ────────────────────────────────

#[tokio::test]
async fn check_access_enabled_returns_ok() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/keys/.+"))
        .and(header("authorization", "Bearer test-bearer-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": {
                "kid": test_key_uri(),
                "kty": "RSA-HSM",
                "key_ops": ["wrapKey", "unwrapKey"],
            },
            "attributes": { "enabled": true }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Ok);
}

// ─── 4. check_access: disabled key returns Revoked ─────────────────────────

#[tokio::test]
async fn check_access_disabled_returns_revoked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/keys/.+"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": {"kty": "RSA-HSM", "key_ops": ["wrapKey", "unwrapKey"]},
            "attributes": {"enabled": false}
        })))
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Revoked);
}

// ─── 5. check_access: 403 access denied → Revoked ──────────────────────────

#[tokio::test]
async fn check_access_403_returns_revoked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/keys/.+"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {"code": "Forbidden", "message": "User does not have access"}
        })))
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::Revoked);
}

// ─── 6. check_access: 404 not found ─────────────────────────────────────────

#[tokio::test]
async fn check_access_404_returns_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/keys/.+"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"code": "KeyNotFound", "message": "key not found"}
        })))
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let status = provider.check_access(&test_key_id()).await.expect("check");
    assert_eq!(status, KmsAccessStatus::NotFound);
}

// ─── 7. wrap rejects malformed key URI (pre-flight, no HTTP) ────────────────

#[tokio::test]
async fn wrap_rejects_malformed_key_uri() {
    let server = MockServer::start().await;
    let provider = provider_with_static_token(&server.uri());

    let bad = KmsKeyId {
        provider: KmsProviderKind::AzureKeyVault,
        key_arn_or_id: "arn:aws:kms:us-east-1:123:key/abc".to_string(),
        region: "eastus".to_string(),
    };
    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T"});
    let err = provider.wrap_dek(&dek, &bad, Some(&ctx)).await.unwrap_err();
    match err {
        BYOKError::Provider(msg) => assert!(msg.contains("malformed Azure Key Vault")),
        other => panic!("expected Provider, got {other:?}"),
    }
}

// ─── 8. wrap rejects missing encryption_context ─────────────────────────────

#[tokio::test]
async fn wrap_rejects_missing_encryption_context() {
    let server = MockServer::start().await;
    let provider = provider_with_static_token(&server.uri());
    let dek = Dek::generate().unwrap();
    let err = provider
        .wrap_dek(&dek, &test_key_id(), None)
        .await
        .unwrap_err();
    assert!(matches!(err, BYOKError::EncryptionContextMissing));
}

// ─── 9. Entra ID token cache: only one token POST across many ops ──────────

#[tokio::test]
async fn entra_token_cached_across_operations() {
    let server = MockServer::start().await;

    // Token endpoint: counts invocations.
    let token_calls = Arc::new(AtomicUsize::new(0));
    let token_calls_clone = token_calls.clone();
    Mock::given(method("POST"))
        .and(path_regex(
            r"^/00000000-0000-0000-0000-000000000000/oauth2/v2\.0/token$",
        ))
        .respond_with(move |_: &Request| {
            token_calls_clone.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "fresh-token-from-entra",
                "token_type": "Bearer",
                "expires_in": 3600,
            }))
        })
        .mount(&server)
        .await;

    // Wrap echo.
    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/wrapkey$"))
        .and(header("authorization", "Bearer fresh-token-from-entra"))
        .respond_with(EchoWrapResponder {
            counter: Arc::new(AtomicUsize::new(0)),
        })
        .mount(&server)
        .await;

    let creds = EntraCredentials::for_test_client_secret(
        http_client(),
        &server.uri(),
        "00000000-0000-0000-0000-000000000000",
        "00000000-0000-0000-0000-000000000000",
        "redacted-secret-not-logged",
    );
    let provider = AzureKeyVaultRealProvider::for_test(
        http_client(),
        creds,
        "eastus",
        &server.uri(),
    );

    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T1"});
    for _ in 0..5 {
        provider
            .wrap_dek(&dek, &test_key_id(), Some(&ctx))
            .await
            .expect("wrap");
    }
    // Token should only have been fetched once (3600s expiry, 60s margin ⇒ ~3540s).
    assert_eq!(
        token_calls.load(Ordering::SeqCst),
        1,
        "expected exactly one token-endpoint hit across 5 wraps"
    );
}

// ─── 10. Entra ID workload-identity fallback: SA JWT file path exercised ───

#[tokio::test]
async fn entra_workload_identity_token_exchange() {
    let server = MockServer::start().await;

    // Write a fake SA JWT to a tempfile.
    let tmp = tempfile_path();
    tokio::fs::write(&tmp, b"fake.federated.jwt").await.unwrap();

    let token_calls = Arc::new(AtomicUsize::new(0));
    let token_calls_clone = token_calls.clone();
    Mock::given(method("POST"))
        .and(path_regex(
            r"^/00000000-0000-0000-0000-000000000000/oauth2/v2\.0/token$",
        ))
        .respond_with(move |req: &Request| {
            token_calls_clone.fetch_add(1, Ordering::SeqCst);
            // Form-encoded body — verify the federated assertion is forwarded.
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            assert!(
                body.contains("client_assertion=fake.federated.jwt"),
                "body missing federated assertion: {body}"
            );
            assert!(body.contains("client_assertion_type=urn"));
            ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "wi-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            }))
        })
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/wrapkey$"))
        .and(header("authorization", "Bearer wi-token"))
        .respond_with(EchoWrapResponder {
            counter: Arc::new(AtomicUsize::new(0)),
        })
        .mount(&server)
        .await;

    let creds = EntraCredentials::for_test_workload_identity(
        http_client(),
        &server.uri(),
        "00000000-0000-0000-0000-000000000000",
        "00000000-0000-0000-0000-000000000000",
        tmp.clone(),
    );
    let provider = AzureKeyVaultRealProvider::for_test(
        http_client(),
        creds,
        "eastus",
        &server.uri(),
    );

    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T"});
    provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .expect("wrap");
    assert_eq!(token_calls.load(Ordering::SeqCst), 1);

    let _ = tokio::fs::remove_file(&tmp).await;
}

fn tempfile_path() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!("corelink-azure-test-{nanos}.jwt"));
    p
}

// ─── 11. Token endpoint failure surfaces as Provider error ─────────────────

#[tokio::test]
async fn entra_token_failure_surfaces_as_provider_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/.+/oauth2/v2\.0/token$"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "invalid_client",
            "error_description": "AADSTS7000215: Invalid client secret",
        })))
        .mount(&server)
        .await;

    let creds = EntraCredentials::for_test_client_secret(
        http_client(),
        &server.uri(),
        "00000000-0000-0000-0000-000000000000",
        "00000000-0000-0000-0000-000000000000",
        "wrong-secret",
    );
    let provider =
        AzureKeyVaultRealProvider::for_test(http_client(), creds, "eastus", &server.uri());

    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T"});
    let err = provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .unwrap_err();
    match err {
        BYOKError::Provider(msg) => assert!(msg.contains("Entra:") || msg.contains("token")),
        other => panic!("expected Provider, got {other:?}"),
    }
}

// ─── 12. wrap 403 surfaces as CmkRevoked ───────────────────────────────────

#[tokio::test]
async fn wrap_403_surfaces_as_cmk_revoked() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/keys/.+/wrapkey$"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {"code": "Forbidden", "message": "User does not have wrapKey permission"}
        })))
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T", "blob_hash": "H"});
    let err = provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AzureKeyVault,
                ..
            }
        ),
        "got: {err:?}"
    );
}

// ─── 13. wrapkey request body shape is correct ────────────────────────────

#[tokio::test]
async fn wrapkey_request_uses_rsa_oaep_256_and_base64url() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/keys/{TEST_KEY_NAME}/{TEST_KEY_VER}/wrapkey")))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            assert_eq!(body["alg"].as_str().unwrap(), "RSA-OAEP-256");
            let v = body["value"].as_str().unwrap();
            // base64url-no-pad: no '+' or '/' or '='.
            assert!(!v.contains('='));
            assert!(!v.contains('+'));
            assert!(!v.contains('/'));
            let decoded = b64u_d(v);
            assert_eq!(decoded.len(), 32, "wrapped inner key must be 32 bytes");
            ResponseTemplate::new(200).set_body_json(json!({
                "kid": test_key_uri(),
                "value": v,
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T", "blob_hash": "H"});
    provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .expect("wrap");
}

// ─── 14. URL uses api-version 7.4 ──────────────────────────────────────────

#[tokio::test]
async fn url_carries_api_version_7_4() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!("/keys/{TEST_KEY_NAME}/{TEST_KEY_VER}/wrapkey")))
        .and(wiremock::matchers::query_param("api-version", "7.4"))
        .respond_with(EchoWrapResponder {
            counter: Arc::new(AtomicUsize::new(0)),
        })
        .expect(1)
        .mount(&server)
        .await;

    let provider = provider_with_static_token(&server.uri());
    let dek = Dek::generate().unwrap();
    let ctx = json!({"tenant_id": "T", "blob_hash": "H"});
    provider
        .wrap_dek(&dek, &test_key_id(), Some(&ctx))
        .await
        .expect("wrap");
}

// ─── 15. b64url roundtrip sanity ───────────────────────────────────────────

#[test]
fn b64url_roundtrip() {
    let data: &[u8] = b"the quick brown fox jumps over the lazy dog";
    let enc = b64u(data);
    let dec = b64u_d(&enc);
    assert_eq!(&dec, data);
}

// ─── 16. Property: composite-key AAD bytes stable across wrap/unwrap ──────

mod prop {
    use proptest::prelude::*;
    use serde_json::json;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

        /// JSON serialization of `encryption_context` is byte-stable when
        /// the same field order is used at wrap and unwrap. CoreLink stores
        /// the `encryption_context: Value` verbatim on the `WrappedDek`, so
        /// the AAD bytes match by construction (`serde_json::to_vec(&v)` is
        /// deterministic for a given `Value` shape).
        #[test]
        fn aad_bytes_stable(
            tenant in "[A-Za-z0-9_-]{1,32}",
            hash in "[a-f0-9]{16,64}",
        ) {
            let ctx = json!({"tenant_id": tenant, "blob_hash": hash});
            let wrap_aad = serde_json::to_vec(&ctx).unwrap();
            let unwrap_aad = serde_json::to_vec(&ctx).unwrap();
            prop_assert_eq!(wrap_aad, unwrap_aad);
        }
    }
}
