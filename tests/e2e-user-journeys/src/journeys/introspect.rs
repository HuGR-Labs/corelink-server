//! PAT-introspection journeys — `POST /internal/v1/auth/introspect`.
//!
//! Surface S16. This is a CONTAINER-INTERNAL endpoint gated by the
//! `X-Corelink-Internal-Auth` header carrying a DEDICATED service secret
//! (`FABRIC_INTROSPECT_AUTH_KEY`) — NOT a customer PAT and NOT the
//! Worker↔container key. It is therefore NOT a customer surface: a black-box
//! test can only exercise the happy/edge paths when the operator supplies the
//! service secret out-of-band via `CORELINK_E2E_INTROSPECT_KEY`.
//!
//! Grounded in `crates/corelink-container/src/routes/auth_introspect.rs`:
//!   - Request body: `{ "token": "<pat>" }`.
//!   - Valid PAT + valid service key → 200 `{ valid:true, tenant_id, plan }`.
//!   - Invalid PAT + valid service key → 200 `{ valid:false }` (uniform, NO
//!     tenant_id, NO reason — no oracle on *why*).
//!   - Missing / wrong service key → 401 (body `{ "error":"unauthorized" }`),
//!     gate runs BEFORE the body is even parsed.
//!   - If the route is not mounted in the deploy (secret absent server-side) a
//!     404 is also an acceptable deny for the adversarial probe.
//!
//! Gating policy:
//!   - happy + edge GATE when `CORELINK_E2E_INTROSPECT_KEY` is absent (it is an
//!     internal secret, not a customer credential — never fail for not having
//!     it). happy additionally needs a real RW PAT (P1) and the tenant id to
//!     assert the echoed `tenant_id`.
//!   - adversarial is the deny-probe for the MISSING/WRONG key, so it does NOT
//!     require the real key — it runs whenever the endpoint is reachable and
//!     asserts the deny (401/403/404). A 200 on this probe is a BUG.

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::harness::{expect_gate_denied, expect_status, url_introspect, Config, JourneyResult};
use crate::personas::Persona;

/// Env var carrying the DEDICATED introspection service secret
/// (`FABRIC_INTROSPECT_AUTH_KEY`'s value), supplied out-of-band by the operator.
const INTROSPECT_KEY_ENV: &str = "CORELINK_E2E_INTROSPECT_KEY";

/// The service-auth header the introspect route gates on.
const INTERNAL_AUTH_HEADER: &str = "X-Corelink-Internal-Auth";

/// Run the introspection journeys: happy, edge, adversarial.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        happy_valid_pat(cfg, client),
        edge_invalid_pat(cfg, client),
        adversarial_no_service_key(cfg, client),
    ]
}

/// Read the operator-supplied introspection service secret, if present.
fn introspect_key() -> Option<String> {
    env::var(INTROSPECT_KEY_ENV).ok().filter(|v| !v.is_empty())
}

/// HAPPY — valid PAT + valid service key → 200 `{valid:true, tenant_id, plan}`,
/// and `tenant_id` echoes the caller's own tenant.
fn happy_valid_pat(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Introspect: valid PAT + service key → 200 {valid:true,tenant_id,plan}";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Gate: the internal service secret (not a customer credential).
    let key = match introspect_key() {
        Some(k) => k,
        None => {
            return JourneyResult::gated(
                name,
                format!(
                    "{INTROSPECT_KEY_ENV} not set — introspect is an internal \
                     (X-Corelink-Internal-Auth) endpoint, not a customer surface"
                ),
            )
        }
    };

    // Gate: a real RW PAT (the token we introspect) and a known tenant id to
    // assert the echoed tenant_id against.
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");
    let expected_tenant = match cfg.tenant.as_deref() {
        Some(t) => t,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_TENANT not set — cannot assert the echoed tenant_id",
            )
        }
    };

    let resp = match client
        .post(url_introspect(cfg))
        .header(INTERNAL_AUTH_HEADER, &key)
        .json(&json!({ "token": token }))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("introspect endpoint unreachable ({}): {e}", cfg.endpoint),
            )
        }
    };

    let code = resp.status().as_u16();
    if let Err(m) = expect_status("introspect (valid PAT + key)", code, 200) {
        // A 401 here means the supplied service key is wrong/expired, not a
        // contract break we can attribute — surface it but as a fail since the
        // operator asserted the key is valid by setting it.
        return JourneyResult::fail(name, ms(start), m);
    }

    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("introspect not JSON: {e}")),
    };

    if body["valid"].as_bool() != Some(true) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("expected valid:true for a live RW PAT, got: {body}"),
        );
    }
    match body["tenant_id"].as_str() {
        Some(t) if t == expected_tenant => {}
        Some(t) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("tenant_id mismatch: introspect returned {t}, expected {expected_tenant}"),
            )
        }
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "valid:true response missing the required tenant_id field".to_string(),
            )
        }
    }
    if body["plan"].as_str().is_none() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("valid:true response missing the required plan field: {body}"),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// EDGE — a well-formed-but-bogus PAT + valid service key → 200 `{valid:false}`
/// with NO tenant_id (uniform invalid; no oracle on why).
fn edge_invalid_pat(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Introspect: bogus PAT + service key → 200 {valid:false} (no tenant_id)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let key = match introspect_key() {
        Some(k) => k,
        None => {
            return JourneyResult::gated(
                name,
                format!("{INTROSPECT_KEY_ENV} not set — internal endpoint, not a customer surface"),
            )
        }
    };

    // A syntactically plausible but non-existent PAT — verification must fail.
    let bogus = "corelink_pat_0000000000000000000000000000000000000000000000000000000000000001";

    let resp = match client
        .post(url_introspect(cfg))
        .header(INTERNAL_AUTH_HEADER, &key)
        .json(&json!({ "token": bogus }))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("introspect endpoint unreachable ({}): {e}", cfg.endpoint),
            )
        }
    };

    let code = resp.status().as_u16();
    if let Err(m) = expect_status("introspect (bogus PAT + key)", code, 200) {
        return JourneyResult::fail(name, ms(start), m);
    }

    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("introspect not JSON: {e}")),
    };

    if body["valid"].as_bool() != Some(false) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("expected valid:false for a bogus PAT, got: {body}"),
        );
    }
    // Uniform-invalid contract: no tenant_id, no plan leaked.
    if !body["tenant_id"].is_null() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("valid:false leaked a tenant_id (oracle): {body}"),
        );
    }
    if !body["plan"].is_null() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("valid:false leaked a plan (oracle): {body}"),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// ADVERSARIAL — introspect without / with the WRONG service key must be denied
/// (401/403), or 404 if the route is not mounted in this deploy. A 200 here is a
/// security failure: the internal endpoint would be reachable from the edge.
///
/// This probe does NOT need the real service key (it tests its absence), so it
/// runs whenever the endpoint is reachable — gating only on connectivity.
fn adversarial_no_service_key(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Introspect: missing/wrong service key → denied (401/403/404), never 200";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // We still send a plausible body so the ONLY thing that can let us through
    // is a missing auth gate — not a malformed-body rejection.
    let probe_body = json!({ "token": "corelink_pat_probe" });

    // 3a — NO service-auth header at all.
    match client.post(url_introspect(cfg)).json(&probe_body).send() {
        Ok(r) => {
            let code = r.status().as_u16();
            if code == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    "SECURITY: introspect returned 200 with NO service-auth header — \
                     the internal endpoint is reachable unauthenticated from the edge"
                        .to_string(),
                );
            }
            // The introspect route is always-mounted when the secret is
            // configured. A 404 means the route disappeared, not that the
            // gate actively rejected — that would leave the security property
            // unproven. Use expect_gate_denied (401/403 only). M3.
            if let Err(m) = expect_gate_denied("introspect (no service key)", code) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => {
            // Connectivity failure → gate (no env / endpoint down), don't fail.
            return JourneyResult::gated(
                name,
                format!("introspect endpoint unreachable ({}): {e}", cfg.endpoint),
            );
        }
    }

    // 3b — WRONG service-auth header (a non-matching secret).
    let wrong_key = "wrong-service-key-0000000000000000000000000000000000";
    match client
        .post(url_introspect(cfg))
        .header(INTERNAL_AUTH_HEADER, wrong_key)
        .json(&probe_body)
        .send()
    {
        Ok(r) => {
            let code = r.status().as_u16();
            if code == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    "SECURITY: introspect returned 200 with a WRONG service-auth header — \
                     the service-secret gate is not enforced"
                        .to_string(),
                );
            }
            // Same reasoning: the gate must actively answer 401/403; a 404
            // means the route is absent/renamed and the rejection is
            // unproven. M3.
            if let Err(m) = expect_gate_denied("introspect (wrong service key)", code) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("wrong-key probe: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}
