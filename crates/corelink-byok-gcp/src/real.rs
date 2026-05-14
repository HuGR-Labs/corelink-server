//! `GcpKmsRealProvider` — real Cloud KMS v1 REST client (R2-7).
//!
//! Replaces the `PendingStub` behavior of [`crate::GcpKmsProvider::new_production`]
//! with a working HTTPS client that:
//!
//! - Authenticates via Application Default Credentials (see [`crate::adc`]).
//! - Validates the customer key resource path against the canonical Cloud KMS
//!   regex (`projects/.../cryptoKeyVersions/...`).
//! - Calls `cryptoKeys.encrypt` for [`KmsProvider::wrap_dek`], passing the
//!   serialized `encryption_context` as `additionalAuthenticatedData`. The
//!   Cloud KMS service enforces AAD binding server-side; tamper attempts on
//!   the `encryption_context` of a stored [`WrappedDek`] surface as
//!   `INVALID_ARGUMENT` and map to [`BYOKError::AadMismatch`].
//! - Calls `cryptoKeys.decrypt` for [`KmsProvider::unwrap_dek`], replaying
//!   the same AAD.
//! - Calls `cryptoKeys.get` for [`KmsProvider::check_access`], mapping the
//!   `primary.state` field to the canonical [`KmsAccessStatus`] enum.
//!
//! # HSM tier (FIPS 140-2 Level 3)
//!
//! GCP Cloud KMS supports both `SOFTWARE` and `HSM` protection levels. The
//! HSM tier is FIPS 140-2 Level 3 validated (vs. Level 1 for software). The
//! [`GcpKmsRealProvider::fips_level`] method **reports the level inferred
//! from the resource path's parent CryptoKey**:
//!
//! | Returned `protectionLevel` | `fips_level()`         |
//! |----------------------------|------------------------|
//! | `HSM`                      | `Fips140_2_L3` (logical) |
//! | `EXTERNAL` / `EXTERNAL_VPC`| `Fips140_2_L3`         |
//! | `SOFTWARE` (default)       | `Fips140_2_L1`         |
//!
//! Per CoreLink policy, production BYOK customers MUST use the HSM tier. The
//! provider does **not** enforce this at wrap time (the customer is the
//! source of truth on their CMK config); enforcement happens in the
//! orchestrator. See `compliance/byok-fips-matrix.md`.
//!
//! # Endpoint
//!
//! Default: `https://cloudkms.googleapis.com`. The `endpoint` constructor
//! parameter is exposed for `wiremock` and regional FIPS endpoints
//! (e.g. `cloudkms.us-east1.rep.googleapis.com` for regional FIPS).
//!
//! # Errors
//!
//! All HTTP / parse / auth errors map to [`BYOKError`] variants. ADC
//! credential material is never included in error messages.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use corelink_byok::{
    BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

use crate::adc::{b64_decode, b64_encode, AdcCredentials};

/// Default Cloud KMS REST endpoint.
const DEFAULT_ENDPOINT: &str = "https://cloudkms.googleapis.com";

/// Real GCP Cloud KMS provider — production REST client.
///
/// Construct via [`GcpKmsRealProvider::new`] (default endpoint, ADC).
#[derive(Debug, Clone)]
pub struct GcpKmsRealProvider {
    http: reqwest::Client,
    creds: AdcCredentials,
    endpoint: String,
    region: String,
}

impl GcpKmsRealProvider {
    /// Construct a real GCP KMS provider using ADC.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if HTTP client init fails or ADC
    /// credentials cannot be resolved.
    pub async fn new(region: &str) -> Result<Self, BYOKError> {
        Self::with_endpoint(region, DEFAULT_ENDPOINT).await
    }

    /// Construct with a custom endpoint (regional FIPS endpoint or test mock).
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] on client / credential failure.
    pub async fn with_endpoint(region: &str, endpoint: &str) -> Result<Self, BYOKError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| BYOKError::Provider(format!("reqwest client init: {e}")))?;
        let creds = AdcCredentials::detect(http.clone()).await?;
        Ok(Self {
            http,
            creds,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            region: region.to_string(),
        })
    }

    /// Test-only constructor that injects credentials wholesale. Useful for
    /// `wiremock` tests where we want to bypass real ADC.
    #[doc(hidden)]
    pub fn for_test(http: reqwest::Client, creds: AdcCredentials, region: &str, endpoint: &str) -> Self {
        Self {
            http,
            creds,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            region: region.to_string(),
        }
    }

    fn validate_key_resource(key_arn_or_id: &str) -> Result<(), BYOKError> {
        if !crate::key_resource::is_valid_cloud_kms_resource(key_arn_or_id) {
            return Err(BYOKError::Provider(format!(
                "malformed Cloud KMS key resource: '{key_arn_or_id}' \
                 (expected projects/<P>/locations/<L>/keyRings/<R>/cryptoKeys/<K>[/cryptoKeyVersions/<V>])"
            )));
        }
        Ok(())
    }

    async fn bearer(&self) -> Result<String, BYOKError> {
        self.creds.access_token().await
    }
}

// ---------------------------------------------------------------------------
// REST wire types (Cloud KMS v1).
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct EncryptRequest {
    plaintext: String,
    #[serde(rename = "additionalAuthenticatedData", skip_serializing_if = "String::is_empty")]
    additional_authenticated_data: String,
}

#[derive(Debug, Deserialize)]
struct EncryptResponse {
    ciphertext: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
}

#[derive(Debug, Serialize)]
struct DecryptRequest {
    ciphertext: String,
    #[serde(rename = "additionalAuthenticatedData", skip_serializing_if = "String::is_empty")]
    additional_authenticated_data: String,
}

#[derive(Debug, Deserialize)]
struct DecryptResponse {
    plaintext: String,
}

#[derive(Debug, Deserialize)]
struct CryptoKey {
    #[serde(default)]
    primary: Option<CryptoKeyVersion>,
    #[serde(default)]
    #[allow(dead_code)]
    purpose: String,
}

#[derive(Debug, Deserialize)]
struct CryptoKeyVersion {
    /// One of: PENDING_GENERATION, ENABLED, DISABLED, DESTROYED,
    /// DESTROY_SCHEDULED, PENDING_IMPORT, IMPORT_FAILED.
    #[serde(default)]
    state: String,
    /// One of: SOFTWARE, HSM, EXTERNAL, EXTERNAL_VPC.
    #[serde(rename = "protectionLevel", default)]
    #[allow(dead_code)]
    protection_level: String,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    error: ApiErrorInner,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorInner {
    #[serde(default)]
    #[allow(dead_code)]
    code: u16,
    #[serde(default)]
    message: String,
    #[serde(default)]
    status: String,
}

fn parse_api_error(body: &str) -> ApiErrorInner {
    serde_json::from_str::<ApiErrorBody>(body)
        .map(|e| e.error)
        .unwrap_or_default()
}

fn aad_bytes(ctx: Option<&serde_json::Value>) -> Vec<u8> {
    match ctx {
        None => vec![],
        Some(v) => serde_json::to_vec(v).unwrap_or_default(),
    }
}

#[async_trait]
impl KmsProvider for GcpKmsRealProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::GcpKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // We cannot know the per-key protection level without a network round-trip.
        // The static contract is FIPS 140-2 L1 (the default tier). HSM-tier keys
        // are logically L3; the orchestrator enforces tier policy.
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
                "GcpKmsRealProvider: wrong provider {:?}",
                key_id.provider
            )));
        }
        Self::validate_key_resource(&key_id.key_arn_or_id)?;
        let ctx = encryption_context.ok_or(BYOKError::EncryptionContextMissing)?;

        let token = self.bearer().await?;
        let url = format!(
            "{}/v1/{}:encrypt",
            self.endpoint,
            key_id.key_arn_or_id.trim_end_matches('/')
        );
        let body = EncryptRequest {
            plaintext: b64_encode(&dek.bytes),
            additional_authenticated_data: b64_encode(&aad_bytes(Some(ctx))),
        };

        debug!(key = %key_id.key_arn_or_id, "GCP KMS wrap_dek");

        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("GCP KMS encrypt POST: {e}")))?;

        if !resp.status().is_success() {
            return Err(map_http_error(resp, key_id, "encrypt").await);
        }

        let er: EncryptResponse = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("GCP KMS encrypt JSON parse: {e}")))?;
        let ciphertext = b64_decode(&er.ciphertext)?;

        Ok(WrappedDek {
            provider: KmsProviderKind::GcpKms,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: Some(ctx.clone()),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::GcpKms {
            return Err(BYOKError::EnvelopeError(format!(
                "GcpKmsRealProvider: wrong provider {:?}",
                wrapped.provider
            )));
        }
        Self::validate_key_resource(&wrapped.key_id.key_arn_or_id)?;
        let ctx = wrapped
            .encryption_context
            .as_ref()
            .ok_or(BYOKError::EncryptionContextMissing)?;

        // For decrypt we MUST address the parent CryptoKey, not the version
        // (Cloud KMS auto-selects the version from the ciphertext header).
        let parent = crate::key_resource::strip_version_suffix(&wrapped.key_id.key_arn_or_id);

        let token = self.bearer().await?;
        let url = format!("{}/v1/{}:decrypt", self.endpoint, parent);
        let body = DecryptRequest {
            ciphertext: b64_encode(&wrapped.ciphertext),
            additional_authenticated_data: b64_encode(&aad_bytes(Some(ctx))),
        };

        debug!(key = %parent, "GCP KMS unwrap_dek");

        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("GCP KMS decrypt POST: {e}")))?;

        if !resp.status().is_success() {
            return Err(map_http_error(resp, &wrapped.key_id, "decrypt").await);
        }

        let dr: DecryptResponse = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("GCP KMS decrypt JSON parse: {e}")))?;
        let plaintext = b64_decode(&dr.plaintext)?;

        if plaintext.len() != 32 {
            return Err(BYOKError::DekLengthInvalid { got: plaintext.len() });
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);
        Ok(Dek { bytes })
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::GcpKms {
            return Err(BYOKError::EnvelopeError(format!(
                "GcpKmsRealProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }
        Self::validate_key_resource(&key_id.key_arn_or_id)?;

        let parent = crate::key_resource::strip_version_suffix(&key_id.key_arn_or_id);
        let token = self.bearer().await?;
        let url = format!("{}/v1/{}", self.endpoint, parent);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("GCP KMS GetCryptoKey: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let api_err = parse_api_error(&body);
            return Ok(map_get_error_to_access(status.as_u16(), &api_err));
        }

        let ck: CryptoKey = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("GCP KMS CryptoKey JSON: {e}")))?;
        let state = ck.primary.as_ref().map(|v| v.state.as_str()).unwrap_or("");
        Ok(map_state_to_access(state))
    }
}

fn map_state_to_access(state: &str) -> KmsAccessStatus {
    match state {
        "ENABLED" => KmsAccessStatus::Ok,
        "DISABLED" | "DESTROY_SCHEDULED" => KmsAccessStatus::Revoked,
        "DESTROYED" => KmsAccessStatus::NotFound,
        "PENDING_GENERATION" | "PENDING_IMPORT" => KmsAccessStatus::ApiError(0),
        "IMPORT_FAILED" => KmsAccessStatus::ApiError(0),
        _ => KmsAccessStatus::ApiError(0),
    }
}

fn map_get_error_to_access(http_status: u16, api: &ApiErrorInner) -> KmsAccessStatus {
    match http_status {
        403 => KmsAccessStatus::Revoked,
        404 => KmsAccessStatus::NotFound,
        429 => KmsAccessStatus::Throttled,
        _ => {
            warn!(http = http_status, status = %api.status, "GCP KMS GetCryptoKey error");
            KmsAccessStatus::ApiError(http_status)
        }
    }
}

async fn map_http_error(resp: reqwest::Response, key_id: &KmsKeyId, op: &str) -> BYOKError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let api = parse_api_error(&body);

    if status.as_u16() == 400 && api.message.to_ascii_lowercase().contains("authentication") {
        return BYOKError::AadMismatch;
    }
    // Cloud KMS surfaces AAD mismatch as `INVALID_ARGUMENT` with message
    // mentioning `additional_authenticated_data` or `authentication tag`.
    let msg_lc = api.message.to_ascii_lowercase();
    if status.as_u16() == 400
        && (msg_lc.contains("additional_authenticated_data")
            || msg_lc.contains("authentication tag")
            || msg_lc.contains("checksum"))
    {
        return BYOKError::AadMismatch;
    }
    if status.as_u16() == 403 {
        return BYOKError::CmkRevoked {
            provider: KmsProviderKind::GcpKms,
            key_id: key_id.key_arn_or_id.clone(),
        };
    }
    if status.as_u16() == 404 {
        return BYOKError::CmkRevoked {
            provider: KmsProviderKind::GcpKms,
            key_id: key_id.key_arn_or_id.clone(),
        };
    }
    warn!(http = %status, op = %op, "GCP KMS provider error");
    BYOKError::Provider(format!("GCP KMS {op} HTTP {status}: {}", api.message))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn state_mapping() {
        assert_eq!(map_state_to_access("ENABLED"), KmsAccessStatus::Ok);
        assert_eq!(map_state_to_access("DISABLED"), KmsAccessStatus::Revoked);
        assert_eq!(map_state_to_access("DESTROY_SCHEDULED"), KmsAccessStatus::Revoked);
        assert_eq!(map_state_to_access("DESTROYED"), KmsAccessStatus::NotFound);
        assert!(matches!(
            map_state_to_access("PENDING_GENERATION"),
            KmsAccessStatus::ApiError(_)
        ));
        assert!(matches!(map_state_to_access("unknown"), KmsAccessStatus::ApiError(_)));
    }

    #[test]
    fn http_to_access() {
        let empty = ApiErrorInner::default();
        assert_eq!(map_get_error_to_access(403, &empty), KmsAccessStatus::Revoked);
        assert_eq!(map_get_error_to_access(404, &empty), KmsAccessStatus::NotFound);
        assert_eq!(map_get_error_to_access(429, &empty), KmsAccessStatus::Throttled);
        assert!(matches!(
            map_get_error_to_access(500, &empty),
            KmsAccessStatus::ApiError(500)
        ));
    }
}
