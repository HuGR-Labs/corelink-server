//! HMAC-SHA256 per-consent signing via tenant-scoped HKDF.
//!
//! # Design (DD-001)
//!
//! HMAC-SHA256 with tenant-scoped key derived from a master key using
//! HKDF-SHA256 (RFC 5869). HKDF `info` = `corelink/v1/consent-hmac`
//! (security_model.md §7.2). Key is **separate** from the JWT receipt
//! key (`corelink/v1/dsr-receipt`) per key-separation principle.
//!
//! The signature preimage is the canonical JSON serialisation of:
//! `{consent_id, tenant_id, purpose, notice_text_hash, notice_version,
//!  locale, wording_id, ui_capture_ts, submission_ts}`.
//!
//! Tenant-scoping prevents cross-tenant signature forgery (AC-005 /
//! INV-CONSENT-PROOF-VERIFIABLE): HKDF `salt` = `tenant_id.as_bytes()`.
//! Signed with the per-tenant derived key; verify re-derives the key
//! from the same tenant context → cross-tenant derive yields different
//! key → MAC mismatch.
//!
//! # wasm32 compatibility
//!
//! Uses `hmac` + `sha2` + `hkdf` crates — pure Rust, no C deps, no
//! `ring`. wasm32-unknown-unknown clean per S-11 hardening sprint.

use std::sync::{Arc, Mutex};

use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::error::ConsentLedgerError;

/// HKDF info string per security_model.md §7.2.
pub const HMAC_HKDF_INFO: &[u8] = b"corelink/v1/consent-hmac";

/// HMAC-SHA256 hex-64 signature wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HmacSignature {
    /// Hex-encoded HMAC-SHA256 digest (64 chars).
    pub hex: String,
    /// Key identifier (tenant_short_id) for rotation support.
    pub kid: String,
}

/// All fields that contribute to a consent HMAC preimage.
///
/// Passed to [`ConsentHmacSigner::sign`] and [`ConsentHmacSigner::verify`]
/// to avoid the `too_many_arguments` clippy lint.
#[derive(serde::Serialize, Debug, Clone)]
pub struct HmacParams<'a> {
    /// Consent or revocation record ID.
    pub record_id: &'a str,
    /// Tenant identifier (used as HKDF salt for tenant-scoping).
    pub tenant_id: &'a str,
    /// Purpose string.
    pub purpose: &'a str,
    /// SHA-256 hex-64 of the notice text.
    pub notice_text_hash: &'a str,
    /// Semver notice version.
    pub notice_version: &'a str,
    /// BCP-47 locale string.
    pub locale: &'a str,
    /// Wording variant ID.
    pub wording_id: &'a str,
    /// UI capture timestamp.
    pub ui_capture_ts: &'a str,
    /// Server submission timestamp.
    pub submission_ts: &'a str,
}

/// Derive a 32-byte per-tenant HMAC key from the master key using
/// HKDF-SHA256.
///
/// - `salt` = `tenant_id.as_bytes()` (tenant-scoping prevents cross-tenant
///   forgery).
/// - `ikm` = `master_key`.
/// - `info` = [`HMAC_HKDF_INFO`].
fn derive_tenant_key(master_key: &[u8], tenant_id: &str) -> Result<[u8; 32], ConsentLedgerError> {
    let hk = Hkdf::<Sha256>::new(Some(tenant_id.as_bytes()), master_key);
    let mut okm = [0u8; 32];
    hk.expand(HMAC_HKDF_INFO, &mut okm)
        .map_err(|e| ConsentLedgerError::Hmac(format!("HKDF expand failed: {e}")))?;
    Ok(okm)
}

/// Compute HMAC-SHA256 over the canonical JSON preimage of `params`.
fn compute_hmac(key: &[u8], params: &HmacParams<'_>) -> Result<[u8; 32], ConsentLedgerError> {
    let preimage_bytes = serde_json::to_vec(params)
        .map_err(|e| ConsentLedgerError::Hmac(format!("preimage serialisation: {e}")))?;

    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .map_err(|e| ConsentLedgerError::Hmac(format!("HMAC key length: {e}")))?;
    mac.update(&preimage_bytes);
    Ok(mac.finalize().into_bytes().into())
}

/// Trait for signing and verifying consent HMAC signatures.
pub trait ConsentHmacSigner: Send + Sync {
    /// Sign a consent record and return [`HmacSignature`].
    fn sign(&self, params: &HmacParams<'_>) -> Result<HmacSignature, ConsentLedgerError>;

    /// Verify a consent HMAC signature (stateless — no DB lookup).
    ///
    /// Returns `true` if the signature is valid for the given parameters.
    fn verify(
        &self,
        signature_hex: &str,
        kid: &str,
        params: &HmacParams<'_>,
    ) -> Result<bool, ConsentLedgerError>;
}

/// In-memory HMAC signer backed by a fixed 32-byte master key.
///
/// Uses `Arc<Mutex<()>>` F-001 closure (per-instance, NEVER
/// `static LazyLock`).
#[derive(Debug)]
pub struct InMemoryConsentHmacSigner {
    /// 32-byte master key.
    master_key: Vec<u8>,
    _lock: Arc<Mutex<()>>,
}

impl InMemoryConsentHmacSigner {
    /// Create a new signer with the given master key.
    ///
    /// In production the master key is loaded from KMS / D1 vault
    /// (key_management.md §2; WI-S11-008 wiring).
    pub fn new(master_key: Vec<u8>) -> Self {
        Self {
            master_key,
            _lock: Arc::new(Mutex::new(())),
        }
    }

    /// Create a deterministic test signer (32 zero bytes).
    pub fn new_test() -> Self {
        Self::new(vec![0u8; 32])
    }
}

impl ConsentHmacSigner for InMemoryConsentHmacSigner {
    fn sign(&self, params: &HmacParams<'_>) -> Result<HmacSignature, ConsentLedgerError> {
        let tenant_key = derive_tenant_key(&self.master_key, params.tenant_id)?;
        let mac = compute_hmac(&tenant_key, params)?;
        Ok(HmacSignature {
            hex: hex::encode(mac),
            kid: format!(
                "consent-hmac-{}",
                &params.tenant_id[..params.tenant_id.len().min(8)]
            ),
        })
    }

    fn verify(
        &self,
        signature_hex: &str,
        _kid: &str,
        params: &HmacParams<'_>,
    ) -> Result<bool, ConsentLedgerError> {
        let tenant_key = derive_tenant_key(&self.master_key, params.tenant_id)?;
        let expected_mac = compute_hmac(&tenant_key, params)?;
        let expected_hex = hex::encode(expected_mac);

        // Constant-time comparison via subtle to prevent timing attacks.
        use subtle::ConstantTimeEq;
        let result = expected_hex.as_bytes().ct_eq(signature_hex.as_bytes());
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn params<'a>(
        record_id: &'a str,
        tenant_id: &'a str,
        notice_text_hash: &'a str,
    ) -> HmacParams<'a> {
        HmacParams {
            record_id,
            tenant_id,
            purpose: "analytics_personalized",
            notice_text_hash,
            notice_version: "1.2.0",
            locale: "pt-BR",
            wording_id: "wording-v3",
            ui_capture_ts: "2026-05-13T10:00:00Z",
            submission_ts: "2026-05-13T10:00:02Z",
        }
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let signer = InMemoryConsentHmacSigner::new_test();
        let p = params("consent-id-01", "tenant-01", "abc123");
        let sig = signer.sign(&p).unwrap();
        let valid = signer.verify(&sig.hex, &sig.kid, &p).unwrap();
        assert!(valid);
    }

    #[test]
    fn cross_tenant_forgery_rejected() {
        let signer = InMemoryConsentHmacSigner::new_test();
        let p1 = params("consent-id-01", "tenant-01", "abc123");
        let sig = signer.sign(&p1).unwrap();
        // Verify T1 sig against T2
        let p2 = params("consent-id-01", "tenant-02", "abc123");
        let valid = signer.verify(&sig.hex, &sig.kid, &p2).unwrap();
        assert!(!valid, "cross-tenant forgery must be rejected");
    }

    #[test]
    fn tampered_hash_rejected() {
        let signer = InMemoryConsentHmacSigner::new_test();
        let p1 = params("consent-id-01", "tenant-01", "abc123");
        let sig = signer.sign(&p1).unwrap();
        let p_tampered = params("consent-id-01", "tenant-01", "TAMPERED");
        let valid = signer.verify(&sig.hex, &sig.kid, &p_tampered).unwrap();
        assert!(!valid, "tampered hash must be rejected");
    }
}
