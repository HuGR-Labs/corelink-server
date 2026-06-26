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
    bearer, expect_denied, expect_gate_denied, sha256_hex, unique_blob, url_brew, url_cargo,
    url_npm, url_oci_token,
    url_oci_v2, url_pip, Config, JourneyResult, TokenKind,
};
use crate::personas::Persona;

/// Run the adapter journeys (cargo / npm / pip / brew / OCI).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        // cargo (sccache) — fully black-box exercisable.
        cargo_store_fetch_round_trip(cfg, client),
        cargo_sccache_check_probe_accepted(cfg, client),
        cargo_fresh_tenant_first_write(cfg, client),
        cargo_readonly_write_denied(cfg, client),
        cargo_anonymous_denied(cfg, client),
        cargo_cross_tenant_isolation(cfg, client),
        // OCI — two-leg auth handshake is black-box.
        oci_token_two_leg(cfg, client),
        oci_token_anonymous_denied(cfg, client),
        // npm / pip — anonymous deny + the PUBLIC moat-path regression (200 not 502).
        npm_anonymous_denied(cfg, client),
        npm_public_package_fetch(cfg, client),
        pip_anonymous_denied(cfg, client),
        pip_public_package_fetch(cfg, client),
        // brew — public-bottle proxy: the 5-cause-chain regression (200 not 502).
        brew_public_bottle_fetch(cfg, client),
    ]
}

// ── cargo (sccache) ────────────────────────────────────────────────────────

/// Build a unique sccache-style key for a hermetic run.
fn cargo_key(blob: &[u8]) -> String {
    // sccache uses a content hash as the object key; mirror that shape.
    format!("e2e/{}", sha256_hex(blob))
}

/// cargo HAPPY (REGRESSION — the MoatCache fix): a raw-HTTP `PUT
/// /cargo/{tenant}/{key}` then `GET` round-trips. This locks in
/// FINDING-sccache-adapter-gaps: the old `CargoCasBridge` passed the sccache key
/// (a hash of compile INPUTS) straight through as the CAS `digest_hex`, but the
/// CAS write VERIFIES `claimed == blake3(content)`, so EVERY sccache PUT failed
/// integrity and 502'd. The fix routes cargo through the 2-level
/// [`MoatCache`] (`routes/cargo.rs::CargoMoatStore`): `put` computes
/// `content_hash = blake3(bytes)`, stores content-addressed (verify passes), and
/// records `(tenant, key) → content_hash`; `get` resolves the map then fetches.
/// So a plain HTTP PUT of arbitrary bytes under an arbitrary single-segment key
/// MUST now succeed and round-trip the exact bytes — and MUST NOT 502.
///
/// We use a 64-hex key (an sccache object key) and assert: PUT 200/201/204,
/// GET 200 + bytes match, and (regression assert) NEITHER leg returns 502.
fn cargo_store_fetch_round_trip(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: store→fetch — raw PUT key, GET bytes match (MoatCache fix, no 502)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("corelink-e2e-cargo-rt");
    // sccache object keys are 64-hex; use that exact shape (the canonical key).
    let key = sha256_hex(&blob);
    let url = url_cargo(cfg, &p1.tenant, &key);

    // PUT — raw octet-stream body. The MoatCache fix means this is accepted.
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
    if put_status == 502 {
        return JourneyResult::fail(
            name,
            ms(start),
            "REGRESSION: cargo PUT 502 — the MoatCache content-addressing fix is gone \
             (sccache key passed through as CAS digest → integrity fail). url="
                .to_string()
                + &url,
        );
    }
    if !matches!(put_status, 200 | 201 | 204) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("cargo PUT got {put_status} (expected 200/201/204). url={url}"),
        );
    }

    // GET — must be a hit and round-trip the EXACT bytes via the url-map → blob.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let get_status = get.status().as_u16();
    if get_status == 502 {
        return JourneyResult::fail(
            name,
            ms(start),
            "REGRESSION: cargo GET 502 — MoatCache map→blob resolution broken".to_string(),
        );
    }
    if get_status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("cargo GET got {get_status} (expected 200 hit after PUT)"),
        );
    }
    match get.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("cargo GET bytes mismatch (put {} got {})", blob.len(), b.len()),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("cargo GET body: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

/// cargo REGRESSION (`.sccache_check` health probe): the real sccache client
/// PUTs/GETs the control key `.sccache_check` on startup to verify the backend.
/// The old `normalize_key` assumed EVERY key was 64-hex and rejected everything
/// else → the cargo route **400'd `.sccache_check`** → sccache deemed the
/// backend unusable and disabled the cache (real client never worked even though
/// hex keys round-tripped). The fix (`cargo/translate.rs::normalize_key`) accepts
/// any safe single-segment key verbatim. So a GET of an UNWRITTEN `.sccache_check`
/// must be a clean **404 miss** (key validated → looked up → absent), NEVER a
/// 400 (key rejected) and NEVER a 502. That 404-not-400 distinction IS the
/// regression. (We GET without first PUTting so the assert is deterministic
/// regardless of prior probes.)
fn cargo_sccache_check_probe_accepted(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: .sccache_check health probe accepted (GET → 404 miss, not 400)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    let url = url_cargo(cfg, &p1.tenant, ".sccache_check");
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let status = get.status().as_u16();
    if status == 400 {
        return JourneyResult::fail(
            name,
            ms(start),
            "REGRESSION: cargo 400'd `.sccache_check` — key-validation fix is gone; \
             sccache would deem the backend unusable and disable the cache"
                .to_string(),
        );
    }
    if status == 502 {
        return JourneyResult::fail(
            name,
            ms(start),
            "cargo `.sccache_check` GET 502 — backend error on the health-probe key".to_string(),
        );
    }
    // The key is ACCEPTED (validated, looked up). An unwritten probe key is a
    // clean miss (404); a 200 is also acceptable if a prior probe seeded it.
    if !matches!(status, 200 | 404) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("cargo `.sccache_check` GET got {status} (expected 404 miss or 200 hit)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

/// cargo REGRESSION (fresh-tenant cap-seed): a tenant that has NEVER done a
/// native CAS write has no `tenant_storage_state` row, and the byte-accounting
/// reservation FAILS CLOSED (502) when asked to seed it with an indeterminate
/// (`None`) cap — so a brand-new sccache user's FIRST `PUT /cargo/{tenant}/{key}`
/// 502'd. The fix (`routes/cargo.rs::CargoMoatStore::put` + the shared
/// `TenantCapResolver`) resolves the tenant's per-tier cap container-side so the
/// row auto-seeds on the first write.
///
/// A black-box client CANNOT provision a guaranteed-fresh (zero-prior-native-
/// write) tenant — the harness env contract has no fresh-tenant slot, and the
/// primary `CORELINK_E2E_TENANT` may already have a storage-state row from prior
/// CAS journeys (so it would NOT exercise the fresh-row seed path). We therefore
/// GATE this cell with a precise reason rather than fake a fresh tenant. The
/// regression IS unit-covered server-side (`cargo.rs::tests::
/// put_threads_resolved_cap_into_cas_write`).
fn cargo_fresh_tenant_first_write(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: fresh-tenant first write seeds cap (no 502) — cap-seed fix";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // The cap-seed path needs a GUARANTEED-FRESH tenant (zero prior native CAS
    // write ⇒ no `tenant_storage_state` row), which a black-box client cannot
    // manufacture itself — the provisioner mints a brand-new tenant + PAT each
    // run and exposes them as CORELINK_E2E_FRESH_TENANT / CORELINK_E2E_PAT_FRESH.
    // Absent that pairing we GATE with the precise reason (the fix stays
    // unit-covered server-side: cargo.rs::put_threads_resolved_cap_into_cas_write).
    let (Some(tenant), Some(token)) = (cfg.fresh_tenant.clone(), cfg.token(TokenKind::Fresh)) else {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_FRESH_TENANT / CORELINK_E2E_PAT_FRESH not set — needs a \
             guaranteed-fresh tenant (zero prior native CAS write, so no \
             tenant_storage_state row) to exercise the cap-seed path. The fix is \
             unit-covered server-side (routes/cargo.rs::put_threads_resolved_cap_into_cas_write).",
        );
    };

    // The fresh tenant's FIRST cargo PUT must auto-seed its cap container-side
    // and SERVE (200/201) — the regression was a fail-CLOSED 502 on the absent
    // tenant_storage_state row. A 502 here = the cap-seed fix has regressed.
    let blob = unique_blob("corelink-e2e-cargo-fresh-seed");
    let key = cargo_key(&blob);
    let url = url_cargo(cfg, &tenant, &key);
    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("fresh PUT {url}: {e}")),
    };
    let status = put.status().as_u16();
    match status {
        200 | 201 => JourneyResult::pass(name, ms(start)),
        502 => JourneyResult::fail(
            name,
            ms(start),
            "REGRESSION: fresh-tenant first cargo PUT 502 — the cap-seed fix is gone; \
             the absent tenant_storage_state row fails closed instead of auto-seeding"
                .to_string(),
        ),
        other => JourneyResult::fail(
            name,
            ms(start),
            format!("fresh-tenant first cargo PUT got {other} (expected a 200/201 serve, never 502)"),
        ),
    }
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
    if let Err(m) = expect_gate_denied("cargo RO write", status) {
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
    // A plausible key; the deny must happen at the gate, BEFORE any lookup.
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
    // expect_GATE_denied (401/403 only), NOT expect_denied (auditor finding): the
    // cargo key is absent and cannot be seeded black-box (raw-HTTP cargo PUT 502s),
    // so accepting a 404 here meant a fully-DISABLED auth gate (request reaches the
    // missing-key lookup → 404) scored as "secure". The auth middleware must reject
    // an anonymous request with 401/403 BEFORE the lookup; a 404 now FAILS — it
    // proves auth did not gate before the lookup (or the route is absent).
    if let Err(m) = expect_gate_denied("cargo anonymous", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// cargo ADVERSARIAL (P10) — cross-tenant isolation (GATED): the isolation cell
/// hinges on tenant A first STORING an object, but a raw-HTTP cargo PUT cannot
/// drive the sccache/WebDAV write contract (live-probed: 502 at the edge — see
/// [`cargo_store_fetch_round_trip`]). Without a successful seed write there is
/// nothing to cross-read, so this can only be exercised with a real sccache
/// client. GATED with that reason, mirroring the store→fetch gate. (Cross-tenant
/// isolation IS positively exercised black-box on the native CAS and Turbo
/// surfaces, which DO accept raw-HTTP writes.)
fn cargo_cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "cargo: P10 cross-tenant — A stores; B fetches same key → no leak";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // A = primary tenant (P1 RW), B = a DIFFERENT tenant's PAT (P6). Both are
    // raw-HTTP cargo-writable (the old "/cargo rejects raw PUT → 502" premise was
    // stale — the cap-seed fix made the first cargo PUT serve 200/201).
    let a = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_a = a.token.expect("P1 has a token");
    let token_b = b.token.expect("P6 has a token");

    // A seeds a private cargo artifact under its own tenant namespace.
    let secret = unique_blob("corelink-e2e-cargo-private-A");
    let key = cargo_key(&secret);
    let a_url = url_cargo(cfg, &a.tenant, &key);
    let put = match client
        .put(&a_url)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(secret.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("A seed PUT {a_url}: {e}")),
    };
    match put.status().as_u16() {
        200 | 201 => {}
        // A at its cap can't seed — that's account state, not a contract break.
        402 | 429 => {
            return JourneyResult::gated(
                name,
                "A's tenant is at its cap — the isolation seed write needs an under-cap tenant",
            )
        }
        502 => {
            return JourneyResult::fail(
                name,
                ms(start),
                "REGRESSION: cargo seed PUT 502 — the MoatCache cap-seed fix is gone".to_string(),
            )
        }
        other => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("A cargo seed PUT got {other} (expected 200/201)"),
            )
        }
    }

    // B (a DIFFERENT tenant) attempts to read A's exact key under A's namespace.
    // The cargo route binds the path tenant to the PAT's tenant, so this MUST be
    // denied (403 tenant-mismatch / 401 / 404) — and must NEVER return A's bytes.
    let resp = match client
        .get(&a_url)
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B cross GET {a_url}: {e}")),
    };
    let status = resp.status().as_u16();
    if matches!(status, 200 | 206) {
        // A successful read is only a LEAK if it returns A's private bytes.
        let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
        if body == secret {
            return JourneyResult::fail(
                name,
                ms(start),
                "CROSS-TENANT LEAK: B read A's private cargo artifact bytes under A's tenant"
                    .to_string(),
            );
        }
        return JourneyResult::fail(
            name,
            ms(start),
            format!("B cross-tenant cargo GET got {status} (expected a 401/403/404 deny)"),
        );
    }
    match expect_denied("cross-tenant cargo read", status) {
        Ok(()) => JourneyResult::pass(name, ms(start)),
        Err(_) => JourneyResult::fail(
            name,
            ms(start),
            format!("B cross-tenant cargo GET got {status} (expected a 401/403/404 deny)"),
        ),
    }
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

    // Leg 1: Basic base64("oci:<pat>") → /token. The Docker/OCI token-auth
    // spec REQUIRES a `scope` query param (`repository:<name>:<actions>`); the
    // CoreLink `/token` endpoint mints a bearer scoped to exactly that
    // repository and 401s without it. We request a pull scope on a probe repo.
    let basic = oci_basic_header(token);
    let token_url = format!("{}?scope=repository:e2e:pull", url_oci_token(cfg));
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
    } else if let Err(m) = expect_gate_denied("oci /token anonymous", tstatus) {
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
    if let Err(m) = expect_gate_denied("oci /v2 anonymous", vstatus) {
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
    if let Err(m) = expect_gate_denied("npm anonymous", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// npm REGRESSION (public moat path): an AUTHENTICATED GET of a PUBLIC package's
/// metadata doc (`/npm/{tenant}/{pkg}`) must return **200**, NOT **502**. The
/// read-through adapter fetches `registry.npmjs.org/{pkg}` on a miss and serves
/// it (public metadata lands in the shared `_public` namespace). This is the
/// shared `_public` moat-path regression: the brew/npm/pip public-dedup path
/// failed closed (502) when the `_public` `tenant_storage_state` row + sentinel
/// R2 prefix were absent (the 5-cause chain). We assert the surface returns a
/// 200 (cache hit OR cache-fill from upstream) and that it does NOT 502 for a
/// well-known public package.
///
/// GATED if the PAT is absent. A 502 is a hard FAIL (the moat-path regression).
/// A transient upstream failure surfaces as 502/503/504 from the adapter; we
/// treat **502 specifically** as the regression (the `_public`-row fail-closed
/// signature) and a 503/504 as a gate (upstream/transient), never a false fail.
fn npm_public_package_fetch(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "npm: public package metadata GET → 200 (shared _public moat path, no 502)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    // `is-odd` is a tiny, stable, dependency-free public package — a cheap
    // metadata fetch that exercises the public read-through path.
    let url = url_npm(cfg, &p1.tenant, "is-odd");
    public_fetch_assert(name, start, ms, client, &url, token, "npm")
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
    if let Err(m) = expect_gate_denied("pip anonymous", status) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// pip REGRESSION (public moat path): an AUTHENTICATED GET of a PUBLIC package's
/// PEP-503 simple index (`/pip/{tenant}/simple/{pkg}/`) must return **200**, NOT
/// **502** — the same shared `_public` moat-path regression as npm/brew. The
/// read-through adapter fetches PyPI's simple index on a miss and serves it.
fn pip_public_package_fetch(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "pip: public simple-index GET → 200 (shared _public moat path, no 502)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    // `six` is a tiny, stable, ubiquitous public package — its simple index is a
    // cheap fetch that exercises the public read-through path.
    let url = url_pip(cfg, &p1.tenant, "simple/six/");
    public_fetch_assert(name, start, ms, client, &url, token, "pip")
}

/// brew REGRESSION (the 5-cause chain): an AUTHENTICATED GET of a PUBLIC bottle
/// manifest (`/brew/{tenant}/v2/.../manifests/...`) must return **200**, NOT
/// **502**. This locks in the documented `_public` chain fix (image d1cdccb2):
/// brew (+ npm/pip) PUBLIC-dedup writes to the `_public` namespace need BOTH a
/// `tenant_storage_state` row (migration 0073) AND a sentinel-UUID R2 prefix,
/// else the write fails CLOSED → the proxy surfaces **502**. The other causes in
/// the chain (ghcr token, `Accept` header, path-strip) all manifest as the same
/// 502 if regressed. We send the ghcr-manifest `Accept` header and assert the
/// surface returns 200 (hit or upstream cache-fill), never 502.
///
/// The bottle bytes are PUBLIC + shared cross-tenant by design (the moat), so
/// there is no private-isolation cell here — this is purely the availability
/// regression (502 → 200).
fn brew_public_bottle_fetch(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "brew: public bottle manifest GET → 200 (5-cause _public chain, no 502)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token = p1.token.expect("P1 always has a token");

    // A Homebrew bottle manifest path on ghcr.io: `hello` is a tiny, stable
    // formula. The adapter joins the (tenant-stripped) path onto ghcr.io.
    // Homebrew bottles on ghcr are tagged by VERSION, not `latest` (there is no
    // `latest` tag → ghcr 404 → adapter 502). Pin a stable, long-published version
    // of the tiny `hello` formula (verified 200 live). Bump if ghcr ever drops it.
    let url = url_brew(cfg, &p1.tenant, "v2/homebrew/core/hello/manifests/2.12.1");
    // The bottle/manifest fetch needs the OCI/ghcr manifest Accept header — one
    // of the 5 causes was a missing Accept header → 502. brew is UPSTREAM-dependent
    // (ghcr) and a cold path triggers a cache-fill, so a SINGLE 5xx is often a
    // transient upstream hiccup, NOT the regression — retry up to 3× and only
    // treat a PERSISTENT 502 as the _public-chain regression (no false-RED on a
    // one-off ghcr blip).
    let accept = "application/vnd.oci.image.index.v1+json,\
                  application/vnd.docker.distribution.manifest.v2+json,\
                  application/vnd.oci.image.manifest.v1+json";
    let mut status = 0u16;
    let mut last_err = String::new();
    let mut body = String::new();
    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        match client
            .get(&url)
            .header(AUTHORIZATION, bearer(token))
            .header(reqwest::header::ACCEPT, accept)
            .send()
        {
            Ok(r) => {
                status = r.status().as_u16();
                if status < 500 {
                    // Capture the body on a definitive response so the 200 path
                    // can assert it is a real manifest, not a hollow 200.
                    body = r.text().unwrap_or_default();
                    break; // 200/3xx/4xx is definitive — stop retrying
                }
            }
            Err(e) => {
                last_err = e.to_string();
                status = 0;
            }
        }
    }
    if status == 0 {
        return JourneyResult::fail(name, ms(start), format!("GET {url}: {last_err}"));
    }
    // PERSISTENT 502 across retries ⇒ the 5-cause _public chain regressed
    // (PUBLIC-dedup write fails closed). 503/504 ⇒ upstream/edge transient (gate).
    if status == 502 {
        return JourneyResult::fail(
            name,
            ms(start),
            "REGRESSION: brew public bottle GET 502 across 3 retries — the 5-cause \
             _public chain (ghcr token / Accept / path-strip / _public storage-row / \
             _public R2-prefix) regressed; PUBLIC-dedup write fails closed. url="
                .to_string()
                + &url,
        );
    }
    if matches!(status, 503 | 504) {
        return JourneyResult::gated(
            name,
            format!("brew upstream/edge transient ({status}) after 3 retries — not the _public regression"),
        );
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("brew public bottle GET got {status} (expected 200, hit or cache-fill)"),
        );
    }
    // A 200 must carry a real OCI manifest, not a hollow body (auditor: status-only
    // check passed on an empty 200). The bottle manifest is JSON; assert non-empty
    // + a manifest marker ("schemaVersion"/"mediaType" appear in every OCI index).
    if body.is_empty() || !(body.contains("schemaVersion") || body.contains("mediaType")) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "brew public bottle GET 200 but body is not a real OCI manifest (len={}) — \
                 a hollow 200 is not a real cache hit/fill",
                body.len()
            ),
        );
    }
    JourneyResult::pass(name, ms(start))
}

/// Shared assert for the npm/pip PUBLIC read-through moat-path regression: an
/// authenticated GET of a well-known public package must be **200** (cache hit
/// OR upstream cache-fill) and MUST NOT **502** (the `_public` fail-closed
/// signature). A 503/504 is treated as a transient upstream/edge GATE, never a
/// false FAIL; a deny (401/403) is a real FAIL (P1 is rw + the tenant is set).
fn public_fetch_assert(
    name: &'static str,
    start: Instant,
    ms: impl Fn(Instant) -> u64,
    client: &Client,
    url: &str,
    token: &str,
    surface: &str,
) -> JourneyResult {
    let get = match client.get(url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let status = get.status().as_u16();
    if status == 502 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "REGRESSION: {surface} public GET 502 — the shared _public moat path \
                 (storage-row + sentinel R2-prefix) has failed closed. url={url}"
            ),
        );
    }
    if matches!(status, 503 | 504) {
        return JourneyResult::gated(
            name,
            format!("{surface} upstream/edge transient ({status}) — not the _public regression; retry"),
        );
    }
    if status != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("{surface} public GET got {status} (expected 200, hit or cache-fill)"),
        );
    }
    // 200: assert the body is the REAL public package index, not a hollow/empty
    // 200 (auditor: the status-only check passed on an empty/garbage body). Markers
    // verified live: npm metadata carries "dist-tags"; pip PEP-691 simple index
    // carries "files".
    let body = match get.text() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("{surface} 200 but body read failed: {e}"))
        }
    };
    let marker = match surface {
        "npm" => "dist-tags",
        "pip" => "files",
        _ => "",
    };
    if body.is_empty() || (!marker.is_empty() && !body.contains(marker)) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "{surface} public GET 200 but body lacks the expected '{marker}' marker \
                 (len={}) — a hollow 200 is not a real cache hit/fill",
                body.len()
            ),
        );
    }
    JourneyResult::pass(name, ms(start))
}
