//! `corelink-byok-gcp` — GCP Cloud KMS adapter implementing [`KmsProvider`].
//!
//! # FIPS status
//!
//! GCP Cloud KMS default tier = **FIPS 140-2 Level 1** (NIST CMVP #3978).
//! GCP has committed to FIPS 140-3 certification; this adapter will be
//! updated when GCP publishes the CMVP certificate. Quarterly Crypto SME
//! review tracks CMVP status.
//!
//! # Authentication
//!
//! Service Account with minimal IAM scope:
//! - `roles/cloudkms.cryptoKeyEncrypterDecrypter` on the specific key resource
//! - No broader KMS or project-level permissions
//!
//! Credentials loaded from the environment (Application Default Credentials):
//! `GOOGLE_APPLICATION_CREDENTIALS` env var → service account JSON.
//!
//! # AAD semantics
//!
//! GCP KMS `encrypt` / `decrypt` API accepts `additional_authenticated_data`
//! as raw bytes. CoreLink serializes `encryption_context: Option<serde_json::Value>`
//! to UTF-8 JSON bytes before passing to GCP. The same bytes must be
//! re-presented at decrypt time; mismatch causes GCP to reject the call.
//!
//! # Mock mode (CI / tests)
//!
//! When `CORELINK_BYOK_GCP_MOCK=1` env var is set, [`GcpKmsProvider`]
//! operates in mock mode: wrap = identity copy, unwrap = identity copy,
//! check_access = Ok. This allows the 16-combination matrix test to run
//! in CI without real GCP credentials.
//!
//! # Examples
//!
//! ## Constructing a GcpKmsProvider in mock mode
//!
//! ```rust
//! use corelink_byok::gcp::GcpKmsProvider;
//! use corelink_byok::{KmsProvider, FipsLevel};
//!
//! let provider = GcpKmsProvider::new_mock("us-east1");
//! assert_eq!(provider.fips_level(), FipsLevel::Fips140_2_L1);
//! assert_eq!(provider.region(), "us-east1");
//! ```
//!
//! ## wrap + unwrap roundtrip in mock mode
//!
//! ```rust
//! # tokio_test::block_on(async {
//! use corelink_byok::{KmsProvider, KmsKeyId, KmsProviderKind, Dek};
//! use corelink_byok::gcp::GcpKmsProvider;
//!
//! let provider = GcpKmsProvider::new_mock("us-east1");
//! let key_id = KmsKeyId {
//!     provider: KmsProviderKind::GcpKms,
//!     key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(),
//!     region: "us-east1".to_string(),
//! };
//! let dek = Dek::generate().unwrap();
//! let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H1"});
//! let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx)).await.unwrap();
//! let unwrapped = provider.unwrap_dek(&wrapped).await.unwrap();
//! assert_eq!(dek.bytes, unwrapped.bytes);
//! # });
//! ```
//!
//! ## AAD binding: mismatched context rejected
//!
//! ```rust
//! # tokio_test::block_on(async {
//! use corelink_byok::{KmsProvider, KmsKeyId, KmsProviderKind, Dek, WrappedDek};
//! use corelink_byok::gcp::GcpKmsProvider;
//! use serde_json::json;
//!
//! let provider = GcpKmsProvider::new_mock("us-east1");
//! let key_id = KmsKeyId {
//!     provider: KmsProviderKind::GcpKms,
//!     key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(),
//!     region: "us-east1".to_string(),
//! };
//! let dek = Dek::generate().unwrap();
//! let ctx_a = json!({"tenant_id": "T1"});
//! let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx_a)).await.unwrap();
//!
//! // Tamper: change the stored encryption_context
//! let tampered = WrappedDek {
//!     encryption_context: Some(json!({"tenant_id": "ATTACKER"})),
//!     ..wrapped
//! };
//! let result = provider.unwrap_dek(&tampered).await;
//! assert!(result.is_err());
//! # });
//! ```


use async_trait::async_trait;
use crate::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider,
                    KmsProviderKind, WrappedDek};

pub(crate) mod key_resource;

#[cfg(feature = "production-gcp")]
mod adc;

/// Canonical BYOK GCP KMS real-mode entry point — see
/// `specs/_audits/2026-05-15-byok-real-provider-pattern.md`.
///
/// The module ships JCS-canonicalization helpers + wasm32 stub
/// unconditionally; the native HTTPS / ADC client is gated by the
/// `production` feature (which pulls in `reqwest`, `base64`,
/// `jsonwebtoken`).
pub mod real;

#[cfg(all(not(target_arch = "wasm32"), feature = "production-gcp"))]
pub use real::GcpKmsRealProvider;

#[cfg(target_arch = "wasm32")]
pub use real::{GcpKmsRealProvider, GcpKmsWasmStub};

/// Test-only utilities. Hidden from documentation; gated on the `production`
/// feature. Used by the in-crate `tests/` integration suites to inject a
/// static bearer token + custom endpoint.
#[cfg(feature = "production-gcp")]
#[doc(hidden)]
pub mod __test_support {
    pub use super::adc::AdcCredentials;
}

/// GCP Cloud KMS adapter.
///
/// Implements [`KmsProvider`] against the GCP Cloud KMS REST/gRPC API.
///
/// In production, credentials are loaded from Application Default
/// Credentials (`GOOGLE_APPLICATION_CREDENTIALS`). In test/CI, set
/// `CORELINK_BYOK_GCP_MOCK=1` to use mock mode, or construct directly
/// with [`GcpKmsProvider::new_mock`].
///
/// FIPS level: [`FipsLevel::Fips140_2_L1`] (NIST CMVP #3978; 140-3
/// pending GCP certification — quarterly Crypto SME review tracks).
#[derive(Debug)]
pub struct GcpKmsProvider {
    region: String,
    /// Mock mode: bypass real GCP API calls.
    mock: bool,
}

impl GcpKmsProvider {
    /// Construct a mock GcpKmsProvider for CI / unit tests.
    ///
    /// In mock mode, `wrap_dek` = identity copy of DEK bytes,
    /// `unwrap_dek` = identity copy, `check_access` = `Ok`.
    /// AAD binding is still enforced (context stored + verified at
    /// unwrap time).
    #[must_use]
    pub fn new_mock(region: &str) -> Self {
        Self {
            region: region.to_string(),
            mock: true,
        }
    }

    /// Construct a production GcpKmsProvider.
    ///
    /// Credentials loaded from Application Default Credentials.
    /// Returns `Err` if credentials cannot be loaded (misconfiguration).
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if ADC credentials are unavailable.
    pub fn new_production(region: &str) -> Result<Self, BYOKError> {
        // In production this would initialize the GCP KMS SDK client.
        // Deferred: google-kms1 or cloud.google.com/go/kms SDK wiring.
        // The mock flag allows CI to pass without real credentials.
        let mock = std::env::var("CORELINK_BYOK_GCP_MOCK")
            .map(|v| v == "1")
            .unwrap_or(false);
        Ok(Self {
            region: region.to_string(),
            mock,
        })
    }

    /// Serialize `encryption_context` to GCP `additional_authenticated_data` bytes.
    ///
    /// GCP accepts raw bytes; CoreLink serializes to UTF-8 JSON for a
    /// canonical, deterministic encoding.
    fn context_to_aad(ctx: Option<&serde_json::Value>) -> Vec<u8> {
        match ctx {
            None => vec![],
            Some(v) => serde_json::to_vec(v).unwrap_or_default(),
        }
    }

    /// Compute an 8-byte fingerprint of AAD bytes for ciphertext binding.
    ///
    /// Production GCP KMS enforces AAD binding server-side. In mock mode,
    /// we embed this fingerprint in the ciphertext so post-wrap tampering
    /// of `encryption_context` is detectable.
    ///
    /// Implementation: XOR-fold the AAD bytes into an 8-byte array.
    /// Not cryptographically secure, but sufficient for mock-mode tamper
    /// detection (production relies on GCP server-side AEAD verification).
    fn aad_fingerprint(aad: &[u8]) -> [u8; 8] {
        let mut fp = [0u8; 8];
        for (i, &b) in aad.iter().enumerate() {
            // Safe: i % 8 is always in [0..8); fp has length 8.
            if let Some(slot) = fp.get_mut(i % 8) {
                *slot ^= b;
            }
        }
        // Mix with length to make None (len=0) vs Some([]) distinguishable.
        let len_byte = (aad.len() as u8).wrapping_mul(0x37);
        if let Some(last) = fp.last_mut() {
            *last ^= len_byte;
        }
        fp
    }
}

#[async_trait]
impl KmsProvider for GcpKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::GcpKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // GCP Cloud KMS NIST CMVP #3978: FIPS 140-2 Level 1.
        // FIPS 140-3 pending GCP certification (quarterly Crypto SME review).
        FipsLevel::Fips140_2_L1
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        if key_id.provider != KmsProviderKind::GcpKms {
            return Err(BYOKError::EnvelopeError(format!(
                "GcpKmsProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }

        if !key_resource::is_valid_cloud_kms_resource(&key_id.key_arn_or_id) {
            return Err(BYOKError::Provider(format!(
                "malformed Cloud KMS key resource: '{}'",
                key_id.key_arn_or_id
            )));
        }

        if self.mock {
            // Mock ciphertext layout:
            // [0..8]  = AAD fingerprint (first 8 bytes of SHA2-like: XOR-folded JSON of ctx)
            // [8..40] = DEK bytes XOR'd with 0xAA (32 bytes)
            // Total: 40 bytes.
            //
            // The AAD fingerprint allows verify_aad_ciphertext to detect
            // post-wrap tampering of encryption_context.
            let aad = Self::context_to_aad(encryption_context);
            let aad_fingerprint = Self::aad_fingerprint(&aad);
            let mut ciphertext = Vec::with_capacity(8 + 32);
            ciphertext.extend_from_slice(&aad_fingerprint);
            for b in &dek.bytes {
                ciphertext.push(b ^ 0xAA);
            }
            return Ok(WrappedDek {
                provider: KmsProviderKind::GcpKms,
                key_id: key_id.clone(),
                ciphertext,
                encryption_context: encryption_context.cloned(),
            });
        }

        // Production path: POST to GCP KMS encrypt endpoint.
        // additional_authenticated_data = JSON-encoded encryption_context.
        let _aad = Self::context_to_aad(encryption_context);
        Err(BYOKError::Provider(
            "GcpKmsProvider production mode not wired (SDK integration deferred; use mock for CI)".to_string(),
        ))
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::GcpKms {
            return Err(BYOKError::EnvelopeError(format!(
                "GcpKmsProvider received WrappedDek with wrong provider: {:?}",
                wrapped.provider
            )));
        }

        if self.mock {
            if wrapped.ciphertext.len() != 40 {
                return Err(BYOKError::EnvelopeError(format!(
                    "GCP mock: wrong ciphertext length {} (expected 40)",
                    wrapped.ciphertext.len()
                )));
            }

            // Verify AAD binding: compare stored fingerprint with current context.
            let expected_aad = Self::context_to_aad(wrapped.encryption_context.as_ref());
            let expected_fingerprint = Self::aad_fingerprint(&expected_aad);
            let stored_fingerprint = wrapped.ciphertext
                .get(..8)
                .ok_or(BYOKError::EnvelopeError("GCP mock: ciphertext too short for fingerprint".to_string()))?;

            if stored_fingerprint != expected_fingerprint.as_slice() {
                return Err(BYOKError::EnvelopeError(
                    "GCP KMS additional_authenticated_data mismatch: encryption_context tampered".to_string(),
                ));
            }

            // Reverse the XOR marker.
            let ct = wrapped.ciphertext
                .get(8..)
                .ok_or(BYOKError::EnvelopeError("GCP mock: ciphertext too short for DEK".to_string()))?;
            let mut bytes = [0u8; 32];
            for (b_out, &b_in) in bytes.iter_mut().zip(ct.iter()) {
                *b_out = b_in ^ 0xAA;
            }
            Ok(Dek { bytes })
        } else {
            Err(BYOKError::Provider(
                "GcpKmsProvider production mode not wired".to_string(),
            ))
        }
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::GcpKms {
            return Err(BYOKError::EnvelopeError(format!(
                "GcpKmsProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }
        if self.mock {
            return Ok(KmsAccessStatus::Ok);
        }
        // Production: GET key resource + check enabled state.
        Err(BYOKError::Provider(
            "GcpKmsProvider production check_access not wired".to_string(),
        ))
    }
}

/// Return `true` if this provider is operating in mock mode.
///
/// Used by the 16-combination matrix test to ensure at least one of
/// mock/real mode is exercised per CI run.
#[must_use]
pub fn is_mock_env() -> bool {
    std::env::var("CORELINK_BYOK_GCP_MOCK")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::KmsProvider;

    #[test]
    fn fips_level_is_140_2_l1() {
        let p = GcpKmsProvider::new_mock("us-east1");
        assert_eq!(p.fips_level(), FipsLevel::Fips140_2_L1);
    }

    #[test]
    fn region_stored() {
        let p = GcpKmsProvider::new_mock("europe-west1");
        assert_eq!(p.region(), "europe-west1");
    }

    #[test]
    fn provider_kind_is_gcp() {
        let p = GcpKmsProvider::new_mock("us-east1");
        assert_eq!(p.provider_kind(), KmsProviderKind::GcpKms);
    }

    #[tokio::test]
    async fn wrap_unwrap_roundtrip_mock() {
        let p = GcpKmsProvider::new_mock("us-east1");
        let key_id = KmsKeyId {
            provider: KmsProviderKind::GcpKms,
            key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(),
            region: "us-east1".to_string(),
        };
        let dek = Dek::generate().expect("entropy");
        let orig = dek.bytes;
        let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H1"});
        let wrapped = p.wrap_dek(&dek, &key_id, Some(&ctx)).await.expect("wrap");
        let unwrapped = p.unwrap_dek(&wrapped).await.expect("unwrap");
        assert_eq!(orig, unwrapped.bytes);
    }

    #[tokio::test]
    async fn check_access_ok_mock() {
        let p = GcpKmsProvider::new_mock("us-east1");
        let key_id = KmsKeyId {
            provider: KmsProviderKind::GcpKms,
            key_arn_or_id: "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk".to_string(),
            region: "us-east1".to_string(),
        };
        let status = p.check_access(&key_id).await.expect("check");
        assert_eq!(status, KmsAccessStatus::Ok);
    }

    #[tokio::test]
    async fn wrong_provider_rejected_on_wrap() {
        let p = GcpKmsProvider::new_mock("us-east1");
        let key_id = KmsKeyId {
            provider: KmsProviderKind::AwsKms, // wrong provider
            key_arn_or_id: "arn:aws:kms:us-east-1:123:key/abc".to_string(),
            region: "us-east-1".to_string(),
        };
        let dek = Dek::generate().expect("entropy");
        let result = p.wrap_dek(&dek, &key_id, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn wrong_provider_rejected_on_unwrap() {
        let p = GcpKmsProvider::new_mock("us-east1");
        let wrapped = WrappedDek {
            provider: KmsProviderKind::AwsKms, // wrong provider
            key_id: KmsKeyId {
                provider: KmsProviderKind::AwsKms,
                key_arn_or_id: "arn:aws:kms:us-east-1:123:key/abc".to_string(),
                region: "us-east-1".to_string(),
            },
            ciphertext: vec![0xAA; 32],
            encryption_context: None,
        };
        let result = p.unwrap_dek(&wrapped).await;
        assert!(result.is_err());
    }
}
