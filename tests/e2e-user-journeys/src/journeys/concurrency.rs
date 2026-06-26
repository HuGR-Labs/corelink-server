//! Concurrency / race-correctness journeys — real clients hammer the native CAS
//! surface (`/v1/cas/{tenant}/{hash}`) in PARALLEL.
//!
//! Black-box only: the deployed HTTP API + Bearer PATs, via the
//! [`crate::harness`] URL builders + the [`crate::personas`] persona/token map.
//! No `corelink-*` crate import, no mocks, no internal-state reads.
//!
//! Real customers don't drive a cache one request at a time — a build farm fires
//! N parallel writers against the SAME content-addressed blob (every shard of a
//! distributed build computed the identical artifact) AND many DIFFERENT blobs at
//! once. Content-addressing makes the same-blob race SAFE by construction (the
//! address IS the BLAKE3 of the bytes, so two writers can only ever agree), and
//! the storage layer must keep that promise under contention: no torn read, no
//! corruption, no lost or double-counted write.
//!
//! All journeys use **P1 (read+write)** against the PRIMARY tenant only — they
//! never touch another tenant, never delete real data, never drive a quota to its
//! ceiling, never charge. They GATE if P1's PAT or the tenant segment is absent.
//!
//! Cells (each a PASS/GATED/FAIL [`JourneyResult`]):
//!   - **same-blob fan-out** — N concurrent PUTs of the SAME blob all succeed
//!     (idempotent / content-addressed) and a subsequent GET returns the exact
//!     bytes (no corruption / partial write wins).
//!   - **distinct-blob fan-out** — N concurrent PUTs of N DIFFERENT blobs all
//!     store, and each is independently + correctly readable afterwards.
//!   - **read-during-write** — a GET racing a PUT of the same key never observes a
//!     torn/partial body (either the absent/old state OR the full new bytes —
//!     never garbage).
//!   - **byte-accounting under concurrency** (best-effort, GATED-safe) — concurrent
//!     writes don't double-count or lose the charge; probed via the customer usage
//!     surface, GATED when usage is not observable black-box.
//!
//! N is kept MODEST (8): the runners are the founder's Mac and prod is shared —
//! this is a correctness probe, never a load test / DoS.

use std::sync::Mutex;
use std::thread;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, blake3_hex, unique_blob, url_cas, url_customer, Config, JourneyResult, TokenKind,
};
use crate::personas::Persona;

/// Modest concurrency width. Enough to expose a race; small enough to never load
/// the shared Mac / shared prod (this is a correctness probe, NOT a load test).
const N: usize = 8;

/// Milliseconds elapsed since `start`.
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// Run the concurrency journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        same_blob_fan_out(cfg, client),
        distinct_blobs_fan_out(cfg, client),
        read_during_write(cfg, client),
        byte_accounting_under_concurrency(cfg, client),
    ]
}

/// One concurrent PUT's outcome: either a status code or a transport-error
/// message. Collected from every worker thread so a single failure is reportable.
type PutOutcome = Result<u16, String>;

/// Fire `n` concurrent PUTs of `(url, body)` pairs with `token`, returning each
/// worker's outcome in submission order. Uses scoped threads so the borrowed
/// `client`/`token`/`cfg` need no `'static` bound and nothing is leaked.
fn concurrent_puts(
    client: &Client,
    token: &str,
    jobs: &[(String, Vec<u8>)],
) -> Vec<PutOutcome> {
    let results: Vec<Mutex<Option<PutOutcome>>> =
        (0..jobs.len()).map(|_| Mutex::new(None)).collect();

    thread::scope(|scope| {
        for (i, (url, body)) in jobs.iter().enumerate() {
            let slot = &results[i];
            scope.spawn(move || {
                let outcome = client
                    .put(url)
                    .header(AUTHORIZATION, bearer(token))
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(body.clone())
                    .send()
                    .map(|r| r.status().as_u16())
                    .map_err(|e| format!("PUT {url}: {e}"));
                *slot.lock().expect("put slot poisoned") = Some(outcome);
            });
        }
    });

    results
        .into_iter()
        .map(|m| m.into_inner().expect("put slot poisoned").expect("worker set slot"))
        .collect()
}

/// **Same-blob fan-out:** N writers PUT the IDENTICAL content-addressed blob at
/// once. Because the address is the BLAKE3 of the bytes, every writer agrees on
/// the key + payload, so all N must succeed (idempotent / content-addressed — a
/// real build farm where every shard computed the same artifact). After the
/// storm a single GET must return the EXACT bytes — no partial write won, no
/// interleave corrupted the object.
fn same_blob_fan_out(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CONC CAS: N concurrent PUTs of SAME blob — all succeed, GET bytes exact";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("conc-same-blob");
    let hash = blake3_hex(&blob);
    let url = url_cas(cfg, &p1.tenant, &hash);

    // N identical PUTs, fired concurrently.
    let jobs: Vec<(String, Vec<u8>)> = (0..N).map(|_| (url.clone(), blob.clone())).collect();
    let outcomes = concurrent_puts(client, token, &jobs);

    for (i, o) in outcomes.iter().enumerate() {
        match o {
            Ok(200 | 201) => {}
            Ok(st) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "concurrent PUT #{i} got {st} (expected 200/201 — same-blob writes must be idempotent). url={url}"
                    ),
                )
            }
            Err(m) => return JourneyResult::fail(name, ms(start), m.clone()),
        }
    }

    // After the storm, the object must read back as the EXACT bytes.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if get.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("post-storm GET got {} (expected 200)", get.status()),
        );
    }
    match get.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "CORRUPTION: post-storm GET bytes mismatch (put {} got {} bytes)",
                    blob.len(),
                    b.len()
                ),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET body: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Distinct-blob fan-out:** N writers PUT N DIFFERENT blobs at once (the common
/// case — a build emits many distinct artifacts in parallel). All N must store,
/// and AFTERWARDS each must be independently + correctly readable: a concurrent
/// write of object X must never clobber, truncate, or cross-wire object Y.
fn distinct_blobs_fan_out(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CONC CAS: N concurrent PUTs of DIFFERENT blobs — each stored + readable";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    // N distinct blobs, each at its own content address.
    let blobs: Vec<Vec<u8>> = (0..N).map(|i| unique_blob(&format!("conc-distinct-{i}"))).collect();
    let jobs: Vec<(String, Vec<u8>)> = blobs
        .iter()
        .map(|b| (url_cas(cfg, &p1.tenant, &blake3_hex(b)), b.clone()))
        .collect();

    let outcomes = concurrent_puts(client, token, &jobs);
    for (i, o) in outcomes.iter().enumerate() {
        match o {
            Ok(200 | 201) => {}
            Ok(st) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("concurrent PUT of distinct blob #{i} got {st} (expected 200/201)"),
                )
            }
            Err(m) => return JourneyResult::fail(name, ms(start), m.clone()),
        }
    }

    // Each distinct blob must read back as ITS OWN exact bytes.
    for (i, blob) in blobs.iter().enumerate() {
        let url = url_cas(cfg, &p1.tenant, &blake3_hex(blob));
        let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET #{i} {url}: {e}")),
        };
        if get.status().as_u16() != 200 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET of distinct blob #{i} got {} (expected 200)", get.status()),
            );
        }
        match get.bytes() {
            Ok(b) if b.as_ref() == blob.as_slice() => {}
            Ok(b) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "CORRUPTION: distinct blob #{i} mismatch (put {} got {} bytes) — cross-wired write",
                        blob.len(),
                        b.len()
                    ),
                )
            }
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET #{i} body: {e}")),
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **Read-during-write:** GETs race a PUT of the SAME key. A reader must NEVER
/// observe a torn / partial body — at any instant the object is either
/// absent/old (any deny / not-yet-200) OR the FULL new bytes; never a truncated
/// or garbage interleave. We fire one PUT and N readers concurrently against a
/// fresh address, then assert every reader that saw a 200 saw the COMPLETE bytes
/// (and the write itself succeeded). A reader that 404/401/403s simply observed
/// the pre-write state — acceptable; a reader that 200s with the wrong bytes is a
/// torn read — a launch-blocking corruption.
fn read_during_write(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CONC CAS: GET racing PUT of same key — never a torn/partial read";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("conc-read-during-write");
    let hash = blake3_hex(&blob);
    let url = url_cas(cfg, &p1.tenant, &hash);

    // Each reader records: Ok((status, bytes_if_200)) or Err(transport msg).
    type ReadOutcome = Result<(u16, Option<Vec<u8>>), String>;
    let put_result: Mutex<Option<Result<u16, String>>> = Mutex::new(None);
    let reads: Vec<Mutex<Option<ReadOutcome>>> = (0..N).map(|_| Mutex::new(None)).collect();

    thread::scope(|scope| {
        // The single writer.
        {
            let url = &url;
            let blob = &blob;
            let slot = &put_result;
            scope.spawn(move || {
                let outcome = client
                    .put(url)
                    .header(AUTHORIZATION, bearer(token))
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(blob.clone())
                    .send()
                    .map(|r| r.status().as_u16())
                    .map_err(|e| format!("PUT {url}: {e}"));
                *slot.lock().expect("put slot poisoned") = Some(outcome);
            });
        }
        // N concurrent readers racing the write.
        for slot in reads.iter() {
            let url = &url;
            scope.spawn(move || {
                let outcome: ReadOutcome = match client
                    .get(url)
                    .header(AUTHORIZATION, bearer(token))
                    .send()
                {
                    Ok(r) => {
                        let st = r.status().as_u16();
                        if st == 200 {
                            match r.bytes() {
                                Ok(b) => Ok((st, Some(b.to_vec()))),
                                Err(e) => Err(format!("GET body: {e}")),
                            }
                        } else {
                            Ok((st, None))
                        }
                    }
                    Err(e) => Err(format!("GET {url}: {e}")),
                };
                *slot.lock().expect("read slot poisoned") = Some(outcome);
            });
        }
    });

    // The write must have succeeded — otherwise the race is unverifiable.
    match put_result.into_inner().expect("put slot poisoned") {
        Some(Ok(200 | 201)) => {}
        Some(Ok(st)) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("racing PUT got {st} (expected 200/201) — cannot verify read integrity"),
            )
        }
        Some(Err(m)) => return JourneyResult::fail(name, ms(start), m),
        None => return JourneyResult::fail(name, ms(start), "writer thread set no result".to_string()),
    }

    // Every reader that observed a 200 must have observed the FULL exact bytes.
    // A non-200 (deny / not-yet-present) is the legitimate pre-write state.
    for (i, slot) in reads.into_iter().enumerate() {
        match slot.into_inner().expect("read slot poisoned") {
            Some(Ok((200, Some(bytes)))) => {
                if bytes.as_slice() != blob.as_slice() {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!(
                            "TORN READ: reader #{i} got 200 with wrong bytes (put {} got {} bytes) — partial/garbage read during write",
                            blob.len(),
                            bytes.len()
                        ),
                    );
                }
            }
            Some(Ok((200, None))) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("reader #{i} reported 200 but no body captured"),
                )
            }
            Some(Ok((_st, _))) => { /* pre-write state (deny / absent) — acceptable */ }
            Some(Err(m)) => return JourneyResult::fail(name, ms(start), m),
            None => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("reader #{i} thread set no result"),
                )
            }
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **Byte-accounting under concurrency** (best-effort, GATED-safe): concurrent
/// writes must not double-count or lose the charge. We read the tenant's usage
/// (bytes stored) before, fire N concurrent PUTs of N DISTINCT blobs of a KNOWN
/// total size, then read usage after — the delta should reflect exactly the new
/// bytes (no double-count, no loss).
///
/// This is inherently best-effort and **GATED-safe**: prod usage is often
/// eventually-consistent / rolled-up async, other writers on the same tenant can
/// move the meter, and the usage field may not be observable black-box at all. So
/// a missing/unparseable usage surface, or a delta that's plausibly affected by
/// concurrent activity, GATES (recorded, never a false FAIL). It only FAILs on an
/// unambiguous accounting bug — a delta that is a clean MULTIPLE of the written
/// bytes (classic N× double-count) or a NEGATIVE delta (lost/corrupted meter).
/// It never drives usage toward a quota ceiling (N small, distinct fresh blobs).
fn byte_accounting_under_concurrency(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "CONC CAS: byte-accounting under concurrency — no double-count / no loss";
    let start = Instant::now();

    // Prefer a DEDICATED no-other-writers tenant: there the bytes-stored meter
    // delta is attributable EXACTLY to this journey's writes, so we assert REAL
    // accounting (delta == written, ± one blob). Verified live: the meter is
    // precise + read-your-writes consistent (delta == written exactly). WITHOUT
    // the dedicated tenant we fall back to the shared primary tenant, where a
    // concurrent writer can inflate the delta — there we can only GATE (record),
    // never assert (the prior always-gated behavior).
    let (tenant, token, dedicated) =
        match (cfg.acct_tenant.as_deref(), cfg.token(TokenKind::Acct)) {
            (Some(t), Some(tok)) => (t.to_string(), tok, true),
            _ => {
                let p1 = match Persona::P1ReadWrite.resolve(cfg) {
                    Ok(p) => p,
                    Err(reason) => return JourneyResult::gated(name, reason),
                };
                if cfg.tenant.is_none() {
                    return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
                }
                (
                    cfg.tenant_or_anon().to_string(),
                    p1.token.expect("P1 always has a token"),
                    false,
                )
            }
        };

    // Probe the usage surface (scoped to `token`'s tenant); GATE if not observable.
    let before = match read_bytes_used(client, cfg, token) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return JourneyResult::gated(
                name,
                "usage surface returned no parseable bytes-stored field — accounting not observable black-box",
            )
        }
        Err(reason) => return JourneyResult::gated(name, reason),
    };

    // N distinct fresh blobs of a known total size.
    let blobs: Vec<Vec<u8>> = (0..N).map(|i| unique_blob(&format!("conc-acct-{i}"))).collect();
    let written_total: u64 = blobs.iter().map(|b| b.len() as u64).sum();
    let single_blob_len = blobs.first().map(|b| b.len() as u64).unwrap_or(0);
    let jobs: Vec<(String, Vec<u8>)> = blobs
        .iter()
        .map(|b| (url_cas(cfg, &tenant, &blake3_hex(b)), b.clone()))
        .collect();

    let outcomes = concurrent_puts(client, token, &jobs);
    for (i, o) in outcomes.iter().enumerate() {
        match o {
            Ok(200 | 201) => {}
            Ok(st) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("accounting PUT #{i} got {st} (expected 200/201)"),
                )
            }
            Err(m) => return JourneyResult::fail(name, ms(start), m.clone()),
        }
    }

    // SETTLE the meter: poll until two consecutive reads agree, so an in-flight
    // update can't under-report the delta and trip a false loss-FAIL.
    let mut after = before;
    let mut prev = u64::MAX;
    for _ in 0..6 {
        match read_bytes_used(client, cfg, token) {
            Ok(Some(v)) => {
                after = v;
                if v == prev {
                    break;
                }
                prev = v;
            }
            Ok(None) => {
                return JourneyResult::gated(
                    name,
                    "usage surface stopped reporting bytes-stored after the writes — not observable",
                )
            }
            Err(reason) => return JourneyResult::gated(name, reason),
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // A negative delta is an unambiguous accounting bug (lost/corrupted meter):
    // writing data can never DECREASE bytes-stored.
    if after < before {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "ACCOUNTING: bytes-stored DECREASED across concurrent writes ({before} → {after}) — lost/corrupted charge"
            ),
        );
    }
    let delta = after - before;

    // A clean N× (or 2×) multiple of the written total is the classic concurrent
    // double-count signature — a HARD FAIL in BOTH modes.
    if single_blob_len > 0 {
        let doubled = written_total.checked_mul(2);
        let n_times = written_total.checked_mul(N as u64);
        if Some(delta) == doubled || Some(delta) == n_times {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "ACCOUNTING: bytes-stored delta {delta} is a clean multiple of the written {written_total} ({N} blobs) — concurrent DOUBLE-COUNT"
                ),
            );
        }
    }

    if dedicated {
        // No other writers → the settled delta MUST equal the written bytes,
        // within one blob of slack (metadata/rounding). Below = lost charge;
        // above = over-count. This is the real accounting tooth.
        if delta < written_total {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "ACCOUNTING: dedicated-tenant settled delta {delta} < written {written_total} — bytes-stored UNDER-counted (lost charge)"
                ),
            );
        }
        if delta > written_total + single_blob_len {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "ACCOUNTING: dedicated-tenant settled delta {delta} > written {written_total} (+1 blob tol {single_blob_len}) — bytes-stored OVER-counted"
                ),
            );
        }
        return JourneyResult::pass(name, ms(start));
    }

    JourneyResult::gated(
        name,
        format!(
            "observed bytes-stored delta {delta} for {written_total} written ({N} distinct blobs) on \
             the SHARED primary tenant; a concurrent writer can inflate it, so an exact assert is not \
             safe — provision CORELINK_E2E_ACCT_TENANT for the strict check. No double-count/loss signature detected"
        ),
    )
}

/// Best-effort black-box read of the tenant's bytes-stored counter from the
/// customer usage/overview surfaces. Returns:
///   - `Ok(Some(bytes))` when a plausible bytes-stored number was parsed,
///   - `Ok(None)`        when the surface responded but exposed no such field,
///   - `Err(reason)`     when the surface is unreachable / unauthorized (→ GATE).
///
/// It probes a small set of common field names (the exact schema is not part of
/// the frozen URL contract, so this stays defensive and GATES on absence).
fn read_bytes_used(client: &Client, cfg: &Config, token: &str) -> Result<Option<u64>, String> {
    // Try the usage surface first, then the overview surface.
    for resource in ["usage", "overview"] {
        let url = url_customer(cfg, resource);
        let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
            Ok(r) => r,
            Err(e) => return Err(format!("usage probe GET {url}: {e}")),
        };
        let st = resp.status().as_u16();
        if st != 200 {
            // 401/403 → not authorized to read usage with this PAT; 404 → no
            // such surface. Either way usage isn't observable → GATE upstream.
            continue;
        }
        let body: serde_json::Value = match resp.json() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(v) = find_bytes_field(&body) {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// Recursively search a JSON value for a plausible "bytes stored" integer field.
/// Matches common key spellings the usage surfaces might use; returns the first
/// non-negative integer found. Conservative by design — an unrecognized schema
/// yields `None` (→ the journey GATES, never falsely FAILs).
fn find_bytes_field(v: &serde_json::Value) -> Option<u64> {
    const KEYS: [&str; 6] = [
        "bytes_stored",
        "storage_bytes",
        "bytes_used",
        "used_bytes",
        "cas_bytes",
        "total_bytes",
    ];
    match v {
        serde_json::Value::Object(map) => {
            for k in KEYS {
                if let Some(n) = map.get(k).and_then(|x| x.as_u64()) {
                    return Some(n);
                }
            }
            for (_k, child) in map {
                if let Some(n) = find_bytes_field(child) {
                    return Some(n);
                }
            }
            None
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_bytes_field),
        _ => None,
    }
}
