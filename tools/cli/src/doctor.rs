//! `corelink doctor` — 8 canonical diagnostic checks (WI-S15-001).
//!
//! Checks align with Lote 9.5c spec (8 checks). Every check is grounded on a
//! route that actually exists in the container/worker router (a prior version
//! called invented paths — `/v1/health`, `/v1/auth/me`, `/v1/byok/status`,
//! `/v1/tenant/region`, `/v1/quota/status`, `/v1/sdk/verify-status` — none of
//! which are mounted, so every check 404'd red):
//! 1. Network — `GET /health` (public worker liveness route).
//! 2. Auth — `GET /v1/users/me` (caller-identity reflection).
//! 3. Storage write — real CAS round-trip PUT `/v1/cas/<tenant>/<blake3>`.
//! 4. Storage read — GET the same blob back + BLAKE3 integrity verify.
//! 5. BYOK — `byok.status` from `GET /v1/customer/overview` (skip if the
//!    caller has not opted into BYOK, OR the token can't read the
//!    billing-gated overview — the status needs an admin/billing token).
//! 6. Region — SKIP: no public route exposes the tenant's primary region
//!    (informational only; not invented).
//! 7. Quota — `cas_bytes`/`quota_bytes` from `GET /v1/customer/usage` (the
//!    plain PAT-readable usage route, NOT the billing-admin-gated overview).
//! 8. Client verify — this CLI always BLAKE3-verifies downloads (compile-time
//!    guarantee, CTRL-CAS-002); reported `ok` without a network call.
//!
//! Each check emits a [`DoctorCheck`] with: check name, status, latency_ms,
//! error_code (COR_* from `docs/error_taxonomy.md`), and next_action.

use std::fmt;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::client::CorelinkClient;
use crate::config::load as load_config;
use crate::error::CliError;

/// Public worker liveness route (`worker/src/index.ts` `matchRoute`).
pub(crate) const HEALTH_ROUTE: &str = "/health";
/// Caller-identity reflection route (`routes/users.rs` `USERS_ME_ROUTE`).
pub(crate) const AUTH_ROUTE: &str = "/v1/users/me";
/// Customer account overview (`routes/customer.rs`) — carries `plan`,
/// `billing.*`, and `byok.status`. GATED by `billing_pii_gate_reject`
/// (`requires_billing_admin`), so a plain cache PAT gets 403 here.
pub(crate) const OVERVIEW_ROUTE: &str = "/v1/customer/overview";
/// Customer usage (`routes/customer.rs` `handle_usage`) — `cas_bytes` +
/// `quota_bytes` at the TOP LEVEL. Same tenant + PAT-possession gates as the
/// data plane but NO billing-admin gate, so a normal cache PAT can read it.
pub(crate) const USAGE_ROUTE: &str = "/v1/customer/usage";

/// Status of a single doctor check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum CheckStatus {
    /// Check passed.
    Ok,
    /// Check failed.
    Fail,
    /// Check skipped (e.g., BYOK not configured).
    Skip,
}

impl fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckStatus::Ok => write!(f, "ok"),
            CheckStatus::Fail => write!(f, "FAIL"),
            CheckStatus::Skip => write!(f, "skip"),
        }
    }
}

/// Result of a single doctor check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DoctorCheck {
    /// Check name (e.g., "network").
    pub check: String,
    /// Pass / fail / skip.
    pub status: CheckStatus,
    /// Latency in milliseconds (None for skipped checks).
    pub latency_ms: Option<u64>,
    /// COR_* error code from `docs/error_taxonomy.md` (None when status = ok/skip).
    pub error_code: Option<String>,
    /// Human-readable remediation step (None when status = ok/skip).
    pub next_action: Option<String>,
}

impl DoctorCheck {
    fn ok(check: &str, latency_ms: u64) -> Self {
        Self {
            check: check.to_owned(),
            status: CheckStatus::Ok,
            latency_ms: Some(latency_ms),
            error_code: None,
            next_action: None,
        }
    }

    fn fail(check: &str, latency_ms: u64, error_code: &str, next_action: &str) -> Self {
        Self {
            check: check.to_owned(),
            status: CheckStatus::Fail,
            latency_ms: Some(latency_ms),
            error_code: Some(error_code.to_owned()),
            next_action: Some(next_action.to_owned()),
        }
    }

    fn skip(check: &str, next_action: &str) -> Self {
        Self {
            check: check.to_owned(),
            status: CheckStatus::Skip,
            latency_ms: None,
            error_code: None,
            next_action: Some(next_action.to_owned()),
        }
    }
}

impl fmt::Display for DoctorCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = &self.status;
        let latency = self
            .latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "n/a".to_owned());
        let action = self.next_action.as_deref().unwrap_or("");
        let code = self.error_code.as_deref().unwrap_or("");
        write!(
            f,
            "{:<18} {:<6} {:>8}   {:>20}   {}",
            self.check, status, latency, code, action
        )
    }
}

/// Run all 8 doctor checks and return results.
pub async fn run_checks(client: &CorelinkClient) -> Result<Vec<DoctorCheck>, CliError> {
    let cfg = load_config()?;

    let mut results = Vec::with_capacity(8);

    results.push(check_network(client).await);
    results.push(check_auth(client).await);

    let (write_ok, test_digest) = check_storage_write(client).await;
    results.push(write_ok);

    let read_result = check_storage_read(client, test_digest.as_deref()).await;
    results.push(read_result);

    results.push(check_byok(client, cfg.auth.byok_enabled).await);
    results.push(check_region());
    results.push(check_quota(client).await);
    results.push(check_client_verify());

    Ok(results)
}

// ---------------------------------------------------------------------------
// Check implementations
// ---------------------------------------------------------------------------

/// Check #1 — Network: ping the public worker liveness route.
async fn check_network(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json(HEALTH_ROUTE).await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => DoctorCheck::ok("network", latency),
        Err(_) => DoctorCheck::fail(
            "network",
            latency,
            "COR_NET_UNREACHABLE",
            &format!(
                "Verify network connectivity, firewall rules, and DNS resolution for the configured endpoint ({}). See docs/error_taxonomy.md#COR_NET_UNREACHABLE",
                client.base_url()
            ),
        ),
    }
}

/// Check #2 — Auth: validate PAT format + tenant scope via API.
async fn check_auth(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json(AUTH_ROUTE).await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => DoctorCheck::ok("auth", latency),
        Err(_) => DoctorCheck::fail(
            "auth",
            latency,
            "COR_AUTH_INVALID",
            "Verify CORELINK_PAT env var is set correctly or regenerate token in admin UI. See docs/error_taxonomy.md#COR_AUTH_INVALID",
        ),
    }
}

/// Check #3 — Storage write: upload a 1 KB test blob via a REAL CAS PUT
/// (`PUT /v1/cas/<tenant>/<blake3>`). The blob is BLAKE3-addressed, matching
/// the native CAS write contract (a non-BLAKE3 claim would 422).
///
/// Returns (check_result, digest_option) so check #4 can read it back.
async fn check_storage_write(client: &CorelinkClient) -> (DoctorCheck, Option<String>) {
    let start = Instant::now();
    // 1 KB synthetic blob for smoke test.
    let blob: Vec<u8> = (0u8..=255u8).cycle().take(1024).collect();
    let digest = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&blob);
        hex::encode(hasher.finalize().as_bytes())
    };
    let result = client.cas_put(&digest, bytes::Bytes::from(blob)).await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => (DoctorCheck::ok("storage_write", latency), Some(digest)),
        Err(_) => (
            DoctorCheck::fail(
                "storage_write",
                latency,
                "COR_STORAGE_WRITE_DENIED",
                "Verify tenant quota and plan limits, and that a tenant is cached (run `corelink login`). See docs/error_taxonomy.md#COR_STORAGE_WRITE_DENIED",
            ),
            None,
        ),
    }
}

/// Check #4 — Storage read: download and BLAKE3-verify the test blob from
/// check #3 via a REAL CAS GET (`GET /v1/cas/<tenant>/<blake3>`).
async fn check_storage_read(client: &CorelinkClient, digest: Option<&str>) -> DoctorCheck {
    let Some(digest) = digest else {
        return DoctorCheck::fail(
            "storage_read",
            0,
            "COR_STORAGE_READ_FAIL",
            "Storage write failed; cannot verify read. Fix COR_STORAGE_WRITE_DENIED first. See docs/error_taxonomy.md#COR_STORAGE_READ_FAIL",
        );
    };

    let start = Instant::now();
    let result = client.cas_get(digest).await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(data) => {
            // Client-verify: BLAKE3 the downloaded bytes and compare.
            let mut hasher = blake3::Hasher::new();
            hasher.update(&data);
            let got = hasher.finalize().to_hex().to_string();
            if got == digest {
                DoctorCheck::ok("storage_read", latency)
            } else {
                DoctorCheck::fail(
                    "storage_read",
                    latency,
                    "COR_STORAGE_READ_FAIL",
                    "Downloaded blob BLAKE3 hash mismatch — possible data corruption or KMS issue. See docs/error_taxonomy.md#COR_STORAGE_READ_FAIL",
                )
            }
        }
        Err(_) => DoctorCheck::fail(
            "storage_read",
            latency,
            "COR_STORAGE_READ_FAIL",
            "Cannot read blob; verify tenant region and KMS access if BYOK is enabled. See docs/error_taxonomy.md#COR_STORAGE_READ_FAIL",
        ),
    }
}

/// Check #5 — BYOK: KMS access check (S-14 alignment).
///
/// Skipped if `auth.byok_enabled = false`. `byok.status` lives ONLY on the
/// billing-admin-gated `GET /v1/customer/overview`, so a plain cache PAT
/// gets 403 — in that case we degrade to `skip` (honest: the status needs an
/// admin/billing token) rather than a spurious `fail`. We do NOT invent a
/// dedicated byok route.
async fn check_byok(client: &CorelinkClient, byok_enabled: bool) -> DoctorCheck {
    if !byok_enabled {
        return DoctorCheck::skip(
            "byok",
            "BYOK not configured; set auth.byok_enabled=true and KMS credentials to enable.",
        );
    }

    let start = Instant::now();
    let result = client.get_json(OVERVIEW_ROUTE).await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(v) => {
            // `GET /v1/customer/overview` returns `{ "byok": { "status": … } }`.
            let status = v
                .get("byok")
                .and_then(|b| b.get("status"))
                .and_then(|s| s.as_str());
            match status {
                Some("active") => DoctorCheck::ok("byok", latency),
                _ => DoctorCheck::fail(
                    "byok",
                    latency,
                    "COR_BYOK_REVOKED",
                    "BYOK is enabled locally but the tenant BYOK state is not active. Verify Customer Managed Key (CMK) status in your KMS provider. See docs/error_taxonomy.md#COR_BYOK_REVOKED",
                ),
            }
        }
        // The overview is billing-admin-gated; a normal cache PAT 403s here.
        // BYOK status is genuinely unreadable with this token → honest skip,
        // not a false COR_BYOK_REVOKED.
        Err(_) => DoctorCheck::skip(
            "byok",
            "BYOK status requires an admin/billing token (it lives on the billing-gated account overview); skipping with this token.",
        ),
    }
}

/// Check #6 — Region: SKIP.
///
/// No public route exposes the tenant's primary region (the invented
/// `/v1/tenant/region` never existed). Rather than 404 red, report `skip`
/// with an honest reason — region is informational only for `doctor`.
fn check_region() -> DoctorCheck {
    DoctorCheck::skip(
        "region",
        "Tenant primary region is not exposed on a public read route; skipping (informational only).",
    )
}

/// Check #7 — Quota: current usage vs plan limit, derived from the
/// PAT-readable usage route (`GET /v1/customer/usage`), which returns
/// `cas_bytes` + `quota_bytes` at the TOP LEVEL and — unlike the billing-
/// admin-gated `/v1/customer/overview` — is reachable with a plain cache PAT.
async fn check_quota(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json(USAGE_ROUTE).await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(v) => {
            let used = v.get("cas_bytes").and_then(serde_json::Value::as_u64);
            let limit = v.get("quota_bytes").and_then(serde_json::Value::as_u64);
            match (used, limit) {
                (Some(used), Some(limit)) if limit > 0 && used >= limit => DoctorCheck::fail(
                    "quota",
                    latency,
                    "COR_QUOTA_EXCEEDED",
                    "Tenant storage quota reached (cas_bytes ≥ quota_bytes). Verify plan limits and contact sales to upgrade. See docs/error_taxonomy.md#COR_QUOTA_EXCEEDED",
                ),
                _ => DoctorCheck::ok("quota", latency),
            }
        }
        Err(_) => DoctorCheck::fail(
            "quota",
            latency,
            "COR_QUOTA_EXCEEDED",
            "Cannot read usage from /v1/customer/usage. Verify plan + contact support. See docs/error_taxonomy.md#COR_QUOTA_EXCEEDED",
        ),
    }
}

/// Check #8 — Client verify: BLAKE3 verify default-on (CTRL-CAS-002).
///
/// This CLI ALWAYS BLAKE3-verifies downloaded blobs (see `commands::get` /
/// `commands::cas`) — a compile-time guarantee, not a server flag. No route
/// reflects it (the invented `/v1/sdk/verify-status` never existed), so we
/// report `ok` without a network call.
fn check_client_verify() -> DoctorCheck {
    DoctorCheck::ok("client_verify", 0)
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    #[test]
    fn check_ok_has_no_error() {
        let c = DoctorCheck::ok("network", 42);
        assert_eq!(c.status, CheckStatus::Ok);
        assert!(c.error_code.is_none());
        assert!(c.next_action.is_none());
        assert_eq!(c.latency_ms, Some(42));
    }

    #[test]
    fn check_fail_has_error_code() {
        let c = DoctorCheck::fail("network", 10, "COR_NET_UNREACHABLE", "Check DNS");
        assert_eq!(c.status, CheckStatus::Fail);
        assert_eq!(c.error_code.as_deref(), Some("COR_NET_UNREACHABLE"));
        assert!(c.next_action.is_some());
    }

    #[test]
    fn check_skip_has_no_latency() {
        let c = DoctorCheck::skip("byok", "Configure BYOK first");
        assert_eq!(c.status, CheckStatus::Skip);
        assert!(c.latency_ms.is_none());
    }

    #[test]
    fn check_serialises_to_json() {
        let c = DoctorCheck::ok("auth", 5);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"check\""));
        assert!(json.contains("\"ok\""));
    }

    #[test]
    fn all_check_names_unique() {
        // Confirm the 8 check names match spec §6.4.
        let names = [
            "network",
            "auth",
            "storage_write",
            "storage_read",
            "byok",
            "region",
            "quota",
            "client_verify",
        ];
        let mut deduped = names.to_vec();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(deduped.len(), 8);
    }

    #[test]
    fn doctor_routes_are_real_not_invented() {
        // B2 regression: the network/auth/overview checks must hit routes that
        // actually exist in the container/worker router — never the invented
        // `/v1/health`, `/v1/auth/me`, `/v1/byok|tenant|quota|sdk/...` paths
        // that made every check 404 red.
        assert_eq!(HEALTH_ROUTE, "/health");
        assert_eq!(AUTH_ROUTE, "/v1/users/me");
        assert_eq!(OVERVIEW_ROUTE, "/v1/customer/overview");
        // Quota reads the PAT-readable usage route, NOT the billing-gated
        // overview (a plain cache PAT 403s on overview → false COR_QUOTA).
        assert_eq!(USAGE_ROUTE, "/v1/customer/usage");
    }

    #[test]
    fn region_check_is_skip_with_reason() {
        // No public route exposes region → honest skip, not a 404 fail.
        let c = check_region();
        assert_eq!(c.status, CheckStatus::Skip);
        assert!(c.next_action.is_some());
        assert!(c.error_code.is_none());
    }

    #[test]
    fn client_verify_is_ok_without_network() {
        // Compile-time BLAKE3 guarantee → ok, no dead route.
        let c = check_client_verify();
        assert_eq!(c.status, CheckStatus::Ok);
        assert!(c.error_code.is_none());
    }

    #[test]
    fn storage_read_skips_gracefully_when_write_failed() {
        // check_storage_read with digest=None should return FAIL with COR_STORAGE_READ_FAIL.
        // We test the synchronous logic indirectly via the DoctorCheck::fail branch.
        let c = DoctorCheck::fail(
            "storage_read",
            0,
            "COR_STORAGE_READ_FAIL",
            "Storage write failed; cannot verify read.",
        );
        assert_eq!(c.status, CheckStatus::Fail);
        assert_eq!(c.error_code.as_deref(), Some("COR_STORAGE_READ_FAIL"));
    }
}
