//! Native CAS journeys — `/v1/cas/{tenant}/{hash}`.
//!
//! Migrated from the old single-file suite (journeys 3 + 5). URLs CORRECTED:
//! the old suite used `/v1/cas/blobs/{hash}/{size}` which does NOT exist — the
//! real native CAS route is `/v1/cas/{tenant}/{hash}` (the blob/size shape is
//! the BAZEL surface). The stale "expected RED / P0" framing is flipped to the
//! live contract.
//!
//! ## M9 — batch plane (the hot path the latency memo built)
//!
//! Three routes collapse N single-object round-trips into one request:
//!   - `POST /v1/cas/{tenant}/batch`        — bulk write (length-framed)
//!   - `POST /v1/cas/{tenant}/batch-read`   — bulk read (NDJSON request, length-framed response)
//!   - `POST /v1/cas/{tenant}/batch-exists` — bulk HEAD-class existence probe (NDJSON both sides)
//!
//! ### Batch-write wire format (FROZEN — `application/x-hugit-cas-batch`)
//! ```text
//! {"hash":"<blake3-64hex>","len":<u64>}\n   <- manifest line per object
//! ...
//! \n                                          <- blank line = manifest terminator
//! <raw bytes of object 0><raw bytes of object 1>...  <- length-framed payload
//! ```
//! Response 200: JSON array `[{"hash":"…","status":"created|exists|error","error":<null|msg>}]`
//! in manifest order.
//!
//! ### Batch-read request / response (FROZEN)
//! Request (`application/x-ndjson`): NDJSON `{"hash":"<blake3>"}` lines.
//! Response 200: NDJSON manifest `{"hash":"…","len":<u64>,"status":"ok|absent|gone"}` lines
//! terminated by `\n`, then the concatenated raw bytes of `ok` objects in manifest order.
//!
//! ### Batch-exists request / response (FROZEN)
//! Request (`application/x-ndjson`): NDJSON `{"hash":"<blake3>"}` lines.
//! Response 200: JSON array `[{"hash":"…","present":<bool>}]` in request order.
//!
//! ## D-8 — list (`GET /v1/cas/{tenant}`)
//! Response 200: `{"blobs":[{"hash","size","created_at"}…],"next_cursor":<opaque|null>}`.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::harness::{
    bearer, blake3_hex, expect_denied, unique_blob, url_cas, url_cas_batch, url_cas_batch_exists,
    url_cas_batch_read, url_cas_list, Config, JourneyResult,
};
use crate::personas::Persona;

/// FROZEN batch content-type (upload route only).
const BATCH_CT: &str = "application/x-hugit-cas-batch";
/// FROZEN NDJSON content-type (read-side batch routes).
const NDJSON_CT: &str = "application/x-ndjson";

/// Run the CAS journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        cache_miss_then_hit(cfg, client),
        tenant_isolation(cfg, client),
        // M9 — batch plane
        batch_write_then_get(cfg, client),
        batch_exists_present_and_missing(cfg, client),
        batch_read_bytes_match(cfg, client),
        // D-8 — list
        list_enumerates_uploaded_blobs(cfg, client),
        list_cross_tenant_isolation(cfg, client),
    ]
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
        Ok(_) => return JourneyResult::fail(name, ms(start), "GET#2 bytes mismatch".to_string()),
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
    let get = match client
        .get(&url_b)
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
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

// ── M9: batch plane ────────────────────────────────────────────────────────────

/// **M9a — batch write then GET:** POST N blobs in one `application/x-hugit-cas-batch`
/// request; assert the response reports `created`/`exists` per object and that
/// each blob is then GET-able with matching bytes.
///
/// Wire format: manifest NDJSON lines `{"hash":"…","len":<u64>}`, blank line
/// terminator (`\n\n`), then the raw bytes concatenated in order.
/// Response 200: JSON array `[{"hash":"…","status":"created|exists|error","error":…}]`.
fn batch_write_then_get(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CAS M9a: batch write (POST /batch) - N blobs written, each GET returns exact bytes";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — batch path requires tenant segment",
        );
    }
    let token = p.token.expect("P1 always has a token");

    // Build 3 distinct blobs.
    let blobs: Vec<Vec<u8>> = (0..3)
        .map(|i| unique_blob(&format!("cas-batch-write-{i}")))
        .collect();
    let hashes: Vec<String> = blobs.iter().map(|b| blake3_hex(b)).collect();

    // Build the length-framed batch body: manifest lines then blank line then payload.
    let mut body: Vec<u8> = Vec::new();
    for (hash, blob) in hashes.iter().zip(blobs.iter()) {
        let line = format!("{{\"hash\":\"{hash}\",\"len\":{}}}\n", blob.len());
        body.extend_from_slice(line.as_bytes());
    }
    body.push(b'\n'); // blank line — manifest terminator
    for blob in &blobs {
        body.extend_from_slice(blob);
    }

    let url = url_cas_batch(cfg, &p.tenant);
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, BATCH_CT)
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST batch {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("POST /batch got {status} (expected 200). url={url}"),
        );
    }
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("batch response body: {e}")),
    };
    let results: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("batch response not JSON: {e}"))
        }
    };
    let arr = match results.as_array() {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "batch response is not a JSON array".to_string(),
            )
        }
    };
    if arr.len() != hashes.len() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "batch response has {} entries, expected {}",
                arr.len(),
                hashes.len()
            ),
        );
    }
    for (i, entry) in arr.iter().enumerate() {
        let got_hash = entry.get("hash").and_then(Value::as_str).unwrap_or("");
        if got_hash != hashes[i] {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "batch result[{i}] hash mismatch: got {got_hash:?} want {:?}",
                    hashes[i]
                ),
            );
        }
        let st = entry.get("status").and_then(Value::as_str).unwrap_or("");
        if !matches!(st, "created" | "exists") {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("batch result[{i}] status={st:?} (expected \"created\" or \"exists\"); error={:?}", entry.get("error")),
            );
        }
    }

    // Now GET each blob back and assert bytes match exactly.
    for (hash, blob) in hashes.iter().zip(blobs.iter()) {
        let get_url = url_cas(cfg, &p.tenant, hash);
        let get = match client
            .get(&get_url)
            .header(AUTHORIZATION, bearer(token))
            .send()
        {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {get_url}: {e}")),
        };
        if get.status().as_u16() != 200 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "GET blob after batch write got {} (expected 200). url={get_url}",
                    get.status()
                ),
            );
        }
        match get.bytes() {
            Ok(b) if b.as_ref() == blob.as_slice() => {}
            Ok(b) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "GET {hash} bytes mismatch after batch write (put {} bytes, got {} bytes)",
                        blob.len(),
                        b.len()
                    ),
                )
            }
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET body {hash}: {e}")),
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **M9b — batch-exists present/missing split:** PUT one known blob, then probe
/// `POST /batch-exists` with a mix of that known hash + a fresh never-PUT hash.
/// Assert the response reports `"present":true` for the known blob and
/// `"present":false` for the unknown one, and that the order matches the request.
fn batch_exists_present_and_missing(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CAS M9b: batch-exists (POST /batch-exists) - correct present/missing split";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p.token.expect("P1 always has a token");

    // PUT one blob so it is definitely present.
    let present_blob = unique_blob("cas-batch-exists-present");
    let present_hash = blake3_hex(&present_blob);
    let put_url = url_cas(cfg, &p.tenant, &present_hash);
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(present_blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {put_url}: {e}")),
    };
    if !matches!(put.status().as_u16(), 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "PUT blob for exists probe got {} (expected 200/201)",
                put.status()
            ),
        );
    }

    // A blob that was never PUT — its hash is the blake3 of a fresh unique payload
    // that we deliberately do NOT upload.
    let absent_blob = unique_blob("cas-batch-exists-absent");
    let absent_hash = blake3_hex(&absent_blob);

    // Build NDJSON request: present hash first, absent second.
    let ndjson = format!("{{\"hash\":\"{present_hash}\"}}\n{{\"hash\":\"{absent_hash}\"}}\n");

    let exists_url = url_cas_batch_exists(cfg, &p.tenant);
    let resp = match client
        .post(&exists_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, NDJSON_CT)
        .body(ndjson.into_bytes())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("POST batch-exists {exists_url}: {e}"),
            )
        }
    };
    if resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "POST /batch-exists got {} (expected 200). url={exists_url}",
                resp.status()
            ),
        );
    }
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("batch-exists response body: {e}"))
        }
    };
    let results: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("batch-exists response not JSON: {e}"),
            )
        }
    };
    let arr = match results.as_array() {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "batch-exists response is not a JSON array".to_string(),
            )
        }
    };
    if arr.len() != 2 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("batch-exists returned {} entries, expected 2", arr.len()),
        );
    }

    // Entry 0: the known-present hash — must be present:true.
    let e0 = &arr[0];
    let h0 = e0.get("hash").and_then(Value::as_str).unwrap_or("");
    if h0 != present_hash {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("batch-exists result[0] hash={h0:?} want {present_hash:?} (order mismatch)"),
        );
    }
    match e0.get("present").and_then(Value::as_bool) {
        Some(true) => {}
        Some(false) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "batch-exists result[0] present=false for a blob we just PUT ({present_hash})"
                ),
            )
        }
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "batch-exists result[0] missing \"present\" field".to_string(),
            )
        }
    }

    // Entry 1: the never-PUT hash — must be present:false.
    let e1 = &arr[1];
    let h1 = e1.get("hash").and_then(Value::as_str).unwrap_or("");
    if h1 != absent_hash {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("batch-exists result[1] hash={h1:?} want {absent_hash:?} (order mismatch)"),
        );
    }
    match e1.get("present").and_then(Value::as_bool) {
        Some(false) => {}
        Some(true) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                "batch-exists result[1] present=true for a hash that was never PUT ({absent_hash})"
            ),
            )
        }
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "batch-exists result[1] missing \"present\" field".to_string(),
            )
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **M9c — batch-read bytes match:** write N blobs, then bulk-read them all in
/// one `POST /batch-read` and assert the manifest + payload exactly match the
/// PUT bytes. The response is a length-framed stream: NDJSON manifest lines
/// (`{"hash":"…","len":<u64>,"status":"ok"}`) terminated by a blank line,
/// followed by the raw bytes of each `ok` object in manifest order.
fn batch_read_bytes_match(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "CAS M9c: batch-read (POST /batch-read) - manifest correct, bytes match PUT contents";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p.token.expect("P1 always has a token");

    // Write 2 blobs via the single-object PUT so they are definitely present.
    let blobs: Vec<Vec<u8>> = (0..2)
        .map(|i| unique_blob(&format!("cas-batch-read-{i}")))
        .collect();
    let hashes: Vec<String> = blobs.iter().map(|b| blake3_hex(b)).collect();
    for (hash, blob) in hashes.iter().zip(blobs.iter()) {
        let put_url = url_cas(cfg, &p.tenant, hash);
        let put = match client
            .put(&put_url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(blob.clone())
            .send()
        {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {put_url}: {e}")),
        };
        if !matches!(put.status().as_u16(), 200 | 201) {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PUT blob {hash} got {} (expected 200/201)", put.status()),
            );
        }
    }

    // Build the NDJSON request body.
    let ndjson: String = hashes
        .iter()
        .map(|h| format!("{{\"hash\":\"{h}\"}}\n"))
        .collect();

    let read_url = url_cas_batch_read(cfg, &p.tenant);
    let resp = match client
        .post(&read_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, NDJSON_CT)
        .body(ndjson.into_bytes())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("POST batch-read {read_url}: {e}"))
        }
    };
    if resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "POST /batch-read got {} (expected 200). url={read_url}",
                resp.status()
            ),
        );
    }
    let raw = match resp.bytes() {
        Ok(b) => b.to_vec(),
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("batch-read response body: {e}"))
        }
    };

    // Split at the first `\n\n`: manifest is before it, payload is after.
    let sep = raw.windows(2).position(|w| w == b"\n\n");
    let sep = match sep {
        Some(s) => s,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "batch-read response missing \\n\\n manifest terminator".to_string(),
            )
        }
    };
    let manifest_bytes = &raw[..sep + 1];
    let payload = raw.get(sep + 2..).unwrap_or(&[]);
    let manifest_text = match std::str::from_utf8(manifest_bytes) {
        Ok(s) => s,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("batch-read manifest not UTF-8: {e}"),
            )
        }
    };

    // Parse each NDJSON manifest line as a serde_json::Value to avoid a
    // local derive — this crate only has serde_json, not serde+derive.
    let mut entries: Vec<Value> = Vec::new();
    for line in manifest_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(v) => entries.push(v),
            Err(e) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("malformed manifest line {line:?}: {e}"),
                )
            }
        }
    }
    if entries.len() != hashes.len() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "batch-read manifest has {} entries, expected {}",
                entries.len(),
                hashes.len()
            ),
        );
    }

    // Walk the manifest + payload together and assert byte-for-byte correctness.
    let mut offset: usize = 0;
    for (i, (entry, expected)) in entries.iter().zip(blobs.iter()).enumerate() {
        let got_hash = entry.get("hash").and_then(Value::as_str).unwrap_or("");
        if got_hash != hashes[i] {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("manifest[{i}] hash={got_hash:?} want {:?}", hashes[i]),
            );
        }
        let status = entry.get("status").and_then(Value::as_str).unwrap_or("");
        if status != "ok" {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("manifest[{i}] status={status:?} (expected \"ok\")"),
            );
        }
        let len = entry.get("len").and_then(Value::as_u64).unwrap_or(0) as usize;
        let end = offset + len;
        if end > payload.len() {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "manifest[{i}] claims {len} bytes but payload only has {} remaining",
                    payload.len() - offset
                ),
            );
        }
        let got = &payload[offset..end];
        if got != expected.as_slice() {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("manifest[{i}] bytes mismatch (put {} bytes, got {} bytes at payload offset {offset})", expected.len(), len),
            );
        }
        offset = end;
    }
    if offset != payload.len() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "batch-read payload has {} trailing bytes beyond the manifest entries",
                payload.len() - offset
            ),
        );
    }

    JourneyResult::pass(name, ms(start))
}

// ── D-8: CAS list ─────────────────────────────────────────────────────────────

/// **D-8a — list enumerates uploaded blobs:** PUT a couple of blobs, then
/// `GET /v1/cas/{tenant}` and assert both hashes appear in the `blobs` array.
/// Response shape: `{"blobs":[{"hash","size","created_at"}…],"next_cursor":<null>}`.
fn list_enumerates_uploaded_blobs(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "CAS D-8: GET /v1/cas/{tenant} - list enumerates PUTted blobs (hash present, size correct)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p.token.expect("P1 always has a token");

    // PUT 2 blobs.
    let blobs: Vec<Vec<u8>> = (0..2)
        .map(|i| unique_blob(&format!("cas-list-{i}")))
        .collect();
    let hashes: Vec<String> = blobs.iter().map(|b| blake3_hex(b)).collect();
    for (hash, blob) in hashes.iter().zip(blobs.iter()) {
        let put_url = url_cas(cfg, &p.tenant, hash);
        let put = match client
            .put(&put_url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(blob.clone())
            .send()
        {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {put_url}: {e}")),
        };
        if !matches!(put.status().as_u16(), 200 | 201) {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PUT blob {hash} got {} (expected 200/201)", put.status()),
            );
        }
    }

    // GET the list. Use a generous limit so fresh blobs are visible on a shared tenant.
    let list_url = format!("{}?limit=1000", url_cas_list(cfg, &p.tenant));
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
                "GET /cas list got {} (expected 200). url={list_url}",
                resp.status()
            ),
        );
    }
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("list response body: {e}")),
    };
    let parsed: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("list response not JSON: {e}"))
        }
    };
    let blobs_arr = match parsed.get("blobs").and_then(Value::as_array) {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("list response missing \"blobs\" array; got: {parsed}"),
            )
        }
    };

    // Assert each hash appears in the list.
    for (i, hash) in hashes.iter().enumerate() {
        let found = blobs_arr
            .iter()
            .any(|entry| entry.get("hash").and_then(Value::as_str) == Some(hash.as_str()));
        if !found {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "PUT blob[{i}] hash {hash} not found in /cas list ({} entries)",
                    blobs_arr.len()
                ),
            );
        }
        // Also assert the size is correct for each entry we can find.
        if let Some(entry) = blobs_arr
            .iter()
            .find(|e| e.get("hash").and_then(Value::as_str) == Some(hash.as_str()))
        {
            let size = entry.get("size").and_then(Value::as_u64).unwrap_or(0);
            let expected_size = blobs[i].len() as u64;
            if size != expected_size {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "list entry for {hash}: size={size} but blob was {expected_size} bytes"
                    ),
                );
            }
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **D-8b — list cross-tenant isolation:** tenant A PUTs a blob; tenant B lists
/// their own namespace and must NOT see tenant A's hash. This is the list-surface
/// isolation gate (complements the per-blob GET isolation journey).
fn list_cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CAS D-8: list cross-tenant isolation — A's blobs must not appear in B's list";
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

    // A PUTs a uniquely-named blob.
    let blob_a = unique_blob("cas-list-iso-a");
    let hash_a = blake3_hex(&blob_a);
    let put_url = url_cas(cfg, &a.tenant, &hash_a);
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob_a)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("A PUT {put_url}: {e}")),
    };
    if !matches!(put.status().as_u16(), 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("A PUT got {} — cannot verify list isolation", put.status()),
        );
    }

    // B lists their own namespace — must not see A's hash.
    let list_url_b = format!("{}?limit=1000", url_cas_list(cfg, &b.tenant));
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
            format!("B GET list got {status} (expected 200 or deny). url={list_url_b}"),
        );
    }
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("B list response body: {e}"))
        }
    };
    let parsed: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("B list response not JSON: {e}"))
        }
    };
    let blobs_arr = match parsed.get("blobs").and_then(Value::as_array) {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("B list response missing \"blobs\" array; got: {parsed}"),
            )
        }
    };
    if blobs_arr
        .iter()
        .any(|e| e.get("hash").and_then(Value::as_str) == Some(hash_a.as_str()))
    {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("SECURITY: tenant B's list contains tenant A's blob hash {hash_a} — cross-tenant leak"),
        );
    }

    JourneyResult::pass(name, ms(start))
}
