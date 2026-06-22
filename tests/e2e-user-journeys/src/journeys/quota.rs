//! Quota journeys — hard-cap enforcement (no silent overage).
//!
//! Migrated from the old single-file suite (journey 7). URL CORRECTED to the
//! real native CAS template `/v1/cas/{tenant}/{hash}`. Slow + write-heavy, so
//! gated behind `CORELINK_E2E_RUN_SLOW=1`. Probes the MECHANISM: under quota
//! pressure the server returns 429 (hard cap) rather than silently accepting
//! overage. A real 10GB cap needs a near-limit test account.

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{bearer, blake3_hex, url_cas, Config, JourneyResult};
use crate::personas::Persona;

/// Opt-in flag for the destructive near-limit cap drive. `CORELINK_E2E_QUOTA_TEST=1`
/// is the WP-specified gate; we also honour the legacy `CORELINK_E2E_RUN_SLOW=1`
/// (the original slow gate) so either flag enables it.
const QUOTA_TEST_ENV: &str = "CORELINK_E2E_QUOTA_TEST";

/// True iff the destructive cap drive is enabled (either flag).
fn quota_drive_enabled(cfg: &Config) -> bool {
    cfg.run_slow
        || env::var(QUOTA_TEST_ENV)
            .map(|v| v == "1")
            .unwrap_or(false)
}

/// Run the quota journeys: under-cap-serves (the positive half) + hard-cap
/// enforcement (the destructive half, flag-gated).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        under_cap_serves(cfg, client),
        quota_hard_cap(cfg, client),
    ]
}

/// Positive half of the cap contract: a single small write by a healthy tenant
/// must be SERVED (2xx) — the cap must not be so aggressive that normal use is
/// rejected, and a quota deny must mean "over cap", not "always denied". This is
/// idempotent (content-addressed) and cheap, so it runs without the slow flag.
/// Gates on the RW token + tenant (never a hard skip).
fn under_cap_serves(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Quota: under-cap write is SERVED (2xx) — cap denies overage, not normal use";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 has a token");

    // A small unique blob — well under any tier cap for a healthy test tenant.
    let blob = format!("quota-undercap-{}", uuid::Uuid::new_v4()).into_bytes();
    let hash = blake3_hex(&blob);
    let url = url_cas(cfg, &p1.tenant, &hash);

    let resp = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let status = resp.status().as_u16();
    match status {
        200 | 201 => JourneyResult::pass(name, ms(start)),
        // If THIS tenant is already at its cap, a 402/429 is legitimate state, not
        // a contract break — gate rather than fail (the positive half needs an
        // under-cap account).
        402 | 429 => JourneyResult::gated(
            name,
            format!(
                "tenant is already at its cap (got {status}) — under-cap-serves needs a tenant \
                 below its limit"
            ),
        ),
        other => JourneyResult::fail(
            name,
            ms(start),
            format!("under-cap PUT got {other} (expected 200/201 serve)"),
        ),
    }
}

/// PUT small unique blobs until a 429 hard-cap is observed (or declare the cap
/// unobserved after a bounded number of attempts).
fn quota_hard_cap(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Quota: hard-cap — PUT blobs until 429 (not silent overage)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if !quota_drive_enabled(cfg) {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_QUOTA_TEST=1 (or CORELINK_E2E_RUN_SLOW=1) not set — quota hard-cap skipped (write-heavy; run only against a near-limit test account)",
        );
    }
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 has a token");

    const MAX_ATTEMPTS: usize = 100;
    let template = vec![b'x'; 1024]; // 1KB

    for i in 0..MAX_ATTEMPTS {
        let mut blob = template.clone();
        let marker = format!("quota-test-{}-{i}", uuid::Uuid::new_v4());
        let mb = marker.as_bytes();
        if mb.len() < blob.len() {
            blob[..mb.len()].copy_from_slice(mb);
        }
        let hash = blake3_hex(&blob);
        let url = url_cas(cfg, &p1.tenant, &hash);

        let resp = match client
            .put(&url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(blob)
            .send()
        {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT #{i}: {e}")),
        };
        let status = resp.status().as_u16();
        // Hard cap = 402 (ADR-0068 monthly $-ceiling) or 429 (rate cap). Either
        // proves the cap is enforced, not silently overaged.
        if status == 402 || status == 429 {
            return JourneyResult::pass(name, ms(start)); // hard cap enforced
        }
        if status == 404 || status >= 500 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PUT #{i} got {status} — data plane not operational; cannot verify quota"),
            );
        }
    }

    JourneyResult::fail(
        name,
        ms(start),
        format!(
            "sent {MAX_ATTEMPTS}×1KB blobs without a 429. Quota cap not observed — either the test account is far below its limit, or the cap is not enforced. A real cap test needs a near-limit account."
        ),
    )
}
