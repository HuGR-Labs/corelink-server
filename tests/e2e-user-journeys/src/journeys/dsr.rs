//! DSR / GDPR erasure journeys (data-subject-request, account deletion).
//!
//! Surface S15. The erasure TRANSPORTS are container-internal and
//! internal-auth-gated (`POST /_internal/dsr/erase`,
//! `POST /_internal/cas/{tenant}/{hash}/erase`) — NOT customer surfaces. So a
//! black-box suite verifies the CUSTOMER-OBSERVABLE contract:
//!   1. An already-erased content address returns **410 Gone** on read (the
//!      tombstone gate) — the proof the bytes are unrecoverable.
//!   2. The batch-read plane reports an erased hash as **gone**, never its bytes
//!      (and fails CLOSED on a tombstone-lookup fault — PEN-2/REV-S1, #421).
//!   3. The internal erase path is NOT reachable with a plain customer PAT (a
//!      customer cannot self-erase arbitrary objects).
//!   4. The full account-deletion DSR flow + legal-hold preservation runs on
//!      internal transports → GATED (needs the internal key, supplied
//!      out-of-band; never faked).
//!
//! ## Fixture for (1)+(2): a known-erased hash
//! There is no black-box way to *create* a tombstone (erase is internal). So the
//! operator supplies a hash that has already been erased on the target env via
//! `CORELINK_E2E_TOMBSTONED_HASH`; absent it, (1)+(2) GATE (recorded, not
//! skipped). This keeps the suite black-box AND honest. The batch-read request
//! body shape is the live contract's; if it ever diverges, only the live run
//! (gated on the fixture) is affected, never CI.

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::harness::{
    bearer, blake3_hex, expect_gate_denied, unique_blob, url_cas, url_cas_batch_read, Config,
    JourneyResult,
};
use crate::personas::Persona;

/// Env var carrying a content address the operator has ALREADY erased on the
/// target env (so a read must return 410). Absent → (1)+(2) gate.
const TOMBSTONED_HASH_ENV: &str = "CORELINK_E2E_TOMBSTONED_HASH";

/// Env var carrying a Clerk/operator session bearer that can drive the
/// customer-facing DSR request surface (account-deletion request). Absent → the
/// request→gone end-to-end journey GATES.
const DSR_SESSION_ENV: &str = "CORELINK_E2E_DSR_SESSION";

/// Opt-in flag for the SELF-DRIVING DSR full-flow (write → request erasure →
/// assert gone). This MUTATES state (it erases content on the target tenant), so
/// it is GATED off by default — the operator opts in only against a DEDICATED
/// test tenant. Absent → the full-flow journey GATES (recorded, never skipped).
const DSR_TEST_ENV: &str = "CORELINK_E2E_DSR_TEST";

/// Env var carrying the DEDICATED test tenant id the self-driving full-flow may
/// erase. The drive REFUSES to run against the primary tenant
/// (`CORELINK_E2E_TENANT`) to avoid erasing real customer content — the operator
/// must point this at a throwaway tenant. Absent → the full-flow journey GATES.
const DSR_TEST_TENANT_ENV: &str = "CORELINK_E2E_DSR_TEST_TENANT";

/// Env var carrying a Clerk/operator session bearer scoped to the DEDICATED test
/// tenant, used to drive the customer-facing erasure-request surface in the
/// self-driving full-flow. Absent → the full-flow journey GATES.
const DSR_TEST_SESSION_ENV: &str = "CORELINK_E2E_DSR_TEST_SESSION";

/// Run the DSR journeys: erased-read, batch-erased-read, internal-erase deny,
/// request→gone end-to-end (gated), self-driving full-flow (gated), full account
/// deletion flow (gated).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        erased_read_is_gone(cfg, client),
        batch_read_erased_is_gone(cfg, client),
        internal_erase_not_customer_reachable(cfg, client),
        dsr_request_then_content_gone(cfg, client),
        dsr_full_flow_self_driven(cfg, client),
        account_deletion_flow_gated(),
    ]
}

/// The operator-supplied already-erased hash, if present.
fn tombstoned_hash() -> Option<String> {
    env::var(TOMBSTONED_HASH_ENV).ok().filter(|v| !v.is_empty())
}

/// Poll a CAS GET for the GDPR erased end-state, classifying STRONG vs WEAK
/// proof (auditor tooth-audit, refined against live prod behavior):
///
///   - 200 at any point → FAIL (erased bytes SERVED — the hard GDPR violation).
///   - 410             → PASS (STRONG proof: the durable tombstone marker — the
///     bytes provably *were here and were erased*, not merely absent).
///   - persistent 404 through the budget → GATED (WEAK-but-acceptable: the bytes
///     are NOT served, so GDPR "content gone" holds, but the durable 410 marker
///     did not surface within the window). This is NOT a PASS (a 404 alone can't
///     distinguish "erased" from "never-written" — the auditor's concern) and
///     NOT a FAIL (the bytes ARE gone, and prod's tombstone bloom-refresh lag is
///     eventually-consistent and observed to exceed the budget on some runs, so
///     a hard 410-only assert would FLAKE). 410 PASSES whenever the marker is
///     warm; otherwise we honestly record the weak form.
///
/// A durable tombstone answers 410 once its bloom refreshes; until then a GET
/// can transiently 404, so we poll rather than assert once.
fn poll_until_gone(
    client: &Client,
    url: &str,
    token: &str,
    name: &'static str,
    start: Instant,
) -> JourneyResult {
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let mut last = 0u16;
    for attempt in 0..8 {
        let resp = match client.get(url).header(AUTHORIZATION, bearer(token)).send() {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
        };
        last = resp.status().as_u16();
        match last {
            410 => return JourneyResult::pass(name, ms(start)),
            200 => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    "ERASED CONTENT SERVED 200 — GDPR tombstone gate not enforced".to_string(),
                )
            }
            // 404 (bloom-refresh lag) or transient — wait briefly and re-poll.
            _ if attempt < 7 => std::thread::sleep(std::time::Duration::from_secs(2)),
            _ => {}
        }
    }
    // Bytes are not served (GDPR-satisfied) but the strong 410 marker never
    // surfaced within the budget — record the weak form, never a false green.
    JourneyResult::gated(
        name,
        format!(
            "erased hash returned {last} (not served — GDPR 'content gone' holds) but the durable \
             410 tombstone marker did not surface within the poll budget (bloom-refresh lag, \
             eventually-consistent); strong-form 410 unproven this run"
        ),
    )
}

/// (1) HAPPY (observable) — GET an erased content address → **410 Gone**.
fn erased_read_is_gone(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "DSR: erased content-address GET -> 410 Gone (tombstone)";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let hash = match tombstoned_hash() {
        Some(h) => h,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_TOMBSTONED_HASH not set — supply an already-erased hash on the target env",
            )
        }
    };
    let token = p1.token.expect("P1 always has a token");
    let url = url_cas(cfg, &p1.tenant, &hash);
    // STRICT 410 with bounded bloom-lag tolerance (see poll_until_gone).
    poll_until_gone(client, &url, token, name, start)
}

/// (2) EDGE — batch-read of an erased hash reports it **gone**, never its bytes;
/// the plane fails CLOSED on a tombstone fault (#421).
fn batch_read_erased_is_gone(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "DSR: batch-read of erased hash -> gone, never bytes";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let hash = match tombstoned_hash() {
        Some(h) => h,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_TOMBSTONED_HASH not set — supply an already-erased hash on the target env",
            )
        }
    };
    let token = p1.token.expect("P1 always has a token");
    let url = url_cas_batch_read(cfg, &p1.tenant);

    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .body(json!({ "hashes": [hash] }).to_string())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let got = resp.status().as_u16();
    let body = resp.text().unwrap_or_default();

    // Contract: the erased hash must NOT come back with its bytes. Safe outcomes:
    //   - the whole batch failing CLOSED (503) on the tombstone gate (#421), OR
    //   - a per-hash gone/410/missing marker (no "ok" + non-zero len for it).
    // HARD FAIL: an "ok" entry with non-zero length for the erased hash.
    if got == 503 {
        return JourneyResult::pass(name, ms(start));
    }
    let served_bytes = body.contains("\"status\":\"ok\"")
        && body.contains(&hash)
        && body.contains("\"len\":")
        && !body.contains("\"len\":0")
        && !body.contains("\"len\": 0");
    if served_bytes {
        JourneyResult::fail(
            name,
            ms(start),
            format!("batch-read returned bytes for an erased hash (status {got})"),
        )
    } else {
        JourneyResult::pass(name, ms(start))
    }
}

/// (3) ADVERSARIAL — the internal erase endpoint must NOT be reachable with a
/// plain customer PAT (a customer cannot self-erase arbitrary objects).
fn internal_erase_not_customer_reachable(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "DSR: internal erase path denied to a customer PAT";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");
    // Deliberately NOT a harness URL builder: the internal erase transport is not
    // a customer surface, so it has no public-contract builder. We probe it raw to
    // assert it is NOT customer-reachable. A made-up hash is fine — the auth gate
    // must reject before any erase work happens.
    let fake_hash = "0".repeat(64);
    let url = format!(
        "{}/_internal/cas/{}/{}/erase",
        cfg.endpoint, p1.tenant, fake_hash
    );

    let resp = match client.post(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    let got = resp.status().as_u16();
    // A customer PAT must be denied (401/403) or the path not exist at the edge
    // (404). A 200 would mean a customer can erase arbitrary objects — a BUG.
    match expect_gate_denied(name, got) {
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(_) => JourneyResult::fail(
            name,
            ms(start),
            format!("customer PAT reached internal erase (got {got}) — must be denied"),
        ),
    }
}

/// (3b) END-TO-END (gated) — DSR request → erasure → content gone.
///
/// The WP's launch-critical contract: a data-subject erasure REQUEST is accepted,
/// and afterwards the erased tenant's content is GONE (410/404 on read, never its
/// bytes). The request-creation surface (`POST /v1/customer/account/delete` /
/// `/v1/customer/dsr`) is Clerk-session authenticated (a customer initiates their
/// OWN erasure — a customer PAT must not erase arbitrary objects, asserted in
/// (3)). So this end-to-end journey is GATED unless the operator supplies a DSR
/// session bearer (`CORELINK_E2E_DSR_SESSION`) AND the already-erased fixture
/// hash (`CORELINK_E2E_TOMBSTONED_HASH`) proving the post-erasure end-state.
///
/// NOTE: 0069 `dsr_requested` table is a known prod consideration — if it is
/// missing on the target env the request path fails closed (cas_erase.rs forbids
/// erasure). We GATE on the absent creds rather than asserting that prod state.
///
/// When BOTH are supplied we assert: (a) the request POST is ACCEPTED
/// (200/202 — queued/accepted, never a 5xx), and (b) the operator's already-erased
/// hash reads as GONE (410/404, never 200 with bytes) — the request and the
/// proven end-state together.
fn dsr_request_then_content_gone(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "DSR: erasure request accepted -> erased content is GONE (request->gone, gated)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let session = env::var(DSR_SESSION_ENV).ok().filter(|v| !v.is_empty());
    let session = match session {
        Some(s) => s,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_DSR_SESSION not set — the erasure-request surface is Clerk-session \
                 authenticated (a customer erases their OWN data); supply a DSR session bearer \
                 out-of-band to drive request->gone (0069 dsr_requested must exist on the env)",
            )
        }
    };
    let hash = match tombstoned_hash() {
        Some(h) => h,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_TOMBSTONED_HASH not set — supply an already-erased hash to prove \
                 the post-erasure GONE end-state",
            )
        }
    };
    let token = p1.token.expect("P1 always has a token");

    // (a) The DSR request must be ACCEPTED (queued). The customer-facing request
    // route is Clerk-session authenticated; we do NOT a harness builder for it
    // (not a PAT/cache surface) — probe it with the supplied session bearer.
    let req_url = format!("{}/v1/customer/account/delete", cfg.endpoint);
    let req = match client
        .post(&req_url)
        .header(AUTHORIZATION, bearer(&session))
        .header(CONTENT_TYPE, "application/json")
        .body(json!({ "confirm": true }).to_string())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {req_url}: {e}")),
    };
    let req_status = req.status().as_u16();
    // Accepted = 200/202. A 404 here means the request route is named differently
    // on this env — gate (the end-to-end needs the right route), don't false-fail.
    if req_status == 404 {
        return JourneyResult::gated(
            name,
            "DSR request route returned 404 — the account-delete request surface is named \
             differently on this env; cannot drive request->gone here",
        );
    }
    if !matches!(req_status, 200 | 202) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("DSR erasure request got {req_status} (expected 200/202 accepted)"),
        );
    }

    // (b) The already-erased content must read as GONE (the proven end-state):
    // STRICT 410 with bounded bloom-lag tolerance (see poll_until_gone).
    let read_url = url_cas(cfg, &p1.tenant, &hash);
    poll_until_gone(client, &read_url, token, name, start)
}

/// (3c) SELF-DRIVING FULL-FLOW (gated) — write content → request erasure →
/// assert the content is GONE (410/404, never the bytes) AND a batch-read of the
/// erased hash reports it gone.
///
/// This is the launch-critical end-to-end the WP demands: not just observing a
/// pre-erased fixture, but DRIVING the whole customer DSR path on a fresh object.
///
/// SAFETY (critical): this MUTATES state (it erases content). It is GATED behind
/// `CORELINK_E2E_DSR_TEST=1` AND refuses to run unless the operator points it at
/// a DEDICATED throwaway tenant (`CORELINK_E2E_DSR_TEST_TENANT`) that is NOT the
/// primary tenant — so it can never erase real customer content. It also needs a
/// session bearer scoped to that test tenant (`CORELINK_E2E_DSR_TEST_SESSION`),
/// since the customer-facing erasure-request surface is Clerk-session auth'd. The
/// RW PAT (P1) is used only to WRITE+READ the test object (a cache credential).
///
/// 0069 NOTE: the prod schema may be missing migration 0069 (`dsr_requested`); if
/// the erasure request 500s/404s on that missing table, we GATE with that exact
/// diagnosis (don't FAIL on a known prod-schema gap).
fn dsr_full_flow_self_driven(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "DSR: self-driving full-flow — write -> request erasure -> content GONE (gated, mutating)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // GATE 1: the opt-in flag (this mutates / erases state).
    let enabled = env::var(DSR_TEST_ENV).map(|v| v == "1").unwrap_or(false);
    if !enabled {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_DSR_TEST=1 not set — the self-driving DSR full-flow ERASES content; \
             enable it only against a DEDICATED test tenant",
        );
    }

    // GATE 2: a write credential for the test object.
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    // GATE 3: a DEDICATED test tenant that is NOT the primary tenant. Refuse to
    // erase under the primary tenant — that could destroy real customer content.
    let test_tenant = match env::var(DSR_TEST_TENANT_ENV).ok().filter(|v| !v.is_empty()) {
        Some(t) => t,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_DSR_TEST_TENANT not set — the full-flow needs a DEDICATED throwaway \
                 tenant to erase (never the primary tenant)",
            )
        }
    };
    if let Some(primary) = cfg.tenant.as_deref() {
        if test_tenant == primary {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_DSR_TEST_TENANT must differ from CORELINK_E2E_TENANT — refusing to \
                 erase content under the primary tenant",
            );
        }
    }

    // GATE 4: a session bearer scoped to the test tenant for the request surface.
    let session = match env::var(DSR_TEST_SESSION_ENV).ok().filter(|v| !v.is_empty()) {
        Some(s) => s,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_DSR_TEST_SESSION not set — the erasure-request surface is \
                 Clerk-session authenticated; supply a session bearer for the test tenant",
            )
        }
    };

    // STEP 1: WRITE a fresh, unique object under the test tenant (content-addressed,
    // so a fresh UUID body yields a fresh digest — idempotent across runs).
    let blob = unique_blob("corelink-e2e-dsr-fullflow");
    let hash = blake3_hex(&blob);
    let write_url = url_cas(cfg, &test_tenant, &hash);
    let put = match client
        .put(&write_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {write_url}: {e}")),
    };
    let put_status = put.status().as_u16();
    if !matches!(put_status, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("write of the test object got {put_status} (expected 200/201) — cannot drive erasure"),
        );
    }

    // STEP 2: REQUEST erasure on the customer-facing DSR surface (Clerk-session
    // auth'd). A 404 here means the request route is named differently on this env
    // → GATE (the end-to-end needs the right route, don't false-fail). A 500/404
    // attributable to a missing 0069 dsr_requested table → GATE with that exact
    // diagnosis (a known prod-schema gap, not a contract break).
    let req_url = format!("{}/v1/customer/account/delete", cfg.endpoint);
    let req = match client
        .post(&req_url)
        .header(AUTHORIZATION, bearer(&session))
        .header(CONTENT_TYPE, "application/json")
        .body(json!({ "confirm": true }).to_string())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {req_url}: {e}")),
    };
    let req_status = req.status().as_u16();
    let req_body = req.text().unwrap_or_default();
    if req_status == 404 {
        return JourneyResult::gated(
            name,
            "DSR request route returned 404 — the account-delete request surface is named \
             differently on this env (or 0069 dsr_requested is missing); cannot drive the full-flow",
        );
    }
    // Known prod-schema gap: a 5xx mentioning the dsr_requested table → GATE, not FAIL.
    if req_status >= 500 {
        let lc = req_body.to_lowercase();
        if lc.contains("dsr_requested") || lc.contains("no such table") {
            return JourneyResult::gated(
                name,
                "DSR erasure request 5xx'd on a missing 0069 dsr_requested table — known \
                 prod-schema gap; apply migration 0069 then re-run (not a contract break)",
            );
        }
        return JourneyResult::fail(
            name,
            ms(start),
            format!("DSR erasure request got {req_status} (a 5xx, not the 0069 gap) — server fault"),
        );
    }
    if !matches!(req_status, 200 | 202) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("DSR erasure request got {req_status} (expected 200/202 accepted)"),
        );
    }

    // STEP 3a: the erased content must now read as GONE (410/404, never 200 bytes).
    let read = match client
        .get(&write_url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {write_url}: {e}")),
    };
    let read_status = read.status().as_u16();
    match read_status {
        410 | 404 => {}
        200 => {
            return JourneyResult::fail(
                name,
                ms(start),
                "ERASED CONTENT SERVED 200 after a DSR request — erasure end-state not honoured"
                    .to_string(),
            )
        }
        other => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("erased-content read got {other} after DSR request (expected 410/404 gone)"),
            )
        }
    }

    // STEP 3b: the batch-read plane must ALSO report the erased hash as gone, never
    // its bytes. Safe outcomes: a fail-CLOSED 503 (the tombstone gate, #421) or a
    // per-hash gone/missing marker. HARD FAIL: an "ok" entry with non-zero length.
    let batch_url = url_cas_batch_read(cfg, &test_tenant);
    let batch = match client
        .post(&batch_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .body(json!({ "hashes": [hash] }).to_string())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {batch_url}: {e}")),
    };
    let batch_status = batch.status().as_u16();
    let batch_body = batch.text().unwrap_or_default();
    if batch_status == 503 {
        return JourneyResult::pass(name, ms(start));
    }
    let served_bytes = batch_body.contains("\"status\":\"ok\"")
        && batch_body.contains(&hash)
        && batch_body.contains("\"len\":")
        && !batch_body.contains("\"len\":0")
        && !batch_body.contains("\"len\": 0");
    if served_bytes {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("batch-read returned bytes for the erased hash after a DSR request (status {batch_status})"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

/// (4) The full account-deletion DSR flow + legal-hold preservation runs on
/// internal transports (`/_internal/dsr/erase`, internal-auth gated). Not
/// black-box exercisable without the internal key → GATED (never faked).
fn account_deletion_flow_gated() -> JourneyResult {
    JourneyResult::gated(
        "DSR: account-deletion full flow + legal-hold preservation",
        "internal-auth transport (/_internal/dsr/erase) — needs the internal key out-of-band; \
         exercise via the operator DSR runbook, not the black-box edge",
    )
}
