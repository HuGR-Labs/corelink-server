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
fn mint_bearer(client: &Client, base: &str, pat: &str, scope: &str) -> Result<String, String> {
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
        return Err(format!(
            "GET /token got {code} (expected 200) for scope={scope:?}"
        ));
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
    // waits for readiness; it asserts nothing. (Also pre-warmed globally in
    // `mod::all` before the adapters OCI journeys — this call is the idempotent
    // belt-and-suspenders when oci::run is invoked on its own.)
    //
    // `oci_host_reachable` performs the warmup AND records it as an explicit
    // PASS/FAIL row so a dead host reds the gate instead of silently gating the
    // whole module (auditor finding).
    vec![
        oci_host_reachable(cfg, client),
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
///
/// `pub(crate)` so the runner can pre-warm the scale-to-zero OCI host ONCE before
/// ANY module runs — the `adapters` module's OCI journeys run before this module,
/// so a warmup local to `oci::run` would be too late for them.
/// Returns `true` once the host answers warm (any non-5xx), `false` if the
/// bounded attempt budget is spent without a warm answer (host unreachable /
/// stuck cold). Callers use the bool to surface an EXPLICIT failure
/// (`oci_host_reachable`) rather than letting a dead OCI host silently turn the
/// whole module Gated/erroring (auditor finding).
pub(crate) fn warm_oci(cfg: &Config, client: &Client) -> bool {
    let url = format!("{}/v2/", oci_base(cfg));
    for attempt in 0..6 {
        match client.get(&url).send() {
            Ok(r) if r.status().as_u16() < 500 => return true, // warm (e.g. 401 challenge)
            _ => {
                // Timeout or 5xx → still warming. The 30s client timeout already
                // paces a hung cold-start; pause briefly after a fast 5xx too.
                if attempt < 5 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    false
}

/// EXPLICIT reachability assertion (auditor tooth-audit): if the scale-to-zero
/// OCI host never warms within the budget, the registry is genuinely
/// unreachable — surface ONE hard FAIL here instead of letting every OCI journey
/// below silently gate/error (a dead host would otherwise read as an all-Gated
/// module, never RED). A warm host (the normal case) PASSES fast.
mod early;
mod late;
use early::*;
use late::*;
