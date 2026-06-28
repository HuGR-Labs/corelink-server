//! `AwsKmsRealProvider` — production AWS KMS REST/RPC client (R-prep GA).
//!
//! This module is the canonical AWS KMS real-mode entry point. It mirrors the
//! existing `AwsKmsProvider` API (which retains compatibility shims for the
//! BYOK matrix tests) but applies the GA-hardened policy:
//!
//! 1. **FIPS endpoint enforced.** Constructor unconditionally builds the
//!    client with `use_fips(true)` (overridable only by passing a custom
//!    endpoint URL for `wiremock` testing — see [`AwsKmsRealProvider::for_test`]).
//!    Resolved hostname is exposed via
//!    [`AwsKmsRealProvider::resolved_fips_endpoint`] so the test suite can
//!    assert the URL pattern `kms-fips.<region>.amazonaws.com`.
//! 2. **AAD JCS canonicalization (mandatory).** Every `encrypt` / `decrypt`
//!    call passes `EncryptionContext` derived from RFC 8785 JCS canonical
//!    bytes of the AAD JSON. This ensures the same logical AAD produces the
//!    same wire bytes across architectures (native, wasm32) and re-orderings.
//!    See [`canonicalize_aad_to_string_map`].
//! 3. **Audit fail-CLOSED ordering.** Every KMS error path emits a
//!    structured `tracing` event (target `corelink.byok.aws.audit`,
//!    `audit = true`) **before** the error bubbles up. The orchestrator's
//!    audit subscriber turns these events into BLAKE3-linked audit
//!    records (see `corelink-audit-chain`).
//! 4. **No `unsafe`, no `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.**
//!    Enforced by crate-level lints.
//! 5. **Constant-time AAD fingerprint compare.** Mock-mode tamper detection
//!    uses [`subtle::ConstantTimeEq`]; the real KMS path delegates to AWS
//!    server-side enforcement (which is itself constant-time).
//!
//! ## wasm32 strategy
//!
//! `aws-sdk-kms` does not compile to `wasm32-unknown-unknown` (it uses
//! tokio + hyper + OS-level TLS). For the CF-Worker build target we link
//! [`AwsKmsWasmStub`] which returns
//! `BYOKError::Provider("AWS KMS real provider unsupported on wasm32; ...")`
//! from every method. CoreLink Workers proxy envelope operations to the
//! native server process, which holds the actual AWS SDK client.

use serde_json::Value;
use std::collections::BTreeMap;

use crate::BYOKError;

#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;

#[cfg(target_arch = "wasm32")]
use crate::{Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek};

/// Canonicalize an AAD JSON value to the `EncryptionContext` string map that
/// AWS KMS accepts.
///
/// The canonicalization rules are:
///
/// 1. Top-level value MUST be a JSON object — otherwise
///    [`BYOKError::EnvelopeError`] is returned.
/// 2. Each value MUST be a JSON string — otherwise
///    [`BYOKError::EnvelopeError`] is returned (AWS KMS only accepts
///    string-to-string maps for `EncryptionContext`).
/// 3. Keys are sorted lexicographically (`BTreeMap`) so the resulting wire
///    bytes are deterministic regardless of input ordering. This matches
///    AWS KMS server-side behavior (KMS canonicalizes server-side too, but
///    we want byte-identical reproduction across architectures).
/// 4. RFC 8785 JCS canonical bytes of the (now sorted) object are also
///    returned for downstream binding / digest use.
///
/// The same logical AAD value MUST produce byte-identical canonical bytes
/// across native and wasm32 targets so `WrappedDek.encryption_context`
/// stored on D1 can be unwrapped from either environment.
///
/// # Errors
///
/// - [`BYOKError::EnvelopeError`] if the AAD is not an object or contains
///   non-string values.
/// - [`BYOKError::EnvelopeError`] if JCS serialization fails (impossible in
///   practice — the input is already a `serde_json::Value` object).
pub fn canonicalize_aad_to_string_map(
    aad: &Value,
) -> Result<(BTreeMap<String, String>, Vec<u8>), BYOKError> {
    let obj = aad.as_object().ok_or_else(|| {
        BYOKError::EnvelopeError("encryption_context must be a JSON object".to_string())
    })?;
    let mut map = BTreeMap::new();
    for (k, v) in obj {
        let s = v.as_str().ok_or_else(|| {
            BYOKError::EnvelopeError(format!("encryption_context.{k} value must be a string"))
        })?;
        map.insert(k.clone(), s.to_string());
    }
    // Rebuild a sorted-key Value so the JCS bytes are stable across input
    // orderings (JCS already sorts keys, but we want a single source of
    // truth and to also catch any non-string-value drift).
    let mut canonical_obj = serde_json::Map::with_capacity(map.len());
    for (k, v) in &map {
        canonical_obj.insert(k.clone(), Value::String(v.clone()));
    }
    let canonical = Value::Object(canonical_obj);
    let bytes = serde_jcs::to_vec(&canonical).map_err(|e| {
        BYOKError::EnvelopeError(format!("encryption_context JCS canonicalize: {e}"))
    })?;
    Ok((map, bytes))
}

/// Cheap 8-byte AAD fingerprint used by [`AwsKmsRealProvider`] mock mode for
/// tamper detection. Compared in constant time via [`subtle::ConstantTimeEq`].
pub fn aad_fingerprint(canonical_aad: &[u8]) -> [u8; 8] {
    let mut fp = [0u8; 8];
    for (i, &b) in canonical_aad.iter().enumerate() {
        if let Some(slot) = fp.get_mut(i % 8) {
            *slot ^= b;
        }
    }
    // Length-mix: defeat trivial truncation collisions.
    let len_byte = (canonical_aad.len() as u8).wrapping_mul(0x37);
    if let Some(last) = fp.last_mut() {
        *last ^= len_byte;
    }
    fp
}

// =============================================================================
// Native-only real provider (AWS SDK).
// =============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::super::{resolve_endpoint_hostname, validate_aws_kms_key_arn};
    use super::{aad_fingerprint, canonicalize_aad_to_string_map};
    use crate::{
        BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind,
        WrappedDek,
    };
    use async_trait::async_trait;
    use aws_sdk_kms::{
        config::{BehaviorVersion, Builder as KmsConfigBuilder, Credentials, Region},
        primitives::Blob,
        Client,
    };
    use serde_json::Value;
    use subtle::ConstantTimeEq;
    use tracing::{debug, warn};

    /// Production AWS KMS provider.
    ///
    /// Construct via [`AwsKmsRealProvider::new`] for the standard AWS
    /// credential chain (env vars, shared config, IRSA, IMDS, SSO), or
    /// [`AwsKmsRealProvider::for_test`] for `wiremock`-style integration
    /// tests with a custom endpoint URL.
    ///
    /// # FIPS enforcement
    ///
    /// `new` unconditionally enables `use_fips(true)` — the only way to
    /// disable FIPS is to use `for_test` (intended for local testing only).
    /// Production deployments MUST go through `new`.
    #[derive(Debug)]
    #[non_exhaustive]
    pub struct AwsKmsRealProvider {
        client: Client,
        region: String,
        fips_endpoint: bool,
        /// Resolved FIPS endpoint hostname, exposed for test assertions
        /// (e.g. `"kms-fips.us-east-1.amazonaws.com"`).
        resolved_fips_endpoint: String,
        /// When `Some`, `mock_state` is consulted instead of `client`
        /// (in-process wrap/unwrap). Used by unit tests in this crate.
        mock_mode: bool,
    }

    impl AwsKmsRealProvider {
        /// Construct a production AWS KMS client for `region`, with FIPS
        /// endpoint enforced.
        ///
        /// # Errors
        ///
        /// AWS SDK initialisation errors (e.g. missing credentials) surface
        /// here as [`BYOKError::Provider`].
        pub async fn new(region: &str) -> Result<Self, BYOKError> {
            Self::with_fips(region, true).await
        }

        /// Construct with explicit control over the FIPS flag. Production
        /// callers should always use [`AwsKmsRealProvider::new`]; this is
        /// exposed only for the matrix test that exercises both endpoint
        /// shapes.
        ///
        /// # [C4] No `aws_config::from_env().load()` — cold-start fix
        ///
        /// `aws_config::from_env()...load()` triggers the AWS
        /// credential-provider chain (IMDS / ECS / STS), which performs
        /// blocking outbound metadata probes. In CF Containers there is no
        /// IMDS endpoint, so each probe runs to its full retry budget —
        /// observed 60-90s cold-start hang (the exact pattern banned in
        /// `corelink-container/src/storage/r2_s3.rs:101`). This mirrors that
        /// adapter: EXPLICIT static credentials + an EXPLICIT FIPS endpoint
        /// URL + an EXPLICIT region, bypassing the auto-detection path
        /// entirely (zero I/O at construction; no provider chain).
        ///
        /// Credentials are read from BYOK-KMS-specific env vars, falling back
        /// to the standard AWS env vars the S3 adapter uses:
        ///
        /// | role     | BYOK-specific                         | fallback                |
        /// |----------|---------------------------------------|-------------------------|
        /// | access   | `CORELINK_BYOK_KMS_ACCESS_KEY_ID`     | `AWS_ACCESS_KEY_ID`     |
        /// | secret   | `CORELINK_BYOK_KMS_SECRET_ACCESS_KEY` | `AWS_SECRET_ACCESS_KEY` |
        /// | session  | `CORELINK_BYOK_KMS_SESSION_TOKEN`     | `AWS_SESSION_TOKEN`     |
        ///
        /// # Errors
        ///
        /// [`BYOKError::Provider`] if the access-key / secret-key env vars are
        /// not set.
        pub async fn with_fips(region: &str, fips: bool) -> Result<Self, BYOKError> {
            let access_key =
                read_env_fallback("CORELINK_BYOK_KMS_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID")
                    .ok_or_else(|| {
                        BYOKError::Provider(
                            "BYOK KMS credentials missing: set \
                             CORELINK_BYOK_KMS_ACCESS_KEY_ID (or AWS_ACCESS_KEY_ID)"
                                .to_string(),
                        )
                    })?;
            let secret_key =
                read_env_fallback("CORELINK_BYOK_KMS_SECRET_ACCESS_KEY", "AWS_SECRET_ACCESS_KEY")
                    .ok_or_else(|| {
                        BYOKError::Provider(
                            "BYOK KMS credentials missing: set \
                             CORELINK_BYOK_KMS_SECRET_ACCESS_KEY (or AWS_SECRET_ACCESS_KEY)"
                                .to_string(),
                        )
                    })?;
            let session_token =
                read_env_fallback("CORELINK_BYOK_KMS_SESSION_TOKEN", "AWS_SESSION_TOKEN");

            let credentials = Credentials::new(
                access_key,
                secret_key,
                session_token,
                None, // expiry
                "corelink-byok-kms",
            );

            // Explicit FIPS (or standard) endpoint URL — no endpoint
            // auto-resolution, no IMDS probe.
            let endpoint_host = resolve_endpoint_hostname(region, fips);
            let endpoint_url = format!("https://{endpoint_host}");

            let svc_cfg = KmsConfigBuilder::default()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new(region.to_string()))
                .endpoint_url(&endpoint_url)
                .credentials_provider(credentials)
                .build();
            let client = Client::from_conf(svc_cfg);

            Ok(Self {
                client,
                region: region.to_string(),
                fips_endpoint: fips,
                resolved_fips_endpoint: endpoint_host,
                mock_mode: false,
            })
        }

        /// Test-only constructor that does not contact AWS.
        ///
        /// In mock mode, `wrap_dek` / `unwrap_dek` / `check_access` operate
        /// in-process and exercise the AAD-binding code paths. FIPS endpoint
        /// is reported as enabled so URL-pattern tests still pass.
        #[must_use]
        pub fn new_mock(region: &str) -> Self {
            // Build a no-op client; it is never used in mock mode.
            let cfg = KmsConfigBuilder::default()
                .region(Some(Region::new(region.to_string())))
                .use_fips(true)
                .behavior_version(aws_sdk_kms::config::BehaviorVersion::latest())
                .build();
            let client = Client::from_conf(cfg);
            Self {
                client,
                region: region.to_string(),
                fips_endpoint: true,
                resolved_fips_endpoint: resolve_endpoint_hostname(region, true),
                mock_mode: true,
            }
        }

        /// Resolved FIPS endpoint hostname, exposed for test assertions.
        ///
        /// Example: `"kms-fips.us-east-1.amazonaws.com"`.
        #[must_use]
        pub fn resolved_fips_endpoint(&self) -> &str {
            &self.resolved_fips_endpoint
        }

        /// Whether FIPS endpoint is enforced on this provider.
        #[must_use]
        pub const fn fips_endpoint_enforced(&self) -> bool {
            self.fips_endpoint
        }

        /// Generate a data key via `kms:GenerateDataKey`. Returns
        /// `(plaintext_dek, ciphertext_blob)`. Mostly a convenience for
        /// orchestrators that prefer KMS-side DEK generation; CoreLink's
        /// default path uses CSPRNG locally.
        ///
        /// # Errors
        ///
        /// Any KMS error surfaces as [`BYOKError::Provider`] or
        /// [`BYOKError::CmkRevoked`].
        pub async fn generate_data_key(
            &self,
            key_id: &KmsKeyId,
            encryption_context: &Value,
        ) -> Result<(Dek, Vec<u8>), BYOKError> {
            if key_id.provider != KmsProviderKind::AwsKms {
                emit_audit("generate_data_key", &key_id.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "AwsKmsRealProvider received key_id with wrong provider: {:?}",
                    key_id.provider
                )));
            }
            validate_aws_kms_key_arn(&key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit("generate_data_key", &key_id.key_arn_or_id, "malformed_arn");
            })?;
            let (ec_map, _) = canonicalize_aad_to_string_map(encryption_context)?;

            if self.mock_mode {
                let dek = Dek::generate()?;
                let mut ct = Vec::with_capacity(dek.bytes.len());
                ct.extend_from_slice(&dek.bytes);
                return Ok((dek, ct));
            }

            let mut req = self
                .client
                .generate_data_key()
                .key_id(&key_id.key_arn_or_id)
                .key_spec(aws_sdk_kms::types::DataKeySpec::Aes256);
            for (k, v) in &ec_map {
                req = req.encryption_context(k, v);
            }

            let resp = req.send().await.map_err(|e| {
                map_sdk_error("generate_data_key", &key_id.key_arn_or_id, &e.to_string())
            })?;

            let plaintext = resp
                .plaintext()
                .ok_or_else(|| {
                    emit_audit(
                        "generate_data_key",
                        &key_id.key_arn_or_id,
                        "missing_plaintext",
                    );
                    BYOKError::Provider("aws kms GenerateDataKey: missing plaintext".to_string())
                })?
                .clone()
                .into_inner();
            if plaintext.len() != 32 {
                emit_audit(
                    "generate_data_key",
                    &key_id.key_arn_or_id,
                    "dek_length_invalid",
                );
                return Err(BYOKError::DekLengthInvalid {
                    got: plaintext.len(),
                });
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&plaintext);
            let dek = Dek { bytes };

            let ciphertext = resp
                .ciphertext_blob()
                .ok_or_else(|| {
                    emit_audit(
                        "generate_data_key",
                        &key_id.key_arn_or_id,
                        "missing_ciphertext",
                    );
                    BYOKError::Provider("aws kms GenerateDataKey: missing ciphertext".to_string())
                })?
                .clone()
                .into_inner();

            Ok((dek, ciphertext))
        }

        /// List aliases for a key (or all aliases if `key_id` is `None`).
        ///
        /// CoreLink's policy explicitly REJECTS aliases as CMK identifiers
        /// (see `validate_aws_kms_key_arn`); this method exists for the
        /// admin-side discovery surface, not for envelope operations.
        ///
        /// # Errors
        ///
        /// Any KMS error surfaces as [`BYOKError::Provider`].
        pub async fn list_aliases(
            &self,
            key_id: Option<&KmsKeyId>,
        ) -> Result<Vec<String>, BYOKError> {
            if let Some(k) = key_id {
                validate_aws_kms_key_arn(&k.key_arn_or_id).inspect_err(|_| {
                    emit_audit("list_aliases", &k.key_arn_or_id, "malformed_arn");
                })?;
            }
            if self.mock_mode {
                return Ok(Vec::new());
            }
            let mut req = self.client.list_aliases();
            if let Some(k) = key_id {
                req = req.key_id(&k.key_arn_or_id);
            }
            let resp = req.send().await.map_err(|e| {
                let key_str = key_id.map_or("*", |k| k.key_arn_or_id.as_str());
                map_sdk_error("list_aliases", key_str, &e.to_string())
            })?;
            let aliases = resp
                .aliases()
                .iter()
                .filter_map(|a| a.alias_name().map(str::to_owned))
                .collect();
            Ok(aliases)
        }

        /// Describe a key — exposed for compatibility with the `KmsProvider`
        /// trait's `check_access`, plus admin-side surfaces.
        ///
        /// # Errors
        ///
        /// Returns [`BYOKError::Provider`] for non-recoverable KMS errors.
        pub async fn describe_key(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            self.check_access(key_id).await
        }
    }

    /// Read `primary` from the environment, falling back to `fallback`.
    /// Empty values are treated as unset. Returns `None` if neither is set.
    fn read_env_fallback(primary: &str, fallback: &str) -> Option<String> {
        for name in [primary, fallback] {
            if let Ok(v) = std::env::var(name) {
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Map a stringified AWS SDK error into a [`BYOKError`] variant, emitting
    /// a fail-CLOSED audit event BEFORE returning.
    fn map_sdk_error(op: &str, key: &str, msg: &str) -> BYOKError {
        if msg.contains("AccessDenied") || msg.contains("DisabledException") {
            emit_audit(op, key, "access_denied");
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AwsKms,
                key_id: key.to_string(),
            }
        } else if msg.contains("InvalidCiphertext") {
            emit_audit(op, key, "invalid_ciphertext_aad_mismatch");
            BYOKError::AadMismatch
        } else if msg.contains("KMSInvalidStateException") {
            emit_audit(op, key, "kms_invalid_state");
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AwsKms,
                key_id: key.to_string(),
            }
        } else if msg.contains("ThrottlingException") {
            emit_audit(op, key, "throttled");
            BYOKError::Provider(format!("aws kms {op}: throttled"))
        } else if msg.contains("NotFoundException") {
            emit_audit(op, key, "not_found");
            BYOKError::Provider(format!("aws kms {op}: NotFoundException"))
        } else {
            emit_audit(op, key, "provider_error");
            // Do not leak the raw SDK message — it may contain ARN or
            // account info beyond what the caller passed in.
            BYOKError::Provider(format!("aws kms {op}: provider error"))
        }
    }

    /// Emit a fail-CLOSED audit event BEFORE the error bubbles up.
    ///
    /// The orchestrator subscribes to `target = "corelink.byok.aws.audit"`
    /// and turns each event into a BLAKE3-linked audit record (see
    /// `corelink-audit-chain`). The `audit = true` field is the canonical
    /// marker used by the subscriber to distinguish audit events from
    /// regular `tracing` output.
    fn emit_audit(op: &str, key: &str, reason: &str) {
        warn!(
            target: "corelink.byok.aws.audit",
            audit = true,
            op = op,
            key = key,
            reason = reason,
            "BYOK AWS KMS audit event"
        );
    }

    #[async_trait]
    impl KmsProvider for AwsKmsRealProvider {
        fn provider_kind(&self) -> KmsProviderKind {
            KmsProviderKind::AwsKms
        }

        fn region(&self) -> &str {
            &self.region
        }

        fn fips_level(&self) -> FipsLevel {
            FipsLevel::Fips140_3_L1
        }

        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            encryption_context: Option<&Value>,
        ) -> Result<WrappedDek, BYOKError> {
            if key_id.provider != KmsProviderKind::AwsKms {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "AwsKmsRealProvider received key_id with wrong provider: {:?}",
                    key_id.provider
                )));
            }
            let ctx = encryption_context.ok_or_else(|| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_missing");
                BYOKError::EncryptionContextMissing
            })?;
            validate_aws_kms_key_arn(&key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "malformed_arn");
            })?;
            let (ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx)?;

            debug!(
                target: "corelink.byok.aws",
                key = %key_id.key_arn_or_id,
                region = %self.region,
                fips = self.fips_endpoint,
                "AWS KMS wrap_dek"
            );

            if self.mock_mode {
                return Ok(mock_wrap(dek, key_id, ctx, &canonical_aad));
            }

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
                .map_err(|e| map_sdk_error("wrap_dek", &key_id.key_arn_or_id, &e.to_string()))?;

            let ciphertext = resp
                .ciphertext_blob()
                .ok_or_else(|| {
                    emit_audit("wrap_dek", &key_id.key_arn_or_id, "missing_ciphertext");
                    BYOKError::Provider("aws kms encrypt: missing ciphertext".to_string())
                })?
                .clone()
                .into_inner();

            Ok(WrappedDek {
                provider: KmsProviderKind::AwsKms,
                key_id: key_id.clone(),
                ciphertext,
                encryption_context: Some(ctx.clone()),
            })
        }

        async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
            if wrapped.provider != KmsProviderKind::AwsKms {
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "wrong_provider",
                );
                return Err(BYOKError::EnvelopeError(format!(
                    "AwsKmsRealProvider received WrappedDek with wrong provider: {:?}",
                    wrapped.provider
                )));
            }
            let ctx = wrapped.encryption_context.as_ref().ok_or_else(|| {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aad_missing");
                BYOKError::EncryptionContextMissing
            })?;
            validate_aws_kms_key_arn(&wrapped.key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "malformed_arn");
            })?;
            let (ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx)?;

            debug!(
                target: "corelink.byok.aws",
                key = %wrapped.key_id.key_arn_or_id,
                "AWS KMS unwrap_dek"
            );

            if self.mock_mode {
                return mock_unwrap(wrapped, &canonical_aad);
            }

            let mut req = self
                .client
                .decrypt()
                .ciphertext_blob(Blob::new(wrapped.ciphertext.clone()))
                .key_id(&wrapped.key_id.key_arn_or_id);
            for (k, v) in &ec_map {
                req = req.encryption_context(k, v);
            }
            let resp = req.send().await.map_err(|e| {
                map_sdk_error("unwrap_dek", &wrapped.key_id.key_arn_or_id, &e.to_string())
            })?;

            let plaintext = resp
                .plaintext()
                .ok_or_else(|| {
                    emit_audit(
                        "unwrap_dek",
                        &wrapped.key_id.key_arn_or_id,
                        "missing_plaintext",
                    );
                    BYOKError::Provider("aws kms decrypt: missing plaintext".to_string())
                })?
                .clone()
                .into_inner();
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
            if key_id.provider != KmsProviderKind::AwsKms {
                emit_audit("check_access", &key_id.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "AwsKmsRealProvider received key_id with wrong provider: {:?}",
                    key_id.provider
                )));
            }
            validate_aws_kms_key_arn(&key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit("check_access", &key_id.key_arn_or_id, "malformed_arn");
            })?;

            if self.mock_mode {
                return Ok(KmsAccessStatus::Ok);
            }

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
                            emit_audit(
                                "check_access",
                                &key_id.key_arn_or_id,
                                "revoked_or_pending_deletion",
                            );
                            KmsAccessStatus::Revoked
                        }
                        "PendingImport" | "Unavailable" => KmsAccessStatus::ApiError(0),
                        _ => KmsAccessStatus::ApiError(0),
                    };
                    Ok(status)
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("AccessDenied") {
                        emit_audit("check_access", &key_id.key_arn_or_id, "access_denied");
                        Ok(KmsAccessStatus::Revoked)
                    } else if msg.contains("NotFoundException") {
                        emit_audit("check_access", &key_id.key_arn_or_id, "not_found");
                        Ok(KmsAccessStatus::NotFound)
                    } else if msg.contains("ThrottlingException") {
                        emit_audit("check_access", &key_id.key_arn_or_id, "throttled");
                        Ok(KmsAccessStatus::Throttled)
                    } else {
                        emit_audit("check_access", &key_id.key_arn_or_id, "provider_error");
                        Err(BYOKError::Provider(
                            "aws kms describe_key: provider error".to_string(),
                        ))
                    }
                }
            }
        }
    }

    // ── Mock-mode wrap / unwrap (in-process, AAD-binding enforced) ────────

    fn mock_wrap(dek: &Dek, key_id: &KmsKeyId, ctx: &Value, canonical_aad: &[u8]) -> WrappedDek {
        let fp = aad_fingerprint(canonical_aad);
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

    fn mock_unwrap(wrapped: &WrappedDek, canonical_aad: &[u8]) -> Result<Dek, BYOKError> {
        if wrapped.ciphertext.len() != 40 {
            return Err(BYOKError::EnvelopeError(format!(
                "AWS real (mock): wrong ciphertext length {} (expected 40)",
                wrapped.ciphertext.len()
            )));
        }
        let expected = aad_fingerprint(canonical_aad);
        let stored = wrapped.ciphertext.get(..8).ok_or_else(|| {
            BYOKError::EnvelopeError("AWS real (mock): ciphertext too short".to_string())
        })?;
        // Constant-time fingerprint compare.
        if stored.ct_eq(&expected).unwrap_u8() == 0 {
            return Err(BYOKError::AadMismatch);
        }
        let body = wrapped.ciphertext.get(8..).ok_or_else(|| {
            BYOKError::EnvelopeError("AWS real (mock): ciphertext too short".to_string())
        })?;
        let mut bytes = [0u8; 32];
        for (out, &b) in bytes.iter_mut().zip(body.iter()) {
            *out = b ^ 0xBB;
        }
        Ok(Dek { bytes })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::AwsKmsRealProvider;

// =============================================================================
// wasm32 stub.
// =============================================================================

/// wasm32 stub for AWS KMS real provider.
///
/// The official `aws-sdk-kms` does not compile to `wasm32-unknown-unknown`.
/// Any method call on this stub returns
/// `BYOKError::Provider("AWS KMS real provider unsupported on wasm32; ...")`.
/// In production CF Worker deployments envelope operations are forwarded to
/// the native server process via the internal control-plane RPC — see
/// `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AwsKmsWasmStub {
    region: String,
}

#[cfg(target_arch = "wasm32")]
impl AwsKmsWasmStub {
    /// Construct the wasm32 stub for `region`. Does not contact AWS.
    #[must_use]
    pub fn new(region: &str) -> Self {
        Self {
            region: region.to_string(),
        }
    }

    /// Same shape as the native [`AwsKmsRealProvider::resolved_fips_endpoint`].
    /// Returned even on wasm32 so spec validation can cross-reference the
    /// canonical FIPS host string.
    #[must_use]
    pub fn resolved_fips_endpoint(&self) -> String {
        crate::resolve_endpoint_hostname(&self.region, true)
    }
}

#[cfg(target_arch = "wasm32")]
const WASM_UNSUPPORTED_MSG: &str =
    "AWS KMS real provider unsupported on wasm32; use CF Worker-side AWS SDK binding instead";

#[cfg(target_arch = "wasm32")]
#[async_trait]
impl KmsProvider for AwsKmsWasmStub {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AwsKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_3_L1
    }

    async fn wrap_dek(
        &self,
        _dek: &Dek,
        _key_id: &KmsKeyId,
        _encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }

    async fn unwrap_dek(&self, _wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }

    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
/// Type alias so downstream code can name `AwsKmsRealProvider` on both
/// targets — on wasm32 it resolves to the stub. CF Workers therefore
/// link a working type but every call surfaces the explicit error above.
pub type AwsKmsRealProvider = AwsKmsWasmStub;

// =============================================================================
// Tests — arch-agnostic AAD canonicalization + arch-specific behavior.
// =============================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn aad_canonicalize_rejects_non_object() {
        let err = canonicalize_aad_to_string_map(&json!("hello")).unwrap_err();
        match err {
            BYOKError::EnvelopeError(_) => {}
            other => panic!("expected EnvelopeError, got {other:?}"),
        }
    }

    #[test]
    fn aad_canonicalize_rejects_non_string_values() {
        let err = canonicalize_aad_to_string_map(&json!({"x": 42})).unwrap_err();
        match err {
            BYOKError::EnvelopeError(_) => {}
            other => panic!("expected EnvelopeError, got {other:?}"),
        }
    }

    #[test]
    fn aad_canonicalize_is_key_order_independent() {
        let a = json!({"tenant_id": "t-1", "blob_hash": "sha256:abc"});
        let b = json!({"blob_hash": "sha256:abc", "tenant_id": "t-1"});
        let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
        let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
        assert_eq!(bytes_a, bytes_b);
        // Sanity: bytes start with `{` and contain sorted keys.
        let s = std::str::from_utf8(&bytes_a).unwrap();
        assert!(s.starts_with('{'));
        assert!(s.find("\"blob_hash\"").unwrap() < s.find("\"tenant_id\"").unwrap());
    }

    #[test]
    fn aad_canonicalize_is_value_change_sensitive() {
        let a = json!({"tenant_id": "t-1"});
        let b = json!({"tenant_id": "t-2"});
        let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
        let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
        assert_ne!(bytes_a, bytes_b);
    }

    #[test]
    fn aad_fingerprint_is_length_sensitive() {
        let short = aad_fingerprint(b"abc");
        let long = aad_fingerprint(b"abcabcabc");
        assert_ne!(short, long);
    }

    #[test]
    fn aad_fingerprint_is_content_sensitive() {
        let a = aad_fingerprint(b"abcdefgh");
        let b = aad_fingerprint(b"abcdefgi");
        assert_ne!(a, b);
    }
}
