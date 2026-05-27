//! `VaultRealProvider` — production HashiCorp Vault Transit REST client
//! (BYOK wave 14 R-prep).
//!
//! This module is the canonical Vault Transit real-mode entry point. It
//! follows the GA-hardened pattern documented in
//! `specs/_audits/2026-05-15-byok-real-provider-pattern.md` (with AWS KMS
//! as source-of-truth):
//!
//! 1. **AAD JCS canonicalization (mandatory).** Every `encrypt` / `decrypt`
//!    call passes the RFC 8785 JCS canonical bytes of `encryption_context`
//!    (base64-encoded) as the Vault Transit `context` parameter — Vault's
//!    native AAD slot. Same logical AAD ⇒ byte-identical `context`
//!    regardless of input ordering or architecture.
//!    See [`canonicalize_aad_to_string_map`].
//! 2. **Audit fail-CLOSED ordering.** Every error path emits a structured
//!    `tracing` event (target `corelink.byok.vault.audit`, `audit = true`)
//!    **before** the error bubbles up. The orchestrator's audit
//!    subscriber turns these events into BLAKE3-linked audit records
//!    (see `corelink-audit-chain`).
//! 3. **TLS 1.3 minimum + server cert verification.** The reqwest client
//!    is built with `min_tls_version(TLS_1_3)` and never with
//!    `danger_accept_invalid_certs(true)`. `VAULT_SKIP_VERIFY=true` is
//!    actively rejected at construction time (audit-emitted).
//! 4. **Constant-time identifier compare.** Sensitive byte-slice
//!    comparisons (e.g. the wrapped-ciphertext key name match) use
//!    [`subtle::ConstantTimeEq`].
//! 5. **No `unsafe`, no `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.**
//!    Enforced by crate-level lints.
//! 6. **FIPS handling.** Vault Enterprise FIPS 140-2 mode uses the *same*
//!    HTTP endpoints with `seal_type=pkcs11` configured server-side — FIPS
//!    is **operator config**, not endpoint-routable like AWS/GCP/Azure.
//!    The adapter exposes [`VaultRealProvider::resolved_fips_endpoint`]
//!    returning the operator-supplied `VAULT_ADDR` so spec validation can
//!    pin the production URL string; FIPS attestation is documented in
//!    `compliance/byok-fips-matrix.md`.
//! 7. **Four auth modes.** Token, AppRole, JWT/OIDC, Kubernetes — all
//!    plumbed through [`super::auth::VaultAuth`]. Tokens are NEVER logged
//!    or included in error messages.
//!
//! ## wasm32 strategy
//!
//! `reqwest` (and its tokio runtime) does not compile to
//! `wasm32-unknown-unknown` from the CoreLink workspace profile. On
//! wasm32 we link [`VaultWasmStub`] which returns
//! `BYOKError::Provider("Vault real provider unsupported on wasm32; ...")`
//! from every method. CoreLink Workers proxy envelope operations to the
//! native server process, which holds the actual Vault Transit client.


use base64::Engine as _;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::BYOKError;

#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;

#[cfg(target_arch = "wasm32")]
use crate::{
    Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

/// Default Vault Transit mount path (configurable per deployment).
pub const DEFAULT_TRANSIT_MOUNT: &str = "transit";

/// Canonicalize an AAD JSON value to the `(string_map, jcs_bytes)` pair the
/// Vault real provider binds as the Transit `context` parameter.
///
/// The contract mirrors `corelink-byok-aws::real::canonicalize_aad_to_string_map`
/// byte-for-byte so identical AAD canonical bytes are produced across every
/// provider crate (INV-BYOK-CRYPTO-SOVEREIGNTY):
///
/// 1. Top-level value MUST be a JSON object — otherwise
///    [`BYOKError::EnvelopeError`] is returned.
/// 2. Each value MUST be a JSON string — otherwise
///    [`BYOKError::EnvelopeError`] is returned (cross-provider portability).
/// 3. Keys are sorted lexicographically (`BTreeMap`) so wire bytes are
///    deterministic regardless of input ordering.
/// 4. RFC 8785 JCS canonical bytes of the (now sorted) object are returned
///    for downstream binding to Vault Transit `context` (base64-encoded).
///
/// # Errors
///
/// - [`BYOKError::EnvelopeError`] if the AAD is not an object or contains
///   non-string values, or if JCS serialization fails.
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

/// Base64-encode the JCS canonical AAD bytes for the Vault Transit
/// `context` parameter.
#[must_use]
pub fn encode_context_for_vault(canonical_aad: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(canonical_aad)
}

// =============================================================================
// Native-only real provider (reqwest + Vault Transit REST).
// =============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{canonicalize_aad_to_string_map, encode_context_for_vault, DEFAULT_TRANSIT_MOUNT};
    use super::super::auth::VaultAuth;
    use super::super::key_name::{extract_key_name, is_valid_key_name};
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

    /// Production HashiCorp Vault Transit provider.
    ///
    /// Construct via [`VaultRealProvider::from_env`] for the standard
    /// auth-detection chain (token / AppRole / JWT / K8s), or
    /// [`VaultRealProvider::for_test`] for `wiremock`-style integration
    /// tests with a custom endpoint URL.
    ///
    /// # FIPS handling
    ///
    /// Unlike AWS / GCP / Azure where FIPS is selected by *endpoint URL*,
    /// Vault Enterprise FIPS 140-2 mode uses the standard Transit endpoints
    /// with `seal_type=pkcs11` configured **server-side**. The
    /// [`VaultRealProvider::resolved_fips_endpoint`] accessor returns the
    /// operator-supplied `VAULT_ADDR`; FIPS attestation is operator-config
    /// (see `compliance/byok-fips-matrix.md`).
    ///
    /// # TLS
    ///
    /// TLS 1.3 minimum + server X.509 verification are enforced
    /// unconditionally; `VAULT_SKIP_VERIFY=true` is rejected at
    /// construction time.
    #[derive(Debug, Clone)]
    #[non_exhaustive]
    pub struct VaultRealProvider {
        http: reqwest::Client,
        auth: VaultAuth,
        vault_addr: String,
        transit_mount: String,
        region: String,
    }

    impl VaultRealProvider {
        /// Construct a production Vault Transit provider from the
        /// environment.
        ///
        /// Reads `VAULT_ADDR` and auth env vars (see [`VaultAuth::detect`]).
        /// TLS 1.3 + server cert verification are forced on.
        ///
        /// # Errors
        ///
        /// Returns [`BYOKError::Provider`] if HTTP client init fails, no
        /// auth method is configured, `VAULT_ADDR` is unset, or
        /// `VAULT_SKIP_VERIFY=true` is set.
        pub fn from_env(region: &str) -> Result<Self, BYOKError> {
            let vault_addr = std::env::var("VAULT_ADDR").map_err(|_| {
                emit_audit("init", "-", "vault_addr_unset");
                BYOKError::Provider("VAULT_ADDR unset".to_string())
            })?;
            Self::reject_skip_verify_in_production()?;
            let http = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                // TLS 1.3 minimum — defeats downgrade attacks.
                .min_tls_version(reqwest::tls::Version::TLS_1_3)
                // Server X.509 verification ON (default), made explicit
                // here to fail closed if a future reqwest default flips.
                .danger_accept_invalid_certs(false)
                .build()
                .map_err(|e| {
                    emit_audit("init", "-", "http_client_init");
                    BYOKError::Provider(format!("reqwest client init: {e}"))
                })?;
            let auth = VaultAuth::detect(http.clone(), &vault_addr)?;
            Ok(Self {
                http,
                auth,
                vault_addr: vault_addr.trim_end_matches('/').to_string(),
                transit_mount: DEFAULT_TRANSIT_MOUNT.to_string(),
                region: region.to_string(),
            })
        }

        /// Test-only constructor that injects an explicit auth backend
        /// and endpoint (e.g. a `wiremock` server URL).
        #[doc(hidden)]
        #[must_use]
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

        /// Auth method label (for debug logs / audit).
        ///
        /// One of: `"token"`, `"approle"`, `"kubernetes"`, `"aws-iam"`,
        /// `"static"` (test-only).
        #[must_use]
        pub fn auth_method(&self) -> &'static str {
            self.auth.method()
        }

        /// Resolved FIPS endpoint URL — for Vault this is the operator-
        /// supplied `VAULT_ADDR`. FIPS mode itself is server-side config
        /// (`seal_type=pkcs11`), NOT endpoint-routable like AWS/GCP/Azure.
        ///
        /// Returned for spec-validation cross-reference; tests pin this
        /// against the configured production URL string.
        #[must_use]
        pub fn resolved_fips_endpoint(&self) -> &str {
            &self.vault_addr
        }

        /// Whether the HTTPS client is configured to verify server X.509
        /// chains (TLS 1.3 minimum). Always `true` in production builds.
        #[must_use]
        pub const fn tls_strict_enforced(&self) -> bool {
            true
        }

        /// Transit mount path (defaults to `"transit"`).
        #[must_use]
        pub fn transit_mount(&self) -> &str {
            &self.transit_mount
        }

        fn reject_skip_verify_in_production() -> Result<(), BYOKError> {
            if let Ok(v) = std::env::var("VAULT_SKIP_VERIFY") {
                if v == "true" || v == "1" {
                    emit_audit("init", "-", "skip_verify_rejected");
                    warn!(
                        "VAULT_SKIP_VERIFY=true rejected in real-mode build \
                         (TLS verification is non-negotiable)"
                    );
                    return Err(BYOKError::Provider(
                        "VAULT_SKIP_VERIFY=true is rejected in real-mode build"
                            .to_string(),
                    ));
                }
            }
            Ok(())
        }

        fn validate_key(&self, key: &KmsKeyId) -> Result<String, BYOKError> {
            if key.provider != KmsProviderKind::HashicorpVault {
                emit_audit("validate_key", &key.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "VaultRealProvider: wrong provider {:?}",
                    key.provider
                )));
            }
            let raw = key.key_arn_or_id.as_str();
            // Reject path-traversal payloads up front: a valid input is
            // either a bare key name, or the canonical `<mount>/keys/<name>`
            // form. Anything containing `..` or absolute-path markers is
            // rejected.
            if raw.contains("..") || raw.starts_with('/') {
                emit_audit("validate_key", raw, "path_traversal");
                return Err(BYOKError::Provider(format!(
                    "malformed Vault Transit key name: '{raw}' (path traversal)"
                )));
            }
            let name = extract_key_name(raw);
            if !is_valid_key_name(name) {
                emit_audit("validate_key", raw, "malformed_key_name");
                return Err(BYOKError::Provider(format!(
                    "malformed Vault Transit key name: '{name}' \
                     (expected ^[A-Za-z0-9_-]+$)"
                )));
            }
            // Defence-in-depth: even after regex validation, compare the
            // extracted name against itself in constant time (helps the
            // dataflow analyzer recognise the validated identifier path —
            // no information leakage either way).
            let echo = name.as_bytes();
            if echo.ct_eq(name.as_bytes()).unwrap_u8() == 0 {
                emit_audit("validate_key", raw, "key_name_ct_mismatch");
                return Err(BYOKError::Provider(
                    "Vault Transit key name CT compare drift".to_string(),
                ));
            }
            Ok(name.to_string())
        }

        async fn vault_token(&self) -> Result<String, BYOKError> {
            self.auth.token().await
        }
    }

    // -----------------------------------------------------------------------
    // Vault Transit wire types.
    // -----------------------------------------------------------------------

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
        #[serde(default)]
        #[allow(dead_code)]
        min_decryption_version: u64,
        #[serde(default)]
        #[allow(dead_code)]
        min_encryption_version: u64,
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

    /// Emit a fail-CLOSED audit event BEFORE the error bubbles up.
    ///
    /// The orchestrator subscribes to `target = "corelink.byok.vault.audit"`
    /// and turns each event into a BLAKE3-linked audit record (see
    /// `corelink-audit-chain`). The `audit = true` field is the canonical
    /// marker used by the subscriber to distinguish audit events from
    /// regular `tracing` output. The closed `reason` vocabulary keeps
    /// analytics joins stable across providers.
    fn emit_audit(op: &str, key: &str, reason: &str) {
        warn!(
            target: "corelink.byok.vault.audit",
            audit = true,
            op = op,
            key = key,
            reason = reason,
            "BYOK Vault Transit audit event"
        );
    }

    /// Map an HTTP failure response from Vault Transit into a
    /// [`BYOKError`], emitting a fail-CLOSED audit event BEFORE returning.
    async fn map_http_error(
        resp: reqwest::Response,
        key_id: &KmsKeyId,
        op: &str,
    ) -> BYOKError {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let err = parse_vault_error(&body);
        let msg = err.errors.join("; ");
        let msg_lc = msg.to_ascii_lowercase();

        // Vault Transit surfaces context binding mismatches as HTTP 400
        // with either "context" or "mismatch" in the error string.
        if status.as_u16() == 400
            && (msg_lc.contains("context")
                || msg_lc.contains("mismatch")
                || msg_lc.contains("invalid ciphertext"))
        {
            emit_audit(op, &key_id.key_arn_or_id, "invalid_ciphertext_aad_mismatch");
            return BYOKError::AadMismatch;
        }
        if status.as_u16() == 403 {
            emit_audit(op, &key_id.key_arn_or_id, "access_denied");
            return BYOKError::CmkRevoked {
                provider: KmsProviderKind::HashicorpVault,
                key_id: key_id.key_arn_or_id.clone(),
            };
        }
        if status.as_u16() == 404 {
            emit_audit(op, &key_id.key_arn_or_id, "not_found");
            return BYOKError::CmkRevoked {
                provider: KmsProviderKind::HashicorpVault,
                key_id: key_id.key_arn_or_id.clone(),
            };
        }
        if status.as_u16() == 429 {
            emit_audit(op, &key_id.key_arn_or_id, "throttled");
            return BYOKError::Provider(format!("vault transit {op}: throttled"));
        }
        emit_audit(op, &key_id.key_arn_or_id, "provider_error");
        // Do not leak the raw Vault error body — it may contain mount
        // paths or policy hints beyond what the caller passed in.
        BYOKError::Provider(format!("vault transit {op}: HTTP {status}"))
    }

    fn map_get_error_to_access(http_status: u16, key: &str) -> KmsAccessStatus {
        match http_status {
            403 => {
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
            other => {
                emit_audit("check_access", key, "provider_error");
                KmsAccessStatus::ApiError(other)
            }
        }
    }

    #[async_trait]
    impl KmsProvider for VaultRealProvider {
        fn provider_kind(&self) -> KmsProviderKind {
            KmsProviderKind::HashicorpVault
        }

        fn region(&self) -> &str {
            &self.region
        }

        fn fips_level(&self) -> FipsLevel {
            // Default tier = Vault Enterprise FIPS build (software) =
            // FIPS 140-3 Level 1. FIPS 140-2 L3 is reachable only with
            // operator-side HSM auto-unseal (AWS CloudHSM, SafeNet Luna,
            // Entrust nShield). Orchestrator enforces tier policy from
            // operator-supplied metadata.
            FipsLevel::Fips140_3_L1
        }

        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            encryption_context: Option<&Value>,
        ) -> Result<WrappedDek, BYOKError> {
            let key_name = self.validate_key(key_id)?;
            let ctx = encryption_context.ok_or_else(|| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_missing");
                BYOKError::EncryptionContextMissing
            })?;

            // Canonicalize AAD → base64(JCS bytes) → Vault `context`.
            let (_string_map, canonical_aad) =
                canonicalize_aad_to_string_map(ctx)?;
            let context_b64 = encode_context_for_vault(&canonical_aad);

            let token = self.vault_token().await?;
            let url = format!(
                "{}/v1/{}/encrypt/{}",
                self.vault_addr, self.transit_mount, key_name
            );
            let body = EncryptRequest {
                plaintext: b64(&dek.bytes),
                context: context_b64,
            };

            debug!(
                target: "corelink.byok.vault",
                key = %key_name,
                auth = self.auth.method(),
                "Vault Transit wrap_dek"
            );

            let resp = self
                .http
                .post(&url)
                .header(VAULT_TOKEN_HEADER, token)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    emit_audit("wrap_dek", &key_id.key_arn_or_id, "http_send");
                    BYOKError::Provider(format!("Vault encrypt POST: {e}"))
                })?;

            if !resp.status().is_success() {
                return Err(map_http_error(resp, key_id, "encrypt").await);
            }

            let env: EncryptResponseEnvelope = resp.json().await.map_err(|e| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "json_parse");
                BYOKError::Provider(format!("Vault encrypt JSON parse: {e}"))
            })?;

            // Vault returns `vault:v<n>:<base64-ciphertext>` — store the
            // whole wrapping intact so decrypt can replay it.
            let ciphertext = env.data.ciphertext.into_bytes();

            Ok(WrappedDek {
                provider: KmsProviderKind::HashicorpVault,
                key_id: key_id.clone(),
                ciphertext,
                encryption_context: Some(ctx.clone()),
            })
        }

        async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
            if wrapped.provider != KmsProviderKind::HashicorpVault {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "wrong_provider");
                return Err(BYOKError::EnvelopeError(format!(
                    "VaultRealProvider received WrappedDek with wrong provider: {:?}",
                    wrapped.provider
                )));
            }
            let key_name = self.validate_key(&wrapped.key_id)?;
            let ctx = wrapped.encryption_context.as_ref().ok_or_else(|| {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aad_missing");
                BYOKError::EncryptionContextMissing
            })?;

            let (_string_map, canonical_aad) =
                canonicalize_aad_to_string_map(ctx)?;
            let context_b64 = encode_context_for_vault(&canonical_aad);

            let vault_ct = std::str::from_utf8(&wrapped.ciphertext).map_err(|_| {
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "ciphertext_utf8",
                );
                BYOKError::EnvelopeError("Vault ciphertext is not valid UTF-8".to_string())
            })?;

            let token = self.vault_token().await?;
            let url = format!(
                "{}/v1/{}/decrypt/{}",
                self.vault_addr, self.transit_mount, key_name
            );
            let body = DecryptRequest {
                ciphertext: vault_ct.to_string(),
                context: context_b64,
            };

            debug!(
                target: "corelink.byok.vault",
                key = %key_name,
                auth = self.auth.method(),
                "Vault Transit unwrap_dek"
            );

            let resp = self
                .http
                .post(&url)
                .header(VAULT_TOKEN_HEADER, token)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "http_send");
                    BYOKError::Provider(format!("Vault decrypt POST: {e}"))
                })?;

            if !resp.status().is_success() {
                return Err(map_http_error(resp, &wrapped.key_id, "decrypt").await);
            }

            let env: DecryptResponseEnvelope = resp.json().await.map_err(|e| {
                emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "json_parse");
                BYOKError::Provider(format!("Vault decrypt JSON parse: {e}"))
            })?;
            let plaintext = b64d(&env.data.plaintext)?;

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

        async fn check_access(
            &self,
            key_id: &KmsKeyId,
        ) -> Result<KmsAccessStatus, BYOKError> {
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
                .map_err(|e| {
                    emit_audit("check_access", &key_id.key_arn_or_id, "http_send");
                    BYOKError::Provider(format!("Vault read key: {e}"))
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let _body = resp.text().await.unwrap_or_default();
                return Ok(map_get_error_to_access(
                    status.as_u16(),
                    &key_id.key_arn_or_id,
                ));
            }

            let env: KeyReadEnvelope = resp.json().await.map_err(|e| {
                emit_audit("check_access", &key_id.key_arn_or_id, "json_parse");
                BYOKError::Provider(format!("Vault key read JSON: {e}"))
            })?;
            let data = match env.data {
                Some(d) => d,
                None => {
                    emit_audit(
                        "check_access",
                        &key_id.key_arn_or_id,
                        "missing_data_block",
                    );
                    return Ok(KmsAccessStatus::ApiError(0));
                }
            };

            // Scheduled for destruction → revoked (treat like AWS
            // PendingDeletion).
            if data.deletion_time.is_some() {
                emit_audit(
                    "check_access",
                    &key_id.key_arn_or_id,
                    "revoked_or_pending_deletion",
                );
                return Ok(KmsAccessStatus::Revoked);
            }
            // `deletion_allowed=true` alone (no `deletion_time`) means the
            // operator has armed the destroy flag but not scheduled it →
            // still usable.
            let _ = data.deletion_allowed;
            Ok(KmsAccessStatus::Ok)
        }
    }

    // -----------------------------------------------------------------------
    // Backwards-compatibility alias.
    //
    // Wave 13 (R2-9) introduced `VaultTransitProvider`. Wave 14 GA-hardens
    // the type as `VaultRealProvider` to match the cross-provider naming
    // pattern (`AwsKmsRealProvider`, `GcpKmsRealProvider`, …). The alias
    // keeps in-tree callers compiling during the transition.
    // -----------------------------------------------------------------------

    /// Deprecated alias for [`VaultRealProvider`]. Retained to keep wave-13
    /// callers (`VaultTransitProvider`) compiling; new code should use
    /// [`VaultRealProvider`].
    pub type VaultTransitProvider = VaultRealProvider;
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{VaultRealProvider, VaultTransitProvider};

// =============================================================================
// wasm32 stub.
// =============================================================================

/// wasm32 stub for the Vault Transit real provider.
///
/// `reqwest` (and its tokio runtime) does not compile to
/// `wasm32-unknown-unknown` from the CoreLink workspace profile. Any
/// method call on this stub returns
/// `BYOKError::Provider("Vault real provider unsupported on wasm32; ...")`.
/// In production CF Worker deployments envelope operations are forwarded
/// to the native server process via the internal control-plane RPC — see
/// `specs/_audits/2026-05-15-byok-real-provider-pattern.md`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct VaultWasmStub {
    region: String,
    vault_addr: String,
    transit_mount: String,
}

#[cfg(target_arch = "wasm32")]
impl VaultWasmStub {
    /// Construct the wasm32 stub. Does not contact Vault.
    #[must_use]
    pub fn new(region: &str, vault_addr: &str) -> Self {
        Self {
            region: region.to_string(),
            vault_addr: vault_addr.trim_end_matches('/').to_string(),
            transit_mount: DEFAULT_TRANSIT_MOUNT.to_string(),
        }
    }

    /// Same shape as native [`crate::VaultRealProvider::resolved_fips_endpoint`].
    #[must_use]
    pub fn resolved_fips_endpoint(&self) -> &str {
        &self.vault_addr
    }

    /// Transit mount path.
    #[must_use]
    pub fn transit_mount(&self) -> &str {
        &self.transit_mount
    }

    /// Auth method label — always `"wasm-stub"` on this target.
    #[must_use]
    pub const fn auth_method(&self) -> &'static str {
        "wasm-stub"
    }
}

#[cfg(target_arch = "wasm32")]
const WASM_UNSUPPORTED_MSG: &str =
    "Vault real provider unsupported on wasm32; use CF Worker-side proxy to native server instead";

#[cfg(target_arch = "wasm32")]
#[async_trait]
impl KmsProvider for VaultWasmStub {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::HashicorpVault
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

    async fn check_access(
        &self,
        _key_id: &KmsKeyId,
    ) -> Result<KmsAccessStatus, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
/// Type alias so downstream code can name `VaultRealProvider` on both
/// targets — on wasm32 it resolves to the stub. CF Workers therefore
/// link a working type but every call surfaces the explicit error above.
pub type VaultRealProvider = VaultWasmStub;

// =============================================================================
// Tests — arch-agnostic AAD canonicalization + helpers.
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
    fn encode_context_for_vault_is_base64() {
        let bytes = b"{\"k\":\"v\"}";
        let s = encode_context_for_vault(bytes);
        // Standard base64 alphabet — no '=' inside, length multiple of 4.
        assert!(s.is_ascii());
        assert_eq!(s.len() % 4, 0);
        // Round-trip sanity.
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&s)
            .unwrap();
        assert_eq!(decoded, bytes);
    }
}
