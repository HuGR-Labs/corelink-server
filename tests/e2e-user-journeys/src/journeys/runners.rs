//! # Runners entitlement journeys (gap-map M8 — owner WP **W2**).
//!
//! ## What "Runners enforcement" actually is in this codebase (captured from source)
//!
//! The Runners concurrency/vCPU-h entitlement is a **SEPARATE axis from the
//! cache tier**, keyed on `tenant_id` in the `runners_entitlement` D1 table
//! (migrations 0070 + 0072). A Runners-tier Stripe purchase seeds a row
//! (`crates/corelink-billing-stripe-materializer/src/runners.rs` +
//! `handler.rs::reconcile_runners`; price→cap map in
//! `crates/corelink-container/src/main.rs::RUNNER_PRICE_ENV_TABLE`, e.g.
//! `RUNNER_PRO → max_concurrency 40, max_vcpu_h 240`).
//!
//! The cap is **READ AND ENFORCED OFF the CoreLink API**, by the separate
//! **corelink-runners fabric**, via exactly ONE endpoint:
//!
//! ```text
//! POST /internal/v1/auth/introspect
//! X-Corelink-Internal-Auth: <FABRIC_INTROSPECT_AUTH_KEY>   (>=32 chars, DEDICATED)
//! { "token": "corelink_pat_..." }
//!   → 200 { valid, tenant_id, plan, max_concurrency?, max_vcpu_h? }
//! ```
//! (`crates/corelink-container/src/routes/auth_introspect.rs`). The fabric calls
//! this per job, reads `max_concurrency` / `max_vcpu_h`, and ADMITS-under-cap /
//! REJECTS-over-cap **inside the fabric** (its `CoreLinkPlanStore`). An ABSENT
//! `max_concurrency` ⇒ the fabric rejects the placement (no entitlement = no
//! cap = reject); an absent `max_vcpu_h` ⇒ wall-off (no monthly cap).
//!
//! ## The honest black-box boundary (the finding the lead needs)
//!
//! The ONLY place the cap value is observable, and the ONLY place admit/over-cap
//! is decided, is that `/internal/v1/auth/introspect` route — which is
//! **internal-auth gated** (`FABRIC_INTROSPECT_AUTH_KEY`, NOT the customer PAT;
//! route not even mounted if the secret is < 32 chars) and **not reachable from
//! the public edge**. A black-box CUSTOMER (PAT bearer, no internal keys) CANNOT:
//!   - read its own `max_concurrency`/`max_vcpu_h` (no customer endpoint surfaces
//!     it — checked `routes/customer.rs`, `routes/users.rs`), nor
//!   - drive a runner admit/lease (there is NO customer-facing runner-admit /
//!     lease / placement endpoint on the CoreLink API; the admit decision lives
//!     in the corelink-runners fabric, a separate service).
//!
//! So the admit→over-cap→reject boundary is **NOT black-box-testable from the
//! CoreLink API**; it is enforced internal-only in the fabric. What a black-box
//! customer CAN prove — and what these journeys assert — is the *negative
//! security property*: the introspect gate (the cap's source of truth) ACTIVELY
//! REJECTS a customer-presented PAT (401/403), so the cap cannot be read or
//! forged from the public edge. Journeys 1 + 2 therefore GATE with the precise
//! enforcement path; journey 3 ASSERTS the gate-deny.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, expect_gate_denied, url_introspect, Config, JourneyResult, TokenKind,
};

/// Run the Runners entitlement journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        entitlement_value_reflected(cfg, client),
        admit_under_cap_reject_over_cap(cfg, client),
        introspect_gate_denies_customer_pat(cfg, client),
    ]
}

/// **M8.1 — a Runners-entitled tenant's cap value is reflected.**
///
/// Goal: assert the ACTUAL cap value (`max_concurrency`/`max_vcpu_h`) for the
/// Runners-entitled tenant. The cap is exposed by exactly one surface — the
/// internal-auth-gated `/internal/v1/auth/introspect` (the fabric's source of
/// truth) — and NO customer-facing endpoint surfaces it. With only a customer
/// PAT (no `FABRIC_INTROSPECT_AUTH_KEY`), we cannot read the value, so we GATE
/// — but FIRST we prove there is nothing to read on the public edge: a
/// customer-bearer introspect attempt must be actively rejected (it must NOT
/// leak the cap). That keeps this an honest, evidence-bearing gate rather than
/// a bare TODO.
fn entitlement_value_reflected(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Runners: entitled tenant's cap value (max_concurrency/max_vcpu_h) is reflected — M8";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let Some(token) = cfg.token(TokenKind::Runner) else {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_PAT_RUNNER not set — no Runners-entitled PAT to exercise",
        );
    };
    if cfg.runner_tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_RUNNER_TENANT not set — cannot identify the entitled tenant",
        );
    }

    // The cap is observable ONLY via POST /internal/v1/auth/introspect, which is
    // gated by X-Corelink-Internal-Auth (FABRIC_INTROSPECT_AUTH_KEY) — a secret a
    // black-box customer does NOT hold. Prove the value does not leak to a plain
    // PAT bearer: the gate must actively reject (401/403), never 200-with-the-cap.
    let url = url_introspect(cfg);
    let body = format!(r#"{{"token":"{token}"}}"#);
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token)) // customer PAT bearer — NOT the fabric secret
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // A 200 here would mean the cap leaked to a customer-presented credential —
    // a real finding. Fail loudly on that; otherwise this confirms the cap's only
    // surface is gated, and we GATE on the missing fabric secret.
    if status == 200 {
        let txt = resp.text().unwrap_or_default();
        if txt.contains("max_concurrency") || txt.contains("\"valid\":true") {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "SECURITY: /internal/v1/auth/introspect returned 200 to a customer PAT bearer \
                     (no FABRIC_INTROSPECT_AUTH_KEY) and exposed the entitlement: {txt}"
                ),
            );
        }
    }
    if let Err(m) = expect_gate_denied("introspect (customer PAT, no fabric key)", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::gated(
        name,
        "cap value (max_concurrency/max_vcpu_h) is exposed ONLY via POST \
         /internal/v1/auth/introspect, gated by X-Corelink-Internal-Auth \
         (FABRIC_INTROSPECT_AUTH_KEY) — a black-box customer PAT cannot read it, and NO \
         customer-facing endpoint (routes/customer.rs, users.rs) surfaces it. \
         Confirmed the gate actively denied the customer-bearer attempt (no cap leak); \
         the value itself is fabric-internal and not black-box-assertable.",
    )
}

/// **M8.2 — admit-under-cap → reject-over-cap boundary.**
///
/// This is THE revenue-leak guard: a Runners-entitled tenant must be ADMITTED
/// under its concurrency cap and a placement OVER the cap must be REJECTED (not
/// silently allowed). HONEST FINDING for the lead: the admit/reject decision is
/// made by the **corelink-runners fabric** (a separate service), which reads the
/// cap from `POST /internal/v1/auth/introspect` and enforces it in its
/// `CoreLinkPlanStore`. The CoreLink API exposes **NO** customer-reachable
/// runner-admit / lease / placement endpoint (verified across
/// `crates/corelink-container/src/routes/*` + `worker/src/index.ts`: the only
/// runner paths are `/internal/v1/runner/{mint,revoke}`, `/internal/v1/billing/usage`,
/// and `/internal/v1/auth/introspect` — all internal-auth gated). A black-box
/// PAT therefore cannot drive admission, so the boundary is NOT black-box-
/// testable from this API; it must be tested in the corelink-runners repo
/// against the fabric. We GATE, naming exactly which endpoint enforces it.
fn admit_under_cap_reject_over_cap(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Runners: admit-under-cap → reject-over-cap boundary (revenue-leak guard) — M8";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Even if a Runner PAT is present, there is no customer endpoint to drive
    // admission against; surface the precise enforcement path regardless.
    let _ = (client, ms(start));
    if cfg.token(TokenKind::Runner).is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_PAT_RUNNER not set; and INDEPENDENTLY: the admit/over-cap decision is \
             enforced in the corelink-runners fabric (its CoreLinkPlanStore reads the cap from \
             POST /internal/v1/auth/introspect) — the CoreLink API exposes NO customer-reachable \
             runner-admit/lease endpoint, so a black-box PAT cannot drive admission.",
        );
    }

    JourneyResult::gated(
        name,
        "admit-under-cap / reject-over-cap is enforced by the corelink-runners FABRIC (separate \
         service): the fabric calls POST /internal/v1/auth/introspect (internal-auth gated, \
         FABRIC_INTROSPECT_AUTH_KEY), reads max_concurrency/max_vcpu_h, and admits/rejects in its \
         CoreLinkPlanStore (absent max_concurrency ⇒ reject; over-cap ⇒ reject; absent max_vcpu_h \
         ⇒ wall-off). The CoreLink API has NO customer-facing runner-admit/lease/placement route \
         (only /internal/v1/runner/{mint,revoke} + /internal/v1/billing/usage + the introspect \
         endpoint, all internal-auth gated). => NOT black-box-testable from this API; the boundary \
         must be exercised in the corelink-runners repo against the fabric+introspect seam.",
    )
}

/// **M8.3 — a tenant with NO Runners entitlement is denied the runner surface.**
///
/// The only customer-reachable touchpoint of the runner entitlement system is
/// the introspect endpoint (the cap source of truth). A no-entitlement tenant's
/// PAT presented as a customer bearer (no fabric key) must be ACTIVELY rejected
/// (401/403) — proving the runner surface is gated shut against the public edge,
/// not merely absent. We use the primary read-write PAT (the cache-only tenant
/// holds no `runners_entitlement` row) as the "no entitlement" actor.
///
/// `expect_gate_denied` (401/403 only): a 404 would be a FAILURE — it would mean
/// the gate is absent/renamed rather than actively rejecting, leaving the
/// security property unproven (harness M3 rule).
fn introspect_gate_denies_customer_pat(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Runners: no-entitlement tenant denied the runner surface (introspect gate) — M8";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Prefer the dedicated read-write PAT (cache-only tenant = no runners_entitlement
    // row); fall back to any present token. If none, gate.
    let token =
        match cfg
            .token(TokenKind::ReadWrite)
            .or_else(|| cfg.token(TokenKind::Runner))
        {
            Some(t) => t,
            None => return JourneyResult::gated(
                name,
                "neither CORELINK_E2E_PAT_RW nor CORELINK_E2E_PAT_RUNNER set — no PAT to present \
                 to the runner surface",
            ),
        };

    let url = url_introspect(cfg);
    let body = format!(r#"{{"token":"{token}"}}"#);
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token)) // customer PAT — NOT FABRIC_INTROSPECT_AUTH_KEY
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let status = resp.status().as_u16();

    // A 200 to a customer-presented credential = the runner surface is open to
    // the public edge (cap could be read/forged) — a real finding, fail loudly.
    if status == 200 {
        let txt = resp.text().unwrap_or_default();
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SECURITY: /internal/v1/auth/introspect returned 200 to a customer PAT bearer \
                 (no FABRIC_INTROSPECT_AUTH_KEY) — the runner surface is reachable from the public \
                 edge: {txt}"
            ),
        );
    }
    if let Err(m) = expect_gate_denied("introspect runner surface (customer PAT)", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}
