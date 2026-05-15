//! `AzureKeyVaultRealProvider` — real Key Vault REST 7.4 client (R2-8).
//!
//! Replaces the [`crate::AzureKeyVaultProvider::new_production`] pending
//! stub with a working HTTPS client that:
//!
//! - Authenticates via Microsoft Entra ID (see [`crate::entra`]). Service
//!   principal (client secret) or federated workload identity.
//! - Validates the customer key resource URI against the canonical Key Vault
//!   regex (`https://{vault}.vault.azure.net/keys/{name}[/{version}]`).
//! - Implements the **composite-key AAD binding** (ADR-S14-001): Azure
//!   `wrapKey` / `unwrapKey` does **not** accept additional authenticated
//!   data. CoreLink wraps an inner AES-256-GCM key (which itself encrypts
//!   the DEK with the encryption context bound as AAD); the GCM tag on
//!   `unwrap_dek` enforces tamper detection (`BYOKError::AadMismatch`).
//! - Calls `POST /keys/{name}/{version}/wrapkey` with `alg=RSA-OAEP-256`
//!   for the outer wrap (the inner-AES-GCM key, 32 bytes).
//! - Calls `POST /keys/{name}/{version}/unwrapkey` for the outer unwrap.
//! - Calls `GET /keys/{name}` (or `/keys/{name}/{version}`) for
//!   [`KmsProvider::check_access`], mapping the `attributes.enabled` flag
//!   and HTTP status to the canonical [`KmsAccessStatus`] enum.
//!
//! # FIPS tier (140-2 L2 default; L3 with Managed HSM)
//!
//! Azure Key Vault host pattern selects the tier:
//!
//! | Host suffix              | Tier                  | FIPS level (reported)      |
//! |--------------------------|-----------------------|----------------------------|
//! | `.vault.azure.net`       | Premium HSM           | `Fips140_2_L2` (CMVP #3516)|
//! | `.managedhsm.azure.net`  | Managed HSM / Dedicated| `Fips140_2_L2` (logical L3 in matrix)|
//!
//! The static [`KmsProvider::fips_level`] returns L2 — the orchestrator is
//! the source of truth on tier policy and may upgrade the reported level
//! after a `check_access` call confirms Managed HSM backing. See
//! `compliance/byok-fips-matrix.md`.
//!
//! # Endpoint
//!
//! The `endpoint_override` constructor parameter exists only for `wiremock`;
//! production callers pass `None` (the per-key vault URL is used directly).
//!
//! # Errors
//!
//! All HTTP / parse / auth errors map to [`BYOKError`] variants. Entra
//! credential material (client secret, federated SA JWT, bearer token) is
//! never included in error messages or `tracing` spans.

use aes_gcm::aead::{Aead, KeyInit, generic_array::GenericArray};
use aes_gcm::Aes256Gcm;
use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use corelink_byok::{
    BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

use crate::entra::EntraCredentials;
use crate::key_resource::{parse_kv_resource, KvKeyResource};

/// Key Vault REST API version used for wrap / unwrap / get.
const KV_API_VERSION: &str = "7.4";

/// Outer wrap algorithm (RSA-OAEP-256). Symmetric Managed HSM keys could
/// alternatively use `AES-KW`, but CoreLink standardizes on RSA-OAEP-256
/// because (a) it works across both Premium HSM and Managed HSM, and (b)
/// the inner AES-GCM layer already provides confidentiality + AAD binding.
const WRAP_ALG_RSA_OAEP_256: &str = "RSA-OAEP-256";

/// Inner AES-GCM key size (256-bit).
const INNER_KEY_LEN: usize = 32;
/// AES-GCM nonce length (96-bit).
const NONCE_LEN: usize = 12;
/// AES-GCM authentication tag length (128-bit).
const TAG_LEN: usize = 16;

/// Real Azure Key Vault provider — production REST client.
///
/// Construct via [`AzureKeyVaultRealProvider::new`] (Entra ID auto-detect)
/// or the test-only injection constructors.
#[derive(Debug, Clone)]
pub struct AzureKeyVaultRealProvider {
    http: reqwest::Client,
    creds: EntraCredentials,
    region: String,
    /// Optional endpoint override for tests; production reads the host from
    /// the key URI passed by the customer.
    endpoint_override: Option<String>,
}

impl AzureKeyVaultRealProvider {
    /// Construct a real Azure KV provider using Entra ID auto-detection.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if HTTP client init fails or no
    /// Entra ID credentials can be resolved.
    pub fn new(region: &str) -> Result<Self, BYOKError> {
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
        })
    }

    /// Test-only constructor that injects credentials + a wiremock endpoint.
    #[doc(hidden)]
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
        }
    }

    fn parse_resource(uri: &str) -> Result<KvKeyResource, BYOKError> {
        parse_kv_resource(uri).ok_or_else(|| {
            BYOKError::Provider(format!(
                "malformed Azure Key Vault key URI: '{uri}' (expected \
                 https://{{vault}}.vault.azure.net/keys/{{name}}[/{{version}}])"
            ))
        })
    }

    /// Build the base URL for a given parsed key resource.
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

    /// Generate a random 32-byte inner AES key.
    fn gen_inner_key() -> Result<[u8; INNER_KEY_LEN], BYOKError> {
        let mut key = [0u8; INNER_KEY_LEN];
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

    /// AES-256-GCM encrypt with AAD.
    fn aes_gcm_encrypt(
        key: &[u8; INNER_KEY_LEN],
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

    /// AES-256-GCM decrypt with AAD. Returns plaintext.
    fn aes_gcm_decrypt(
        key: &[u8; INNER_KEY_LEN],
        nonce_and_ct: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, BYOKError> {
        if nonce_and_ct.len() < NONCE_LEN + TAG_LEN {
            return Err(BYOKError::AesGcm("inner ciphertext too short".to_string()));
        }
        let (nonce_bytes, ct) = nonce_and_ct.split_at(NONCE_LEN);
        let nonce = GenericArray::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
        let payload = aes_gcm::aead::Payload { msg: ct, aad };
        cipher.decrypt(nonce, payload).map_err(|_| {
            // Tag mismatch ⇒ AAD or ciphertext tampered. Surface as AadMismatch
            // (covers all in-flight tamper vectors enforced by GCM).
            BYOKError::AadMismatch
        })
    }
}

// ---------------------------------------------------------------------------
// REST wire types (Key Vault 7.4).
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct WrapRequest<'a> {
    alg: &'a str,
    /// Base64-URL-no-padding encoded plaintext key (Key Vault spec).
    value: String,
}

#[derive(Debug, Deserialize)]
struct WrapResponse {
    /// Base64-URL-no-padding encoded ciphertext.
    value: String,
    #[serde(default)]
    #[allow(dead_code)]
    kid: String,
}

#[derive(Debug, Serialize)]
struct UnwrapRequest<'a> {
    alg: &'a str,
    /// Base64-URL-no-padding encoded ciphertext.
    value: String,
}

#[derive(Debug, Deserialize)]
struct UnwrapResponse {
    /// Base64-URL-no-padding encoded plaintext.
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct KeyBundle {
    #[serde(default)]
    attributes: KeyAttributes,
    #[serde(default)]
    key: Option<JsonWebKey>,
}

#[derive(Debug, Deserialize, Default)]
struct KeyAttributes {
    #[serde(default)]
    enabled: Option<bool>,
    /// Unix-seconds expiry; None ⇒ never.
    #[serde(default)]
    #[allow(dead_code)]
    exp: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct JsonWebKey {
    #[serde(default)]
    #[allow(dead_code)]
    kty: String,
    /// Array of permitted key ops, e.g. `["wrapKey","unwrapKey"]`.
    #[serde(default)]
    key_ops: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorEnvelope {
    #[serde(default)]
    error: ApiErrorInner,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorInner {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

fn parse_api_error(body: &str) -> ApiErrorInner {
    serde_json::from_str::<ApiErrorEnvelope>(body)
        .map(|e| e.error)
        .unwrap_or_default()
}

/// Serialize encryption_context to AAD bytes (UTF-8 JSON).
fn context_to_aad(ctx: Option<&serde_json::Value>) -> Vec<u8> {
    match ctx {
        None => vec![],
        Some(v) => serde_json::to_vec(v).unwrap_or_default(),
    }
}

/// Key Vault uses base64url-no-pad encoding for binary fields.
fn b64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, BYOKError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| BYOKError::Provider(format!("base64url decode: {e}")))
}

#[async_trait]
impl KmsProvider for AzureKeyVaultRealProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AzureKeyVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Premium HSM = FIPS 140-2 L2 (CMVP #3516). Managed HSM is logically
        // L3 in the compliance matrix; the orchestrator handles tier policy.
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
                "AzureKeyVaultRealProvider: wrong provider {:?}",
                key_id.provider
            )));
        }
        let ctx = encryption_context.ok_or(BYOKError::EncryptionContextMissing)?;
        let resource = Self::parse_resource(&key_id.key_arn_or_id)?;

        // Step 1: generate inner AES-256-GCM key + nonce.
        let inner_key = Self::gen_inner_key()?;
        let nonce = Self::gen_nonce()?;

        // Step 2: encrypt DEK with AES-256-GCM, AAD = JSON(ctx).
        let aad = context_to_aad(Some(ctx));
        let inner_ct = Self::aes_gcm_encrypt(&inner_key, &nonce, &dek.bytes, &aad)?;

        // Step 3: wrap inner_key via Azure wrapKey (RSA-OAEP-256).
        let token = self.bearer().await?;
        let url = self.wrapkey_url(&resource);
        let body = WrapRequest {
            alg: WRAP_ALG_RSA_OAEP_256,
            value: b64url_encode(&inner_key),
        };

        debug!(key = %key_id.key_arn_or_id, "Azure KV wrap_dek");

        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("Azure KV wrapkey POST: {e}")))?;

        if !resp.status().is_success() {
            return Err(map_http_error(resp, key_id, "wrapkey").await);
        }

        let wr: WrapResponse = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("Azure KV wrapkey JSON: {e}")))?;
        let azure_wrapped_inner = b64url_decode(&wr.value)?;

        // Step 4: ciphertext layout = [u32 BE outer-len] || outer || inner_ct
        // outer = Azure-wrapped inner AES key (RSA-OAEP-256 → ~256-512 bytes
        // depending on RSA key size); inner_ct = nonce || gcm_ct || gcm_tag.
        let mut ciphertext =
            Vec::with_capacity(4 + azure_wrapped_inner.len() + inner_ct.len());
        let outer_len_u32: u32 = azure_wrapped_inner.len().try_into().map_err(|_| {
            BYOKError::EnvelopeError("outer wrapped key length > u32::MAX".to_string())
        })?;
        ciphertext.extend_from_slice(&outer_len_u32.to_be_bytes());
        ciphertext.extend_from_slice(&azure_wrapped_inner);
        ciphertext.extend_from_slice(&inner_ct);

        Ok(WrappedDek {
            provider: KmsProviderKind::AzureKeyVault,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: Some(ctx.clone()),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::AzureKeyVault {
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider: wrong provider {:?}",
                wrapped.provider
            )));
        }
        let ctx = wrapped
            .encryption_context
            .as_ref()
            .ok_or(BYOKError::EncryptionContextMissing)?;
        let resource = Self::parse_resource(&wrapped.key_id.key_arn_or_id)?;

        // Parse ciphertext layout: [u32 BE outer-len] || outer || inner_ct.
        if wrapped.ciphertext.len() < 4 + NONCE_LEN + TAG_LEN {
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: ciphertext too short ({} bytes)",
                wrapped.ciphertext.len()
            )));
        }
        let (len_bytes, rest) = wrapped.ciphertext.split_at(4);
        let outer_len = u32::from_be_bytes([
            *len_bytes.first().unwrap_or(&0),
            *len_bytes.get(1).unwrap_or(&0),
            *len_bytes.get(2).unwrap_or(&0),
            *len_bytes.get(3).unwrap_or(&0),
        ]) as usize;
        if rest.len() < outer_len + NONCE_LEN + TAG_LEN {
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: ciphertext underflow (outer_len={outer_len}, rest={})",
                rest.len()
            )));
        }
        let (azure_wrapped_inner, inner_ct) = rest.split_at(outer_len);

        // Step 1: unwrap inner_key via Azure unwrapKey (RSA-OAEP-256).
        let token = self.bearer().await?;
        let url = self.unwrapkey_url(&resource);
        let body = UnwrapRequest {
            alg: WRAP_ALG_RSA_OAEP_256,
            value: b64url_encode(azure_wrapped_inner),
        };

        debug!(key = %wrapped.key_id.key_arn_or_id, "Azure KV unwrap_dek");

        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("Azure KV unwrapkey POST: {e}")))?;

        if !resp.status().is_success() {
            return Err(map_http_error(resp, &wrapped.key_id, "unwrapkey").await);
        }

        let ur: UnwrapResponse = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("Azure KV unwrapkey JSON: {e}")))?;
        let inner_key_bytes = b64url_decode(&ur.value)?;
        if inner_key_bytes.len() != INNER_KEY_LEN {
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: inner key length {} != {}",
                inner_key_bytes.len(),
                INNER_KEY_LEN
            )));
        }
        let mut inner_key = [0u8; INNER_KEY_LEN];
        inner_key.copy_from_slice(&inner_key_bytes);

        // Step 2: decrypt AES-GCM with AAD = JSON(ctx). Tag mismatch ⇒ AadMismatch.
        let aad = context_to_aad(Some(ctx));
        let plaintext = Self::aes_gcm_decrypt(&inner_key, inner_ct, &aad)?;

        if plaintext.len() != 32 {
            return Err(BYOKError::DekLengthInvalid {
                got: plaintext.len(),
            });
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);
        Ok(Dek { bytes })
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::AzureKeyVault {
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }
        let resource = Self::parse_resource(&key_id.key_arn_or_id)?;
        let token = self.bearer().await?;
        let url = self.getkey_url(&resource);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("Azure KV GetKey: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let api_err = parse_api_error(&body);
            return Ok(map_get_error_to_access(status.as_u16(), &api_err));
        }

        let kb: KeyBundle = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("Azure KV GetKey JSON: {e}")))?;

        Ok(map_bundle_to_access(&kb))
    }
}

fn map_bundle_to_access(kb: &KeyBundle) -> KmsAccessStatus {
    match kb.attributes.enabled {
        Some(false) => return KmsAccessStatus::Revoked,
        Some(true) => {}
        None => {}
    }
    // Ensure wrapKey / unwrapKey ops are permitted.
    if let Some(jwk) = kb.key.as_ref() {
        if !jwk.key_ops.is_empty()
            && (!jwk.key_ops.iter().any(|o| o.eq_ignore_ascii_case("wrapKey"))
                || !jwk
                    .key_ops
                    .iter()
                    .any(|o| o.eq_ignore_ascii_case("unwrapKey")))
        {
            return KmsAccessStatus::Revoked;
        }
    }
    KmsAccessStatus::Ok
}

fn map_get_error_to_access(http_status: u16, api: &ApiErrorInner) -> KmsAccessStatus {
    match http_status {
        401 | 403 => KmsAccessStatus::Revoked,
        404 => KmsAccessStatus::NotFound,
        429 => KmsAccessStatus::Throttled,
        _ => {
            warn!(http = http_status, code = %api.code, "Azure KV GetKey error");
            KmsAccessStatus::ApiError(http_status)
        }
    }
}

async fn map_http_error(resp: reqwest::Response, key_id: &KmsKeyId, op: &str) -> BYOKError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let api = parse_api_error(&body);
    let code_lc = api.code.to_ascii_lowercase();

    // Azure KV surfaces tamper / cryptographic mismatch as
    // `BadParameter` or `Forbidden` with messages around `decrypt` / `wrap`;
    // map cleanly to AadMismatch where the GCM tag did not protect us
    // (only used here if the inner AES-GCM happened to succeed somehow but
    // Azure rejected the outer wrap key — defensive).
    if (status.as_u16() == 400 || status.as_u16() == 403)
        && (code_lc.contains("badparameter") && api.message.to_ascii_lowercase().contains("decrypt"))
    {
        return BYOKError::AadMismatch;
    }

    match status.as_u16() {
        401 | 403 => BYOKError::CmkRevoked {
            provider: KmsProviderKind::AzureKeyVault,
            key_id: key_id.key_arn_or_id.clone(),
        },
        404 => BYOKError::CmkRevoked {
            provider: KmsProviderKind::AzureKeyVault,
            key_id: key_id.key_arn_or_id.clone(),
        },
        _ => {
            warn!(http = %status, op = %op, "Azure KV provider error");
            BYOKError::Provider(format!("Azure KV {op} HTTP {status}: {}", api.message))
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn fips_static_l2() {
        let http = reqwest::Client::new();
        let creds = EntraCredentials::for_test_static("t");
        let p = AzureKeyVaultRealProvider::for_test(http, creds, "eastus", "https://x.invalid");
        assert_eq!(p.fips_level(), FipsLevel::Fips140_2_L2);
        assert_eq!(p.provider_kind(), KmsProviderKind::AzureKeyVault);
        assert_eq!(p.region(), "eastus");
    }

    #[test]
    fn access_mapping() {
        let mut kb = KeyBundle::default();
        kb.attributes.enabled = Some(true);
        assert_eq!(map_bundle_to_access(&kb), KmsAccessStatus::Ok);

        kb.attributes.enabled = Some(false);
        assert_eq!(map_bundle_to_access(&kb), KmsAccessStatus::Revoked);

        // enabled=true but key_ops missing wrap/unwrap.
        kb.attributes.enabled = Some(true);
        kb.key = Some(JsonWebKey {
            kty: "RSA-HSM".to_string(),
            key_ops: vec!["sign".to_string()],
        });
        assert_eq!(map_bundle_to_access(&kb), KmsAccessStatus::Revoked);
    }

    #[test]
    fn http_to_access() {
        let api = ApiErrorInner::default();
        assert_eq!(map_get_error_to_access(401, &api), KmsAccessStatus::Revoked);
        assert_eq!(map_get_error_to_access(403, &api), KmsAccessStatus::Revoked);
        assert_eq!(map_get_error_to_access(404, &api), KmsAccessStatus::NotFound);
        assert_eq!(
            map_get_error_to_access(429, &api),
            KmsAccessStatus::Throttled
        );
        assert!(matches!(
            map_get_error_to_access(500, &api),
            KmsAccessStatus::ApiError(500)
        ));
    }

}
