//! AWS KMS adapter — first-class BYOK provider for CoreLink.
//!
//! # FIPS compliance
//!
//! AWS KMS uses FIPS 140-3 Level 1 validated cryptographic modules by default
//! (NIST CMVP Certificate #4523).  No additional configuration is needed for
//! FIPS-validated cryptography; the underlying HSMs are FIPS-validated.
//!
//! For deployments that must enforce a FIPS-validated TLS terminator
//! (FIPS 140-3 Level 1 TLS endpoint) — typically required by US Federal /
//! GovCloud customers — set:
//!
//! ```text
//! AWS_USE_FIPS_ENDPOINT=true
//! ```
//!
//! `AwsKmsProvider::new` will then resolve the FIPS regional endpoint
//! (e.g. `kms-fips.us-east-1.amazonaws.com`).  See
//! `compliance/byok-fips-matrix.md` for the certified module list.
//!
//! # IAM scope
//!
//! CoreLink requires only three permissions on the customer CMK:
//! - `kms:Encrypt` — wrap DEK.
//! - `kms:Decrypt` — unwrap DEK.
//! - `kms:DescribeKey` — CMK access check (kill-switch detection).
//!
//! Optionally `kms:GenerateDataKey` if upstream callers prefer
//! KMS-server-side DEK generation; CoreLink uses CSPRNG locally so this
//! is not required.
//!
//! No `kms:CreateKey`, `kms:ScheduleKeyDeletion`, or other destructive
//! permissions.
//!
//! # AAD (encryption_context)
//!
//! Every `wrap_dek` call binds `{"tenant_id": "...", "blob_hash": "..."}` as
//! `EncryptionContext`.  The same map must be provided on `Decrypt`; AWS KMS
//! enforces the match server-side — mismatched context = `InvalidCiphertextException`.
//! This is the INV-BYOK-CRYPTO-SOVEREIGNTY enforcement boundary; the provider
//! NEVER bypasses it.
//!
//! # ARN validation
//!
//! Customer CMK identifiers must be full ARNs of the form
//! `arn:aws:kms:<region>:<account>:key/<uuid>` (or `arn:aws-us-gov:...` /
//! `arn:aws-cn:...` for GovCloud / China partitions).  Aliases are NOT
//! accepted — they introduce a layer of indirection the customer cannot
//! prove cryptographically.  See [`validate_aws_kms_key_arn`].
//!
//! # Latency SLO
//!
//! p99 ≤ 30 ms when CoreLink Workers are in the same AWS region as the CMK.
//!
//! # wasm32 strategy
//!
//! This crate depends on `aws-sdk-kms`, which is native-only (uses tokio,
//! `hyper`, OS-level TLS).  CoreLink intentionally runs BYOK envelope
//! operations inside the native server process (`apps/server`) rather than
//! inside the Cloudflare Worker, so AWS KMS calls never need to compile to
//! `wasm32-unknown-unknown`.  Workers proxy envelope ops to the server over
//! the internal control-plane RPC.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use aws_sdk_kms::{
    config::{Builder as KmsConfigBuilder, Region},
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

/// Environment variable that, when set to `"true"` / `"1"`, instructs the
/// adapter to resolve AWS KMS FIPS endpoints (e.g. `kms-fips.us-east-1.amazonaws.com`).
pub const ENV_AWS_USE_FIPS_ENDPOINT: &str = "AWS_USE_FIPS_ENDPOINT";

/// AWS KMS implementation of [`KmsProvider`].
///
/// # Construction
///
/// Use [`AwsKmsProvider::new`] — picks up credentials from the environment
/// (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`,
/// `AWS_PROFILE`, IRSA / IMDS instance role, etc. — the standard AWS SDK
/// credential provider chain).
///
/// For unit tests, use [`AwsKmsProvider::new_mock`] which does not perform
/// any network calls.
#[derive(Debug)]
#[non_exhaustive]
pub struct AwsKmsProvider {
    client: Option<Client>,
    region: String,
    fips_endpoint: bool,
    mock: bool,
}

impl AwsKmsProvider {
    /// Construct from the ambient AWS environment for `region`.
    ///
    /// Resolves credentials via the standard AWS SDK chain (env vars, shared
    /// config, IRSA / IAM role).
    ///
    /// If `AWS_USE_FIPS_ENDPOINT=true` / `=1`, the client is configured with
    /// `use_fips_endpoint(true)` so the FIPS-validated TLS terminator is used
    /// (FIPS 140-3 Level 1 endpoint path; required for GovCloud).
    ///
    /// # Errors
    ///
    /// AWS SDK initialisation errors (e.g. missing credentials) surface here
    /// as [`BYOKError::Provider`].
    pub async fn new(region: &str) -> Result<Self, BYOKError> {
        let fips_endpoint = read_fips_endpoint_flag();

        let mut loader = aws_config::from_env().region(Region::new(region.to_string()));
        if fips_endpoint {
            loader = loader.use_fips(true);
        }
        let shared = loader.load().await;

        // Re-build the KMS-scoped config so `use_fips` is honored at the
        // service-client layer as well (some SDK versions only honor the
        // flag when set on the service config).
        let mut svc_cfg = KmsConfigBuilder::from(&shared);
        if fips_endpoint {
            svc_cfg = svc_cfg.use_fips(true);
        }
        let client = Client::from_conf(svc_cfg.build());

        Ok(Self {
            client: Some(client),
            region: region.to_string(),
            fips_endpoint,
            mock: false,
        })
    }

    /// Construct a mock provider for unit tests.
    ///
    /// In mock mode, `wrap_dek` / `unwrap_dek` / `check_access` operate
    /// in-process without contacting AWS.  AAD binding is enforced
    /// (encryption_context fingerprint is stored in ciphertext and verified
    /// at unwrap time) so security regressions still fail.
    #[must_use]
    pub fn new_mock(region: &str) -> Self {
        Self {
            client: None,
            region: region.to_string(),
            fips_endpoint: false,
            mock: true,
        }
    }

    /// Whether this provider is using the FIPS-validated TLS endpoint.
    #[must_use]
    pub const fn fips_endpoint_enabled(&self) -> bool {
        self.fips_endpoint
    }

    /// Whether this provider is running in mock (no-network) mode.
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.mock
    }

    /// Resolve the real AWS KMS endpoint name for `region` given the current
    /// FIPS flag.  Used by tests; exposed for documentation.
    ///
    /// Returns e.g. `"kms-fips.us-east-1.amazonaws.com"` when FIPS is on,
    /// `"kms.us-east-1.amazonaws.com"` otherwise.
    #[must_use]
    pub fn resolved_endpoint_hostname(&self) -> String {
        resolve_endpoint_hostname(&self.region, self.fips_endpoint)
    }
}

fn read_fips_endpoint_flag() -> bool {
    matches!(
        std::env::var(ENV_AWS_USE_FIPS_ENDPOINT).ok().as_deref(),
        Some("true" | "1" | "TRUE" | "True")
    )
}

fn resolve_endpoint_hostname(region: &str, fips: bool) -> String {
    if fips {
        format!("kms-fips.{region}.amazonaws.com")
    } else {
        format!("kms.{region}.amazonaws.com")
    }
}

/// Validate that `s` is a syntactically well-formed AWS KMS key ARN.
///
/// Accepted form: `arn:<partition>:kms:<region>:<account>:key/<uuid>` where
/// `<partition>` ∈ `{aws, aws-us-gov, aws-cn}` and `<uuid>` is a canonical
/// lowercase 36-char UUID (8-4-4-4-12 hex with hyphens).
///
/// Aliases (`arn:aws:kms:...:alias/...`) are intentionally rejected — they
/// add indirection the customer cannot cryptographically attest to and the
/// AAD binding works on key ARN, not alias.
///
/// # Errors
///
/// Returns [`BYOKError::Provider`] with a descriptive message if the ARN is
/// malformed.
pub fn validate_aws_kms_key_arn(s: &str) -> Result<(), BYOKError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return Err(BYOKError::Provider(format!(
            "malformed AWS KMS key ARN (expected 6 colon-separated segments, got {}): redacted",
            parts.len()
        )));
    }
    let arn = parts.first().copied().unwrap_or("");
    let partition = parts.get(1).copied().unwrap_or("");
    let service = parts.get(2).copied().unwrap_or("");
    let region = parts.get(3).copied().unwrap_or("");
    let account = parts.get(4).copied().unwrap_or("");
    let resource = parts.get(5).copied().unwrap_or("");

    if arn != "arn" {
        return Err(BYOKError::Provider(
            "malformed AWS KMS key ARN: missing 'arn' prefix".to_string(),
        ));
    }
    if !matches!(partition, "aws" | "aws-us-gov" | "aws-cn") {
        return Err(BYOKError::Provider(format!(
            "malformed AWS KMS key ARN: unknown partition '{partition}'"
        )));
    }
    if service != "kms" {
        return Err(BYOKError::Provider(format!(
            "malformed AWS KMS key ARN: service must be 'kms', got '{service}'"
        )));
    }
    if region.is_empty() {
        return Err(BYOKError::Provider(
            "malformed AWS KMS key ARN: empty region".to_string(),
        ));
    }
    if account.len() != 12 || !account.chars().all(|c| c.is_ascii_digit()) {
        return Err(BYOKError::Provider(
            "malformed AWS KMS key ARN: account must be 12 digits".to_string(),
        ));
    }
    let Some(uuid) = resource.strip_prefix("key/") else {
        return Err(BYOKError::Provider(
            "malformed AWS KMS key ARN: resource must start with 'key/' (aliases not accepted)"
                .to_string(),
        ));
    };
    if !is_canonical_uuid(uuid) {
        return Err(BYOKError::Provider(
            "malformed AWS KMS key ARN: key id is not a canonical UUID".to_string(),
        ));
    }
    Ok(())
}

fn is_canonical_uuid(s: &str) -> bool {
    // 8-4-4-4-12 hex with hyphens, total 36 chars.
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    let group_lengths = [8usize, 4, 4, 4, 12];
    let mut idx = 0usize;
    for (i, &len) in group_lengths.iter().enumerate() {
        for _ in 0..len {
            let Some(&b) = bytes.get(idx) else {
                return false;
            };
            if !b.is_ascii_hexdigit() {
                return false;
            }
            idx += 1;
        }
        if i < group_lengths.len() - 1 {
            if bytes.get(idx).copied() != Some(b'-') {
                return false;
            }
            idx += 1;
        }
    }
    idx == 36
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
    /// - Returns [`BYOKError::Provider`] if `key_id.key_arn_or_id` is not a
    ///   syntactically valid AWS KMS key ARN.
    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        if key_id.provider != KmsProviderKind::AwsKms {
            return Err(BYOKError::EnvelopeError(format!(
                "AwsKmsProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }
        let ctx = encryption_context.ok_or(BYOKError::EncryptionContextMissing)?;
        let ec_map = json_to_string_map(ctx)?;

        validate_aws_kms_key_arn(&key_id.key_arn_or_id)?;

        if self.mock {
            return Ok(mock_wrap(dek, key_id, ctx));
        }

        debug!(
            key_arn = %key_id.key_arn_or_id,
            region = %key_id.region,
            fips = self.fips_endpoint,
            "AWS KMS wrap_dek"
        );

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| BYOKError::Provider("aws kms client uninitialized".to_string()))?;

        let mut req = client
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
    /// The `encryption_context` stored in `wrapped` is passed back to AWS KMS
    /// for AAD verification — server-side enforcement (mismatched context =
    /// [`BYOKError::AadMismatch`]).
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::AwsKms {
            return Err(BYOKError::EnvelopeError(format!(
                "AwsKmsProvider received WrappedDek with wrong provider: {:?}",
                wrapped.provider
            )));
        }
        let ctx = wrapped
            .encryption_context
            .as_ref()
            .ok_or(BYOKError::EncryptionContextMissing)?;
        let ec_map = json_to_string_map(ctx)?;

        validate_aws_kms_key_arn(&wrapped.key_id.key_arn_or_id)?;

        if self.mock {
            return mock_unwrap(wrapped, ctx);
        }

        debug!(
            key_arn = %wrapped.key_id.key_arn_or_id,
            "AWS KMS unwrap_dek"
        );

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| BYOKError::Provider("aws kms client uninitialized".to_string()))?;

        let mut req = client
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
    /// State mapping:
    /// - `Enabled` → [`KmsAccessStatus::Ok`].
    /// - `Disabled` / `PendingDeletion` → [`KmsAccessStatus::Revoked`]
    ///   (customer kill-switch).
    /// - `NotFoundException` → [`KmsAccessStatus::NotFound`]
    ///   (CMK already deleted by customer).
    /// - `AccessDeniedException` → [`KmsAccessStatus::Revoked`]
    ///   (customer revoked IAM policy).
    /// - `ThrottlingException` → [`KmsAccessStatus::Throttled`].
    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::AwsKms {
            return Err(BYOKError::EnvelopeError(format!(
                "AwsKmsProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }
        validate_aws_kms_key_arn(&key_id.key_arn_or_id)?;

        if self.mock {
            return Ok(KmsAccessStatus::Ok);
        }

        debug!(key_arn = %key_id.key_arn_or_id, "AWS KMS check_access");

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| BYOKError::Provider("aws kms client uninitialized".to_string()))?;

        let resp = client
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

// ── Mock-mode wrap/unwrap ─────────────────────────────────────────────────────

/// Mock-mode wrap: 8-byte AAD fingerprint + 32-byte XOR'd DEK = 40 bytes.
///
/// Mirrors the GCP mock path so the matrix test exercises identical
/// AAD-binding semantics across providers.
fn mock_wrap(dek: &Dek, key_id: &KmsKeyId, ctx: &Value) -> WrappedDek {
    let aad = serde_json::to_vec(ctx).unwrap_or_default();
    let fp = aad_fingerprint(&aad);
    let mut ct = Vec::with_capacity(8 + 32);
    ct.extend_from_slice(&fp);
    for b in &dek.bytes {
        ct.push(b ^ 0xBB);
    }
    WrappedDek {
        provider: KmsProviderKind::AwsKms,
        key_id: key_id.clone(),
        ciphertext: ct,
        encryption_context: Some(ctx.clone()),
    }
}

fn mock_unwrap(wrapped: &WrappedDek, ctx: &Value) -> Result<Dek, BYOKError> {
    if wrapped.ciphertext.len() != 40 {
        return Err(BYOKError::EnvelopeError(format!(
            "AWS mock: wrong ciphertext length {} (expected 40)",
            wrapped.ciphertext.len()
        )));
    }
    let aad = serde_json::to_vec(ctx).unwrap_or_default();
    let expected = aad_fingerprint(&aad);
    let stored = wrapped
        .ciphertext
        .get(..8)
        .ok_or_else(|| BYOKError::EnvelopeError("AWS mock: ciphertext too short".to_string()))?;
    if stored != expected.as_slice() {
        return Err(BYOKError::AadMismatch);
    }
    let body = wrapped
        .ciphertext
        .get(8..)
        .ok_or_else(|| BYOKError::EnvelopeError("AWS mock: ciphertext too short".to_string()))?;
    let mut bytes = [0u8; 32];
    for (out, &b) in bytes.iter_mut().zip(body.iter()) {
        *out = b ^ 0xBB;
    }
    Ok(Dek { bytes })
}

/// Cheap 8-byte fingerprint of AAD bytes (mock-mode tamper detection only;
/// production relies on AWS KMS server-side enforcement).
fn aad_fingerprint(aad: &[u8]) -> [u8; 8] {
    let mut fp = [0u8; 8];
    for (i, &b) in aad.iter().enumerate() {
        if let Some(slot) = fp.get_mut(i % 8) {
            *slot ^= b;
        }
    }
    let len_byte = (aad.len() as u8).wrapping_mul(0x37);
    if let Some(last) = fp.last_mut() {
        *last ^= len_byte;
    }
    fp
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

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_ARN: &str =
        "arn:aws:kms:us-east-1:000000000000:key/00000000-0000-0000-0000-000000000000";

    #[test]
    fn validate_arn_accepts_canonical_form() {
        assert!(validate_aws_kms_key_arn(VALID_ARN).is_ok());
    }

    #[test]
    fn validate_arn_accepts_govcloud_partition() {
        let arn = "arn:aws-us-gov:kms:us-gov-east-1:000000000000:key/00000000-0000-0000-0000-000000000000";
        assert!(validate_aws_kms_key_arn(arn).is_ok());
    }

    #[test]
    fn validate_arn_accepts_china_partition() {
        let arn = "arn:aws-cn:kms:cn-north-1:000000000000:key/00000000-0000-0000-0000-000000000000";
        assert!(validate_aws_kms_key_arn(arn).is_ok());
    }

    #[test]
    fn validate_arn_rejects_alias() {
        let arn = "arn:aws:kms:us-east-1:000000000000:alias/my-key";
        assert!(validate_aws_kms_key_arn(arn).is_err());
    }

    #[test]
    fn validate_arn_rejects_short_account() {
        let arn = "arn:aws:kms:us-east-1:12345:key/00000000-0000-0000-0000-000000000000";
        assert!(validate_aws_kms_key_arn(arn).is_err());
    }

    #[test]
    fn validate_arn_rejects_non_uuid_resource() {
        let arn = "arn:aws:kms:us-east-1:000000000000:key/not-a-uuid";
        assert!(validate_aws_kms_key_arn(arn).is_err());
    }

    #[test]
    fn validate_arn_rejects_empty() {
        assert!(validate_aws_kms_key_arn("").is_err());
    }

    #[test]
    fn validate_arn_rejects_wrong_partition() {
        let arn = "arn:aws-bogus:kms:us-east-1:000000000000:key/00000000-0000-0000-0000-000000000000";
        assert!(validate_aws_kms_key_arn(arn).is_err());
    }

    #[test]
    fn validate_arn_rejects_wrong_service() {
        let arn = "arn:aws:s3:us-east-1:000000000000:key/00000000-0000-0000-0000-000000000000";
        assert!(validate_aws_kms_key_arn(arn).is_err());
    }

    #[test]
    fn fips_endpoint_hostname_resolution() {
        let p = AwsKmsProvider::new_mock("us-east-1");
        // Default mock has FIPS off.
        assert!(!p.fips_endpoint_enabled());
        assert_eq!(p.resolved_endpoint_hostname(), "kms.us-east-1.amazonaws.com");
    }

    #[test]
    fn fips_endpoint_hostname_when_enabled() {
        // The flag-read helper is tested standalone; here we exercise the
        // formatter directly.
        assert_eq!(
            resolve_endpoint_hostname("us-gov-east-1", true),
            "kms-fips.us-gov-east-1.amazonaws.com"
        );
        assert_eq!(
            resolve_endpoint_hostname("us-east-1", true),
            "kms-fips.us-east-1.amazonaws.com"
        );
    }
}
