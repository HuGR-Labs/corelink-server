//! Package-ecosystem adapter journeys — S6 cargo (sccache) / S7 npm / S8 pip /
//! S9 brew / S10 OCI.
//!
//! BLACK-BOX: deployed HTTP API + Bearer PAT only. We exercise the auth /
//! scope / tenant-isolation contract of each adapter surface through the public
//! edge — the slice that a raw HTTP probe CAN drive end-to-end. The cells that
//! genuinely need a package-manager client (npm/cargo/pip/brew tooling for the
//! real metadata-and-tarball protocol, or an upstream-registry round trip) are
//! GATED with a precise reason — never faked, never silently skipped.
//!
//! ## What is black-box exercisable vs gated
//!
//! - **cargo** (`/cargo/{tenant}/{key}`) is a plain sccache HTTP KV surface:
//!   PUT a key, GET it back, HEAD it. Fully exercisable with raw HTTP → happy
//!   round-trip + the P5 read-only-write→403 + P12 anonymous→deny + P10
//!   cross-tenant deny journeys are REAL here.
//! - **OCI** (`/token` + `/v2/...`) has a black-box two-leg auth handshake:
//!   `GET /token` with `Authorization: Basic base64("oci:<pat>")` mints an HMAC
//!   bearer; that bearer authorizes `/v2/*`. We drive both legs: a valid PAT
//!   must mint a token (200 + `{"token":…}`); no/invalid credential must be
//!   denied; the minted bearer must reach `/v2/`. Blob push/pull is the full
//!   OCI distribution protocol (chunked uploads, manifests) → GATED to a real
//!   OCI client.
//! - **npm / pip / brew** are registry/proxy surfaces whose happy path is the
//!   package-manager wire protocol (metadata docs, integrity-checked tarballs,
//!   PEP-503 simple index, ghcr bottle redirects). A raw HTTP GET cannot
//!   synthesize a valid fetch the way `npm`/`pip`/`brew` do, and the public
//!   tarball/wheel/bottle bytes are SHARED cross-tenant by design (the moat),
//!   so a cross-tenant "leak" of public bytes is not a finding. We therefore
//!   run the BLACK-BOX-OBSERVABLE security probes — anonymous→deny — and GATE
//!   the protocol happy path + the Worker-internal "quota-without-tenant
//!   fail-closed" cell (that header is Worker-set and stripped from client
//!   input — not reachable from a black-box client).
//!
//! Routes are taken from `crates/corelink-container/src/routes/{cargo,npm,pip,
//! brew,oci}.rs`; URLs go through the [`crate::harness`] builders only.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, expect_denied, sha256_hex, unique_blob, url_cargo, url_npm, url_oci_token, url_oci_v2,
    url_pip, Config, JourneyResult,
};
use crate::personas::Persona;

/// Run the adapter journeys (cargo / npm / pip / brew / OCI).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        // cargo (sccache) — fully black-box exercisable.
        cargo_store_fetch_round_trip(cfg, client),
        cargo_readonly_write_denied(cfg, client),
        cargo_anonymous_denied(cfg, client),
        cargo_cross_tenant_isolation(cfg, client),
        // OCI — two-leg auth handshake is black-box.
        oci_token_two_leg(cfg, client),
        oci_token_anonymous_denied(cfg, client),
        // npm / pip — anonymous deny is observable; protocol happy path gated.
        npm_anonymous_denied(cfg, client),
        npm_happy_gated(cfg, client),
        pip_anonymous_denied(cfg, client),
        // brew — public-bottle proxy; happy path gated.
        brew_happy_gated(cfg, client),
    ]
}

// ── cargo (sccache) ────────────────────────────────────────────────────────

/// Build a unique sccache-style key for a hermetic run.
fn cargo_key(blob: &[u8]) -> String {
    // sccache uses a content hash as the object key; mirror that shape.
    format!("e2e/{}", sha256_hex(blob))
}

/// cargo HAPPY: a read-write PAT stores an object under `/cargo/{tenant}/{key}`
/// and fetches the exact bytes back (PUT → GET → HEAD). This is the customer's
/// core sccache value — a real round trip over raw HTTP.
fn cargo_store_fetch_round_trip(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: store→fetch — PUT key, GET bytes match, HEAD exists";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — cargo path requires the tenant segment",
        );
    }
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("corelink-e2e-cargo");
    let key = cargo_key(&blob);
    let url = url_cargo(cfg, &p1.tenant, &key);

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
    if !matches!(put.status().as_u16(), 200 | 201 | 204) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT got {} (expected 200/201/204). url={url}", put.status()),
        );
    }

    // GET — bytes must match.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if get.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET got {} (expected 200)", get.status()),
        );
    }
    match get.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET bytes mismatch (put {} got {})", blob.len(), b.len()),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET body: {e}")),
    }

    // HEAD — existence probe (sccache checks this before upload). Some adapters
    // answer HEAD 200, some 204; accept either as "present".
    let head = match client
        .head(&url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("HEAD {url}: {e}")),
    };
    if !matches!(head.status().as_u16(), 200 | 204) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HEAD got {} (expected 200/204 for present key)", head.status()),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// cargo ADVERSARIAL (P5): a read-only PAT must NOT be able to write. The Worker
/// derives the scope from the PAT; the cargo gate's `requires_cache_write`
/// rejects a read-only scope with 403. (401/404 are also acceptable denies.)
fn cargo_readonly_write_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: P5 read-only PAT write → denied (no RO escalation)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p5 = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p5.token.expect("P2 has a token");

    let blob = unique_blob("corelink-e2e-cargo-ro");
    let key = cargo_key(&blob);
    let url = url_cargo(cfg, &p5.tenant, &key);

    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let status = put.status().as_u16();
    if matches!(status, 200 | 201 | 204) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("SECURITY: read-only PAT WROTE to cargo (got {status}) — scope not enforced"),
        );
    }
    if let Err(m) = expect_denied("cargo RO write", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// cargo ADVERSARIAL (P12): no Authorization header at all → must be denied.
fn cargo_anonymous_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: P12 anonymous GET → denied (auth required)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let tenant = cfg.tenant_or_anon().to_string();
    // A plausible key; the deny must happen at the gate, before any lookup.
    let key = "e2e/anonymous-probe";
    let url = url_cargo(cfg, &tenant, key);

    let get = match client.get(&url).send() {
        // Connection error ⇒ endpoint not under test (e.g. no env) → gate.
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("endpoint unreachable ({}): {e}", cfg.endpoint))
        }
    };
    let status = get.status().as_u16();
    if status == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "SECURITY: anonymous GET on cargo returned 200 — auth not enforced".to_string(),
        );
    }
    if let Err(m) = expect_denied("cargo anonymous", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// cargo ADVERSARIAL (P10): tenant A stores an object; tenant B fetches the SAME
/// key under B's own path and must NOT receive A's bytes. The adapter re-derives
/// the tenant from the PAT (ignoring the path segment) and namespaces per
/// tenant, so B sees a miss/deny — never A's content.
fn cargo_cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: P10 cross-tenant — A stores; B fetches same key → no leak";
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

    let blob = unique_blob("tenant-a-cargo-secret");
    let key = cargo_key(&blob);

    // A stores under A's path.
    let url_a = url_cargo(cfg, &a.tenant, &key);
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
    if !matches!(put.status().as_u16(), 200 | 201 | 204) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "A PUT got {} — cannot verify isolation without a successful write",
                put.status()
            ),
        );
    }

    // B fetches the SAME key under B's own path — must not get A's bytes.
    let url_b = url_cargo(cfg, &b.tenant, &key);
    let get = match client
        .get(&url_b)
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B GET {url_b}: {e}")),
    };
    let status = get.status().as_u16();
    if status == 200 {
        let leaked = get.bytes().map(|b| b.as_ref() == blob.as_slice()).unwrap_or(false);
        if leaked {
            return JourneyResult::fail(
                name,
                ms(start),
                "SECURITY: tenant B fetched tenant A's exact cargo bytes — cross-tenant leak"
                    .to_string(),
            );
        }
        // 200 with different bytes (B's own namespace, coincidental key) is not
        // a leak — but A's key was unique, so B should be a miss. Treat a 200
        // that is NOT A's bytes as a pass on the isolation property.
        return JourneyResult::pass(name, ms(start));
    }
    if let Err(m) = expect_denied("cargo cross-tenant", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

// ── OCI (two-leg token auth) ────────────────────────────────────────────────

/// OCI HAPPY (leg 1 + reach leg 2): `GET /token` with `Authorization: Basic
/// base64("oci:<pat>")` mints an HMAC bearer (200 + `{"token":…}`); that bearer
/// is then presented to `/v2/`. We assert the token mint and that the minted
/// bearer is ACCEPTED at `/v2/` (200/404 — i.e. authorized, not 401). The blob
/// push/pull protocol itself (chunked uploads, manifests) needs an OCI client →
/// out of black-box scope here.
fn oci_token_two_leg(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI: two-leg — Basic PAT → /token mints bearer → /v2 authorized";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    // Leg 1: Basic base64("oci:<pat>") → /token.
    let basic = oci_basic_header(token);
    let token_url = url_oci_token(cfg);
    let leg1 = match client.get(&token_url).header(AUTHORIZATION, &basic).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {token_url}: {e}")),
    };
    if leg1.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "/token with Basic PAT got {} (expected 200 minting a bearer)",
                leg1.status()
            ),
        );
    }
    let body: serde_json::Value = match leg1.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("/token body not JSON: {e}")),
    };
    let bearer_token = match body.get("token").and_then(|t| t.as_str()) {
        Some(t) if !t.is_empty() => t.to_owned(),
        _ => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("/token 200 but no `token` field in body: {body}"),
            )
        }
    };

    // Leg 2: present the minted bearer at /v2/ — must be AUTHORIZED (not 401).
    // The OCI base path /v2/ is the version check; an authorized client gets 200
    // (or 404 for a missing repo), an unauthorized one gets 401.
    let v2_url = url_oci_v2(cfg, "");
    let leg2 = match client
        .get(&v2_url)
        .header(AUTHORIZATION, bearer(&bearer_token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {v2_url}: {e}")),
    };
    let status = leg2.status().as_u16();
    if status == 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            "minted OCI bearer rejected at /v2/ (401) — token exchange not honored".to_string(),
        );
    }
    // 200 (version OK) or 404 (no such path/repo) both mean the bearer was
    // accepted by the auth layer — that is the property under test.
    if !matches!(status, 200 | 404) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("/v2/ with minted bearer got {status} (expected 200 or 404, authorized)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

/// OCI ADVERSARIAL: `/token` with NO credential (and `/v2/` with none) must be
/// denied (401) — the registry must not hand out a usable bearer to anonymous
/// callers, and `/v2/` must challenge.
fn oci_token_anonymous_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI: anonymous — no-cred /token & /v2 → 401 (challenge, no free bearer)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // /token without Basic credential. A connection error means the endpoint
    // isn't under test (e.g. no env) → gate, don't fail.
    let token_url = url_oci_token(cfg);
    let t = match client.get(&token_url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("endpoint unreachable ({}): {e}", cfg.endpoint))
        }
    };
    let tstatus = t.status().as_u16();
    // A no-cred /token must NOT mint a usable bearer. The registry may answer
    // 401 (challenge) — anything that hands back a 200 with a `token` is a fail.
    if tstatus == 200 {
        let body: serde_json::Value = t.json().unwrap_or(serde_json::Value::Null);
        if body.get("token").and_then(|x| x.as_str()).is_some() {
            return JourneyResult::fail(
                name,
                ms(start),
                "SECURITY: anonymous /token minted a bearer — free credential".to_string(),
            );
        }
    } else if let Err(m) = expect_denied("oci /token anonymous", tstatus) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // /v2/ without any Authorization must challenge with 401 (OCI spec).
    let v2_url = url_oci_v2(cfg, "");
    let v = match client.get(&v2_url).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {v2_url}: {e}")),
    };
    let vstatus = v.status().as_u16();
    if vstatus == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "SECURITY: anonymous /v2/ returned 200 — registry not challenging".to_string(),
        );
    }
    if let Err(m) = expect_denied("oci /v2 anonymous", vstatus) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// Build the OCI Basic auth header: `Basic base64("oci:<pat>")` (the `oci:`
/// username is the adapter's convention; the PAT is the password).
fn oci_basic_header(pat: &str) -> String {
    // Minimal standard base64 without pulling a new dep (harness deps are
    // frozen). `oci:<pat>` is ASCII; encode it directly.
    format!("Basic {}", base64_standard(format!("oci:{pat}").as_bytes()))
}

/// Standard-alphabet base64 (no padding tricks) — tiny inline encoder so we do
/// NOT add a dependency (the harness dep set is frozen).
fn base64_standard(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ── npm / pip / brew (registry/proxy surfaces) ──────────────────────────────

/// npm ADVERSARIAL (P12): an anonymous GET on `/npm/{tenant}/{rest}` must be
/// denied — the npm cache surface requires a PAT (the moat gates ACCESS even for
/// public packages).
fn npm_anonymous_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "npm: P12 anonymous GET → denied (cache access requires PAT)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let tenant = cfg.tenant_or_anon().to_string();
    // A common npm metadata path (`/<pkg>`); the deny is at the gate.
    let url = url_npm(cfg, &tenant, "left-pad");
    let get = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("endpoint unreachable ({}): {e}", cfg.endpoint))
        }
    };
    let status = get.status().as_u16();
    if status == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "SECURITY: anonymous npm GET returned 200 — cache not gated".to_string(),
        );
    }
    if let Err(m) = expect_denied("npm anonymous", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// npm HAPPY (GATED): the real npm cache-fill round trip (metadata doc +
/// SHA-512-integrity-checked tarball) is the npm wire protocol, which a raw HTTP
/// probe cannot synthesize against a live upstream. Gated to an npm client +
/// upstream connectivity. The "quota-without-tenant fail-closed" cell is
/// likewise GATED: that hinges on the Worker-set `x-corelink-scope` /
/// `x-corelink-tenant-id` headers, which the Worker strips from client input —
/// it is not reachable from a black-box client (no way to forge the header).
fn npm_happy_gated(cfg: &Config, _client: &Client) -> JourneyResult {
    let name = "npm: store→fetch tarball (SHA512) + quota-without-tenant fail-closed";
    let _ = cfg;
    JourneyResult::gated(
        name,
        "needs an npm client + upstream registry round trip (metadata + integrity-checked \
         tarball); and the quota-fail-closed cell hinges on the Worker-set x-corelink-scope/\
         tenant headers, which are stripped from client input — not black-box reachable",
    )
}

/// pip ADVERSARIAL (P12): an anonymous GET on `/pip/{tenant}/{rest}` must be
/// denied. The pip simple-index/wheel surface gates access on a PAT.
fn pip_anonymous_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "pip: P12 anonymous GET → denied (cache access requires PAT)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let tenant = cfg.tenant_or_anon().to_string();
    // PEP-503 simple index path for a package.
    let url = url_pip(cfg, &tenant, "simple/requests/");
    let get = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("endpoint unreachable ({}): {e}", cfg.endpoint))
        }
    };
    let status = get.status().as_u16();
    if status == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "SECURITY: anonymous pip GET returned 200 — cache not gated".to_string(),
        );
    }
    if let Err(m) = expect_denied("pip anonymous", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// brew HAPPY (GATED): the Homebrew bottle proxy happy path is a ghcr.io bottle
/// fetch driven by `brew` (OCI-ish bottle paths + upstream redirect/dedup). The
/// public bottle bytes are SHARED cross-tenant by design (the moat), so a
/// cross-tenant "leak" of public bottles is not a finding — the only isolation
/// property is on PRIVATE content, which brew (all-public) does not store.
/// Gated to a `brew` client + upstream connectivity.
fn brew_happy_gated(cfg: &Config, _client: &Client) -> JourneyResult {
    let name = "brew: bottle proxy (public dedup) round trip";
    let _ = cfg;
    JourneyResult::gated(
        name,
        "needs a `brew` client + upstream ghcr.io bottle fetch; bottle bytes are PUBLIC and \
         shared cross-tenant by design (the moat) so there is no private-isolation cell to \
         black-box here",
    )
}
