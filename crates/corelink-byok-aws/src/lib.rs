//! AWS KMS adapter — first-class BYOK provider for CoreLink.
//!
//! # FIPS compliance
//!
//! AWS KMS uses FIPS 140-3 Level 1 validated cryptographic modules by default
//! (NIST CMVP Certificate #4523).  No additional configuration is needed;
//! all `kms:Encrypt` / `kms:Decrypt` operations are processed inside the HSM.
//!
//! # IAM scope
//!
//! CoreLink requires only three permissions on the customer CMK:
//! - `kms:Encrypt` — wrap DEK.
//! - `kms:Decrypt` — unwrap DEK.
//! - `kms:DescribeKey` — CMK access check (kill-switch detection).
//!
//! No `kms:CreateKey`, `kms:ScheduleKeyDeletion`, or other destructive permissions.
//!
//! # AAD (encryption_context)
//!
//! Every `wrap_dek` call binds `{"tenant_id": "...", "blob_hash": "..."}` as
//! `EncryptionContext`.  The same map must be provided on `Decrypt`; AWS KMS
//! enforces the match server-side — mismatched context = `InvalidCiphertextException`.
//!
//! # Latency SLO
//!
//! p99 ≤ 30 ms when CoreLink Workers are in the same AWS region as the CMK.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use aws_sdk_kms::{
    config::Region,
    primitives::Blob,
    Client,
};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, warn};

use corelink_byok::{
    types::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek},
    KmsProvider,
};

/// AWS KMS implementation of [`KmsProvider`].
///
/// # Construction
///
/// Use [`AwsKmsProvider::new`] — picks up credentials from the environment
/// (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, or
/// instance role / IRSA).
#[derive(Debug)]
pub struct AwsKmsProvider {
    client: Client,
    region: String,
}

impl AwsKmsProvider {
    /// Construct from the ambient AWS environment for `region`.
    ///
    /// # Errors
    ///
    /// AWS SDK initialisation errors (e.g. missing credentials) surface here.
    pub async fn new(region: &str) -> Result<Self, BYOKError> {
        let config = aws_config::from_env()
            .region(Region::new(region.to_string()))
            .load()
            .await;
        let client = Client::new(&config);
        Ok(Self {
            client,
            region: region.to_string(),
        })
    }
}

#[async_trait]
impl KmsProvider for AwsKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AwsKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    /// AWS KMS FIPS 140-3 Level 1 (NIST CMVP #4523).
    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_3_L1
    }

    /// Wrap DEK via `kms:Encrypt`.
    ///
    /// - `encryption_context` is mandatory (`{"tenant_id": ..., "blob_hash": ...}`).
    /// - Returns [`BYOKError::EncryptionContextMissing`] if absent.
    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        let ctx = encryption_context.ok_or(BYOKError::EncryptionContextMissing)?;
        let ec_map = json_to_string_map(ctx)?;

        debug!(
            key_arn = %key_id.key_arn_or_id,
            region = %key_id.region,
            "AWS KMS wrap_dek"
        );

        let mut req = self
            .client
            .encrypt()
            .key_id(&key_id.key_arn_or_id)
            .plaintext(Blob::new(dek.bytes.to_vec()));

        for (k, v) in &ec_map {
            req = req.encryption_context(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("AccessDenied") || msg.contains("DisabledException") {
                    warn!(key_arn = %key_id.key_arn_or_id, "AWS KMS wrap: access denied");
                    BYOKError::CmkRevoked {
                        provider: KmsProviderKind::AwsKms,
                        key_id: key_id.key_arn_or_id.clone(),
                    }
                } else {
                    error!(error = %msg, "AWS KMS wrap: provider error");
                    BYOKError::Provider(format!("aws kms encrypt: {msg}"))
                }
            })?;

        let ciphertext = resp
            .ciphertext_blob()
            .ok_or_else(|| BYOKError::Provider("aws kms encrypt: missing ciphertext".to_string()))?
            .clone()
            .into_inner();

        Ok(WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: Some(ctx.clone()),
        })
    }

    /// Unwrap DEK via `kms:Decrypt`.
    ///
    /// The `encryption_context` stored in `wrapped` is passed back to AWS KMS for
    /// AAD verification — server-side enforcement.
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        let ctx = wrapped
            .encryption_context
            .as_ref()
            .ok_or(BYOKError::EncryptionContextMissing)?;
        let ec_map = json_to_string_map(ctx)?;

        debug!(
            key_arn = %wrapped.key_id.key_arn_or_id,
            "AWS KMS unwrap_dek"
        );

        let mut req = self
            .client
            .decrypt()
            .ciphertext_blob(Blob::new(wrapped.ciphertext.clone()))
            .key_id(&wrapped.key_id.key_arn_or_id);

        for (k, v) in &ec_map {
            req = req.encryption_context(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("AccessDenied")
                    || msg.contains("DisabledException")
                    || msg.contains("KMSInvalidStateException")
                {
                    warn!(key_arn = %wrapped.key_id.key_arn_or_id, "AWS KMS unwrap: access denied");
                    BYOKError::CmkRevoked {
                        provider: KmsProviderKind::AwsKms,
                        key_id: wrapped.key_id.key_arn_or_id.clone(),
                    }
                } else if msg.contains("InvalidCiphertext") {
                    warn!("AWS KMS unwrap: InvalidCiphertext — AAD mismatch?");
                    BYOKError::AadMismatch
                } else {
                    error!(error = %msg, "AWS KMS unwrap: provider error");
                    BYOKError::Provider(format!("aws kms decrypt: {msg}"))
                }
            })?;

        let plaintext = resp
            .plaintext()
            .ok_or_else(|| BYOKError::Provider("aws kms decrypt: missing plaintext".to_string()))?
            .clone()
            .into_inner();

        if plaintext.len() != 32 {
            return Err(BYOKError::DekLengthInvalid { got: plaintext.len() });
        }

        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);

        Ok(Dek { bytes })
    }

    /// Check CMK access via `kms:DescribeKey`.
    ///
    /// Called every 60 s in background per active BYOK tenant (WI-S14-006).
    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        debug!(key_arn = %key_id.key_arn_or_id, "AWS KMS check_access");

        let resp = self
            .client
            .describe_key()
            .key_id(&key_id.key_arn_or_id)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let state = r
                    .key_metadata()
                    .and_then(|m| m.key_state())
                    .map(|s| s.as_str().to_owned())
                    .unwrap_or_default();

                let status = match state.as_str() {
                    "Enabled" => KmsAccessStatus::Ok,
                    "PendingDeletion" | "Disabled" => {
                        warn!(key_arn = %key_id.key_arn_or_id, state = %state, "CMK revoked/disabled");
                        KmsAccessStatus::Revoked
                    }
                    "PendingImport" | "Unavailable" => KmsAccessStatus::ApiError(0),
                    other => {
                        warn!(key_arn = %key_id.key_arn_or_id, state = %other, "CMK unknown state");
                        KmsAccessStatus::ApiError(0)
                    }
                };
                Ok(status)
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("AccessDenied") {
                    warn!(key_arn = %key_id.key_arn_or_id, "check_access: AccessDenied — CMK revoked");
                    Ok(KmsAccessStatus::Revoked)
                } else if msg.contains("NotFoundException") {
                    warn!(key_arn = %key_id.key_arn_or_id, "check_access: CMK not found");
                    Ok(KmsAccessStatus::NotFound)
                } else if msg.contains("ThrottlingException") {
                    warn!("check_access: AWS KMS throttled");
                    Ok(KmsAccessStatus::Throttled)
                } else {
                    Err(BYOKError::Provider(format!("aws kms describe_key: {msg}")))
                }
            }
        }
    }
}

/// Convert a `serde_json::Value` (object) to `HashMap<String, String>` for AWS SDK.
///
/// # Errors
///
/// Returns [`BYOKError::EnvelopeError`] if the value is not a JSON object or
/// any value is not a string.
fn json_to_string_map(value: &Value) -> Result<HashMap<String, String>, BYOKError> {
    let obj = value
        .as_object()
        .ok_or_else(|| BYOKError::EnvelopeError("encryption_context must be a JSON object".to_string()))?;

    let mut map = HashMap::with_capacity(obj.len());
    for (k, v) in obj {
        let s = v.as_str().ok_or_else(|| {
            BYOKError::EnvelopeError(format!(
                "encryption_context.{k} value must be a string"
            ))
        })?;
        map.insert(k.clone(), s.to_owned());
    }
    Ok(map)
}
