//! # OCI `_public` cross-tenant SAFETY invariants (F3.2 inc7 / WP-D).
//!
//! The F3.2 campaign introduces a flag-gated cross-tenant public-base dedup path
//! for OCI layer blobs (`OCI_PUBLIC_DEDUP_ENABLED`, injected into
//! `OciMoatStore::new(…, dedup)` — see `routes/oci.rs`). Whether that flag is ON
//! or OFF, three cross-tenant SAFETY invariants must ALWAYS hold — they are the
//! properties the flip must never break. This module asserts them as HTTP
//! black-box regressions, so a future flip (or a routing regression) that leaked
//! private bytes, dropped the auth gate, or substituted content reds the gate.
//!
//! ## Why these are FLAG-INDEPENDENT
//!
//! The dedup flag only ever changes routing for an **allowlisted PUBLIC base
//! layer** digest (owner-pinned; the shipped manifest is deny-all). For every
//! digest that is NOT on the allowlist — which is every digest these journeys
//! create, since they push freshly-randomised bytes — the routing is per-tenant
//! in BOTH flag states. So:
//!
//!   1. **Private isolation** — a non-allowlisted (private) layer pushed by
//!      tenant A is never served to tenant B, even for a byte-identical digest. A
//!      random digest is never allowlisted, so it stays per-tenant whether the
//!      flag is ON or OFF → the 404 assertion holds in both states.
//!   2. **PAT-gated** — every `/v2/` blob op requires a valid bearer for the URL
//!      tenant; a missing or forged bearer is rejected with an ACTIVE 401 gate
//!      deny (never a byte leak). The token gate sits ahead of the dedup routing,
//!      so the flag cannot affect it.
//!   3. **Digest integrity** — a served blob's bytes hash to the requested
//!      digest (no substitution). Write-time `verify_against_bytes` is fail-closed
//!      on both the per-tenant and the `_public` store, so the served-bytes ==
//!      digest property is independent of which store answered.
//!
//! ## Flag state under test (honest boundary)
//!
//! `OCI_PUBLIC_DEDUP_ENABLED` is a **prod boot var** injected at container start;
//! there is no black-box HTTP surface to toggle or observe it. So this module
//! asserts the invariants that hold under the CURRENTLY-DEPLOYED (flag-OFF)
//! reality. Each journey documents, inline, why its assertion ALSO holds under
//! flag-ON (the digests are non-allowlisted, so the dedup path is never taken).
//! None of the assertions DEPEND on the flag being on.
//!
//! ## Relationship to `journeys::oci`
//!
//! `journeys::oci` locks the OCI *conformance* fixes (the 9 docker-push
//! regressions + protocol ops), and its #14 proves cross-tenant isolation for a
//! generic push. This module is the F3.2-specific SAFETY contract: it re-anchors
//! the same three properties AS THE INVARIANTS THE `_public` FLIP MUST PRESERVE,
//! with the identical-digest framing the dedup path makes load-bearing. Both are
//! black-box (deployed `/v2/` + `/token` + PATs only); this file re-implements the
//! tiny OCI auth primitives locally (as `oci.rs` does) so it owns no shared file.
//!
//! ## Gating
//!
//! Every journey GATES (never fails) when a required PAT/tenant is absent or the
//! OCI host is unreachable — recorded, never silently skipped. The invariants
//! require live creds (P1 read-write + P6 tenant-B PATs, an endpoint) to run; in
//! a credless CI they GATE. The OCI host defaults to `CORELINK_E2E_ENDPOINT` but
//! can be overridden with `CORELINK_E2E_OCI_ENDPOINT` (prod splits the OCI surface
//! onto the dedicated flat host `corelink-oci.humangr.com`).

use std::env;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::harness::{Config, JourneyResult};
use crate::personas::Persona;

/// Optional override for the OCI base URL (prod runs OCI on the dedicated flat
/// host `corelink-oci.humangr.com`). Falls back to the suite's primary endpoint.
const OCI_ENDPOINT_ENV: &str = "CORELINK_E2E_OCI_ENDPOINT";

/// The `service=` value the adapter stamps into every challenge + accepts on the
/// `/token` exchange.
const OCI_SERVICE: &str = "corelink-oci";

/// Run the F3.2 `_public` cross-tenant safety invariants.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    // Belt-and-suspenders warm of the scale-to-zero OCI host (idempotent with the
    // global pre-warm in `mod::all`) so a cold-start does not flake the invariants.
    let _ = crate::journeys::oci::warm_oci(cfg, client);
    vec![
        inv1_private_layer_never_leaks_cross_tenant(cfg, client),
        inv2_blob_ops_are_pat_gated(cfg, client),
        inv3_served_blob_matches_requested_digest(cfg, client),
    ]
}

// ── tiny base64 (standard alphabet) — no extra dep (mirrors oci.rs) ───────────

/// Minimal standard (RFC 4648) base64 encoder — encode-only, padded. The OCI
/// Basic leg needs `base64(user:PAT)` and the frozen dep set has no base64 crate.
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

/// Lowercase-hex sha256 of `bytes` (the digest-integrity anchor recomputed on a
/// served body).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Minimal percent-encoder for the characters that appear in an OCI scope
/// (`repository:<repo>:pull,push`) + a digest query value.
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

/// Resolve the OCI base URL (override env or the suite endpoint), trailing slash
/// trimmed.
fn oci_base(cfg: &Config) -> String {
    env::var(OCI_ENDPOINT_ENV)
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_else(|| cfg.endpoint.clone())
}

/// A deterministic-per-run repository name. Content-addressed blobs make the push
/// idempotent; the fresh repo keeps the run hermetic (no cross-run collision).
fn run_repo() -> String {
    format!("e2e-f32-isolation/run-{}", uuid::Uuid::new_v4().simple())
}

/// Mint an OCI bearer from `pat` for `scope` (empty ⇒ docker-login credential
/// token) via the GET `/token` leg. Returns the bearer or a human error.
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

/// Curl-style monolithic blob push: `POST /v2/<repo>/blobs/uploads/` (→ 202 +
/// `Location`), then `PUT <location>?digest=<digest>` with the bytes (→ 201).
/// Returns `Ok(())` on a 200/201 finalize, else a human error.
fn push_blob(
    client: &Client,
    base: &str,
    auth: &str,
    repo: &str,
    digest: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
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
        return Err(format!(
            "open upload got {oc} (expected 202). url={open_url}"
        ));
    }
    let location = open
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| "open upload 202 missing the Location header".to_string())?;
    let put_target = if location.starts_with("http://") || location.starts_with("https://") {
        location
    } else {
        format!("{base}{location}")
    };
    let sep = if put_target.contains('?') { '&' } else { '?' };
    let put_url = format!("{put_target}{sep}digest={}", urlencode(digest));
    let put = client
        .put(&put_url)
        .header(AUTHORIZATION, auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|e| format!("PUT {put_url}: {e}"))?;
    let pc = put.status().as_u16();
    if !matches!(pc, 201 | 200) {
        return Err(format!(
            "finalize blob PUT got {pc} (expected 201). url={put_url}"
        ));
    }
    Ok(())
}

/// Resolve P1's (tenant A) PAT or a gate reason.
fn p1_pat<'a>(cfg: &'a Config, name: &'static str) -> Result<&'a str, JourneyResult> {
    match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => Ok(p.token.expect("P1 always has a token")),
        Err(reason) => Err(JourneyResult::gated(name, reason)),
    }
}

// ── INVARIANT 1 — a private (non-allowlisted) layer never leaks cross-tenant ──

/// **Private isolation (flag-independent).** Tenant A pushes a NON-allowlisted
/// (private, freshly-randomised) OCI layer blob to a repo; tenant B, holding its
/// own valid bearer for the SAME repo path and requesting the SAME digest, must
/// NOT receive A's bytes. The correct, existence-non-leaking answer is `404`
/// (per oci.md §8 row 4: cross-tenant reads are 404, never 403, to avoid
/// existence inference; a `200` would be a tenant-isolation BREACH).
///
/// FLAG-INDEPENDENT: the dedup path only ever shares an ALLOWLISTED public-base
/// digest. This blob's digest is the sha256 of fresh random bytes, so it is never
/// on the owner-pinned allowlist — the routing is per-tenant whether
/// `OCI_PUBLIC_DEDUP_ENABLED` is ON or OFF. Under flag-ON the ONLY behaviour that
/// changes is that an allowlisted base layer would be shared; a private layer
/// like this one stays isolated, so the 404 holds identically in both states.
fn inv1_private_layer_never_leaks_cross_tenant(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI _public inv1: private layer never served cross-tenant (identical digest → 404)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // Tenant A = P1 (read-write). Tenant B = P6 (a DIFFERENT tenant's valid PAT).
    let a_pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(r) => r,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let b_pat = match b.token {
        Some(t) => t,
        None => return JourneyResult::gated(name, "P6 token unexpectedly absent".to_string()),
    };

    // Tenant A pushes a unique PRIVATE layer (random bytes ⇒ non-allowlisted).
    let repo = run_repo();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let payload = format!("corelink-e2e-f32-private-layer-{run_id}").into_bytes();
    let digest = oci_digest(&payload);

    let a_scope = format!("repository:{repo}:pull,push");
    let a_bearer = match mint_bearer(client, &base, a_pat, &a_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(name, format!("could not mint tenant A push bearer: {e}"))
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
        return JourneyResult::fail(name, ms(start), format!("tenant A private layer push: {m}"));
    }

    // Tenant B mints its OWN pull bearer for the SAME repo path and requests the
    // SAME digest. The tenant lives INSIDE B's signed bearer, so the store is
    // scoped to B — A's private bytes must not appear.
    let b_scope = format!("repository:{repo}:pull");
    let b_bearer = match mint_bearer(client, &base, b_pat, &b_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(name, format!("could not mint tenant B pull bearer: {e}"))
        }
    };
    let blob_url = format!("{base}/v2/{repo}/blobs/{digest}");
    let get = match client
        .get(&blob_url)
        .header(AUTHORIZATION, format!("Bearer {b_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B GET {blob_url}: {e}")),
    };
    let gc = get.status().as_u16();
    if gc == 200 {
        let n = get.bytes().map(|b| b.len()).unwrap_or(0);
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "TENANT ISOLATION BREACH: tenant B received tenant A's private layer (200, {n} \
                 bytes) at the identical digest {digest} — the `_public` flip leaked private bytes. \
                 url={blob_url}"
            ),
        );
    }
    if gc == 403 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "cross-tenant GET returned 403 — leaks blob existence across tenants (must be 404 \
                 per oci.md §8). url={blob_url}"
            ),
        );
    }
    if gc != 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("cross-tenant private GET got {gc} (expected 404 isolation). url={blob_url}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── INVARIANT 2 — every /v2/ blob op is PAT-gated ────────────────────────────

/// **PAT-gated (flag-independent).** Every `/v2/` blob op requires a valid bearer
/// for the URL tenant. This journey proves the auth gate ACTIVELY rejects, and
/// leaks no bytes, for the two credential-absent cases the dedup routing sits
/// behind:
///
///   - **no Authorization header** on a blob GET → `401` + a `Www-Authenticate`
///     challenge (docker's discovery path), NOT a `200` and NOT a bare `404`.
///   - **a forged/garbage bearer** on a blob GET → `401` (the HMAC bearer fails
///     to verify; the request never reaches the store).
///
/// A `404` here is a FAILURE (mirrors the harness `expect_gate_denied` rule): a
/// 404 would mean the route is absent/renamed, so "the gate rejected this" would
/// be unproven. We require an ACTIVE `401` (or `403`).
///
/// FLAG-INDEPENDENT: the token gate runs BEFORE the `OciMoatStore` dedup routing,
/// so `OCI_PUBLIC_DEDUP_ENABLED` cannot change whether an unauthenticated or
/// forged request is admitted. (A cross-tenant VALID bearer is covered by inv1;
/// this journey covers missing + forged.)
fn inv2_blob_ops_are_pat_gated(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI _public inv2: blob ops PAT-gated (no/forged bearer → active 401, no leak)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let repo = run_repo();
    let digest = oci_digest(b"corelink-e2e-f32-pat-gate-probe");
    let blob_url = format!("{base}/v2/{repo}/blobs/{digest}");

    // Leg A — NO Authorization header. Must be an ACTIVE gate deny (401), never a
    // 200 (leak) and never a bare 404 (route absent ⇒ gate unproven).
    let anon = match client.get(&blob_url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"))
        }
    };
    let ac = anon.status().as_u16();
    if ac == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("unauth blob GET returned 200 — a `/v2/` blob op is NOT PAT-gated (byte leak). url={blob_url}"),
        );
    }
    if ac != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "unauth blob GET got {ac} (expected an ACTIVE 401 gate deny; a 404 means the route \
                 is absent/renamed, not that the gate rejected — PAT-gating UNPROVEN). url={blob_url}"
            ),
        );
    }
    // The 401 must carry a Www-Authenticate challenge (docker's token discovery).
    if anon
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .is_none()
    {
        return JourneyResult::fail(
            name,
            ms(start),
            "unauth blob GET 401 missing the Www-Authenticate challenge header".to_string(),
        );
    }

    // Leg B — a FORGED/garbage bearer. The HMAC bearer must fail verification →
    // 401 (never 200). A forged token that reached the store would be a critical
    // auth bypass.
    let forged = format!("Bearer forged.{}.not-a-real-hmac", uuid::Uuid::new_v4());
    let forged_resp = match client.get(&blob_url).header(AUTHORIZATION, &forged).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("forged-bearer GET {blob_url}: {e}"),
            )
        }
    };
    let fc = forged_resp.status().as_u16();
    if fc == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("AUTH BYPASS: a forged bearer received 200 on a blob GET — the OCI bearer HMAC is not verified. url={blob_url}"),
        );
    }
    if !matches!(fc, 401 | 403) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "forged-bearer blob GET got {fc} (expected an ACTIVE 401/403 gate deny; a 404 \
                 leaves the forged-token rejection UNPROVEN). url={blob_url}"
            ),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── INVARIANT 3 — a served blob's bytes hash to the requested digest ─────────

/// **Digest integrity (flag-independent).** Push a blob and read it back; the
/// served bytes must hash (sha256) to the digest that addressed them — no
/// substitution. If the registry served the `Docker-Content-Digest` header, it
/// too must equal the requested digest. A mismatch would mean the store returned
/// bytes other than the ones written under that address (a poisoned/substituted
/// serve), the exact failure the `_public` write-time `verify_against_bytes`
/// fail-closed check exists to prevent.
///
/// FLAG-INDEPENDENT: write-time digest verification is fail-closed on BOTH the
/// per-tenant store and the `_public` store, so the served-bytes == digest
/// property does not depend on which store answered — it holds whether the dedup
/// flag is ON or OFF. (This blob is per-tenant in both states anyway, being
/// non-allowlisted; under flag-ON an allowlisted `_public` serve would satisfy
/// the SAME assertion because the promote path verifies before store.)
fn inv3_served_blob_matches_requested_digest(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI _public inv3: served blob bytes hash to the requested digest (no substitution)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let payload = format!("corelink-e2e-f32-digest-integrity-{run_id}").into_bytes();
    let digest = oci_digest(&payload);
    let want_hex = digest
        .strip_prefix("sha256:")
        .expect("oci_digest always prefixes sha256:")
        .to_string();

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
    if let Err(m) = push_blob(
        client,
        &base,
        &format!("Bearer {bearer}"),
        &repo,
        &digest,
        payload.clone(),
    ) {
        return JourneyResult::fail(name, ms(start), format!("push blob: {m}"));
    }

    // Read the blob back with a pull bearer and assert byte + digest integrity.
    let pull_scope = format!("repository:{repo}:pull");
    let pull_bearer = match mint_bearer(client, &base, pat, &pull_scope) {
        Ok(t) => t,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("mint pull bearer: {e}")),
    };
    let blob_url = format!("{base}/v2/{repo}/blobs/{digest}");
    let get = match client
        .get(&blob_url)
        .header(AUTHORIZATION, format!("Bearer {pull_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {blob_url}: {e}")),
    };
    let gc = get.status().as_u16();
    if gc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET blob got {gc} (expected 200 after push). url={blob_url}"),
        );
    }
    // If present, the Docker-Content-Digest header must equal the requested digest
    // (read BEFORE consuming the body).
    let content_digest = get
        .headers()
        .get("docker-content-digest")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string());
    if let Some(cd) = &content_digest {
        if !cd.is_empty() && cd != &digest {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "Docker-Content-Digest header {cd:?} != requested digest {digest:?} — the \
                     registry addressed the served bytes to a DIFFERENT digest (substitution)"
                ),
            );
        }
    }
    let body = match get.bytes() {
        Ok(b) => b.to_vec(),
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET blob body: {e}")),
    };
    // The ground-truth assertion: the served bytes hash to the requested digest.
    let got_hex = sha256_hex(&body);
    if got_hex != want_hex {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "DIGEST INTEGRITY VIOLATION: served {} bytes hash to sha256:{got_hex} but were \
                 requested at sha256:{want_hex} — the store substituted content. url={blob_url}",
                body.len()
            ),
        );
    }
    // Defensive: the served bytes must also equal the exact payload we pushed.
    if body != payload {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "served bytes ({} B) differ from the pushed payload ({} B) despite a matching \
                 digest — impossible without a sha256 collision; treat as corruption",
                body.len(),
                payload.len()
            ),
        );
    }
    JourneyResult::pass(name, ms(start))
}
