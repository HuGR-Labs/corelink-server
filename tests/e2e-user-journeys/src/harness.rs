//! # E2E black-box harness — the shared kit every journey module uses.
//!
//! BLACK-BOX rules (INVIOLABLE — apply to this file and EVERY journey module):
//!   - ONLY the deployed HTTP API + Bearer PAT + the `corelink` CLI binary.
//!   - FORBIDDEN: any `corelink-*` crate import, direct D1/R2/KV access, any
//!     mock of the system-under-test, reading internal state to assert.
//!   - Dependencies are frozen at: reqwest(blocking) + serde_json + uuid +
//!     sha2 + hex. Do NOT add more.
//!
//! This module owns:
//!   - [`Config`] — endpoint + the full token/tenant map, all from env.
//!   - [`JourneyResult`] / [`JourneyStatus`] — the per-journey verdict.
//!   - [`build_client`], [`bearer`], blob + SHA-256 helpers.
//!   - The **URL builder functions** — the single source of truth for every
//!     real route template (grounded in the live router, NOT invented).
//!   - assert helpers: [`expect_status`], [`expect_denied`].
//!
//! ## Why every URL goes through a builder fn here
//!
//! The old single-file suite hard-coded WRONG paths (e.g.
//! `/v1/cas/blobs/{hash}/{size}` — that route does not exist). The real CAS
//! route is `/v1/cas/{tenant}/{hash}`. To stop that class of bug, journeys
//! NEVER `format!` a path inline: they call a builder fn here, and the
//! builder table is the captured-from-source contract (see
//! `URL-template table` below).
//!
//! ## URL-template table (captured from the live router 2026-06-20)
//!
//! Sources: `crates/corelink-container/src/routes/*.rs` (the per-file
//! `router()` fns + `*_ROUTE` consts) and `worker/src/index.ts` (`matchRoute`).
//! `{tenant}` is the caller's tenant id; the Worker forwards the path UNCHANGED
//! for the path-tenant surfaces and re-derives/validates the tenant from the
//! PAT. `{hash}`/`{digest}` = 64 lowercase hex chars.
//!
//! | Surface          | Method        | Template                                                       |
//! |------------------|---------------|---------------------------------------------------------------|
//! | health (ok)      | GET           | `/health`, `/_health`                                          |
//! | health (SERVING) | GET           | `/api/health`                                                  |
//! | users/me         | GET           | `/v1/users/me`                                                 |
//! | CAS read/write   | GET/PUT/DELETE| `/v1/cas/{tenant}/{hash}`                                      |
//! | CAS list         | GET           | `/v1/cas/{tenant}`                                             |
//! | CAS batch write  | POST          | `/v1/cas/{tenant}/batch`                                       |
//! | CAS batch read   | POST          | `/v1/cas/{tenant}/batch-read`                                  |
//! | CAS batch exists | POST          | `/v1/cas/{tenant}/batch-exists`                               |
//! | AC lookup/update | GET/PUT/DELETE| `/v1/ac/{tenant}/{action_digest}`                            |
//! | AC list          | GET           | `/v1/ac/{tenant}`                                              |
//! | Bazel CAS read   | GET           | `/bazel/v2/{instance}/blobs/{hash}/{size}`                    |
//! | Bazel AC r/w     | GET/PUT       | `/bazel/v2/{instance}/blobs/ac/{hash}/{size}`                 |
//! | Bazel CAS write  | PUT           | `/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}`     |
//! | Bazel findMissing| POST          | `/bazel/v2/{instance}/findMissingBlobs`                       |
//! | Turbo get/put    | GET/PUT       | `/v8/artifacts/{hash}`  (tenant via `?teamId=`/`?slug=`)      |
//! | Turbo status     | POST          | `/v8/artifacts/status`                                         |
//! | Turbo events     | POST          | `/v8/artifacts/events`                                         |
//! | cargo (sccache)  | GET/PUT/HEAD  | `/cargo/{tenant}/{key}`                                        |
//! | npm              | GET           | `/npm/{tenant}/{rest…}`  (e.g. `/{pkg}`, `/{pkg}/-/{tgz}`)    |
//! | pip              | GET           | `/pip/{tenant}/{pep-path…}`                                    |
//! | brew             | GET           | `/brew/{tenant}/{bottle-path…}`                               |
//! | OCI token        | GET           | `/token`  (Basic base64(user:PAT))                            |
//! | OCI v2           | GET/PUT/...   | `/v2/{name}/...`  (forwarded UNCHANGED; tenant from OCI token)|
//! | customer overview| GET           | `/v1/customer/overview`                                        |
//! | customer usage   | GET           | `/v1/customer/usage`                                           |
//! | customer billing | GET           | `/v1/customer/billing`                                         |
//! | billing portal   | POST          | `/v1/customer/billing/portal`                                  |
//! | customer audit   | GET           | `/v1/customer/audit`                                           |
//! | customer keys    | GET/POST      | `/v1/customer/keys`                                            |
//! | key revoke       | POST          | `/v1/customer/keys/{pat_id}/revoke`                            |
//! | team list/invite | GET/POST      | `/v1/customer/team`, `/v1/customer/team/invite`              |
//! | audit export     | GET           | `/v1/audit/export`  (optional `?tenant=` cross-tenant probe)  |
//! | tier-select      | POST          | `/v1/onboarding/tier-select`  (Clerk session, not PAT)        |
//! | introspect       | POST          | `/internal/v1/auth/introspect`  (internal-auth gated)         |
//!
//! ### Differences vs the old main.rs / the matrix (FLAGGED)
//!   - CAS is `/v1/cas/{tenant}/{hash}` — NOT `/v1/cas/blobs/{hash}/{size}`.
//!     The blob/size shape is the BAZEL surface, not native CAS.
//!   - There is NO `/v1/admin/audit/events` route. The customer-facing audit
//!     export is `GET /v1/audit/export`; the admin surface is
//!     `/v1/admin/read/{resource}` + `/v1/admin/mutate`.
//!   - `/internal/v1/auth/introspect` is a CONTAINER-internal route gated by
//!     `X-Corelink-Internal-Auth`; it is NOT reachable with a plain customer
//!     PAT from the public edge (the introspect journey gates accordingly).

use std::env;
use std::time::Duration;

use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

// ── Configuration ───────────────────────────────────────────────────────────

/// The black-box test configuration, resolved entirely from the environment.
///
/// Every value is optional except `endpoint`; an absent token simply makes the
/// journeys that need it **GATE** (recorded, not silently skipped).
pub struct Config {
    /// Base URL of the API under test (trailing slash trimmed).
    pub endpoint: String,
    /// Primary tenant id (path segment for `/v1/cas/{tenant}/...` etc.).
    pub tenant: Option<String>,
    /// Second tenant id, for cross-tenant isolation journeys.
    pub tenant_b: Option<String>,
    /// Token map keyed by [`TokenKind`]. Absent kinds gate the journey.
    tokens: TokenMap,
    /// Slow / destructive journeys are opt-in via this flag.
    pub run_slow: bool,
}

/// The set of named PATs the suite knows how to consume, each from its own env
/// var. A journey resolves the persona it needs ([`crate::personas`]) which in
/// turn pulls the right token here.
#[derive(Default)]
struct TokenMap {
    rw: Option<String>,
    ro: Option<String>,
    admin: Option<String>,
    revoked: Option<String>,
    expired: Option<String>,
    tenant_b: Option<String>,
    free: Option<String>,
    solo: Option<String>,
    pro: Option<String>,
    enterprise: Option<String>,
    pastdue: Option<String>,
}

/// A named PAT slot. Resolved to an actual token (or `None` → gate) via
/// [`Config::token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Read+write cache PAT for the primary tenant.
    ReadWrite,
    /// Read-only cache PAT for the primary tenant.
    ReadOnly,
    /// Admin-scoped PAT for the primary tenant.
    Admin,
    /// A PAT that has been revoked (must be denied).
    Revoked,
    /// A PAT that has expired (must be denied).
    Expired,
    /// A valid PAT belonging to a DIFFERENT tenant (isolation tests).
    TenantB,
    /// A PAT on the Free plan.
    Free,
    /// A PAT on the Solo plan.
    Solo,
    /// A PAT on the Pro plan.
    Pro,
    /// A PAT on the Enterprise plan.
    Enterprise,
    /// A PAT whose subscription is past-due (billing journeys).
    PastDue,
}

impl Config {
    /// Resolve the full config from the environment. Never panics; missing
    /// optional vars simply gate the journeys that need them.
    pub fn from_env() -> Self {
        let var = |k: &str| env::var(k).ok().filter(|v| !v.is_empty());
        Config {
            endpoint: env::var("CORELINK_E2E_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:8787".to_string())
                .trim_end_matches('/')
                .to_string(),
            tenant: var("CORELINK_E2E_TENANT"),
            tenant_b: var("CORELINK_E2E_TENANT_B"),
            tokens: TokenMap {
                rw: var("CORELINK_E2E_PAT_RW"),
                ro: var("CORELINK_E2E_PAT_RO"),
                admin: var("CORELINK_E2E_PAT_ADMIN"),
                revoked: var("CORELINK_E2E_PAT_REVOKED"),
                expired: var("CORELINK_E2E_PAT_EXPIRED"),
                tenant_b: var("CORELINK_E2E_PAT_TENANT_B"),
                free: var("CORELINK_E2E_PAT_FREE"),
                solo: var("CORELINK_E2E_PAT_SOLO"),
                pro: var("CORELINK_E2E_PAT_PRO"),
                enterprise: var("CORELINK_E2E_PAT_ENTERPRISE"),
                pastdue: var("CORELINK_E2E_PAT_PASTDUE"),
            },
            run_slow: env::var("CORELINK_E2E_RUN_SLOW")
                .map(|v| v == "1")
                .unwrap_or(false),
        }
    }

    /// Resolve a named token, or `None` if its env var was absent (→ gate).
    pub fn token(&self, kind: TokenKind) -> Option<&str> {
        let slot = match kind {
            TokenKind::ReadWrite => &self.tokens.rw,
            TokenKind::ReadOnly => &self.tokens.ro,
            TokenKind::Admin => &self.tokens.admin,
            TokenKind::Revoked => &self.tokens.revoked,
            TokenKind::Expired => &self.tokens.expired,
            TokenKind::TenantB => &self.tokens.tenant_b,
            TokenKind::Free => &self.tokens.free,
            TokenKind::Solo => &self.tokens.solo,
            TokenKind::Pro => &self.tokens.pro,
            TokenKind::Enterprise => &self.tokens.enterprise,
            TokenKind::PastDue => &self.tokens.pastdue,
        };
        slot.as_deref()
    }

    /// Primary tenant id, or the `_anonymous` sentinel when unset.
    pub fn tenant_or_anon(&self) -> &str {
        self.tenant.as_deref().unwrap_or("_anonymous")
    }
}

// ── Journey result ──────────────────────────────────────────────────────────

/// The verdict of a single journey.
#[derive(Debug, Clone)]
pub enum JourneyStatus {
    /// The journey ran end-to-end and the live contract held.
    Pass,
    /// The journey ran and the live contract was violated.
    Fail(String),
    /// A prerequisite (token/CLI/flag) was absent — recorded, never silently
    /// skipped. Stubs also use this with a `TODO:` message.
    Gated(String),
}

/// One row in the results table.
pub struct JourneyResult {
    /// Short human-readable journey name.
    pub name: &'static str,
    /// The verdict.
    pub status: JourneyStatus,
    /// Wall-clock duration in ms.
    pub duration_ms: u64,
}

impl JourneyResult {
    /// A `Gated` stub result — used by not-yet-implemented journey modules so
    /// the skeleton compiles and runs (the fleet replaces these).
    pub fn stub(name: &'static str, surface: &str) -> Self {
        JourneyResult {
            name,
            status: JourneyStatus::Gated(format!(
                "TODO: {surface} journeys not yet implemented"
            )),
            duration_ms: 0,
        }
    }

    /// A `Gated` result with a custom reason (prerequisite missing).
    pub fn gated(name: &'static str, reason: impl Into<String>) -> Self {
        JourneyResult {
            name,
            status: JourneyStatus::Gated(reason.into()),
            duration_ms: 0,
        }
    }

    /// A `Pass` result.
    pub fn pass(name: &'static str, duration_ms: u64) -> Self {
        JourneyResult {
            name,
            status: JourneyStatus::Pass,
            duration_ms,
        }
    }

    /// A `Fail` result.
    pub fn fail(name: &'static str, duration_ms: u64, msg: impl Into<String>) -> Self {
        JourneyResult {
            name,
            status: JourneyStatus::Fail(msg.into()),
            duration_ms,
        }
    }
}

// ── HTTP / crypto helpers ─────────────────────────────────────────────────────

/// Build the shared blocking HTTP client (30s timeout, real TLS verification).
pub fn build_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(false)
        .build()
        .expect("failed to build reqwest client")
}

/// Format a `Bearer <token>` header value.
pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Generate a unique blob payload for a hermetic run (content-addressed, so a
/// fresh UUID means a fresh digest every run).
pub fn unique_blob(prefix: &str) -> Vec<u8> {
    format!("{prefix}-{}", uuid::Uuid::new_v4()).into_bytes()
}

/// Lowercase-hex SHA-256 of `bytes`. Use for OPAQUE keys (AC action digests,
/// Turbo artifact hashes) where the server does NOT recompute the address from
/// the body — a unique string suffices.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Lowercase-hex BLAKE3 of `bytes` — the **native CAS content address**.
/// CoreLink CAS verifies `body`'s BLAKE3 against the URL `{hash}` and returns
/// 422 "content hash mismatch" on divergence (`routes/cas.rs`). So CAS journeys
/// MUST address with this, NOT [`sha256_hex`].
pub fn blake3_hex(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

// ── assert helpers ────────────────────────────────────────────────────────────

/// `Ok(())` iff `got == want`; otherwise an `Err` carrying a contract message.
pub fn expect_status(label: &str, got: u16, want: u16) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{label}: got {got}, expected {want}"))
    }
}

/// `Ok(())` iff `got` is one of the "denied" statuses (401 / 403 / 404). Use
/// for negative paths where any of unauthenticated / forbidden / hidden is an
/// acceptable deny (e.g. tenant isolation: a miss and a forbid are both safe).
pub fn expect_denied(label: &str, got: u16) -> Result<(), String> {
    if matches!(got, 401 | 403 | 404) {
        Ok(())
    } else {
        Err(format!(
            "{label}: got {got}, expected a deny (401/403/404)"
        ))
    }
}

// ── URL builders — the ONE source of truth for every route template ───────────
//
// Journeys MUST go through these. See the URL-template table in the module doc.
// Many builders are not yet consumed by the migrated journeys — they are the
// frozen contract the fleet fills against (dead-code allowed crate-wide via the
// `#![allow(dead_code)]` scaffold attribute in `main.rs`).

/// `GET /api/health` → `{"status":"SERVING",...}`.
pub fn url_health_serving(cfg: &Config) -> String {
    format!("{}/api/health", cfg.endpoint)
}

/// `GET /health` / `/_health` → `{"status":"ok",...}`.
pub fn url_health(cfg: &Config) -> String {
    format!("{}/health", cfg.endpoint)
}

/// `GET /v1/users/me`.
pub fn url_users_me(cfg: &Config) -> String {
    format!("{}/v1/users/me", cfg.endpoint)
}

/// Native CAS read/write/delete: `/v1/cas/{tenant}/{hash}`.
pub fn url_cas(cfg: &Config, tenant: &str, hash: &str) -> String {
    format!("{}/v1/cas/{tenant}/{hash}", cfg.endpoint)
}

/// CAS list: `/v1/cas/{tenant}`.
pub fn url_cas_list(cfg: &Config, tenant: &str) -> String {
    format!("{}/v1/cas/{tenant}", cfg.endpoint)
}

/// CAS batch write: `/v1/cas/{tenant}/batch`.
pub fn url_cas_batch(cfg: &Config, tenant: &str) -> String {
    format!("{}/v1/cas/{tenant}/batch", cfg.endpoint)
}

/// CAS batch read: `/v1/cas/{tenant}/batch-read`.
pub fn url_cas_batch_read(cfg: &Config, tenant: &str) -> String {
    format!("{}/v1/cas/{tenant}/batch-read", cfg.endpoint)
}

/// CAS batch exists: `/v1/cas/{tenant}/batch-exists`.
pub fn url_cas_batch_exists(cfg: &Config, tenant: &str) -> String {
    format!("{}/v1/cas/{tenant}/batch-exists", cfg.endpoint)
}

/// AC lookup/update/delete: `/v1/ac/{tenant}/{action_digest}`.
pub fn url_ac(cfg: &Config, tenant: &str, action_digest: &str) -> String {
    format!("{}/v1/ac/{tenant}/{action_digest}", cfg.endpoint)
}

/// AC list: `/v1/ac/{tenant}`.
pub fn url_ac_list(cfg: &Config, tenant: &str) -> String {
    format!("{}/v1/ac/{tenant}", cfg.endpoint)
}

/// Bazel REAPI v2 CAS read: `/bazel/v2/{instance}/blobs/{hash}/{size}`.
pub fn url_bazel_cas_read(cfg: &Config, instance: &str, hash: &str, size: usize) -> String {
    format!("{}/bazel/v2/{instance}/blobs/{hash}/{size}", cfg.endpoint)
}

/// Bazel REAPI v2 AC read/write: `/bazel/v2/{instance}/blobs/ac/{hash}/{size}`.
pub fn url_bazel_ac(cfg: &Config, instance: &str, hash: &str, size: usize) -> String {
    format!("{}/bazel/v2/{instance}/blobs/ac/{hash}/{size}", cfg.endpoint)
}

/// Bazel REAPI v2 CAS write:
/// `/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}`.
pub fn url_bazel_cas_write(
    cfg: &Config,
    instance: &str,
    upload_uuid: &str,
    hash: &str,
    size: usize,
) -> String {
    format!(
        "{}/bazel/v2/{instance}/uploads/{upload_uuid}/blobs/{hash}/{size}",
        cfg.endpoint
    )
}

/// Bazel REAPI v2 find-missing: `/bazel/v2/{instance}/findMissingBlobs`.
pub fn url_bazel_find_missing(cfg: &Config, instance: &str) -> String {
    format!("{}/bazel/v2/{instance}/findMissingBlobs", cfg.endpoint)
}

/// Turbo get/put: `/v8/artifacts/{hash}` (tenant via `?teamId=` query).
pub fn url_turbo_artifact(cfg: &Config, hash: &str) -> String {
    format!("{}/v8/artifacts/{hash}", cfg.endpoint)
}

/// Turbo status: `/v8/artifacts/status`.
pub fn url_turbo_status(cfg: &Config) -> String {
    format!("{}/v8/artifacts/status", cfg.endpoint)
}

/// Turbo events: `/v8/artifacts/events`.
pub fn url_turbo_events(cfg: &Config) -> String {
    format!("{}/v8/artifacts/events", cfg.endpoint)
}

/// cargo (sccache) cache key: `/cargo/{tenant}/{key}`.
pub fn url_cargo(cfg: &Config, tenant: &str, key: &str) -> String {
    format!("{}/cargo/{tenant}/{key}", cfg.endpoint)
}

/// npm path: `/npm/{tenant}/{rest}`.
pub fn url_npm(cfg: &Config, tenant: &str, rest: &str) -> String {
    format!("{}/npm/{tenant}/{rest}", cfg.endpoint)
}

/// pip path: `/pip/{tenant}/{rest}`.
pub fn url_pip(cfg: &Config, tenant: &str, rest: &str) -> String {
    format!("{}/pip/{tenant}/{rest}", cfg.endpoint)
}

/// brew path: `/brew/{tenant}/{rest}`.
pub fn url_brew(cfg: &Config, tenant: &str, rest: &str) -> String {
    format!("{}/brew/{tenant}/{rest}", cfg.endpoint)
}

/// OCI token endpoint: `/token`.
pub fn url_oci_token(cfg: &Config) -> String {
    format!("{}/token", cfg.endpoint)
}

/// OCI v2 path (forwarded unchanged): `/v2/{rest}`.
pub fn url_oci_v2(cfg: &Config, rest: &str) -> String {
    format!("{}/v2/{rest}", cfg.endpoint)
}

/// Customer portal endpoint: `/v1/customer/{resource}`.
pub fn url_customer(cfg: &Config, resource: &str) -> String {
    format!("{}/v1/customer/{resource}", cfg.endpoint)
}

/// Customer-facing audit export: `/v1/audit/export`.
pub fn url_audit_export(cfg: &Config) -> String {
    format!("{}/v1/audit/export", cfg.endpoint)
}

/// Onboarding tier-select checkout: `/v1/onboarding/tier-select`.
pub fn url_tier_select(cfg: &Config) -> String {
    format!("{}/v1/onboarding/tier-select", cfg.endpoint)
}

/// Container-internal PAT introspection: `/internal/v1/auth/introspect`.
pub fn url_introspect(cfg: &Config) -> String {
    format!("{}/internal/v1/auth/introspect", cfg.endpoint)
}

/// Stripe webhook receiver on the **signup-worker** (a DIFFERENT host from the
/// API `endpoint`): `POST /webhooks/stripe` (`apps/signup-worker/src/index.ts`).
/// The base URL is passed explicitly because the signup-worker is not the API
/// under test — the billing-webhook journey supplies it from its own env var
/// (`CORELINK_E2E_SIGNUP_WORKER_ENDPOINT`).
pub fn url_stripe_webhook(signup_worker_base: &str) -> String {
    format!(
        "{}/webhooks/stripe",
        signup_worker_base.trim_end_matches('/')
    )
}

// ── Stripe webhook signing (frozen-dep, no `hmac`/`base64` crate) ─────────────
//
// The signup-worker verifies `Stripe-Signature: t=<ts>,v1=<hexhmac>` where the
// HMAC key is the base64-decoded body of a `whsec_<base64>` secret and the
// signed message is `"${ts}.${raw_body}"` (apps/signup-worker/src/webhooks/
// stripe.ts `verifyStripeSignature`). To construct a VALID test signature using
// ONLY the frozen deps (sha2 + hex — no `hmac`, no `base64` crate), we implement
// the two primitives the contract needs by hand:
//   - `b64_decode_std` — standard-alphabet base64 (the secret body), and
//   - `hmac_sha256`    — the RFC 2104 ipad/opad construction over SHA-256.
// Both are tiny, dependency-free, and exercised only by the GATED webhook
// journey, so the black-box "frozen deps" rule is preserved.

/// Decode standard-alphabet ('+' '/') base64, ignoring '=' padding and any
/// internal whitespace. Returns `None` on any non-alphabet byte so a malformed
/// secret GATES rather than producing a wrong key.
pub fn b64_decode_std(input: &str) -> Option<Vec<u8>> {
    const INVALID: u8 = 0xFF;
    let val = |c: u8| -> u8 {
        match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => INVALID,
        }
    };
    let mut out = Vec::with_capacity(input.len() / 4 * 3 + 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' || c == b' ' || c == b'\t' {
            continue;
        }
        let v = val(c);
        if v == INVALID {
            return None;
        }
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// HMAC-SHA256 (RFC 2104) over `(key, message)`, returning the 32 raw bytes.
/// Pure `sha2` — no `hmac` crate (frozen-dep rule).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64; // SHA-256 block size.
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        // Keys longer than the block are first hashed.
        let mut h = Sha256::new();
        h.update(key);
        let digest = h.finalize();
        block_key[..32].copy_from_slice(&digest);
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= block_key[i];
        opad[i] ^= block_key[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let out = outer.finalize();

    let mut result = [0u8; 32];
    result.copy_from_slice(&out);
    result
}

/// Build a valid `Stripe-Signature` header value (`t=<ts>,v1=<hexhmac>`) for a
/// `whsec_<base64>` secret over `"${ts}.${body}"` — the exact contract the
/// signup-worker `verifyStripeSignature` checks. Returns `None` if the secret is
/// not a decodable `whsec_…` value (→ the journey GATES, never sends a bad sig).
pub fn stripe_signature_header(secret: &str, timestamp_secs: u64, body: &str) -> Option<String> {
    let b64 = secret.strip_prefix("whsec_")?;
    let key = b64_decode_std(b64)?;
    let signed = format!("{timestamp_secs}.{body}");
    let mac = hmac_sha256(&key, signed.as_bytes());
    Some(format!("t={timestamp_secs},v1={}", hex::encode(mac)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_decode_matches_known_vectors() {
        // RFC 4648 examples.
        assert_eq!(b64_decode_std("Zg==").unwrap(), b"f");
        assert_eq!(b64_decode_std("Zm8=").unwrap(), b"fo");
        assert_eq!(b64_decode_std("Zm9v").unwrap(), b"foo");
        assert_eq!(b64_decode_std("Zm9vYg==").unwrap(), b"foob");
        // 32 zero bytes (the signup-worker test secret body).
        assert_eq!(
            b64_decode_std("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap(),
            vec![0u8; 32]
        );
        // Padding/whitespace tolerated; non-alphabet rejected.
        assert_eq!(b64_decode_std("Zm 9v").unwrap(), b"foo");
        assert!(b64_decode_std("not*base64").is_none());
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_test_case_2() {
        // RFC 4231 Test Case 2: key="Jefe", data="what do ya want for nothing?".
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex::encode(mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn stripe_signature_header_matches_worker_contract() {
        // Verified against the Python reference (hmac-sha256 over `${t}.${body}`
        // with the base64-decoded whsec body) — the exact bytes the signup-worker
        // `verifyStripeSignature` recomputes. This locks the cross-language
        // signature contract: if either the HMAC or the message framing drifts,
        // this fails BEFORE a live run silently 400s on every webhook.
        let secret = "whsec_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let header = stripe_signature_header(secret, 1_700_000_000, "{\"id\":\"evt_test\"}")
            .expect("canonical secret signs");
        assert_eq!(
            header,
            "t=1700000000,v1=8d580b797e256f17241de6d34c5506ffb7d3a156a5f266e321f649357325cf55"
        );
    }

    #[test]
    fn stripe_signature_header_rejects_non_whsec() {
        assert!(stripe_signature_header("sk_test_not_a_whsec", 1, "{}").is_none());
        assert!(stripe_signature_header("whsec_***bad***", 1, "{}").is_none());
    }
}
