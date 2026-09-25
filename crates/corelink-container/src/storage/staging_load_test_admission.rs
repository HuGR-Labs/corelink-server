//! Atomic, verifier-gated admission for staging load-test runs.
//!
//! This storage port consumes claims that a later credential verifier has
//! already authenticated. It does not authenticate tokens, construct routes,
//! invoke providers, or claim that a staging deployment has run.

use core::fmt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::{
    d1_http::{D1BatchStatement, D1HttpClient},
    staging_load_test_ownership::StagingLoadTestScenario,
};

const NONCE_DOMAIN: &[u8] = b"corelink/staging-load-admission-nonce/v1\0";
const MAX_CLAIM_LIFETIME_MS: i64 = 15 * 60 * 1_000;
const SQL_INSERT_RUN: &str = "INSERT INTO staging_load_test_runs \
    (run_id, scenario, target_environment, target_deployment_sha, state, admitted_at_ms) \
    VALUES (?1, ?2, 'staging', ?3, 'open', ?4)";
const SQL_INSERT_NONCE: &str = "INSERT INTO staging_load_test_admission_nonces \
    (nonce_digest, run_id, scenario, target_environment, target_deployment_sha, issued_at_ms, expires_at_ms, admitted_at_ms) \
    VALUES (?1, ?2, ?3, 'staging', ?4, ?5, ?6, ?7)";

/// Exact identity that a caller expects a previously verified claim to name.
#[derive(Debug)]
pub struct StagingLoadTestAdmissionExpectation<'a> {
    /// Canonical positive decimal GitHub Actions run ID.
    pub run_id: &'a str,
    /// Allowlisted scenario associated with this run.
    pub scenario: StagingLoadTestScenario,
    /// Target environment; only `staging` is accepted.
    pub target_environment: &'a str,
    /// Lowercase 40-character deployment commit SHA.
    pub target_deployment_sha: &'a str,
}

/// Credential-verified claim, constructible only by a verifier in this crate.
pub struct VerifiedStagingLoadTestAdmission {
    run_id: String,
    scenario: StagingLoadTestScenario,
    target_environment: String,
    target_deployment_sha: String,
    nonce: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
}

impl fmt::Debug for VerifiedStagingLoadTestAdmission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedStagingLoadTestAdmission")
            .field("run_id", &self.run_id)
            .field("scenario", &self.scenario)
            .field("target_environment", &self.target_environment)
            .field("target_deployment_sha", &self.target_deployment_sha)
            .field("nonce", &"[REDACTED]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl VerifiedStagingLoadTestAdmission {
    /// Create a claim after a separate verifier has authenticated its source.
    /// This stays crate-private until that verifier is implemented.
    #[allow(dead_code)]
    pub(crate) fn from_verified_claims(
        run_id: String,
        scenario: StagingLoadTestScenario,
        target_environment: String,
        target_deployment_sha: String,
        nonce: String,
        issued_at_ms: i64,
        expires_at_ms: i64,
    ) -> Self {
        Self {
            run_id,
            scenario,
            target_environment,
            target_deployment_sha,
            nonce,
            issued_at_ms,
            expires_at_ms,
        }
    }
}

/// Nonsecret result of a successful durable admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagingLoadTestAdmissionContext {
    /// Canonical positive decimal GitHub Actions run ID.
    pub run_id: String,
    /// Allowlisted scenario associated with this run.
    pub scenario: StagingLoadTestScenario,
    /// Admitted environment, always `staging`.
    pub target_environment: String,
    /// Lowercase 40-character deployment commit SHA.
    pub target_deployment_sha: String,
    /// Server timestamp at which the batch was submitted.
    pub admitted_at_ms: i64,
}

/// Fixed, parameter-free admission errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestAdmissionError {
    /// Run ID or deployment SHA is not canonical.
    InvalidIdentity,
    /// A non-staging environment was requested.
    NonStaging,
    /// The nonce is not exactly 32 bytes encoded as lowercase hex.
    InvalidNonce,
    /// The claim lifetime is malformed or exceeds 15 minutes.
    InvalidLifetime,
    /// The claim has expired or was issued in the future.
    Expired,
    /// The claim does not match the caller's expected identity.
    MismatchedIdentity,
    /// A run/scenario or nonce digest has already been consumed.
    AlreadyConsumed,
    /// The durable write or its response failed.
    PersistenceUnavailable,
}

impl fmt::Display for StagingLoadTestAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidIdentity => "staging admission identity is invalid",
            Self::NonStaging => "staging admission requires the staging environment",
            Self::InvalidNonce => "staging admission nonce is invalid",
            Self::InvalidLifetime => "staging admission lifetime is invalid",
            Self::Expired => "staging admission is expired or not yet valid",
            Self::MismatchedIdentity => "staging admission identity does not match expectation",
            Self::AlreadyConsumed => "staging admission was already consumed",
            Self::PersistenceUnavailable => "staging admission persistence is unavailable",
        };
        f.write_str(message)
    }
}

impl std::error::Error for StagingLoadTestAdmissionError {}

/// Narrow capability for consuming verified staging admissions in D1.
pub struct StagingLoadTestAdmissionStore {
    d1: D1HttpClient,
}

impl fmt::Debug for StagingLoadTestAdmissionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagingLoadTestAdmissionStore")
            .field("d1", &"[REDACTED]")
            .finish()
    }
}

impl StagingLoadTestAdmissionStore {
    /// Load the D1-only writable capability for this typed adapter.
    pub fn from_d1_env() -> Result<Self, StagingLoadTestAdmissionError> {
        let d1 = D1HttpClient::for_staging_load_test_ownership_writes()
            .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)?;
        Ok(Self { d1 })
    }

    /// Validate and consume one claim with a single all-or-nothing D1 batch.
    pub async fn consume_verified_admission(
        &self,
        expected: StagingLoadTestAdmissionExpectation<'_>,
        admission: VerifiedStagingLoadTestAdmission,
    ) -> Result<StagingLoadTestAdmissionContext, StagingLoadTestAdmissionError> {
        let now_ms = unix_time_ms()?;
        let nonce_bytes = validate_claim(&expected, &admission, now_ms)?;
        let nonce_digest = nonce_digest_hex(&nonce_bytes)?;
        let scenario = scenario_name(admission.scenario);
        let run_params = vec![
            json!(admission.run_id),
            json!(scenario),
            json!(admission.target_deployment_sha),
            json!(now_ms),
        ];
        let nonce_params = vec![
            json!(nonce_digest),
            json!(admission.run_id),
            json!(scenario),
            json!(admission.target_deployment_sha),
            json!(admission.issued_at_ms),
            json!(admission.expires_at_ms),
            json!(now_ms),
        ];
        self.d1
            .batch(vec![
                D1BatchStatement::new(SQL_INSERT_RUN, run_params),
                D1BatchStatement::new(SQL_INSERT_NONCE, nonce_params),
            ])
            .await
            .map_err(|error| {
                if error.statement.is_some() {
                    StagingLoadTestAdmissionError::AlreadyConsumed
                } else {
                    StagingLoadTestAdmissionError::PersistenceUnavailable
                }
            })?;

        Ok(StagingLoadTestAdmissionContext {
            run_id: admission.run_id,
            scenario: admission.scenario,
            target_environment: admission.target_environment,
            target_deployment_sha: admission.target_deployment_sha,
            admitted_at_ms: now_ms,
        })
    }
}

fn validate_claim(
    expected: &StagingLoadTestAdmissionExpectation<'_>,
    admission: &VerifiedStagingLoadTestAdmission,
    now_ms: i64,
) -> Result<[u8; 32], StagingLoadTestAdmissionError> {
    validate_identity(
        &admission.run_id,
        admission.scenario,
        &admission.target_deployment_sha,
    )?;
    if admission.target_environment != "staging" || expected.target_environment != "staging" {
        return Err(StagingLoadTestAdmissionError::NonStaging);
    }
    validate_identity(
        expected.run_id,
        expected.scenario,
        expected.target_deployment_sha,
    )?;
    if admission.run_id != expected.run_id
        || admission.scenario != expected.scenario
        || admission.target_environment != expected.target_environment
        || admission.target_deployment_sha != expected.target_deployment_sha
    {
        return Err(StagingLoadTestAdmissionError::MismatchedIdentity);
    }
    let nonce_bytes = decode_nonce(&admission.nonce)?;
    validate_lifetime(admission.issued_at_ms, admission.expires_at_ms)?;
    if admission.issued_at_ms > now_ms || now_ms >= admission.expires_at_ms {
        return Err(StagingLoadTestAdmissionError::Expired);
    }
    Ok(nonce_bytes)
}

fn validate_identity(
    run_id: &str,
    _scenario: StagingLoadTestScenario,
    target_deployment_sha: &str,
) -> Result<(), StagingLoadTestAdmissionError> {
    if run_id.is_empty()
        || run_id.len() > 20
        || run_id.starts_with('0')
        || !run_id.bytes().all(|byte| byte.is_ascii_digit())
        || target_deployment_sha.len() != 40
        || !target_deployment_sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StagingLoadTestAdmissionError::InvalidIdentity);
    }
    Ok(())
}

fn validate_lifetime(
    issued_at_ms: i64,
    expires_at_ms: i64,
) -> Result<(), StagingLoadTestAdmissionError> {
    let lifetime = expires_at_ms
        .checked_sub(issued_at_ms)
        .filter(|lifetime| *lifetime > 0 && *lifetime <= MAX_CLAIM_LIFETIME_MS);
    if issued_at_ms < 0 || expires_at_ms < 0 || lifetime.is_none() {
        return Err(StagingLoadTestAdmissionError::InvalidLifetime);
    }
    Ok(())
}

fn decode_nonce(nonce: &str) -> Result<[u8; 32], StagingLoadTestAdmissionError> {
    if nonce.len() != 64 {
        return Err(StagingLoadTestAdmissionError::InvalidNonce);
    }
    let mut bytes = [0_u8; 32];
    let mut input = nonce.bytes();
    for output in bytes.iter_mut() {
        let high = input.next().and_then(hex_nibble);
        let low = input.next().and_then(hex_nibble);
        let (Some(high), Some(low)) = (high, low) else {
            return Err(StagingLoadTestAdmissionError::InvalidNonce);
        };
        *output = (high << 4) | low;
    }
    if input.next().is_some() {
        return Err(StagingLoadTestAdmissionError::InvalidNonce);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn nonce_digest_hex(nonce_bytes: &[u8; 32]) -> Result<String, StagingLoadTestAdmissionError> {
    let mut hasher = Sha256::new();
    hasher.update(NONCE_DOMAIN);
    for byte in nonce_bytes.iter().copied() {
        hasher.update([byte]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}")
            .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)?;
    }
    Ok(encoded)
}

fn scenario_name(scenario: StagingLoadTestScenario) -> &'static str {
    match scenario {
        StagingLoadTestScenario::Signup => "signup",
        StagingLoadTestScenario::Webhook => "webhook",
        StagingLoadTestScenario::Dsr => "dsr",
        StagingLoadTestScenario::Cas => "cas",
        StagingLoadTestScenario::Byok => "byok",
        StagingLoadTestScenario::Endurance2h => "endurance-2h",
        StagingLoadTestScenario::B103CargoWrite => "b103-cargo-write",
    }
}

fn unix_time_ms() -> Result<i64, StagingLoadTestAdmissionError> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)?;
    i64::try_from(elapsed.as_millis())
        .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nonce() -> String {
        "0123456789abcdef".repeat(4)
    }

    #[test]
    fn nonce_parser_requires_canonical_lowercase_hex_without_indexing() {
        assert_eq!(
            decode_nonce(&nonce()),
            Ok([
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
                0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
                0x89, 0xab, 0xcd, 0xef
            ])
        );
        assert_eq!(
            decode_nonce(&"A".repeat(64)),
            Err(StagingLoadTestAdmissionError::InvalidNonce)
        );
        assert_eq!(
            decode_nonce(&"0".repeat(63)),
            Err(StagingLoadTestAdmissionError::InvalidNonce)
        );
    }

    #[test]
    fn digest_is_domain_separated_lowercase_sha256_hex() {
        let digest = nonce_digest_hex(&decode_nonce(&nonce()).expect("valid test nonce"))
            .expect("hex formatting is infallible for String");
        assert_eq!(digest.len(), 64);
        assert!(digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_eq!(
            digest,
            "1fff857caecccadff3a32ad2952ad385c0e9c40e165de7a86bc2a86f2ce4dfc2"
        );
    }

    #[test]
    fn validation_rejects_bad_lifetime_and_redacts_nonce() {
        assert_eq!(
            validate_lifetime(100, 100),
            Err(StagingLoadTestAdmissionError::InvalidLifetime)
        );
        assert_eq!(
            validate_lifetime(100, 100 + MAX_CLAIM_LIFETIME_MS + 1),
            Err(StagingLoadTestAdmissionError::InvalidLifetime)
        );
        let claim = VerifiedStagingLoadTestAdmission::from_verified_claims(
            "123".to_owned(),
            StagingLoadTestScenario::Cas,
            "staging".to_owned(),
            "a".repeat(40),
            nonce(),
            100,
            200,
        );
        let debug = format!("{claim:?}");
        let nonce = nonce();
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(nonce.as_str()));
    }

    #[test]
    fn verified_claim_validation_rejects_mismatch_future_expiry_and_nonstaging() {
        let expected = StagingLoadTestAdmissionExpectation {
            run_id: "123",
            scenario: StagingLoadTestScenario::Cas,
            target_environment: "staging",
            target_deployment_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        };
        let claim = |run_id: &str, environment: &str, issued: i64, expires: i64| {
            VerifiedStagingLoadTestAdmission::from_verified_claims(
                run_id.to_owned(),
                StagingLoadTestScenario::Cas,
                environment.to_owned(),
                "a".repeat(40),
                nonce(),
                issued,
                expires,
            )
        };
        let valid = claim("123", "staging", 100, 200);
        assert!(validate_claim(&expected, &valid, 150).is_ok());
        assert_eq!(
            validate_claim(&expected, &claim("124", "staging", 100, 200), 150),
            Err(StagingLoadTestAdmissionError::MismatchedIdentity),
        );
        let scenario_mismatch = VerifiedStagingLoadTestAdmission::from_verified_claims(
            "123".to_owned(),
            StagingLoadTestScenario::Signup,
            "staging".to_owned(),
            "a".repeat(40),
            nonce(),
            100,
            200,
        );
        assert_eq!(
            validate_claim(&expected, &scenario_mismatch, 150),
            Err(StagingLoadTestAdmissionError::MismatchedIdentity),
        );
        let sha_mismatch = VerifiedStagingLoadTestAdmission::from_verified_claims(
            "123".to_owned(),
            StagingLoadTestScenario::Cas,
            "staging".to_owned(),
            "b".repeat(40),
            nonce(),
            100,
            200,
        );
        assert_eq!(
            validate_claim(&expected, &sha_mismatch, 150),
            Err(StagingLoadTestAdmissionError::MismatchedIdentity),
        );
        assert_eq!(
            validate_claim(&expected, &claim("123", "production", 100, 200), 150),
            Err(StagingLoadTestAdmissionError::NonStaging),
        );
        assert_eq!(
            validate_claim(&expected, &claim("123", "staging", 151, 200), 150),
            Err(StagingLoadTestAdmissionError::Expired),
        );
        assert_eq!(
            validate_claim(&expected, &claim("123", "staging", 100, 150), 150),
            Err(StagingLoadTestAdmissionError::Expired),
        );
    }
}
