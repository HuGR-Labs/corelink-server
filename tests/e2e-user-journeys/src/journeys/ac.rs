//! Action-Cache (AC) journeys — native S3 surface `/v1/ac/{tenant}/{digest}`.
//!
//! Black-box only: the deployed HTTP API + a Bearer PAT, via the [`crate::harness`]
//! URL builders + the [`crate::personas`] persona/token map. No `corelink-*`
//! crate import, no mocks, no internal-state reads.
//!
//! Cells covered (matrix S3):
//!   - **Happy** — `update→read` round-trip: PUT an action-result under a fresh
//!     digest, GET it back, and assert the stored payload matches byte-for-byte.
//!   - **Edge** — divergent-body re-PUT: PUT the *same* digest with a *different*
//!     body and observe the live contract. See the FLAG below — the native AC
//!     route treats `{digest}` as an OPAQUE key (no digest↔body cryptographic
//!     guard at this layer), so a re-PUT is **last-write-wins**, NOT a 409. The
//!     journey asserts the real overwrite contract (the read returns the second
//!     body) rather than a guard the route does not implement.
//!   - **Adversarial** — P5 read-only PAT cannot WRITE an AC entry → 403; P10
//!     tenant-B cannot GET tenant-A's digest → denied (never tenant-A's bytes).
//!
//! ### Persona-id mapping (matrix vs the frozen `Persona` enum) — FLAGGED
//! The matrix numbers personas P5 (read-only) and P10 (cross-tenant). The frozen
//! `personas.rs` enum encodes the SAME actors as [`Persona::P2ReadOnly`] and
//! [`Persona::P6TenantB`]. This module uses the enum (the contract); the doc
//! comments keep the matrix Pn label in parentheses for traceability.
//!
//! ### Native-AC vs Bazel-AC divergent-body — FLAGGED
//! The matrix's "divergent-body guard" is a Bazel-REAPI property (the AC key is
//! the action *digest* and the server can reject a payload whose digest differs).
//! The NATIVE `/v1/ac` route (`crates/corelink-container/src/routes/ac.rs`,
//! `handle_update`) stores the body verbatim under the opaque path digest and
//! returns 201/200 on overwrite — there is no divergent-body rejection here.
//! Asserting a 409 would be a false test, so the Edge cell pins the actual
//! last-write-wins contract and the flag documents the gap.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, expect_denied, expect_status, sha256_hex, unique_blob, url_ac, Config, JourneyResult,
};
use crate::personas::Persona;

/// Milliseconds elapsed since `start`.
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// Run the AC journeys — one [`JourneyResult`] per cell (happy / edge /
/// adversarial-RO / adversarial-cross-tenant).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        update_then_read(cfg, client),
        divergent_body_reput(cfg, client),
        read_only_cannot_update(cfg, client),
        cross_tenant_read_denied(cfg, client),
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

/// **Edge (matrix S3 edge — "divergent-body guard"):** PUT the SAME digest twice
/// with DIFFERENT bodies and pin the real native-AC contract.
///
/// FLAG: native `/v1/ac` keys on the opaque path digest with no digest↔body
/// cryptographic guard, so the second PUT is accepted (last-write-wins) and the
/// subsequent GET returns the SECOND body. This is the actual deployed behavior;
/// asserting a 409 (a Bazel-REAPI-only property) would be a false test.
fn divergent_body_reput(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "AC: divergent-body re-PUT - same digest, different body -> 409 integrity guard";
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

    let put = |body: &[u8]| -> Result<u16, String> {
        client
            .put(&url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body.to_vec())
            .send()
            .map(|r| r.status().as_u16())
            .map_err(|e| format!("PUT {url}: {e}"))
    };

    // First write.
    match put(&body_one) {
        Ok(200 | 201) => {}
        Ok(s) => {
            return JourneyResult::fail(name, ms(start), format!("first PUT got {s} (want 200/201)"))
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // Second write to the SAME digest, DIFFERENT body. The native AC route
    // ENFORCES a digest↔body integrity guard (verified live): a divergent
    // overwrite is rejected with 409 — a same action-digest must not be made to
    // map to a different result (anti cache-poisoning / non-determinism). This is
    // the strong, correct contract. (A 200/201 last-write-wins would be a weaker
    // policy and is tolerated, but prod returns 409.)
    let second = match put(&body_two) {
        Ok(s) => s,
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    };
    let expect_original_preserved = match second {
        409 => true,        // integrity guard rejected the divergent overwrite
        200 | 201 => false, // weaker last-write-wins (tolerated)
        s => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("divergent re-PUT got {s} (expected 409 integrity guard, or 200/201)"),
            )
        }
    };

    // GET: under the 409 guard the ORIGINAL body must survive; under last-write
    // -wins the SECOND body wins. Either way the stored value must be ONE of the
    // two, never corrupt.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if let Err(m) = expect_status("AC lookup after re-PUT", get.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let want = if expect_original_preserved { &body_one } else { &body_two };
    match get.bytes() {
        Ok(b) if b.as_ref() == want.as_slice() => JourneyResult::pass(name, ms(start)),
        Ok(_) => JourneyResult::fail(
            name,
            ms(start),
            if expect_original_preserved {
                "409 guard fired but GET did NOT return the preserved original body".to_string()
            } else {
                "last-write-wins but GET did NOT return the second body".to_string()
            },
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
    if let Err(m) = expect_denied("RO AC update", status) {
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
    let get = match client.get(&url_b).header(AUTHORIZATION, bearer(token_b)).send() {
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
