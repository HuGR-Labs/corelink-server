//! `corelink-byok-azure` — Azure Key Vault Premium HSM adapter implementing
//! [`KmsProvider`] (WI-S14-005).
//!
//! # FIPS status
//!
//! Azure Key Vault **Premium HSM tier** = **FIPS 140-2 Level 2** (mandatory
//! for BYOK; NIST CMVP #3516). Standard tier = FIPS 140-2 Level 1;
//! CoreLink requires Premium HSM for all BYOK tenants — this is enforced at
//! provisioning time and documented in the compliance matrix.
//!
//! # Authentication
//!
//! Managed Identity (preferred in Azure-hosted deployment) OR Service
//! Principal (for cross-cloud). The `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`,
//! `AZURE_CLIENT_SECRET` env vars are used for Service Principal auth.
//! Managed Identity uses the IMDS endpoint; no credentials required.
//!
//! # Custom AAD flow (ADR-S14-001)
//!
//! Azure Key Vault `wrapKey` / `unwrapKey` API (RSA-OAEP-256) does **not**
//! support Additional Authenticated Data natively — RSA-OAEP wraps a key,
//! not arbitrary data, and has no AAD input.
//!
//! CoreLink implements a two-layer approach:
//!
//! ```text
//! wrap_dek(dek, ctx):
//!   1. Generate ephemeral AES-256-GCM key (32 bytes CSPRNG).
//!   2. Encrypt dek.bytes using AES-256-GCM with AAD = ctx.
//!      → produces: aes_gcm_ciphertext (32 bytes) + nonce (12 bytes) + tag (16 bytes)
//!   3. Call Azure wrapKey(aes_gcm_ciphertext || nonce || tag, RSA-OAEP-256).
//!      → produces: rsa_ciphertext (~256 bytes for RSA-2048)
//!   4. Persist: rsa_ciphertext + aes_gcm_key (wrapped by rsa_ciphertext?) ...
//! ```
//!
//! Wait — actually the simpler correct flow is:
//!
//! ```text
//! wrap_dek(dek, ctx):
//!   1. Generate inner AES-GCM key K_inner (32 bytes CSPRNG).
//!   2. Encrypt dek.bytes using AES-256-GCM(K_inner, nonce_random, AAD=ctx).
//!      → inner_ct = nonce || gcm_ciphertext || gcm_tag
//!   3. Call Azure wrapKey(K_inner) → azure_wrapped_K_inner (RSA-OAEP ciphertext).
//!   4. Store: azure_wrapped_K_inner || inner_ct in WrappedDek.ciphertext.
//!
//! unwrap_dek(wrapped):
//!   1. Split ciphertext: first KEY_WRAP_LEN bytes = azure_wrapped_K_inner;
//!      remainder = inner_ct.
//!   2. Call Azure unwrapKey(azure_wrapped_K_inner) → K_inner.
//!   3. Decrypt AES-256-GCM(K_inner, nonce, inner_ct, AAD=ctx).
//!   4. Verify GCM tag — tag mismatch = AAD tampered.
//! ```
//!
//! This adds ~2 ms overhead but preserves AAD binding semantics.
//! Documented in ADR-S14-001.
//!
//! # Mock mode
//!
//! Set `CORELINK_BYOK_AZURE_MOCK=1` or use [`AzureKeyVaultProvider::new_mock`].
//! In mock mode the AES-GCM inner layer is exercised fully (real crypto);
//! only the Azure wrapKey call is stubbed (identity copy).
//!
//! # Examples
//!
//! ## wrap + unwrap roundtrip (mock)
//!
//! ```rust
//! # tokio_test::block_on(async {
//! use corelink_byok::{KmsProvider, KmsKeyId, KmsProviderKind, Dek, FipsLevel};
//! use corelink_byok::azure::AzureKeyVaultProvider;
//!
//! let provider = AzureKeyVaultProvider::new_mock("eastus");
//! assert_eq!(provider.fips_level(), FipsLevel::Fips140_2_L2);
//! let key_id = KmsKeyId {
//!     provider: KmsProviderKind::AzureKeyVault,
//!     key_arn_or_id: "https://myvault.vault.azure.net/keys/mykey".to_string(),
//!     region: "eastus".to_string(),
//! };
//! let dek = Dek::generate().unwrap();
//! let orig = dek.bytes;
//! let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H2"});
//! let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx)).await.unwrap();
//! let unwrapped = provider.unwrap_dek(&wrapped).await.unwrap();
//! assert_eq!(orig, unwrapped.bytes);
//! # });
//! ```
//!
//! ## AAD binding enforced via GCM tag
//!
//! ```rust
//! # tokio_test::block_on(async {
//! use corelink_byok::{KmsProvider, KmsKeyId, KmsProviderKind, Dek, WrappedDek};
//! use corelink_byok::azure::AzureKeyVaultProvider;
//! use serde_json::json;
//!
//! let provider = AzureKeyVaultProvider::new_mock("eastus");
//! let key_id = KmsKeyId {
//!     provider: KmsProviderKind::AzureKeyVault,
//!     key_arn_or_id: "https://myvault.vault.azure.net/keys/mykey".to_string(),
//!     region: "eastus".to_string(),
//! };
//! let dek = Dek::generate().unwrap();
//! let ctx_a = json!({"tenant_id": "T1"});
//! let wrapped = provider.wrap_dek(&dek, &key_id, Some(&ctx_a)).await.unwrap();
//!
//! // Tamper: change the stored encryption_context → GCM tag mismatch on unwrap.
//! let tampered = WrappedDek {
//!     encryption_context: Some(json!({"tenant_id": "ATTACKER"})),
//!     ..wrapped
//! };
//! let result = provider.unwrap_dek(&tampered).await;
//! assert!(result.is_err());
//! # });
//! ```


use async_trait::async_trait;
use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit, generic_array::GenericArray},
};
use crate::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider,
                    KmsProviderKind, WrappedDek};

pub(crate) mod key_resource;

// The native real provider is gated on `production` AND non-wasm32. On
// `wasm32-unknown-unknown` we always expose the `AzureKeyVaultWasmStub`
// regardless of feature flags (the underlying `reqwest` / `regex` / `tokio`
// stack does not target wasm32). See
// `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md` §4.
#[cfg(all(feature = "production-azure", not(target_arch = "wasm32")))]
mod entra;
#[cfg(any(
    all(feature = "production-azure", not(target_arch = "wasm32")),
    target_arch = "wasm32"
))]
pub mod real;

#[cfg(all(feature = "production-azure", not(target_arch = "wasm32")))]
pub use real::AzureKeyVaultRealProvider;

#[cfg(target_arch = "wasm32")]
pub use real::{AzureKeyVaultRealProvider, AzureKeyVaultWasmStub};

/// Test-only utilities. Hidden from documentation; gated on the `production`
/// feature. Used by the in-crate `tests/` integration suites to inject a
/// static bearer token or wiremock endpoint.
#[cfg(all(feature = "production-azure", not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod __test_support {
    pub use super::entra::EntraCredentials;
}

/// Number of bytes in the Azure RSA-wrapped inner key (mock: 32 bytes identity).
const MOCK_KEY_WRAP_LEN: usize = 32;
/// AES-GCM nonce length (96-bit = 12 bytes).
const NONCE_LEN: usize = 12;

/// Azure Key Vault Premium HSM adapter.
///
/// Implements the custom AAD flow: AES-256-GCM inner layer (AAD-bound)
/// before Azure RSA-OAEP-256 wrapKey. This preserves AAD binding semantics
/// despite Azure wrapKey not supporting AAD natively.
///
/// FIPS level: [`FipsLevel::Fips140_2_L2`] (NIST CMVP #3516, Premium HSM
/// tier mandatory for BYOK).
#[derive(Debug)]
pub struct AzureKeyVaultProvider {
    region: String,
    vault_url: String,
    mock: bool,
}

impl AzureKeyVaultProvider {
    /// Construct a mock AzureKeyVaultProvider for CI / unit tests.
    ///
    /// In mock mode, the inner AES-GCM layer is exercised with real
    /// crypto. The Azure wrapKey call is stubbed (identity copy of the
    /// inner key). AAD binding via GCM tag is fully enforced.
    #[must_use]
    pub fn new_mock(region: &str) -> Self {
        Self {
            region: region.to_string(),
            vault_url: "https://mock.vault.azure.net".to_string(),
            mock: true,
        }
    }

    /// Construct a production AzureKeyVaultProvider.
    ///
    /// Credentials from Managed Identity (IMDS) or Service Principal
    /// (`AZURE_CLIENT_ID` + `AZURE_TENANT_ID` + `AZURE_CLIENT_SECRET`).
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if auth setup fails.
    pub fn new_production(vault_url: &str, region: &str) -> Result<Self, BYOKError> {
        let mock = std::env::var("CORELINK_BYOK_AZURE_MOCK")
            .map(|v| v == "1")
            .unwrap_or(false);
        Ok(Self {
            region: region.to_string(),
            vault_url: vault_url.to_string(),
            mock,
        })
    }

    /// Return the vault URL.
    #[must_use]
    pub fn vault_url(&self) -> &str {
        &self.vault_url
    }

    /// Generate a random 32-byte inner AES key.
    fn gen_inner_key() -> Result<[u8; 32], BYOKError> {
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key)
            .map_err(|e| BYOKError::EnvelopeError(format!("getrandom inner key: {e}")))?;
        Ok(key)
    }

    /// Generate a random 12-byte nonce.
    fn gen_nonce() -> Result<[u8; NONCE_LEN], BYOKError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce)
            .map_err(|e| BYOKError::EnvelopeError(format!("getrandom nonce: {e}")))?;
        Ok(nonce)
    }

    /// AES-256-GCM encrypt `plaintext` with `key`, `nonce`, and `aad`.
    ///
    /// Returns `nonce || ciphertext_with_tag` (12 + len + 16 bytes).
    fn aes_gcm_encrypt(
        key: &[u8; 32],
        nonce: &[u8; NONCE_LEN],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, BYOKError> {
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
        let nonce_ga = GenericArray::from_slice(nonce);
        let payload = aes_gcm::aead::Payload { msg: plaintext, aad };
        let ct = cipher
            .encrypt(nonce_ga, payload)
            .map_err(|e| BYOKError::AesGcm(format!("azure inner AES-GCM encrypt: {e}")))?;
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ct);
        Ok(result)
    }

    /// AES-256-GCM decrypt `nonce_and_ct` (produced by [`Self::aes_gcm_encrypt`]).
    fn aes_gcm_decrypt(
        key: &[u8; 32],
        nonce_and_ct: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, BYOKError> {
        if nonce_and_ct.len() < NONCE_LEN + 16 {
            return Err(BYOKError::AesGcm("inner ciphertext too short".to_string()));
        }
        let (nonce_bytes, ct) = nonce_and_ct.split_at(NONCE_LEN);
        let nonce = GenericArray::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
        let payload = aes_gcm::aead::Payload { msg: ct, aad };
        cipher
            .decrypt(nonce, payload)
            .map_err(|e| BYOKError::AesGcm(format!("azure inner AES-GCM decrypt (AAD mismatch or corrupt): {e}")))
    }

    /// Mock Azure wrapKey: identity copy of key bytes.
    fn mock_wrap_key(key: &[u8; 32]) -> Vec<u8> {
        key.to_vec()
    }

    /// Mock Azure unwrapKey: identity copy of key bytes.
    fn mock_unwrap_key(wrapped: &[u8]) -> Result<[u8; 32], BYOKError> {
        if wrapped.len() != MOCK_KEY_WRAP_LEN {
            return Err(BYOKError::EnvelopeError(format!(
                "Azure mock unwrapKey: expected {} bytes, got {}",
                MOCK_KEY_WRAP_LEN,
                wrapped.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(wrapped);
        Ok(key)
    }
}

#[async_trait]
impl KmsProvider for AzureKeyVaultProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AzureKeyVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Azure Key Vault Premium HSM = FIPS 140-2 Level 2 (NIST CMVP #3516).
        // Standard tier is Level 1; CoreLink requires Premium HSM for BYOK.
        FipsLevel::Fips140_2_L2
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        if key_id.provider != KmsProviderKind::AzureKeyVault {
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }

        // Step 1: Generate inner AES-256-GCM key + nonce.
        let inner_key = Self::gen_inner_key()?;
        let nonce = Self::gen_nonce()?;

        // Step 2: Encrypt dek.bytes with AES-GCM, AAD = JSON ctx.
        let aad = context_to_aad(encryption_context);
        let inner_ct = Self::aes_gcm_encrypt(&inner_key, &nonce, &dek.bytes, &aad)?;

        // Step 3: Wrap inner_key via Azure wrapKey (mock: identity).
        let azure_wrapped_key = if self.mock {
            Self::mock_wrap_key(&inner_key)
        } else {
            return Err(BYOKError::Provider(
                "AzureKeyVaultProvider production mode not wired (SDK deferred; use mock for CI)".to_string(),
            ));
        };

        // Step 4: Store: azure_wrapped_key || inner_ct in ciphertext field.
        // Layout: [0..32] = azure_wrapped_key, [32..] = nonce || gcm_ct_with_tag
        let mut ciphertext = azure_wrapped_key;
        ciphertext.extend_from_slice(&inner_ct);

        Ok(WrappedDek {
            provider: KmsProviderKind::AzureKeyVault,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: encryption_context.cloned(),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::AzureKeyVault {
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultProvider received WrappedDek with wrong provider: {:?}",
                wrapped.provider
            )));
        }

        if wrapped.ciphertext.len() < MOCK_KEY_WRAP_LEN + NONCE_LEN + 16 {
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultProvider: ciphertext too short: {} bytes",
                wrapped.ciphertext.len()
            )));
        }

        // Split ciphertext: [0..MOCK_KEY_WRAP_LEN] = azure_wrapped_key;
        // [MOCK_KEY_WRAP_LEN..] = nonce || gcm_ct_with_tag.
        let (azure_wrapped_key, inner_ct) = wrapped.ciphertext.split_at(MOCK_KEY_WRAP_LEN);

        // Step 1: Unwrap inner_key via Azure unwrapKey (mock: identity).
        let inner_key = if self.mock {
            Self::mock_unwrap_key(azure_wrapped_key)?
        } else {
            return Err(BYOKError::Provider(
                "AzureKeyVaultProvider production mode not wired".to_string(),
            ));
        };

        // Step 2: Decrypt AES-GCM with AAD = encryption_context.
        // TAG MISMATCH = AAD tampered → GCM decryption fails → BYOKError::AesGcm.
        let aad = context_to_aad(wrapped.encryption_context.as_ref());
        let plaintext = Self::aes_gcm_decrypt(&inner_key, inner_ct, &aad)?;

        if plaintext.len() != 32 {
            return Err(BYOKError::EnvelopeError(format!(
                "Azure unwrap: DEK length {} != 32",
                plaintext.len()
            )));
        }

        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);
        Ok(Dek { bytes })
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::AzureKeyVault {
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }
        if self.mock {
            return Ok(KmsAccessStatus::Ok);
        }
        Err(BYOKError::Provider(
            "AzureKeyVaultProvider production check_access not wired".to_string(),
        ))
    }
}

/// Serialize `encryption_context` to AAD bytes using RFC 8785 JCS
/// canonicalization so the same logical context always produces
/// byte-identical AAD regardless of JSON serializer field order
/// (GAP-34 — SOC 2 Type I fix; INV-BYOK-CRYPTO-SOVEREIGNTY).
fn context_to_aad(ctx: Option<&serde_json::Value>) -> Vec<u8> {
    match ctx {
        None => vec![],
        Some(v) => serde_jcs::to_vec(v).unwrap_or_default(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::KmsProvider;

    fn gcp_key_id() -> KmsKeyId {
        KmsKeyId {
            provider: KmsProviderKind::AzureKeyVault,
            key_arn_or_id: "https://myvault.vault.azure.net/keys/mykey".to_string(),
            region: "eastus".to_string(),
        }
    }

    #[test]
    fn fips_level_is_140_2_l2() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        assert_eq!(p.fips_level(), FipsLevel::Fips140_2_L2);
    }

    #[test]
    fn provider_kind_is_azure() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        assert_eq!(p.provider_kind(), KmsProviderKind::AzureKeyVault);
    }

    #[tokio::test]
    async fn wrap_unwrap_roundtrip_no_ctx() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        let dek = Dek::generate().expect("entropy");
        let orig = dek.bytes;
        let wrapped = p.wrap_dek(&dek, &gcp_key_id(), None).await.expect("wrap");
        let unwrapped = p.unwrap_dek(&wrapped).await.expect("unwrap");
        assert_eq!(orig, unwrapped.bytes);
    }

    #[tokio::test]
    async fn wrap_unwrap_roundtrip_with_ctx() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        let dek = Dek::generate().expect("entropy");
        let orig = dek.bytes;
        let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H2"});
        let wrapped = p.wrap_dek(&dek, &gcp_key_id(), Some(&ctx)).await.expect("wrap");
        let unwrapped = p.unwrap_dek(&wrapped).await.expect("unwrap");
        assert_eq!(orig, unwrapped.bytes);
    }

    #[tokio::test]
    async fn aad_binding_enforced_on_tamper() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        let dek = Dek::generate().expect("entropy");
        let ctx_a = serde_json::json!({"tenant_id": "T1"});
        let wrapped = p.wrap_dek(&dek, &gcp_key_id(), Some(&ctx_a)).await.expect("wrap");

        // Tamper: change encryption_context → AES-GCM tag mismatch on unwrap.
        let tampered = WrappedDek {
            encryption_context: Some(serde_json::json!({"tenant_id": "ATTACKER"})),
            ..wrapped
        };
        let result = p.unwrap_dek(&tampered).await;
        assert!(result.is_err(), "AES-GCM AAD binding must reject tampered context");
    }

    #[tokio::test]
    async fn wrong_provider_rejected_wrap() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        let bad_key_id = KmsKeyId {
            provider: KmsProviderKind::GcpKms,
            key_arn_or_id: "projects/p/locations/l/keyRings/r/cryptoKeys/k".to_string(),
            region: "us-east1".to_string(),
        };
        let dek = Dek::generate().expect("entropy");
        let result = p.wrap_dek(&dek, &bad_key_id, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_access_ok_mock() {
        let p = AzureKeyVaultProvider::new_mock("eastus");
        let status = p.check_access(&gcp_key_id()).await.expect("check");
        assert_eq!(status, KmsAccessStatus::Ok);
    }

    // GAP-34: verify JCS AAD is deterministic regardless of JSON key order.
    // `serde_json::Value::Object` preserves insertion order on round-trip
    // but serde_jcs MUST produce the same bytes for any permutation of the
    // same logical object.
    #[test]
    fn context_to_aad_jcs_deterministic_across_key_order() {
        // Two JSON objects with the same keys/values but different insertion order.
        let v1 = serde_json::json!({"tenant_id": "T1", "blob_hash": "H2"});
        let v2 = serde_json::json!({"blob_hash": "H2", "tenant_id": "T1"});
        let aad1 = context_to_aad(Some(&v1));
        let aad2 = context_to_aad(Some(&v2));
        assert_eq!(
            aad1, aad2,
            "JCS AAD must be byte-identical regardless of JSON key insertion order"
        );
        // Also confirm non-empty.
        assert!(!aad1.is_empty());
    }

    #[test]
    fn context_to_aad_none_is_empty() {
        let empty: Vec<u8> = vec![];
        assert_eq!(context_to_aad(None), empty);
    }
}
