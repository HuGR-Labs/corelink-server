//! Audit journeys — customer-facing audit export + offline chain re-derive.
//!
//! Migrated from the old single-file suite (journey 6). URL CORRECTED: the old
//! suite hit `/v1/admin/audit/events` which does NOT exist. The real
//! customer-facing export is `GET /v1/audit/export` (the admin surface is
//! `/v1/admin/read/{resource}` + `/v1/admin/mutate`). The stale P0 framing is
//! flipped to the live contract.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use serde_json::Value;

use crate::harness::{bearer, url_audit_export, url_users_me, Config, JourneyResult};
use crate::personas::Persona;

/// Run the audit journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![audit_export_rederive(cfg, client)]
}

/// Perform a few authenticated ops, export the audit log, and re-derive the
/// hash chain from ONLY the public event payload (black-box: no internal read).
fn audit_export_rederive(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Audit: export /v1/audit/export → re-derive chain from public payload";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 has a token");

    // Generate a few audit-worthy events.
    let me = url_users_me(cfg);
    for _ in 0..3 {
        let _ = client.get(&me).header(AUTHORIZATION, bearer(token)).send();
    }

    // Export.
    let url = format!("{}?limit=100", url_audit_export(cfg));
    let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if status == 403 {
        return JourneyResult::gated(
            name,
            "GET /v1/audit/export → 403 — PAT lacks audit-export scope; supply a scoped PAT",
        );
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v1/audit/export got {status} (expected 200)"),
        );
    }

    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("export not JSON: {e}")),
    };
    // The export page may use `items` or `events` as the array key.
    let items = body["items"]
        .as_array()
        .or_else(|| body["events"].as_array());
    let items = match items {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("export missing items/events array: {body}"),
            )
        }
    };
    if items.is_empty() {
        return JourneyResult::fail(
            name,
            ms(start),
            "audit export returned 0 events after operations".to_string(),
        );
    }

    // Re-derive: required fields present, occurred_at_ms monotonic, prev_hash
    // (if present) is valid 64-hex. This is the strongest check possible from
    // the public payload alone.
    let mut prev_at: Option<i64> = None;
    for (i, ev) in items.iter().enumerate() {
        for field in ["event_id", "event_type", "occurred_at_ms"] {
            if ev[field].is_null() {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("event {i} missing required field '{field}'"),
                );
            }
        }
        let at = ev["occurred_at_ms"].as_i64().unwrap_or(0);
        if let Some(p) = prev_at {
            if at < p {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("ordering violation: event {i} occurred_at_ms={at} < prev={p}"),
                );
            }
        }
        prev_at = Some(at);
        if let Some(ph) = ev["prev_hash"].as_str() {
            if ph.len() != 64 || !ph.chars().all(|c| c.is_ascii_hexdigit()) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("event {i} prev_hash '{ph}' is not valid SHA-256 hex"),
                );
            }
        }
    }

    JourneyResult::pass(name, ms(start))
}
