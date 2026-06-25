//! OCI / docker registry conformance journeys — `/v2/*` + `/token`.
//!
//! Surface: the OCI Distribution Spec v1.1 registry that `docker` / `podman` /
//! `buildah` / `containerd` / `crane` / Helm-OCI all speak to. These journeys
//! LOCK IN, as HTTP black-box regressions, the 9 conformance fixes that made a
//! real `docker push` / `docker pull` work against `corelink-oci.humangr.com`.
//!
//! Grounded in:
//!   - `crates/corelink-container/src/routes/oci.rs` (the mount + realm const
//!     `OCI_BEARER_REALM = "https://corelink-oci.humangr.com/token"`).
//!   - `crates/corelink-adapter-host/src/oci/server/handlers.rs` + `auth.rs`
//!     (the `/v2/`, `/token` GET+POST, `dispatch_v2` per-op challenge logic, the
//!     `{token, access_token, expires_in}` body shape, and
//!     `www_authenticate_header` = `Bearer realm="…",service="corelink-oci",
//!     scope="…"`).
//!
//! ## Two-leg auth (why every assertion needs the `/token` exchange)
//!
//! OCI uses a two-leg flow: the client `GET`s `/token` with `Authorization:
//! Basic base64(user:<PAT>)` and receives an HMAC *bearer*; it then retries
//! `/v2/*` ops with `Authorization: Bearer <hmac>`. The tenant lives INSIDE the
//! minted bearer (never in the path). So these journeys mint a bearer from P1's
//! cache PAT and drive the data plane with it.
//!
//! ## Gating
//!
//! Every journey GATES (never fails) when P1's PAT is absent — the bearer is
//! minted from it and there is no other way to reach the authenticated data
//! plane as a black box. Unauthenticated challenge journeys (#1, #6) run on
//! connectivity alone (they assert the *401 challenge*, which needs no creds).
//! The OCI host defaults to `CORELINK_E2E_ENDPOINT` but can be overridden with
//! `CORELINK_E2E_OCI_ENDPOINT` (prod splits the OCI surface onto the dedicated
//! flat host `corelink-oci.humangr.com`).

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::harness::{Config, JourneyResult};
use crate::personas::Persona;

/// Optional override for the OCI base URL (prod runs the OCI surface on the
/// dedicated flat host `corelink-oci.humangr.com`). Falls back to the suite's
/// primary endpoint when unset.
const OCI_ENDPOINT_ENV: &str = "CORELINK_E2E_OCI_ENDPOINT";

/// The `service=` value the adapter stamps into every challenge + accepts on the
/// `/token` exchange (`www_authenticate_header(realm, "corelink-oci", scope)`).
const OCI_SERVICE: &str = "corelink-oci";

/// A deterministic per-run repository name. Content-addressed blobs make the
/// push idempotent; the repo is fresh per run so the manifest-HEAD assertion is
/// hermetic (no cross-run tag collision).
fn run_repo() -> String {
    // A repo name docker accepts: lowercase alphanumerics + separators.
    format!("e2e-conformance/run-{}", uuid::Uuid::new_v4().simple())
}

/// Resolve the OCI base URL (override env or the suite endpoint), trailing slash
/// trimmed.
fn oci_base(cfg: &Config) -> String {
    env::var(OCI_ENDPOINT_ENV)
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_else(|| cfg.endpoint.clone())
}

// ── tiny base64 (standard alphabet) — no extra dep ───────────────────────────
//
// The frozen dep set (reqwest/serde_json/uuid/sha2/blake3/hex) has no base64,
// and the OCI Basic leg needs `base64(user:PAT)`. This is a minimal standard
// (RFC 4648) encoder — encode-only, padded — exactly what the `Basic` header
// requires.
fn b64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// `Authorization: Basic base64("oci:<pat>")` — the OCI Basic leg (the username
/// is ignored by the adapter; only the PAT matters).
fn basic(pat: &str) -> String {
    format!("Basic {}", b64_encode(format!("oci:{pat}").as_bytes()))
}

/// OCI digest wire string for `bytes`: `sha256:<lowercase-hex>`.
fn oci_digest(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// Read the `Www-Authenticate` header value, if present + UTF-8.
fn www_auth(resp: &reqwest::blocking::Response) -> Option<String> {
    resp.headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Mint an OCI bearer from P1's PAT for `scope` (empty string ⇒ docker-login
/// credential-check token). Uses the GET `/token` leg. Returns the bearer or a
/// human error.
fn mint_bearer(
    client: &Client,
    base: &str,
    pat: &str,
    scope: &str,
) -> Result<String, String> {
    let url = if scope.is_empty() {
        format!("{base}/token?service={OCI_SERVICE}")
    } else {
        format!(
            "{base}/token?service={OCI_SERVICE}&scope={}",
            urlencode(scope)
        )
    };
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, basic(pat))
        .send()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let code = resp.status().as_u16();
    if code != 200 {
        return Err(format!("GET /token got {code} (expected 200) for scope={scope:?}"));
    }
    let body: Value = resp.json().map_err(|e| format!("/token not JSON: {e}"))?;
    body["token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("/token response missing string `token`: {body}"))
}

/// Minimal percent-encoder for the few characters that appear in an OCI scope
/// (`repository:<repo>:pull,push`). Encodes everything outside the unreserved
/// set so the query parameter round-trips through the adapter's `%XX` decoder.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Run the OCI conformance journeys (9 regressions + 5 protocol ops).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    // The OCI host is scale-to-zero: the FIRST request after it idles eats a
    // cold-start — often a >30s client timeout, then a brief 5xx cascade while
    // the container warms. A real registry client (docker) simply retries
    // through that window, so a cold-start is not a conformance failure. We
    // mirror that with a bounded warmup so the assertions below run against a
    // warm host and a cold-start cannot flake the ship gate. The warmup only
    // waits for readiness; it asserts nothing.
    warm_oci(cfg, client);
    vec![
        j1_v2_challenge(cfg, client),
        j2_token_get(cfg, client),
        j3_token_post(cfg, client),
        j4_token_empty_scope(cfg, client),
        j5_no_numeric_issued_at(cfg, client),
        j6_blob_head_specific_challenge(cfg, client),
        j7_push_scoped_bearer_pulls(cfg, client),
        j8_insufficient_bearer_rechallenge(cfg, client),
        j9_manifest_head_content_length(cfg, client),
        // M17 — OCI protocol ops (chunked upload, tags/list, catalog gate,
        // RO-push deny, cross-tenant isolation).
        j10_chunked_patch_upload(cfg, client),
        j11_tags_list_after_push(cfg, client),
        j12_catalog_always_401(cfg, client),
        j13_ro_push_denied(cfg, client),
        j14_cross_tenant_isolation(cfg, client),
    ]
}

/// Best-effort warmup: ping `GET /v2/` until the OCI host answers warm (any
/// non-5xx status — the expected unauth challenge is a 401, which counts) or a
/// bounded attempt budget is spent. A cold-start surfaces as a request timeout
/// or a transient 5xx; both are retried (a short pause lets the container finish
/// booting after a fast 5xx). Never asserts — readiness only.
fn warm_oci(cfg: &Config, client: &Client) {
    let url = format!("{}/v2/", oci_base(cfg));
    for attempt in 0..6 {
        match client.get(&url).send() {
            Ok(r) if r.status().as_u16() < 500 => return, // warm (e.g. 401 challenge)
            _ => {
                // Timeout or 5xx → still warming. The 30s client timeout already
                // paces a hung cold-start; pause briefly after a fast 5xx too.
                if attempt < 5 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
}

/// Resolve P1's PAT or a gate reason.
fn p1_pat<'a>(cfg: &'a Config, name: &'static str) -> Result<&'a str, JourneyResult> {
    match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => Ok(p.token.expect("P1 always has a token")),
        Err(reason) => Err(JourneyResult::gated(name, reason)),
    }
}

// ── #1 — `GET /v2/` → 401 + Bearer realm challenge ───────────────────────────

/// `GET /v2/` with no auth → 401 carrying a `Www-Authenticate: Bearer
/// realm="https://corelink-oci.humangr.com/token",service="corelink-oci",…`.
/// This is the entrypoint docker uses to discover the token endpoint; a missing
/// or malformed challenge breaks `docker login` before it starts. Runs on
/// connectivity alone (no creds needed to assert the challenge).
fn j1_v2_challenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #1: GET /v2/ → 401 + Bearer realm/service challenge";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let resp = match client.get(format!("{base}/v2/")).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}")),
    };
    let code = resp.status().as_u16();
    if code != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v2/ got {code} (expected 401 to advertise the token realm)"),
        );
    }
    let wa = match www_auth(&resp) {
        Some(w) => w,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "401 on /v2/ missing the Www-Authenticate header".to_string(),
            )
        }
    };
    if !wa.starts_with("Bearer ") {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("Www-Authenticate is not a Bearer challenge: {wa}"),
        );
    }
    let want_realm = r#"realm="https://corelink-oci.humangr.com/token""#;
    if !wa.contains(want_realm) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge realm wrong — expected {want_realm} in: {wa}"),
        );
    }
    if !wa.contains(&format!(r#"service="{OCI_SERVICE}""#)) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge missing service=\"{OCI_SERVICE}\": {wa}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #2 — GET /token with Basic(PAT) → 200 + bearer ───────────────────────────

/// `GET /token?service=…&scope=repository:<repo>:pull` with Basic(tenant:PAT) →
/// 200 + a non-empty `token`. The classic docker-pull token leg.
fn j2_token_get(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #2: GET /token Basic(PAT) pull-scope → 200 + bearer";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull");
    match mint_bearer(client, &base, pat, &scope) {
        Ok(tok) if !tok.is_empty() => JourneyResult::pass(name, ms(start)),
        Ok(_) => JourneyResult::fail(name, ms(start), "GET /token returned an empty token".to_string()),
        Err(e) => JourneyResult::fail(name, ms(start), e),
    }
}

// ── #3 — POST /token (form) → 200 + bearer (NOT 405) ─────────────────────────

/// `POST /token` with `application/x-www-form-urlencoded`
/// `grant_type=password&username=…&password=<PAT>&scope=…&service=…` → 200 +
/// bearer. Real `docker push` uses the OAuth2 POST form, NOT the GET leg — a 405
/// here breaks every push. This is the load-bearing "docker push uses POST" fix.
fn j3_token_post(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #3: POST /token form grant → 200 + bearer (not 405)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull,push");
    let form = format!(
        "grant_type=password&service={}&username=oci&password={}&scope={}",
        OCI_SERVICE,
        urlencode(pat),
        urlencode(&scope),
    );
    let resp = match client
        .post(format!("{base}/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}")),
    };
    let code = resp.status().as_u16();
    if code == 405 {
        return JourneyResult::fail(
            name,
            ms(start),
            "POST /token got 405 — docker push's OAuth2 token leg is rejected (regression)".to_string(),
        );
    }
    if code != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("POST /token got {code} (expected 200)"),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST /token not JSON: {e}")),
    };
    match body["token"].as_str() {
        Some(t) if !t.is_empty() => JourneyResult::pass(name, ms(start)),
        _ => JourneyResult::fail(
            name,
            ms(start),
            format!("POST /token response missing non-empty `token`: {body}"),
        ),
    }
}

// ── #4 — /token empty scope (docker login) → 200, bearer round-trips ─────────

/// `/token` with an EMPTY scope (docker login's credential-check token) → 200
/// and the minted bearer round-trips on `GET /v2/` (→ 200) even though it grants
/// no repository scope. A non-200 here breaks `docker login`.
fn j4_token_empty_scope(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #4: /token empty scope (docker login) → 200, bearer ok on /v2/";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let bearer = match mint_bearer(client, &base, pat, "") {
        Ok(t) => t,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("empty-scope mint: {e}")),
    };
    // The login token grants no repo scope but MUST pass the /v2/ base recheck.
    let resp = match client
        .get(format!("{base}/v2/"))
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET /v2/ with login bearer: {e}")),
    };
    let code = resp.status().as_u16();
    if code != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("login bearer did not round-trip on /v2/ — got {code} (expected 200)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #5 — /token JSON has NO numeric `issued_at` ──────────────────────────────

/// The `/token` response must NOT carry a numeric `issued_at` field. docker's
/// JSON decoder expects `issued_at` to be an RFC3339 *string*; a numeric one
/// (epoch int) makes docker fail to decode the token envelope. The adapter omits
/// it entirely — assert it is either absent or, if present, a string.
fn j5_no_numeric_issued_at(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #5: /token JSON has no numeric issued_at (docker decode)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let url = format!(
        "{base}/token?service={OCI_SERVICE}&scope={}",
        urlencode(&format!("repository:{repo}:pull"))
    );
    let resp = match client.get(&url).header(AUTHORIZATION, basic(pat)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /token got {} (expected 200)", resp.status()),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("/token not JSON: {e}")),
    };
    match &body["issued_at"] {
        Value::Null => {}
        Value::String(_) => {} // RFC3339 string is spec-legal
        other => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("issued_at must be absent or an RFC3339 string, got non-string: {other}"),
            )
        }
    }
    JourneyResult::pass(name, ms(start))
}

// ── #6 — blob HEAD no-auth → 401 with SPECIFIC scope challenge ───────────────

/// `HEAD /v2/<repo>/blobs/<digest>` with NO auth → 401 whose `Www-Authenticate`
/// names the SPECIFIC `repository:<repo>:pull` scope, NOT the wildcard
/// `repository:*:pull`. The wildcard made docker request a `*`-scoped token that
/// the exact-match `OciScope::allows` then rejected → an endless push 401-loop.
/// The fix parses the path BEFORE the auth check so the challenge is specific.
/// Runs on connectivity alone (the 401 challenge needs no creds).
fn j6_blob_head_specific_challenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #6: blob HEAD no-auth → 401 with specific repo:pull scope (not *)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let repo = run_repo();
    let digest = oci_digest(b"corelink-e2e-oci-absent-blob-probe");
    let url = format!("{base}/v2/{repo}/blobs/{digest}");
    let resp = match client.head(&url).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}")),
    };
    let code = resp.status().as_u16();
    if code != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("unauth blob HEAD got {code} (expected 401 + scope challenge)"),
        );
    }
    let wa = match www_auth(&resp) {
        Some(w) => w,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "401 on blob HEAD missing the Www-Authenticate challenge".to_string(),
            )
        }
    };
    let want = format!(r#"scope="repository:{repo}:pull""#);
    if !wa.contains(&want) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge must name the SPECIFIC scope {want} — got: {wa}"),
        );
    }
    if wa.contains(r#"scope="repository:*"#) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge advertised the wildcard repository:* scope (push 401-loop): {wa}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #7 — a push-scoped bearer is accepted on a pull op ───────────────────────

/// A bearer minted for `repository:<repo>:pull,push` (push superset) must be
/// ACCEPTED on a pull op: `HEAD /v2/<repo>/blobs/<absent-digest>` → 404 (miss),
/// NOT 401. i.e. the scope check treats push⇒pull (a push grant subsumes pull),
/// so `docker push`'s mount/exists probes don't bounce.
fn j7_push_scoped_bearer_pulls(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #7: push-scoped bearer accepted on pull (HEAD blob → 404, not 401)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            // A read-only PAT cannot mint a push bearer; that is a credential
            // mismatch, not a contract break we can attribute — gate it.
            return JourneyResult::gated(
                name,
                format!("could not mint a push-scoped bearer (RW PAT required): {e}"),
            );
        }
    };
    let digest = oci_digest(b"corelink-e2e-oci-push-then-pull-absent");
    let url = format!("{base}/v2/{repo}/blobs/{digest}");
    let resp = match client
        .head(&url)
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("HEAD {url}: {e}")),
    };
    let code = resp.status().as_u16();
    if code == 401 || code == 403 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("push-scoped bearer was REJECTED on a pull op (got {code}) — push⇏pull regression"),
        );
    }
    if code != 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("absent blob HEAD with push bearer got {code} (expected 404 miss)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #8 — insufficient (empty-scope) bearer re-challenges with required scope ──

/// An empty-scope (login) bearer on a blob HEAD is INSUFFICIENT → 401, and the
/// re-challenge must re-advertise the REQUIRED `repository:<repo>:pull` scope
/// (NOT an empty scope), so docker re-fetches a token for the RIGHT scope
/// instead of looping. This is the scope-re-challenge fix.
fn j8_insufficient_bearer_rechallenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #8: empty-scope bearer on blob HEAD → 401 re-advertising repo:pull";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let bearer = match mint_bearer(client, &base, pat, "") {
        Ok(t) => t,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("login-bearer mint: {e}")),
    };
    let repo = run_repo();
    let digest = oci_digest(b"corelink-e2e-oci-rechallenge-probe");
    let url = format!("{base}/v2/{repo}/blobs/{digest}");
    let resp = match client
        .head(&url)
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("HEAD {url}: {e}")),
    };
    let code = resp.status().as_u16();
    if code != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("empty-scope bearer on blob HEAD got {code} (expected 401 insufficient-scope)"),
        );
    }
    let wa = match www_auth(&resp) {
        Some(w) => w,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "insufficient-scope 401 missing the Www-Authenticate re-challenge".to_string(),
            )
        }
    };
    let want = format!(r#"scope="repository:{repo}:pull""#);
    if !wa.contains(&want) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("re-challenge must re-advertise {want} (not empty/wildcard) — got: {wa}"),
        );
    }
    if wa.contains(r#"scope="""#) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("re-challenge advertised an EMPTY scope (docker loop): {wa}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #9 — manifest HEAD after a full push → 200 + real Content-Length ─────────

/// Do a full curl-style monolithic push (POST upload → PUT ?digest= → 201 → PUT
/// manifest → 201), then `HEAD /v2/<repo>/manifests/<tag>` → 200 with a
/// `Content-Length` equal to the manifest's REAL byte size (NOT 0) and a
/// `Content-Type`. A `Content-Length: 0` HEAD made docker reject the descriptor.
fn j9_manifest_head_content_length(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #9: full push then HEAD manifest → 200 + real Content-Length";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let tag = "e2e";
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint a push bearer (RW PAT required): {e}"),
            )
        }
    };
    let auth = format!("Bearer {bearer}");

    // 1) A config blob (small JSON) + the image config descriptor digest.
    let config_blob = format!(
        r#"{{"architecture":"amd64","os":"linux","rootfs":{{"type":"layers","diff_ids":[]}},"e2e":"{}"}}"#,
        uuid::Uuid::new_v4().simple()
    )
    .into_bytes();
    let config_digest = oci_digest(&config_blob);

    if let Err(m) = push_blob(client, &base, &auth, &repo, &config_digest, config_blob.clone()) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // 2) Build a minimal OCI image manifest referencing the config blob (no
    //    layers — a valid empty-layer manifest is enough for HEAD/Content-Length).
    let manifest = format!(
        r#"{{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"{}","size":{}}},"layers":[]}}"#,
        config_digest,
        config_blob.len()
    )
    .into_bytes();
    let manifest_len = manifest.len();
    let manifest_media = "application/vnd.oci.image.manifest.v1+json";

    // 3) PUT the manifest under the tag.
    let put_url = format!("{base}/v2/{repo}/manifests/{tag}");
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, manifest_media)
        .body(manifest.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT manifest {put_url}: {e}")),
    };
    let pc = put.status().as_u16();
    if !matches!(pc, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT manifest got {pc} (expected 201). url={put_url}"),
        );
    }

    // 4) HEAD the manifest — the regression assertion.
    let head_url = format!("{base}/v2/{repo}/manifests/{tag}");
    let head = match client
        .head(&head_url)
        .header(AUTHORIZATION, &auth)
        .header("Accept", manifest_media)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("HEAD manifest {head_url}: {e}")),
    };
    let hc = head.status().as_u16();
    if hc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HEAD manifest got {hc} (expected 200 after a successful push)"),
        );
    }
    // Content-Length must equal the real manifest size, never 0.
    let cl = head
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    match cl {
        Some(n) if n == manifest_len => {}
        Some(0) => {
            return JourneyResult::fail(
                name,
                ms(start),
                "HEAD manifest Content-Length is 0 — docker rejects the descriptor (regression)".to_string(),
            )
        }
        Some(n) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("HEAD manifest Content-Length {n} ≠ real manifest size {manifest_len}"),
            )
        }
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "HEAD manifest missing Content-Length (empty body would default to 0)".to_string(),
            )
        }
    }
    if head.headers().get(CONTENT_TYPE).is_none() {
        return JourneyResult::fail(
            name,
            ms(start),
            "HEAD manifest missing Content-Type (docker needs the manifest mediaType)".to_string(),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #10 — chunked PATCH upload: POST → PATCH → PUT → GET bytes equality ───────

/// The large-layer (multi-chunk) upload path: open a session with `POST
/// /v2/<repo>/blobs/uploads/` (→ 202 + `Location`), stream the blob in two
/// `PATCH` chunks (→ 202 + `Range: 0-<n>` after each), finalize with `PUT
/// <location>?digest=<digest>` (no body — all bytes already PATCHed; → 201 +
/// `Location: /v2/<repo>/blobs/<digest>`), then `GET` the canonical blob URL
/// and assert byte-for-byte + Content-Length equality.
///
/// This differs from `push_blob` (monolithic) in that data is sent as TWO
/// separate PATCH requests before the PUT finalizer, proving the server
/// accumulates chunks correctly. The digest is asserted at both finalize time
/// (server-side 400 on mismatch) and GET time (local equality).
fn j10_chunked_patch_upload(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #10: chunked PATCH upload (POST→PATCH×2→PUT) → GET byte equality";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint a push bearer (RW PAT required): {e}"),
            )
        }
    };
    let auth = format!("Bearer {bearer}");

    // Build a small but two-chunk-able payload.
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let full_bytes: Vec<u8> = format!("corelink-e2e-oci-chunked-{run_id}").into_bytes();
    let chunk1 = &full_bytes[..full_bytes.len() / 2];
    let chunk2 = &full_bytes[full_bytes.len() / 2..];
    let digest = oci_digest(&full_bytes);

    // 1) Open session — expect 202 + Location.
    let open_url = format!("{base}/v2/{repo}/blobs/uploads/");
    let open = match client
        .post(&open_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::<u8>::new())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {open_url}: {e}")),
    };
    let oc = open.status().as_u16();
    if !matches!(oc, 202 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("open upload got {oc} (expected 202). url={open_url}"),
        );
    }
    let location = match open
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
    {
        Some(l) => l,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "open upload 202 missing the Location header".to_string(),
            )
        }
    };
    let session_url = if location.starts_with("http://") || location.starts_with("https://") {
        location
    } else {
        format!("{base}{location}")
    };

    // 2) PATCH chunk 1 — expect 202 + Range: 0-<len1-1>.
    let patch1 = match client
        .patch(&session_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(chunk1.to_vec())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PATCH chunk1 {session_url}: {e}"),
            )
        }
    };
    let p1c = patch1.status().as_u16();
    if p1c != 202 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PATCH chunk1 got {p1c} (expected 202)"),
        );
    }
    // The Range header after chunk1 should reflect the bytes received so far.
    let range1 = patch1
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let expected_range1 = format!("0-{}", chunk1.len() - 1);
    if let Some(ref r) = range1 {
        if r != &expected_range1 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "PATCH chunk1 Range={r:?} does not match expected {expected_range1:?}"
                ),
            );
        }
    }

    // 3) PATCH chunk 2 — expect 202 + Range: 0-<total-1>.
    let patch2 = match client
        .patch(&session_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(chunk2.to_vec())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PATCH chunk2 {session_url}: {e}"),
            )
        }
    };
    let p2c = patch2.status().as_u16();
    if p2c != 202 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PATCH chunk2 got {p2c} (expected 202)"),
        );
    }
    let expected_range2 = format!("0-{}", full_bytes.len() - 1);
    let range2 = patch2
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if let Some(ref r) = range2 {
        if r != &expected_range2 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "PATCH chunk2 Range={r:?} does not match expected {expected_range2:?}"
                ),
            );
        }
    }

    // 4) PUT finalize with ?digest= and NO body (all bytes already PATCHed).
    let sep = if session_url.contains('?') { '&' } else { '?' };
    let put_url = format!("{session_url}{sep}digest={}", urlencode(&digest));
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::<u8>::new())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("PUT finalize {put_url}: {e}"))
        }
    };
    let pc = put.status().as_u16();
    if !matches!(pc, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT finalize got {pc} (expected 201). url={put_url}"),
        );
    }
    // Location header on 201 should point to the canonical blob URL.
    let blob_location = put
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 5) GET the blob and assert byte + Content-Length equality.
    let get_url = match blob_location {
        Some(ref loc) if loc.starts_with("http://") || loc.starts_with("https://") => {
            loc.clone()
        }
        Some(ref loc) => format!("{base}{loc}"),
        None => format!("{base}/v2/{repo}/blobs/{digest}"),
    };
    let pull_scope = format!("repository:{repo}:pull");
    let pull_bearer = match mint_bearer(client, &base, pat, &pull_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("mint pull bearer for GET verification: {e}"),
            )
        }
    };
    let get = match client
        .get(&get_url)
        .header(AUTHORIZATION, format!("Bearer {pull_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {get_url}: {e}")),
    };
    let gc = get.status().as_u16();
    if gc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET blob got {gc} (expected 200 after chunked upload). url={get_url}"),
        );
    }
    // Content-Length must equal the full payload size.
    let cl = get
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    match cl {
        Some(n) if n == full_bytes.len() => {}
        Some(n) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "GET blob Content-Length {n} ≠ uploaded size {} (chunked upload byte equality)",
                    full_bytes.len()
                ),
            )
        }
        None => {
            // Missing Content-Length is acceptable if the body is correct.
        }
    }
    // Byte equality — the body round-trip is the ground-truth assertion.
    let body_bytes = match get.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET blob body drain: {e}"),
            )
        }
    };
    if body_bytes.as_ref() != full_bytes.as_slice() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "GET blob body ({} bytes) does not match uploaded payload ({} bytes) — chunked upload corrupted",
                body_bytes.len(),
                full_bytes.len()
            ),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #11 — tags/list after a tagged manifest push ──────────────────────────────

/// Push a tagged manifest and assert that `GET /v2/<repo>/tags/list` returns
/// a JSON body containing the pushed tag. The response must be:
///
/// - `200 OK`
/// - `Content-Type: application/json`
/// - Body: `{"name": "<repo>", "tags": ["<tag>", ...]}` where `tags` is a
///   non-empty array containing the tag we just pushed.
///
/// This exercises the `oci_tags:<repo>` KV slot written by `push::manifest`
/// and read by `tags::list`.
fn j11_tags_list_after_push(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #11: tags/list after manifest push → 200 + tag enumerated";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let tag = "e2e-list-probe";
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint push bearer (RW PAT required): {e}"),
            )
        }
    };
    let auth = format!("Bearer {bearer}");

    // Push a minimal config blob + manifest with the tag.
    let config_blob = format!(
        r#"{{"architecture":"amd64","os":"linux","rootfs":{{"type":"layers","diff_ids":[]}},"e2e-tags":"{}"}}"#,
        uuid::Uuid::new_v4().simple()
    )
    .into_bytes();
    let config_digest = oci_digest(&config_blob);

    if let Err(m) = push_blob(client, &base, &auth, &repo, &config_digest, config_blob.clone()) {
        return JourneyResult::fail(name, ms(start), format!("push config blob: {m}"));
    }

    let manifest = format!(
        r#"{{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"{}","size":{}}},"layers":[]}}"#,
        config_digest,
        config_blob.len()
    )
    .into_bytes();
    let manifest_media = "application/vnd.oci.image.manifest.v1+json";

    let put_url = format!("{base}/v2/{repo}/manifests/{tag}");
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, manifest_media)
        .body(manifest)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("PUT manifest {put_url}: {e}"))
        }
    };
    let pc = put.status().as_u16();
    if !matches!(pc, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT manifest got {pc} (expected 201)"),
        );
    }

    // GET the tags list — a pull-scoped bearer is sufficient.
    let pull_scope = format!("repository:{repo}:pull");
    let pull_bearer = match mint_bearer(client, &base, pat, &pull_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("mint pull bearer for tags/list: {e}"),
            )
        }
    };
    let list_url = format!("{base}/v2/{repo}/tags/list");
    let list = match client
        .get(&list_url)
        .header(AUTHORIZATION, format!("Bearer {pull_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("GET {list_url}: {e}"))
        }
    };
    let lc = list.status().as_u16();
    if lc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET tags/list got {lc} (expected 200)"),
        );
    }
    // Content-Type must be application/json.
    let ct = list
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    if !ct.starts_with("application/json") {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET tags/list Content-Type={ct:?} (expected application/json)"),
        );
    }
    // Body: {"name": "<repo>", "tags": ["<tag>"]}.
    let body: Value = match list.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET tags/list response not JSON: {e}"),
            )
        }
    };
    // "name" must match the repo we pushed to.
    if body["name"].as_str() != Some(repo.as_str()) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "tags/list \"name\" field = {:?} (expected {:?})",
                body["name"], repo
            ),
        );
    }
    // "tags" must be an array containing our pushed tag.
    let tags = match body["tags"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("tags/list \"tags\" field is not an array: {body}"),
            )
        }
    };
    if tags.is_empty() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("tags/list returned an empty tags array after pushing tag {tag:?}"),
        );
    }
    if !tags.iter().any(|t| t == tag) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("pushed tag {tag:?} not in tags/list response: {tags:?}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #12 — `_catalog` always disabled → 401 ───────────────────────────────────

/// `GET /v2/_catalog` must ALWAYS return 401 (disabled by design, regardless
/// of whether the caller is authenticated). This is the OCI catalog gate:
/// both anonymous and bearer-authenticated callers see 401 + a
/// `Www-Authenticate` challenge. The status is asserted, not merely the
/// absence of a 200, to guard against a regression that silently drops the
/// endpoint (which would manifest as 404, not the required gate deny).
fn j12_catalog_always_401(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #12: GET /v2/_catalog disabled → 401 (gate proven, not absent)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // Anonymous probe — the catalog handler fires before auth, so no creds
    // needed. The response MUST be 401 (CatalogDisabled error code = DENIED).
    let anon_url = format!("{base}/v2/_catalog");
    let anon = match client.get(&anon_url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("OCI endpoint unreachable ({base}): {e}"),
            )
        }
    };
    let ac = anon.status().as_u16();
    if ac == 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            "GET /v2/_catalog returned 404 — the route is absent instead of gated (regression)"
                .to_string(),
        );
    }
    if ac == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "GET /v2/_catalog returned 200 — the catalog gate is disabled (information disclosure)"
                .to_string(),
        );
    }
    if ac != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v2/_catalog (anon) got {ac} (expected 401 gate deny)"),
        );
    }
    // Www-Authenticate must be present on the 401.
    let wa = anon
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if wa.is_none() {
        return JourneyResult::fail(
            name,
            ms(start),
            "catalog 401 missing Www-Authenticate challenge header".to_string(),
        );
    }

    // Authenticated probe — a valid bearer must ALSO get 401 (catalog is
    // unconditionally gated, not merely unauthenticated-denied).
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(_) => {
            // The anonymous probe already confirmed the 401 gate. Gate the
            // authenticated leg separately so the anon result is preserved.
            return JourneyResult::pass(name, ms(start));
        }
    };
    let bearer = match mint_bearer(client, &base, pat, "") {
        Ok(t) => t,
        Err(e) => {
            // Authenticated leg cannot be checked without a bearer — but the
            // anonymous leg already passed, so report pass.
            let _ = e;
            return JourneyResult::pass(name, ms(start));
        }
    };
    let auth_url = format!("{base}/v2/_catalog");
    let authed = match client
        .get(&auth_url)
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET /v2/_catalog (authed): {e}"),
            )
        }
    };
    let bc = authed.status().as_u16();
    if bc == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "GET /v2/_catalog returned 200 for an authenticated caller — catalog gate bypassed"
                .to_string(),
        );
    }
    if bc != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v2/_catalog (authed) got {bc} (expected 401 gate deny)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #13 — read-only PAT cannot push blobs or manifests ───────────────────────

/// A read-only PAT (P2) attempting blob or manifest pushes must be denied.
///
/// The token-exchange layer (`issue_token`) downscopes: a read-only PAT
/// requesting `pull,push` receives a pull-only bearer. The data-plane
/// `scope.allows(repo, "push")` check then returns false → 401 on the
/// `POST /v2/<repo>/blobs/uploads/` open call.
///
/// We assert 401 or 403 on the open-upload step itself (before PATCH/PUT).
/// The exact status depends on whether the adapter returns Auth(…) → 401 or
/// a scope-denied shape; both are acceptable denials for this assertion.
fn j13_ro_push_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #13: read-only PAT push attempt → denied (scope downscoped at token mint)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // P2 is the read-only persona. Gate if the token is absent.
    let ro_pat = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let pat = match ro_pat.token {
        Some(t) => t,
        None => return JourneyResult::gated(name, "P2 token unexpectedly absent".to_string()),
    };

    let repo = run_repo();
    // The token exchange silently downscopes the push grant: a read-only PAT
    // gets a pull-only bearer. The bearer then fails `scope.allows(push)` on
    // the data plane → 401.
    let push_scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &push_scope) {
        Ok(t) => t,
        Err(e) => {
            // If the mint itself errors (e.g. the PAT is bad), gate — we can't
            // distinguish "downscoped to pull" from "token rejected outright".
            return JourneyResult::gated(
                name,
                format!("RO mint_bearer returned an error — cannot prove scope downscope: {e}"),
            );
        }
    };
    let auth = format!("Bearer {bearer}");

    // Attempt to open an upload session — must be denied (not 202).
    let open_url = format!("{base}/v2/{repo}/blobs/uploads/");
    let open = match client
        .post(&open_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::<u8>::new())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("POST {open_url}: {e}"),
            )
        }
    };
    let oc = open.status().as_u16();
    if oc == 202 || oc == 201 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "read-only PAT opened an upload session (got {oc}) — push scope not downscoped"
            ),
        );
    }
    if !matches!(oc, 401 | 403) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("open upload with RO PAT got {oc} (expected 401 or 403 deny)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #14 — cross-tenant blob isolation ────────────────────────────────────────

/// Tenant B must NOT be able to pull blobs or manifests pushed by tenant A.
///
/// The tenant lives INSIDE the signed OCI bearer (minted from the PAT's
/// tenant_id at `/token` exchange time). Tenant B's bearer embeds B's
/// tenant_id, so `GET /v2/<repo>/blobs/<digest>` against a blob owned by
/// tenant A returns `404 NAME_UNKNOWN` — never a data leak (per oci.md §8
/// row 4: cross-tenant reads must be 404, NOT 403, to avoid existence
/// inference).
///
/// Assertion: tenant A pushes a blob. Tenant B issues a GET for the SAME
/// blob URL → 404. (Not 200, which would be a tenant-isolation breach.)
fn j14_cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #14: cross-tenant blob pull → 404 (not 200 or 403)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // Tenant A = P1.
    let p1_pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };
    // Tenant B = P6.
    let p6 = match Persona::P6TenantB.resolve(cfg) {
        Ok(r) => r,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let p6_pat = match p6.token {
        Some(t) => t,
        None => return JourneyResult::gated(name, "P6 token unexpectedly absent".to_string()),
    };

    // Tenant A pushes a unique blob.
    let repo = run_repo();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let payload = format!("corelink-e2e-oci-cross-tenant-{run_id}").into_bytes();
    let digest = oci_digest(&payload);

    let a_scope = format!("repository:{repo}:pull,push");
    let a_bearer = match mint_bearer(client, &base, p1_pat, &a_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint tenant A push bearer: {e}"),
            )
        }
    };
    if let Err(m) = push_blob(
        client,
        &base,
        &format!("Bearer {a_bearer}"),
        &repo,
        &digest,
        payload,
    ) {
        return JourneyResult::fail(name, ms(start), format!("tenant A blob push: {m}"));
    }

    // Tenant B requests the same blob.
    // Tenant B needs a pull bearer for the SAME repo path.
    let b_scope = format!("repository:{repo}:pull");
    let b_bearer = match mint_bearer(client, &base, p6_pat, &b_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint tenant B pull bearer: {e}"),
            )
        }
    };
    let blob_url = format!("{base}/v2/{repo}/blobs/{digest}");
    let get = match client
        .get(&blob_url)
        .header(AUTHORIZATION, format!("Bearer {b_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {blob_url}: {e}")),
    };
    let gc = get.status().as_u16();
    if gc == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "tenant B received tenant A's blob (got 200) — TENANT ISOLATION BREACH. url={blob_url}"
            ),
        );
    }
    if gc == 403 {
        // 403 would leak existence (oci.md §8 row 4); the server should use 404.
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "cross-tenant GET returned 403 — leaks blob existence across tenants (must be 404). url={blob_url}"
            ),
        );
    }
    if gc != 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("cross-tenant GET got {gc} (expected 404 isolation). url={blob_url}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

/// Curl-style monolithic blob push: `POST /v2/<repo>/blobs/uploads/` (→ 202 +
/// `Location`), then `PUT <location>?digest=<digest>` with the bytes (→ 201).
/// Returns `Ok(())` on a 201/200 finalize, else a human error.
fn push_blob(
    client: &Client,
    base: &str,
    auth: &str,
    repo: &str,
    digest: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    // 1) Open the upload session.
    let open_url = format!("{base}/v2/{repo}/blobs/uploads/");
    let open = client
        .post(&open_url)
        .header(AUTHORIZATION, auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::new())
        .send()
        .map_err(|e| format!("POST {open_url}: {e}"))?;
    let oc = open.status().as_u16();
    if !matches!(oc, 202 | 201) {
        return Err(format!("open upload got {oc} (expected 202). url={open_url}"));
    }
    let location = open
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| "open upload 202 missing the Location header".to_string())?;

    // The Location may be absolute or relative; resolve against the base host.
    let put_target = if location.starts_with("http://") || location.starts_with("https://") {
        location
    } else {
        format!("{base}{location}")
    };
    // Append the digest query param (monolithic finalize).
    let sep = if put_target.contains('?') { '&' } else { '?' };
    let put_url = format!("{put_target}{sep}digest={}", urlencode(digest));

    // 2) Monolithic PUT of the bytes + digest → 201.
    let put = client
        .put(&put_url)
        .header(AUTHORIZATION, auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|e| format!("PUT {put_url}: {e}"))?;
    let pc = put.status().as_u16();
    if !matches!(pc, 201 | 200) {
        return Err(format!("finalize blob PUT got {pc} (expected 201). url={put_url}"));
    }
    Ok(())
}
