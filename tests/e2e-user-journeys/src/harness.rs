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

/// Lowercase-hex SHA-256 of `bytes` (the CAS content address).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
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
