//! Native CAS journeys — `/v1/cas/{tenant}/{hash}`.
//!
//! Migrated from the old single-file suite (journeys 3 + 5). URLs CORRECTED:
//! the old suite used `/v1/cas/blobs/{hash}/{size}` which does NOT exist — the
//! real native CAS route is `/v1/cas/{tenant}/{hash}` (the blob/size shape is
//! the BAZEL surface). The stale "expected RED / P0" framing is flipped to the
//! live contract.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, expect_denied, blake3_hex, unique_blob, url_cas, Config, JourneyResult,
};
use crate::personas::Persona;

/// Run the CAS journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![cache_miss_then_hit(cfg, client), tenant_isolation(cfg, client)]
}

/// Cache miss→hit: PUT a unique blob, GET the bytes back, GET again — the round
/// trip is the customer's core value. Content-addressed, so the hash is the key.
fn cache_miss_then_hit(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CAS: cache miss→hit — PUT blob, GET bytes match, 2nd GET still match";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — CAS path requires the tenant segment",
        );
    }
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("corelink-e2e-cas");
    let hash = blake3_hex(&blob);
    let url = url_cas(cfg, &p1.tenant, &hash);

    // PUT.
    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
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
            format!("PUT blob got {put_status} (expected 200/201). url={url}"),
        );
    }

    // First GET — bytes must match.
    let get1 = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET#1 {url}: {e}")),
    };
    if get1.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET#1 got {} (expected 200)", get1.status()),
        );
    }
    match get1.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET#1 bytes mismatch (put {} got {})", blob.len(), b.len()),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET#1 body: {e}")),
    }

    // Second GET — must still serve the same bytes (durability, not ephemeral).
    let get2 = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET#2 {url}: {e}")),
    };
    if get2.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET#2 got {} (expected 200)", get2.status()),
        );
    }
    match get2.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(_) => {
            return JourneyResult::fail(name, ms(start), "GET#2 bytes mismatch".to_string())
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET#2 body: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

/// Tenant isolation (adversarial): tenant A PUTs a secret blob; tenant B GETs
/// the SAME content address and must be DENIED (never 200 with A's bytes). This
/// is the multi-tenancy security ship gate.
fn tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CAS: tenant isolation — A PUTs; B GETs same addr → denied (no leak)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

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

    let blob = unique_blob("tenant-a-secret");
    let hash = blake3_hex(&blob);

    // Tenant A PUTs under tenant A's path.
    let url_a = url_cas(cfg, &a.tenant, &hash);
    let put = match client
        .put(&url_a)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
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

    // Tenant B GETs the SAME content address under tenant B's path — must deny.
    let url_b = url_cas(cfg, &b.tenant, &hash);
    let get = match client.get(&url_b).header(AUTHORIZATION, bearer(token_b)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B GET {url_b}: {e}")),
    };
    let get_status = get.status().as_u16();
    if get_status == 200 {
        let n = get.bytes().map(|b| b.len()).unwrap_or(0);
        return JourneyResult::fail(
            name,
            ms(start),
            format!("SECURITY: tenant B got 200 ({n} bytes) from A's content address — cross-tenant leak"),
        );
    }
    if let Err(m) = expect_denied("tenant B cross-read", get_status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}
