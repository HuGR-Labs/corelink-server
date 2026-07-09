//! Deterministic `email_hash` primitive used for indexed lookup-by-email
//! without exposing plaintext PII at rest.
//!
//! # Algorithm
//!
//! ```text
//! email_hash       = HMAC-SHA256(email_hash_key, lower(trim(email)))
//! email_hash_key   = HKDF-SHA256(
//!     ikm  = master_key (Worker secret CORELINK_MASTER_KEY; 32 random bytes),
//!     salt = b"corelink-email-hash-salt-v1",
//!     info = b"corelink-v1-email-hash-key",
//!     L    = 32 bytes,
//! )
//! ```
//!
//! The Postgres canonical evaluation lives inside the migration:
//!
//! ```sql
//! email_hash = hmac(
//!     lower(trim(email))::bytea,
//!     current_setting('app.email_hash_key', true)::bytea,
//!     'sha256'
//! )
//! ```
//!
//! This Rust implementation MUST stay byte-exact with the Postgres
//! version — every property test that asserts cross-language equivalence
//! lives in `tests/prop_email_hash.rs`.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::ZeroizeOnDrop;

/// Length of an `email_hash` value in bytes (HMAC-SHA256 output).
pub const EMAIL_HASH_LEN: usize = 32;

/// HKDF salt bytes (canonical per `auth_model.md` / WI-S03-005 §6.1.3-bis).
const HKDF_SALT: &[u8] = b"corelink-email-hash-salt-v1";

/// HKDF info bytes for the email-hash key (domain-separated against
/// `b"ac-sig"`, `b"manifest-sig"`, `b"meta-manifest-sig"`,
/// `b"audit-chain"`; non-prefix → length-extension safe).
const HKDF_INFO: &[u8] = b"corelink-v1-email-hash-key";

/// 32-byte secret used to compute `email_hash` values.
///
/// The struct zeroes its bytes on drop and intentionally implements
/// neither `Clone` nor `Copy` so the secret cannot be duplicated by
/// accident.
#[derive(ZeroizeOnDrop)]
pub struct EmailHashKey([u8; EMAIL_HASH_LEN]);

impl core::fmt::Debug for EmailHashKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("EmailHashKey(REDACTED)")
    }
}

impl EmailHashKey {
    /// Construct from raw 32 bytes (e.g. read from a Worker secret).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; EMAIL_HASH_LEN]) -> Self {
        Self(bytes)
    }

    /// Construct from a hex-encoded ASCII string (the format set on the
    /// Neon database: `ALTER DATABASE … SET app.email_hash_key = '<hex>'`).
    ///
    /// Returns `None` if `hex_str` is not exactly `2 * EMAIL_HASH_LEN`
    /// hex-encoded ASCII bytes.
    #[must_use]
    pub fn from_hex(hex_str: &str) -> Option<Self> {
        let bytes = hex::decode(hex_str.trim()).ok()?;
        let arr: [u8; EMAIL_HASH_LEN] = bytes.try_into().ok()?;
        Some(Self(arr))
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Derive the per-database `email_hash_key` from a 32-byte master via
/// HKDF-SHA256 with the canonical (salt, info) tuple.
///
/// The deployment script runs this once at provisioning time and writes
/// the resulting hex-encoded key to
/// `ALTER DATABASE … SET app.email_hash_key = '…'`.
#[must_use]
pub fn derive_email_hash_key(master_key: &[u8; 32]) -> EmailHashKey {
    // HKDF-Extract: prk = HMAC(salt, ikm).
    // `Hmac::<Sha256>::new_from_slice` only fails for invalid key lengths,
    // and SHA-256 accepts any byte slice (block size 64 → keys longer than
    // 64 bytes are pre-hashed by `Hmac`); the call is infallible. The
    // strategic `expect_used` allow documents the unreachable Err arm.
    #[allow(
        clippy::expect_used,
        reason = "Hmac<Sha256>::new_from_slice never returns Err for any byte slice"
    )]
    let mut extract = <Hmac<Sha256> as KeyInit>::new_from_slice(HKDF_SALT)
        .expect("HMAC-SHA256 accepts any salt length");
    extract.update(master_key);
    let prk = extract.finalize().into_bytes();

    // HKDF-Expand: T(1) = HMAC(prk, info || 0x01). PRK is exactly 32 bytes
    // (SHA-256 output) so this also never errors.
    #[allow(
        clippy::expect_used,
        reason = "Hmac<Sha256>::new_from_slice on a 32-byte SHA-256 output is infallible"
    )]
    let mut expand = <Hmac<Sha256> as KeyInit>::new_from_slice(&prk)
        .expect("PRK is the 32-byte SHA-256 output; never errors");
    expand.update(HKDF_INFO);
    expand.update(&[0x01]);
    let okm = expand.finalize().into_bytes();

    // SHA-256 output is exactly 32 bytes, which equals EMAIL_HASH_LEN,
    // so the slice is in-bounds. Use `.get` to satisfy the
    // `indexing_slicing = "deny"` lint without an unwrap.
    let mut out = [0u8; EMAIL_HASH_LEN];
    if let Some(slice) = okm.get(..EMAIL_HASH_LEN) {
        out.copy_from_slice(slice);
    }
    EmailHashKey(out)
}

/// Compute `email_hash = HMAC-SHA256(key, lower(trim(email)))`.
///
/// `lower` is the ASCII lowercase transform; non-ASCII characters are
/// passed through unchanged. `trim` strips leading and trailing ASCII
/// whitespace (Postgres `trim()` default behaviour).
#[must_use]
pub fn compute_email_hash(key: &EmailHashKey, email: &str) -> [u8; EMAIL_HASH_LEN] {
    let normalised = canonicalise_email(email);
    #[allow(
        clippy::expect_used,
        reason = "Hmac<Sha256>::new_from_slice never returns Err for any byte slice"
    )]
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes())
        .expect("HMAC-SHA256 accepts any key length");
    mac.update(normalised.as_bytes());
    let tag = mac.finalize().into_bytes();
    let mut out = [0u8; EMAIL_HASH_LEN];
    if let Some(slice) = tag.get(..EMAIL_HASH_LEN) {
        out.copy_from_slice(slice);
    }
    out
}

fn canonicalise_email(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn determinism_same_input_same_hash() {
        let key = EmailHashKey::from_bytes([0x42; 32]);
        let h1 = compute_email_hash(&key, "user@acme.com");
        let h2 = compute_email_hash(&key, "user@acme.com");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), EMAIL_HASH_LEN);
    }

    #[test]
    fn case_insensitive() {
        let key = EmailHashKey::from_bytes([0x42; 32]);
        let h1 = compute_email_hash(&key, "User@Acme.COM");
        let h2 = compute_email_hash(&key, "user@acme.com");
        assert_eq!(h1, h2);
    }

    #[test]
    fn whitespace_trimmed() {
        let key = EmailHashKey::from_bytes([0x42; 32]);
        let h1 = compute_email_hash(&key, "  user@acme.com  ");
        let h2 = compute_email_hash(&key, "user@acme.com");
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_keys_produce_different_hashes() {
        let key_a = EmailHashKey::from_bytes([0x01; 32]);
        let key_b = EmailHashKey::from_bytes([0x02; 32]);
        let h_a = compute_email_hash(&key_a, "user@acme.com");
        let h_b = compute_email_hash(&key_b, "user@acme.com");
        assert_ne!(h_a, h_b);
    }

    #[test]
    fn derive_is_deterministic_per_master() {
        let master = [0x33; 32];
        let k1 = derive_email_hash_key(&master);
        let k2 = derive_email_hash_key(&master);
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn derive_changes_with_master() {
        let m1 = [0x33; 32];
        let m2 = [0x77; 32];
        let k1 = derive_email_hash_key(&m1);
        let k2 = derive_email_hash_key(&m2);
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn from_hex_roundtrip() {
        let bytes = [0xab_u8; 32];
        let hex_str = hex::encode(bytes);
        let key = EmailHashKey::from_hex(&hex_str).expect("32-byte hex parses");
        assert_eq!(key.as_bytes(), &bytes);
        assert!(EmailHashKey::from_hex("not-hex").is_none());
        assert!(EmailHashKey::from_hex("00ff").is_none());
    }

    #[test]
    fn debug_redacts() {
        let key = EmailHashKey::from_bytes([0x99; 32]);
        let s = format!("{key:?}");
        assert!(s.contains("REDACTED"));
        assert!(!s.contains("99"));
    }
}
