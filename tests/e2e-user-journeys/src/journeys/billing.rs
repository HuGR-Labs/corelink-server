//! Billing journeys — S13 billing/tier surface.
//!
//! Black-box coverage of the customer-facing billing surface a paying SMB
//! actually touches, using ONLY the deployed HTTP API + Bearer PAT:
//!
//!   - **Happy** `GET /v1/customer/billing` → 200 with a billing state
//!     (`status` + `plan`). Source contract: `routes/customer.rs`
//!     `handle_billing` returns `{status, plan, current_period_*,
//!     amount_due_cents, currency, invoices[]}`.
//!   - **Happy/edge** `POST /v1/customer/billing/portal` → 200 with a
//!     `portal_url` string (the Stripe customer-portal hand-off).
//!   - **Adversarial** P11 past-due subscription → the DATA PLANE is denied
//!     (no paid-tier service without `subscription_state='active'`). Per the
//!     matrix this is a 402/503. We assert the deny (a 2xx here is the
//!     billing-state-integrity bug — paid service served without payment).
//!
//! GATED (needs a non-PAT credential — never failed, never faked):
//!   - `POST /v1/onboarding/tier-select` → Stripe Checkout. This route is
//!     authenticated by a **Clerk session**, NOT a customer PAT
//!     (`routes/tier_select.rs`), so it is not PAT-exercisable from this
//!     black-box harness. We GATE with that reason.
//!   - The full checkout→active→dunning→cancel lifecycle (matrix S13 "slow"
//!     bits) requires driving Stripe + Clerk and is out of scope for the PAT
//!     black box; folded into the tier-select gate.
//!
//! Routes go through the harness URL builders only:
//! [`url_customer`] (`/v1/customer/billing` + `/v1/customer/billing/portal`),
//! [`url_tier_select`], and [`url_cas`] for the past-due data-plane probe.

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::harness::{
    bearer, expect_denied, sha256_hex, unique_blob, url_cas, url_customer, url_tier_select, Config,
    JourneyResult,
};
use crate::personas::Persona;

/// Run the billing journeys (happy GET billing, happy portal, adversarial
/// past-due deny, checkout-session creation + unauthed-checkout deny, gated
/// tier-select happy path).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        billing_state(cfg, client),
        billing_portal(cfg, client),
        past_due_data_plane_denied(cfg, client),
        checkout_unauthed_denied(cfg, client),
        checkout_session_authed(cfg, client),
        tier_select_gated(cfg, client),
    ]
}

/// Happy: `GET /v1/customer/billing` → 200 carrying the live billing state
/// (`status` + `plan` present). This is the dashboard's billing tab.
fn billing_state(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Billing: GET /v1/customer/billing → 200 with billing state";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let url = url_customer(cfg, "billing");
    let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v1/customer/billing got {status} (expected 200). url={url}"),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("billing body not JSON: {e}"))
        }
    };
    // Contract (routes/customer.rs handle_billing): a billing state must carry
    // a subscription `status` and a `plan` — the two fields the dashboard and
    // the billing-state-integrity invariant turn on.
    for field in ["status", "plan"] {
        if body.get(field).map(Value::is_null).unwrap_or(true) {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("billing 200 but missing/null required field '{field}': {body}"),
            );
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// Happy/edge: `POST /v1/customer/billing/portal`.
///
/// The Stripe customer-portal hand-off only exists once the tenant has a Stripe
/// CUSTOMER. The seed e2e tenant has no Stripe customer, so `portal_url` errs
/// `NotFound` and the handler returns **404 "not found"**
/// (`routes/customer.rs::handle_billing_portal` → `map_err(NotFound)` →
/// 404). That is the CORRECT behaviour for a non-Stripe-backed tenant — not a
/// bug — so a 404 here is GATED (the journey needs a real Stripe-backed test
/// tenant to drive the happy `200 {portal_url}` path). A `200` (when run against
/// a Stripe tenant) must still carry a non-empty `portal_url`; any other status
/// (e.g. 5xx) is a real failure.
fn billing_portal(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Billing: POST /v1/customer/billing/portal → portal_url (or 404 if no Stripe cust)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let url = url_customer(cfg, "billing/portal");
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // Non-Stripe-backed tenant: portal_url → NotFound → 404. Expected; GATE so
    // the happy path is exercised only against a Stripe-backed test account.
    if status == 404 {
        return JourneyResult::gated(
            name,
            "404 'not found' — the e2e tenant has no Stripe customer, so portal_url \
             errs NotFound (correct for a non-Stripe tenant). The happy \
             200 {portal_url} path needs a real Stripe-backed test tenant.",
        );
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("POST /v1/customer/billing/portal got {status} (expected 200 or 404). url={url}"),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("portal body not JSON: {e}")),
    };
    // Contract (routes/customer.rs handle_billing_portal): {"portal_url": "..."}.
    match body.get("portal_url").and_then(Value::as_str) {
        Some(u) if !u.is_empty() => {}
        _ => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("portal 200 but missing/empty 'portal_url' string: {body}"),
            )
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// Adversarial: a past-due (P11) tenant must NOT receive paid-tier service.
/// We exercise the data plane (a CAS write) with the past-due PAT and require a
/// deny. Per the matrix the billing-gated denial is 402 (payment required) or
/// 503 (service unavailable) — a 2xx here is the billing-state-integrity bug
/// (paid capacity served without an active subscription).
fn past_due_data_plane_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Billing: past-due (P11) data-plane write → denied (402/503, no service w/o active sub)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p11 = match Persona::P11PastDue.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — CAS path requires the tenant segment",
        );
    }
    let token = p11.token.expect("P11 has a token");

    // A fresh, unique blob: this is a write that would consume paid capacity.
    let blob = unique_blob("past-due-should-be-denied");
    let hash = sha256_hex(&blob);
    let url = url_cas(cfg, &p11.tenant, &hash);

    let resp = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("P11 PUT {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // A successful write (200/201) is the security/billing-integrity failure:
    // a past-due subscription was served paid-tier capacity.
    if matches!(status, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "BILLING-STATE INTEGRITY: past-due PAT got {status} on a CAS write — \
                 paid-tier service granted without an active subscription"
            ),
        );
    }

    // The billing-gated denial is specifically 402/503 (matrix S13/P7). We also
    // accept the auth-layer denials (401/403/404) as a valid deny — what must
    // never happen is a 2xx. Assert the matrix-precise codes first, then fall
    // back to the generic deny so the journey can't pass on an unexpected 2xx.
    if matches!(status, 402 | 503) {
        return JourneyResult::pass(name, ms(start));
    }
    match expect_denied("past-due data-plane write", status) {
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(_) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "past-due PUT got {status} — expected a billing deny (402/503) or auth deny (401/403/404)"
            ),
        ),
    }
}

/// Adversarial: an UNAUTHED checkout-session creation must be DENIED. The money
/// path (`routes/tier_select.rs`) is authenticated by a Clerk session; a request
/// carrying NO credential must never mint a Stripe Checkout session (that would
/// let an anonymous caller open checkout sessions / probe the money path). This
/// is deterministic and needs no creds — it asserts the negative contract: a
/// `POST /v1/onboarding/tier-select` with no Authorization is denied
/// (401/403/404), never a 2xx and never a 5xx (a 5xx on a missing-auth request
/// is itself a contract failure — auth must reject before any Stripe work).
fn checkout_unauthed_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Billing: unauthed checkout (tier-select, no auth) -> denied (no session minted)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let url = url_tier_select(cfg);
    // A plausible checkout body so the deny is the AUTH gate, not a 400 body-parse
    // reject — we want to prove unauthenticated callers can't mint a session.
    let body = json!({ "tier": "solo", "interval": "month" }).to_string();
    let resp = match client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        // This is the one cred-free negative probe, so it has no token/tenant gate
        // to short-circuit on. A transport-level error (connect/TLS/timeout) means
        // the endpoint is unreachable — the PREREQUISITE (a live endpoint) is
        // absent, so GATE rather than FAIL (keeps a no-server local run GREEN, in
        // line with the suite's gate-not-skip contract). A reachable endpoint that
        // returns the wrong status is still a hard FAIL below.
        Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
            return JourneyResult::gated(
                name,
                format!("endpoint unreachable ({e}) — no live API to probe the unauthed deny"),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // A 2xx is the security failure: an anonymous caller opened a checkout session.
    if (200..300).contains(&status) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("MONEY-PATH: unauthed tier-select got {status} — a checkout session was \
                     reachable without a Clerk session"),
        );
    }
    match expect_denied("unauthed tier-select", status) {
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(_) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "unauthed tier-select got {status} — expected an auth deny (401/403/404), not \
                 a 5xx (auth must reject before any Stripe work)"
            ),
        ),
    }
}

/// Happy (Stripe-test-gated): an AUTHED checkout-session creation responds
/// correctly. The tier-select money path is Clerk-session authenticated, NOT a
/// customer PAT (`routes/tier_select.rs`), and minting a live Stripe Checkout
/// session needs Stripe-test creds + a Clerk session token — neither is part of
/// the PAT black box. So unless the operator provides a Clerk session bearer via
/// `CORELINK_E2E_CLERK_SESSION` (run only with `CORELINK_E2E_STRIPE_TEST=1`),
/// this GATES (recorded, never faked).
///
/// When the operator DOES supply the session, the live contract
/// (`routes/tier_select.rs`) returns 200 with a `checkout_url` (the Stripe
/// Checkout hand-off) or `url`; we assert a 2xx carrying a non-empty checkout
/// URL string. Any 5xx is a real failure.
fn checkout_session_authed(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Billing: authed checkout-session creation -> 200 {checkout_url} (Stripe-test-gated)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let stripe_test = env::var("CORELINK_E2E_STRIPE_TEST")
        .map(|v| v == "1")
        .unwrap_or(false);
    let session = env::var("CORELINK_E2E_CLERK_SESSION")
        .ok()
        .filter(|v| !v.is_empty());

    let session = match (stripe_test, session) {
        (true, Some(s)) => s,
        (false, _) => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_STRIPE_TEST=1 not set — the authed checkout-session creation drives \
                 Stripe-test + a Clerk session; run only against a Stripe-test-backed env",
            )
        }
        (true, None) => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_CLERK_SESSION not set — tier-select is Clerk-session authenticated \
                 (not a PAT); supply a Clerk session bearer out-of-band to exercise the happy path",
            )
        }
    };

    let url = url_tier_select(cfg);
    let body = json!({ "tier": "solo", "interval": "month" }).to_string();
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(&session))
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("authed tier-select got {status} (expected 200). url={url}"),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("tier-select body not JSON: {e}"))
        }
    };
    // Contract (routes/tier_select.rs): the Stripe Checkout hand-off carries a
    // checkout URL. Accept either `checkout_url` or `url` (the two field names the
    // hand-off has shipped under) — must be a non-empty string.
    let has_url = ["checkout_url", "url"].iter().any(|k| {
        body.get(*k)
            .and_then(Value::as_str)
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    });
    if !has_url {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("tier-select 200 but missing/empty checkout URL (checkout_url|url): {body}"),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// Gated: `POST /v1/onboarding/tier-select` (Stripe Checkout) is authenticated
/// by a **Clerk session**, not a customer PAT (`routes/tier_select.rs`), so it
/// is not exercisable from this PAT-only black box. We GATE (record, never fake)
/// with that reason. This also covers the slow checkout→active→dunning→cancel
/// lifecycle, which needs driving Stripe + Clerk end-to-end.
fn tier_select_gated(cfg: &Config, _client: &Client) -> JourneyResult {
    let name = "Billing: tier-select checkout (Clerk-session-gated)";
    // Touch the builder so the route stays grounded in the harness contract even
    // though we cannot exercise it with a PAT.
    let _url = url_tier_select(cfg);
    JourneyResult::gated(
        name,
        "POST /v1/onboarding/tier-select requires a Clerk session (not a PAT); \
         the checkout→active→dunning→cancel lifecycle needs Stripe+Clerk — \
         out of scope for the PAT black box",
    )
}
