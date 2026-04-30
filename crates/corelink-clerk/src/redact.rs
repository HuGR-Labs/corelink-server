//! PII-safe principal hashing for trace-span emission.
//!
//! INV-NO-PII-IN-LOGS (CRITICAL) mandates that raw `sub` claims never
//! appear in logs/traces; the canonical surrogate is a SHA-256-based
//! short hash (8 hex chars) emitted as the `auth.principal_hash` span
//! attribute (WI §21).
//!
//! The hash is **not** cryptographically secret — it's a privacy
//! redaction surrogate. Callers that need cryptographic uniqueness
//! (audit chain hashing) MUST use the full domain-separated chain
//! hash from S-09 instead.

use sha2::{Digest, Sha256};

/// Compute the canonical 8-hex-char principal hash for trace
/// emission. Domain-separated under `corelink/v1/principal-hash` to
/// keep the surrogate distinct from any other `sub`-derived value.
#[must_use]
pub fn principal_hash(sub: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"corelink/v1/principal-hash\x00");
    hasher.update(sub.as_bytes());
    let digest = hasher.finalize();
    // Only the first 4 bytes (8 hex chars) per WI §21.
    // Unreachable fallback: SHA-256 always emits 32 bytes; clippy prefers
    // unwrap_or_default over explicit match here.
    let first4: [u8; 4] = digest.get(..4).and_then(|s| s.try_into().ok()).unwrap_or_default();
    hex::encode(first4)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test-only: Result arms here are unreachable on fixed inputs"
)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_eight_hex_chars() {
        let h = principal_hash("user_2abc");
        assert_eq!(h.len(), 8);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(principal_hash("alice"), principal_hash("alice"));
    }

    #[test]
    fn hash_distinguishes_inputs() {
        assert_ne!(principal_hash("alice"), principal_hash("bob"));
    }

    #[test]
    fn hash_does_not_leak_input() {
        let sub = "user_long_secret_value_2abc";
        let h = principal_hash(sub);
        assert!(!h.contains("user"));
        assert!(!h.contains("secret"));
    }
}
