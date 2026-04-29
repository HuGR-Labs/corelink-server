//! Error type for [`derive_prefix`](crate::derive_prefix).
//!
//! In practice, [`derive_prefix`] is total over its typed inputs — the only
//! way this enum is constructed is through internal invariants that should
//! be impossible to violate (e.g. an HMAC engine returning a digest of the
//! wrong size). The variant exists as defense-in-depth so that any future
//! refactor introducing fallible plumbing has an explicit error to surface
//! rather than panicking.

use thiserror::Error;

/// Errors that can be produced while deriving a tenant path prefix.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeriveError {
    /// The HMAC engine returned an output that did not match SHA-256's
    /// 32-byte contract. This must not happen with the `hmac` + `sha2`
    /// crates and signals memory corruption or a broken dependency.
    #[error("internal hmac engine returned an output of unexpected size (expected 32, got {0})")]
    HmacOutputSize(usize),
}
