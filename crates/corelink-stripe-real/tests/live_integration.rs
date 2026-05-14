//! Live integration tests against Stripe test mode.
//!
//! Gated behind `#[cfg(feature = "live-integration")]` so the default
//! `cargo test -p corelink-stripe-real` NEVER hits the network. Run with:
//!
//! ```sh
//! STRIPE_SECRET_KEY_TEST=sk_test_... \
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

fn require_test_key() -> StripeRealClient {
    let _ = env::var("STRIPE_SECRET_KEY_TEST").expect("STRIPE_SECRET_KEY_TEST must be set");
    StripeRealClient::from_env().expect("client init")
}

#[test]
#[ignore = "live network"]
fn live_create_customer() {
    let c = require_test_key();
    let idem = format!("live-test-customer-{}", uuid_like());
    let cust = c
        .create_customer("integration@example.test", "tenant_live_int", &idem)
        .expect("create customer");
    assert!(cust.id.starts_with("cus_"));
}

#[test]
#[ignore = "live network"]
fn live_get_customer_404() {
    let c = require_test_key();
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
    let c = require_test_key();
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
    let c = require_test_key();
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
    let c = require_test_key();
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
fn live_authentication_failure_bad_key() {
    let c = StripeRealClient::builder()
        .api_key("sk_test_INVALID_KEY_xxxxxxx")
        .build()
        .expect("builder");
    let err = c.get_customer("cus_anything").unwrap_err();
    assert!(matches!(err, StripeError::Authentication(_)));
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{ns:x}")
}
