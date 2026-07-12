//! # Self-serve Runner PURCHASE journeys (WP5 — the checkout→seed→revoke path).
//!
//! Complements [`super::runners`] (which covers the introspect READ / enforce
//! side): this module covers the self-serve **purchase** of a Runner tier —
//! `POST /v1/onboarding/tier-select {tier:"runner_*"}` → Stripe Checkout →
//! (on payment) the signup-worker webhook seeds `runner_billing` +
//! `runners_entitlement` → (on cancel/dunning) it revokes.
//!
//! ## What the code does (captured from source, for the reviewer)
//! - The 5 runner SKUs are self-serve tiers (`TierKind::Runner*` /
//!   `RequestedTier::Runner*`, `crates/corelink-tier-selection/src/tier.rs` +
//!   `routes/tier_select.rs`); the checkout backend resolves the Stripe price
//!   via `STRIPE_PRICE_ID_{TIER}` (`corelink-stripe-real/src/client.rs`).
//! - Runner is a SEPARATE entitlement axis: `orchestrate_locked`
//!   (`routes/tier_select.rs`) SKIPS the cache `persist_pending_checkout` for a
//!   runner tier and guards the runner axis independently
//!   (`has_active_runner_subscription`) — a runner checkout never writes the
//!   one-row-per-tenant cache `tier_selections` / `stripe_checkout_sessions`.
//! - The live seed/revoke is the signup-worker Stripe webhook
//!   (`apps/signup-worker/src/webhooks/stripe.ts`): seed `runners_entitlement`
//!   (keyed via `runner_billing`, migration 0087) on a granting runner
//!   subscription, DELETE it on cancel / terminal payment-failure.
//!
//! ## The black-box boundary (what these journeys can vs cannot prove)
//! `tier-select` is **Clerk-session gated** (a logged-in tenant selecting a
//! tier), NOT a customer-PAT surface — and the seed/revoke only happen on a real
//! **Stripe subscription webhook**. So the POSITIVE chain (checkout URL, paid,
//! seeded entitlement, introspect value, cancel, revoked) is Clerk-session and
//! Stripe-test gated, and is asserted as a documented `Gated` frontier (same
//! honest posture as `super::runners`). What a black-box customer CAN prove —
//! and what journey 1 asserts — is the *negative security property*: the runner
//! checkout surface ACTIVELY rejects an unauthenticated caller (401/403), so a
//! runner subscription cannot be driven from the public edge without a session.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;

use crate::harness::{expect_gate_denied, url_tier_select, Config, JourneyResult};

/// Elapsed-ms helper (per-module local, matching the sibling journey modules).
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// Run the self-serve Runner purchase journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        checkout_surface_rejects_unauthenticated(cfg, client),
        purchase_seed_enforce_revoke_chain(cfg),
    ]
}

/// **WP5.1 — the runner checkout surface actively rejects an unauthenticated
/// caller.**
///
/// `POST /v1/onboarding/tier-select {tier:"runner_pro"}` with NO auth MUST be
/// gate-denied (401/403). This proves the self-serve runner purchase cannot be
/// driven from the public edge without a Clerk session — the auth gate runs
/// before any tier/body handling. A 404 would mean the route is
/// absent/renamed (security property UNPROVEN), which `expect_gate_denied`
/// rejects.
fn checkout_surface_rejects_unauthenticated(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "RunnerPurchase: tier-select runner checkout rejects unauthenticated (401/403)";
    let start = Instant::now();

    // A well-formed runner-checkout body, but NO Authorization / session — the
    // gate must reject on auth BEFORE it ever looks at the tier.
    let body = serde_json::json!({
        "tier": "runner_pro",
        "success_url": "https://app.example.com/ok",
        "cancel_url": "https://app.example.com/cancel",
    });

    let resp = match client
        .post(url_tier_select(cfg))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("tier-select unreachable (network): {e}"))
        }
    };

    match expect_gate_denied(
        "POST /v1/onboarding/tier-select (no auth)",
        resp.status().as_u16(),
    ) {
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(m) => JourneyResult::fail(name, ms(start), m),
    }
}

/// **WP5.2 — the purchase→seed→introspect→cancel→revoke chain (documented
/// frontier).**
///
/// The positive self-serve chain is not black-box-testable from this API: the
/// checkout is Clerk-session gated (needs a logged-in, DPA-accepted tenant to
/// mint the Stripe session), the seed/revoke fire only on a real Stripe
/// subscription webhook (Stripe-test gated), and the enforced cap is read
/// internal-only by the runners fabric (`super::runners` documents that
/// introspect boundary). GATE with the precise path so this un-gates the day the
/// Clerk-session + Stripe-test harness lands — not a fake pass.
fn purchase_seed_enforce_revoke_chain(_cfg: &Config) -> JourneyResult {
    let name = "RunnerPurchase: buy→seed→introspect→cancel→revoke (full self-serve chain)";
    JourneyResult::gated(
        name,
        "Clerk-session + Stripe-test gated. The positive chain requires (1) a \
         Clerk-authed DPA-accepted tenant to POST tier-select {tier:runner_*} and \
         receive a Stripe checkout_url (routes/tier_select.rs — runner skips the \
         cache persist), (2) a real Stripe runner-subscription webhook so the \
         signup-worker seeds runner_billing + runners_entitlement (0087/0070) and \
         revokes on cancel, and (3) the fabric-internal introspect read of the \
         seeded cap (see runners:: journeys). None are reachable from a black-box \
         customer PAT. Un-gates with the Clerk-session + Stripe-test harness.",
    )
}
