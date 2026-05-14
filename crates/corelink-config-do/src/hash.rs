//! Payload hashing for the DO config-singleton (WI-S13-001).
//!
//! `payload_hash` is SHA-256 of the RFC 8785 JCS canonical JSON of the
//! [`ConfigPayload`]. Deterministic across platforms (BTreeMap keys are
//! sorted alphabetically by JCS; no HashMap iteration order dependence).

use sha2::{Digest, Sha256};

use crate::{ConfigError, ConfigPayload};

/// Compute SHA-256 over the RFC 8785 JCS canonical JSON of `payload`.
///
/// Returns a 32-byte hash. Callers store this in `ConfigVersionEntry::payload_hash`
/// and in D1 `config_change_log.payload_hash`.
///
/// # Errors
///
/// Returns [`ConfigError::Serialization`] if JCS canonicalization fails
/// (should be unreachable for valid `ConfigPayload`; surfaced for safety).
pub fn compute_payload_hash(payload: &ConfigPayload) -> Result<[u8; 32], ConfigError> {
    let canonical =
        serde_jcs::to_string(payload).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    let digest = Sha256::digest(canonical.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

/// Render a 32-byte hash as a lowercase hex string (64 chars).
#[must_use]
pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}
