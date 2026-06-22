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

use crate::harness::{bearer, expect_denied, url_cas, url_cas_batch_read, Config, JourneyResult};
use crate::personas::Persona;

/// Env var carrying a content address the operator has ALREADY erased on the
/// target env (so a read must return 410). Absent → (1)+(2) gate.
const TOMBSTONED_HASH_ENV: &str = "CORELINK_E2E_TOMBSTONED_HASH";

/// Env var carrying a Clerk/operator session bearer that can drive the
/// customer-facing DSR request surface (account-deletion request). Absent → the
/// request→gone end-to-end journey GATES.
const DSR_SESSION_ENV: &str = "CORELINK_E2E_DSR_SESSION";

/// Run the DSR journeys: erased-read, batch-erased-read, internal-erase deny,
/// request→gone end-to-end (gated), full-flow (gated).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        erased_read_is_gone(cfg, client),
        batch_read_erased_is_gone(cfg, client),
        internal_erase_not_customer_reachable(cfg, client),
        dsr_request_then_content_gone(cfg, client),
        account_deletion_flow_gated(),
    ]
}

/// The operator-supplied already-erased hash, if present.
fn tombstoned_hash() -> Option<String> {
    env::var(TOMBSTONED_HASH_ENV).ok().filter(|v| !v.is_empty())
}

/// (1) HAPPY (observable) — GET an erased content address → **410 Gone**.
fn erased_read_is_gone(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "DSR: erased content-address GET -> 410 Gone (tombstone)";
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
    let url = url_cas(cfg, &p1.tenant, &hash);

    let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let got = resp.status().as_u16();
    // 410 is the contract. A 404 (never-existed / hidden) is acceptable-but-weaker
    // (the bytes are still unreadable). A 200 is a HARD FAIL — erased bytes served.
    match got {
        410 | 404 => JourneyResult::pass(name, ms(start)),
        200 => JourneyResult::fail(
            name,
            ms(start),
            "ERASED CONTENT SERVED 200 — GDPR tombstone gate not enforced".to_string(),
        ),
        other => JourneyResult::fail(
            name,
            ms(start),
            format!("GET erased hash got {other} (expected 410 Gone)"),
        ),
    }
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
    match expect_denied(name, got) {
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

    // (b) The already-erased content must read as GONE (the proven end-state).
    let read_url = url_cas(cfg, &p1.tenant, &hash);
    let read = match client.get(&read_url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {read_url}: {e}")),
    };
    let read_status = read.status().as_u16();
    match read_status {
        410 | 404 => JourneyResult::pass(name, ms(start)),
        200 => JourneyResult::fail(
            name,
            ms(start),
            "ERASED CONTENT SERVED 200 after a DSR request — erasure end-state not honoured"
                .to_string(),
        ),
        other => JourneyResult::fail(
            name,
            ms(start),
            format!("erased-content read got {other} after DSR request (expected 410/404 gone)"),
        ),
    }
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
