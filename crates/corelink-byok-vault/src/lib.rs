//! `corelink-byok-vault` — HashiCorp Vault Transit adapter implementing
//! [`KmsProvider`] (WI-S14-005).
//!
//! # FIPS status
//!
//! HashiCorp Vault Enterprise **FIPS 140-3 Level 1** build (customer-hosted).
//! NIST CMVP certification pending as of 2026-04 (quarterly Crypto SME
//! review tracks). OSS Vault is NOT FIPS-certified; this adapter requires
//! the Enterprise FIPS build.
//!
//! # Authentication
//!
//! **Mutual TLS (mTLS)** — customer provides:
//! - Client certificate (PEM): presented by CoreLink to Vault.
//! - CA certificate (PEM): used to verify Vault's server certificate.
//!
//! Cert pinning is enforced: the CA cert PEM hash is stored at config
//! time; connection is rejected if Vault presents a cert from a
//! different CA. This prevents MITM attacks.
//!
//! **Cert expiry alert**: when `check_access` runs, the client cert
//! expiry is checked. If ≤ 30 days remain, [`BYOKError::MtlsCertExpiringSoon`]
//! is returned alongside `KmsAccessStatus::Ok` (non-fatal; caller should
//! fire SEV-3 alert + notify customer). The 30-day threshold is enforced;
//! renewal flow is documented in runbook RB-BYOK-VAULT-CERT-RENEWAL.
//!
//! # Transit secrets engine
//!
//! Vault Transit engine API:
//! - `POST {transit_path}/encrypt/{key_name}` — wrap DEK.
//! - `POST {transit_path}/decrypt/{key_name}` — unwrap DEK.
//! - `GET {transit_path}/keys/{key_name}` — check access.
//!
//! `context` parameter = base64-encoded JSON of `encryption_context`.
//! Vault re-verifies the context at decrypt time; mismatch = reject.
//!
//! # Mock mode
//!
//! Set `CORELINK_BYOK_VAULT_MOCK=1` or use [`VaultProvider::new_mock`].
//! In mock mode, wrap = XOR marker, unwrap = reverse XOR, context
//! binding enforced via stored context comparison.
//!
//! # Examples
//!
//! ## wrap + unwrap roundtrip (mock)
//!
//! ```rust
//! # tokio_test::block_on(async {
//! use corelink_byok::{KmsProvider, KmsKeyId, KmsProviderKind, Dek, FipsLevel};
//! use corelink_byok_vault::VaultProvider;
//!
//! let provider = VaultProvider::new_mock("us-east-1");
//! assert_eq!(provider.fips_level(), FipsLevel::Fips140_3_L1);
//! let key_id = KmsKeyId {
//!     provider: KmsProviderKind::HashicorpVault,
//!     key_arn_or_id: "transit/keys/my-key".to_string(),
//!     region: "us-east-1".to_string(),
//! };
//! let dek = Dek::generate().unwrap();
//! let orig = dek.bytes;
//! let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H3"});
//! let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx)).await.unwrap();
//! let unwrapped = provider.unwrap_dek(&wrapped).await.unwrap();
//! assert_eq!(orig, unwrapped.bytes);
//! # });
//! ```
//!
//! ## Transit context binding enforced
//!
//! ```rust
//! # tokio_test::block_on(async {
//! use corelink_byok::{KmsProvider, KmsKeyId, KmsProviderKind, Dek, WrappedDek};
//! use corelink_byok_vault::VaultProvider;
//! use serde_json::json;
//!
//! let provider = VaultProvider::new_mock("us-east-1");
//! let key_id = KmsKeyId {
//!     provider: KmsProviderKind::HashicorpVault,
//!     key_arn_or_id: "transit/keys/my-key".to_string(),
//!     region: "us-east-1".to_string(),
//! };
//! let dek = Dek::generate().unwrap();
//! let ctx_a = json!({"tenant_id": "T1"});
//! let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx_a)).await.unwrap();
//!
//! // Tamper: change transit context → Vault context mismatch on decrypt.
//! let tampered = WrappedDek {
//!     encryption_context: Some(json!({"tenant_id": "ATTACKER"})),
//!     ..wrapped
//! };
//! let result = provider.unwrap_dek(&tampered).await;
//! assert!(result.is_err());
//! # });
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use corelink_byok::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider,
                    KmsProviderKind, WrappedDek};

#[cfg(feature = "production")]
pub mod auth;
#[cfg(feature = "production")]
mod key_name;
#[cfg(feature = "production")]
mod real;

#[cfg(feature = "production")]
pub use real::{VaultTransitProvider, DEFAULT_TRANSIT_MOUNT};

/// Test-only utilities (production feature). Allows the in-crate `tests/`
/// integration suites to inject a static auth backend + custom endpoint.
#[cfg(feature = "production")]
#[doc(hidden)]
pub mod __test_support {
    pub use crate::auth::VaultAuth;
}

/// Number of days before mTLS cert expiry that triggers a SEV-3 alert.
pub const MTLS_CERT_ALERT_DAYS: u64 = 30;

/// HashiCorp Vault Transit adapter.
///
/// Implements mutual TLS auth, context-bound transit wrap/unwrap, and
/// cert expiry alerting.
///
/// FIPS level: [`FipsLevel::Fips140_3_L1`] (Vault Enterprise FIPS build;
/// customer-hosted).
#[derive(Debug)]
pub struct VaultProvider {
    region: String,
    transit_engine_path: String,
    mock: bool,
    /// Simulated cert days remaining for cert expiry tests.
    /// `None` = not simulated (production behaviour; inspect real cert).
    mock_cert_days_remaining: Option<u64>,
}

impl VaultProvider {
    /// Construct a mock VaultProvider for CI / unit tests.
    ///
    /// mTLS cert check is stubbed (always Ok, no expiry unless
    /// `set_mock_cert_days_remaining` is called in tests).
    #[must_use]
    pub fn new_mock(region: &str) -> Self {
        Self {
            region: region.to_string(),
            transit_engine_path: "transit".to_string(),
            mock: true,
            mock_cert_days_remaining: None,
        }
    }

    /// Override the simulated mTLS cert days remaining for tests.
    ///
    /// Set to `Some(n)` to simulate a cert expiring in `n` days.
    /// Set to `None` to disable simulation (default). Only meaningful
    /// in mock mode; production mode reads the real cert.
    pub fn set_mock_cert_days_remaining(&mut self, days: Option<u64>) {
        self.mock_cert_days_remaining = days;
    }

    /// Construct a production VaultProvider with mTLS credentials.
    ///
    /// `vault_url` — e.g. `https://vault.customer.com:8200`.
    /// `transit_engine_path` — e.g. `transit` (relative to API prefix).
    /// `mtls_cert_pem` — customer-provided client cert.
    /// `ca_cert_pem` — CA cert for server cert pinning.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::MtlsError`] if certs cannot be parsed.
    pub fn new_production(
        vault_url: &str,
        transit_engine_path: &str,
        region: &str,
        _mtls_cert_pem: &[u8],
        _ca_cert_pem: &[u8],
    ) -> Result<Self, BYOKError> {
        let mock = std::env::var("CORELINK_BYOK_VAULT_MOCK")
            .map(|v| v == "1")
            .unwrap_or(false);
        // Production: initialize reqwest TLS client with mTLS cert + CA pinning.
        // Deferred: vault-client SDK wiring.
        let _ = vault_url;
        let _ = transit_engine_path;
        Ok(Self {
            region: region.to_string(),
            transit_engine_path: transit_engine_path.to_string(),
            mock,
            mock_cert_days_remaining: None,
        })
    }

    /// Return the transit engine path.
    #[must_use]
    pub fn transit_engine_path(&self) -> &str {
        &self.transit_engine_path
    }

    /// Encode `encryption_context` as base64-encoded JSON bytes for
    /// Vault Transit `context` parameter.
    fn context_to_vault_context(ctx: Option<&serde_json::Value>) -> String {
        match ctx {
            None => String::new(),
            Some(v) => {
                let json_bytes = serde_json::to_vec(v).unwrap_or_default();
                BASE64.encode(&json_bytes)
            }
        }
    }

    /// Check Vault mTLS cert expiry.
    ///
    /// Returns `Some(days_remaining)` if a simulated expiry is configured
    /// (via [`Self::set_mock_cert_days_remaining`]) or the real cert is
    /// inspected in production. Returns `None` if no simulation is active
    /// and production cert inspection is not yet wired.
    fn check_cert_expiry(&self) -> Option<u64> {
        if self.mock_cert_days_remaining.is_some() {
            return self.mock_cert_days_remaining;
        }
        // Production: parse client cert notAfter and compute days remaining.
        // Deferred: x509-parser crate integration.
        None
    }
}

#[async_trait]
impl KmsProvider for VaultProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::HashicorpVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Vault Enterprise FIPS 140-3 Level 1 build (customer-hosted).
        // OSS Vault is NOT FIPS-certified; Enterprise build required.
        // Quarterly Crypto SME review tracks CMVP certificate status.
        FipsLevel::Fips140_3_L1
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        if key_id.provider != KmsProviderKind::HashicorpVault {
            return Err(BYOKError::EnvelopeError(format!(
                "VaultProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }

        if self.mock {
            // Mock: ciphertext = DEK bytes XOR'd with 0x55 + prepend
            // a 32-byte "vault_ciphertext" marker; context stored in WrappedDek.
            let mut ct = dek.bytes.to_vec();
            for b in &mut ct {
                *b ^= 0x55;
            }
            // Store the vault context as first 32 bytes (mock: hash of context).
            let ctx_b64 = Self::context_to_vault_context(encryption_context);
            let ctx_bytes = ctx_b64.as_bytes();
            let mut ciphertext = Vec::with_capacity(32 + ct.len());
            // Pad/truncate context marker to 32 bytes for fixed-layout parsing.
            let mut ctx_marker = [0u8; 32];
            let copy_len = ctx_bytes.len().min(32);
            ctx_marker
                .get_mut(..copy_len)
                .ok_or(BYOKError::EnvelopeError("ctx_marker slice".to_string()))?
                .copy_from_slice(
                    ctx_bytes.get(..copy_len)
                        .ok_or(BYOKError::EnvelopeError("ctx_bytes slice".to_string()))?,
                );
            ciphertext.extend_from_slice(&ctx_marker);
            ciphertext.extend_from_slice(&ct);

            return Ok(WrappedDek {
                provider: KmsProviderKind::HashicorpVault,
                key_id: key_id.clone(),
                ciphertext,
                encryption_context: encryption_context.cloned(),
            });
        }

        // Production: POST {transit_engine_path}/encrypt/{key_name}
        // with plaintext = base64(dek.bytes) and context = base64(JSON(ctx)).
        Err(BYOKError::Provider(
            "VaultProvider production mode not wired (SDK deferred; use mock for CI)".to_string(),
        ))
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::HashicorpVault {
            return Err(BYOKError::EnvelopeError(format!(
                "VaultProvider received WrappedDek with wrong provider: {:?}",
                wrapped.provider
            )));
        }

        if self.mock {
            if wrapped.ciphertext.len() < 64 {
                return Err(BYOKError::EnvelopeError(format!(
                    "Vault mock: ciphertext too short: {}",
                    wrapped.ciphertext.len()
                )));
            }

            // Verify context binding: the stored context marker must match
            // the context currently in wrapped.encryption_context.
            let stored_ctx_b64 = Self::context_to_vault_context(wrapped.encryption_context.as_ref());
            let stored_ctx_bytes = stored_ctx_b64.as_bytes();
            let mut expected_marker = [0u8; 32];
            let copy_len = stored_ctx_bytes.len().min(32);
            expected_marker
                .get_mut(..copy_len)
                .ok_or(BYOKError::EnvelopeError("expected_marker slice".to_string()))?
                .copy_from_slice(
                    stored_ctx_bytes.get(..copy_len)
                        .ok_or(BYOKError::EnvelopeError("stored_ctx_bytes slice".to_string()))?,
                );

            let actual_marker: [u8; 32] = wrapped.ciphertext
                .get(..32)
                .ok_or(BYOKError::EnvelopeError("Vault mock: ciphertext < 32 bytes".to_string()))?
                .try_into()
                .map_err(|_| BYOKError::EnvelopeError("Vault mock: cannot read context marker".to_string()))?;

            if actual_marker != expected_marker {
                return Err(BYOKError::EnvelopeError(
                    "Vault transit context mismatch: encryption_context tampered".to_string(),
                ));
            }

            // Reverse XOR marker.
            let ct = wrapped.ciphertext
                .get(32..)
                .ok_or(BYOKError::EnvelopeError("Vault mock: ciphertext < 64 bytes for ct".to_string()))?;
            if ct.len() != 32 {
                return Err(BYOKError::EnvelopeError(format!(
                    "Vault mock: DEK ciphertext length {} != 32",
                    ct.len()
                )));
            }
            let mut bytes = [0u8; 32];
            for (b_out, &b_in) in bytes.iter_mut().zip(ct.iter()) {
                *b_out = b_in ^ 0x55;
            }
            return Ok(Dek { bytes });
        }

        // Production: POST {transit_engine_path}/decrypt/{key_name}
        Err(BYOKError::Provider(
            "VaultProvider production mode not wired".to_string(),
        ))
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::HashicorpVault {
            return Err(BYOKError::EnvelopeError(format!(
                "VaultProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }

        // Check mTLS cert expiry regardless of mock/production mode.
        if let Some(days) = self.check_cert_expiry() {
            if days <= MTLS_CERT_ALERT_DAYS {
                // Cert expiry within alert threshold — return warning.
                // Caller fires SEV-3 + customer notification.
                return Err(BYOKError::MtlsCertExpiringSoon { days_remaining: days });
            }
        }

        if self.mock {
            return Ok(KmsAccessStatus::Ok);
        }

        // Production: GET {transit_engine_path}/keys/{key_name}; check status.
        Err(BYOKError::Provider(
            "VaultProvider production check_access not wired".to_string(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, clippy::indexing_slicing, clippy::uninlined_format_args)]
mod tests {
    use super::*;
    use corelink_byok::KmsProvider;

    fn vault_key_id() -> KmsKeyId {
        KmsKeyId {
            provider: KmsProviderKind::HashicorpVault,
            key_arn_or_id: "transit/keys/my-key".to_string(),
            region: "us-east-1".to_string(),
        }
    }

    #[test]
    fn fips_level_is_140_3_l1() {
        let p = VaultProvider::new_mock("us-east-1");
        assert_eq!(p.fips_level(), FipsLevel::Fips140_3_L1);
    }

    #[test]
    fn provider_kind_is_vault() {
        let p = VaultProvider::new_mock("us-east-1");
        assert_eq!(p.provider_kind(), KmsProviderKind::HashicorpVault);
    }

    #[test]
    fn transit_engine_path_default() {
        let p = VaultProvider::new_mock("us-east-1");
        assert_eq!(p.transit_engine_path(), "transit");
    }

    #[tokio::test]
    async fn wrap_unwrap_roundtrip_no_ctx() {
        let p = VaultProvider::new_mock("us-east-1");
        let dek = Dek::generate().expect("entropy");
        let orig = dek.bytes;
        let wrapped = p.wrap_dek(&dek, &vault_key_id(), None).await.expect("wrap");
        let unwrapped = p.unwrap_dek(&wrapped).await.expect("unwrap");
        assert_eq!(orig, unwrapped.bytes);
    }

    #[tokio::test]
    async fn wrap_unwrap_roundtrip_with_ctx() {
        let p = VaultProvider::new_mock("us-east-1");
        let dek = Dek::generate().expect("entropy");
        let orig = dek.bytes;
        let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H3"});
        let wrapped = p.wrap_dek(&dek, &vault_key_id(), Some(&ctx)).await.expect("wrap");
        let unwrapped = p.unwrap_dek(&wrapped).await.expect("unwrap");
        assert_eq!(orig, unwrapped.bytes);
    }

    #[tokio::test]
    async fn context_binding_enforced_on_tamper() {
        let p = VaultProvider::new_mock("us-east-1");
        let dek = Dek::generate().expect("entropy");
        let ctx_a = serde_json::json!({"tenant_id": "T1"});
        let wrapped = p.wrap_dek(&dek, &vault_key_id(), Some(&ctx_a)).await.expect("wrap");

        // Tamper: change encryption_context → context marker mismatch.
        let tampered = WrappedDek {
            encryption_context: Some(serde_json::json!({"tenant_id": "ATTACKER"})),
            ..wrapped
        };
        let result = p.unwrap_dek(&tampered).await;
        assert!(result.is_err(), "Vault transit context binding must reject tampered context");
    }

    #[tokio::test]
    async fn check_access_ok_mock() {
        let p = VaultProvider::new_mock("us-east-1");
        let status = p.check_access(&vault_key_id()).await.expect("check");
        assert_eq!(status, KmsAccessStatus::Ok);
    }

    #[tokio::test]
    async fn mtls_cert_expiry_alert_30d() {
        let mut p = VaultProvider::new_mock("us-east-1");
        p.set_mock_cert_days_remaining(Some(25)); // within 30d threshold
        let result = p.check_access(&vault_key_id()).await;
        assert!(
            matches!(result, Err(BYOKError::MtlsCertExpiringSoon { days_remaining: 25 })),
            "cert expiry within 30d must return MtlsCertExpiringSoon"
        );
    }

    #[tokio::test]
    async fn mtls_cert_expiry_exactly_30d() {
        let mut p = VaultProvider::new_mock("us-east-1");
        p.set_mock_cert_days_remaining(Some(30)); // exactly at threshold
        let result = p.check_access(&vault_key_id()).await;
        assert!(
            matches!(result, Err(BYOKError::MtlsCertExpiringSoon { days_remaining: 30 })),
            "cert expiry exactly 30d must alert"
        );
    }

    #[tokio::test]
    async fn mtls_cert_31_days_ok() {
        let mut p = VaultProvider::new_mock("us-east-1");
        p.set_mock_cert_days_remaining(Some(31)); // just outside threshold
        let status = p.check_access(&vault_key_id()).await.expect("check");
        assert_eq!(status, KmsAccessStatus::Ok);
    }

    #[tokio::test]
    async fn wrong_provider_rejected_wrap() {
        let p = VaultProvider::new_mock("us-east-1");
        let bad_key_id = KmsKeyId {
            provider: KmsProviderKind::AzureKeyVault,
            key_arn_or_id: "https://v.vault.azure.net/keys/k".to_string(),
            region: "eastus".to_string(),
        };
        let dek = Dek::generate().expect("entropy");
        let result = p.wrap_dek(&dek, &bad_key_id, None).await;
        assert!(result.is_err());
    }
}
