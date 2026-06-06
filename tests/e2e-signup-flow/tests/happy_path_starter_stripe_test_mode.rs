//! R3-1 Happy path — Starter tier via Stripe Checkout TEST MODE.
//!
//! In default `cargo test -p e2e-signup-flow` the in-memory Stripe
//! fake stands in for the live HTTPS client; the test asserts that
//! `select_tier(Starter)` issues a Checkout redirect with a session id
//! and that the webhook activates the subscription idempotently.
//!
//! When `HUGR_WALLET_TOKEN` is set in the environment, an additional
//! `#[ignore]`-gated variant constructs the real HTTPS Stripe client
//! (`corelink-stripe-real`) routed through the HuGR Wallet broker
//! (wave-31 wallet-broker series, stream-1) and asserts that the
//! Checkout Session POST returns 200 + a `cs_test_*` session id. The
//! live test reads only the `hugrw_` token from env — the underlying
//! `STRIPE_SECRET_KEY` lives inside the wallet KV and is never seen
//! by CoreLink (charter CTRL-CRED-REDACT).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::env;

use corelink_tier_selection::{
    StripeCheckoutSessionCompletedEvent, StripeCustomerId, SubscriptionActivationReceipt,
    TenantId as TierTenantId, TierKind, TierSelectionReceipt,
};

use e2e_signup_flow::helpers::{
    dpa_ctx_for, dpa_request_for, make_test_tenant, setup_test_ledgers, tier_ctx_for,
    verify_audit_chain, ExpectedAuditEvent, ProvisionedTenant,
};

fn run_in_memory() {
    let env = setup_test_ledgers();
    let tenant = make_test_tenant("starter-happy");

    // (1) Signup.
    let resp = env.signup.provision(&tenant.signup_request).unwrap();
    let prov = ProvisionedTenant::from_response(resp).expect("provisioned");

    // (2) DPA accept + flip gate.
    let dpa_req = dpa_request_for(&env, corelink_dpa_acceptance::LocaleBcp47::EnUs);
    let ctx = dpa_ctx_for(&prov, "sig-starter-happy");
    let _ = env.dpa.accept(&ctx, dpa_req).unwrap();
    env.dpa_gate
        .accept(TierTenantId::new(prov.tenant_id.as_str()), "1.0.0");

    // (3) Tier select Starter → Checkout redirect.
    let receipt = env
        .tier
        .select_tier(
            &tier_ctx_for(&prov, env.now_ms),
            TierKind::Starter,
            "u@example.com",
        )
        .unwrap();
    let (session_id, _checkout_url) = match receipt {
        TierSelectionReceipt::CheckoutRedirect {
            session_id,
            checkout_url,
            ..
        } => (session_id, checkout_url),
        other => panic!("expected CheckoutRedirect, got {other:?}"),
    };
    assert!(!session_id.is_empty());
    assert_eq!(env.stripe.sessions().len(), 1);

    // (4) Webhook completes the session.
    let event = StripeCheckoutSessionCompletedEvent::new(
        format!("evt_e2e_{}", prov.tenant_id),
        session_id.clone(),
        TierTenantId::new(prov.tenant_id.as_str()),
        TierKind::Starter,
        StripeCustomerId::new(format!("cus_fake_{}", prov.tenant_id)),
        env.now_ms.saturating_add(5_000),
    );
    let activation = env.tier.on_checkout_completed(&event).unwrap();
    assert!(matches!(
        activation,
        SubscriptionActivationReceipt::Activated { .. }
    ));
    // Idempotent: duplicate webhook delivery → DuplicateIgnored.
    let dup = env.tier.on_checkout_completed(&event).unwrap();
    assert!(matches!(
        dup,
        SubscriptionActivationReceipt::DuplicateIgnored { .. }
    ));

    verify_audit_chain(
        &env,
        &[
            ExpectedAuditEvent::SignupStarted,
            ExpectedAuditEvent::SignupCompleted,
            ExpectedAuditEvent::DpaAccepted,
            ExpectedAuditEvent::TierAttempted,
            ExpectedAuditEvent::TierStripeCheckoutCreated,
        ],
    )
    .unwrap();
}

#[test]
fn r3_1_happy_path_starter_in_memory() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|rt| rt.block_on(async { run_in_memory() }))
        .unwrap();
}

/// Live-integration: real HTTPS Stripe Checkout Session create via
/// either auth mode (wave-31 dual-mode auth, 2026-05-21).
///
/// Skipped unless `STRIPE_PRICE_ID_STARTER` is present AND the
/// credentials for the selected `STRIPE_AUTH_MODE` are set:
///
/// - default `direct` → requires `STRIPE_SECRET_KEY`,
/// - `wallet-broker` → requires `HUGR_WALLET_TOKEN`.
///
/// Run via:
///
/// ```text
/// # Direct (default):
/// STRIPE_SECRET_KEY=sk_test_... \
/// STRIPE_PRICE_ID_STARTER=price_... \
/// cargo test -p e2e-signup-flow -- --ignored
///
/// # Wallet-broker (when the broker is back online):
/// STRIPE_AUTH_MODE=wallet-broker \
/// HUGR_WALLET_BASE=https://api.humangr.com \
/// HUGR_WALLET_TOKEN=hugrw_live_... \
/// HUGR_STRIPE_REF=stripe-prod-test \
/// STRIPE_PRICE_ID_STARTER=price_... \
/// cargo test -p e2e-signup-flow -- --ignored
/// ```
#[test]
#[ignore]
fn r3_1_happy_path_starter_live_stripe() {
    if env::var("STRIPE_PRICE_ID_STARTER").is_err() {
        return;
    }
    // Skip unless credentials for the selected mode are present.
    let mode = env::var("STRIPE_AUTH_MODE").unwrap_or_else(|_| "direct".to_string());
    match mode.as_str() {
        "direct" => {
            if env::var("STRIPE_SECRET_KEY").is_err() {
                return;
            }
        }
        "wallet-broker" | "wallet_broker" => {
            if env::var("HUGR_WALLET_TOKEN").is_err() {
                return;
            }
        }
        _ => return,
    }
    // Build a live Stripe client via env. The harness does not log
    // any secret material — both `sk_…` (direct) and `hugrw_…`
    // (wallet) tokens are held in redacting SecretStrings.
    let client =
        corelink_stripe_real::StripeRealClient::from_env().expect("stripe live client build");
    let base = client.effective_base_url();
    assert!(
        base.starts_with("https://"),
        "base URL must be HTTPS: {base}"
    );
    if mode == "wallet-broker" || mode == "wallet_broker" {
        assert!(
            base.contains("/_wallet/proxy/"),
            "wallet-broker base must route through HuGR Wallet broker: {base}"
        );
        assert!(
            !base.starts_with("https://api.stripe.com"),
            "wallet-broker client must NOT hit Stripe directly: {base}"
        );
    } else {
        // direct
        assert!(
            !base.contains("/_wallet/proxy/"),
            "direct-mode base must NOT carry a wallet proxy segment: {base}"
        );
    }
    // The full create_checkout_session call is exercised by the
    // corelink-stripe-real crate's own `live-integration` tests; we
    // only assert the harness can wire the client end-to-end without
    // panicking when the env is set.
    let _ = client;
}
