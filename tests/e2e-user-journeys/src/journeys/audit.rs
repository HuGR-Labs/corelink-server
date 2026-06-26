//! Audit journeys — customer-facing audit export + offline row re-derive.
//!
//! URL CORRECTED (round-3 live run): the worker does NOT route
//! `/v1/audit/export` (404 at the edge). The customer-facing audit surface is
//! `GET /v1/customer/audit` (under the `/v1/customer/*` proxy), whose handler
//! (`routes/customer.rs::handle_audit`) returns
//! `{"rows":[{event_id, ts, event_type, severity, actor, summary}]}`.
//!
//! Re-derive contract (black-box, no internal read): assert 200 + a well-formed
//! `rows` array, and validate every row's required fields + monotonic `ts`. The
//! customer audit sink is a SEPARATE store from the request-path audit chain, so
//! it is legitimately empty for a quiet tenant — the live handler returns an
//! empty `rows` array for a new tenant (server test
//! `audit_route_returns_empty_rows_for_new_tenant`). We therefore treat an empty
//! `rows` (well-formed) as a PASS on the SHAPE contract rather than forcing a
//! seed we cannot deterministically land in this sink from the public edge.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use serde_json::Value;

use crate::harness::{bearer, url_customer, url_users_me, Config, JourneyResult};
use crate::personas::Persona;

/// Run the audit journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![audit_export_rederive(cfg, client)]
}

/// Perform a few authenticated ops, read the customer audit log, and re-derive
/// the structure from ONLY the public payload (black-box: no internal read).
fn audit_export_rederive(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Audit: GET /v1/customer/audit → re-derive row shape from public payload";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 has a token");

    // Generate a few audit-worthy events on the request path.
    let me = url_users_me(cfg);
    for _ in 0..3 {
        let _ = client.get(&me).header(AUTHORIZATION, bearer(token)).send();
    }

    // Customer-facing audit list (worker-routed under /v1/customer/*).
    let url = url_customer(cfg, "audit?limit=100");
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
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("audit 200 but missing/non-array 'rows' field: {body}"),
            )
        }
    };

    // An empty `rows` is the documented contract for a quiet tenant (the
    // customer audit sink is distinct from the request-path chain, and the
    // /users/me reads above land in the request-path chain, NOT this sink — so
    // we cannot deterministically seed it black-box). GATED, not PASS (auditor
    // finding): an empty page validates NOTHING about row re-derivation, so
    // passing on it is a false-green that counts toward the floor. We only PASS
    // when there is at least one row and the per-row + monotonic-ts checks below
    // actually run. Auto-arms the moment the sink carries a row.
    if rows.is_empty() {
        return JourneyResult::gated(
            name,
            "GET /v1/customer/audit returned an empty rows page (quiet tenant; the \
             customer audit sink cannot be seeded black-box) — re-derive asserts \
             nothing on an empty page, so this is GATED not PASS",
        );
    }

    // Re-derive: every row carries the required fields and `ts` is monotonic.
    let mut prev_ts: Option<i64> = None;
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
        // `ts` may be a numeric epoch or an RFC-3339 string; only enforce
        // monotonicity when it is numeric (the only black-box-orderable form).
        if let Some(ts) = row["ts"].as_i64() {
            if let Some(p) = prev_ts {
                if ts < p {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("ordering violation: row {i} ts={ts} < prev={p}"),
                    );
                }
            }
            prev_ts = Some(ts);
        }
    }

    JourneyResult::pass(name, ms(start))
}
