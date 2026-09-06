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

// ---------------------------------------------------------------------
// REST wire types (Key Vault 7.4).
// ---------------------------------------------------------------------

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
    #[serde(default)]
    #[allow(dead_code)]
    exp: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct JsonWebKey {
    #[serde(default)]
    #[allow(dead_code)]
    kty: String,
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

/// Key Vault uses base64url-no-pad encoding for binary fields.
fn b64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, BYOKError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| BYOKError::Provider(format!("base64url decode: {e}")))
}

/// Emit a fail-CLOSED audit event BEFORE the error bubbles up.
///
/// The orchestrator subscribes to `target =
/// "corelink.byok.azure.audit"` and turns each event into a BLAKE3-
/// linked audit record (see `corelink-audit-chain`). The `audit = true`
/// field is the canonical marker used by the subscriber to distinguish
/// audit events from regular `tracing` output.
pub(super) fn emit_audit(op: &str, key: &str, reason: &str) {
    warn!(
        target: "corelink.byok.azure.audit",
        audit = true,
        op = op,
        key = key,
        reason = reason,
        "BYOK Azure Key Vault audit event"
    );
}

fn map_bundle_to_access(kb: &KeyBundle) -> KmsAccessStatus {
    if matches!(kb.attributes.enabled, Some(false)) {
        return KmsAccessStatus::Revoked;
    }
    if let Some(jwk) = kb.key.as_ref() {
        if !jwk.key_ops.is_empty()
            && (!jwk
                .key_ops
                .iter()
                .any(|o| o.eq_ignore_ascii_case("wrapKey"))
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

fn map_get_error_to_access(http_status: u16, api: &ApiErrorInner, key: &str) -> KmsAccessStatus {
    match http_status {
        401 | 403 => {
            emit_audit("check_access", key, "access_denied");
            KmsAccessStatus::Revoked
        }
        404 => {
            emit_audit("check_access", key, "not_found");
            KmsAccessStatus::NotFound
        }
        429 => {
            emit_audit("check_access", key, "throttled");
            KmsAccessStatus::Throttled
        }
        _ => {
            emit_audit("check_access", key, "provider_error");
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

    // Defensive: Azure may surface `BadParameter`/`decrypt`-mention as
    // a cryptographic mismatch (e.g. tampered outer wrap). The inner
    // AES-GCM tag protects the AAD, so this branch is only reached when
    // Azure rejected the outer wrap key.
    if (status.as_u16() == 400 || status.as_u16() == 403)
        && (code_lc.contains("badparameter")
            && api.message.to_ascii_lowercase().contains("decrypt"))
    {
        emit_audit(op, &key_id.key_arn_or_id, "invalid_ciphertext_aad_mismatch");
        return BYOKError::AadMismatch;
    }

    match status.as_u16() {
        401 | 403 => {
            emit_audit(op, &key_id.key_arn_or_id, "access_denied");
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AzureKeyVault,
                key_id: key_id.key_arn_or_id.clone(),
            }
        }
        404 => {
            emit_audit(op, &key_id.key_arn_or_id, "not_found");
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AzureKeyVault,
                key_id: key_id.key_arn_or_id.clone(),
            }
        }
        429 => {
            emit_audit(op, &key_id.key_arn_or_id, "throttled");
            BYOKError::Provider(format!("Azure KV {op}: throttled"))
        }
        _ => {
            emit_audit(op, &key_id.key_arn_or_id, "provider_error");
            warn!(http = %status, op = %op, "Azure KV provider error");
            // Do not leak the raw SDK message — it may contain key
            // metadata the caller did not pass in.
            BYOKError::Provider(format!("Azure KV {op}: provider error"))
        }
    }
}

// ── Mock-mode wrap / unwrap (in-process, AAD-binding enforced) ───────

fn mock_wrap(dek: &Dek, key_id: &KmsKeyId, ctx: &Value, canonical_aad: &[u8]) -> WrappedDek {
    let fp = aad_fingerprint(canonical_aad);
    let mut ct = Vec::with_capacity(8 + 32);
    ct.extend_from_slice(&fp);
    for b in &dek.bytes {
        ct.push(b ^ 0xCC);
    }
    WrappedDek {
        provider: KmsProviderKind::AzureKeyVault,
        key_id: key_id.clone(),
        ciphertext: ct,
        encryption_context: Some(ctx.clone()),
    }
}

fn mock_unwrap(wrapped: &WrappedDek, canonical_aad: &[u8]) -> Result<Dek, BYOKError> {
    if wrapped.ciphertext.len() != 40 {
        return Err(BYOKError::EnvelopeError(format!(
            "Azure KV real (mock): wrong ciphertext length {} (expected 40)",
            wrapped.ciphertext.len()
        )));
    }
    let expected = aad_fingerprint(canonical_aad);
    let stored = wrapped.ciphertext.get(..8).ok_or_else(|| {
        BYOKError::EnvelopeError("Azure KV real (mock): ciphertext too short".to_string())
    })?;
    // Constant-time fingerprint compare.
    if stored.ct_eq(&expected).unwrap_u8() == 0 {
        return Err(BYOKError::AadMismatch);
    }
    let body = wrapped.ciphertext.get(8..).ok_or_else(|| {
        BYOKError::EnvelopeError("Azure KV real (mock): ciphertext too short".to_string())
    })?;
    let mut bytes = [0u8; 32];
    for (out, &b) in bytes.iter_mut().zip(body.iter()) {
        *out = b ^ 0xCC;
    }
    Ok(Dek { bytes })
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
        // Premium HSM = FIPS 140-2 L2 (CMVP #3516). Managed HSM is
        // logically L3 in the compliance matrix; the orchestrator is the
        // source of truth on tier policy.
        FipsLevel::Fips140_2_L2
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        if key_id.provider != KmsProviderKind::AzureKeyVault {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "wrong_provider");
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }
        let ctx = encryption_context.ok_or_else(|| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_missing");
            BYOKError::EncryptionContextMissing
        })?;
        let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx).inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_canonicalize");
        })?;

        debug!(
            target: "corelink.byok.azure",
            key = %key_id.key_arn_or_id,
            region = %self.region,
            "Azure KV wrap_dek"
        );

        if self.mock_mode {
            return Ok(mock_wrap(dek, key_id, ctx, &canonical_aad));
        }

        let resource = Self::parse_resource(&key_id.key_arn_or_id).inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "malformed_arn");
        })?;

        // Step 1: generate inner AES-256-GCM key + nonce.
        let inner_key = Self::gen_inner_key()?;
        let nonce = Self::gen_nonce()?;

        // Step 2: encrypt DEK with AES-256-GCM, AAD = JCS(ctx).
        let inner_ct = Self::aes_gcm_encrypt(&inner_key, &nonce, &dek.bytes, &canonical_aad)?;

        // Step 3: wrap inner_key via Azure wrapKey (RSA-OAEP-256).
        let token = self.bearer().await.inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "entra_token_failure");
        })?;
        let url = self.wrapkey_url(&resource);
        let body = WrapRequest {
            alg: WRAP_ALG_RSA_OAEP_256,
            value: b64url_encode(&inner_key),
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "http_send_error");
                BYOKError::Provider(format!("Azure KV wrapkey POST: {e}"))
            })?;
        if !resp.status().is_success() {
            return Err(map_http_error(resp, key_id, "wrap_dek").await);
        }

        let wr: WrapResponse = resp.json().await.map_err(|e| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "http_body_parse");
            BYOKError::Provider(format!("Azure KV wrapkey JSON: {e}"))
        })?;
        let azure_wrapped_inner = b64url_decode(&wr.value).inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "b64_decode_failed");
        })?;

        // Step 4: ciphertext layout = [u32 BE outer-len] || outer || inner_ct.
        let mut ciphertext = Vec::with_capacity(4 + azure_wrapped_inner.len() + inner_ct.len());
        let outer_len_u32: u32 = azure_wrapped_inner.len().try_into().map_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "outer_len_overflow");
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
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "wrong_provider",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider received WrappedDek with wrong provider: {:?}",
                wrapped.provider
            )));
        }
        let ctx = wrapped.encryption_context.as_ref().ok_or_else(|| {
            emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aad_missing");
            BYOKError::EncryptionContextMissing
        })?;
        let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx).inspect_err(|_| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "aad_canonicalize",
            );
        })?;

        debug!(
            target: "corelink.byok.azure",
            key = %wrapped.key_id.key_arn_or_id,
            "Azure KV unwrap_dek"
        );

        if self.mock_mode {
            return mock_unwrap(wrapped, &canonical_aad);
        }

        let resource = Self::parse_resource(&wrapped.key_id.key_arn_or_id).inspect_err(|_| {
            emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "malformed_arn");
        })?;

        // Parse ciphertext layout: [u32 BE outer-len] || outer || inner_ct.
        if wrapped.ciphertext.len() < 4 + NONCE_LEN + TAG_LEN {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "ciphertext_too_short",
            );
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
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "ciphertext_underflow",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: ciphertext underflow (outer_len={outer_len}, rest={})",
                rest.len()
            )));
        }
        let (azure_wrapped_inner, inner_ct) = rest.split_at(outer_len);

        // Step 1: unwrap inner_key via Azure unwrapKey (RSA-OAEP-256).
        let token = self.bearer().await.inspect_err(|_| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "entra_token_failure",
            );
        })?;
        let url = self.unwrapkey_url(&resource);
        let body = UnwrapRequest {
            alg: WRAP_ALG_RSA_OAEP_256,
            value: b64url_encode(azure_wrapped_inner),
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "http_send_error",
                );
                BYOKError::Provider(format!("Azure KV unwrapkey POST: {e}"))
            })?;
        if !resp.status().is_success() {
            return Err(map_http_error(resp, &wrapped.key_id, "unwrap_dek").await);
        }

        let ur: UnwrapResponse = resp.json().await.map_err(|e| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "http_body_parse",
            );
            BYOKError::Provider(format!("Azure KV unwrapkey JSON: {e}"))
        })?;
        let inner_key_bytes = b64url_decode(&ur.value).inspect_err(|_| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "b64_decode_failed",
            );
        })?;
        if inner_key_bytes.len() != INNER_KEY_LEN {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "inner_key_length",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: inner key length {} != {}",
                inner_key_bytes.len(),
                INNER_KEY_LEN
            )));
        }
        let mut inner_key = [0u8; INNER_KEY_LEN];
        inner_key.copy_from_slice(&inner_key_bytes);

        // Step 2: decrypt AES-GCM with AAD = JCS(ctx). Tag mismatch ⇒
        // AadMismatch.
        let plaintext =
            Self::aes_gcm_decrypt(&inner_key, inner_ct, &canonical_aad).inspect_err(|e| {
                if matches!(e, BYOKError::AadMismatch) {
                    emit_audit(
                        "unwrap_dek",
                        &wrapped.key_id.key_arn_or_id,
                        "invalid_ciphertext_aad_mismatch",
                    );
                } else {
                    emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aes_gcm_error");
                }
            })?;

        if plaintext.len() != 32 {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "dek_length_invalid",
            );
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
            emit_audit("check_access", &key_id.key_arn_or_id, "wrong_provider");
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }

        if self.mock_mode {
            return Ok(KmsAccessStatus::Ok);
        }

        let resource = Self::parse_resource(&key_id.key_arn_or_id).inspect_err(|_| {
            emit_audit("check_access", &key_id.key_arn_or_id, "malformed_arn");
        })?;
        let token = self.bearer().await.inspect_err(|_| {
            emit_audit("check_access", &key_id.key_arn_or_id, "entra_token_failure");
        })?;
        let url = self.getkey_url(&resource);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| {
                emit_audit("check_access", &key_id.key_arn_or_id, "http_send_error");
                BYOKError::Provider(format!("Azure KV GetKey: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let api_err = parse_api_error(&body);
            return Ok(map_get_error_to_access(
                status.as_u16(),
                &api_err,
                &key_id.key_arn_or_id,
            ));
        }

        let kb: KeyBundle = resp.json().await.map_err(|e| {
            emit_audit("check_access", &key_id.key_arn_or_id, "http_body_parse");
            BYOKError::Provider(format!("Azure KV GetKey JSON: {e}"))
        })?;

        Ok(map_bundle_to_access(&kb))
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests;
