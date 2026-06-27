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

use crate::harness::{bearer, blake3_hex, url_cas, Config, JourneyResult, TokenKind};
use crate::personas::Persona;

/// Max PUT attempts the cap-drive will make per content-address before declaring
/// the data plane unreachable. The CAS write hot-path has 2 synchronous
/// D1-over-HTTP hops (quota + tombstone) plus a ~2.5s cold-container start
/// (docs/perf/2026-06-19-cas-hot-path-latency.md), so a fresh quota tenant's
/// first writes can 503/time-out while the container is cold and D1 is slow. A
/// single cold-start blip must NOT be able to exhaust the budget mid-drive, so
/// we allow up to 8 attempts with EXPONENTIAL backoff (the real perf fix is
/// PR #368 WP-2 — this only de-flakes the e2e).
const MAX_PUT_ATTEMPTS: usize = 8;

/// PUT a blob, riding out TRANSIENT prod faults so a flaky window can't turn a
/// quota journey RED on a NON-cap blip. A per-tenant data plane can 503 on its
/// first (cold) touch, and a slow-prod window can time the request out — neither
/// is a quota-cap result, yet the old inline PUTs hard-FAILED on the first such
/// blip (observed: a fresh quota tenant timed out at 60s / 503'd on PUT #0,
/// rotating run-to-run). We retry the SAME content-address — idempotent, since
/// CAS is content-addressed so a re-PUT is a safe no-op/dedup — on a send-error /
/// 503 / 504, up to `MAX_PUT_ATTEMPTS` with EXPONENTIAL backoff (1,2,4,8,8,8,8 s,
/// capped at 8s) so a cold-start blip can't exhaust the budget mid-drive. Returns:
///   `Ok(status)`  — a DEFINITIVE response: 2xx served, 402/429 cap, 404, a
///                   non-transient 5xx (crash-on-cap), etc. — the caller decides.
///   `Err(reason)` — a PERSISTENT transient fault after the budget; the caller
///                   GATES (a prod outage is not a quota-cap result → never a FAIL).
fn put_blob_tolerant(
    client: &Client,
    url: &str,
    token: &str,
    blob: &[u8],
) -> Result<u16, String> {
    let mut last = String::new();
    for attempt in 0..MAX_PUT_ATTEMPTS {
        match client
            .put(url)
            .header(AUTHORIZATION, bearer(token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(blob.to_vec())
            .send()
        {
            Ok(r) => {
                let st = r.status().as_u16();
                if matches!(st, 503 | 504) {
                    last = format!("transient {st}"); // cold plane / edge blip — retry
                } else {
                    return Ok(st); // definitive (2xx / 402 / 429 / 404 / other 5xx)
                }
            }
            Err(e) => last = e.to_string(), // timeout / connection — retry
        }
        if attempt + 1 < MAX_PUT_ATTEMPTS {
            // Exponential backoff capped at 8s: 1,2,4,8,8,8,8 — rides a cold-start
            // window (container boot + slow D1) without a fixed-pause storm.
            let backoff = (1u64 << attempt).min(8);
            std::thread::sleep(std::time::Duration::from_secs(backoff));
        }
    }
    Err(format!(
        "data plane unreachable after {MAX_PUT_ATTEMPTS} attempts ({last})"
    ))
}

/// Number of priming READS the warm-up phase issues before a timed cap-drive.
const WARMUP_PRIMING_READS: usize = 3;

/// Per-priming-read attempt budget (generous, exponential 1..16 s) — warm-up
/// rides a cold container far more patiently than the timed drive does.
const WARMUP_READ_ATTEMPTS: usize = 6;

/// WARM-UP the per-tenant data plane BEFORE the timed cap-drive: issue a few
/// priming GET requests so the cold container boots (~2.5s) and the quota +
/// tombstone D1-over-HTTP hops are exercised (docs/perf/2026-06-19-cas-hot-path-
/// latency.md) WHILE we are still patient — instead of eating a cold-start 503
/// inside the timed drive where it can rotate the journey to a transient gate.
///
/// We deliberately use READS (GET of a random unknown content-address), NOT
/// writes: a GET warms the container + the same D1 hops a PUT takes, but it does
/// NOT mutate the tenant's quota — so warm-up cannot consume the carefully-seeded
/// under-cap headroom the drive needs to start below its limit, and it cannot
/// pre-trip (or mask) the cap. A 404 (unknown hash) / 200 / 401 — ANY definitive
/// response — proves the plane is warm + reachable for that probe. Transient
/// 503/504/timeout are retried with a generous exponential budget; only a
/// PERSISTENT fault after the whole budget is an `Err` (the caller GATES — a cold
/// or outaged plane is not a cap result, never a FAIL).
fn warm_up_plane(
    client: &Client,
    cfg: &Config,
    tenant: &str,
    token: &str,
) -> Result<(), String> {
    for p in 0..WARMUP_PRIMING_READS {
        // Random unknown content-address → a pure read: warms the container + the
        // quota/tombstone D1 hops, mutates nothing.
        let probe = format!("quota-warmup-{}-{p}", uuid::Uuid::new_v4()).into_bytes();
        let hash = blake3_hex(&probe);
        let url = url_cas(cfg, tenant, &hash);
        let mut last = String::new();
        let mut reached = false;
        for attempt in 0..WARMUP_READ_ATTEMPTS {
            match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
                Ok(r) => {
                    let st = r.status().as_u16();
                    if matches!(st, 503 | 504) {
                        last = format!("transient {st}"); // still cold — keep priming
                    } else {
                        reached = true; // any definitive response ⇒ warm + reachable
                        break;
                    }
                }
                Err(e) => last = e.to_string(),
            }
            if attempt + 1 < WARMUP_READ_ATTEMPTS {
                // Exponential 1,2,4,8,16 s — warm-up is patient by design.
                let backoff = (1u64 << attempt).min(16);
                std::thread::sleep(std::time::Duration::from_secs(backoff));
            }
        }
        if !reached {
            return Err(format!(
                "warm-up read #{p} could not reach a warm data plane ({last})"
            ));
        }
    }
    Ok(())
}

/// Resolve the (tenant, token) the destructive quota drives target.
///
/// Prefers a DEDICATED throwaway quota tenant + its PAT
/// (`CORELINK_E2E_QUOTA_TENANT` / `CORELINK_E2E_PAT_QUOTA`) so the cap-drive
/// never touches the shared primary tenant — that dedicated tenant is seeded
/// with a low `tenant_quota` ceiling + accrued headroom, so a bounded drive
/// deterministically trips 402. Falls back to the primary tenant + P1 RW PAT
/// (the legacy behaviour, which gates as "cap unobserved" on a far-from-limit
/// shared tenant). Returns the gate/`JourneyResult` to short-circuit on missing
/// creds.
pub(crate) fn quota_target<'a>(
    cfg: &'a Config,
    name: &'static str,
) -> Result<(String, &'a str), JourneyResult> {
    if let (Some(t), Some(tok)) = (cfg.quota_tenant.clone(), cfg.token(TokenKind::Quota)) {
        return Ok((t, tok));
    }
    let p1 = Persona::P1ReadWrite
        .resolve(cfg)
        .map_err(|reason| JourneyResult::gated(name, reason))?;
    let tenant = cfg
        .tenant
        .clone()
        .ok_or_else(|| JourneyResult::gated(name, "CORELINK_E2E_TENANT not set"))?;
    Ok((tenant, p1.token.expect("P1 has a token")))
}

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

/// Number of bounded blobs the aggressive cap-drive will write at most before
/// declaring the cap unobserved. Bounded BY CONSTRUCTION so a non-near-limit
/// account cannot fill prod R2 unboundedly (count × size is the hard ceiling).
const AGGRESSIVE_MAX_BLOBS: usize = 256;

/// Per-blob size for the aggressive cap-drive (64 KiB). With AGGRESSIVE_MAX_BLOBS
/// this caps the total written at ~16 MiB — enough to push a near-limit test
/// tenant over its cap, small enough to never balloon prod storage.
const AGGRESSIVE_BLOB_BYTES: usize = 64 * 1024;

/// Run the quota journeys: under-cap-serves (the positive half) + hard-cap
/// enforcement (the destructive half, flag-gated) + the bounded aggressive
/// cap-drive that proves a CLEAN 402/429 (never a 5xx, never silent over-serve)
/// and that an under-cap write still serves 2xx (flag-gated).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    // Order matters when a DEDICATED quota tenant is used: the cap budget
    // accrues monotonically across the three journeys (shared tenant), so the
    // two journeys that need an under-cap SERVE to start (under_cap_serves +
    // the combined cap-drive's under-cap probe) run BEFORE quota_hard_cap, which
    // has no under-cap precondition and simply writes until it sees the 402/429.
    // The provisioner seeds ~3 ops of headroom, enough for both under-cap probes
    // + one drive write before the cap trips.
    vec![
        under_cap_serves(cfg, client),
        quota_hard_cap_clean_and_under_cap_serves(cfg, client),
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

    let (tenant, token) = match quota_target(cfg, name) {
        Ok(x) => x,
        Err(gate) => return gate,
    };

    // WARM-UP: prime the cold container + D1 hops with reads before the timed
    // write, so a ~2.5s cold start can't 503 the probe. A persistent fault here
    // is a cold/outaged plane, not a cap result → GATE.
    if let Err(reason) = warm_up_plane(client, cfg, &tenant, token) {
        return JourneyResult::gated(name, format!("warm-up: {reason}"));
    }

    // A small unique blob — well under any tier cap for a healthy test tenant.
    let blob = format!("quota-undercap-{}", uuid::Uuid::new_v4()).into_bytes();
    let hash = blake3_hex(&blob);
    let url = url_cas(cfg, &tenant, &hash);

    let status = match put_blob_tolerant(client, &url, token, &blob) {
        Ok(s) => s,
        // A persistent transient fault (slow/cold prod) is not a cap result.
        Err(reason) => return JourneyResult::gated(name, format!("under-cap probe: {reason}")),
    };
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
    let (tenant, token) = match quota_target(cfg, name) {
        Ok(x) => x,
        Err(gate) => return gate,
    };

    // WARM-UP before the cap-drive: boot the cold container + exercise the D1 hops
    // with reads, so the drive's first writes don't eat a cold-start 503 inside
    // the timed loop. Persistent fault ⇒ cold/outaged plane, not a cap → GATE.
    if let Err(reason) = warm_up_plane(client, cfg, &tenant, token) {
        return JourneyResult::gated(name, format!("warm-up: {reason}"));
    }

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
        let url = url_cas(cfg, &tenant, &hash);

        let status = match put_blob_tolerant(client, &url, token, &blob) {
            Ok(s) => s,
            // Persistent transient fault (cold plane / slow-prod) — not a quota
            // result; GATE rather than flaky-FAIL on a prod blip.
            Err(reason) => {
                return JourneyResult::gated(name, format!("PUT #{i}: {reason}"))
            }
        };
        // Hard cap = 402 (ADR-0068 monthly $-ceiling) or 429 (rate cap). Either
        // proves the cap is enforced, not silently overaged.
        if status == 402 || status == 429 {
            return JourneyResult::pass(name, ms(start)); // hard cap enforced
        }
        // A NON-transient 5xx (500/502 — 503/504 are retried in the helper) or a
        // 404 is a real fault: the data plane can't verify the cap.
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

/// The WP's launch-critical cap contract, in ONE flow: (a) an under-cap write
/// SERVES (2xx) — the cap denies overage, not normal use; then (b) a BOUNDED
/// aggressive drive pushes the tenant to its cap and asserts the rejection is a
/// CLEAN 402/429 — explicitly NOT a silent over-serve (a 2xx forever) and NOT a
/// 5xx (a crash-on-cap; the cap must reject cleanly, not fault).
///
/// SAFETY (critical): this MUTATES state (drives a tenant toward its cap). It is
/// GATED behind `CORELINK_E2E_QUOTA_TEST=1` (or the legacy `CORELINK_E2E_RUN_SLOW=1`)
/// and the drive is BOUNDED BY CONSTRUCTION — at most `AGGRESSIVE_MAX_BLOBS`
/// blobs of `AGGRESSIVE_BLOB_BYTES` each (~16 MiB total ceiling), so it can never
/// fill prod R2 unboundedly. Run it only against a near-limit DEDICATED test
/// tenant.
fn quota_hard_cap_clean_and_under_cap_serves(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Quota: bounded cap-drive — clean 402/429 (not 5xx, not silent over-serve) + under-cap 2xx";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if !quota_drive_enabled(cfg) {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_QUOTA_TEST=1 (or CORELINK_E2E_RUN_SLOW=1) not set — this drives a tenant \
             to its cap (bounded, ~16 MiB max); enable only against a near-limit DEDICATED test tenant",
        );
    }
    let (tenant, token) = match quota_target(cfg, name) {
        Ok(x) => x,
        Err(gate) => return gate,
    };

    // WARM-UP before BOTH halves: prime the cold container + D1 hops with reads so
    // neither the under-cap probe nor the timed drive eats a cold-start 503.
    // Persistent fault ⇒ cold/outaged plane, not a cap result → GATE.
    if let Err(reason) = warm_up_plane(client, cfg, &tenant, token) {
        return JourneyResult::gated(name, format!("warm-up: {reason}"));
    }

    // (a) UNDER-CAP first: a single small write must serve 2xx — proving a later
    // deny means "over cap", not "always denied". If THIS tenant is already at its
    // cap we cannot prove the positive half here → GATE (needs an under-cap start).
    let probe = format!("quota-clean-undercap-{}", uuid::Uuid::new_v4()).into_bytes();
    let probe_hash = blake3_hex(&probe);
    let probe_url = url_cas(cfg, &tenant, &probe_hash);
    let probe_status = match put_blob_tolerant(client, &probe_url, token, &probe) {
        Ok(s) => s,
        Err(reason) => return JourneyResult::gated(name, format!("under-cap probe: {reason}")),
    };
    match probe_status {
        200 | 201 => {}
        402 | 429 => {
            return JourneyResult::gated(
                name,
                "tenant already at its cap on the first write — the under-cap half needs a tenant \
                 starting below its limit",
            )
        }
        other => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("under-cap probe PUT got {other} (expected 200/201 serve)"),
            )
        }
    }

    // (b) BOUNDED aggressive drive: write fixed-size blobs until a CLEAN 402/429.
    // A 5xx is a HARD FAIL — the cap must reject cleanly, never fault. Reaching the
    // bound without a cap means the tenant is far below its limit (cap unobserved).
    let template = vec![b'q'; AGGRESSIVE_BLOB_BYTES];
    for i in 0..AGGRESSIVE_MAX_BLOBS {
        let mut blob = template.clone();
        let marker = format!("quota-clean-drive-{}-{i}", uuid::Uuid::new_v4());
        let mb = marker.as_bytes();
        if mb.len() < blob.len() {
            blob[..mb.len()].copy_from_slice(mb);
        }
        let hash = blake3_hex(&blob);
        let url = url_cas(cfg, &tenant, &hash);

        let status = match put_blob_tolerant(client, &url, token, &blob) {
            Ok(s) => s,
            // Persistent transient fault (cold plane / slow-prod) — not a cap
            // result; GATE rather than flaky-FAIL on a prod blip.
            Err(reason) => return JourneyResult::gated(name, format!("drive PUT #{i}: {reason}")),
        };
        // CLEAN cap rejection — the contract.
        if matches!(status, 402 | 429) {
            return JourneyResult::pass(name, ms(start));
        }
        // A NON-transient 5xx (500/502; 503/504 are retried in the helper) under
        // cap pressure = crash-on-cap, NOT a clean rejection → HARD FAIL.
        if status >= 500 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PUT #{i} got {status} — cap rejected with a 5xx (crash-on-cap), not a clean 402/429"),
            );
        }
        // A 404 means the data plane isn't operational; we can't verify the cap.
        if status == 404 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PUT #{i} got 404 — data plane not operational; cannot verify the cap"),
            );
        }
        // Any other non-2xx that isn't a recognised serve is a contract surprise.
        if !matches!(status, 200 | 201) {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PUT #{i} got {status} — neither a serve (2xx) nor a clean cap (402/429)"),
            );
        }
    }

    JourneyResult::fail(
        name,
        ms(start),
        format!(
            "wrote {AGGRESSIVE_MAX_BLOBS}×{AGGRESSIVE_BLOB_BYTES}B (~{} MiB) without a 402/429. Cap not observed — \
             the test tenant is far below its limit, or the cap is not enforced. The bounded drive \
             needs a NEAR-LIMIT dedicated test tenant.",
            (AGGRESSIVE_MAX_BLOBS * AGGRESSIVE_BLOB_BYTES) / (1024 * 1024)
        ),
    )
}
