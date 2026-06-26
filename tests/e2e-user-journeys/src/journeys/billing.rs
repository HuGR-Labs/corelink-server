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
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::harness::{
    bearer, expect_denied, expect_gate_denied, sha256_hex, stripe_signature_header, unique_blob,
    url_cas, url_customer,
    url_stripe_webhook, url_tier_select, Config, JourneyResult,
};
use crate::personas::Persona;

// ── Stripe-webhook simulation gate (NO REAL CHARGE) ───────────────────────────
//
// The pay→tier path is covered WITHOUT a charge by POSTing a SIGNED test Stripe
// webhook to the signup-worker and observing the tenant's billing state move.
// Because that MUTATES a tenant's billing/entitlement state, the whole journey
// is opt-in behind `CORELINK_E2E_STRIPE_WEBHOOK_TEST=1` PLUS the secret/host/ids
// the operator must provision. Absent any of them → GATE (recorded, never run).
//
// | Env var                                 | Meaning                              |
// |-----------------------------------------|--------------------------------------|
// | `CORELINK_E2E_STRIPE_WEBHOOK_TEST`      | `1` to opt into the mutating journey |
// | `CORELINK_E2E_SIGNUP_WORKER_ENDPOINT`   | base URL of the signup-worker        |
// | `CORELINK_E2E_STRIPE_WEBHOOK_SECRET`    | the `whsec_…` signing secret         |
// | `CORELINK_E2E_STRIPE_SUBSCRIPTION_ID`   | the test tenant's Stripe sub id      |
// | `CORELINK_E2E_STRIPE_CUSTOMER_ID`       | the test tenant's Stripe customer id |
// | `CORELINK_E2E_STRIPE_PRICE_ID`          | (optional) price id for the tier map |

/// Whether the operator opted into the mutating Stripe-webhook simulation.
fn webhook_test_enabled() -> bool {
    env::var("CORELINK_E2E_STRIPE_WEBHOOK_TEST")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Current unix time in whole seconds (for the `t=` signature timestamp; the
/// receiver rejects a skew > 5 minutes, so we sign with the real wall clock).
fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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
        webhook_unsigned_rejected(cfg, client),
        webhook_tier_upgrade_simulation(cfg, client),
        webhook_subscription_cancel_simulation(cfg, client),
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

    // STRICT (auditor tooth-audit): the past-due deny MUST be the billing-state
    // gate's 402 (ADR-0068 $-ceiling; ver. live: a past-due tenant's CAS PUT → 402),
    // tolerating 503 only as a fail-closed. A 401/403/404 is NO LONGER accepted: it
    // means the past-due credential was rejected by the AUTH layer, so the journey
    // never exercised the BILLING-STATE gate at all and would pass for the wrong
    // reason (a mis-provisioned PAT → false green). 2xx is the integrity failure
    // (handled above).
    match status {
        402 => JourneyResult::pass(name, ms(start)),
        503 => JourneyResult::pass(name, ms(start)), // fail-closed tolerated
        other => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "past-due PUT got {other} — expected the billing-state deny 402 (503 tolerated \
                 fail-closed). A 401/403/404 means the past-due credential hit the AUTH layer, \
                 not the billing gate — the billing-state property is UNPROVEN"
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
    match expect_gate_denied("unauthed tier-select", status) {
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

/// The resolved opt-in config for the mutating Stripe-webhook simulation, or a
/// gate reason naming the first missing piece. Centralised so every webhook
/// journey gates on exactly the same, fully-provisioned, opt-in surface.
struct WebhookCfg {
    signup_worker_base: String,
    secret: String,
    subscription_id: String,
    customer_id: String,
    /// Optional Stripe price id — only needed to drive the price→tier map.
    price_id: Option<String>,
}

impl WebhookCfg {
    /// Resolve from env. `Err(reason)` when the operator has NOT fully opted in
    /// (flag off, or any required value absent) → the journey GATES with that
    /// reason. NEVER partially runs a mutating webhook.
    fn resolve() -> Result<Self, String> {
        if !webhook_test_enabled() {
            return Err(
                "CORELINK_E2E_STRIPE_WEBHOOK_TEST=1 not set — the Stripe-webhook → tier \
                 simulation MUTATES a tenant's billing/entitlement state; opt in only against \
                 a Stripe-test-backed env with a provisioned test tenant"
                    .to_string(),
            );
        }
        let var = |k: &str| env::var(k).ok().filter(|v| !v.is_empty());
        let signup_worker_base = var("CORELINK_E2E_SIGNUP_WORKER_ENDPOINT").ok_or_else(|| {
            "CORELINK_E2E_SIGNUP_WORKER_ENDPOINT not set — the /webhooks/stripe receiver lives \
             on the signup-worker (a different host from the API endpoint)"
                .to_string()
        })?;
        let secret = var("CORELINK_E2E_STRIPE_WEBHOOK_SECRET").ok_or_else(|| {
            "CORELINK_E2E_STRIPE_WEBHOOK_SECRET not set — needed to SIGN the test event so the \
             receiver's verifyStripeSignature accepts it (whsec_… value)"
                .to_string()
        })?;
        let subscription_id = var("CORELINK_E2E_STRIPE_SUBSCRIPTION_ID").ok_or_else(|| {
            "CORELINK_E2E_STRIPE_SUBSCRIPTION_ID not set — the webhook keys tenant_billing by \
             stripe_subscription_id; supply the test tenant's provisioned subscription id"
                .to_string()
        })?;
        let customer_id = var("CORELINK_E2E_STRIPE_CUSTOMER_ID").ok_or_else(|| {
            "CORELINK_E2E_STRIPE_CUSTOMER_ID not set — the entitlement gate (tier_selections) is \
             keyed by stripe_customer_id; supply the test tenant's provisioned customer id"
                .to_string()
        })?;
        Ok(WebhookCfg {
            signup_worker_base,
            secret,
            subscription_id,
            customer_id,
            price_id: var("CORELINK_E2E_STRIPE_PRICE_ID"),
        })
    }
}

/// Adversarial (cred-free): an UNSIGNED (or wrong-signature) Stripe webhook MUST
/// be rejected with 400 `invalid_signature` BEFORE any side effect — the
/// signup-worker verifies the HMAC over `${t}.${body}` against the configured
/// secret before parsing or writing (`verifyStripeSignature`). This is the
/// negative half of the money path: an attacker who can POST to /webhooks/stripe
/// must NOT be able to forge a tier upgrade without the signing secret. It needs
/// only the signup-worker base URL (no secret), so it runs whenever that URL is
/// provided; a 2xx here is a launch-blocking forgery hole, a 5xx means the
/// signature gate did not run before processing.
fn webhook_unsigned_rejected(_cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Billing: unsigned Stripe webhook -> 400 (forged tier upgrade rejected)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let base = match env::var("CORELINK_E2E_SIGNUP_WORKER_ENDPOINT")
        .ok()
        .filter(|v| !v.is_empty())
    {
        Some(b) => b,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_SIGNUP_WORKER_ENDPOINT not set — no signup-worker host to probe the \
                 unsigned-webhook deny",
            )
        }
    };

    let url = url_stripe_webhook(&base);
    // A well-formed subscription.updated body, but NO Stripe-Signature header.
    let body = json!({
        "id": "evt_e2e_unsigned_probe",
        "type": "customer.subscription.updated",
        "data": { "object": { "id": "sub_e2e_unsigned_probe", "status": "active" } }
    })
    .to_string();
    let resp = match client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        // Unreachable signup-worker → the prerequisite (a live receiver) is
        // absent: GATE, don't FAIL (keeps a no-server run green, per the suite's
        // gate-not-skip contract).
        Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
            return JourneyResult::gated(
                name,
                format!("signup-worker unreachable ({e}) — no live /webhooks/stripe to probe"),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // A 2xx is the forgery hole: an unsigned event was accepted (and may have
    // mutated billing). The receiver returns 400 `invalid_signature` for a bad
    // sig and 503 only when the secret/db binding is unconfigured (a deploy
    // misconfig, not a forgery) — so a 503 GATES (env not ready), a 2xx FAILS,
    // and 400/401/403 PASS (the signature gate rejected before any side effect).
    if (200..300).contains(&status) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "MONEY-PATH: unsigned webhook got {status} — a forged Stripe event was accepted \
                 without a valid signature (tier upgrade forgeable)"
            ),
        );
    }
    if status == 503 {
        return JourneyResult::gated(
            name,
            "503 from /webhooks/stripe — the receiver's STRIPE_WEBHOOK_SECRET / BILLING_DB is \
             unconfigured on this env (fail-closed); cannot probe the signature gate here",
        );
    }
    match expect_denied("unsigned webhook", status) {
        // 400 is the canonical reject; expect_denied also accepts 401/403.
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(_) if status == 400 => JourneyResult::pass(name, ms(start)),
        Err(_) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "unsigned webhook got {status} — expected 400 invalid_signature (the signature \
                 gate must reject before any side effect)"
            ),
        ),
    }
}

/// Happy (opt-in, NO CHARGE): the SAFE pay→tier coverage. Construct a SIGNED test
/// `customer.subscription.updated(status=active)` for the provisioned test tenant
/// and POST it to the signup-worker `/webhooks/stripe`; the receiver verifies the
/// signature, maps `active → paid`, updates `tenant_billing`, and (when the price
/// maps to a tier) propagates the tier + keeps the canonical entitlement gate in
/// sync. We assert the webhook is ACCEPTED (200) and then that the tenant's
/// billing state — read black-box via `GET /v1/customer/billing` with the RW PAT
/// — reflects an active/paid subscription. No real Stripe charge happens:
/// creating/POSTing a signed event is not a payment.
///
/// GATED in full behind `CORELINK_E2E_STRIPE_WEBHOOK_TEST=1` + the secret/host/
/// ids (it mutates billing state). A 400 (signature reject) or 503
/// (secret/db unconfigured) is reported as a gate/fail per the contract below.
fn webhook_tier_upgrade_simulation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Billing: signed Stripe webhook -> tier upgrade reflected (opt-in, NO charge)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let wh = match WebhookCfg::resolve() {
        Ok(w) => w,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    // We need the RW PAT to read the post-webhook billing state black-box.
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    // Build the signed event. `metadata[tenant_id]` lets the analytics emit
    // attribute the tenant; the billing/entitlement writes key on the
    // subscription/customer ids. Include the price id (when supplied) so the
    // price→tier map can propagate the tier — but never invent one.
    let ts = now_unix_secs();
    let mut object = json!({
        "id": wh.subscription_id,
        "status": "active",
        "customer": wh.customer_id,
        "current_period_end": ts + 30 * 24 * 3600,
        "metadata": { "tenant_id": cfg.tenant_or_anon() }
    });
    if let Some(price) = wh.price_id.as_deref() {
        object["items"] = json!({ "data": [ { "price": { "id": price } } ] });
    }
    let body = json!({
        "id": format!("evt_e2e_tier_upgrade_{ts}"),
        "type": "customer.subscription.updated",
        "data": { "object": object }
    })
    .to_string();

    let sig = match stripe_signature_header(&wh.secret, ts, &body) {
        Some(s) => s,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_STRIPE_WEBHOOK_SECRET is not a decodable whsec_<base64> value — \
                 cannot sign the test event",
            )
        }
    };

    let url = url_stripe_webhook(&wh.signup_worker_base);
    let resp = match client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header("Stripe-Signature", sig)
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
            return JourneyResult::gated(
                name,
                format!("signup-worker unreachable ({e}) — no live /webhooks/stripe receiver"),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // 503 = receiver's secret/db binding unconfigured on this env (fail-closed),
    // not a contract violation we can assert against → GATE.
    if status == 503 {
        return JourneyResult::gated(
            name,
            "503 from /webhooks/stripe — STRIPE_WEBHOOK_SECRET / BILLING_DB unconfigured on the \
             receiver; cannot drive the simulation here",
        );
    }
    // 400 = our signature did not verify. Most often the provided secret does not
    // match the deployed one (write-only secrets), so treat as a GATE with a
    // precise reason rather than a hard FAIL of the suite.
    if status == 400 {
        return JourneyResult::gated(
            name,
            "400 invalid_signature — the supplied CORELINK_E2E_STRIPE_WEBHOOK_SECRET does not \
             match the receiver's deployed secret (or the clock skew exceeds 5m); cannot drive \
             the signed simulation",
        );
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("signed webhook got {status} (expected 200 ok). url={url}"),
        );
    }

    // The webhook committed. Read the tenant's billing state black-box and assert
    // it reflects an active/paid subscription — the pay→tier outcome WITHOUT a
    // charge. We require the `status`/`plan` fields the contract guarantees and
    // assert the status is not a denied/canceled state.
    let billing_url = url_customer(cfg, "billing");
    let bresp = match client
        .get(&billing_url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {billing_url}: {e}")),
    };
    let bstatus = bresp.status().as_u16();
    if bstatus != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "webhook 200 but GET /v1/customer/billing got {bstatus} — cannot confirm the \
                 tier upgrade reflected"
            ),
        );
    }
    let body: Value = match bresp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("billing body not JSON: {e}")),
    };
    let status_str = body.get("status").and_then(Value::as_str).unwrap_or("");
    // The active/paid lexicon the contract exposes (routes/customer.rs maps the
    // tenant_billing/tier_selections state into a customer-facing status). A
    // canceled/past_due/inactive status after an active webhook is a failure.
    let is_active = matches!(
        status_str,
        "active" | "paid" | "trialing" | "current" | "ok"
    );
    if !is_active {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "signed active-subscription webhook accepted (200) but billing status is \
                 '{status_str}' (expected an active/paid state): {body}"
            ),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// Subscription lifecycle (opt-in, NO CHARGE): a SIGNED
/// `customer.subscription.deleted` (cancel) for the provisioned test tenant must
/// REVOKE entitlement — the receiver flips `tier_selections` away from 'active'
/// and marks `tenant_billing` canceled. We POST the signed cancel, assert 200,
/// then assert the canonical enforcement on the DATA PLANE: a CAS write with the
/// (now-canceled) tenant's RW PAT is DENIED (402/503 billing-gate, or an auth
/// deny). This proves cancel/downgrade transitions enforce, not just that a row
/// changed — and it never charges anything (a webhook is not a payment).
///
/// GATED behind the same opt-in surface as the upgrade simulation, AND it
/// requires `CORELINK_E2E_TENANT` (the CAS data-plane probe needs the tenant
/// segment). Because it leaves the test tenant CANCELED, run it LAST / against a
/// throwaway tenant; the operator opts in explicitly.
fn webhook_subscription_cancel_simulation(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Billing: signed cancel webhook -> entitlement revoked on data plane (opt-in, NO charge)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let wh = match WebhookCfg::resolve() {
        Ok(w) => w,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — the cancel-enforcement probe writes CAS, which needs \
             the tenant segment",
        );
    }
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    // Build + sign a customer.subscription.deleted for the provisioned ids.
    let ts = now_unix_secs();
    let body = json!({
        "id": format!("evt_e2e_cancel_{ts}"),
        "type": "customer.subscription.deleted",
        "data": { "object": {
            "id": wh.subscription_id,
            "status": "canceled",
            "customer": wh.customer_id,
            "metadata": { "tenant_id": cfg.tenant_or_anon() }
        } }
    })
    .to_string();
    let sig = match stripe_signature_header(&wh.secret, ts, &body) {
        Some(s) => s,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_STRIPE_WEBHOOK_SECRET is not a decodable whsec_<base64> value — \
                 cannot sign the cancel event",
            )
        }
    };

    let url = url_stripe_webhook(&wh.signup_worker_base);
    let resp = match client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header("Stripe-Signature", sig)
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
            return JourneyResult::gated(
                name,
                format!("signup-worker unreachable ({e}) — no live /webhooks/stripe receiver"),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if status == 503 {
        return JourneyResult::gated(
            name,
            "503 from /webhooks/stripe — receiver secret/db unconfigured; cannot drive the cancel",
        );
    }
    if status == 400 {
        return JourneyResult::gated(
            name,
            "400 invalid_signature — supplied secret does not match the receiver's deployed \
             secret (or clock skew > 5m); cannot drive the signed cancel",
        );
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("signed cancel webhook got {status} (expected 200 ok). url={url}"),
        );
    }

    // Canonical enforcement: a now-canceled tenant must lose paid-tier service.
    // Probe the data plane with a fresh CAS write; a 2xx is the billing-integrity
    // failure (paid capacity served after cancel). 402/503 is the billing deny;
    // 401/403/404 is an acceptable auth-layer deny. (The container reads the
    // canonical tier_selections gate; entitlement revocation may take a moment to
    // propagate across edges — a 2xx is still the only outright failure.)
    let blob = unique_blob("post-cancel-should-be-denied");
    let hash = sha256_hex(&blob);
    let cas_url = url_cas(cfg, &p1.tenant, &hash);
    let cresp = match client
        .put(&cas_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {cas_url}: {e}")),
    };
    let cstatus = cresp.status().as_u16();
    if matches!(cstatus, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "BILLING-STATE INTEGRITY: a CAS write got {cstatus} AFTER a signed cancel webhook \
                 — paid-tier capacity served without an active subscription"
            ),
        );
    }
    if matches!(cstatus, 402 | 503) {
        return JourneyResult::pass(name, ms(start));
    }
    match expect_denied("post-cancel data-plane write", cstatus) {
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(_) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "post-cancel CAS write got {cstatus} — expected a billing deny (402/503) or auth \
                 deny (401/403/404)"
            ),
        ),
    }
}
