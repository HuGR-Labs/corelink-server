//! Action-Cache (AC) journeys — native S3 surface `/v1/ac/{tenant}/{digest}`.
//!
//! Black-box only: the deployed HTTP API + a Bearer PAT, via the [`crate::harness`]
//! URL builders + the [`crate::personas`] persona/token map. No `corelink-*`
//! crate import, no mocks, no internal-state reads.
//!
//! Cells covered (matrix S3):
//!   - **Happy** — `update→read` round-trip: PUT an action-result under a fresh
//!     digest, GET it back, and assert the stored payload matches byte-for-byte.
//!   - **Edge / M11** — divergent-body re-PUT (integrity guard): PUT the *same*
//!     digest with a *different* body. The native AC route enforces a hard
//!     **divergent-body integrity guard**: `AcHandlerError::DivergentBody` maps
//!     to HTTP **409 Conflict** (`map_err` line ~734 of `routes/ac.rs`). The
//!     original body MUST be preserved — a same action-digest must not be made to
//!     map to different result bytes (anti cache-poisoning / non-determinism). The
//!     journey requires 409 and then GETs the entry to assert the ORIGINAL body
//!     survived.
//!   - **Adversarial** — P5 read-only PAT cannot WRITE an AC entry → 403; P10
//!     tenant-B cannot GET tenant-A's digest → denied (never tenant-A's bytes).
//!   - **D-7** — `GET /v1/ac/{tenant}` refs list: PUT an AC entry, assert it
//!     enumerates; cross-tenant list isolation.
//!
//! ### Persona-id mapping (matrix vs the frozen `Persona` enum) — FLAGGED
//! The matrix numbers personas P5 (read-only) and P10 (cross-tenant). The frozen
//! `personas.rs` enum encodes the SAME actors as [`Persona::P2ReadOnly`] and
//! [`Persona::P6TenantB`]. This module uses the enum (the contract); the doc
//! comments keep the matrix Pn label in parentheses for traceability.
//!
//! ### Native-AC divergent-body policy — CONFIRMED (M11 fix)
//! Reading `crates/corelink-container/src/routes/ac.rs` `map_err` confirms that
//! `AcHandlerError::DivergentBody` is mapped to **409 Conflict** with body
//! `"divergent body"`. The handler does NOT do last-write-wins for a divergent
//! body — it rejects the overwrite. The prior version of `divergent_body_reput`
//! accepted EITHER 409 OR 200/201 (a tautology). M11 fixes that: require 409
//! strictly, then verify the original body is preserved via a GET.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use serde_json::Value;

use crate::harness::{
    bearer, expect_denied, expect_gate_denied, expect_status, sha256_hex, unique_blob, url_ac,
    url_ac_list, Config, JourneyResult,
};
use crate::personas::Persona;

/// Milliseconds elapsed since `start`.
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// Run the AC journeys — one [`JourneyResult`] per cell.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        update_then_read(cfg, client),
        divergent_body_integrity_guard(cfg, client),
        read_only_cannot_update(cfg, client),
        cross_tenant_read_denied(cfg, client),
        // D-7 — refs list
        list_enumerates_ac_entries(cfg, client),
        list_cross_tenant_isolation(cfg, client),
    ]
}

/// **Happy (matrix S3 happy, P2/P1):** PUT an action-result under a fresh digest,
/// then GET it back and assert the payload round-trips byte-for-byte.
///
/// The digest is a fresh SHA-256 (of unique bytes) so the entry cannot collide
/// with a prior run; the stored *value* is a distinct action-result payload.
fn update_then_read(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "AC: update->read - PUT action-result under digest, GET returns same bytes";
    let start = Instant::now();

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — AC path requires the tenant segment",
        );
    }
    let token = p.token.expect("P1 always has a token");

    // A fresh, canonical 64-hex action digest (opaque key) + a distinct payload.
    let digest = sha256_hex(&unique_blob("corelink-e2e-ac-key"));
    let payload = unique_blob("corelink-e2e-ac-result");
    let url = url_ac(cfg, &p.tenant, &digest);

    // PUT (update) the action-result.
    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(payload.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let put_status = put.status().as_u16();
    if !matches!(put_status, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT got {put_status} (expected 200/201). url={url}"),
        );
    }

    // GET (lookup) — bytes must match the stored action-result exactly.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if let Err(m) = expect_status("AC lookup", get.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }
    match get.bytes() {
        Ok(b) if b.as_ref() == payload.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "lookup bytes mismatch (put {} bytes, got {} bytes)",
                    payload.len(),
                    b.len()
                ),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("lookup body: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Edge / M11 — divergent-body integrity guard (FIXED — no tautology):**
/// PUT the SAME digest twice with DIFFERENT bodies and assert the REAL native-AC
/// contract.
///
/// CONTRACT (read from `routes/ac.rs` `map_err`): `AcHandlerError::DivergentBody`
/// maps to **HTTP 409 Conflict** with body `"divergent body"`. The native AC
/// route does NOT do last-write-wins for a divergent body — it enforces a hard
/// integrity guard so that a proven action result for a given digest can never be
/// silently replaced. The journey:
///   1. PUTs `body_one` → 201 (created).
///   2. PUTs `body_two` to the SAME digest → MUST be 409.
///   3. GETs the entry → MUST return `body_one` exactly (original preserved).
///
/// The prior version of this journey was a TAUTOLOGY: it accepted EITHER 409 OR
/// 200/201, so it passed whether the guard existed or not. This version requires
/// 409 strictly (not LWW) and pins the original-body-preserved GET assertion.
fn divergent_body_integrity_guard(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "AC M11: divergent-body re-PUT — 409 integrity guard + original body preserved (not LWW)";
    let start = Instant::now();

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p.token.expect("P1 always has a token");

    let digest = sha256_hex(&unique_blob("corelink-e2e-ac-divergent-key"));
    let body_one = unique_blob("corelink-e2e-ac-result-ONE");
    let body_two = unique_blob("corelink-e2e-ac-result-TWO");
    let url = url_ac(cfg, &p.tenant, &digest);

    let do_put = |body: &[u8]| -> Result<u16, String> {
        client
            .put(&url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body.to_vec())
            .send()
            .map(|r| r.status().as_u16())
            .map_err(|e| format!("PUT {url}: {e}"))
    };

    // First write — must succeed (fresh entry).
    match do_put(&body_one) {
        Ok(200 | 201) => {}
        Ok(s) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("first PUT got {s} (want 200/201)"),
            )
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // Second write to the SAME digest with DIFFERENT bytes — must be 409.
    // CONTRACT: the native AC route maps DivergentBody → 409. A 200/201 here
    // means the integrity guard is absent (last-write-wins) and the test FAILS.
    match do_put(&body_two) {
        Ok(409) => {} // correct — integrity guard fired
        Ok(200 | 201) => {
            return JourneyResult::fail(
                name,
                ms(start),
                "divergent re-PUT returned 200/201 (last-write-wins) — \
                 the 409 integrity guard is ABSENT; this is a SECURITY regression \
                 (action-result cache-poisoning: a different result can replace a \
                 proven result for the same action digest)"
                    .to_string(),
            )
        }
        Ok(s) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "divergent re-PUT got {s} (expected 409 Conflict from the integrity guard)"
                ),
            )
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // GET: with the 409 guard the ORIGINAL body must be preserved.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if let Err(m) = expect_status(
        "AC lookup after rejected re-PUT",
        get.status().as_u16(),
        200,
    ) {
        return JourneyResult::fail(name, ms(start), m);
    }
    match get.bytes() {
        Ok(b) if b.as_ref() == body_one.as_slice() => JourneyResult::pass(name, ms(start)),
        Ok(b) if b.as_ref() == body_two.as_slice() => JourneyResult::fail(
            name,
            ms(start),
            "GET returned body_two even though the 409 fired — original body was REPLACED \
             (integrity guard is present at the HTTP layer but the handler committed the divergent \
             bytes anyway)"
                .to_string(),
        ),
        Ok(b) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "GET returned {} bytes that match neither body_one nor body_two — corrupt state",
                b.len()
            ),
        ),
        Err(e) => JourneyResult::fail(name, ms(start), format!("GET body: {e}")),
    }
}

/// **Adversarial (matrix P5 → frozen [`Persona::P2ReadOnly`]):** a read-only PAT
/// must NOT be able to UPDATE an AC entry — the write must be denied (the route
/// returns 403 "insufficient scope"; any of 401/403/404 is an acceptable deny).
/// A deny-journey that passes on a 2xx is a BUG.
fn read_only_cannot_update(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "AC: read-only PAT cannot update (P2/RO) - PUT -> denied (403)";
    let start = Instant::now();

    let ro = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = ro.token.expect("P2 always has a token");

    let digest = sha256_hex(&unique_blob("corelink-e2e-ac-ro-write-key"));
    let payload = unique_blob("corelink-e2e-ac-ro-write-attempt");
    let url = url_ac(cfg, &ro.tenant, &digest);

    let resp = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(payload)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if matches!(status, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("SECURITY: read-only PAT WROTE an AC entry (got {status}) — scope bypass"),
        );
    }
    if let Err(m) = expect_gate_denied("RO AC update", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}

/// **Adversarial (matrix P10 → frozen [`Persona::P6TenantB`]):** tenant A writes
/// an AC entry; tenant B requests the SAME digest under tenant B's path and must
/// be DENIED — never a 200 carrying tenant A's bytes. This is the multi-tenant
/// AC isolation ship gate.
fn cross_tenant_read_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "AC: cross-tenant isolation - A writes; B GETs same digest -> denied (no leak)";
    let start = Instant::now();

    let a = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_a = a.token.expect("P1 has a token");
    let token_b = b.token.expect("P6 has a token");

    let digest = sha256_hex(&unique_blob("corelink-e2e-ac-iso-key"));
    let secret = unique_blob("tenant-a-ac-secret");

    // Tenant A writes under tenant A's path.
    let url_a = url_ac(cfg, &a.tenant, &digest);
    let put = match client
        .put(&url_a)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(secret.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("A PUT {url_a}: {e}")),
    };
    if !matches!(put.status().as_u16(), 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "A PUT got {} — cannot verify isolation without a successful write",
                put.status()
            ),
        );
    }

    // Tenant B requests the SAME digest under tenant B's path — must be denied
    // and must NEVER return tenant A's bytes.
    let url_b = url_ac(cfg, &b.tenant, &digest);
    let get = match client
        .get(&url_b)
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B GET {url_b}: {e}")),
    };
    let status = get.status().as_u16();
    if status == 200 {
        let body = get.bytes().map(|b| b.to_vec()).unwrap_or_default();
        if body.as_slice() == secret.as_slice() {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "SECURITY: tenant B read tenant A's AC bytes ({} bytes) — cross-tenant leak",
                    body.len()
                ),
            );
        }
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "tenant B got 200 ({} bytes) on A's digest — must be denied (no cross-tenant 200)",
                body.len()
            ),
        );
    }
    if let Err(m) = expect_denied("tenant B cross-AC-read", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}

// ── D-7: AC refs list ─────────────────────────────────────────────────────────

/// **D-7a — list enumerates AC entries:** PUT an AC entry, then
/// `GET /v1/ac/{tenant}` and assert the digest appears in the `refs` array.
/// Response shape: `{"refs":[{"ref_key","updated_at","size"}…],"next_cursor":<null>}`.
///
/// `ref_key` is the opaque action digest used as the PUT path segment.
fn list_enumerates_ac_entries(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "AC D-7: GET /v1/ac/{tenant} - list enumerates PUT entries (ref_key present, size correct)";
    let start = Instant::now();

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p.token.expect("P1 always has a token");

    // PUT an AC entry.
    let payload = unique_blob("corelink-e2e-ac-list-entry");
    let digest = sha256_hex(&unique_blob("corelink-e2e-ac-list-key"));
    let put_url = url_ac(cfg, &p.tenant, &digest);
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(payload.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {put_url}: {e}")),
    };
    if !matches!(put.status().as_u16(), 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT AC entry got {} (expected 200/201)", put.status()),
        );
    }

    // GET the list. Use a generous limit so the fresh entry is visible.
    let list_url = format!("{}?limit=1000", url_ac_list(cfg, &p.tenant));
    let resp = match client
        .get(&list_url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {list_url}: {e}")),
    };
    if resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "GET /ac list got {} (expected 200). url={list_url}",
                resp.status()
            ),
        );
    }
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("AC list response body: {e}"))
        }
    };
    let parsed: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("AC list response not JSON: {e}"))
        }
    };
    let refs_arr = match parsed.get("refs").and_then(Value::as_array) {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("AC list response missing \"refs\" array; got: {parsed}"),
            )
        }
    };

    // The entry we PUT must appear in the list.
    let entry = refs_arr
        .iter()
        .find(|e| e.get("ref_key").and_then(Value::as_str) == Some(digest.as_str()));
    let entry = match entry {
        Some(e) => e,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "PUT AC digest {digest} not found in /ac list ({} entries)",
                    refs_arr.len()
                ),
            )
        }
    };

    // The `size` field must match the payload length.
    let size = entry.get("size").and_then(Value::as_u64).unwrap_or(0);
    let expected_size = payload.len() as u64;
    if size != expected_size {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "AC list entry for {digest}: size={size} but payload was {expected_size} bytes"
            ),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// **D-7b — list cross-tenant isolation:** tenant A writes an AC entry; tenant B
/// lists their own namespace and must NOT see tenant A's digest. Complements the
/// per-entry GET isolation journey for the list surface.
fn list_cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "AC D-7: list cross-tenant isolation — A's refs must not appear in B's list";
    let start = Instant::now();

    let a = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_a = a.token.expect("P1 has a token");
    let token_b = b.token.expect("P6 has a token");

    // A PUTs a uniquely-named AC entry.
    let payload_a = unique_blob("ac-list-iso-a");
    let digest_a = sha256_hex(&unique_blob("ac-list-iso-a-key"));
    let put_url = url_ac(cfg, &a.tenant, &digest_a);
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(payload_a)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("A PUT {put_url}: {e}")),
    };
    if !matches!(put.status().as_u16(), 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "A PUT got {} — cannot verify AC list isolation",
                put.status()
            ),
        );
    }

    // B lists their own namespace — must not see A's digest.
    let list_url_b = format!("{}?limit=1000", url_ac_list(cfg, &b.tenant));
    let resp = match client
        .get(&list_url_b)
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B GET {list_url_b}: {e}")),
    };
    // If B's list is outright denied (403/401) that is also acceptable isolation.
    let status = resp.status().as_u16();
    if matches!(status, 401 | 403) {
        return JourneyResult::pass(name, ms(start));
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("B GET AC list got {status} (expected 200 or deny). url={list_url_b}"),
        );
    }
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("B AC list response body: {e}"))
        }
    };
    let parsed: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("B AC list response not JSON: {e}"),
            )
        }
    };
    let refs_arr = match parsed.get("refs").and_then(Value::as_array) {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("B AC list response missing \"refs\" array; got: {parsed}"),
            )
        }
    };
    if refs_arr
        .iter()
        .any(|e| e.get("ref_key").and_then(Value::as_str) == Some(digest_a.as_str()))
    {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("SECURITY: tenant B's AC list contains tenant A's ref_key {digest_a} — cross-tenant leak"),
        );
    }

    JourneyResult::pass(name, ms(start))
}
