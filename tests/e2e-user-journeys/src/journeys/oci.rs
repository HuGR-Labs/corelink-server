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

/// Run the OCI conformance journeys (9 regressions).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
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
    ]
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
