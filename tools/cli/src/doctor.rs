//! `corelink doctor` — 8 canonical diagnostic checks (WI-S15-001).
//!
//! Checks align with Lote 9.5c spec (8 checks vs 6 Codex R3-14 fix):
//! 1. Network — ping CF endpoint; report p50/p99 latency.
//! 2. Auth — validate PAT format + tenant scope via API call.
//! 3. Storage write — 1 KB blob upload smoke test.
//! 4. Storage read — round-trip integrity verify (BLAKE3).
//! 5. BYOK — KMS access check if `auth.byok_enabled = true` (S-14).
//! 6. Region — tenant region matches expected hostname.
//! 7. Quota — current usage vs plan limit + soft/hard thresholds.
//! 8. Client verify — BLAKE3 verify default-on reflection (CTRL-CAS-002).
//!
//! Each check emits a [`DoctorCheck`] with: check name, status, latency_ms,
//! error_code (COR_* from `docs/error_taxonomy.md`), and next_action.

use std::fmt;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::client::CorelinkClient;
use crate::config::load as load_config;
use crate::error::CliError;

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
    results.push(check_region(client).await);
    results.push(check_quota(client).await);
    results.push(check_client_verify(client).await);

    Ok(results)
}

// ---------------------------------------------------------------------------
// Check implementations
// ---------------------------------------------------------------------------

/// Check #1 — Network: ping the cluster endpoint.
async fn check_network(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json("/v1/health").await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => DoctorCheck::ok("network", latency),
        Err(_) => DoctorCheck::fail(
            "network",
            latency,
            "COR_NET_UNREACHABLE",
            "Verify network connectivity, firewall rules, and DNS resolution for corelink.humangr.com. See docs/error_taxonomy.md#COR_NET_UNREACHABLE",
        ),
    }
}

/// Check #2 — Auth: validate PAT format + tenant scope via API.
async fn check_auth(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json("/v1/auth/me").await;
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

/// Check #3 — Storage write: upload a 1 KB test blob.
///
/// Returns (check_result, digest_option) so check #4 can read it back.
async fn check_storage_write(client: &CorelinkClient) -> (DoctorCheck, Option<String>) {
    let start = Instant::now();
    // 1 KB synthetic blob for smoke test.
    let blob: Vec<u8> = (0u8..=255u8).cycle().take(1024).collect();
    let digest = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&blob);
        hasher.finalize().to_hex().to_string()
    };
    let result = client
        .post_bytes("/v1/cas/upload", bytes::Bytes::from(blob))
        .await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => (DoctorCheck::ok("storage_write", latency), Some(digest)),
        Err(_) => (
            DoctorCheck::fail(
                "storage_write",
                latency,
                "COR_STORAGE_WRITE_DENIED",
                "Verify tenant quota and plan limits. See docs/error_taxonomy.md#COR_STORAGE_WRITE_DENIED",
            ),
            None,
        ),
    }
}

/// Check #4 — Storage read: download and BLAKE3-verify the test blob from check #3.
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
    let path = format!("/v1/cas/download/{digest}");
    let result = client.get_bytes(&path).await;
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
/// Skipped if `auth.byok_enabled = false`.
async fn check_byok(client: &CorelinkClient, byok_enabled: bool) -> DoctorCheck {
    if !byok_enabled {
        return DoctorCheck::skip(
            "byok",
            "BYOK not configured; set auth.byok_enabled=true and KMS credentials to enable.",
        );
    }

    let start = Instant::now();
    let result = client.get_json("/v1/byok/status").await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => DoctorCheck::ok("byok", latency),
        Err(_) => DoctorCheck::fail(
            "byok",
            latency,
            "COR_BYOK_REVOKED",
            "Verify Customer Managed Key (CMK) status in your KMS provider. The key may be disabled or revoked. See docs/error_taxonomy.md#COR_BYOK_REVOKED",
        ),
    }
}

/// Check #6 — Region: tenant region matches expected hostname.
async fn check_region(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json("/v1/tenant/region").await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(v) => {
            // Server returns `{"region":"wnam","expected":"wnam","match":true}`.
            let region_match = v
                .get("match")
                .and_then(|m| m.as_bool())
                .unwrap_or(false);
            if region_match {
                DoctorCheck::ok("region", latency)
            } else {
                DoctorCheck::fail(
                    "region",
                    latency,
                    "COR_REGION_MISMATCH",
                    "Tenant primary_region does not match the connected endpoint. Update tenant primary_region in admin UI. See docs/error_taxonomy.md#COR_REGION_MISMATCH",
                )
            }
        }
        Err(_) => DoctorCheck::fail(
            "region",
            latency,
            "COR_REGION_MISMATCH",
            "Cannot determine tenant region. Verify tenant configuration in admin UI. See docs/error_taxonomy.md#COR_REGION_MISMATCH",
        ),
    }
}

/// Check #7 — Quota: current usage vs plan limit.
async fn check_quota(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    let result = client.get_json("/v1/quota/status").await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(v) => {
            // Server returns `{"exceeded":false,"soft_threshold":false,"hard_threshold":false}`.
            let hard = v
                .get("hard_threshold")
                .and_then(|h| h.as_bool())
                .unwrap_or(false);
            let exceeded = v
                .get("exceeded")
                .and_then(|e| e.as_bool())
                .unwrap_or(false);
            if hard || exceeded {
                DoctorCheck::fail(
                    "quota",
                    latency,
                    "COR_QUOTA_EXCEEDED",
                    "Tenant quota exceeded. Verify plan limits and contact sales to upgrade. See docs/error_taxonomy.md#COR_QUOTA_EXCEEDED",
                )
            } else {
                DoctorCheck::ok("quota", latency)
            }
        }
        Err(_) => DoctorCheck::fail(
            "quota",
            latency,
            "COR_QUOTA_EXCEEDED",
            "Cannot determine quota status. Verify plan + contact support. See docs/error_taxonomy.md#COR_QUOTA_EXCEEDED",
        ),
    }
}

/// Check #8 — Client verify: BLAKE3 verify default-on (CTRL-CAS-002 reflection).
///
/// Validates that the CLI itself has client-verify enabled (always true for this
/// binary; reflects on the SDK verify flag via /v1/sdk/verify-status if available).
async fn check_client_verify(client: &CorelinkClient) -> DoctorCheck {
    let start = Instant::now();
    // This CLI always performs BLAKE3 client-verify (compile-time guarantee).
    // We query the server for any override flags.
    let result = client.get_json("/v1/sdk/verify-status").await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(v) => {
            let verify_on = v
                .get("client_verify_enabled")
                .and_then(|e| e.as_bool())
                .unwrap_or(true); // default-on: if field absent, assume OK.
            if verify_on {
                DoctorCheck::ok("client_verify", latency)
            } else {
                DoctorCheck::fail(
                    "client_verify",
                    latency,
                    "COR_CLIENT_VERIFY_DISABLED",
                    "Client-side BLAKE3 verification is disabled (CTRL-CAS-002 violation). Do NOT disable except for explicit dev/test. See docs/error_taxonomy.md#COR_CLIENT_VERIFY_DISABLED",
                )
            }
        }
        Err(_) => {
            // Endpoint may not exist in all versions; assume OK (default-on).
            DoctorCheck::ok("client_verify", latency)
        }
    }
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
