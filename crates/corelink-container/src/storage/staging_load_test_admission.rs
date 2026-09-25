//! Authenticated, staging-only admission context for #2541.
//!
//! This module verifies a domain-separated HMAC credential before exposing an
//! immutable run context. A raw request header is never an admission context.

use hmac::{Hmac, Mac};
use sha2::Sha256;

const DOMAIN: &[u8] = b"corelink-staging-load-admission-v1\0";
const MAX_TTL_MS: u64 = 15 * 60 * 1000;

type HmacSha256 = Hmac<Sha256>;

/// Immutable identity admitted for one staging load-test run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagingLoadTestAdmission {
    run_id: String,
    scenario: String,
    deployment_sha: String,
}

impl StagingLoadTestAdmission {
    /// Canonical positive GitHub Actions run ID.
    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    /// Allowlisted load-test scenario.
    #[must_use]
    pub fn scenario(&self) -> &str {
        &self.scenario
    }
    /// Lowercase 40-hex staging deployment SHA.
    #[must_use]
    pub fn deployment_sha(&self) -> &str {
        &self.deployment_sha
    }
}

/// HMAC verifier holding the staging-only credential outside `Debug` output.
pub struct StagingLoadTestAdmissionVerifier {
    key: Vec<u8>,
}

impl core::fmt::Debug for StagingLoadTestAdmissionVerifier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StagingLoadTestAdmissionVerifier")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl StagingLoadTestAdmissionVerifier {
    /// Build only for a process explicitly configured as staging.
    pub fn new(runtime_environment: &str, key: impl AsRef<[u8]>) -> Result<Self, String> {
        if runtime_environment != "staging" {
            return Err("staging load admission is disabled outside staging".to_owned());
        }
        let key = key.as_ref();
        if key.len() < 32 {
            return Err("staging load admission key must be at least 32 bytes".to_owned());
        }
        Ok(Self { key: key.to_vec() })
    }

    /// Load the staging-only verifier from process configuration.
    pub fn from_env() -> Result<Self, String> {
        let environment = std::env::var("CORELINK_ENVIRONMENT").unwrap_or_default();
        let key = std::env::var("CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY")
            .map_err(|_| "CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY is required".to_owned())?;
        Self::new(&environment, key.as_bytes())
    }

    /// Verify `v1.run_id.scenario.sha.issued_ms.expires_ms.nonce.tag_hex` at `now_ms`.
    pub fn verify(
        &self,
        credential: &str,
        now_ms: u64,
    ) -> Result<StagingLoadTestAdmission, String> {
        let parts: Vec<&str> = credential.split('.').collect();
        if parts.len() != 8 || parts[0] != "v1" {
            return Err("invalid staging admission credential".to_owned());
        }
        let (run_id, scenario, sha, issued, expires, nonce, tag) = (
            parts[1], parts[2], parts[3], parts[4], parts[5], parts[6], parts[7],
        );
        validate_identity(run_id, scenario, sha)?;
        if nonce.len() != 32 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid staging admission credential".to_owned());
        }
        let issued = issued
            .parse::<u64>()
            .map_err(|_| "invalid staging admission credential".to_owned())?;
        let expires = expires
            .parse::<u64>()
            .map_err(|_| "invalid staging admission credential".to_owned())?;
        if issued > now_ms
            || expires <= now_ms
            || expires <= issued
            || expires - issued > MAX_TTL_MS
        {
            return Err("staging admission expired or invalid".to_owned());
        }
        let tag =
            hex::decode(tag).map_err(|_| "invalid staging admission credential".to_owned())?;
        if tag.len() != 32 {
            return Err("invalid staging admission credential".to_owned());
        }
        let signed = parts[..7].join(".");
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| "invalid staging admission key".to_owned())?;
        mac.update(DOMAIN);
        mac.update(signed.as_bytes());
        mac.verify_slice(&tag)
            .map_err(|_| "invalid staging admission credential".to_owned())?;
        Ok(StagingLoadTestAdmission {
            run_id: run_id.to_owned(),
            scenario: scenario.to_owned(),
            deployment_sha: sha.to_owned(),
        })
    }

    #[cfg(test)]
    fn sign_for_test(
        &self,
        run_id: &str,
        scenario: &str,
        sha: &str,
        issued: u64,
        expires: u64,
        nonce: &str,
    ) -> String {
        let signed = format!("v1.{run_id}.{scenario}.{sha}.{issued}.{expires}.{nonce}");
        let mut mac = HmacSha256::new_from_slice(&self.key).unwrap();
        mac.update(DOMAIN);
        mac.update(signed.as_bytes());
        format!("{signed}.{}", hex::encode(mac.finalize().into_bytes()))
    }
}

fn validate_identity(run_id: &str, scenario: &str, sha: &str) -> Result<(), String> {
    if run_id.is_empty()
        || run_id.len() > 20
        || run_id.starts_with('0')
        || !run_id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("invalid staging admission identity".to_owned());
    }
    if !matches!(
        scenario,
        "signup" | "webhook" | "dsr" | "cas" | "byok" | "endurance-2h" | "b103-cargo-write"
    ) {
        return Err("invalid staging admission identity".to_owned());
    }
    if sha.len() != 40
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid staging admission identity".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fn verifier() -> StagingLoadTestAdmissionVerifier {
        StagingLoadTestAdmissionVerifier::new("staging", b"01234567890123456789012345678901")
            .unwrap()
    }
    #[test]
    fn accepts_exact_signed_staging_identity() {
        let v = verifier();
        let c = v.sign_for_test(
            "123",
            "webhook",
            SHA,
            100,
            200,
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(v.verify(&c, 150).unwrap().scenario(), "webhook");
    }
    #[test]
    fn rejects_forged_expired_or_rebound_credentials() {
        let v = verifier();
        let c = v.sign_for_test(
            "123",
            "webhook",
            SHA,
            100,
            200,
            "0123456789abcdef0123456789abcdef",
        );
        assert!(v.verify(&c.replace("webhook", "cas"), 150).is_err());
        assert!(v.verify(&c, 200).is_err());
    }
    #[test]
    fn rejects_non_staging_and_redacts_key() {
        assert!(StagingLoadTestAdmissionVerifier::new(
            "production",
            b"01234567890123456789012345678901"
        )
        .is_err());
        assert!(!format!("{:?}", verifier()).contains("012345"));
    }
}
