//! Checkout promo-code / coupon wiring — integration tests against a
//! `wiremock::MockServer` impersonating `api.stripe.com`.
//!
//! Pins that the real `create_checkout_session` path threads the
//! checkout-level discount capability all the way into the
//! `POST /v1/checkout/sessions` form body:
//!
//! - `default_checkout_sends_allow_promotion_codes` — absent a configured
//!   launch coupon, the request carries `allow_promotion_codes=true` (Stripe's
//!   hosted page shows the promo-code field) and NO `discounts[…]` pair.
//! - `configured_coupon_preapplies_discount` — with `STRIPE_LAUNCH_COUPON`
//!   set, the request carries `discounts[0][coupon]=<id>` and NO
//!   `allow_promotion_codes` pair (Stripe rejects both together → clean
//!   checkout → $0).
//!
//! Both are mutually exclusive by construction; these tests are the wire-level
//! proof that the two are never sent together.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use corelink_stripe_real::{StripeClientConfig, StripeRealClient};
use corelink_tier_selection::stripe::{CheckoutSessionRequest, StripeClient};
use corelink_tier_selection::tenant::TenantId;
use corelink_tier_selection::tier::TierKind;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Serialise the whole file: `create_checkout_session` reads the
/// process-global `STRIPE_PRICE_ID_*` + `STRIPE_LAUNCH_COUPON` env vars, which
/// would race across concurrently-run `#[tokio::test]`s in this binary.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn checkout_req() -> CheckoutSessionRequest {
    CheckoutSessionRequest::new(
        TenantId::new("tenant_promo"),
        TierKind::Starter,
        "buyer@example.test",
        "https://app/ok",
        "https://app/cancel",
    )
}

fn ok_session_body() -> String {
    r#"{"id":"cs_test_promo","url":"https://checkout.stripe.com/c/pay/cs_test_promo","customer":"cus_promo"}"#
        .to_string()
}

#[tokio::test]
async fn default_checkout_sends_allow_promotion_codes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ok_session_body()))
        .mount(&server)
        .await;

    let cfg = StripeClientConfig::direct(server.uri(), SecretString::from("sk_test_promo".to_string()));
    let uri = server.uri();
    // The env mutation + client call live entirely inside the blocking
    // closure, and the process-wide ENV_LOCK is acquired + released THERE
    // (never held across an `.await` — clippy `await_holding_lock`).
    let resp = tokio::task::spawn_blocking(move || {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var("STRIPE_PRICE_ID_STARTER", "price_starter_test");
        std::env::remove_var("STRIPE_LAUNCH_COUPON");
        let client = StripeRealClient::builder().config(cfg).build().expect("build");
        assert_eq!(client.effective_base_url(), uri);
        client.create_checkout_session(&checkout_req())
    })
    .await
    .expect("join")
    .expect("checkout session");
    assert_eq!(resp.session_id, "cs_test_promo");

    let received = server.received_requests().await.expect("wiremock log");
    assert_eq!(received.len(), 1);
    let body = String::from_utf8(received[0].body.clone()).expect("utf8 body");
    assert!(
        body.contains("allow_promotion_codes=true"),
        "default checkout must send allow_promotion_codes=true: {body}"
    );
    assert!(
        !body.contains("discounts"),
        "default checkout must NOT send discounts (mutually exclusive): {body}"
    );
}

#[tokio::test]
async fn configured_coupon_preapplies_discount() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ok_session_body()))
        .mount(&server)
        .await;

    let cfg = StripeClientConfig::direct(server.uri(), SecretString::from("sk_test_promo".to_string()));
    // ENV_LOCK is acquired + released inside the blocking closure so it is
    // never held across an `.await` (clippy `await_holding_lock`).
    let resp = tokio::task::spawn_blocking(move || {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var("STRIPE_PRICE_ID_STARTER", "price_starter_test");
        std::env::set_var("STRIPE_LAUNCH_COUPON", "coupon_LAUNCH100");
        let client = StripeRealClient::builder().config(cfg).build().expect("build");
        let r = client.create_checkout_session(&checkout_req());
        // Restore so a later test in this binary is not polluted.
        std::env::remove_var("STRIPE_LAUNCH_COUPON");
        r
    })
    .await
    .expect("join")
    .expect("checkout session");
    assert_eq!(resp.session_id, "cs_test_promo");

    let received = server.received_requests().await.expect("wiremock log");
    assert_eq!(received.len(), 1);
    let body = String::from_utf8(received[0].body.clone()).expect("utf8 body");
    // Form-encoded `discounts[0][coupon]=coupon_LAUNCH100` (brackets are
    // percent-encoded by reqwest's form encoder → `discounts%5B0%5D%5Bcoupon%5D`).
    assert!(
        body.contains("coupon_LAUNCH100")
            && (body.contains("discounts") || body.contains("discounts%5B")),
        "configured coupon must pre-apply discounts[0][coupon]: {body}"
    );
    assert!(
        !body.contains("allow_promotion_codes"),
        "coupon path must NOT also send allow_promotion_codes (Stripe rejects both): {body}"
    );
}
