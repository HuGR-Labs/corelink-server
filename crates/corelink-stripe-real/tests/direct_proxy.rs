//! Wave-31 dual-mode auth (2026-05-21) — Direct-mode integration tests.
//!
//! Pin the Direct-mode URL + auth contract for the Stripe HTTPS client
//! against a `wiremock::MockServer` impersonating `api.stripe.com`.
//! Three load-bearing assertions:
//!
//! - `direct_mode_uses_api_stripe_com_url` — the constructed request
//!   URL is `{mock_api_base}/v1/customers` (NOT
//!   `{wallet_base}/_wallet/proxy/...`). Confirms the Direct variant
//!   of the enum is in force.
//! - `direct_mode_uses_sk_token_auth` — the `Authorization` header is
//!   `Bearer sk_test_<key>` (NOT `Bearer hugrw_<token>`, NOT HTTP
//!   Basic).
//! - `direct_mode_fails_closed_on_upstream_5xx` — when Stripe returns
//!   5xx after retries exhaust, the client surfaces a CoreLink-side
//!   `StripeError::Generic` 5xx and NEVER silently falls back to the
//!   wallet-broker mode.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use corelink_stripe_real::{RetryPolicy, StripeClientConfig, StripeError, StripeRealClient};
use secrecy::SecretString;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn direct_config_for(server: &MockServer, api_key: &str) -> StripeClientConfig {
    StripeClientConfig::direct(server.uri(), SecretString::from(api_key.to_string()))
}

#[tokio::test]
async fn direct_mode_uses_api_stripe_com_url() {
    let server = MockServer::start().await;
    // Mount on the FLAT Stripe path. If the client tried to hit a
    // wallet-broker proxy path like `/_wallet/proxy/.../v1/customers`
    // the mock would never see the request and wiremock would 404.
    Mock::given(method("POST"))
        .and(path("/v1/customers"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"id":"cus_directtest","email":"d@example.test"}"#),
        )
        .mount(&server)
        .await;

    let cfg = direct_config_for(&server, "sk_test_url");
    let base = cfg.effective_base_url();
    let cust = tokio::task::spawn_blocking(move || {
        let client = StripeRealClient::builder()
            .config(cfg)
            .build()
            .expect("build");
        // Sanity: the base URL on the client matches what we computed
        // pre-spawn (i.e. no wallet-broker mutation slipped in).
        assert_eq!(client.effective_base_url(), base);
        client.create_customer("d@example.test", "tenant_direct_url", "idem-direct-url-1")
    })
    .await
    .expect("join")
    .expect("create_customer");
    assert_eq!(cust.id, "cus_directtest");

    let received = server.received_requests().await.expect("wiremock log");
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].url.path(),
        "/v1/customers",
        "Direct mode must hit the flat Stripe path, NOT the wallet proxy"
    );
    assert!(
        !received[0].url.path().contains("_wallet"),
        "Direct mode URL must not carry a wallet proxy segment"
    );
}

#[tokio::test]
async fn direct_mode_uses_sk_token_auth() {
    let server = MockServer::start().await;
    // The mock REQUIRES `Authorization: Bearer sk_test_auth`. If the
    // client sent `Bearer hugrw_...` (the wallet-broker form) or HTTP
    // Basic the matcher would fail and wiremock would respond 404 —
    // which the client would surface as StripeError::Generic, failing
    // the assert below.
    Mock::given(method("POST"))
        .and(path("/v1/customers"))
        .and(header("Authorization", "Bearer sk_test_auth"))
        .and(header_exists("Idempotency-Key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"id":"cus_authdirect","email":"a@example.test"}"#),
        )
        .mount(&server)
        .await;

    let cfg = direct_config_for(&server, "sk_test_auth");
    let cust = tokio::task::spawn_blocking(move || {
        let client = StripeRealClient::builder()
            .config(cfg)
            .build()
            .expect("build");
        client.create_customer("a@example.test", "tenant_direct_auth", "idem-direct-auth-1")
    })
    .await
    .expect("join")
    .expect("create_customer");
    assert_eq!(cust.id, "cus_authdirect");

    let received = server.received_requests().await.expect("wiremock log");
    assert_eq!(received.len(), 1);
    let auth = received[0]
        .headers
        .get("authorization")
        .map(|v| v.to_str().unwrap_or_default().to_string())
        .unwrap_or_default();
    assert_eq!(auth, "Bearer sk_test_auth");
    assert!(
        !auth.contains("hugrw_"),
        "Direct-mode auth header must NOT carry the wallet `hugrw_` prefix: {auth}"
    );
    assert!(
        !auth.contains("Basic"),
        "Direct-mode auth header must NOT be HTTP Basic: {auth}"
    );
}

#[tokio::test]
async fn direct_mode_fails_closed_on_upstream_5xx() {
    let server = MockServer::start().await;
    // Every attempt returns 503. The retry policy MUST exhaust and
    // the client MUST surface a CoreLink-side 5xx error — with NO
    // silent fallback to wallet-broker mode (would expose the
    // upstream key through the broker).
    Mock::given(method("POST"))
        .and(path("/v1/customers"))
        .respond_with(ResponseTemplate::new(503).set_body_string(
            r#"{"error":{"type":"api_error","message":"stripe upstream unavailable"}}"#,
        ))
        .mount(&server)
        .await;

    let cfg = direct_config_for(&server, "sk_test_failclosed");
    // Tight retry budget so the test runs fast (max_retries=2,
    // base_ms=1, cap_ms=5).
    let policy = RetryPolicy::new(2, 1, 5);
    let err = tokio::task::spawn_blocking(move || {
        let client = StripeRealClient::builder()
            .config(cfg)
            .retry_policy(policy)
            .build()
            .expect("build");
        client.create_customer("fc@example.test", "tenant_direct_fc", "idem-direct-fc-1")
    })
    .await
    .expect("join")
    .expect_err("upstream 5xx must surface as CoreLink-side error");

    match err {
        StripeError::Generic { http_status, .. } => {
            assert_eq!(
                http_status, 503,
                "Stripe 5xx must map to CoreLink-side 5xx, NOT fall back to wallet broker"
            );
        }
        other => panic!("expected Generic 503 on upstream outage, got {other:?}"),
    }

    // Verify the URL was the FLAT Stripe path — proving no fallback
    // to the wallet broker was attempted on Stripe failure.
    let received = server.received_requests().await.expect("wiremock log");
    assert!(!received.is_empty(), "Stripe must have received the calls");
    for r in &received {
        assert_eq!(
            r.url.path(),
            "/v1/customers",
            "every retry MUST hit the flat Stripe path (no fallback to wallet broker)"
        );
        assert!(
            !r.url.path().contains("_wallet"),
            "no fallback to wallet broker permitted on Direct-mode 5xx"
        );
    }
}
