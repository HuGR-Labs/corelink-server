//! Salted SHA-256 recipient-hash newtype.
//!
//! Privacy-by-design: the survey infra never stores raw email or any
//! other PII directly identifying the recipient. The token + the stored
//! response row reference only the [`RecipientHash`], a 32-byte SHA-256
//! digest of `salt || recipient_id`. The salt is a per-environment
//! secret rotated alongside the [`crate::SigningKey`].
//!
//! Mirrors the `corelink-audit::EmailHash` pattern: there is no
//! `From<String>` constructor; the only way in is the derivation
//! function with the salt-bearing call.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::SurveyError;

/// SHA-256 length in bytes.
pub const RECIPIENT_HASH_LEN: usize = 32;

/// Salted SHA-256 hash of the recipient identifier (typically an email
/// address). 32 raw bytes; rendered as 64-char lowercase hex on the
/// wire + at rest.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipientHash([u8; RECIPIENT_HASH_LEN]);

impl RecipientHash {
    /// Derive a [`RecipientHash`] from a raw recipient id + salt.
    ///
    /// The recipient id is typically a normalized email address
    /// (`trim().to_lowercase()`). The salt is the per-environment
    /// secret loaded via the same key-management surface that holds
    /// the [`crate::SigningKey`]. Both inputs are consumed; the raw
    /// recipient id is **never stored** by this crate.
    #[must_use]
    pub fn derive_with_salt(recipient_id: &str, salt: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(b"|");
        hasher.update(recipient_id.as_bytes());
        let out = hasher.finalize();
        let mut bytes = [0u8; RECIPIENT_HASH_LEN];
        for (dst, src) in bytes.iter_mut().zip(out.iter()) {
            *dst = *src;
        }
        Self(bytes)
    }

    /// Construct a [`RecipientHash`] from 32 raw bytes (e.g. read from
    /// the database column). Used by adapters that already hold the
    /// derived digest; **never** call this with an attacker-controlled
    /// blob — the type's invariant is that the bytes are a SHA-256
    /// digest of `salt || recipient_id` with a per-environment salt.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; RECIPIENT_HASH_LEN]) -> Self {
        Self(bytes)
    }

    /// Parse a 64-char lowercase hex digest into a [`RecipientHash`].
    ///
    /// # Errors
    ///
    /// Returns [`SurveyError::MalformedRecipientHash`] if the input
    /// length is not 64 or any character is outside `[0-9a-f]`.
    pub fn from_hex(s: &str) -> Result<Self, SurveyError> {
        if s.len() != RECIPIENT_HASH_LEN * 2 {
            return Err(SurveyError::MalformedRecipientHash);
        }
        let raw = hex::decode(s).map_err(|_| SurveyError::MalformedRecipientHash)?;
        if raw.len() != RECIPIENT_HASH_LEN {
            return Err(SurveyError::MalformedRecipientHash);
        }
        let mut bytes = [0u8; RECIPIENT_HASH_LEN];
        for (dst, src) in bytes.iter_mut().zip(raw.iter()) {
            *dst = *src;
        }
        Ok(Self(bytes))
    }

    /// Borrow the raw 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; RECIPIENT_HASH_LEN] {
        &self.0
    }

    /// Render the digest as 64-char lowercase hex.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl core::fmt::Display for RecipientHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
