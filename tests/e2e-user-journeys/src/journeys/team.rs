//! # Team-invite / multi-seat lifecycle journeys (gap-map M13).
//!
//! SMB teams are the target buyer; "invite a teammate" is table-stakes
//! onboarding and was completely untested. This module drives the two
//! live customer-team endpoints (confirmed in
//! `crates/corelink-container/src/routes/customer.rs`):
//!
//! ```text
//! GET  /v1/customer/team
//! POST /v1/customer/team/invite  { email, role }
//! ```
//!
//! ## What is black-box-complete
//!
//! 1. **Invite accepted (endpoint answers)** — admin PAT POSTs invite; server
//!    returns 200/201 or 501 (HONEST v1: the D1 handler gates this with
//!    `NotImplemented` in prod until the feature ships; InMemory handler in
//!    dev/CI returns 201). Both outcomes are expected — we record PASS for
//!    201/200/501 and FAIL for any other status (500/400/401/403/404 etc.).
//! 2. **Non-admin cannot invite a privileged role** — `P1ReadWrite` (cache
//!    read-write, NOT the admin PAT) is used to invite a privileged `Admin`
//!    role; the source gate (`role_is_privileged` + `requires_cache_write`) only
//!    blocks callers WITHOUT cache-write scope, so this journey instead uses
//!    `P2ReadOnly` (`cas:r`) which MUST be denied 403. The gate is on the
//!    *scope* header, not the PAT kind — so this is a real active auth gate.
//! 3. **Team list responds** — `GET /v1/customer/team` with an admin token
//!    returns 200 with a `members` array (or 501 if the D1 handler is
//!    unimplemented for list as well).
//!
//! ## What is gated on out-of-band steps
//!
//! - **Second seat gains scoped access**: the invite flow produces a
//!   `member.status = "invited"` record. The invited user must click the email
//!   link and complete Clerk signup/accept before a second-seat PAT can be
//!   issued. There is no black-box API to complete that step or mint a
//!   second-seat PAT programmatically without an email round-trip.
//!
//! - **Seat removal revokes**: there is NO seat removal route in the router
//!   (`crates/corelink-container/src/routes/customer.rs` routes() table).
//!   The route `DELETE /v1/customer/team/:user_id` or equivalent does NOT
//!   exist as of the harness-freeze. Gated with the missing route recorded.
//!
//! ## Endpoint search log
//!
//! Searched: `crates/corelink-container/src/routes/customer.rs` (router fn,
//! route table) + `worker/src/index.ts` (matchRoute) + full grep for
//! `team.*remove`, `seat.*remove`, `member.*delete`, `DELETE.*team`.
//! Found: `GET /v1/customer/team` + `POST /v1/customer/team/invite` only.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::harness::{bearer, expect_gate_denied, url_customer, Config, JourneyResult, TokenKind};
use crate::personas::Persona;

/// Run the team-invite / multi-seat journeys (M13).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        invite_accepted_or_not_implemented(cfg, client),
        non_admin_scope_cannot_invite_privileged_role(cfg, client),
        team_list_responds(cfg, client),
        second_seat_scoped_access_gated_oob(cfg, client),
        seat_removal_gated_no_route(cfg, client),
    ]
}

// ─── Journey 1: Invite accepted (or HONEST-v1 501) ────────────────────────────

/// Admin PAT (P3Admin / TokenKind::Admin) calls `POST /v1/customer/team/invite`
/// with `cfg.team_invite_email`. Asserts the server returns 201/200 (InMemory /
/// future D1 happy path) OR 501 (HONEST v1: D1 handler gates with
/// `NotImplemented` while the feature is in-flight). Any other status is a
/// contract violation — 400/401/403/404/5xx all fail.
///
/// Gates cleanly if the admin token or invite email env var is absent.
fn invite_accepted_or_not_implemented(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Team: admin invites teammate → 201 (InMemory) or 501 (HONEST-v1 D1 not yet live)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Prereq: admin token.
    let token = match cfg.token(TokenKind::Admin) {
        Some(t) => t,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_PAT_ADMIN not set — admin token required for team invite",
            )
        }
    };

    // Prereq: invite email.
    let email = match cfg.team_invite_email.as_deref() {
        Some(e) if !e.is_empty() => e,
        _ => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_TEAM_INVITE_EMAIL not set — required for team invite journey",
            )
        }
    };

    let url = url_customer(cfg, "team/invite");
    let body = json!({ "email": email, "role": "Developer" });

    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("POST {url}: {e}"),
            )
        }
    };

    let status = resp.status().as_u16();

    match status {
        // 201 = InMemory happy path (dev/CI) or future D1 happy path.
        201 | 200 => {
            // Parse the response body and assert the invite record is present.
            match resp.bytes() {
                Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(v) => {
                        let member_email = v["member"]["email"].as_str().unwrap_or("");
                        let member_status = v["member"]["status"].as_str().unwrap_or("");
                        if member_email.is_empty() {
                            return JourneyResult::fail(
                                name,
                                ms(start),
                                format!(
                                    "POST invite returned {status} but response missing \
                                     member.email — body: {v}"
                                ),
                            );
                        }
                        if member_email != email {
                            return JourneyResult::fail(
                                name,
                                ms(start),
                                format!(
                                    "POST invite returned {status} but member.email \
                                     mismatch: got {member_email:?}, want {email:?}"
                                ),
                            );
                        }
                        // The invite status should reflect a pending/invited state,
                        // NOT "active" (the invited user hasn't accepted yet).
                        if member_status == "active" {
                            return JourneyResult::fail(
                                name,
                                ms(start),
                                format!(
                                    "POST invite: member.status is 'active' immediately — \
                                     invite flow should produce 'invited' (pending email \
                                     acceptance), not 'active'. Got: {member_status:?}"
                                ),
                            );
                        }
                    }
                    Err(e) => {
                        return JourneyResult::fail(
                            name,
                            ms(start),
                            format!("POST invite returned {status} but body is not JSON: {e}"),
                        );
                    }
                },
                Err(e) => {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("POST invite returned {status} but failed to read body: {e}"),
                    );
                }
            }
        }
        // 501 = HONEST v1 — D1 team-invite handler not yet live. This is an
        // explicitly documented, expected production state (customer.rs map_err +
        // map_err_not_implemented_is_501 unit test). Record as PASS: the endpoint
        // is reachable, authenticated, and returns the correct honest status code.
        501 => {
            // PASS — fall through.
        }
        _ => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "POST {url} returned {status} — expected 201 (InMemory happy path), \
                     200, or 501 (HONEST-v1 D1 not-yet-implemented). \
                     400/401/403/404/5xx are contract violations."
                ),
            );
        }
    }

    JourneyResult::pass(name, ms(start))
}

// ─── Journey 2: Non-write-scope PAT cannot invite a privileged role ───────────

/// A read-only PAT (`P2ReadOnly`, `cas:r` scope) POSTs invite with role
/// `"Admin"` — a privileged role — and must be DENIED 403 by the
/// `role_is_privileged` + `requires_cache_write` gate in `handle_team_invite`.
///
/// This is an ACTIVE auth-gate test (`expect_gate_denied` — 401/403 only; a
/// 404 would mean the route is absent, not that the gate fired). The gate is
/// on the Worker-trusted `x-corelink-scope` header, not on the PAT kind itself.
fn non_admin_scope_cannot_invite_privileged_role(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Team: read-only PAT (cas:r scope) inviting 'Admin' role → 403 gate-denied";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Use P2ReadOnly — a read-only cache PAT (cas:r scope) — not the admin PAT.
    // The source gate fires on the *scope* header, not the PAT kind; P2ReadOnly
    // is the persona whose Worker-stamped scope is read-only (cas:r).
    let ro = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = ro.token.expect("P2ReadOnly always has a token");

    let url = url_customer(cfg, "team/invite");
    // Role "Admin" is privileged → gate must fire 403 for a read-only caller.
    let body = json!({ "email": "attacker@example.com", "role": "Admin" });

    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("POST {url}: {e}"),
            )
        }
    };

    let status = resp.status().as_u16();
    if let Err(msg) = expect_gate_denied("read-only-scope inviting Admin role", status) {
        return JourneyResult::fail(name, ms(start), msg);
    }

    JourneyResult::pass(name, ms(start))
}

// ─── Journey 3: Team list responds ────────────────────────────────────────────

/// Admin PAT calls `GET /v1/customer/team`. Asserts the endpoint is reachable
/// (200 with a `members` array) or returns 501 (HONEST v1). Gates cleanly
/// if the admin token is absent. A 401/403/404 is a hard failure — the route
/// must answer for an authenticated admin caller.
fn team_list_responds(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Team: GET /v1/customer/team with admin token → 200 (members array) or 501";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let token = match cfg.token(TokenKind::Admin) {
        Some(t) => t,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_PAT_ADMIN not set — admin token required for team list",
            )
        }
    };

    let url = url_customer(cfg, "team");
    let resp = match client
        .get(&url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };

    let status = resp.status().as_u16();
    match status {
        200 => {
            // Parse and assert the `members` key exists (may be an empty array).
            match resp.bytes() {
                Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(v) => {
                        if v["members"].as_array().is_none() {
                            return JourneyResult::fail(
                                name,
                                ms(start),
                                format!(
                                    "GET team returned 200 but body missing 'members' array. \
                                     body: {v}"
                                ),
                            );
                        }
                    }
                    Err(e) => {
                        return JourneyResult::fail(
                            name,
                            ms(start),
                            format!("GET team returned 200 but body is not JSON: {e}"),
                        );
                    }
                },
                Err(e) => {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("GET team returned 200 but failed to read body: {e}"),
                    );
                }
            }
        }
        // 501 = HONEST v1 (team list not yet backed by D1 handler).
        501 => {}
        _ => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "GET {url} returned {status} — expected 200 or 501. \
                     401/403 means the admin token lacked scope; 404 means the \
                     route is absent."
                ),
            );
        }
    }

    JourneyResult::pass(name, ms(start))
}

// ─── Journey 4: Second seat scoped access — gated OOB ─────────────────────────

/// Drive as far as the invite creates an `"invited"` member record, then gate
/// the scoped-access assertion because completing the invite requires the
/// invitee to click the emailed Clerk link and accept. There is no black-box
/// API endpoint to mint a second-seat PAT without that email round-trip.
///
/// This journey documents the OOB dependency precisely rather than silently
/// skipping it. The `invite_accepted_or_not_implemented` journey already
/// asserts the invite POST works; this one gates on the next step.
fn second_seat_scoped_access_gated_oob(_cfg: &Config, _client: &Client) -> JourneyResult {
    JourneyResult::gated(
        "Team: second seat gains scoped CAS access (multi-seat under one tenant)",
        "OOB step required: the invited user must click the Clerk email link and complete \
         signup/accept before a second-seat PAT is issued. No black-box API exists to \
         complete the invite or mint a second-seat PAT programmatically. The invite POST \
         itself is covered by 'invite_accepted_or_not_implemented'. Re-run this journey \
         once a /v1/customer/team/accept or equivalent invite-completion endpoint lands.",
    )
}

// ─── Journey 5: Seat removal revokes access — gated, no route ─────────────────

/// There is no seat-removal route in the live router. Searched:
/// `crates/corelink-container/src/routes/customer.rs` (full router fn +
/// route table comment), `worker/src/index.ts`, and a repo-wide grep for
/// `TeamRemove`, `team.*remove`, `seat.*remove`, `member.*delete`,
/// `DELETE.*team`. Only `GET /v1/customer/team` and
/// `POST /v1/customer/team/invite` exist as of harness-freeze.
fn seat_removal_gated_no_route(_cfg: &Config, _client: &Client) -> JourneyResult {
    JourneyResult::gated(
        "Team: seat removal → invited member's further access is revoked",
        "No seat-removal route found in crates/corelink-container/src/routes/customer.rs. \
         Searched for DELETE /v1/customer/team/:user_id and equivalents — route does not \
         exist. Gate this journey until a seat-removal endpoint lands and add a \
         url_customer(cfg, \"team/{user_id}/remove\") or equivalent builder.",
    )
}
