//! Identity journeys — onboarding/ping reachability + auth rejection.
//!
//! Migrated from the old single-file suite (journeys 1 + 2), URLs corrected and
//! the stale "expected RED / P0" framing flipped to the LIVE contract (the P0
//! wave is long landed; the system is live in prod).

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use serde_json::Value;

use crate::harness::{
    bearer, expect_status, url_health_serving, url_users_me, Config, JourneyResult,
};
use crate::personas::Persona;

/// Run the identity journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![onboarding_ping(cfg, client), auth_rejection(cfg, client)]
}

/// Onboarding / ping: `/api/health` answers `SERVING`, and a valid PAT reaches
/// an authenticated endpoint (`/v1/users/me`) with a well-formed profile.
fn onboarding_ping(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Identity: onboarding/ping — health SERVING + authed endpoint 200";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Step 1 — health must be 200 + {"status":"SERVING"}.
    // A connection error means the endpoint isn't under test (e.g. no env) —
    // gate, don't fail: a black-box gate fails only on a CONTRACT violation.
    let health = match client.get(url_health_serving(cfg)).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("endpoint unreachable ({}): {e}", cfg.endpoint),
            )
        }
    };
    if let Err(m) = expect_status("GET /api/health", health.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }
    match health.json::<Value>() {
        Ok(body) if body["status"].as_str() == Some("SERVING") => {}
        Ok(body) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("/api/health status field not SERVING: {body}"),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("/api/health not JSON: {e}")),
    }

    // Step 2 — a valid PAT must reach an authenticated endpoint.
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let me = match client
        .get(url_users_me(cfg))
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET /v1/users/me: {e}")),
    };
    if let Err(m) = expect_status("GET /v1/users/me (valid PAT)", me.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let body: Value = match me.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("/v1/users/me not JSON: {e}"))
        }
    };
    // Real /v1/users/me contract (routes/users.rs): the authenticated caller's
    // identity reflection — `{tenant_id, token_prefix, route_kind}`. The key proof
    // is that a valid PAT resolved to a non-empty tenant_id (auth wired end-to-end).
    if body["tenant_id"].as_str().unwrap_or("").is_empty() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("/v1/users/me missing/empty tenant_id (auth not resolved): {body}"),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// Auth rejection: absent / malformed / well-formed-but-invalid PAT must all be
/// denied (401) on a protected endpoint.
fn auth_rejection(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Identity: auth rejection — absent/malformed/fake PAT → 401";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let me = url_users_me(cfg);

    // 2a — no Authorization header. A connection error means the endpoint
    // isn't under test (e.g. no env) — gate, don't fail.
    match client.get(&me).send() {
        Ok(r) => {
            if let Err(m) = expect_status("no-auth", r.status().as_u16(), 401) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("endpoint unreachable ({}): {e}", cfg.endpoint),
            )
        }
    }

    // 2b — malformed (too short) token.
    match client.get(&me).header(AUTHORIZATION, "Bearer tooshort123").send() {
        Ok(r) => {
            if let Err(m) = expect_status("malformed-token", r.status().as_u16(), 401) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("short-token probe: {e}")),
    }

    // 2c — well-formed but invalid PAT (correct shape, wrong content).
    let fake = "0000000000000000000000000000000000000000000000000000000000000001";
    match client.get(&me).header(AUTHORIZATION, bearer(fake)).send() {
        Ok(r) => {
            if let Err(m) = expect_status(
                "fake-but-well-formed PAT (must NOT be accepted)",
                r.status().as_u16(),
                401,
            ) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("fake-token probe: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}
