use super::{aad_fingerprint, canonicalize_aad_to_string_map, resolve_fips_host};
use aes_gcm::aead::{Aead, KeyInit, Nonce};
use aes_gcm::Aes256Gcm;
use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use subtle::ConstantTimeEq;
use tracing::{debug, warn};

use crate::{
    BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

use super::super::entra::EntraCredentials;
use super::super::key_resource::{parse_kv_resource, KvKeyResource};

/// Key Vault REST API version used for wrap / unwrap / get.
const KV_API_VERSION: &str = "7.4";

/// Outer wrap algorithm (RSA-OAEP-256). Symmetric Managed HSM keys could
/// alternatively use `AES-KW`, but CoreLink standardizes on RSA-OAEP-256
/// because (a) it works across both Premium HSM and Managed HSM, and (b)
/// the inner AES-GCM layer already provides confidentiality + AAD
/// binding.
const WRAP_ALG_RSA_OAEP_256: &str = "RSA-OAEP-256";

/// Inner AES-GCM key size (256-bit).
const INNER_KEY_LEN: usize = 32;
/// AES-GCM nonce length (96-bit).
const NONCE_LEN: usize = 12;
/// AES-GCM authentication tag length (128-bit).
const TAG_LEN: usize = 16;

/// Production Azure Key Vault provider — REST 7.4 client.
///
/// Construct via [`AzureKeyVaultRealProvider::new`] for Entra ID
/// auto-detection, [`AzureKeyVaultRealProvider::for_test`] for
/// `wiremock`-style integration tests, or
/// [`AzureKeyVaultRealProvider::new_mock`] for in-process unit tests
/// (no HTTP).
///
/// # FIPS enforcement
///
/// `new` MUST be passed a FIPS-eligible vault URL ([`resolve_fips_host`]);
/// non-FIPS hosts are rejected at construction time. The
/// [`AzureKeyVaultRealProvider::resolved_fips_endpoint`] accessor exposes
/// the host string the production client uses, for test assertion.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AzureKeyVaultRealProvider {
    http: reqwest::Client,
    creds: EntraCredentials,
    region: String,
    /// Optional endpoint override for tests; production reads the host
    /// from the key URI passed by the customer.
    endpoint_override: Option<String>,
    /// Resolved FIPS endpoint host string exposed for test assertion
    /// (e.g. `"myvault.vault.azure.net"`).
    resolved_fips_endpoint: String,
    /// Reported FIPS tier suffix (e.g. `"managedhsm.azure.net"`).
    fips_tier_suffix: &'static str,
    /// When `true`, `wrap_dek` / `unwrap_dek` / `check_access` operate
    /// entirely in-process — no HTTP, no Entra token fetch — and
    /// exercise the AAD-binding / fingerprint code paths.
    mock_mode: bool,
}

impl AzureKeyVaultRealProvider {
    /// Construct a production Azure KV provider using Entra ID
    /// auto-detection, bound to a specific FIPS-eligible vault URL.
    ///
    /// # Errors
    ///
    /// - [`BYOKError::Provider`] if `vault_url` is not a FIPS-eligible
    ///   host (see [`resolve_fips_host`]).
    /// - [`BYOKError::Provider`] if HTTP client init fails or no Entra
    ///   ID credentials can be resolved.
    pub fn new(region: &str, vault_url: &str) -> Result<Self, BYOKError> {
        let (host, suffix) = resolve_fips_host(vault_url)?;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| BYOKError::Provider(format!("reqwest client init: {e}")))?;
        let creds = EntraCredentials::detect(http.clone())?;
        Ok(Self {
            http,
            creds,
            region: region.to_string(),
            endpoint_override: None,
            resolved_fips_endpoint: host,
            fips_tier_suffix: suffix,
            mock_mode: false,
        })
    }

    /// Test-only constructor that injects credentials + a wiremock
    /// endpoint. Skips the FIPS-host enforcement (the wiremock URL is
    /// localhost) but pins the reported FIPS endpoint string so URL-
    /// pattern tests can still exercise [`Self::resolved_fips_endpoint`]
    /// via [`Self::new_mock`].
    #[doc(hidden)]
    #[must_use]
    pub fn for_test(
        http: reqwest::Client,
        creds: EntraCredentials,
        region: &str,
        endpoint_override: &str,
    ) -> Self {
        Self {
            http,
            creds,
            region: region.to_string(),
            endpoint_override: Some(endpoint_override.trim_end_matches('/').to_string()),
            resolved_fips_endpoint: "wiremock.vault.azure.net".to_string(),
            fips_tier_suffix: "vault.azure.net",
            mock_mode: false,
        }
    }

    /// In-process mock constructor that does not contact Azure.
    ///
    /// In mock mode, `wrap_dek` / `unwrap_dek` / `check_access` operate
    /// entirely in-process and exercise the AAD-binding code paths
    /// (real AES-256-GCM crypto, real JCS canonicalization, real
    /// constant-time fingerprint compare). FIPS endpoint is reported
    /// as enabled with the requested vault host so URL-pattern tests
    /// still pass.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if `vault_url` is not a
    /// FIPS-eligible host.
    pub fn new_mock(region: &str, vault_url: &str) -> Result<Self, BYOKError> {
        let (host, suffix) = resolve_fips_host(vault_url)?;
        // The HTTP client is constructed but never used in mock mode.
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(1))
            .build()
            .map_err(|e| BYOKError::Provider(format!("reqwest client init: {e}")))?;
        let creds = EntraCredentials::for_test_static("mock-bearer-token-not-used");
        Ok(Self {
            http,
            creds,
            region: region.to_string(),
            endpoint_override: None,
            resolved_fips_endpoint: host,
            fips_tier_suffix: suffix,
            mock_mode: true,
        })
    }

    /// Resolved FIPS endpoint host string, exposed for test assertions.
    ///
    /// Example: `"myvault.vault.azure.net"` (Premium HSM) or
    /// `"corp-hsm.managedhsm.azure.net"` (Managed HSM).
    #[must_use]
    pub fn resolved_fips_endpoint(&self) -> &str {
        &self.resolved_fips_endpoint
    }

    /// Reported FIPS tier suffix (e.g. `"vault.azure.net"` or
    /// `"managedhsm.azure.net"`).
    #[must_use]
    pub const fn fips_tier_suffix(&self) -> &'static str {
        self.fips_tier_suffix
    }

    /// Whether FIPS endpoint is enforced on this provider. Always `true`
    /// for `new` / `new_mock`; `false` only for `for_test` (wiremock).
    #[must_use]
    pub fn fips_endpoint_enforced(&self) -> bool {
        // `for_test` is the only constructor that bypasses FIPS host
        // enforcement, and it sets `endpoint_override = Some(...)`.
        self.endpoint_override.is_none() || self.mock_mode
    }

    fn parse_resource(uri: &str) -> Result<KvKeyResource, BYOKError> {
        parse_kv_resource(uri).ok_or_else(|| {
            BYOKError::Provider(format!(
                "malformed Azure Key Vault key URI: '{uri}' (expected \
                     https://{{vault}}.vault.azure.net/keys/{{name}}[/{{version}}])"
            ))
        })
    }

    fn base_for(&self, resource: &KvKeyResource) -> String {
        match &self.endpoint_override {
            Some(ep) => ep.clone(),
            None => resource.base_url.clone(),
        }
    }

    fn wrapkey_url(&self, resource: &KvKeyResource) -> String {
        let base = self.base_for(resource);
        let key_seg = match &resource.key_version {
            Some(v) => format!("{}/{}", resource.key_name, v),
            None => resource.key_name.clone(),
        };
        format!("{base}/keys/{key_seg}/wrapkey?api-version={KV_API_VERSION}")
    }

    fn unwrapkey_url(&self, resource: &KvKeyResource) -> String {
        let base = self.base_for(resource);
        let key_seg = match &resource.key_version {
            Some(v) => format!("{}/{}", resource.key_name, v),
            None => resource.key_name.clone(),
        };
        format!("{base}/keys/{key_seg}/unwrapkey?api-version={KV_API_VERSION}")
    }

    fn getkey_url(&self, resource: &KvKeyResource) -> String {
        let base = self.base_for(resource);
        let key_seg = match &resource.key_version {
            Some(v) => format!("{}/{}", resource.key_name, v),
            None => resource.key_name.clone(),
        };
        format!("{base}/keys/{key_seg}?api-version={KV_API_VERSION}")
    }

    async fn bearer(&self) -> Result<String, BYOKError> {
        self.creds.access_token().await
    }

    fn gen_inner_key() -> Result<[u8; INNER_KEY_LEN], BYOKError> {
        let mut key = [0u8; INNER_KEY_LEN];
        getrandom::getrandom(&mut key)
            .map_err(|e| BYOKError::EnvelopeError(format!("getrandom inner key: {e}")))?;
        Ok(key)
    }

    fn gen_nonce() -> Result<[u8; NONCE_LEN], BYOKError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce)
            .map_err(|e| BYOKError::EnvelopeError(format!("getrandom nonce: {e}")))?;
        Ok(nonce)
    }

    fn aes_gcm_encrypt(
        key: &[u8; INNER_KEY_LEN],
        nonce: &[u8; NONCE_LEN],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, BYOKError> {
        let cipher = Aes256Gcm::new(key.into());
        let nonce_ga: &Nonce<Aes256Gcm> = nonce.into();
        let payload = aes_gcm::aead::Payload {
            msg: plaintext,
            aad,
        };
        let ct = cipher
            .encrypt(nonce_ga, payload)
            .map_err(|e| BYOKError::AesGcm(format!("azure inner AES-GCM encrypt: {e}")))?;
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ct);
        Ok(result)
    }

    fn aes_gcm_decrypt(
        key: &[u8; INNER_KEY_LEN],
        nonce_and_ct: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, BYOKError> {
        if nonce_and_ct.len() < NONCE_LEN + TAG_LEN {
            return Err(BYOKError::AesGcm("inner ciphertext too short".to_string()));
        }
        let (nonce_bytes, ct) = nonce_and_ct.split_at(NONCE_LEN);
        let nonce: &Nonce<Aes256Gcm> = nonce_bytes
            .try_into()
            .map_err(|_| BYOKError::AesGcm("azure inner nonce length".to_string()))?;
        let cipher = Aes256Gcm::new(key.into());
        let payload = aes_gcm::aead::Payload { msg: ct, aad };
        cipher.decrypt(nonce, payload).map_err(|_| {
            // Tag mismatch ⇒ AAD or ciphertext tampered. Surface as
            // AadMismatch (covers all in-flight tamper vectors enforced
            // by GCM).
            BYOKError::AadMismatch
        })
    }
}

include!("native/part-01.rs");

impl KmsProvider for AzureKeyVaultRealProvider {
    include!("native/part-02.rs");
}
#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests;
