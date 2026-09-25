//! Durable one-use consumption for previously authenticated staging admission.
//!
//! This module deliberately does not authenticate credentials or integrate a
//! route/writer. The crate-private claim constructor is reserved for that
//! verifier; this store only validates its exact identity and consumes its
//! nonce atomically with the migration-0147 run row.

use core::fmt;

use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    d1_http::{D1BatchStatement, D1HttpClient},
    staging_load_test_ownership::StagingLoadTestScenario,
};

const MAX_LIFETIME_MS: i64 = 15 * 60 * 1000;
const NONCE_DOMAIN: &[u8] = b"corelink/staging-load-admission-nonce/v1\0";
const SQL_INSERT_RUN: &str = "INSERT INTO staging_load_test_runs \
    (run_id, scenario, target_environment, target_deployment_sha, state, admitted_at_ms) \
    VALUES (?1, ?2, 'staging', ?3, 'open', ?4)";
const SQL_INSERT_NONCE: &str = "INSERT INTO staging_load_test_admission_nonces \
    (nonce_digest, run_id, scenario, issued_at_ms, expires_at_ms, consumed_at_ms) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

/// Exact identity expected by the later trusted admission consumer.
pub struct StagingLoadTestAdmissionExpectation<'a> {
    /// Canonical positive decimal GitHub Actions run ID.
    pub run_id: &'a str,
    /// Scenario in the migration-0147 allowlist.
    pub scenario: StagingLoadTestScenario,
    /// Must be exactly `staging`.
    pub target_environment: &'a str,
    /// Lowercase 40-hex deployment commit.
    pub target_deployment_sha: &'a str,
}

/// Claims produced only after a future credential verifier succeeds.
///
/// The nonce is intentionally absent from `Debug` and all public accessors.
pub struct VerifiedStagingLoadTestAdmission {
    run_id: String,
    scenario: StagingLoadTestScenario,
    target_environment: String,
    target_deployment_sha: String,
    nonce: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
}

impl VerifiedStagingLoadTestAdmission {
    /// Construct claims after a trusted credential verifier has authenticated
    /// their complete payload. This is crate-private so raw client input cannot
    /// create an admission claim.
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

/// Secret-free identity returned after durable consumption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagingLoadTestAdmissionContext {
    /// Canonical GitHub Actions run ID.
    pub run_id: String,
    /// Admitted scenario.
    pub scenario: StagingLoadTestScenario,
    /// Always `staging`.
    pub target_environment: String,
    /// Bound deployment commit.
    pub target_deployment_sha: String,
    /// Server time at durable admission.
    pub admitted_at_ms: i64,
}

/// Stable parameter-free admission failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestAdmissionError {
    /// Run ID or deployment SHA has a noncanonical form.
    InvalidIdentity,
    /// The target environment is not staging.
    NonStaging,
    /// Nonce is not 32 bytes encoded as lowercase hexadecimal.
    InvalidNonce,
    /// Claim timestamps are negative, reversed, too long, or future-issued.
    InvalidLifetime,
    /// Claim has expired.
    Expired,
    /// Claim identity differs from the expected identity.
    MismatchedIdentity,
    /// D1 rejected an insert because this run or nonce was already consumed.
    AlreadyConsumed,
    /// D1 or the local clock was unavailable.
    PersistenceUnavailable,
}

impl fmt::Display for StagingLoadTestAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidIdentity => "invalid staging admission identity",
            Self::NonStaging => "staging admission requires staging environment",
            Self::InvalidNonce => "invalid staging admission nonce",
            Self::InvalidLifetime => "invalid staging admission lifetime",
            Self::Expired => "staging admission has expired",
            Self::MismatchedIdentity => "staging admission identity mismatch",
            Self::AlreadyConsumed => "staging admission was already consumed",
            Self::PersistenceUnavailable => "staging admission persistence unavailable",
        })
    }
}

impl std::error::Error for StagingLoadTestAdmissionError {}

/// Restricted D1 capability for atomic, single-use staging admission.
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
    /// Load only the D1 capability required by this store.
    pub fn from_d1_env() -> Result<Self, StagingLoadTestAdmissionError> {
        Ok(Self {
            d1: D1HttpClient::for_staging_load_test_ownership_writes()
                .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)?,
        })
    }

    /// Consume a previously verified claim once, atomically creating the run
    /// row and nonce digest row in the D1 batch transaction.
    pub async fn consume_verified_admission(
        &self,
        expected: StagingLoadTestAdmissionExpectation<'_>,
        admission: VerifiedStagingLoadTestAdmission,
    ) -> Result<StagingLoadTestAdmissionContext, StagingLoadTestAdmissionError> {
        let now_ms = unix_time_ms()?;
        validate_claim(&expected, &admission, now_ms)?;
        let scenario = scenario_as_str(admission.scenario);
        let nonce_digest = nonce_digest(&admission.nonce)?;
        let batch = vec![
            D1BatchStatement::new(
                SQL_INSERT_RUN,
                vec![
                    json!(admission.run_id),
                    json!(scenario),
                    json!(admission.target_deployment_sha),
                    json!(now_ms),
                ],
            ),
            D1BatchStatement::new(
                SQL_INSERT_NONCE,
                vec![
                    json!(nonce_digest),
                    json!(admission.run_id),
                    json!(scenario),
                    json!(admission.issued_at_ms),
                    json!(admission.expires_at_ms),
                    json!(now_ms),
                ],
            ),
        ];
        self.d1.batch(batch).await.map_err(|error| {
            let detail = error.message.to_ascii_lowercase();
            if error.statement.is_some()
                && (detail.contains("unique constraint failed")
                    || detail.contains("constraint failed"))
            {
                StagingLoadTestAdmissionError::AlreadyConsumed
            } else {
                StagingLoadTestAdmissionError::PersistenceUnavailable
            }
        })?;
        Ok(StagingLoadTestAdmissionContext {
            run_id: admission.run_id,
            scenario: admission.scenario,
            target_environment: "staging".to_owned(),
            target_deployment_sha: admission.target_deployment_sha,
            admitted_at_ms: now_ms,
        })
    }
}

fn unix_time_ms() -> Result<i64, StagingLoadTestAdmissionError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)?
        .as_millis();
    i64::try_from(millis).map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)
}

fn validate_claim(
    expected: &StagingLoadTestAdmissionExpectation<'_>,
    claim: &VerifiedStagingLoadTestAdmission,
    now_ms: i64,
) -> Result<(), StagingLoadTestAdmissionError> {
    if expected.target_environment != "staging" || claim.target_environment != "staging" {
        return Err(StagingLoadTestAdmissionError::NonStaging);
    }
    if !valid_run_id(expected.run_id)
        || !valid_run_id(&claim.run_id)
        || !valid_sha(expected.target_deployment_sha)
        || !valid_sha(&claim.target_deployment_sha)
    {
        return Err(StagingLoadTestAdmissionError::InvalidIdentity);
    }
    if expected.run_id != claim.run_id
        || expected.scenario != claim.scenario
        || expected.target_deployment_sha != claim.target_deployment_sha
    {
        return Err(StagingLoadTestAdmissionError::MismatchedIdentity);
    }
    if claim.issued_at_ms < 0
        || claim.expires_at_ms <= claim.issued_at_ms
        || claim.expires_at_ms - claim.issued_at_ms > MAX_LIFETIME_MS
        || claim.issued_at_ms > now_ms
    {
        return Err(StagingLoadTestAdmissionError::InvalidLifetime);
    }
    if now_ms >= claim.expires_at_ms {
        return Err(StagingLoadTestAdmissionError::Expired);
    }
    let _ = nonce_digest(&claim.nonce)?;
    Ok(())
}

fn valid_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= 20
        && !run_id.starts_with('0')
        && run_id.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_sha(sha: &str) -> bool {
    sha.len() == 40
        && sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn nonce_digest(nonce: &str) -> Result<String, StagingLoadTestAdmissionError> {
    if nonce.len() != 64
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StagingLoadTestAdmissionError::InvalidNonce);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in nonce.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or(StagingLoadTestAdmissionError::InvalidNonce)?;
        let low = hex_nibble(pair[1]).ok_or(StagingLoadTestAdmissionError::InvalidNonce)?;
        bytes[index] = (high << 4) | low;
    }
    let mut digest = Sha256::new();
    digest.update(NONCE_DOMAIN);
    digest.update(bytes);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest.finalize() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(encoded)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

const fn scenario_as_str(scenario: StagingLoadTestScenario) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expectation() -> StagingLoadTestAdmissionExpectation<'static> {
        StagingLoadTestAdmissionExpectation {
            run_id: "123456",
            scenario: StagingLoadTestScenario::Signup,
            target_environment: "staging",
            target_deployment_sha: "0123456789abcdef0123456789abcdef01234567",
        }
    }

    fn claim() -> VerifiedStagingLoadTestAdmission {
        VerifiedStagingLoadTestAdmission::from_verified_claims(
            "123456".to_owned(),
            StagingLoadTestScenario::Signup,
            "staging".to_owned(),
            "0123456789abcdef0123456789abcdef01234567".to_owned(),
            "ab".repeat(32),
            1_000,
            2_000,
        )
    }

    #[test]
    fn accepts_exact_unexpired_staging_claim_and_hashes_nonce() {
        let admission = claim();
        assert_eq!(validate_claim(&expectation(), &admission, 1_500), Ok(()));
        let digest = nonce_digest(&admission.nonce).expect("valid nonce");
        assert_eq!(digest.len(), 64);
        assert!(digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_ne!(digest, admission.nonce);
    }

    #[test]
    fn rejects_identity_environment_nonce_time_and_replay_inputs() {
        let mut non_staging = expectation();
        non_staging.target_environment = "production";
        assert_eq!(
            validate_claim(&non_staging, &claim(), 1_500),
            Err(StagingLoadTestAdmissionError::NonStaging)
        );

        let mut mismatch = expectation();
        mismatch.run_id = "123457";
        assert_eq!(
            validate_claim(&mismatch, &claim(), 1_500),
            Err(StagingLoadTestAdmissionError::MismatchedIdentity)
        );
        assert_eq!(
            validate_claim(&expectation(), &claim(), 2_000),
            Err(StagingLoadTestAdmissionError::Expired)
        );

        let mut future = claim();
        future.issued_at_ms = 1_501;
        future.expires_at_ms = 2_000;
        assert_eq!(
            validate_claim(&expectation(), &future, 1_500),
            Err(StagingLoadTestAdmissionError::InvalidLifetime)
        );

        let mut malformed = claim();
        malformed.nonce = "A".repeat(64);
        assert_eq!(
            validate_claim(&expectation(), &malformed, 1_500),
            Err(StagingLoadTestAdmissionError::InvalidNonce)
        );
    }

    #[test]
    fn debug_and_display_redact_nonce_and_are_stable() {
        let admission = claim();
        let debug = format!("{admission:?}");
        assert!(!debug.contains(&admission.nonce));
        assert!(debug.contains("[REDACTED]"));
        assert_eq!(
            StagingLoadTestAdmissionError::AlreadyConsumed.to_string(),
            "staging admission was already consumed"
        );
        assert!(!StagingLoadTestAdmissionError::PersistenceUnavailable
            .to_string()
            .contains("D1"));
    }
}
