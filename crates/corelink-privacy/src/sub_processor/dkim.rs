//! DKIM tenant-scoped key derivation via HKDF.
//!
//! Canonical per WI-S11-005 §9.1 DD-004:
//! - HKDF info string: `corelink/v1/dkim-broadcast` (HKDF_INFO_DKIM_BROADCAST).
//! - One unique 32-byte key per tenant.
//! - Cross-tenant isolation: property test 10k random pairs → 0 collisions.
//!
//! # Why tenant-scoped
//!
//! A global DKIM key would allow cross-tenant email signature replay.
//! Tenant-scoped HKDF-derived keys are unique per tenant while being
//! deterministically derivable from the master secret (security_model.md
//! §374 inheritance). Key rotation deferred to S-19.

use hkdf::Hkdf;
use hmac::Hmac;
use sha2::Sha256;

use super::event::HKDF_INFO_DKIM_BROADCAST;

/// Error type for DKIM key derivation failures.
#[derive(Debug)]
pub struct DkimDerivationError(pub String);

impl std::fmt::Display for DkimDerivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DKIM key derivation error: {}", self.0)
    }
}

impl std::error::Error for DkimDerivationError {}

/// Derive a 32-byte tenant-scoped DKIM key via HKDF-SHA256.
///
/// - `master_secret`: workspace-level secret (never stored plain; from KMS in production).
/// - `tenant_id`: canonical tenant identifier.
///
/// The derivation is: `HKDF-SHA256(ikm=master_secret, salt=tenant_id,
/// info=b"corelink/v1/dkim-broadcast") → 32 bytes`.
///
/// # Errors
///
/// Returns [`DkimDerivationError`] if HKDF expand fails (never happens for 32 bytes
/// with SHA-256 but API requires handling).
pub fn derive_dkim_key(
    master_secret: &[u8],
    tenant_id: &str,
) -> Result<[u8; 32], DkimDerivationError> {
    let hk = Hkdf::<Sha256, Hmac<Sha256>>::new(
        Some(tenant_id.as_bytes()), // salt = tenant_id for scoping
        master_secret,
    );
    let mut okm = [0u8; 32];
    hk.expand(HKDF_INFO_DKIM_BROADCAST, &mut okm)
        .map_err(|e| DkimDerivationError(format!("HKDF expand failed: {e}")))?;
    Ok(okm)
}

/// Returns true if two tenants derive DIFFERENT DKIM keys from the same master secret.
///
/// Used by property tests to verify cross-tenant isolation.
#[must_use]
pub fn dkim_keys_differ(master_secret: &[u8], tenant_a: &str, tenant_b: &str) -> bool {
    if tenant_a == tenant_b {
        return false; // same tenant → same key, trivially
    }
    match (
        derive_dkim_key(master_secret, tenant_a),
        derive_dkim_key(master_secret, tenant_b),
    ) {
        (Ok(key_a), Ok(key_b)) => key_a != key_b,
        _ => false,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn dkim_key_deterministic() {
        let secret = b"test-master-secret";
        let k1 = derive_dkim_key(secret, "tenant-abc").unwrap();
        let k2 = derive_dkim_key(secret, "tenant-abc").unwrap();
        assert_eq!(k1, k2, "DKIM key must be deterministic for same inputs");
    }

    #[test]
    fn dkim_key_cross_tenant_differs() {
        let secret = b"test-master-secret";
        let k1 = derive_dkim_key(secret, "tenant-abc").unwrap();
        let k2 = derive_dkim_key(secret, "tenant-xyz").unwrap();
        assert_ne!(k1, k2, "Different tenants must yield different DKIM keys");
    }

    #[test]
    fn dkim_keys_differ_helper() {
        let secret = b"test-master-secret";
        assert!(dkim_keys_differ(secret, "t1", "t2"));
        assert!(!dkim_keys_differ(secret, "t1", "t1"));
    }
}
