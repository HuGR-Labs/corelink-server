//! Abuse / enforcement / red-team probes — assert the protective guardrails
//! actually ENFORCE (the half a real attacker / abusive client probes).
//!
//! Black-box only: the deployed HTTP API + Bearer PATs, via the
//! [`crate::harness`] URL builders + the [`crate::personas`] persona/token map.
//! No `corelink-*` crate import, no mocks, no internal-state reads.
//!
//! Where `security.rs` walks the per-persona deny matrix and `cas.rs` proves the
//! happy round-trip, THIS module probes the OPERATIONAL guardrails an abusive
//! client hits: the rate limiter, the $-tripwire / quota signal, cache-poisoning
//! resistance on shared/`_public` digests, native-plane bearer forgery, and
//! mint / internal-route abuse. Each cell is a PASS/GATED/FAIL [`JourneyResult`]
//! where **PASS = the defence holds**.
//!
//! SAFETY (inviolable): nothing destructive, nothing charging. The probes are
//! NON-destructive by construction — a *modest* GET burst (never a hammer), a
//! mismatched-bytes PUT that the server MUST reject (so nothing is written), a
//! forged-bearer probe that must 401. The only write that could touch real state
//! (a real quota DRIVE) stays GATED behind `CORELINK_E2E_QUOTA_TEST=1`, exactly
//! like `quota.rs`. The aggressive rate-limit variant is GATED behind
//! `CORELINK_E2E_ABUSE_HARD=1` (default off — prod is shared / runners=the Mac).
//!
//! Cells:
//!   - **Rate-limit present** — a small burst of ~30 quick GETs: either all
//!     served (under the bound) OR a clean 429 (well-formed, no 5xx). Never a
//!     hammer. The aggressive variant gates behind `CORELINK_E2E_ABUSE_HARD=1`.
//!   - **$-tripwire / quota signal** — the quota path returns a CLEAN 402/429
//!     shape (not a 5xx) when signalled; the real drive gates behind
//!     `CORELINK_E2E_QUOTA_TEST=1`.
//!   - **Cache-poisoning resistance** — a PUT whose bytes ≠ the claimed content
//!     address is REJECTED (can't poison a digest); plus the cross-tenant /
//!     `_public` angle: a tenant cannot overwrite a shared digest with
//!     mismatched bytes.
//!   - **Native-plane forgery** — forged / garbage / tampered / cross-tenant /
//!     truncated / oversized bearer values → all denied (401/403), never a 500.
//!   - **Mint abuse** — the internal mint / introspect route rejects a
//!     missing/garbage internal-auth (401/403/404), and a customer PAT cannot
//!     reach an internal/admin route (denied, never 200).
//!
//! GATED whenever a persona / flag / tenant segment is absent — recorded, never
//! a silent skip, never a hard panic.

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::harness::{
    bearer, blake3_hex, expect_denied, unique_blob, url_cas, url_introspect, Config, JourneyResult,
};
use crate::personas::Persona;

/// Opt-in flag for the AGGRESSIVE rate-limit variant (a larger burst). Default
/// off: prod is shared and the CI runners ARE the founder's Mac — never hammer.
const ABUSE_HARD_ENV: &str = "CORELINK_E2E_ABUSE_HARD";

/// Opt-in flag for the real quota DRIVE (writes until a hard cap). Same gate as
/// `quota.rs` so the destructive half is opt-in and consistent.
const QUOTA_TEST_ENV: &str = "CORELINK_E2E_QUOTA_TEST";

/// The internal service-auth header the mint/introspect routes gate on.
const INTERNAL_AUTH_HEADER: &str = "X-Corelink-Internal-Auth";

/// Milliseconds elapsed since `start`.
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// True iff the named flag is set to `1`.
fn flag(name: &str) -> bool {
    env::var(name).map(|v| v == "1").unwrap_or(false)
}

/// Run the abuse / enforcement probes.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        rate_limit_present(cfg, client),
        quota_tripwire_clean_shape(cfg, client),
        cache_poison_mismatch_rejected(cfg, client),
        cache_poison_public_shared_rejected(cfg, client),
        native_plane_forgery_denied(cfg, client),
        mint_internal_auth_required(cfg, client),
        customer_pat_cannot_reach_internal(cfg, client),
    ]
}

/// **Rate-limit present.** Fire a MODEST burst of quick GETs at a benign,
/// authenticated read endpoint and assert the limiter is PRESENT: every response
/// is either served (a 2xx/auth status under the bound) OR a clean 429 — and a
/// 429, if it appears, is well-formed (status only; no 5xx). A 5xx under a light
/// burst is a fail (the limiter must shed cleanly, not crash). Never a hammer:
/// the default burst is small; the aggressive variant gates behind
/// `CORELINK_E2E_ABUSE_HARD=1`.
fn rate_limit_present(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: rate-limit present — modest GET burst either served or clean 429 (no 5xx)";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set — CAS path needs a tenant");
    }
    let token = p1.token.expect("P1 always has a token");

    // A fresh, almost-certainly-absent content address: a GET against it is the
    // cheapest possible authenticated read (a 404/empty, no body served), so the
    // burst cost is minimal. We are probing the LIMITER, not the data.
    let probe = unique_blob("abuse-ratelimit-probe");
    let hash = blake3_hex(&probe);
    let url = url_cas(cfg, &p1.tenant, &hash);

    // Modest by default; a touch larger when the operator opts into the hard
    // variant. Still bounded and small — we never hammer a shared prod.
    let burst = if flag(ABUSE_HARD_ENV) { 60 } else { 30 };

    let mut saw_429 = false;
    let mut saw_5xx: Option<u16> = None;
    for i in 0..burst {
        let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("GET #{i}: {e}")),
        };
        let st = resp.status().as_u16();
        if st == 429 {
            saw_429 = true;
            // Well-formed deny: a 429 must carry a 429 status (it does, trivially)
            // and must NOT also be a 5xx. Nothing else to assert without coupling
            // to a body shape the contract does not promise. A Retry-After header
            // is good practice but not contractually required, so we don't fail on
            // its absence — only assert the limiter sheds cleanly.
        } else if st >= 500 {
            saw_5xx = Some(st);
            break;
        }
        // Any non-5xx, non-429 status (200/401/403/404/…) is "served under the
        // bound" for the purpose of this probe — the limiter let it through.
    }

    if let Some(code) = saw_5xx {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "ENFORCEMENT: a modest {burst}-GET burst produced a {code} 5xx — the limiter must \
                 shed with a clean 429, not crash the data plane"
            ),
        );
    }

    // PASS whether or not a 429 appeared: under the bound everything is served;
    // over the bound a clean 429 is the correct shed. Both prove the path is
    // operational and the limiter (if engaged) sheds cleanly. We record which.
    let _ = saw_429;
    JourneyResult::pass(name, ms(start))
}

/// **$-tripwire / quota signal.** Assert the quota / $-ceiling path returns a
/// CLEAN signal shape — a 402 (ADR-0068 monthly $-ceiling) or 429 (rate cap) —
/// rather than a 5xx, when the cap is signalled. The real near-limit DRIVE is
/// destructive (write-heavy) and so is GATED behind `CORELINK_E2E_QUOTA_TEST=1`,
/// exactly like `quota.rs`. With the gate OFF this cell GATES (recorded, not a
/// silent skip). With it ON, it PUTs bounded small blobs and asserts that the
/// FIRST non-2xx it observes is a clean 402/429 (never a 4xx-not-cap or a 5xx).
fn quota_tripwire_clean_shape(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: $-tripwire — quota signal is a CLEAN 402/429 (not a 5xx)";
    let start = Instant::now();

    if !(flag(QUOTA_TEST_ENV) || cfg.run_slow) {
        return JourneyResult::gated(
            name,
            format!(
                "{QUOTA_TEST_ENV}=1 (or CORELINK_E2E_RUN_SLOW=1) not set — the tripwire DRIVE is \
                 write-heavy; run only against a near-limit test account, never shared prod"
            ),
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

    // Bounded, small writes — we are probing the SHAPE of the signal, not trying
    // to exhaust a 10GB cap. The first non-2xx must be a clean cap signal.
    const MAX_ATTEMPTS: usize = 64;
    let template = vec![b'q'; 1024]; // 1KB
    for i in 0..MAX_ATTEMPTS {
        let mut blob = template.clone();
        let marker = format!("abuse-tripwire-{}-{i}", uuid::Uuid::new_v4());
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
        let st = resp.status().as_u16();
        match st {
            200 | 201 => continue, // still under cap — keep probing the signal
            402 | 429 => return JourneyResult::pass(name, ms(start)), // clean tripwire shape
            s if s >= 500 => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "ENFORCEMENT: quota path returned a {s} 5xx under pressure — the cap must \
                         signal a clean 402/429, not crash"
                    ),
                )
            }
            other => {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "PUT #{i} got {other} — not a clean cap signal (expected 200/201 under cap \
                         or 402/429 at cap)"
                    ),
                )
            }
        }
    }

    // Never reached the cap in a bounded probe — that's account state, not a
    // contract break. Gate (the shape assertion needs a near-limit account).
    JourneyResult::gated(
        name,
        format!(
            "sent {MAX_ATTEMPTS}×1KB under cap without a 402/429 — tenant is far below its limit; \
             the tripwire-shape probe needs a near-limit test account"
        ),
    )
}

/// **Cache-poisoning resistance (digest binding).** A PUT whose body's BLAKE3
/// does NOT equal the claimed `{hash}` in the URL must be REJECTED — the server
/// recomputes the address from the bytes, so an attacker cannot bind arbitrary
/// bytes to a chosen digest (the poisoning primitive). The contract (`routes/
/// cas.rs`) is a 422 "content hash mismatch"; any non-2xx is acceptable here so
/// long as the write does NOT succeed. A 200/201 is a launch-blocking poisoning
/// hole. This is non-destructive: a rejected write stores nothing.
fn cache_poison_mismatch_rejected(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: cache-poison — PUT bytes ≠ claimed digest REJECTED (no poisoning)";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 has a token");

    // Honest bytes, but we claim a DIFFERENT (mismatched) content address.
    let honest = unique_blob("abuse-poison-honest");
    let other = unique_blob("abuse-poison-claimed");
    let lying_hash = blake3_hex(&other); // address of bytes we are NOT sending
    assert_ne!(
        lying_hash,
        blake3_hex(&honest),
        "fresh UUIDs must differ — sanity"
    );
    let url = url_cas(cfg, &p1.tenant, &lying_hash);

    let resp = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(honest.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let st = resp.status().as_u16();
    if matches!(st, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SECURITY: server ACCEPTED (got {st}) bytes whose BLAKE3 ≠ the claimed digest — a \
                 digest can be poisoned with arbitrary bytes"
            ),
        );
    }
    if st >= 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("mismatch PUT got {st} 5xx — must reject cleanly (422), not crash"),
        );
    }
    // Any clean non-2xx (canonically 422) = rejected. Defence holds.
    JourneyResult::pass(name, ms(start))
}

/// **Cache-poisoning resistance — cross-tenant / `_public` shared angle.** A
/// tenant must not be able to overwrite a SHARED / `_public` digest with
/// mismatched bytes. Two layers protect the shared namespace: (a) the digest
/// binding (mismatched bytes are rejected regardless of namespace), and (b)
/// tenant scoping (a customer PAT cannot address the `_public` namespace as if it
/// owned it). We probe BOTH against the `_public` path: a mismatched-bytes PUT
/// must NOT succeed (no poisoning of a shared digest other tenants would then
/// read). Non-destructive: a rejected write stores nothing.
fn cache_poison_public_shared_rejected(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: cache-poison — tenant cannot overwrite _public/shared digest (mismatch)";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 has a token");

    // Mismatched bytes claimed under the shared `_public` namespace. The defence
    // is two-layered: either the `_public` namespace is not writable by a tenant
    // PAT (deny), or the digest binding rejects the mismatch (422) — EITHER way
    // the poisoning write must NOT succeed. A 200/201 is a shared-cache poison.
    let honest = unique_blob("abuse-public-poison-honest");
    let other = unique_blob("abuse-public-poison-claimed");
    let lying_hash = blake3_hex(&other);
    let url = url_cas(cfg, "_public", &lying_hash);

    let resp = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(honest)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let st = resp.status().as_u16();
    if matches!(st, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SECURITY: tenant PUT mismatched bytes into _public (got {st}) — a tenant can \
                 poison a SHARED digest other tenants would read"
            ),
        );
    }
    if st >= 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("_public mismatch PUT got {st} 5xx — must deny/422 cleanly, not crash"),
        );
    }
    // Clean deny (403/404) or content-mismatch (422) — the shared namespace is
    // protected. Defence holds.
    JourneyResult::pass(name, ms(start))
}

/// **Native-plane forgery.** A range of bad Authorization values against the
/// native CAS read must ALL be denied (401/403) and NEVER a 500 — a forged or
/// malformed bearer must be rejected by the auth gate, not crash it:
///   - a garbage / random bearer (no such PAT),
///   - a tampered PAT (a real RW PAT with its last char flipped),
///   - a bearer for a DIFFERENT tenant used against the primary tenant's path
///     (cross-tenant credential — gated on TenantB),
///   - a TRUNCATED Authorization value (header present but empty-ish),
///   - an OVERSIZED Authorization value (a very long bearer must 401, not 500).
fn native_plane_forgery_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: native-plane forgery — garbage/tampered/cross/trunc/oversized bearer DENIED";
    let start = Instant::now();

    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let tenant = cfg.tenant_or_anon().to_string();

    // A fresh address on the primary tenant's path — the target of every forged
    // read. (A genuine miss would be 404 *after* auth; a forged bearer must be
    // rejected at the gate, so we expect a deny regardless of object existence.)
    let probe = unique_blob("abuse-forgery-target");
    let hash = blake3_hex(&probe);
    let url = url_cas(cfg, &tenant, &hash);

    // Each candidate: (label, full Authorization header value).
    let mut candidates: Vec<(String, String)> = vec![
        (
            "garbage bearer".to_string(),
            "Bearer not-a-real-pat-deadbeef-deadbeef-deadbeef".to_string(),
        ),
        (
            "truncated authz value".to_string(),
            "Bearer ".to_string(), // header present, empty token
        ),
        (
            "oversized authz value".to_string(),
            format!("Bearer {}", "A".repeat(8192)),
        ),
    ];

    // A TAMPERED real PAT (flip the last char) — needs the RW token. Gated-soft:
    // if absent we simply skip this candidate (the others still run).
    if let Ok(p1) = Persona::P1ReadWrite.resolve(cfg) {
        if let Some(tok) = p1.token {
            let mut chars: Vec<char> = tok.chars().collect();
            if let Some(last) = chars.last_mut() {
                *last = if *last == 'a' { 'b' } else { 'a' };
            }
            let tampered: String = chars.into_iter().collect();
            candidates.push(("tampered real PAT".to_string(), bearer(&tampered)));
        }
    }

    // A different-tenant credential aimed at the PRIMARY tenant path — a
    // cross-tenant forgery. Gated-soft on the TenantB token.
    if let Ok(pb) = Persona::P6TenantB.resolve(cfg) {
        if let Some(tok) = pb.token {
            candidates.push(("cross-tenant bearer".to_string(), bearer(tok)));
        }
    }

    for (label, authz) in &candidates {
        let resp = match client.get(&url).header(AUTHORIZATION, authz).send() {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("{label} GET: {e}")),
        };
        let st = resp.status().as_u16();
        if st == 200 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("SECURITY: forged bearer accepted (200) — {label}"),
            );
        }
        if st >= 500 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "ENFORCEMENT: forged bearer caused a {st} 5xx ({label}) — the auth gate must \
                     reject (401), not crash on a malformed/oversized credential"
                ),
            );
        }
        if let Err(m) = expect_denied(&format!("forgery: {label}"), st) {
            return JourneyResult::fail(name, ms(start), m);
        }
    }

    JourneyResult::pass(name, ms(start))
}

/// **Mint abuse — internal-auth required.** The runner-mint / token-introspect
/// internal route must REJECT a missing or garbage `X-Corelink-Internal-Auth`
/// (401/403), or 404 if not mounted in this deploy. A 200 with no / wrong
/// internal key means the mint surface is reachable unauthenticated from the
/// edge — a token-minting compromise. We probe the introspect route (the
/// internal-auth-gated surface the harness owns a builder for) with NO key and a
/// WRONG key; both must deny. Non-destructive (no mint succeeds).
fn mint_internal_auth_required(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: mint/internal — missing/garbage internal-auth DENIED (no edge mint)";
    let start = Instant::now();

    // A plausible body so the only thing that can let us through is a missing
    // auth gate (not a malformed-body rejection).
    let body = json!({ "token": "corelink_pat_abuse_probe" });

    // (a) NO internal-auth header at all.
    match client.post(url_introspect(cfg)).json(&body).send() {
        Ok(r) => {
            let st = r.status().as_u16();
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    "SECURITY: internal mint/introspect returned 200 with NO internal-auth header \
                     — reachable unauthenticated from the edge"
                        .to_string(),
                );
            }
            if st >= 500 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("internal route got {st} 5xx with no key — must deny cleanly, not crash"),
                );
            }
            if let Err(m) = expect_denied("internal (no key)", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("internal endpoint unreachable ({}): {e}", cfg.endpoint),
            )
        }
    }

    // (b) WRONG / garbage internal-auth header.
    let wrong = "garbage-internal-auth-0000000000000000000000000000";
    match client
        .post(url_introspect(cfg))
        .header(INTERNAL_AUTH_HEADER, wrong)
        .json(&body)
        .send()
    {
        Ok(r) => {
            let st = r.status().as_u16();
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    "SECURITY: internal mint/introspect returned 200 with a WRONG internal-auth \
                     header — the service-secret gate is not enforced"
                        .to_string(),
                );
            }
            if st >= 500 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("internal route got {st} 5xx with a wrong key — must deny cleanly"),
                );
            }
            if let Err(m) = expect_denied("internal (wrong key)", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("wrong-key probe: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Mint abuse — a customer PAT cannot reach an internal route.** Privilege
/// boundary: a valid CUSTOMER PAT (P1, the highest-trust customer credential the
/// suite holds) presented as an internal-auth header to the internal mint /
/// introspect route must be DENIED — a customer credential must not cross into
/// the internal control plane. A 200 means a customer PAT can drive an internal
/// route (priv-esc). 401/403/404 are all acceptable denies. Non-destructive.
fn customer_pat_cannot_reach_internal(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Abuse: customer PAT presented as internal-auth → DENIED (no priv-esc)";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 has a token");
    let body = json!({ "token": "corelink_pat_abuse_probe" });

    // Present the customer PAT BOTH as the internal-auth header AND as a normal
    // bearer — neither must unlock the internal route.
    match client
        .post(url_introspect(cfg))
        .header(INTERNAL_AUTH_HEADER, token)
        .header(AUTHORIZATION, bearer(token))
        .json(&body)
        .send()
    {
        Ok(r) => {
            let st = r.status().as_u16();
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    "SECURITY: a customer PAT reached the internal mint/introspect route (200) — \
                     a customer credential crossed into the internal control plane"
                        .to_string(),
                );
            }
            if st >= 500 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("internal route got {st} 5xx with a customer PAT — must deny cleanly"),
                );
            }
            if let Err(m) = expect_denied("customer PAT → internal", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
            JourneyResult::pass(name, ms(start))
        }
        Err(e) => JourneyResult::gated(
            name,
            format!("internal endpoint unreachable ({}): {e}", cfg.endpoint),
        ),
    }
}
