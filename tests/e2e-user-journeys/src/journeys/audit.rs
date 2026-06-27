//! Audit journeys — customer-facing audit log: SEED real events, then read back.
//!
//! The customer-facing audit surface is `GET /v1/customer/audit` (under the
//! `/v1/customer/*` proxy), whose handler (`routes/customer.rs::handle_audit` →
//! `customer_d1.rs::CustomerAuditHandler::query`) returns
//! `{"rows":[{event_id, ts, event_type, severity, actor, summary}]}`.
//!
//! As of migration 0077 the audit log has a real D1 backing
//! (`customer_audit_events`): the control-plane mutations (PAT create, team
//! invite) each write a row best-effort, and the query reads them newest-first.
//! So this journey is no longer a black-box shape-only probe: it SEEDS by
//! performing real authenticated mutations as the read-write persona, then GETs
//! the audit log and ASSERTS non-empty rows + re-derives the row shape (required
//! fields present + `ts` monotonic where numeric). It PASSES once rows exist
//! (it no longer gates on an empty page — an empty page after a successful seed
//! is a real failure).

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::harness::{bearer, url_customer, Config, JourneyResult};
use crate::personas::Persona;

/// Run the audit journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![audit_seed_then_rederive(cfg, client)]
}

/// Seed real control-plane events (PAT create + team invite), read the customer
/// audit log, and re-derive the structure from ONLY the public payload.
fn audit_seed_then_rederive(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Audit: seed (pat.create + team.invite) → GET /v1/customer/audit → non-empty rows + re-derive shape";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 has a token");

    // ── SEED ──────────────────────────────────────────────────────────────────
    // Each committed mutation writes a `customer_audit_events` row (0077). We
    // count how many seed ops actually landed: if NONE land (e.g. the env can't
    // mint keys), we cannot fairly assert non-empty → gate; if at least one
    // lands, an empty audit read afterwards is a real failure.
    let mut seeded = 0u32;

    // Event 1 — PAT create (event_type `pat.created`). A read-only key needs no
    // write-scope gate, so this lands for any read-write caller.
    let keys_url = url_customer(cfg, "keys");
    match client
        .post(&keys_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({ "name": "e2e-audit-seed", "scopes": ["cache:read"] }))
        .send()
    {
        Ok(r) if r.status().as_u16() == 201 => seeded += 1,
        Ok(r) if r.status().as_u16() == 403 => {
            return JourneyResult::gated(
                name,
                "POST /v1/customer/keys → 403 — PAT lacks the scope to seed an audit event",
            )
        }
        Ok(_) | Err(_) => { /* fall through; the invite below may still seed */ }
    }

    // Event 2 — team invite (event_type `team.invited`). Best-effort; only when
    // an invite email is configured (a `Developer` role needs no privileged gate).
    if let Some(email) = cfg.team_invite_email.as_deref().filter(|e| !e.is_empty()) {
        let invite_url = url_customer(cfg, "team/invite");
        if let Ok(r) = client
            .post(&invite_url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({ "email": email, "role": "Developer" }))
            .send()
        {
            if matches!(r.status().as_u16(), 200 | 201) {
                seeded += 1;
            }
        }
    }

    if seeded == 0 {
        return JourneyResult::gated(
            name,
            "no audit-seed event landed (key create + team invite both unavailable in this env) \
             — cannot assert a non-empty audit log",
        );
    }

    // ── READ (bounded poll for D1 read-replica lag) ─────────────────────────────
    let url = url_customer(cfg, "audit?limit=100");
    let mut last_body = Value::Null;
    for attempt in 0..7u32 {
        let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
        };
        let status = resp.status().as_u16();
        if status == 403 {
            return JourneyResult::gated(
                name,
                "GET /v1/customer/audit → 403 — PAT lacks audit-read scope; supply a scoped PAT",
            );
        }
        if status != 200 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET /v1/customer/audit got {status} (expected 200). url={url}"),
            );
        }
        let body: Value = match resp.json() {
            Ok(v) => v,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("audit not JSON: {e}")),
        };
        // Contract (routes/customer.rs handle_audit): {"rows":[...]}.
        let rows = match body.get("rows").and_then(Value::as_array) {
            Some(a) => a.clone(),
            None => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("audit 200 but missing/non-array 'rows' field: {body}"),
                )
            }
        };

        if rows.is_empty() {
            // Seeded rows not visible yet — tolerate replication lag, then retry.
            if attempt < 6 {
                std::thread::sleep(std::time::Duration::from_secs(2));
                last_body = body;
                continue;
            }
            return JourneyResult::fail(
                name,
                ms(start),
                "GET /v1/customer/audit returned an empty rows page AFTER seeding \
                 pat.create/team.invite — the customer audit log is not recording events"
                    .to_owned(),
            );
        }

        // Re-derive: every row carries the required fields AND the page is
        // newest-first. The server emits `ts` as an ISO-8601 string via
        // `ms_to_iso8601` (NOT an integer — the old `as_i64()` check was dead
        // code that never ran). The canonical query is `ORDER BY ts_ms DESC`,
        // and fixed-width ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`) sorts
        // lexicographically == chronologically, so each row's `ts` string MUST
        // be NON-INCREASING down the page. A `DESC→ASC` flip (or dropping the
        // ordering) makes a later row's `ts` exceed an earlier one ⇒ this fails.
        let mut prev_ts: Option<String> = None;
        for (i, row) in rows.iter().enumerate() {
            for field in ["event_id", "ts", "event_type"] {
                if row.get(field).map(Value::is_null).unwrap_or(true) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("row {i} missing required field '{field}': {row}"),
                    );
                }
            }
            // `ts` must be an ISO-8601 STRING (the real wire shape). Anything
            // else (incl. the integer the old dead check assumed) is a contract
            // break that the ordering invariant can no longer rely on.
            let ts = match row["ts"].as_str() {
                Some(s) => s.to_owned(),
                None => {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("row {i} 'ts' is not an ISO-8601 string (got {}): {row}", row["ts"]),
                    );
                }
            };
            if let Some(prev) = &prev_ts {
                // Newest-first ⇒ current must be ≤ previous (lexicographic).
                if ts.as_str() > prev.as_str() {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!(
                            "newest-first ordering violation: row {i} ts={ts} > prev={prev} \
                             (server must return audit rows ORDER BY ts_ms DESC)"
                        ),
                    );
                }
            }
            prev_ts = Some(ts);
        }

        return JourneyResult::pass(name, ms(start));
    }

    JourneyResult::fail(
        name,
        ms(start),
        format!("audit log never became non-empty after seeding; last body: {last_body}"),
    )
}
