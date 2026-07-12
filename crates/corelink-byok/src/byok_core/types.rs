//! Core types for the BYOK subsystem.
//!
//! # Key safety
//!
//! [`Dek`] derives `ZeroizeOnDrop` — the 32-byte key material is cleared from
//! memory when the value is dropped.  It intentionally does NOT implement
//! `Debug`, `Display`, `Serialize`, or `Clone` so that key bytes never appear
//! in logs, traces, or serialised payloads.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Which KMS backend is in use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum KmsProviderKind {
    /// AWS Key Management Service.
    AwsKms,
    /// Google Cloud KMS.
    GcpKms,
    /// Azure Key Vault.
    AzureKeyVault,
    /// HashiCorp Vault (transit secrets engine).
    HashicorpVault,
}

/// Stable identifier for a customer-managed key (CMK).
///
/// Stored in D1 per-blob so the correct key is used on read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KmsKeyId {
    /// Provider variant.
    pub provider: KmsProviderKind,
    /// AWS ARN, GCP resource name, Azure key URI, or Vault key path.
    pub key_arn_or_id: String,
    /// Region of the CMK — must match the CoreLink region for latency SLO.
    pub region: String,
}

/// KMS-wrapped DEK as persisted in D1 `byok_envelope`.
///
/// The `ciphertext` can only be decrypted by the KMS provider that wrapped it,
/// using the specific CMK recorded in `key_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedDek {
    /// Provider that performed the wrap.
    pub provider: KmsProviderKind,
    /// CMK identity.
    pub key_id: KmsKeyId,
    /// Provider-opaque ciphertext (typically 150–300 bytes for AWS KMS).
    pub ciphertext: Vec<u8>,
    /// AAD binding: `{"tenant_id": "...", "blob_hash": "..."}`.
    ///
    /// MANDATORY — if absent the unwrap is rejected.
    pub encryption_context: Option<serde_json::Value>,
}

/// Plaintext DEK — 32 bytes, CSPRNG-generated (`getrandom`).
///
/// # Security invariants
///
/// - Generated via `getrandom::getrandom` (OS-backed entropy; NIST SP 800-90A DRBG).
///   **NOT** BLAKE3-derived — deterministic DEK = compromise propagation.
/// - `ZeroizeOnDrop`: memory cleared when dropped.
/// - `Debug` impl intentionally redacts key bytes — they NEVER appear in logs.
#[derive(ZeroizeOnDrop, Zeroize)]
pub struct Dek {
    /// Raw 32-byte key material.  Access only for encrypt / decrypt operations.
    pub bytes: [u8; 32],
}

impl std::fmt::Debug for Dek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dek").field("bytes", &"[REDACTED]").finish()
    }
}

impl Dek {
    /// Generate a fresh 32-byte DEK via OS CSPRNG (NIST SP 800-90A DRBG).
    ///
    /// S-14 sprint-close P0: canonical constructor used by matrix tests
    /// + revocation crate. Mirrors private `envelope::generate_dek()`.
    pub fn generate() -> Result<Self, BYOKError> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes)
            .map_err(|e| BYOKError::EnvelopeError(format!("getrandom: {e}")))?;
        Ok(Self { bytes })
    }
}

/// Tenant Convergence Secret (TCS) — 32-byte per-tenant secret (plan §1).
///
/// The TCS is the input keying material for convergent (Mode A) DEK + nonce
/// derivation: identical plaintext within an org yields an identical DEK →
/// identical ciphertext → intra-org dedup is preserved, while a *different*
/// org's TCS yields different ciphertext for the same bytes (no cross-org
/// correlation). The TCS is itself wrapped by the customer CMK (via
/// [`crate::KmsProvider::wrap_dek`]); the plaintext lives only inside the
/// `≤300s` cache window.
///
/// # Security invariants
///
/// - 32 bytes, CSPRNG-generated via [`Tcs::generate`].
/// - `ZeroizeOnDrop`: memory cleared when dropped (same discipline as [`Dek`]).
/// - `Debug` redacts the bytes; no `Display` / `Serialize` / `Clone`, so the
///   secret never reaches logs, traces, or serialized payloads.
#[derive(ZeroizeOnDrop, Zeroize)]
pub struct Tcs {
    /// Raw 32-byte secret. Access only for HKDF derivation.
    pub bytes: [u8; 32],
}

impl std::fmt::Debug for Tcs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tcs").field("bytes", &"[REDACTED]").finish()
    }
}

impl Tcs {
    /// Generate a fresh 32-byte TCS via OS CSPRNG (NIST SP 800-90A DRBG).
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::EnvelopeError`] if the OS CSPRNG is unavailable.
    pub fn generate() -> Result<Self, BYOKError> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes)
            .map_err(|e| BYOKError::EnvelopeError(format!("getrandom tcs: {e}")))?;
        Ok(Self { bytes })
    }

    /// Wrap raw bytes as a [`Tcs`] (e.g. after unwrapping the CMK-wrapped
    /// form). The caller is responsible for the provenance of `bytes`.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
}

/// CMK access status returned by [`crate::KmsProvider::check_access`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum KmsAccessStatus {
    /// CMK access confirmed.
    Ok,
    /// Customer has revoked CMK access (kill switch).
    Revoked,
    /// Provider is rate-limiting requests.
    Throttled,
    /// Provider returned an HTTP error.
    ApiError(u16),
    /// CMK scheduled for deletion or already deleted.
    NotFound,
}

impl KmsKeyId {
    /// Return the raw key ARN / ID string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.key_arn_or_id
    }
}

impl std::fmt::Display for KmsKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key_arn_or_id)
    }
}

impl KmsProviderKind {
    /// Canonical lowercase string label for logging / audit (e.g. `"aws"`, `"gcp"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwsKms => "aws",
            Self::GcpKms => "gcp",
            Self::AzureKeyVault => "azure",
            Self::HashicorpVault => "vault",
        }
    }
}

/// FIPS compliance level of a KMS provider instance.
///
/// Ordering: higher variant ⇒ higher compliance level.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum FipsLevel {
    /// No FIPS certification.
    None,
    /// FIPS 140-2 Level 1.
    Fips140_2_L1,
    /// FIPS 140-2 Level 2.
    Fips140_2_L2,
    /// FIPS 140-3 Level 1.
    Fips140_3_L1,
    /// FIPS 140-3 Level 2.
    Fips140_3_L2,
}

/// All errors produced by the BYOK subsystem.
#[derive(Debug, Error)]
pub enum BYOKError {
    /// Generic KMS provider error (network, auth, etc.).
    #[error("KMS provider error: {0}")]
    Provider(String),

    /// CMK access has been revoked by the customer.
    #[error("CMK access revoked: provider={provider:?} key_id={key_id}")]
    CmkRevoked {
        /// Which provider reported the revocation.
        provider: KmsProviderKind,
        /// The key ARN / ID.
        key_id: String,
    },

    /// DEK cache entry expired (normal TTL expiry; re-fetch from KMS).
    #[error("DEK cache TTL expired; re-fetch required")]
    DekCacheExpired,

    /// Caller attempted to create a DEK cache with TTL > 300 s.
    ///
    /// This is a hard rejection — INV-BYOK-CRYPTO-SOVEREIGNTY forbids TTL > 5 min.
    #[error(
        "DEK cache TTL > 300 s attempted (got {attempted_seconds} s); \
         INV-BYOK-CRYPTO-SOVEREIGNTY violation — max 300 s"
    )]
    DekCacheTtlViolation {
        /// The TTL value that was rejected.
        attempted_seconds: u64,
    },

    /// Envelope encryption / decryption error.
    #[error("envelope encryption error: {0}")]
    EnvelopeError(String),

    /// AES-256-GCM encrypt / decrypt failure.
    #[error("AES-GCM error: {0}")]
    AesGcm(String),

    /// AAD (encryption_context) mismatch on unwrap.
    #[error("AAD / encryption_context mismatch: cross-blob swap attempt rejected")]
    AadMismatch,

    /// Vault mTLS certificate expiring within renewal SLA (≤30d). Used
    /// by `corelink-byok-vault::check_cert_expiry`; surfaces from
    /// `KmsProvider::check_access` to drive cert rotation alerts.
    #[error("vault mTLS cert expiring soon: days_remaining={days_remaining}")]
    MtlsCertExpiringSoon {
        /// Days until the mTLS cert expires.
        days_remaining: u64,
    },

    /// The wrapped DEK payload is malformed or wrong length.
    #[error("DEK length invalid: got {got} bytes, expected 32")]
    DekLengthInvalid {
        /// Actual byte count.
        got: usize,
    },

    /// Missing mandatory `encryption_context` field.
    #[error("encryption_context (AAD) missing — mandatory for BYOK envelope")]
    EncryptionContextMissing,

    /// Convergent (Mode A) crypto was invoked with a chunked/multipart
    /// context. This crate is single-shot ONLY (audit [C-1]): reusing a
    /// whole-object-digest `(DEK, nonce)` across parts is a catastrophic
    /// AES-GCM break. Multipart BYOK is deferred and must be gated elsewhere.
    #[error(
        "convergent crypto is single-shot only; chunked/multipart unsupported \
         (chunk_index={chunk_index}) — audit C-1"
    )]
    ChunkedConvergentUnsupported {
        /// The offending non-zero chunk index.
        chunk_index: u64,
    },

    /// FIPS compliance level is below required threshold.
    #[error("FIPS compliance mismatch: required={required:?} actual={actual:?}")]
    FipsMismatch {
        /// Required level.
        required: FipsLevel,
        /// Reported level.
        actual: FipsLevel,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn tcs_debug_redacts_secret_and_emits_nonempty() {
        // The `Tcs` Debug impl MUST redact key material. A mutant that writes
        // nothing (`Ok(Default::default())`) produces an empty string with no
        // "REDACTED" marker → this assertion fails and kills it.
        let tcs = Tcs::from_bytes([0xABu8; 32]);
        let rendered = format!("{tcs:?}");
        assert!(
            rendered.contains("REDACTED"),
            "Tcs Debug must contain REDACTED, got {rendered:?}"
        );
        // And it must NOT leak the raw byte value (0xAB == 171 decimal).
        assert!(
            !rendered.contains("171"),
            "Tcs Debug must not leak secret bytes, got {rendered:?}"
        );
    }

    #[test]
    fn dek_debug_redacts_secret() {
        let dek = Dek {
            bytes: [0xCDu8; 32],
        };
        let rendered = format!("{dek:?}");
        assert!(rendered.contains("REDACTED"), "got {rendered:?}");
    }

    #[test]
    fn tcs_from_bytes_roundtrips_value() {
        let tcs = Tcs::from_bytes([7u8; 32]);
        assert_eq!(tcs.bytes, [7u8; 32]);
    }
}
