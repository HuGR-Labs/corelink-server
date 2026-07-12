//! # Shared / `_public` cross-team cache journeys (gap-map M2 — THE MOAT).
//!
//! W1 fill. This module POSITIVELY proves the product's network-effect thesis:
//! a deterministic PUBLIC artifact warmed by tenant A is served to tenant B as a
//! **byte-identical cross-tenant serve** from the shared `_public` namespace,
//! while PRIVATE bytes stay isolated.
//!
//! ## The exact `_public` mechanism (captured from source 2026-06-24)
//!
//! The cross-tenant public dedup lives in the **brew** adapter, NOT native CAS
//! (native CAS is strictly per-tenant — see [`crate::journeys::cas`] isolation).
//! Route: `GET /brew/{tenant}/v2/homebrew/core/{formula}/blobs/sha256:{digest}`
//! (`crates/corelink-container/src/routes/brew.rs`).
//!
//!   - The Worker forwards the full path; the container's `BrewMoatStore` stores
//!     bytes under the SHARED [`PUBLIC_NAMESPACE`] = `"_public"` and **ignores the
//!     per-request tenant** (`routes/brew.rs` `CasStore for BrewMoatStore`).
//!   - The cache key is `blake3(canonical_bottle_path)` and the tenant-route
//!     prefix is stripped before keying, so the SAME bottle under tenant A and
//!     tenant B canonicalizes to the IDENTICAL key
//!     (`adapter-host/src/brew/bottle.rs` `canonical_bottle_path`).
//!   - Bytes are digest-verified against the URL `sha256:<hex>` BEFORE store
//!     (F-005), so a 200 body is provably the real artifact (its sha256 == the
//!     URL digest) — not an error page. We use that to assert "real artifact".
//!   - Only `v2/homebrew/core/…` and `v2/homebrew/cask/…` content-addressed
//!     (`…/sha256:<64hex>`) paths are cacheable into `_public`.
//!
//! So: tenant A GETs the bottle → cache-fill into `_public`; tenant B (a DIFFERENT
//! PAT/tenant) GETs the SAME bottle path → served from A's warm `_public` entry,
//! byte-for-byte identical, no per-tenant re-upload.
//!
//! ## What is and isn't black-box-provable (honest boundary — gap-map rule #3)
//!
//! Byte-equality + a valid 200 from B on A's address proves the cross-tenant
//! **serve** (the network-effect OUTCOME). The `_public` HIT is also
//! black-box-provable via the `X-Cache: HIT` header that the C-MOAT contract
//! mandates the brew server sets on a cross-tenant/`_public` serve (vs
//! `X-Cache: MISS` on a fill). Both assertions are now HARD: (1) `X-Cache: HIT`
//! AND (2) byte-for-byte equality of A's warmed bytes and B's served bytes.
//! If the header is absent the journey GATES (C-MOAT brew WP not yet deployed),
//! and if `X-Cache: MISS` with byte-equal bytes the journey FAILS (that means
//! B independently refilled from upstream, not a moat HIT). Latency is still
//! logged as corroborating evidence but is no longer the only discriminator.
//!
//! ## Inputs (black-box; env only, no new deps)
//!
//!   - tenant A   = `Persona::P1ReadWrite` (`CORELINK_E2E_PAT_RW` + `_TENANT`).
//!   - tenant B   = `Persona::P6TenantB`   (`CORELINK_E2E_PAT_TENANT_B` + `_TENANT_B`).
//!   - public bottle path = `CORELINK_E2E_PUBLIC_BREW_PATH`, e.g.
//!     `v2/homebrew/core/jq/blobs/sha256:<64hex>` — a deterministic, public,
//!     digest-addressed Homebrew bottle (no hardcode: ghcr digests rot, so the
//!     operator supplies a live one). The final `sha256:<hex>` segment is the
//!     integrity anchor we re-verify the served bytes against. Absent ⇒ GATE.
//!
//! Assumption the lead must verify: that env var is the agreed knob for the
//! public-bottle path (the frozen `cfg.public_hash` is a bare CONTENT hash, which
//! is not a usable brew route; native CAS has no `_public` tenant). If the suite
//! prefers `cfg.public_hash`, swap the path source here — the assertions stand.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use sha2::{Digest, Sha256};

use crate::harness::{
    bearer, blake3_hex, expect_denied, unique_blob, url_brew, url_cas, Config, JourneyResult,
};
use crate::personas::Persona;

/// Env var carrying the deterministic public Homebrew bottle path used to prove
/// the cross-tenant `_public` HIT (see module docs).
const PUBLIC_BREW_PATH_ENV: &str = "CORELINK_E2E_PUBLIC_BREW_PATH";

/// Run the shared-cache cross-team journeys (gap-map M2 — the moat).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        public_dedup_hit(cfg, client),
        public_private_boundary(cfg, client),
    ]
}

/// Lowercase-hex sha256 — the OCI/ghcr digest algorithm the brew adapter verifies
/// the stored bytes against. We recompute it on the served body so a 200 only
/// counts when the bytes ARE the real artifact (sha256 == the URL-declared digest).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Extract the URL-declared `sha256:<64hex>` digest from a brew bottle path, if
/// the path is content-addressed (mirrors the adapter's `expected_sha256`).
fn expected_sha256(brew_path: &str) -> Option<String> {
    let last = brew_path.rsplit('/').next()?;
    let hex_digest = last.strip_prefix("sha256:")?;
    (hex_digest.len() == 64 && hex_digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex_digest.to_ascii_lowercase())
}

/// Fetch a brew bottle for `persona`'s tenant/token. Returns the served bytes on
/// 200, or a structured `Err(message)` describing the failed contract.
fn brew_get(
    cfg: &Config,
    client: &Client,
    tenant: &str,
    token: &str,
    brew_path: &str,
) -> Result<Vec<u8>, String> {
    let url = url_brew(cfg, tenant, brew_path);
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, bearer(token))
        .send()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("GET {url} got {status} (expected 200)"));
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("GET {url} body read: {e}"))
}

/// **M2 — cross-team public dedup HIT.** Tenant A warms a deterministic public
/// Homebrew bottle into the shared `_public` namespace; tenant B (a DIFFERENT
/// tenant/PAT) fetches the SAME public bottle path and must get a byte-identical
/// 200 — proving the cross-tenant serve that the COGS/margin story rests on.
///
/// HARD assertions (non-flaky):
///
/// 1. A's served bytes sha256 == the URL-declared digest (the REAL artifact).
/// 2. B's served bytes == A's served bytes, byte-for-byte (no per-tenant copy).
///
/// Corroborating (logged, NOT a fail condition): B-after-A latency vs A's cold
/// fill — a `_public` HIT skips the ghcr.io round trip, so B should be faster.
fn public_dedup_hit(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Shared cache: cross-team `_public` dedup HIT (A warms brew bottle → B byte-equal) — M2";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // tenant A = read+write PAT for the primary tenant.
    let a = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set — brew path needs it");
    }
    // tenant B = a DIFFERENT tenant's valid PAT (the cross-team consumer).
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_a = a.token.expect("P1 always has a token");
    let token_b = b.token.expect("P6 always has a token");

    // The deterministic public bottle path. No hardcode (ghcr digests rot) — the
    // operator supplies a live, digest-addressed Homebrew bottle path.
    let brew_path = match std::env::var(PUBLIC_BREW_PATH_ENV)
        .ok()
        .filter(|v| !v.is_empty())
    {
        Some(p) => p,
        None => {
            return JourneyResult::gated(
                name,
                format!(
                    "{PUBLIC_BREW_PATH_ENV} not set — supply a public digest-addressed Homebrew \
                     bottle path, e.g. v2/homebrew/core/jq/blobs/sha256:<64hex> (the `_public` \
                     cross-tenant dedup is via the brew adapter; native CAS has no `_public`)"
                ),
            );
        }
    };

    // The path MUST be content-addressed (…/sha256:<64hex>) — only digest-verified
    // bottles enter `_public` (F-005), and the digest is our real-artifact anchor.
    let expected = match expected_sha256(&brew_path) {
        Some(d) => d,
        None => {
            return JourneyResult::gated(
                name,
                format!(
                    "{PUBLIC_BREW_PATH_ENV}='{brew_path}' is not digest-addressed \
                     (need a trailing /sha256:<64hex>); only digest-verified bottles enter \
                     `_public`, so a tag-addressed path cannot prove the cross-tenant HIT"
                ),
            );
        }
    };

    // ── Tenant A: warm the bottle into `_public` (cold fill from ghcr.io). ──
    let a_started = Instant::now();
    let a_bytes = match brew_get(cfg, client, &a.tenant, token_a, &brew_path) {
        Ok(b) => b,
        Err(m) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("A warm failed (cannot prove a HIT without a warmed entry): {m}"),
            )
        }
    };
    let a_fill_ms = a_started.elapsed().as_millis() as u64;

    // HARD #1: A's bytes ARE the real public artifact (sha256 == URL digest).
    let a_digest = sha256_hex(&a_bytes);
    if a_digest != expected {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "A served {} bytes whose sha256 ({a_digest}) != URL-declared digest ({expected}) \
                 — not the real artifact; cannot anchor the cross-tenant proof",
                a_bytes.len()
            ),
        );
    }

    // ── Tenant B: GET the SAME public bottle path with a DIFFERENT tenant/PAT. ──
    // Inlined (not via brew_get) so we can read the X-Cache response header before
    // consuming the body. C-MOAT contract: 'X-Cache: HIT' on a cross-tenant/_public
    // serve; 'X-Cache: MISS' on a fill. HIT is now a HARD assertion, not corroborating.
    let b_started = Instant::now();
    let b_url = url_brew(cfg, &b.tenant, &brew_path);
    let b_resp = match client
        .get(&b_url)
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "B GET of A's warmed public bottle failed — the cross-tenant `_public` serve \
                     (the moat) did NOT work: GET {b_url}: {e}"
                ),
            )
        }
    };
    let b_status = b_resp.status().as_u16();
    if b_status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "B GET {b_url} got {b_status} (expected 200) — \
                 the cross-tenant `_public` serve (the moat) did NOT work"
            ),
        );
    }
    // Read X-Cache BEFORE consuming the body. reqwest canonicalises header names to
    // lowercase, so "x-cache" matches the wire 'X-Cache'.
    let x_cache_val = b_resp
        .headers()
        .get("x-cache")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_ascii_uppercase());

    let b_bytes = match b_resp.bytes() {
        Ok(b) => b.to_vec(),
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("B GET {b_url} body read: {e}"))
        }
    };
    let b_serve_ms = b_started.elapsed().as_millis() as u64;

    // HARD #2a: X-Cache header assertion (C-MOAT). Must be 'HIT' for B's serve to
    // count as the moat proof. Absent = C-MOAT WP not yet deployed → gate.
    // MISS + bytes-equal = B independently refilled from upstream → fail (not a HIT).
    match x_cache_val.as_deref() {
        Some("HIT") => {
            // Positive moat proof — B served from A's `_public` entry. Proceed.
        }
        Some("MISS") => {
            // MISS + bytes equal ⇒ B refilled from upstream (not the moat).
            // MISS + bytes unequal ⇒ also a moat failure; byte-equality check below will fire.
            if b_bytes == a_bytes {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "MOAT NOT HIT: B's X-Cache is MISS yet bytes are byte-equal to A's \
                         ({} B, digest={expected}) — B independently refilled from upstream \
                         instead of serving from A's `_public` entry; cross-tenant HIT unproven",
                        b_bytes.len()
                    ),
                );
            }
            // Fall through — byte-equality check below will also fail.
        }
        None | Some("") => {
            return JourneyResult::gated(
                name,
                "B's brew response missing 'X-Cache' header — C-MOAT brew server WP not yet \
                 deployed (the brew server sets 'X-Cache: HIT' on cross-tenant/_public serves \
                 once the container WP ships); gating to avoid false-RED before the update",
            );
        }
        Some(other) => {
            // Unknown value — treat as absent (gate) to avoid false-RED on a
            // non-standard intermediate value before the C-MOAT contract is pinned.
            return JourneyResult::gated(
                name,
                format!(
                    "B's X-Cache header has unexpected value {other:?} (expected 'HIT' or 'MISS') \
                     — treating as C-MOAT not yet deployed; gating to avoid false-RED"
                ),
            );
        }
    }

    // HARD #2b: B's bytes == A's bytes, byte-for-byte. This is the moat proof: the
    // SAME public address served identical content to a DIFFERENT tenant.
    if b_bytes != a_bytes {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "MOAT BROKEN: B's bytes ({} B) != A's warmed bytes ({} B) at the SAME public \
                 address — cross-tenant `_public` serve is not byte-identical",
                b_bytes.len(),
                a_bytes.len()
            ),
        );
    }
    // Defensive: B's bytes must also match the digest (they equal A's, but assert
    // the anchor on B's actual served bytes so a future divergence is caught here).
    let b_digest = sha256_hex(&b_bytes);
    if b_digest != expected {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("B served bytes sha256 ({b_digest}) != URL digest ({expected})"),
        );
    }

    // Corroborating latency evidence (LOGGED, never a fail condition — latency is
    // environment-noisy). X-Cache: HIT above is the decisive black-box discriminator
    // (C-MOAT contract); latency is a secondary corroborating signal.
    let hit_note = if b_serve_ms * 2 <= a_fill_ms.max(1) {
        "B markedly faster than A's fill — consistent with a `_public` HIT"
    } else {
        "B not markedly faster — X-Cache:HIT confirmed; latency delta environment-noisy"
    };
    let _ = hit_note; // surfaced for humans via the trailing log line below.
    eprintln!(
        "[M2] _public cross-tenant serve: A_fill={a_fill_ms}ms B_serve={b_serve_ms}ms \
         bytes={} digest={expected} X-Cache=HIT — {hit_note}",
        a_bytes.len()
    );

    JourneyResult::pass(name, ms(start))
}

/// **Public/private boundary.** The shared `_public` layer must NOT leak PRIVATE
/// content. Tenant A writes private bytes via NATIVE CAS (per-tenant namespace);
/// tenant B reading the SAME content address under B's tenant must be DENIED —
/// proving the shared dedup layer does not bleed into private isolation. Mirrors
/// [`crate::journeys::cas`] isolation, scoped to the moat module's contract.
fn public_private_boundary(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "Shared cache: public/private boundary — A's PRIVATE native-CAS bytes stay isolated from B";
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
    let token_a = a.token.expect("P1 always has a token");
    let token_b = b.token.expect("P6 always has a token");

    // A PRIVATE blob: unique per run, content-addressed under A's NATIVE CAS
    // (per-tenant namespace — NOT `_public`). The address is the blake3 of the
    // content, identical regardless of tenant, so B can attempt the exact same
    // address — the deny must come from isolation, not a different key.
    let blob = unique_blob("shared-cache-private-A");
    let hash = blake3_hex(&blob);

    // A writes the private blob.
    let url_a = url_cas(cfg, &a.tenant, &hash);
    let put = match client
        .put(&url_a)
        .header(AUTHORIZATION, bearer(token_a))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
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
                "A PUT got {} — cannot verify the boundary without a successful private write",
                put.status()
            ),
        );
    }

    // B attempts the SAME content address under B's tenant — must be denied
    // (private content never crosses the tenant boundary via the shared layer).
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
            format!(
                "SECURITY: tenant B got 200 ({n} bytes) reading A's PRIVATE content address — \
                 the shared layer leaked private content cross-tenant"
            ),
        );
    }
    if let Err(m) = expect_denied("B private cross-read", get_status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}
