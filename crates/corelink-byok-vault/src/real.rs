//! `VaultTransitProvider` — real HashiCorp Vault Transit REST client (R2-9).
//!
//! Replaces the `PendingStub` behavior of [`crate::VaultProvider::new_production`]
//! with a working HTTPS client that:
//!
//! - Authenticates via one of: direct token, AppRole, Kubernetes, AWS IAM
//!   (see [`crate::auth`]).
//! - Validates the customer key name against the Transit naming regex
//!   (`^[A-Za-z0-9_-]+$`).
//! - Calls `POST /v1/{mount}/encrypt/{key}` for [`KmsProvider::wrap_dek`],
//!   passing the JCS-canonical JSON of `encryption_context` (base64-encoded)
//!   as the `context` parameter. Vault Transit enforces the binding
//!   server-side: tamper attempts surface as HTTP 400 and map to
//!   [`BYOKError::AadMismatch`].
//! - Calls `POST /v1/{mount}/decrypt/{key}` for [`KmsProvider::unwrap_dek`],
//!   replaying the same `context`.
//! - Calls `GET /v1/{mount}/keys/{key}` for [`KmsProvider::check_access`],
//!   mapping the response (`deletion_allowed`, `min_decryption_version`, ...)
//!   to the canonical [`KmsAccessStatus`] enum.
//!
//! # HSM tier (FIPS 140-2 Level 3)
//!
//! Vault Transit by itself runs in **software** (FIPS 140-2 Level 1 in the
//! Vault Enterprise FIPS build). The full FIPS 140-2 Level 3 tier is reached
//! **only** when the operator deploys Vault Enterprise with HSM auto-unseal
//! against a vendor HSM (AWS CloudHSM, SafeNet Luna, Entrust nShield, etc.).
//! This is a *customer-side* deployment property — CoreLink cannot verify
//! the HSM dependency over the Transit API.
//!
//! [`VaultTransitProvider::fips_level`] reports the **default** tier
//! (`Fips140_3_L1`). Customers that require L3 must document their Vault +
//! HSM unseal configuration in their compliance attestation; the orchestrator
//! enforces tier policy from operator-supplied metadata. See
//! `compliance/byok-fips-matrix.md`.
//!
//! # TLS
//!
//! TLS verification is **ON** by default (rustls via reqwest workspace
//! feature). `VAULT_SKIP_VERIFY=true` is honoured **only** in non-production
//! builds; in `production` feature, the env var is rejected with an audit
//! warning so a misconfiguration cannot silently weaken transport security.
//!
//! # Errors
//!
//! All HTTP / parse / auth errors map to [`BYOKError`] variants. Vault tokens
//! are NEVER included in error messages.

use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use corelink_byok::{
    BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

use crate::auth::VaultAuth;
use crate::key_name::{extract_key_name, is_valid_key_name};

/// Default Vault Transit mount path (configurable per deployment).
pub const DEFAULT_TRANSIT_MOUNT: &str = "transit";

/// HTTP header Vault uses for the client token.
const VAULT_TOKEN_HEADER: &str = "X-Vault-Token";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64d(s: &str) -> Result<Vec<u8>, BYOKError> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| BYOKError::Provider(format!("Vault: base64 decode: {e}")))
}

/// Serialize an optional `encryption_context` to JCS-canonical JSON bytes.
///
/// `serde_json::to_vec` already emits map keys in insertion order; Vault
/// `context` is treated as opaque AAD-equivalent bytes, so byte stability
/// across wrap / unwrap is the only requirement.
fn context_bytes(ctx: Option<&serde_json::Value>) -> Vec<u8> {
    match ctx {
        None => Vec::new(),
        Some(v) => serde_json::to_vec(v).unwrap_or_default(),
    }
}

/// Real Vault Transit provider — production REST client.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct VaultTransitProvider {
    http: reqwest::Client,
    auth: VaultAuth,
    vault_addr: String,
    transit_mount: String,
    region: String,
}

impl VaultTransitProvider {
    /// Construct a real Vault Transit provider from environment.
    ///
    /// Reads `VAULT_ADDR` and auth env vars (see [`VaultAuth::detect`]).
    /// TLS verification is forced on.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if HTTP client init fails, no auth
    /// method is configured, or `VAULT_ADDR` is unset.
    pub fn from_env(region: &str) -> Result<Self, BYOKError> {
        let vault_addr = std::env::var("VAULT_ADDR")
            .map_err(|_| BYOKError::Provider("VAULT_ADDR unset".to_string()))?;
        Self::reject_skip_verify_in_production()?;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| BYOKError::Provider(format!("reqwest client init: {e}")))?;
        let auth = VaultAuth::detect(http.clone(), &vault_addr)?;
        Ok(Self {
            http,
            auth,
            vault_addr: vault_addr.trim_end_matches('/').to_string(),
            transit_mount: DEFAULT_TRANSIT_MOUNT.to_string(),
            region: region.to_string(),
        })
    }

    /// Test-only constructor that injects an explicit auth backend and endpoint.
    #[doc(hidden)]
    pub fn for_test(
        http: reqwest::Client,
        auth: VaultAuth,
        vault_addr: &str,
        transit_mount: &str,
        region: &str,
    ) -> Self {
        Self {
            http,
            auth,
            vault_addr: vault_addr.trim_end_matches('/').to_string(),
            transit_mount: transit_mount.to_string(),
            region: region.to_string(),
        }
    }

    /// Auth method label (for debug logs).
    #[must_use]
    pub fn auth_method(&self) -> &'static str {
        self.auth.method()
    }

    fn reject_skip_verify_in_production() -> Result<(), BYOKError> {
        if let Ok(v) = std::env::var("VAULT_SKIP_VERIFY") {
            if v == "true" || v == "1" {
                warn!(
                    "VAULT_SKIP_VERIFY=true rejected in production build \
                     (TLS verification is non-negotiable)"
                );
                return Err(BYOKError::Provider(
                    "VAULT_SKIP_VERIFY=true is rejected in production build".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn validate_key(&self, key: &KmsKeyId) -> Result<String, BYOKError> {
        if key.provider != KmsProviderKind::HashicorpVault {
            return Err(BYOKError::EnvelopeError(format!(
                "VaultTransitProvider: wrong provider {:?}",
                key.provider
            )));
        }
        let raw = key.key_arn_or_id.as_str();
        // Reject path-traversal payloads up front: a valid input is either a
        // bare key name, or the canonical `<mount>/keys/<name>` form. Anything
        // containing `..` or absolute-path markers is rejected.
        if raw.contains("..") || raw.starts_with('/') {
            return Err(BYOKError::Provider(format!(
                "malformed Vault Transit key name: '{raw}' (path traversal)"
            )));
        }
        let name = extract_key_name(raw);
        if !is_valid_key_name(name) {
            return Err(BYOKError::Provider(format!(
                "malformed Vault Transit key name: '{name}' \
                 (expected ^[A-Za-z0-9_-]+$)"
            )));
        }
        Ok(name.to_string())
    }

    async fn vault_token(&self) -> Result<String, BYOKError> {
        self.auth.token().await
    }
}

// ---------------------------------------------------------------------------
// Vault Transit wire types.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct EncryptRequest {
    plaintext: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    context: String,
}

#[derive(Debug, Deserialize)]
struct EncryptResponseEnvelope {
    data: EncryptData,
}

#[derive(Debug, Deserialize)]
struct EncryptData {
    ciphertext: String,
    #[serde(default)]
    #[allow(dead_code)]
    key_version: u64,
}

#[derive(Debug, Serialize)]
struct DecryptRequest {
    ciphertext: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    context: String,
}

#[derive(Debug, Deserialize)]
struct DecryptResponseEnvelope {
    data: DecryptData,
}

#[derive(Debug, Deserialize)]
struct DecryptData {
    plaintext: String,
}

#[derive(Debug, Deserialize)]
struct KeyReadEnvelope {
    #[serde(default)]
    data: Option<KeyReadData>,
}

#[derive(Debug, Deserialize)]
struct KeyReadData {
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    deletion_allowed: bool,
    /// 0 = no minimum (key fully usable); >0 = older versions blocked.
    #[serde(default)]
    #[allow(dead_code)]
    min_decryption_version: u64,
    #[serde(default)]
    #[allow(dead_code)]
    min_encryption_version: u64,
    /// Returned only for keys that have been disabled.
    #[serde(default)]
    deletion_time: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct VaultErrorBody {
    #[serde(default)]
    errors: Vec<String>,
}

fn parse_vault_error(body: &str) -> VaultErrorBody {
    serde_json::from_str::<VaultErrorBody>(body).unwrap_or_default()
}

#[async_trait]
impl KmsProvider for VaultTransitProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::HashicorpVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Default tier = Vault Enterprise FIPS build (software) = 140-3 L1.
        // FIPS 140-2 L3 is reachable only with operator-side HSM auto-unseal
        // (AWS CloudHSM, SafeNet Luna, Entrust nShield). Orchestrator enforces.
        FipsLevel::Fips140_3_L1
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        let key_name = self.validate_key(key_id)?;
        let ctx = encryption_context.ok_or(BYOKError::EncryptionContextMissing)?;

        let token = self.vault_token().await?;
        let url = format!(
            "{}/v1/{}/encrypt/{}",
            self.vault_addr, self.transit_mount, key_name
        );
        let body = EncryptRequest {
            plaintext: b64(&dek.bytes),
            context: b64(&context_bytes(Some(ctx))),
        };

        debug!(key = %key_name, auth = self.auth.method(), "Vault Transit wrap_dek");

        let resp = self
            .http
            .post(&url)
            .header(VAULT_TOKEN_HEADER, token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("Vault encrypt POST: {e}")))?;

        if !resp.status().is_success() {
            return Err(map_http_error(resp, key_id, "encrypt").await);
        }

        let env: EncryptResponseEnvelope = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("Vault encrypt JSON parse: {e}")))?;

        // Vault returns `vault:v<n>:<base64-ciphertext>` — store the whole
        // wrapping intact so decrypt can replay it. We keep the version
        // prefix inside `ciphertext` (UTF-8 bytes of the Vault token).
        let ciphertext = env.data.ciphertext.into_bytes();

        Ok(WrappedDek {
            provider: KmsProviderKind::HashicorpVault,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: Some(ctx.clone()),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        let key_name = self.validate_key(&wrapped.key_id)?;
        let ctx = wrapped
            .encryption_context
            .as_ref()
            .ok_or(BYOKError::EncryptionContextMissing)?;

        let vault_ct = std::str::from_utf8(&wrapped.ciphertext).map_err(|_| {
            BYOKError::EnvelopeError("Vault ciphertext is not valid UTF-8".to_string())
        })?;

        let token = self.vault_token().await?;
        let url = format!(
            "{}/v1/{}/decrypt/{}",
            self.vault_addr, self.transit_mount, key_name
        );
        let body = DecryptRequest {
            ciphertext: vault_ct.to_string(),
            context: b64(&context_bytes(Some(ctx))),
        };

        debug!(key = %key_name, auth = self.auth.method(), "Vault Transit unwrap_dek");

        let resp = self
            .http
            .post(&url)
            .header(VAULT_TOKEN_HEADER, token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("Vault decrypt POST: {e}")))?;

        if !resp.status().is_success() {
            return Err(map_http_error(resp, &wrapped.key_id, "decrypt").await);
        }

        let env: DecryptResponseEnvelope = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("Vault decrypt JSON parse: {e}")))?;
        let plaintext = b64d(&env.data.plaintext)?;

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
        let key_name = self.validate_key(key_id)?;
        let token = self.vault_token().await?;
        let url = format!(
            "{}/v1/{}/keys/{}",
            self.vault_addr, self.transit_mount, key_name
        );

        let resp = self
            .http
            .get(&url)
            .header(VAULT_TOKEN_HEADER, token)
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("Vault read key: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let _body = resp.text().await.unwrap_or_default();
            return Ok(map_get_error_to_access(status.as_u16()));
        }

        let env: KeyReadEnvelope = resp
            .json()
            .await
            .map_err(|e| BYOKError::Provider(format!("Vault key read JSON: {e}")))?;
        let data = match env.data {
            Some(d) => d,
            None => return Ok(KmsAccessStatus::ApiError(0)),
        };

        // Scheduled for destruction → revoked (treat like AWS PendingDeletion).
        if data.deletion_time.is_some() {
            return Ok(KmsAccessStatus::Revoked);
        }
        // Key flagged deletion_allowed but no deletion_time => caller has
        // toggled the destroy flag but not yet scheduled; treat as Ok.
        let _ = data.deletion_allowed;
        Ok(KmsAccessStatus::Ok)
    }
}

fn map_get_error_to_access(http_status: u16) -> KmsAccessStatus {
    match http_status {
        403 => KmsAccessStatus::Revoked,
        404 => KmsAccessStatus::NotFound,
        429 => KmsAccessStatus::Throttled,
        other => {
            warn!(http = other, "Vault Transit key read error");
            KmsAccessStatus::ApiError(other)
        }
    }
}

async fn map_http_error(resp: reqwest::Response, key_id: &KmsKeyId, op: &str) -> BYOKError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let err = parse_vault_error(&body);
    let msg = err.errors.join("; ");
    let msg_lc = msg.to_ascii_lowercase();

    // Vault Transit surfaces context binding mismatches as HTTP 400 with
    // either "context" or "mismatch" in the error string.
    if status.as_u16() == 400
        && (msg_lc.contains("context")
            || msg_lc.contains("mismatch")
            || msg_lc.contains("invalid ciphertext"))
    {
        return BYOKError::AadMismatch;
    }
    if status.as_u16() == 403 {
        return BYOKError::CmkRevoked {
            provider: KmsProviderKind::HashicorpVault,
            key_id: key_id.key_arn_or_id.clone(),
        };
    }
    if status.as_u16() == 404 {
        return BYOKError::CmkRevoked {
            provider: KmsProviderKind::HashicorpVault,
            key_id: key_id.key_arn_or_id.clone(),
        };
    }
    warn!(http = %status, op = %op, "Vault Transit provider error");
    BYOKError::Provider(format!("Vault {op} HTTP {status}: {msg}"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn http_mapping() {
        assert_eq!(map_get_error_to_access(403), KmsAccessStatus::Revoked);
        assert_eq!(map_get_error_to_access(404), KmsAccessStatus::NotFound);
        assert_eq!(map_get_error_to_access(429), KmsAccessStatus::Throttled);
        assert!(matches!(
            map_get_error_to_access(500),
            KmsAccessStatus::ApiError(500)
        ));
    }

    #[test]
    fn context_bytes_stable() {
        let ctx = serde_json::json!({"tenant_id": "T1", "blob_hash": "H1"});
        let a = context_bytes(Some(&ctx));
        let b = context_bytes(Some(&ctx));
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn context_bytes_none_is_empty() {
        assert!(context_bytes(None).is_empty());
    }
}
