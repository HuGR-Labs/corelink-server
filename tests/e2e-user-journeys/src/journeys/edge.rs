//! Error / edge-path coverage — the malformed + boundary inputs a real client
//! (and a fuzzer) actually throws at the cache surfaces.
//!
//! Black-box only: the deployed HTTP API + a single read+write PAT (P1), via the
//! [`crate::harness`] URL builders. No `corelink-*` import, no mocks, no
//! internal-state reads.
//!
//! Where `cas`/`ac` prove the happy path and `security` proves the deny matrix,
//! THIS module proves the server **rejects garbage cleanly** — the RIGHT 4xx,
//! never a 5xx, never a silent-accept. The governing invariant is hard:
//!
//!   > A malformed / out-of-contract input MUST produce a 4xx. A `5xx` on bad
//!   > input is a server-hardening bug (the server crashed/paniced on attacker-
//!   > controlled data) and is FAILed. A `2xx` that stores/serves the garbage is
//!   > a silent-accept and is likewise FAILed.
//!
//! Every cell is a PASS / GATED / FAIL [`JourneyResult`]:
//!   - **PASS**  — the server rejected with the expected 4xx (and never a 5xx).
//!   - **GATED** — P1 (the RW PAT) or the tenant segment is absent.
//!   - **FAIL**  — a 5xx on malformed input, OR a silent-accept (a 2xx/stored
//!     where the contract demands rejection), OR a transport error.
//!
//! Cells:
//!   CAS/AC
//!     - malformed digest (too short / too long / uppercase / non-hex) → 4xx
//!     - PUT whose body does NOT match the claimed CAS digest → HASH mismatch
//!       (4xx, and the bytes must NOT be retrievable afterwards)
//!     - GET of a well-formed but absent digest → 404
//!     - oversized body (11 MiB) over the 10 MiB global limit → 413
//!     - wrong content-type on a CAS PUT → still correct (accept) or a clean 4xx,
//!       never a 5xx
//!     - empty body PUT → a clean status, never a 5xx
//!   Bazel
//!     - a blobs path with a non-numeric size segment → 4xx
//!     - a blobs path with a wrong (non-hex / short) sha256 → 4xx
//!   HTTP hygiene
//!     - an unsupported METHOD on a cache route → 405 (never 5xx)
//!     - a malformed Authorization header → 401 (never 5xx)

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Method;

use crate::harness::{
    bearer, blake3_hex, unique_blob, url_bazel_cas_read, url_cas, Config, JourneyResult,
};
use crate::personas::Persona;

/// Milliseconds elapsed since `start`.
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// `true` if `st` is a server-error (5xx) — the hard-fail signal for this whole
/// module: a 5xx on malformed input means the server choked on attacker bytes.
fn is_5xx(st: u16) -> bool {
    (500..=599).contains(&st)
}

/// A 4xx client-error in the inclusive 400..=499 band.
fn is_4xx(st: u16) -> bool {
    (400..=499).contains(&st)
}

/// Resolve P1's `(tenant, token)` or a gate reason. Every cell needs the RW PAT
/// + the tenant segment; this centralizes that prerequisite check.
fn p1_ctx<'a>(cfg: &'a Config, name: &'static str) -> Result<(String, &'a str), JourneyResult> {
    let p1 = Persona::P1ReadWrite
        .resolve(cfg)
        .map_err(|reason| JourneyResult::gated(name, reason))?;
    if cfg.tenant.is_none() {
        return Err(JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — edge cases need the tenant path segment",
        ));
    }
    let token = p1.token.expect("P1 always has a token");
    Ok((p1.tenant, token))
}

/// Run the edge / error-path coverage.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        cas_malformed_digest(cfg, client),
        cas_hash_mismatch_not_stored(cfg, client),
        cas_absent_digest_404(cfg, client),
        cas_oversized_body(cfg, client),
        cas_wrong_content_type(cfg, client),
        cas_empty_body(cfg, client),
        bazel_bad_size_segment(cfg, client),
        bazel_wrong_sha256(cfg, client),
        http_unsupported_method(cfg, client),
        http_malformed_authorization(cfg, client),
    ]
}

/// Send a GET and return its status, mapping a transport error to a message.
fn get_status(client: &Client, url: &str, token: &str) -> Result<u16, String> {
    client
        .get(url)
        .header(AUTHORIZATION, bearer(token))
        .send()
        .map(|r| r.status().as_u16())
        .map_err(|e| format!("GET {url}: {e}"))
}

/// **Malformed digest** — for each obviously-invalid `{hash}` segment (too short,
/// too long, uppercase, non-hex) a GET and a PUT must be rejected with a 4xx and
/// NEVER a 5xx. A 64-hex address is the documented shape; anything else is a
/// client error the router/handler must reject cleanly, not crash on.
fn cas_malformed_digest(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE CAS: malformed digest (short/long/upper/non-hex) → 4xx, never 5xx";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    // Each is a well-formed-LOOKING-but-invalid content address.
    let bad = [
        ("too short (63 hex)", "a".repeat(63)),
        ("too long (65 hex)", "a".repeat(65)),
        (
            "uppercase hex",
            "A".repeat(64), // 64 chars but uppercase — not the lowercase-hex contract
        ),
        (
            "non-hex chars",
            format!("{}zzzz", "0".repeat(60)), // 64 chars, contains non-hex 'z'
        ),
        ("path traversal-ish", "..%2f..%2fetc".to_string()),
    ];
    let body = unique_blob("edge-malformed-digest");

    for (why, digest) in bad.iter() {
        let url = url_cas(cfg, &tenant, digest);

        // GET must be a clean 4xx, never a 5xx.
        match get_status(client, &url, token) {
            Ok(st) if is_5xx(st) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                    "HARDENING: GET malformed digest [{why}] → {st} (5xx on bad input). url={url}"
                ),
                )
            }
            Ok(st) if !is_4xx(st) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("GET malformed digest [{why}] → {st} (expected a 4xx). url={url}"),
                )
            }
            Ok(_) => {}
            Err(m) => return JourneyResult::fail(name, ms(start), m),
        }

        // PUT must be a clean 4xx, never a 5xx (and obviously never a stored 2xx).
        let put = client
            .put(&url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body.clone())
            .send();
        match put {
            Ok(r) => {
                let st = r.status().as_u16();
                if is_5xx(st) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("HARDENING: PUT malformed digest [{why}] → {st} (5xx on bad input). url={url}"),
                    );
                }
                if matches!(st, 200 | 201) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("SILENT-ACCEPT: PUT malformed digest [{why}] → {st} (stored garbage). url={url}"),
                    );
                }
                if !is_4xx(st) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("PUT malformed digest [{why}] → {st} (expected a 4xx). url={url}"),
                    );
                }
            }
            Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **Hash-mismatch PUT** — native CAS verifies the body's BLAKE3 against the URL
/// `{hash}`. A PUT whose body does NOT hash to the claimed (well-formed) address
/// must be rejected (400/422 content-hash-mismatch), NEVER 5xx, NEVER stored.
/// We then GET the claimed address and assert the poisoned bytes are NOT served
/// (a content-addressed store must never return bytes that don't match the key).
fn cas_hash_mismatch_not_stored(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE CAS: body≠claimed digest → 4xx hash-mismatch, NOT stored";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    // The claimed address is the BLAKE3 of `claimed`, but we PUT `wrong` under it.
    let claimed = unique_blob("edge-hash-claimed");
    let wrong = unique_blob("edge-hash-wrong-body");
    let addr = blake3_hex(&claimed); // a valid, fresh, lowercase-hex address
    let url = url_cas(cfg, &tenant, &addr);

    let put = client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(wrong.clone())
        .send();
    let put_status = match put {
        Ok(r) => r.status().as_u16(),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    if is_5xx(put_status) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: hash-mismatch PUT → {put_status} (5xx on bad input). url={url}"),
        );
    }
    if matches!(put_status, 200 | 201) {
        // The server claims it stored a body under an address that is NOT its
        // hash — a content-addressing integrity break. Confirm by reading back:
        // if the wrong bytes come back, that is a hard FAIL.
        let getr = client.get(&url).header(AUTHORIZATION, bearer(token)).send();
        if let Ok(r) = getr {
            if r.status().as_u16() == 200 {
                let served = r.bytes().map(|b| b.to_vec()).unwrap_or_default();
                if served.as_slice() == wrong.as_slice() {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!(
                            "INTEGRITY: CAS accepted+served {} bytes whose BLAKE3 ≠ the URL digest — content-addressing broken. url={url}",
                            served.len()
                        ),
                    );
                }
            }
        }
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SILENT-ACCEPT: hash-mismatch PUT → {put_status} (expected 400/422). url={url}"
            ),
        );
    }
    // Expect specifically the hash-mismatch family: 400 or 422 (a deny like 403
    // would mean a scope problem, not the mismatch we're probing).
    if !matches!(put_status, 400 | 422) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("hash-mismatch PUT → {put_status} (expected 400/422 hash-mismatch). url={url}"),
        );
    }

    // The poisoned address must not now serve the wrong bytes (and must not 5xx).
    match get_status(client, &url, token) {
        Ok(st) if is_5xx(st) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("HARDENING: GET after rejected mismatch → {st} (5xx). url={url}"),
            )
        }
        Ok(200) => {
            // If anything is served it must not be the wrong bytes we tried to plant.
            let getr = client.get(&url).header(AUTHORIZATION, bearer(token)).send();
            if let Ok(r) = getr {
                let served = r.bytes().map(|b| b.to_vec()).unwrap_or_default();
                if served.as_slice() == wrong.as_slice() {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!(
                            "INTEGRITY: rejected mismatch bytes are nonetheless served. url={url}"
                        ),
                    );
                }
            }
            // 200 with non-wrong bytes (e.g. a legit earlier-stored object) is fine.
        }
        Ok(_) => {} // 404/403/etc — correctly not stored.
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Absent digest** — a GET of a well-formed-but-never-written address must be a
/// clean 404 (not a 5xx, not a spurious 200). Fresh content per run, so the
/// address provably never existed.
fn cas_absent_digest_404(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE CAS: GET well-formed but absent digest → 404 (not 5xx, not 200)";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    // A valid 64-hex address for content we never PUT.
    let never = unique_blob("edge-never-written");
    let addr = blake3_hex(&never);
    let url = url_cas(cfg, &tenant, &addr);

    match get_status(client, &url, token) {
        Ok(404) => JourneyResult::pass(name, ms(start)),
        Ok(200) => JourneyResult::fail(
            name,
            ms(start),
            format!("GET of a never-written address → 200 (phantom content). url={url}"),
        ),
        Ok(st) if is_5xx(st) => JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: GET absent digest → {st} (5xx). url={url}"),
        ),
        // A deny (401/403) is acceptable-but-not-ideal here; the documented
        // contract is 404. Accept 403 only as a non-5xx deny, but flag a 401
        // (the PAT IS valid) as wrong.
        Ok(403) => JourneyResult::pass(name, ms(start)),
        Ok(st) => JourneyResult::fail(
            name,
            ms(start),
            format!("GET absent digest → {st} (expected 404). url={url}"),
        ),
        Err(m) => JourneyResult::fail(name, ms(start), m),
    }
}

/// **Oversized body** — a PUT whose body exceeds the global per-request body
/// limit must be rejected with 413 (or another clean 4xx), NEVER a 5xx. The
/// container caps EVERY route at a 10 MiB `DefaultBodyLimit`
/// (`crates/corelink-container/src/main.rs:459`), so a body just over that cap
/// is deterministically refused before any handler runs. This probe sends
/// 11 MiB — one MiB over the 10 MiB ceiling — which is large enough to trip the
/// limit yet still a single bounded request that is safe to send once and never
/// a quota-drive. Because 11 MiB > 10 MiB always exceeds the cap, this cell is a
/// real PASS/FAIL on 413, not a GATE.
fn cas_oversized_body(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE CAS: oversized body (11 MiB > 10 MiB cap) → 413";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    // 11 MiB — exactly one MiB over the 10 MiB global `DefaultBodyLimit`
    // (main.rs:459). Deterministically exceeds the cap (a single bounded request,
    // safe to send once), so 413 is guaranteed; small enough to not be a
    // quota-drive or disturb other tenants.
    const PROBE_BYTES: usize = 11 * 1024 * 1024;
    let body = vec![b'A'; PROBE_BYTES];
    // CAS verifies BLAKE3, so address the probe by its real digest — that way a
    // rejection is unambiguously the SIZE, not a hash mismatch.
    let addr = blake3_hex(&body);
    let url = url_cas(cfg, &tenant, &addr);

    let put = client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .send();
    let st = match put {
        Ok(r) => r.status().as_u16(),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    if is_5xx(st) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: oversized PUT → {st} (5xx on a large but valid body). url={url}"),
        );
    }
    if st == 413 {
        return JourneyResult::pass(name, ms(start));
    }
    if matches!(st, 200 | 201) {
        // An 11 MiB body was ACCEPTED — the 10 MiB global `DefaultBodyLimit`
        // (main.rs:459) failed to reject a request over its cap. That is a real
        // regression, not a gate. (Best-effort cleanup: a DELETE of what we wrote.)
        let _ = client
            .delete(&url)
            .header(AUTHORIZATION, bearer(token))
            .send();
        return JourneyResult::fail(
            name,
            ms(start),
            format!("11 MiB body accepted ({st}) — the 10 MiB global body limit (main.rs:459) did not reject an over-cap request. url={url}"),
        );
    }
    // Some other clean 4xx (e.g. 400/403/422) — accept as a non-5xx rejection
    // but note it wasn't the canonical 413.
    if is_4xx(st) {
        return JourneyResult::pass(name, ms(start));
    }
    JourneyResult::fail(
        name,
        ms(start),
        format!("oversized PUT → {st} (expected 413 or a clean 4xx). url={url}"),
    )
}

/// **Wrong content-type** — a CAS PUT with a misleading `Content-Type` (e.g.
/// `application/json` for raw bytes). The address is the correct BLAKE3 of the
/// body, so the only oddity is the header. The server may legitimately accept
/// it (CAS is content-addressed, the type is advisory) OR reject with a clean
/// 4xx — but it must NEVER 5xx on it. If accepted, the round-trip bytes must
/// match (no corruption from the bogus type).
fn cas_wrong_content_type(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE CAS: wrong Content-Type on PUT → accept or clean 4xx, never 5xx";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    let body = unique_blob("edge-wrong-content-type");
    let addr = blake3_hex(&body);
    let url = url_cas(cfg, &tenant, &addr);

    let put = client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json") // deliberately wrong for raw bytes
        .body(body.clone())
        .send();
    let st = match put {
        Ok(r) => r.status().as_u16(),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    if is_5xx(st) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: wrong-content-type PUT → {st} (5xx). url={url}"),
        );
    }
    if matches!(st, 200 | 201) {
        // Accepted — the round-trip bytes must be intact (type didn't corrupt).
        let getr = client.get(&url).header(AUTHORIZATION, bearer(token)).send();
        match getr {
            Ok(r) if r.status().as_u16() == 200 => {
                let served = r.bytes().map(|b| b.to_vec()).unwrap_or_default();
                if served.as_slice() != body.as_slice() {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        "wrong-content-type PUT accepted but round-trip bytes differ (corruption)"
                            .to_string(),
                    );
                }
                let _ = client
                    .delete(&url)
                    .header(AUTHORIZATION, bearer(token))
                    .send();
                return JourneyResult::pass(name, ms(start));
            }
            Ok(r) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "wrong-content-type PUT accepted ({st}) but GET → {}",
                        r.status()
                    ),
                )
            }
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
        }
    }
    if is_4xx(st) {
        // A clean rejection is also a correct, hardened answer.
        return JourneyResult::pass(name, ms(start));
    }
    JourneyResult::fail(
        name,
        ms(start),
        format!("wrong-content-type PUT → {st} (expected accept or clean 4xx). url={url}"),
    )
}

/// **Empty body** — a PUT with a zero-length body. The address is the BLAKE3 of
/// the empty byte string (a real, valid digest), so this is the legitimate
/// "store the empty blob" case OR a clean rejection — but never a 5xx. If
/// accepted, a GET must return exactly zero bytes.
fn cas_empty_body(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE CAS: empty body PUT → clean status (accept 0 bytes or 4xx), never 5xx";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    let empty: Vec<u8> = Vec::new();
    let addr = blake3_hex(&empty); // BLAKE3 of the empty input — a valid address
    let url = url_cas(cfg, &tenant, &addr);

    let put = client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(empty.clone())
        .send();
    let st = match put {
        Ok(r) => r.status().as_u16(),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    if is_5xx(st) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: empty-body PUT → {st} (5xx). url={url}"),
        );
    }
    if matches!(st, 200 | 201) {
        // Accepted — GET must serve exactly zero bytes.
        let getr = client.get(&url).header(AUTHORIZATION, bearer(token)).send();
        match getr {
            Ok(r) if r.status().as_u16() == 200 => {
                let n = r.bytes().map(|b| b.len()).unwrap_or(usize::MAX);
                if n != 0 {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("empty-body PUT accepted but GET served {n} bytes (expected 0)"),
                    );
                }
                return JourneyResult::pass(name, ms(start));
            }
            // The empty blob is content-addressed; a deny/404 on read-back is an
            // acceptable "didn't store the empty object" answer (not a 5xx).
            Ok(r) if is_5xx(r.status().as_u16()) => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("HARDENING: GET empty blob → {} (5xx)", r.status()),
                )
            }
            Ok(_) => return JourneyResult::pass(name, ms(start)),
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
        }
    }
    if is_4xx(st) {
        return JourneyResult::pass(name, ms(start));
    }
    JourneyResult::fail(
        name,
        ms(start),
        format!("empty-body PUT → {st} (expected accept-0 or clean 4xx). url={url}"),
    )
}

/// **Bazel bad size segment** — the REAPI blobs path is
/// `/bazel/v2/{instance}/blobs/{hash}/{size}` where `{size}` is the decimal byte
/// length. A non-numeric size segment is malformed and must be a clean 4xx,
/// never a 5xx. (The instance segment is a stable, harmless test instance.)
fn bazel_bad_size_segment(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE Bazel: blobs path with non-numeric size segment → 4xx, never 5xx";
    let start = Instant::now();
    let (_, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    // A 64-hex sha256-shaped hash, paired with a NON-numeric size segment. The
    // url builder takes `size: usize`, so we build the malformed path explicitly
    // through the builder's hex hash and then swap the size via a raw suffix —
    // but to stay inside the builder contract we instead hand-craft only the
    // malformed segment using the builder for the numeric form, then replace.
    let good = url_bazel_cas_read(cfg, "corelink-e2e", &"a".repeat(64), 0);
    // Replace the trailing "/0" numeric size with a non-numeric token.
    let url = good
        .strip_suffix("/0")
        .map(|p| format!("{p}/not-a-number"))
        .unwrap_or(good);

    match get_status(client, &url, token) {
        Ok(st) if is_5xx(st) => JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: Bazel bad-size GET → {st} (5xx). url={url}"),
        ),
        Ok(200) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SILENT-ACCEPT: Bazel bad-size GET → 200 (served on a malformed path). url={url}"
            ),
        ),
        Ok(st) if is_4xx(st) => JourneyResult::pass(name, ms(start)),
        Ok(st) => JourneyResult::fail(
            name,
            ms(start),
            format!("Bazel bad-size GET → {st} (expected a 4xx). url={url}"),
        ),
        Err(m) => JourneyResult::fail(name, ms(start), m),
    }
}

/// **Bazel wrong sha256** — a blobs path whose `{hash}` segment is not a valid
/// 64-hex sha256 (too short / non-hex). Must be a clean 4xx, never a 5xx.
fn bazel_wrong_sha256(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE Bazel: blobs path with malformed sha256 → 4xx, never 5xx";
    let start = Instant::now();
    let (_, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    // A short, non-hex hash with a valid numeric size — the hash is the defect.
    let url = url_bazel_cas_read(cfg, "corelink-e2e", "deadbeef-not-a-real-sha256", 42);

    match get_status(client, &url, token) {
        Ok(st) if is_5xx(st) => JourneyResult::fail(
            name,
            ms(start),
            format!("HARDENING: Bazel bad-sha GET → {st} (5xx). url={url}"),
        ),
        Ok(200) => JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SILENT-ACCEPT: Bazel bad-sha GET → 200 (served on a malformed hash). url={url}"
            ),
        ),
        Ok(st) if is_4xx(st) => JourneyResult::pass(name, ms(start)),
        Ok(st) => JourneyResult::fail(
            name,
            ms(start),
            format!("Bazel bad-sha GET → {st} (expected a 4xx). url={url}"),
        ),
        Err(m) => JourneyResult::fail(name, ms(start), m),
    }
}

/// **Unsupported method** — an HTTP method a cache route does not implement (here
/// `TRACE`, plus a `PATCH` as a second probe) against a valid CAS object path
/// must yield 405 Method Not Allowed (or another clean 4xx), never a 5xx.
fn http_unsupported_method(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE HTTP: unsupported method on a cache route → 405, never 5xx";
    let start = Instant::now();
    let (tenant, token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    let probe = unique_blob("edge-method-probe");
    let addr = blake3_hex(&probe);
    let url = url_cas(cfg, &tenant, &addr);

    // TRACE is universally not a cache verb; PATCH is a second, distinct probe.
    for method in [Method::TRACE, Method::PATCH] {
        let resp = client
            .request(method.clone(), &url)
            .header(AUTHORIZATION, bearer(token))
            .send();
        match resp {
            Ok(r) => {
                let st = r.status().as_u16();
                if is_5xx(st) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("HARDENING: {method} on a cache route → {st} (5xx). url={url}"),
                    );
                }
                // 405 is canonical; any other clean 4xx (e.g. 400/404/501-as-4xx
                // doesn't apply) is acceptable as a non-5xx rejection, but a 2xx
                // would mean the method was silently honoured.
                if matches!(st, 200 | 201 | 202 | 204) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("SILENT-ACCEPT: {method} on a cache route → {st} (method honoured). url={url}"),
                    );
                }
                if !is_4xx(st) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("{method} on a cache route → {st} (expected 405/4xx). url={url}"),
                    );
                }
            }
            Err(e) => {
                // Some clients/proxies refuse to even send TRACE; treat a transport
                // refusal on TRACE specifically as a non-failure (the method never
                // reached the server, so there's no 5xx to assert). PATCH must send.
                if method == Method::TRACE {
                    continue;
                }
                return JourneyResult::fail(name, ms(start), format!("{method} {url}: {e}"));
            }
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **Malformed Authorization header** — a garbage `Authorization` value (not a
/// `Bearer <token>`, a bare scheme, a truncated token) must be rejected with 401
/// Unauthorized, NEVER a 5xx (the auth layer must not crash parsing attacker
/// header bytes). We probe a CAS GET, which requires auth.
fn http_malformed_authorization(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "EDGE HTTP: malformed Authorization header → 401, never 5xx";
    let start = Instant::now();
    // This cell needs the tenant segment but uses BAD tokens of its own, so it
    // only requires P1 to confirm the path is real (and to gate uniformly).
    let (tenant, _token) = match p1_ctx(cfg, name) {
        Ok(v) => v,
        Err(g) => return g,
    };

    let probe = unique_blob("edge-bad-auth-probe");
    let addr = blake3_hex(&probe);
    let url = url_cas(cfg, &tenant, &addr);

    // A spread of malformed Authorization values a fuzzer/attacker sends.
    let bad_headers = [
        ("not Bearer scheme", "Basic Zm9vOmJhcg=="),
        ("bare scheme, no token", "Bearer"),
        ("scheme + empty token", "Bearer "),
        ("garbage scheme", "Garbage zzz"),
        ("not a header at all", "????"),
        (
            "Bearer + obviously-invalid token",
            "Bearer not-a-real-pat-xxxxxxxxxxxxx",
        ),
    ];

    for (why, val) in bad_headers.iter() {
        let resp = client.get(&url).header(AUTHORIZATION, *val).send();
        match resp {
            Ok(r) => {
                let st = r.status().as_u16();
                if is_5xx(st) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!(
                            "HARDENING: malformed Authorization [{why}] → {st} (5xx). url={url}"
                        ),
                    );
                }
                if st == 200 {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("AUTH-BYPASS: malformed Authorization [{why}] → 200. url={url}"),
                    );
                }
                // 401 is canonical; 403/404 are acceptable non-5xx denies. Anything
                // outside the deny band on a junk credential is wrong.
                if !matches!(st, 401 | 403 | 404) {
                    return JourneyResult::fail(
                        name,
                        ms(start),
                        format!("malformed Authorization [{why}] → {st} (expected 401). url={url}"),
                    );
                }
            }
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
        }
    }

    JourneyResult::pass(name, ms(start))
}
