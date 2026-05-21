//! Live integration tests against the HuGR Wallet broker → Stripe test
//! mode upstream.
//!
//! Wave-31 (wallet-broker series, stream-1): the client no longer holds
//! a real `STRIPE_SECRET_KEY`. The wallet ref `stripe-prod-test` MUST
//! be pre-provisioned by the wallet operator with a `sk_test_...`
//! upstream secret + `proxy` scope on the `hugrw_` token below.
//!
//! Gated behind `#[cfg(feature = "live-integration")]` so the default
//! `cargo test -p corelink-stripe-real` NEVER hits the network. Run with:
//!
//! ```sh
//! HUGR_WALLET_BASE=https://api.humangr.com \
//! HUGR_WALLET_TOKEN=hugrw_live_test_... \
//! HUGR_STRIPE_REF=stripe-prod-test \
//! STRIPE_PRICE_ID_STARTER=price_... \
//! cargo test -p corelink-stripe-real --features live-integration -- --ignored
//! ```
//!
//! Tests are `#[ignore]` even with the feature flag so CI must opt in
//! explicitly with `--include-ignored`.

#![cfg(feature = "live-integration")]

use std::env;

use corelink_stripe_real::{StripeError, StripeRealClient};
use corelink_tier_selection::stripe::{CheckoutSessionRequest, StripeClient};
use corelink_tier_selection::tenant::TenantId;
use corelink_tier_selection::tier::TierKind;

fn require_wallet_token() -> StripeRealClient {
    let _ = env::var("HUGR_WALLET_TOKEN").expect("HUGR_WALLET_TOKEN must be set");
    StripeRealClient::from_env().expect("client init")
}

#[test]
#[ignore = "live network"]
fn live_create_customer() {
    let c = require_wallet_token();
    let idem = format!("live-test-customer-{}", uuid_like());
    let cust = c
        .create_customer("integration@example.test", "tenant_live_int", &idem)
        .expect("create customer");
    assert!(cust.id.starts_with("cus_"));
}

#[test]
#[ignore = "live network"]
fn live_get_customer_404() {
    let c = require_wallet_token();
    let err = c.get_customer("cus_does_not_exist_xyz").unwrap_err();
    // Stripe returns 404 with `resource_missing` → Generic.
    match err {
        StripeError::Generic { http_status, .. } => assert_eq!(http_status, 404),
        other => panic!("expected Generic 404, got {other:?}"),
    }
}

#[test]
#[ignore = "live network"]
fn live_create_checkout_session_starter() {
    let c = require_wallet_token();
    let req = CheckoutSessionRequest {
        tenant_id: TenantId::new(format!("live_t_{}", uuid_like())),
        tier: TierKind::Starter,
        customer_email: "live@example.test".to_string(),
        success_url: "https://example.test/ok".to_string(),
        cancel_url: "https://example.test/cancel".to_string(),
    };
    let resp = c.create_checkout_session(&req).expect("checkout session");
    assert!(resp.session_id.starts_with("cs_"));
    assert!(resp.url.starts_with("https://"));
}

#[test]
#[ignore = "live network"]
fn live_idempotent_checkout_returns_same_session() {
    let c = require_wallet_token();
    let tenant = TenantId::new(format!("live_idem_{}", uuid_like()));
    let req = CheckoutSessionRequest {
        tenant_id: tenant,
        tier: TierKind::Starter,
        customer_email: "idem@example.test".to_string(),
        success_url: "https://example.test/ok".to_string(),
        cancel_url: "https://example.test/cancel".to_string(),
    };
    let a = c.create_checkout_session(&req).expect("first");
    let b = c.create_checkout_session(&req).expect("second");
    assert_eq!(a.session_id, b.session_id, "idempotency-key replay");
}

#[test]
#[ignore = "live network"]
fn live_billing_portal_session() {
    let c = require_wallet_token();
    let idem_cust = format!("live-portal-cust-{}", uuid_like());
    let cust = c
        .create_customer("portal@example.test", "tenant_portal", &idem_cust)
        .expect("customer");
    let idem_portal = format!("live-portal-{}", uuid_like());
    let session = c
        .create_billing_portal_session(&cust.id, "https://example.test/return", &idem_portal)
        .expect("portal");
    assert!(session.id.starts_with("bps_"));
    assert!(session.url.starts_with("https://"));
}

#[test]
#[ignore = "live network"]
fn live_authentication_failure_bad_token() {
    use corelink_stripe_real::StripeClientConfig;
    use secrecy::SecretString;
    let cfg = StripeClientConfig::wallet_broker(
        env::var("HUGR_WALLET_BASE").unwrap_or_else(|_| "https://api.humangr.com".to_string()),
        SecretString::from("hugrw_INVALID_TOKEN".to_string()),
        env::var("HUGR_STRIPE_REF").unwrap_or_else(|_| "stripe-prod-test".to_string()),
    );
    let c = StripeRealClient::builder().config(cfg).build().expect("builder");
    let err = c.get_customer("cus_anything").unwrap_err();
    // Wallet broker rejects the invalid hugrw_ token before forwarding;
    // it returns an HTTP error mapped to either Authentication (401)
    // or Generic (403/4xx) depending on the wallet's error contract.
    assert!(
        matches!(
            err,
            StripeError::Authentication(_) | StripeError::Generic { .. }
        ),
        "expected Authentication/Generic for bad hugrw_ token, got {err:?}"
    );
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{ns:x}")
}
