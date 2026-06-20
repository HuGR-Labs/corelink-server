//! Quota journeys — hard-cap enforcement (no silent overage).
//!
//! Migrated from the old single-file suite (journey 7). URL CORRECTED to the
//! real native CAS template `/v1/cas/{tenant}/{hash}`. Slow + write-heavy, so
//! gated behind `CORELINK_E2E_RUN_SLOW=1`. Probes the MECHANISM: under quota
//! pressure the server returns 429 (hard cap) rather than silently accepting
//! overage. A real 10GB cap needs a near-limit test account.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{bearer, sha256_hex, url_cas, Config, JourneyResult};
use crate::personas::Persona;

/// Run the quota journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![quota_hard_cap(cfg, client)]
}

/// PUT small unique blobs until a 429 hard-cap is observed (or declare the cap
/// unobserved after a bounded number of attempts).
fn quota_hard_cap(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Quota: hard-cap — PUT blobs until 429 (not silent overage)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if !cfg.run_slow {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_RUN_SLOW=1 not set — quota hard-cap skipped (write-heavy; run only against a near-limit test account)",
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
        let hash = sha256_hex(&blob);
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
        if status == 429 {
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
