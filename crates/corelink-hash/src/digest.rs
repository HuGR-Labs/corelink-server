//! `Digest` newtype: 32-byte BLAKE3 output with a private field.
//!
//! Only constructible via [`Digest::compute`] or [`Digest::from_hex`]. Callers
//! cannot fabricate a `Digest` from arbitrary bytes; the type-system carries
//! the invariant that any `Digest` value originated from either a real
//! hashing of a body or a parsed (length- and alphabet-checked) hex string.

use subtle::ConstantTimeEq;

use crate::error::ParseError;

/// Length of a [`Digest`] in raw bytes (BLAKE3-256 output size).
pub const DIGEST_LEN: usize = 32;

/// 32-byte BLAKE3 content digest.
///
/// `PartialEq` / `Eq` are derived for ergonomic non-adversarial use (set
/// membership, D1 lookups). For *adversarial* timing-sensitive paths — i.e.
/// any verification step where an attacker may observe latency — use
/// [`Self::verify_constant_time`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digest([u8; DIGEST_LEN]);

impl Digest {
    /// Hash `body` with BLAKE3-256 and return the resulting [`Digest`].
    ///
    /// `body` is borrowed; no ownership taken. BLAKE3 is deterministic, so
    /// `compute(b) == compute(b)` for every byte slice.
    #[must_use]
    pub fn compute(body: &[u8]) -> Self {
        Self(*blake3::hash(body).as_bytes())
    }

    /// Parse a 64-character hex string (RFC 4648 §8 base16; lowercase or
    /// uppercase accepted) into a [`Digest`].
    ///
    /// Returns:
    /// - [`ParseError::InvalidLength`] if `s.len() != 64`.
    /// - [`ParseError::InvalidHexByte`] (with the byte position) if any
    ///   character is outside `[0-9a-fA-F]`.
    pub fn from_hex(s: &str) -> Result<Self, ParseError> {
        let bytes = s.as_bytes();
        if bytes.len() != DIGEST_LEN * 2 {
            return Err(ParseError::InvalidLength(bytes.len()));
        }

        let mut out = [0u8; DIGEST_LEN];
        // Manual hex decode so we can localize the bad-byte position to the
        // exact character index. The `hex` crate would aggregate this away.
        // Iteration is index-free (clippy::indexing_slicing satisfied) by
        // zipping the output slots against the 2-byte chunks.
        for (slot, (i, chunk)) in out.iter_mut().zip(bytes.chunks_exact(2).enumerate()) {
            let &[hi_byte, lo_byte] = chunk else {
                // Unreachable: chunks_exact(2) always yields slices of len 2.
                return Err(ParseError::InvalidLength(bytes.len()));
            };
            let hi = decode_nibble(hi_byte).map_err(|()| ParseError::InvalidHexByte(i * 2))?;
            let lo = decode_nibble(lo_byte)
                .map_err(|()| ParseError::InvalidHexByte(i * 2 + 1))?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
    }

    /// Render this digest as a 64-character lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Compare two digests in **constant time**.
    ///
    /// Use this rather than `==` when an attacker may observe the latency
    /// difference between a fully-correct digest and one whose first `k`
    /// bytes are correct. The standard derived `PartialEq` impl
    /// short-circuits on the first mismatching byte and therefore leaks
    /// `k` through timing.
    #[must_use]
    pub fn verify_constant_time(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }

    /// Borrow the raw 32 bytes — **testing escape hatch only**.
    ///
    /// Hidden from rustdoc (`#[doc(hidden)]`) and not part of the public
    /// stability surface. Production code must construct [`Digest`] values
    /// via [`Self::compute`] or [`Self::from_hex`] to preserve the
    /// type-driven CAS-integrity invariant. This accessor exists so the
    /// crate's own integration tests (which live outside `src/`) can
    /// generate adversarial single-bit-flipped digests without re-deriving
    /// them through hex manipulation.
    #[doc(hidden)]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }
}

impl core::fmt::Display for Digest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl core::fmt::Debug for Digest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Digest({})", self.to_hex())
    }
}

fn decode_nibble(b: u8) -> Result<u8, ()> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(()),
    }
}
