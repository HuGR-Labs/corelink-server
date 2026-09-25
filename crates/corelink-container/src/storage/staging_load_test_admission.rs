//! Authenticated and atomic admission for staging load-test runs.
//!
//! A caller supplies one server-verified claim to the D1 adapter.  This module
//! neither trusts request headers nor mounts a route or contacts a provider.

use core::fmt;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::{
    d1_http::{D1BatchStatement, D1HttpClient},
    staging_load_test_ownership::StagingLoadTestScenario,
};

const AUTH_DOMAIN: &[u8] = b"corelink/staging-load-admission-auth/v1\0";
const NONCE_DOMAIN: &[u8] = b"corelink/staging-load-admission-nonce/v1\0";
const MAX_CLAIM_LIFETIME_MS: i64 = 15 * 60 * 1_000;
const SQL_INSERT_RUN: &str = "INSERT INTO staging_load_test_runs (run_id, scenario, target_environment, target_deployment_sha, state, admitted_at_ms) VALUES (?1, ?2, 'staging', ?3, 'open', ?4)";
const SQL_INSERT_NONCE: &str = "INSERT INTO staging_load_test_admission_nonces (nonce_digest, run_id, scenario, target_environment, target_deployment_sha, issued_at_ms, expires_at_ms, admitted_at_ms) VALUES (?1, ?2, ?3, 'staging', ?4, ?5, ?6, ?7)";

type HmacSha256 = Hmac<Sha256>;

/// Exact identity expected by a writer before it consumes a verified claim.
#[derive(Debug)]
pub struct StagingLoadTestAdmissionExpectation<'a> {
    /// Canonical positive GitHub Actions run ID.
    pub run_id: &'a str,
    /// Allowlisted scenario for the run.
    pub scenario: StagingLoadTestScenario,
    /// Must be the literal string `staging`.
    pub target_environment: &'a str,
    /// Lowercase 40-hex deployment commit SHA.
    pub target_deployment_sha: &'a str,
}

/// Fixed, parameter-free failures for verification and durable consumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestAdmissionError {
    /// Credential format or tag is invalid.
    InvalidCredential,
    /// Run ID or deployment SHA is not canonical.
    InvalidIdentity,
    /// Only the staging environment is permitted.
    NonStaging,
    /// Nonce is not 32 bytes encoded as lowercase hex.
    InvalidNonce,
    /// Issuance/expiry relation is malformed or over 15 minutes.
    InvalidLifetime,
    /// Claim is expired or issued in the future.
    Expired,
    /// Claim differs from the caller's exact expected identity.
    MismatchedIdentity,
    /// A nonce or run/scenario identity was previously consumed.
    AlreadyConsumed,
    /// The D1 capability or batch was unavailable.
    PersistenceUnavailable,
}

impl fmt::Display for StagingLoadTestAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidCredential => "staging admission credential is invalid",
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

/// Server-only verifier for version-one staging admission credentials.
pub struct StagingLoadTestAdmissionVerifier {
    key: Vec<u8>,
}

impl fmt::Debug for StagingLoadTestAdmissionVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagingLoadTestAdmissionVerifier")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl StagingLoadTestAdmissionVerifier {
    /// Create a verifier only for an explicitly staging server with a 32-byte key.
    pub fn new(
        runtime_environment: &str,
        key: impl AsRef<[u8]>,
    ) -> Result<Self, StagingLoadTestAdmissionError> {
        if runtime_environment != "staging" {
            return Err(StagingLoadTestAdmissionError::NonStaging);
        }
        let key = key.as_ref();
        if key.len() < 32 {
            return Err(StagingLoadTestAdmissionError::InvalidCredential);
        }
        Ok(Self { key: key.to_vec() })
    }

    /// Load the staging-only verifier from process configuration.
    pub fn from_env() -> Result<Self, StagingLoadTestAdmissionError> {
        let environment = std::env::var("CORELINK_ENVIRONMENT").unwrap_or_default();
        let key = std::env::var("CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY")
            .map_err(|_| StagingLoadTestAdmissionError::InvalidCredential)?;
        Self::new(&environment, key.as_bytes())
    }

    /// Verify `v1.run.scenario.staging.sha.issued.expires.nonce.tag` at `now_ms`.
    ///
    /// The tag covers the exact canonical fields under a domain-separated HMAC;
    /// [`Mac::verify_slice`] provides the tag's constant-time comparison.
    pub fn verify(
        &self,
        credential: &str,
        now_ms: i64,
    ) -> Result<VerifiedStagingLoadTestAdmission, StagingLoadTestAdmissionError> {
        let mut fields = credential.split('.');
        let version = fields.next();
        let run_id = fields.next();
        let scenario = fields.next();
        let environment = fields.next();
        let sha = fields.next();
        let issued = fields.next();
        let expires = fields.next();
        let nonce = fields.next();
        let tag = fields.next();
        if version != Some("v1") || fields.next().is_some() {
            return Err(StagingLoadTestAdmissionError::InvalidCredential);
        }
        let (
            Some(run_id),
            Some(scenario),
            Some(environment),
            Some(sha),
            Some(issued),
            Some(expires),
            Some(nonce),
            Some(tag),
        ) = (
            run_id,
            scenario,
            environment,
            sha,
            issued,
            expires,
            nonce,
            tag,
        )
        else {
            return Err(StagingLoadTestAdmissionError::InvalidCredential);
        };
        let scenario = parse_scenario(scenario)?;
        validate_identity(run_id, scenario, sha)?;
        if environment != "staging" {
            return Err(StagingLoadTestAdmissionError::NonStaging);
        }
        let nonce_digest = nonce_digest_hex(&decode_nonce(nonce)?)?;
        let issued_at_ms = parse_canonical_i64(issued)?;
        let expires_at_ms = parse_canonical_i64(expires)?;
        validate_lifetime(issued_at_ms, expires_at_ms)?;
        if issued_at_ms > now_ms || now_ms >= expires_at_ms {
            return Err(StagingLoadTestAdmissionError::Expired);
        }
        let tag = decode_hex_32(tag).ok_or(StagingLoadTestAdmissionError::InvalidCredential)?;
        let signed =
            canonical_signed_payload(run_id, scenario, sha, issued_at_ms, expires_at_ms, nonce);
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| StagingLoadTestAdmissionError::InvalidCredential)?;
        mac.update(AUTH_DOMAIN);
        mac.update(signed.as_bytes());
        mac.verify_slice(&tag)
            .map_err(|_| StagingLoadTestAdmissionError::InvalidCredential)?;
        Ok(VerifiedStagingLoadTestAdmission {
            run_id: run_id.to_owned(),
            scenario,
            target_environment: environment.to_owned(),
            target_deployment_sha: sha.to_owned(),
            nonce_digest,
            issued_at_ms,
            expires_at_ms,
        })
    }

    #[cfg(test)]
    fn sign_for_test(
        &self,
        run_id: &str,
        scenario: StagingLoadTestScenario,
        sha: &str,
        issued_at_ms: i64,
        expires_at_ms: i64,
        nonce: &str,
    ) -> String {
        let signed =
            canonical_signed_payload(run_id, scenario, sha, issued_at_ms, expires_at_ms, nonce);
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("test key is valid");
        mac.update(AUTH_DOMAIN);
        mac.update(signed.as_bytes());
        let tag = lower_hex(mac.finalize().into_bytes()).expect("String formatting is infallible");
        format!("{signed}.{tag}")
    }
}

/// Authenticated claim with private fields, consumable only by this module.
pub struct VerifiedStagingLoadTestAdmission {
    run_id: String,
    scenario: StagingLoadTestScenario,
    target_environment: String,
    target_deployment_sha: String,
    nonce_digest: String,
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
            .field("nonce_digest", &"[REDACTED]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl VerifiedStagingLoadTestAdmission {
    /// Construct only after a server verifier authenticated every field.
    #[cfg(test)]
    fn from_verified_claims(
        run_id: String,
        scenario: StagingLoadTestScenario,
        target_environment: String,
        target_deployment_sha: String,
        nonce_digest: String,
        issued_at_ms: i64,
        expires_at_ms: i64,
    ) -> Self {
        Self {
            run_id,
            scenario,
            target_environment,
            target_deployment_sha,
            nonce_digest,
            issued_at_ms,
            expires_at_ms,
        }
    }
}

/// Immutable, nonsecret context returned after durable one-time consumption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagingLoadTestAdmissionContext {
    run_id: String,
    scenario: StagingLoadTestScenario,
    target_environment: String,
    target_deployment_sha: String,
    admitted_at_ms: i64,
}

impl StagingLoadTestAdmissionContext {
    /// Canonical admitted run ID.
    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    /// Allowlisted admitted scenario.
    #[must_use]
    pub const fn scenario(&self) -> StagingLoadTestScenario {
        self.scenario
    }
    /// Admitted environment, always `staging`.
    #[must_use]
    pub fn target_environment(&self) -> &str {
        &self.target_environment
    }
    /// Canonical admitted deployment SHA.
    #[must_use]
    pub fn target_deployment_sha(&self) -> &str {
        &self.target_deployment_sha
    }
    /// Server timestamp at durable admission.
    #[must_use]
    pub const fn admitted_at_ms(&self) -> i64 {
        self.admitted_at_ms
    }
}

/// Restricted D1 adapter that atomically creates a run and consumes its nonce.
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
    /// Construct the restricted D1 writer from its server-side environment.
    pub fn from_d1_env() -> Result<Self, StagingLoadTestAdmissionError> {
        D1HttpClient::for_staging_load_test_ownership_writes()
            .map(|d1| Self { d1 })
            .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)
    }

    /// Validate, then submit run creation and digest-only nonce persistence in one batch.
    pub async fn consume_verified_admission(
        &self,
        expected: StagingLoadTestAdmissionExpectation<'_>,
        admission: VerifiedStagingLoadTestAdmission,
    ) -> Result<StagingLoadTestAdmissionContext, StagingLoadTestAdmissionError> {
        let now_ms = unix_time_ms()?;
        validate_claim(&expected, &admission, now_ms)?;
        let scenario = scenario_name(admission.scenario);
        let run_params = vec![
            json!(admission.run_id),
            json!(scenario),
            json!(admission.target_deployment_sha),
            json!(now_ms),
        ];
        let nonce_params = vec![
            json!(admission.nonce_digest),
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
) -> Result<(), StagingLoadTestAdmissionError> {
    validate_identity(
        &admission.run_id,
        admission.scenario,
        &admission.target_deployment_sha,
    )?;
    validate_identity(
        expected.run_id,
        expected.scenario,
        expected.target_deployment_sha,
    )?;
    if admission.target_environment != "staging" || expected.target_environment != "staging" {
        return Err(StagingLoadTestAdmissionError::NonStaging);
    }
    if admission.run_id != expected.run_id
        || admission.scenario != expected.scenario
        || admission.target_environment != expected.target_environment
        || admission.target_deployment_sha != expected.target_deployment_sha
    {
        return Err(StagingLoadTestAdmissionError::MismatchedIdentity);
    }
    decode_hex_32(&admission.nonce_digest)
        .ok_or(StagingLoadTestAdmissionError::PersistenceUnavailable)?;
    validate_lifetime(admission.issued_at_ms, admission.expires_at_ms)?;
    if admission.issued_at_ms > now_ms || now_ms >= admission.expires_at_ms {
        return Err(StagingLoadTestAdmissionError::Expired);
    }
    Ok(())
}

fn validate_identity(
    run_id: &str,
    _scenario: StagingLoadTestScenario,
    sha: &str,
) -> Result<(), StagingLoadTestAdmissionError> {
    if run_id.is_empty()
        || run_id.len() > 20
        || run_id.starts_with('0')
        || !run_id.bytes().all(|byte| byte.is_ascii_digit())
        || sha.len() != 40
        || !sha
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
    let lifetime = expires_at_ms.checked_sub(issued_at_ms);
    if issued_at_ms < 0
        || expires_at_ms < 0
        || !matches!(lifetime, Some(value) if value > 0 && value <= MAX_CLAIM_LIFETIME_MS)
    {
        return Err(StagingLoadTestAdmissionError::InvalidLifetime);
    }
    Ok(())
}

fn parse_canonical_i64(value: &str) -> Result<i64, StagingLoadTestAdmissionError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(StagingLoadTestAdmissionError::InvalidCredential);
    }
    value
        .parse()
        .map_err(|_| StagingLoadTestAdmissionError::InvalidCredential)
}

fn decode_nonce(nonce: &str) -> Result<[u8; 32], StagingLoadTestAdmissionError> {
    decode_hex_32(nonce).ok_or(StagingLoadTestAdmissionError::InvalidNonce)
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut output = [0_u8; 32];
    let mut input = value.bytes();
    for byte in output.iter_mut() {
        let high = input.next().and_then(hex_nibble)?;
        let low = input.next().and_then(hex_nibble)?;
        *byte = (high << 4) | low;
    }
    if input.next().is_some() {
        return None;
    }
    Some(output)
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
    lower_hex(hasher.finalize())
}

fn lower_hex(bytes: impl IntoIterator<Item = u8>) -> Result<String, StagingLoadTestAdmissionError> {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}")
            .map_err(|_| StagingLoadTestAdmissionError::PersistenceUnavailable)?;
    }
    Ok(encoded)
}

fn parse_scenario(value: &str) -> Result<StagingLoadTestScenario, StagingLoadTestAdmissionError> {
    match value {
        "signup" => Ok(StagingLoadTestScenario::Signup),
        "webhook" => Ok(StagingLoadTestScenario::Webhook),
        "dsr" => Ok(StagingLoadTestScenario::Dsr),
        "cas" => Ok(StagingLoadTestScenario::Cas),
        "byok" => Ok(StagingLoadTestScenario::Byok),
        "endurance-2h" => Ok(StagingLoadTestScenario::Endurance2h),
        "b103-cargo-write" => Ok(StagingLoadTestScenario::B103CargoWrite),
        _ => Err(StagingLoadTestAdmissionError::InvalidIdentity),
    }
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

fn canonical_signed_payload(
    run_id: &str,
    scenario: StagingLoadTestScenario,
    sha: &str,
    issued: i64,
    expires: i64,
    nonce: &str,
) -> String {
    format!(
        "v1.{run_id}.{}.staging.{sha}.{issued}.{expires}.{nonce}",
        scenario_name(scenario)
    )
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
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const NONCE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    fn verifier() -> StagingLoadTestAdmissionVerifier {
        StagingLoadTestAdmissionVerifier::new("staging", b"01234567890123456789012345678901")
            .expect("test key")
    }
    #[test]
    fn verifies_exact_authenticated_claim_and_redacts_secrets() {
        let verifier = verifier();
        let credential =
            verifier.sign_for_test("123", StagingLoadTestScenario::Cas, SHA, 100, 200, NONCE);
        let claim = verifier.verify(&credential, 150).expect("valid credential");
        assert!(format!("{verifier:?}").contains("[REDACTED]"));
        assert!(format!("{claim:?}").contains("[REDACTED]"));
        assert!(!format!("{claim:?}").contains(NONCE));
        let epoch_credential =
            verifier.sign_for_test("124", StagingLoadTestScenario::Cas, SHA, 0, 100, NONCE);
        assert!(verifier.verify(&epoch_credential, 50).is_ok());
    }
    #[test]
    fn rejects_forgery_nonstaging_and_lifetime_edges() {
        let verifier = verifier();
        let credential =
            verifier.sign_for_test("123", StagingLoadTestScenario::Cas, SHA, 100, 200, NONCE);
        assert!(matches!(
            verifier.verify(&credential.replace(".cas.", ".dsr."), 150),
            Err(StagingLoadTestAdmissionError::InvalidCredential)
        ));
        assert!(matches!(
            verifier.verify(&credential, 200),
            Err(StagingLoadTestAdmissionError::Expired)
        ));
        assert!(StagingLoadTestAdmissionVerifier::new(
            "production",
            b"01234567890123456789012345678901"
        )
        .is_err());
    }
    #[test]
    fn nonce_digest_is_lowercase_domain_separated_without_indexing() {
        let digest =
            nonce_digest_hex(&decode_nonce(NONCE).expect("canonical nonce")).expect("digest");
        assert_eq!(
            digest,
            "1fff857caecccadff3a32ad2952ad385c0e9c40e165de7a86bc2a86f2ce4dfc2"
        );
        assert!(decode_nonce(&"A".repeat(64)).is_err());
    }
    #[test]
    fn expected_identity_rejects_rebinding_and_future_claims() {
        let expected = StagingLoadTestAdmissionExpectation {
            run_id: "123",
            scenario: StagingLoadTestScenario::Cas,
            target_environment: "staging",
            target_deployment_sha: SHA,
        };
        let claim = VerifiedStagingLoadTestAdmission::from_verified_claims(
            "123".into(),
            StagingLoadTestScenario::Cas,
            "staging".into(),
            SHA.into(),
            "a".repeat(64),
            151,
            200,
        );
        assert_eq!(
            validate_claim(&expected, &claim, 150),
            Err(StagingLoadTestAdmissionError::Expired)
        );
    }
}
