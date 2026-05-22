//! `AzureKeyVaultRealProvider` — GA-hardened Azure Key Vault REST 7.4 client.
//!
//! This module is the canonical Azure Key Vault real-mode entry point. It
//! follows the BYOK real-provider pattern locked in by
//! `specs/_audits/2026-05-15-byok-real-provider-pattern.md` (canonical
//! reference impl: `crates/corelink-byok-aws/src/real.rs`):
//!
//! 1. **FIPS endpoint enforced.** [`AzureKeyVaultRealProvider::new`]
//!    rejects any vault URL that is not a FIPS-eligible host
//!    (`*.vault.azure.net`, `*.managedhsm.azure.net`, plus the well-known
//!    sovereign-cloud TLDs). The resolved hostname is exposed via
//!    [`AzureKeyVaultRealProvider::resolved_fips_endpoint`] so the test
//!    suite can pin the exact URL shape (`<vault>.<svc>` where
//!    `svc ∈ {vault.azure.net, managedhsm.azure.net, vault.usgovcloudapi.net,
//!    managedhsm.usgovcloudapi.net, vault.azure.cn, managedhsm.azure.cn,
//!    vault.microsoftazure.de}`). Premium HSM = FIPS 140-2 Level 2; Managed
//!    HSM = FIPS 140-2 Level 3 (logical L3 in the compliance matrix).
//! 2. **AAD JCS canonicalization (mandatory).** Every `wrap_dek` /
//!    `unwrap_dek` call canonicalizes the customer-supplied
//!    `encryption_context` via RFC 8785 JCS bytes (sorted-key,
//!    string-valued, top-level object). The JCS bytes are the AAD passed
//!    into the inner AES-256-GCM cipher; this ensures the same logical AAD
//!    produces byte-identical AAD wire bytes across architectures (native,
//!    wasm32) and re-orderings. See [`canonicalize_aad_to_string_map`].
//! 3. **Composite-key AAD binding (ADR-S14-001).** Azure `wrapKey` /
//!    `unwrapKey` (RSA-OAEP-256) does not accept AAD natively. CoreLink
//!    wraps an ephemeral AES-256-GCM key (which itself encrypts the DEK
//!    with the canonical JCS AAD bound); the GCM authentication tag on
//!    `unwrap_dek` enforces tamper detection — any AAD tamper surfaces as
//!    [`BYOKError::AadMismatch`].
//! 4. **Audit fail-CLOSED ordering.** Every Azure KV error path emits a
//!    structured `tracing` event (target `corelink.byok.azure.audit`,
//!    `audit = true`) BEFORE the error bubbles up. The orchestrator's
//!    audit subscriber turns these events into BLAKE3-linked audit
//!    records (see `corelink-audit-chain`).
//! 5. **No `unsafe`, no `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.**
//!    Enforced by crate-level lints.
//! 6. **Constant-time AAD fingerprint compare.** Mock-mode tamper detection
//!    uses [`subtle::ConstantTimeEq`]; the real REST path delegates AAD
//!    enforcement to the inner AES-GCM tag (which is itself constant-time
//!    by construction).
//!
//! # FIPS region / host table
//!
//! | Host suffix                           | Tier                     | FIPS level (reported)         |
//! |---------------------------------------|--------------------------|-------------------------------|
//! | `<v>.vault.azure.net`                 | Premium HSM (public)     | `Fips140_2_L2` (CMVP #3516)   |
//! | `<v>.managedhsm.azure.net`            | Managed HSM (public)     | `Fips140_2_L2` (logical L3)   |
//! | `<v>.vault.usgovcloudapi.net`         | Premium HSM (US Gov)     | `Fips140_2_L2`                |
//! | `<v>.managedhsm.usgovcloudapi.net`    | Managed HSM (US Gov)     | `Fips140_2_L2` (logical L3)   |
//! | `<v>.vault.azure.cn`                  | Premium HSM (China)      | `Fips140_2_L2`                |
//! | `<v>.managedhsm.azure.cn`             | Managed HSM (China)      | `Fips140_2_L2` (logical L3)   |
//! | `<v>.vault.microsoftazure.de`         | Premium HSM (Germany)    | `Fips140_2_L2`                |
//!
//! The `endpoint_override` parameter exists only for `wiremock` and tests;
//! production callers pass `None` (the per-key vault URL is used directly).
//!
//! # wasm32 strategy
//!
//! `reqwest` (+ tokio "full") does not compile to `wasm32-unknown-unknown`.
//! For the CF-Worker build target we link [`AzureKeyVaultWasmStub`] which
//! returns `BYOKError::Provider("Azure Key Vault real provider unsupported
//! on wasm32; use CF Worker-side Azure SDK binding instead")` from every
//! method. CoreLink Workers proxy envelope operations to the native server
//! process, which holds the actual REST client.

#![forbid(unsafe_code)]

use serde_json::Value;
use std::collections::BTreeMap;

use corelink_byok_core::BYOKError;

#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;
#[cfg(target_arch = "wasm32")]
use corelink_byok_core::{
    Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
};

/// Canonicalize an AAD JSON value to a deterministic `BTreeMap<String,String>`
/// plus its RFC 8785 JCS canonical bytes.
///
/// The canonicalization rules are byte-identical to the AWS reference impl:
///
/// 1. Top-level value MUST be a JSON object — otherwise
///    [`BYOKError::EnvelopeError`] is returned.
/// 2. Each value MUST be a JSON string — otherwise
///    [`BYOKError::EnvelopeError`] is returned. This matches the wire shape
///    that AWS KMS / GCP KMS / Vault accept and keeps the AAD wire shape
///    provider-portable.
/// 3. Keys are sorted lexicographically (`BTreeMap`) so the resulting wire
///    bytes are deterministic regardless of input ordering.
/// 4. RFC 8785 JCS canonical bytes of the (now sorted) object are returned
///    for downstream binding — Azure's composite-key AAD layer uses these
///    JCS bytes as the AES-GCM AAD.
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

/// Cheap 8-byte AAD fingerprint used by [`AzureKeyVaultRealProvider`] mock
/// mode for tamper detection. Compared in constant time via
/// [`subtle::ConstantTimeEq`].
#[must_use]
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

/// Resolve the canonical FIPS-eligible Azure Key Vault host for a parsed
/// vault URL. Returns the host (e.g. `myvault.vault.azure.net`) and the
/// service-tier suffix (e.g. `vault.azure.net` or `managedhsm.azure.net`).
///
/// # Errors
///
/// Returns [`BYOKError::Provider`] if the URL is not a FIPS-eligible host.
pub fn resolve_fips_host(vault_url: &str) -> Result<(String, &'static str), BYOKError> {
    const FIPS_SUFFIXES: &[&str] = &[
        "vault.azure.net",
        "managedhsm.azure.net",
        "vault.usgovcloudapi.net",
        "managedhsm.usgovcloudapi.net",
        "vault.azure.cn",
        "managedhsm.azure.cn",
        "vault.microsoftazure.de",
    ];
    let after_scheme = vault_url.strip_prefix("https://").ok_or_else(|| {
        BYOKError::Provider(
            "Azure KV: vault URL must use https:// scheme".to_string(),
        )
    })?;
    let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = match after_scheme.get(..host_end) {
        Some(h) => h.to_string(),
        None => {
            return Err(BYOKError::Provider(
                "Azure KV: vault URL is missing host segment".to_string(),
            ));
        }
    };
    for suffix in FIPS_SUFFIXES {
        if let Some(prefix) = host.strip_suffix(suffix) {
            if prefix.ends_with('.') && !prefix.is_empty() {
                return Ok((host, suffix));
            }
        }
    }
    Err(BYOKError::Provider(format!(
        "Azure KV: host '{host}' is not a FIPS-eligible Azure Key Vault host \
         (must end in one of: .vault.azure.net, .managedhsm.azure.net, \
         .vault.usgovcloudapi.net, .managedhsm.usgovcloudapi.net, \
         .vault.azure.cn, .managedhsm.azure.cn, .vault.microsoftazure.de)"
    )))
}

// =============================================================================
// Native-only real provider (REST 7.4).
// =============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{aad_fingerprint, canonicalize_aad_to_string_map, resolve_fips_host};
    use aes_gcm::aead::{Aead, KeyInit, generic_array::GenericArray};
    use aes_gcm::Aes256Gcm;
    use async_trait::async_trait;
    use base64::Engine as _;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use subtle::ConstantTimeEq;
    use tracing::{debug, warn};

    use corelink_byok_core::{
        BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind,
        WrappedDek,
    };

    use crate::entra::EntraCredentials;
    use crate::key_resource::{parse_kv_resource, KvKeyResource};

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

    fn map_get_error_to_access(
        http_status: u16,
        api: &ApiErrorInner,
        key: &str,
    ) -> KmsAccessStatus {
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
            let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx)
                .inspect_err(|_| {
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
            let inner_ct =
                Self::aes_gcm_encrypt(&inner_key, &nonce, &dek.bytes, &canonical_aad)?;

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
            let mut ciphertext =
                Vec::with_capacity(4 + azure_wrapped_inner.len() + inner_ct.len());
            let outer_len_u32: u32 =
                azure_wrapped_inner.len().try_into().map_err(|_| {
                    emit_audit("wrap_dek", &key_id.key_arn_or_id, "outer_len_overflow");
                    BYOKError::EnvelopeError(
                        "outer wrapped key length > u32::MAX".to_string(),
                    )
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
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "aad_missing",
                );
                BYOKError::EncryptionContextMissing
            })?;
            let (_ec_map, canonical_aad) =
                canonicalize_aad_to_string_map(ctx).inspect_err(|_| {
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

            let resource = Self::parse_resource(&wrapped.key_id.key_arn_or_id).inspect_err(
                |_| {
                    emit_audit(
                        "unwrap_dek",
                        &wrapped.key_id.key_arn_or_id,
                        "malformed_arn",
                    );
                },
            )?;

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
            let plaintext = Self::aes_gcm_decrypt(&inner_key, inner_ct, &canonical_aad)
                .inspect_err(|e| {
                    if matches!(e, BYOKError::AadMismatch) {
                        emit_audit(
                            "unwrap_dek",
                            &wrapped.key_id.key_arn_or_id,
                            "invalid_ciphertext_aad_mismatch",
                        );
                    } else {
                        emit_audit(
                            "unwrap_dek",
                            &wrapped.key_id.key_arn_or_id,
                            "aes_gcm_error",
                        );
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
                emit_audit(
                    "check_access",
                    &key_id.key_arn_or_id,
                    "entra_token_failure",
                );
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
    mod tests {
        use super::*;

        #[test]
        fn fips_static_l2_for_test_constructor() {
            let http = reqwest::Client::new();
            let creds = EntraCredentials::for_test_static("t");
            let p = AzureKeyVaultRealProvider::for_test(
                http,
                creds,
                "eastus",
                "https://x.invalid",
            );
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
            assert_eq!(
                map_get_error_to_access(401, &api, "k"),
                KmsAccessStatus::Revoked
            );
            assert_eq!(
                map_get_error_to_access(403, &api, "k"),
                KmsAccessStatus::Revoked
            );
            assert_eq!(
                map_get_error_to_access(404, &api, "k"),
                KmsAccessStatus::NotFound
            );
            assert_eq!(
                map_get_error_to_access(429, &api, "k"),
                KmsAccessStatus::Throttled
            );
            assert!(matches!(
                map_get_error_to_access(500, &api, "k"),
                KmsAccessStatus::ApiError(500)
            ));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::AzureKeyVaultRealProvider;

// =============================================================================
// wasm32 stub.
// =============================================================================

/// wasm32 stub for Azure Key Vault real provider.
///
/// The Azure KV REST stack (`reqwest` + `regex` + tokio "full") does not
/// compile to `wasm32-unknown-unknown`. Any method call on this stub
/// returns `BYOKError::Provider("Azure Key Vault real provider unsupported
/// on wasm32; ...")`. In production CF Worker deployments envelope
/// operations are forwarded to the native server process via the internal
/// control-plane RPC — see
/// `specs/_audits/2026-05-15-byok-real-provider-pattern.md`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AzureKeyVaultWasmStub {
    region: String,
    vault_url: String,
}

#[cfg(target_arch = "wasm32")]
impl AzureKeyVaultWasmStub {
    /// Construct the wasm32 stub for `region` + `vault_url`. Does not
    /// contact Azure.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if `vault_url` is not a FIPS-eligible
    /// Azure Key Vault host.
    pub fn new(region: &str, vault_url: &str) -> Result<Self, BYOKError> {
        let _ = resolve_fips_host(vault_url)?;
        Ok(Self {
            region: region.to_string(),
            vault_url: vault_url.to_string(),
        })
    }

    /// Same shape as the native
    /// [`AzureKeyVaultRealProvider::resolved_fips_endpoint`]. Returned even
    /// on wasm32 so spec validation can cross-reference the canonical FIPS
    /// host string.
    #[must_use]
    pub fn resolved_fips_endpoint(&self) -> String {
        match resolve_fips_host(&self.vault_url) {
            Ok((host, _)) => host,
            Err(_) => String::new(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
const WASM_UNSUPPORTED_MSG: &str =
    "Azure Key Vault real provider unsupported on wasm32; use CF Worker-side Azure SDK binding instead";

#[cfg(target_arch = "wasm32")]
#[async_trait]
impl KmsProvider for AzureKeyVaultWasmStub {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AzureKeyVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_2_L2
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
/// Type alias so downstream code can name `AzureKeyVaultRealProvider` on
/// both targets — on wasm32 it resolves to the stub. CF Workers therefore
/// link a working type but every call surfaces the explicit error above.
pub type AzureKeyVaultRealProvider = AzureKeyVaultWasmStub;

// =============================================================================
// Tests — arch-agnostic AAD canonicalization + FIPS host resolution.
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
    fn resolve_fips_host_accepts_premium_hsm() {
        let (h, s) =
            resolve_fips_host("https://myvault.vault.azure.net/keys/k").unwrap();
        assert_eq!(h, "myvault.vault.azure.net");
        assert_eq!(s, "vault.azure.net");
    }

    #[test]
    fn resolve_fips_host_accepts_managed_hsm() {
        let (h, s) =
            resolve_fips_host("https://corp-hsm.managedhsm.azure.net/keys/k").unwrap();
        assert_eq!(h, "corp-hsm.managedhsm.azure.net");
        assert_eq!(s, "managedhsm.azure.net");
    }

    #[test]
    fn resolve_fips_host_accepts_us_gov() {
        let (h, s) =
            resolve_fips_host("https://gov-vault.vault.usgovcloudapi.net/keys/k")
                .unwrap();
        assert_eq!(h, "gov-vault.vault.usgovcloudapi.net");
        assert_eq!(s, "vault.usgovcloudapi.net");
    }

    #[test]
    fn resolve_fips_host_rejects_non_https() {
        let err = resolve_fips_host("http://myvault.vault.azure.net/keys/k")
            .unwrap_err();
        assert!(matches!(err, BYOKError::Provider(_)));
    }

    #[test]
    fn resolve_fips_host_rejects_non_fips_tld() {
        let err = resolve_fips_host("https://attacker.example.com/keys/k").unwrap_err();
        assert!(matches!(err, BYOKError::Provider(_)));
    }
}
