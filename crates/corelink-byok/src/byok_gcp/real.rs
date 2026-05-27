//! `GcpKmsRealProvider` — production GCP Cloud KMS REST/RPC client (R-prep GA).
//!
//! This module is the canonical GCP Cloud KMS real-mode entry point. It mirrors
//! the AWS reference implementation
//! (`crates/corelink-byok-aws/src/real.rs`) and applies the GA-hardened
//! charter constraints documented in
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`:
//!
//! 1. **FIPS endpoint enforced where GCP offers it.** Constructor uses the
//!    canonical Cloud KMS endpoint `cloudkms.googleapis.com` by default and
//!    accepts a regional FIPS variant (`cloudkms.<region>.rep.googleapis.com`)
//!    via [`GcpKmsRealProvider::with_endpoint`]. Resolved hostname is exposed
//!    via [`GcpKmsRealProvider::resolved_fips_endpoint`] so the test suite can
//!    assert the URL pattern. NOTE: GCP's FedRAMP-High regional FIPS endpoints
//!    are scoped to a subset of regions — non-FedRAMP regions resolve to the
//!    canonical `cloudkms.googleapis.com` which is FIPS 140-2 Level 1 backed
//!    by Google's underlying BoringCrypto module. See
//!    `compliance/byok-fips-matrix.md`.
//! 2. **AAD JCS canonicalization (mandatory).** Every `wrap_dek` / `unwrap_dek`
//!    call serializes `encryption_context` via RFC 8785 JCS canonical bytes
//!    (see [`canonicalize_aad_to_string_map`]) and passes those bytes as
//!    `additionalAuthenticatedData` to Cloud KMS. Same logical AAD therefore
//!    produces byte-identical wire bytes across architectures (native, wasm32)
//!    and re-orderings — the INV-BYOK-CRYPTO-SOVEREIGNTY determinism contract.
//! 3. **Audit fail-CLOSED ordering.** Every KMS error path emits a structured
//!    `tracing` event (`target = "corelink.byok.gcp.audit"`, `audit = true`)
//!    BEFORE the error bubbles up. The orchestrator turns each event into a
//!    BLAKE3-linked audit record (see `corelink-audit-chain`).
//! 4. **No `unsafe`, no `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.**
//!    Enforced by crate-level lints.
//! 5. **Constant-time AAD fingerprint compare.** Mock-mode tamper detection
//!    uses [`subtle::ConstantTimeEq`]; the real Cloud KMS path delegates to
//!    Google server-side AEAD verification (which is itself constant-time).
//!
//! # wasm32 strategy
//!
//! `reqwest` (with default features) and the ADC stack do not compile to
//! `wasm32-unknown-unknown` (tokio + hyper + OS-level TLS). For the CF-Worker
//! build target we link [`GcpKmsWasmStub`] which returns
//! `BYOKError::Provider("GCP KMS real provider unsupported on wasm32; ...")`
//! from every method. CoreLink Workers proxy envelope operations to the
//! native server process via the internal control-plane RPC.

use serde_json::Value;
use std::collections::BTreeMap;

use crate::BYOKError;

#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;

#[cfg(target_arch = "wasm32")]
use crate::{
    Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

/// Default (non-FIPS-regional) Cloud KMS REST endpoint hostname.
///
/// Google operates Cloud KMS as a global service rooted at this host; FIPS
/// 140-2 Level 1 enforcement is automatic via the BoringCrypto module. For
/// FedRAMP-High workloads, callers MUST construct via
/// [`GcpKmsRealProvider::with_endpoint`] with the regional FIPS URL
/// `cloudkms.<region>.rep.googleapis.com`.
pub const DEFAULT_GCP_KMS_HOST: &str = "cloudkms.googleapis.com";

/// Resolve the Cloud KMS hostname for `region`, optionally selecting the
/// FedRAMP-High regional FIPS variant.
///
/// Per the audit at `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`
/// (§3), FedRAMP-High FIPS endpoints follow the shape
/// `cloudkms.<region>.rep.googleapis.com`. All other regions resolve to the
/// canonical `cloudkms.googleapis.com` (a global host; the region is then
/// embedded in the resource path, not the hostname).
#[must_use]
pub fn resolve_endpoint_hostname(region: &str, fips_regional: bool) -> String {
    if fips_regional && !region.is_empty() {
        format!("cloudkms.{region}.rep.googleapis.com")
    } else {
        DEFAULT_GCP_KMS_HOST.to_string()
    }
}

/// Canonicalize an AAD JSON value to a (`BTreeMap<String, String>`, JCS bytes)
/// pair suitable for binding into Cloud KMS `additionalAuthenticatedData`.
///
/// The canonicalization rules match the AWS adapter byte-for-byte (the
/// contract is fixed by INV-BYOK-CRYPTO-SOVEREIGNTY and applies to all
/// providers):
///
/// 1. Top-level value MUST be a JSON object — otherwise
///    [`BYOKError::EnvelopeError`] is returned.
/// 2. Each value MUST be a JSON string — otherwise
///    [`BYOKError::EnvelopeError`] is returned. While Cloud KMS technically
///    accepts arbitrary bytes for `additionalAuthenticatedData`, CoreLink
///    enforces the same string-to-string shape across all providers so the
///    AAD wire shape is provider-portable.
/// 3. Keys are sorted lexicographically (`BTreeMap`) so the resulting wire
///    bytes are deterministic regardless of input ordering.
/// 4. RFC 8785 JCS canonical bytes of the (now sorted) object are returned;
///    these are what gets passed to Cloud KMS as
///    `additionalAuthenticatedData` (base64-encoded on the wire).
///
/// The same logical AAD value MUST produce byte-identical canonical bytes
/// across native and wasm32 targets so `WrappedDek.encryption_context` stored
/// on D1 can be unwrapped from either environment.
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
            BYOKError::EnvelopeError(format!(
                "encryption_context.{k} value must be a string"
            ))
        })?;
        map.insert(k.clone(), s.to_string());
    }
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

/// Cheap 8-byte AAD fingerprint used by mock mode for tamper detection.
/// Compared in constant time via [`subtle::ConstantTimeEq`].
///
/// Not cryptographically secure — production Cloud KMS path delegates to
/// server-side AEAD verification.
#[must_use]
pub fn aad_fingerprint(canonical_aad: &[u8]) -> [u8; 8] {
    let mut fp = [0u8; 8];
    for (i, &b) in canonical_aad.iter().enumerate() {
        if let Some(slot) = fp.get_mut(i % 8) {
            *slot ^= b;
        }
    }
    let len_byte = (canonical_aad.len() as u8).wrapping_mul(0x37);
    if let Some(last) = fp.last_mut() {
        *last ^= len_byte;
    }
    fp
}

// =============================================================================
// Native real provider (Cloud KMS REST client + ADC).
//
// Gated by both:
//   - `not(target_arch = "wasm32")` — REST stack does not compile to wasm32.
//   - `feature = "production-gcp"`     — pulls in `reqwest`, `base64`,
//                                    `jsonwebtoken` (see Cargo.toml).
//
// When `production` is off, only the JCS canonicalization helpers + wasm
// stub are reachable. The server feature flag `byok-gcp-real`
// (apps/server/Cargo.toml) is the production toggle.
// =============================================================================

#[cfg(all(not(target_arch = "wasm32"), feature = "production-gcp"))]
mod native {
    use super::{
        aad_fingerprint, canonicalize_aad_to_string_map, resolve_endpoint_hostname,
        DEFAULT_GCP_KMS_HOST,
    };
    use async_trait::async_trait;
    use base64::Engine as _;
    use crate::{
        BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind,
        WrappedDek,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use subtle::ConstantTimeEq;
    use tracing::{debug, warn};

    use super::super::adc::AdcCredentials;

    fn b64_encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn b64_decode(s: &str) -> Result<Vec<u8>, BYOKError> {
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| BYOKError::EnvelopeError(format!("base64 decode: {e}")))
    }

    /// Production GCP Cloud KMS provider.
    ///
    /// Construct via [`GcpKmsRealProvider::new`] for the canonical
    /// (`cloudkms.googleapis.com`) endpoint with ADC, or
    /// [`GcpKmsRealProvider::with_endpoint`] to target a FedRAMP-High regional
    /// FIPS host. [`GcpKmsRealProvider::new_mock`] is for in-crate unit tests
    /// — it exercises the AAD-binding code paths without any network egress.
    ///
    /// # FIPS enforcement
    ///
    /// The default endpoint (`cloudkms.googleapis.com`) is FIPS 140-2 Level 1
    /// backed by Google's BoringCrypto module. For FedRAMP-High workloads,
    /// callers MUST pass the regional FIPS URL
    /// (`cloudkms.<region>.rep.googleapis.com`) to
    /// [`GcpKmsRealProvider::with_endpoint`]; the FIPS state is then surfaced
    /// via [`GcpKmsRealProvider::resolved_fips_endpoint`] for test assertion.
    #[derive(Debug, Clone)]
    #[non_exhaustive]
    pub struct GcpKmsRealProvider {
        http: reqwest::Client,
        creds: Option<AdcCredentials>,
        endpoint: String,
        /// Resolved hostname (without scheme / trailing slash) for the FIPS
        /// attestation cross-reference. Example values:
        /// `"cloudkms.googleapis.com"` (default global) or
        /// `"cloudkms.us-east1.rep.googleapis.com"` (FedRAMP-High regional FIPS).
        resolved_fips_endpoint: String,
        region: String,
        /// `true` when constructed with the FedRAMP-High regional FIPS host.
        fips_regional: bool,
        /// When `true`, `wrap_dek` / `unwrap_dek` / `check_access` operate
        /// in-process and exercise the AAD-binding code paths. Used by unit
        /// tests in this crate.
        mock_mode: bool,
    }

    impl GcpKmsRealProvider {
        /// Construct a production GCP Cloud KMS provider using ADC and the
        /// canonical `cloudkms.googleapis.com` endpoint.
        ///
        /// # Errors
        ///
        /// Returns [`BYOKError::Provider`] if HTTP client init fails or ADC
        /// credentials cannot be resolved.
        pub async fn new(region: &str) -> Result<Self, BYOKError> {
            Self::with_endpoint(region, &format!("https://{DEFAULT_GCP_KMS_HOST}")).await
        }

        /// Construct with a custom endpoint URL. Use this for the FedRAMP-High
        /// regional FIPS host (`https://cloudkms.<region>.rep.googleapis.com`)
        /// or for `wiremock` tests.
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
            let endpoint_trim = endpoint.trim_end_matches('/').to_string();
            let host = hostname_of(&endpoint_trim);
            let fips_regional = host.ends_with(".rep.googleapis.com");
            Ok(Self {
                http,
                creds: Some(creds),
                endpoint: endpoint_trim,
                resolved_fips_endpoint: host,
                region: region.to_string(),
                fips_regional,
                mock_mode: false,
            })
        }

        /// Test-only constructor that injects credentials wholesale. Useful
        /// for `wiremock` integration tests where real ADC must be bypassed.
        #[doc(hidden)]
        #[must_use]
        pub fn for_test(
            http: reqwest::Client,
            creds: AdcCredentials,
            region: &str,
            endpoint: &str,
        ) -> Self {
            let endpoint_trim = endpoint.trim_end_matches('/').to_string();
            let host = hostname_of(&endpoint_trim);
            let fips_regional = host.ends_with(".rep.googleapis.com");
            Self {
                http,
                creds: Some(creds),
                endpoint: endpoint_trim,
                resolved_fips_endpoint: host,
                region: region.to_string(),
                fips_regional,
                mock_mode: false,
            }
        }

        /// Test-only constructor that does not contact GCP and bypasses the
        /// HTTP / ADC stack entirely.
        ///
        /// In mock mode, `wrap_dek` / `unwrap_dek` / `check_access` operate
        /// in-process and exercise the AAD-binding code paths. FIPS endpoint
        /// hostname is reported as `cloudkms.<region>.rep.googleapis.com` so
        /// URL-pattern tests still pass.
        #[must_use]
        pub fn new_mock(region: &str) -> Self {
            let host = resolve_endpoint_hostname(region, true);
            let endpoint = format!("https://{host}");
            Self {
                http: reqwest::Client::new(),
                creds: None,
                endpoint,
                resolved_fips_endpoint: host,
                region: region.to_string(),
                fips_regional: true,
                mock_mode: true,
            }
        }

        /// Resolved Cloud KMS hostname, exposed for test assertions and
        /// audit cross-reference against `BYOK-FIPS-ATTESTATION-MATRIX.md`.
        ///
        /// Example values:
        /// - `"cloudkms.googleapis.com"` (default global, FIPS 140-2 L1).
        /// - `"cloudkms.us-east1.rep.googleapis.com"` (FedRAMP-High regional FIPS).
        #[must_use]
        pub fn resolved_fips_endpoint(&self) -> &str {
            &self.resolved_fips_endpoint
        }

        /// Whether the provider is bound to a FedRAMP-High regional FIPS
        /// hostname. Global `cloudkms.googleapis.com` is FIPS 140-2 L1 but
        /// not the regional FIPS variant; this returns `false` for the
        /// global host.
        #[must_use]
        pub const fn fips_endpoint_enforced(&self) -> bool {
            self.fips_regional
        }

        fn validate_key_resource(key_arn_or_id: &str) -> Result<(), BYOKError> {
            if !super::super::key_resource::is_valid_cloud_kms_resource(key_arn_or_id) {
                return Err(BYOKError::Provider(format!(
                    "malformed Cloud KMS key resource: '{key_arn_or_id}' \
                     (expected projects/<P>/locations/<L>/keyRings/<R>/cryptoKeys/<K>[/cryptoKeyVersions/<V>])"
                )));
            }
            Ok(())
        }

        async fn bearer(&self) -> Result<String, BYOKError> {
            match &self.creds {
                Some(c) => c.access_token().await,
                None => Err(BYOKError::Provider(
                    "GCP KMS: bearer requested with no credentials (mock mode misuse)".to_string(),
                )),
            }
        }
    }

    // -- REST wire types (Cloud KMS v1) --------------------------------------

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
        #[serde(default)]
        state: String,
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
        #[allow(dead_code)]
        status: String,
    }

    fn parse_api_error(body: &str) -> ApiErrorInner {
        serde_json::from_str::<ApiErrorBody>(body)
            .map(|e| e.error)
            .unwrap_or_default()
    }

    fn hostname_of(endpoint: &str) -> String {
        let no_scheme = endpoint
            .strip_prefix("https://")
            .or_else(|| endpoint.strip_prefix("http://"))
            .unwrap_or(endpoint);
        no_scheme
            .split('/')
            .next()
            .unwrap_or(no_scheme)
            .split(':')
            .next()
            .unwrap_or(no_scheme)
            .to_string()
    }

    /// Emit a fail-CLOSED audit event BEFORE the error bubbles up.
    ///
    /// Mirrors the AWS adapter (`corelink.byok.aws.audit`). The
    /// orchestrator subscribes to `target = "corelink.byok.gcp.audit"` and
    /// turns each event into a BLAKE3-linked audit record (see
    /// `corelink-audit-chain`). The `audit = true` field is the canonical
    /// marker used by the subscriber to distinguish audit events from
    /// regular `tracing` output.
    fn emit_audit(op: &str, key: &str, reason: &str) {
        warn!(
            target: "corelink.byok.gcp.audit",
            audit = true,
            op = op,
            key = key,
            reason = reason,
            "BYOK GCP KMS audit event"
        );
    }

    /// Map an HTTP error response into a [`BYOKError`] variant, emitting a
    /// fail-CLOSED audit event BEFORE returning.
    async fn map_http_error(
        resp: reqwest::Response,
        key_id: &KmsKeyId,
        op: &str,
    ) -> BYOKError {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let api = parse_api_error(&body);
        let key = key_id.key_arn_or_id.as_str();

        let msg_lc = api.message.to_ascii_lowercase();
        if status.as_u16() == 400
            && (msg_lc.contains("additional_authenticated_data")
                || msg_lc.contains("authentication tag")
                || msg_lc.contains("checksum")
                || msg_lc.contains("authentication"))
        {
            emit_audit(op, key, "invalid_ciphertext_aad_mismatch");
            return BYOKError::AadMismatch;
        }
        if status.as_u16() == 403 {
            emit_audit(op, key, "access_denied");
            return BYOKError::CmkRevoked {
                provider: KmsProviderKind::GcpKms,
                key_id: key.to_string(),
            };
        }
        if status.as_u16() == 404 {
            emit_audit(op, key, "not_found");
            return BYOKError::CmkRevoked {
                provider: KmsProviderKind::GcpKms,
                key_id: key.to_string(),
            };
        }
        if status.as_u16() == 429 {
            emit_audit(op, key, "throttled");
            return BYOKError::Provider(format!("gcp kms {op}: throttled"));
        }
        emit_audit(op, key, "provider_error");
        BYOKError::Provider(format!("gcp kms {op}: provider error"))
    }

    fn map_state_to_access(state: &str) -> KmsAccessStatus {
        match state {
            "ENABLED" => KmsAccessStatus::Ok,
            "DISABLED" | "DESTROY_SCHEDULED" => KmsAccessStatus::Revoked,
            "DESTROYED" => KmsAccessStatus::NotFound,
            "PENDING_GENERATION" | "PENDING_IMPORT" | "IMPORT_FAILED" => {
                KmsAccessStatus::ApiError(0)
            }
            _ => KmsAccessStatus::ApiError(0),
        }
    }

    fn map_get_error_to_access(http_status: u16) -> KmsAccessStatus {
        match http_status {
            403 => KmsAccessStatus::Revoked,
            404 => KmsAccessStatus::NotFound,
            429 => KmsAccessStatus::Throttled,
            other => KmsAccessStatus::ApiError(other),
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
            // Default Cloud KMS = FIPS 140-2 L1 (BoringCrypto). HSM-tier
            // keys are logically L3 but require a network roundtrip to
            // confirm; orchestrator enforces tier policy.
            FipsLevel::Fips140_2_L1
        }

        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            encryption_context: Option<&Value>,
        ) -> Result<WrappedDek, BYOKError> {
            if key_id.provider != KmsProviderKind::GcpKms {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "GcpKmsRealProvider received key_id with wrong provider: {:?}",
                    key_id.provider
                )));
            }
            let ctx = encryption_context.ok_or_else(|| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_missing");
                BYOKError::EncryptionContextMissing
            })?;
            Self::validate_key_resource(&key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "malformed_arn");
            })?;
            let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx)?;

            debug!(
                target: "corelink.byok.gcp",
                key = %key_id.key_arn_or_id,
                region = %self.region,
                fips_regional = self.fips_regional,
                "GCP KMS wrap_dek"
            );

            if self.mock_mode {
                return Ok(mock_wrap(dek, key_id, ctx, &canonical_aad));
            }

            let token = self.bearer().await?;
            let url = format!(
                "{}/v1/{}:encrypt",
                self.endpoint,
                key_id.key_arn_or_id.trim_end_matches('/')
            );
            let body = EncryptRequest {
                plaintext: b64_encode(&dek.bytes),
                additional_authenticated_data: b64_encode(&canonical_aad),
            };

            let resp = self
                .http
                .post(&url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    emit_audit("wrap_dek", &key_id.key_arn_or_id, "provider_error");
                    BYOKError::Provider(format!("gcp kms encrypt POST: {e}"))
                })?;

            if !resp.status().is_success() {
                return Err(map_http_error(resp, key_id, "wrap_dek").await);
            }

            let er: EncryptResponse = resp.json().await.map_err(|e| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "provider_error");
                BYOKError::Provider(format!("gcp kms encrypt JSON parse: {e}"))
            })?;
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
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "wrong_provider",
                );
                return Err(BYOKError::EnvelopeError(format!(
                    "GcpKmsRealProvider received WrappedDek with wrong provider: {:?}",
                    wrapped.provider
                )));
            }
            let ctx = wrapped.encryption_context.as_ref().ok_or_else(|| {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aad_missing");
                BYOKError::EncryptionContextMissing
            })?;
            Self::validate_key_resource(&wrapped.key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "malformed_arn",
                );
            })?;
            let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx)?;

            debug!(
                target: "corelink.byok.gcp",
                key = %wrapped.key_id.key_arn_or_id,
                "GCP KMS unwrap_dek"
            );

            if self.mock_mode {
                return mock_unwrap(wrapped, &canonical_aad);
            }

            // Decrypt MUST target the parent CryptoKey (Cloud KMS auto-selects
            // the version from the ciphertext header).
            let parent = super::super::key_resource::strip_version_suffix(&wrapped.key_id.key_arn_or_id);
            let token = self.bearer().await?;
            let url = format!("{}/v1/{}:decrypt", self.endpoint, parent);
            let body = DecryptRequest {
                ciphertext: b64_encode(&wrapped.ciphertext),
                additional_authenticated_data: b64_encode(&canonical_aad),
            };

            let resp = self
                .http
                .post(&url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "provider_error");
                    BYOKError::Provider(format!("gcp kms decrypt POST: {e}"))
                })?;

            if !resp.status().is_success() {
                return Err(map_http_error(resp, &wrapped.key_id, "unwrap_dek").await);
            }

            let dr: DecryptResponse = resp.json().await.map_err(|e| {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "provider_error");
                BYOKError::Provider(format!("gcp kms decrypt JSON parse: {e}"))
            })?;
            let plaintext = b64_decode(&dr.plaintext)?;

            if plaintext.len() != 32 {
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "dek_length_invalid",
                );
                return Err(BYOKError::DekLengthInvalid { got: plaintext.len() });
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&plaintext);
            Ok(Dek { bytes })
        }

        async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            if key_id.provider != KmsProviderKind::GcpKms {
                emit_audit("check_access", &key_id.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "GcpKmsRealProvider check_access: wrong provider {:?}",
                    key_id.provider
                )));
            }
            Self::validate_key_resource(&key_id.key_arn_or_id).inspect_err(|_| {
                emit_audit("check_access", &key_id.key_arn_or_id, "malformed_arn");
            })?;

            if self.mock_mode {
                return Ok(KmsAccessStatus::Ok);
            }

            let parent = super::super::key_resource::strip_version_suffix(&key_id.key_arn_or_id);
            let token = self.bearer().await?;
            let url = format!("{}/v1/{}", self.endpoint, parent);

            let resp = self
                .http
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| {
                    emit_audit("check_access", &key_id.key_arn_or_id, "provider_error");
                    BYOKError::Provider(format!("gcp kms GetCryptoKey: {e}"))
                })?;

            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                let _api = parse_api_error(&body);
                emit_audit(
                    "check_access",
                    &key_id.key_arn_or_id,
                    match status {
                        403 => "access_denied",
                        404 => "not_found",
                        429 => "throttled",
                        _ => "provider_error",
                    },
                );
                return Ok(map_get_error_to_access(status));
            }

            let ck: CryptoKey = resp.json().await.map_err(|e| {
                emit_audit("check_access", &key_id.key_arn_or_id, "provider_error");
                BYOKError::Provider(format!("gcp kms CryptoKey JSON: {e}"))
            })?;
            let state = ck.primary.as_ref().map(|v| v.state.as_str()).unwrap_or("");
            Ok(map_state_to_access(state))
        }
    }

    // ── Mock-mode wrap / unwrap (in-process, AAD-binding enforced) ──────────

    fn mock_wrap(
        dek: &Dek,
        key_id: &KmsKeyId,
        ctx: &Value,
        canonical_aad: &[u8],
    ) -> WrappedDek {
        let fp = aad_fingerprint(canonical_aad);
        let mut ct = Vec::with_capacity(8 + 32);
        ct.extend_from_slice(&fp);
        for b in &dek.bytes {
            ct.push(b ^ 0xCC);
        }
        WrappedDek {
            provider: KmsProviderKind::GcpKms,
            key_id: key_id.clone(),
            ciphertext: ct,
            encryption_context: Some(ctx.clone()),
        }
    }

    fn mock_unwrap(wrapped: &WrappedDek, canonical_aad: &[u8]) -> Result<Dek, BYOKError> {
        if wrapped.ciphertext.len() != 40 {
            return Err(BYOKError::EnvelopeError(format!(
                "GCP real (mock): wrong ciphertext length {} (expected 40)",
                wrapped.ciphertext.len()
            )));
        }
        let expected = aad_fingerprint(canonical_aad);
        let stored = wrapped.ciphertext.get(..8).ok_or_else(|| {
            BYOKError::EnvelopeError("GCP real (mock): ciphertext too short".to_string())
        })?;
        // Constant-time fingerprint compare.
        if stored.ct_eq(&expected).unwrap_u8() == 0 {
            return Err(BYOKError::AadMismatch);
        }
        let body = wrapped.ciphertext.get(8..).ok_or_else(|| {
            BYOKError::EnvelopeError("GCP real (mock): ciphertext too short".to_string())
        })?;
        let mut bytes = [0u8; 32];
        for (out, &b) in bytes.iter_mut().zip(body.iter()) {
            *out = b ^ 0xCC;
        }
        Ok(Dek { bytes })
    }

    #[cfg(test)]
    #[allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    mod tests {
        use super::*;

        #[test]
        fn state_mapping_smoke() {
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
        fn http_to_access_smoke() {
            assert_eq!(map_get_error_to_access(403), KmsAccessStatus::Revoked);
            assert_eq!(map_get_error_to_access(404), KmsAccessStatus::NotFound);
            assert_eq!(map_get_error_to_access(429), KmsAccessStatus::Throttled);
            assert!(matches!(
                map_get_error_to_access(500),
                KmsAccessStatus::ApiError(500)
            ));
        }

        #[test]
        fn hostname_of_handles_scheme_and_path() {
            assert_eq!(hostname_of("https://cloudkms.googleapis.com"), "cloudkms.googleapis.com");
            assert_eq!(
                hostname_of("https://cloudkms.us-east1.rep.googleapis.com/"),
                "cloudkms.us-east1.rep.googleapis.com"
            );
            assert_eq!(hostname_of("http://127.0.0.1:8080"), "127.0.0.1");
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "production-gcp"))]
pub use native::GcpKmsRealProvider;

// =============================================================================
// wasm32 stub.
// =============================================================================

/// wasm32 stub for GCP KMS real provider.
///
/// `reqwest` (default features) and the ADC stack do not compile to
/// `wasm32-unknown-unknown`. Any method call on this stub returns
/// `BYOKError::Provider("GCP KMS real provider unsupported on wasm32; ...")`.
/// In production CF Worker deployments envelope operations are forwarded to
/// the native server process via the internal control-plane RPC — see
/// `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GcpKmsWasmStub {
    region: String,
    resolved_fips_endpoint: String,
}

#[cfg(target_arch = "wasm32")]
impl GcpKmsWasmStub {
    /// Construct the wasm32 stub for `region`. Does not contact GCP.
    #[must_use]
    pub fn new(region: &str) -> Self {
        let host = resolve_endpoint_hostname(region, true);
        Self {
            region: region.to_string(),
            resolved_fips_endpoint: host,
        }
    }

    /// Same shape as the native [`GcpKmsRealProvider::resolved_fips_endpoint`].
    #[must_use]
    pub fn resolved_fips_endpoint(&self) -> &str {
        &self.resolved_fips_endpoint
    }
}

#[cfg(target_arch = "wasm32")]
const WASM_UNSUPPORTED_MSG: &str =
    "GCP KMS real provider unsupported on wasm32; use CF Worker-side GCP SDK binding instead";

#[cfg(target_arch = "wasm32")]
#[async_trait]
impl KmsProvider for GcpKmsWasmStub {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::GcpKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_2_L1
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

    async fn check_access(
        &self,
        _key_id: &KmsKeyId,
    ) -> Result<KmsAccessStatus, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
/// Type alias so downstream code can name `GcpKmsRealProvider` on both
/// targets — on wasm32 it resolves to the stub. CF Workers therefore link
/// a working type but every call surfaces the explicit error above.
pub type GcpKmsRealProvider = GcpKmsWasmStub;

// =============================================================================
// Tests — arch-agnostic AAD canonicalization + endpoint shape.
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

    #[test]
    fn resolve_endpoint_hostname_default_is_global() {
        assert_eq!(
            resolve_endpoint_hostname("us-east1", false),
            "cloudkms.googleapis.com"
        );
    }

    #[test]
    fn resolve_endpoint_hostname_fips_regional_shape() {
        assert_eq!(
            resolve_endpoint_hostname("us-east1", true),
            "cloudkms.us-east1.rep.googleapis.com"
        );
        assert_eq!(
            resolve_endpoint_hostname("europe-west1", true),
            "cloudkms.europe-west1.rep.googleapis.com"
        );
    }
}
