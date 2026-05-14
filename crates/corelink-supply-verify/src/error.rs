//! Error types for SLSA L3 provenance verification (WI-S12-001).
//!
//! [`VerifyError`] is `#[non_exhaustive]` to allow adding new error variants
//! without breaking downstream semver. All variants carry descriptive messages
//! suitable for structured logging and customer-facing CLI output.
//!
//! Exit code mapping (for CLI binary):
//! - Verification failures ([`BuilderMismatch`], [`RekorInclusionInvalid`],
//!   [`FulcioChainInvalid`], [`InTotoSchemaInvalid`]) → exit code 1.
//! - [`AttestationExpired`] → exit code 2.
//! - IO / parse errors → exit code 3.
//!
//! [`BuilderMismatch`]: VerifyError::BuilderMismatch
//! [`RekorInclusionInvalid`]: VerifyError::RekorInclusionInvalid
//! [`FulcioChainInvalid`]: VerifyError::FulcioChainInvalid
//! [`InTotoSchemaInvalid`]: VerifyError::InTotoSchemaInvalid
//! [`AttestationExpired`]: VerifyError::AttestationExpired

/// Errors that can occur during SLSA L3 provenance verification.
///
/// Implements [`std::error::Error`] via [`thiserror::Error`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// The Fulcio certificate chain could not be validated to the TUF-pinned root CA.
    ///
    /// This covers: root cert mismatch, intermediate chain broken, certificate revoked,
    /// or signature over the payload does not verify against the certificate public key.
    ///
    /// Exit code: 1.
    #[error("Fulcio chain invalid: {0}")]
    FulcioChainInvalid(String),

    /// The Rekor transparency log inclusion proof is absent or cryptographically invalid.
    ///
    /// This covers: bundle missing from attestation, log index not found in Rekor,
    /// Merkle inclusion proof fails to verify, or Merkle root hash mismatch.
    ///
    /// Exit code: 1.
    #[error("Rekor inclusion proof missing or invalid: {0}")]
    RekorInclusionInvalid(String),

    /// The builder identity in the Fulcio certificate SAN URI does not match the expected
    /// pattern provided by the caller.
    ///
    /// This is the primary defense against attestation forgery from forks or malicious
    /// workflow injections.
    ///
    /// Exit code: 1.
    #[error("builder identity mismatch (got `{got}`, expected pattern `{expected}`)")]
    BuilderMismatch {
        /// Actual builder SAN URI from Fulcio certificate.
        got: String,
        /// Expected builder pattern provided by caller.
        expected: String,
    },

    /// The in-toto attestation `predicateType` does not match the expected v1.0 schema.
    ///
    /// Only `https://slsa.dev/provenance/v1` is accepted. Old schemas (v0.0.1) and
    /// unknown schemas are rejected to prevent schema drift attacks (XZ Utils 2024 class).
    ///
    /// Exit code: 1.
    #[error("in-toto schema invalid: {0}")]
    InTotoSchemaInvalid(String),

    /// The Fulcio certificate validity window has passed (certs are valid ≤ 10 minutes).
    ///
    /// The attestation was valid at generation time but the cert is now expired.
    /// The attestation is still cryptographically valid if the Rekor inclusion proof is present
    /// (Rekor timestamps the entry at inclusion); however, this variant is returned when the
    /// verifier is configured to reject expired certificates outside the Rekor time window.
    ///
    /// Exit code: 2.
    #[error("attestation expired (cert validity window passed)")]
    AttestationExpired,

    /// The DSSE envelope format is invalid (e.g., alg=none, missing payload, malformed base64).
    ///
    /// Covers the `alg=none` equivalent attack class on DSSE envelopes.
    ///
    /// Exit code: 1.
    #[error("DSSE envelope invalid: {0}")]
    DsseEnvelopeInvalid(String),

    /// JSON deserialization or format error.
    ///
    /// Exit code: 3.
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// IO error reading bundle or attestation file.
    ///
    /// Exit code: 3.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Base64 decode error.
    ///
    /// Exit code: 3.
    #[error("base64 decode error: {0}")]
    Base64Decode(String),
}

impl VerifyError {
    /// Exit code to use for this error variant in the CLI binary.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::FulcioChainInvalid(_)
            | Self::RekorInclusionInvalid(_)
            | Self::BuilderMismatch { .. }
            | Self::InTotoSchemaInvalid(_)
            | Self::DsseEnvelopeInvalid(_) => 1,
            Self::AttestationExpired => 2,
            Self::Parse(_) | Self::Io(_) | Self::Base64Decode(_) => 3,
        }
    }

    /// Whether this error represents a security verification failure (vs IO/parse).
    pub fn is_security_failure(&self) -> bool {
        matches!(
            self,
            Self::FulcioChainInvalid(_)
                | Self::RekorInclusionInvalid(_)
                | Self::BuilderMismatch { .. }
                | Self::InTotoSchemaInvalid(_)
                | Self::DsseEnvelopeInvalid(_)
                | Self::AttestationExpired
        )
    }
}
