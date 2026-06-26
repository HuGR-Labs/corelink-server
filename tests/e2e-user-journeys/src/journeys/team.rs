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

use crate::harness::{
    bearer, blake3_hex, expect_denied, expect_gate_denied, unique_blob, url_cas, url_customer,
    Config, JourneyResult, TokenKind,
};
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
        // 501 = HONEST v1 — by-email Clerk invite (WP-4, ADR-S33-001) not yet
        // live. This is GATED, not PASS: a not-implemented feature must not count
        // toward the pass floor (auditor finding — a 501-as-PASS lets the feature
        // stay unbuilt forever while the journey reads green). Only a real 200/201
        // carrying a validated `member` record is a PASS. Auto-arms when WP-4 ships.
        501 => {
            return JourneyResult::gated(
                name,
                "team invite returns 501 (WP-4 by-email Clerk invite not yet live) — \
                 honest not-implemented; only 200/201 with a member record is a PASS",
            );
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
        // 501 = HONEST v1 (team list not backed by a D1 handler). GATED, not
        // PASS (auditor finding): a not-implemented list must not count toward the
        // pass floor. Now that the membership backend is live (ADR-S33-001) this
        // arm should not fire in prod — but if list ever regresses to 501 the
        // journey gates instead of silently reading green. Only a 200 with a
        // `members` array is a PASS.
        501 => {
            return JourneyResult::gated(
                name,
                "team list returns 501 (D1 handler not live) — only a 200 with a \
                 members array is a PASS",
            );
        }
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

/// **Second seat gains scoped access (multi-seat under one tenant).** With the
/// membership backend live (ADR-S33-001), an operator-provisioned ACTIVE member
/// (a second principal on the admin tenant) must (1) appear in the admin's team
/// list and (2) hold a tenant-scoped PAT that actually works on the data plane.
///
/// The by-EMAIL Clerk invitation round-trip (WP-4) is still OOB and not asserted
/// here; this journey proves the membership + per-seat access the backend now
/// supports. Gates cleanly if the member creds are not provisioned.
fn second_seat_scoped_access_gated_oob(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Team: second seat gains scoped CAS access (multi-seat under one tenant)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let (Some(member_token), Some(member_tenant), Some(member_uid)) = (
        cfg.token(TokenKind::TeamMember),
        cfg.team_member_tenant.as_deref(),
        cfg.team_member_user_id.as_deref(),
    ) else {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_PAT_TEAM_MEMBER / _TEAM_MEMBER_TENANT / _TEAM_MEMBER_USER_ID not set — \
             needs an operator-provisioned ACTIVE team member (the by-email Clerk invite flow is \
             WP-4/OOB). The membership backend (list/seat) is what this asserts.",
        );
    };
    let Some(admin) = cfg.token(TokenKind::Admin) else {
        return JourneyResult::gated(name, "CORELINK_E2E_PAT_ADMIN not set — needed to list the team");
    };

    // (1) The member appears in the admin's team list (the seat is registered).
    let list_url = url_customer(cfg, "team");
    let listed = match client.get(&list_url).header(AUTHORIZATION, bearer(admin)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {list_url}: {e}")),
    };
    if listed.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET team got {} (expected 200 to verify the seat)", listed.status()),
        );
    }
    let body: serde_json::Value = match listed.bytes() {
        Ok(b) => serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("team body: {e}")),
    };
    let member_listed = body["members"]
        .as_array()
        .map(|ms_| ms_.iter().any(|m| m["user_id"].as_str() == Some(member_uid)))
        .unwrap_or(false);
    if !member_listed {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("member {member_uid} not present in the team list — seat not registered"),
        );
    }

    // (2) The member's tenant-scoped PAT actually works on the data plane.
    let blob = unique_blob("corelink-e2e-second-seat");
    let hash = blake3_hex(&blob);
    let url = url_cas(cfg, member_tenant, &hash);
    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(member_token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("member PUT {url}: {e}")),
    };
    match put.status().as_u16() {
        200 | 201 => JourneyResult::pass(name, ms(start)),
        other => JourneyResult::fail(
            name,
            ms(start),
            format!("second-seat member PAT CAS write got {other} (expected 200/201 scoped access)"),
        ),
    }
}

// ─── Journey 5: Seat removal revokes access — gated, no route ─────────────────

/// **Seat removal revokes the member's access.** With `DELETE
/// /v1/customer/team/:user_id` live (ADR-S33-001), removing a member must flip
/// the seat to `removed` AND revoke the member's PATs — so the member's
/// previously-working data-plane access is then DENIED. This is the load-bearing
/// security boundary (a removed member must lose access, not just a list entry).
///
/// Order: this runs AFTER `second_seat_scoped_access` (which proved the member
/// could write), so the member starts with access. Gates cleanly if the member
/// creds are not provisioned.
fn seat_removal_gated_no_route(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Team: seat removal → invited member's further access is revoked";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let (Some(member_token), Some(member_tenant), Some(member_uid)) = (
        cfg.token(TokenKind::TeamMember),
        cfg.team_member_tenant.as_deref(),
        cfg.team_member_user_id.as_deref(),
    ) else {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_PAT_TEAM_MEMBER / _TEAM_MEMBER_TENANT / _TEAM_MEMBER_USER_ID not set — \
             needs an operator-provisioned ACTIVE team member to remove.",
        );
    };
    let Some(admin) = cfg.token(TokenKind::Admin) else {
        return JourneyResult::gated(name, "CORELINK_E2E_PAT_ADMIN not set — needed to remove a seat");
    };

    // Precondition: the member currently HAS data-plane access (so a later deny
    // proves the removal, not a pre-existing lack of access).
    let probe = unique_blob("corelink-e2e-seat-removal-pre");
    let probe_hash = blake3_hex(&probe);
    let probe_url = url_cas(cfg, member_tenant, &probe_hash);
    let pre = match client
        .put(&probe_url)
        .header(AUTHORIZATION, bearer(member_token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(probe)
        .send()
    {
        Ok(r) => r.status().as_u16(),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("pre-probe {probe_url}: {e}")),
    };
    if !matches!(pre, 200 | 201) {
        return JourneyResult::gated(
            name,
            format!("member lacks access BEFORE removal (got {pre}) — cannot prove revocation"),
        );
    }

    // Remove the seat (owner/admin scope). Expect 200 + a revoked-PAT count.
    let del_url = url_customer(cfg, &format!("team/{member_uid}"));
    let del = match client
        .delete(&del_url)
        .header(AUTHORIZATION, bearer(admin))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("DELETE {del_url}: {e}")),
    };
    let dstatus = del.status().as_u16();
    if dstatus != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("seat-removal DELETE got {dstatus} (expected 200; 403 ⇒ admin lacked write scope)"),
        );
    }

    // The member's access must now be DENIED. The native PAT gate has a ~5s
    // verify-cache, so poll (bounded) until the deny lands — a real removal takes
    // effect within that window; a persistent 2xx is the security failure.
    for attempt in 0..8 {
        let blob = unique_blob("corelink-e2e-seat-removal-post");
        let hash = blake3_hex(&blob);
        let url = url_cas(cfg, member_tenant, &hash);
        let status = match client
            .put(&url)
            .header(AUTHORIZATION, bearer(member_token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(blob)
            .send()
        {
            Ok(r) => r.status().as_u16(),
            Err(e) => return JourneyResult::fail(name, ms(start), format!("post-probe {url}: {e}")),
        };
        if expect_denied("removed-member CAS write", status).is_ok() {
            return JourneyResult::pass(name, ms(start));
        }
        if !matches!(status, 200 | 201) {
            // Some non-2xx, non-standard-deny — surface it rather than spin.
            return JourneyResult::fail(
                name,
                ms(start),
                format!("post-removal member write got {status} (expected a 401/403/404 deny)"),
            );
        }
        if attempt < 7 {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
    JourneyResult::fail(
        name,
        ms(start),
        "SEAT-REMOVAL INTEGRITY: removed member's PAT still wrote (2xx) after ~16s — the seat \
         removal did NOT revoke the member's data-plane access"
            .to_string(),
    )
}
