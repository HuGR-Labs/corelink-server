//! Canonical [`Digest`] newtype — wave-33 Stage 0 cross-cutting type.
//!
//! 32-byte content-address newtype shared across every CoreLink context
//! crate. The hashing implementation (BLAKE3) lives in `corelink-crypto`
//! per §3 of the wave-33 reorg spec: `corelink-core` is the apex of the
//! dependency graph and intentionally carries no hashing primitive.
//! `corelink-crypto::blake3::compute(&[u8]) -> corelink_core::Digest`
//! is the bridge factory.
//!
//! Construction surface mirrors the pre-existing
//! `corelink_hash::Digest` for behavioural compatibility: callers
//! cannot fabricate a `Digest` from arbitrary bytes outside this module
//! without going through [`Digest::from_bytes`] (parser-style, no
//! computation), [`Digest::from_hex`] (length + alphabet check), or the
//! crypto crate's BLAKE3 factory.

use core::fmt;

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::errors::DigestParseError;

/// Length of a [`Digest`] in raw bytes (BLAKE3-256 output size).
pub const DIGEST_LEN: usize = 32;

/// 32-byte content-address digest.
///
/// `PartialEq` / `Eq` are derived for ergonomic non-adversarial use
/// (set membership, D1 lookups). For *adversarial* timing-sensitive
/// paths — i.e. any verification step where an attacker may observe
/// latency — use [`Self::verify_constant_time`].
///
/// Behaviour parity: this type mirrors `corelink_hash::Digest`
/// exactly. The BLAKE3 hashing constructor lives in
/// `corelink-crypto::blake3` (Stage 0 sub-step 2 absorbs it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[non_exhaustive]
pub struct Digest(#[serde(with = "hex_serde")] [u8; DIGEST_LEN]);

impl Digest {
    /// Construct a [`Digest`] directly from a 32-byte array.
    ///
    /// Reserved for crypto-crate bridges + deserialization paths that
    /// have already validated the source. Application code should use
    /// `corelink_crypto::blake3::compute` to hash a body, or
    /// [`Self::from_hex`] to parse a hex string.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; DIGEST_LEN]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw 32-byte array.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// Parse a 64-character hex string (RFC 4648 §8 base16; lowercase
    /// or uppercase accepted) into a [`Digest`].
    ///
    /// # Errors
    ///
    /// - [`DigestParseError::InvalidLength`] if `s.len() != 64`.
    /// - [`DigestParseError::InvalidHexByte`] (with the byte position)
    ///   if any character is outside `[0-9a-fA-F]`.
    pub fn from_hex(s: &str) -> Result<Self, DigestParseError> {
        let bytes = s.as_bytes();
        if bytes.len() != DIGEST_LEN * 2 {
            return Err(DigestParseError::InvalidLength(bytes.len()));
        }

        let mut out = [0u8; DIGEST_LEN];
        for (slot, (i, chunk)) in out.iter_mut().zip(bytes.chunks_exact(2).enumerate()) {
            let &[hi_byte, lo_byte] = chunk else {
                return Err(DigestParseError::InvalidLength(bytes.len()));
            };
            let hi = decode_nibble(hi_byte)
                .map_err(|()| DigestParseError::InvalidHexByte(i * 2))?;
            let lo = decode_nibble(lo_byte)
                .map_err(|()| DigestParseError::InvalidHexByte(i * 2 + 1))?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
    }

    /// Render this digest as a 64-character lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(DIGEST_LEN * 2);
        for byte in &self.0 {
            // hex::encode would pull a dep; manual lowercase nibbles
            // keep `corelink-core` minimal-dep (no hex/blake3/etc).
            out.push(nibble_to_hex(byte >> 4));
            out.push(nibble_to_hex(byte & 0x0f));
        }
        out
    }

    /// Compare two digests in **constant time**.
    ///
    /// Use this rather than `==` when an attacker may observe the
    /// latency difference between a fully-correct digest and one
    /// whose first `k` bytes are correct. The derived `PartialEq`
    /// impl short-circuits on the first mismatching byte and
    /// therefore leaks `k` through timing.
    #[must_use]
    pub fn verify_constant_time(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Digest").field(&self.to_hex()).finish()
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

fn decode_nibble(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble & 0x0f {
        0..=9 => (b'0' + nibble) as char,
        // SAFETY: nibble & 0x0f produces 0..=15; the match above
        // covers 0..=9 so the remaining values are 10..=15.
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

mod hex_serde {
    //! Hex (de)serialization for the inner 32-byte array, matching
    //! `corelink-hash` wire format exactly (lowercase 64-char hex).

    use serde::{Deserialize, Deserializer, Serializer};

    use super::{Digest, DIGEST_LEN};
    use crate::errors::DigestParseError;

    pub fn serialize<S>(bytes: &[u8; DIGEST_LEN], ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let d = Digest::from_bytes(*bytes);
        ser.serialize_str(&d.to_hex())
    }

    pub fn deserialize<'de, D>(de: D) -> Result<[u8; DIGEST_LEN], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(de)?;
        let d = Digest::from_hex(&s).map_err(|e| match e {
            DigestParseError::InvalidLength(n) => {
                serde::de::Error::invalid_length(n, &"64 hex chars")
            }
            DigestParseError::InvalidHexByte(i) => serde::de::Error::custom(format!(
                "invalid hex byte at position {i}"
            )),
        })?;
        Ok(*d.as_bytes())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_hex_lowercase() {
        let raw = [0x12u8; DIGEST_LEN];
        let d = Digest::from_bytes(raw);
        let hex = d.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(hex, "12".repeat(32));
        let parsed = Digest::from_hex(&hex).expect("round-trip hex must parse");
        assert_eq!(parsed, d);
    }

    #[test]
    fn from_hex_rejects_wrong_length() {
        assert!(matches!(
            Digest::from_hex("abcd"),
            Err(DigestParseError::InvalidLength(4))
        ));
    }

    #[test]
    fn from_hex_rejects_bad_alphabet() {
        let mut s = "00".repeat(32);
        // Replace position 5 with a non-hex char.
        s.replace_range(5..6, "Z");
        assert!(matches!(
            Digest::from_hex(&s),
            Err(DigestParseError::InvalidHexByte(5))
        ));
    }

    #[test]
    fn verify_constant_time_matches_partialeq() {
        let a = Digest::from_bytes([1u8; DIGEST_LEN]);
        let b = Digest::from_bytes([1u8; DIGEST_LEN]);
        let c = Digest::from_bytes([2u8; DIGEST_LEN]);
        assert!(a.verify_constant_time(&b));
        assert!(!a.verify_constant_time(&c));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn upper_and_lower_hex_both_parse() {
        let lower = "ab".repeat(32);
        let upper = "AB".repeat(32);
        assert_eq!(
            Digest::from_hex(&lower).unwrap(),
            Digest::from_hex(&upper).unwrap()
        );
    }

    #[test]
    fn serde_json_round_trip() {
        let d = Digest::from_bytes([0xab; DIGEST_LEN]);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
        let back: Digest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
