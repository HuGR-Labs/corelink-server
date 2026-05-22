//! [`DigestParseError`] — surfaced by [`crate::types::Digest::from_hex`].

use thiserror::Error;

/// Errors returned by [`crate::types::Digest::from_hex`].
///
/// Behaviour parity with `corelink_hash::ParseError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DigestParseError {
    /// Input was not exactly 64 characters long.
    #[error("digest hex must be 64 chars; got {0}")]
    InvalidLength(usize),

    /// A non-hex byte was found at the given position (0-based).
    #[error("invalid hex byte at position {0}")]
    InvalidHexByte(usize),
}
