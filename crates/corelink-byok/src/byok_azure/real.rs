//! `AzureKeyVaultRealProvider` — GA-hardened Azure Key Vault REST 7.4 client.
//!
//! This module is the canonical Azure Key Vault real-mode entry point. It
//! follows the BYOK real-provider pattern locked in by
//! `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md` (canonical
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

use serde_json::Value;
use std::collections::BTreeMap;

use crate::BYOKError;

#[cfg(target_arch = "wasm32")]
use crate::{Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek};
#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;

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
        BYOKError::Provider("Azure KV: vault URL must use https:// scheme".to_string())
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
#[path = "native.rs"]
mod native;

#[cfg(not(target_arch = "wasm32"))]
pub use native::AzureKeyVaultRealProvider;

// =============================================================================
// wasm32 stub (kept in a target-specific child module).
// =============================================================================

#[path = "wasm.rs"]
#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::AzureKeyVaultRealProvider;
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
#[path = "tests.rs"]
mod tests;
