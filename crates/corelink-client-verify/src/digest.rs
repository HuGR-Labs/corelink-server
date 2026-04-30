//! Re-exports of the digest types used by client-side verify.
//!
//! The `corelink-hash` crate is the single source of truth for the
//! `Digest` newtype + constant-time compare. This crate re-exports it so
//! SDK consumers (Python pyO3, Go cgo, JS WASM via wasm-bindgen) only
//! depend on `corelink-client-verify`. Re-exporting (rather than
//! pub-using just the name) means the SDK FFI surface and the Rust-native
//! surface share types without forcing FFI consumers to take an extra
//! workspace dep.

pub use corelink_hash::{Digest, ParseError, DIGEST_LEN};

/// Stable error-taxonomy code returned by
/// [`VerifyError::DigestMismatch`](crate::VerifyError::DigestMismatch).
///
/// Mirrors `error_taxonomy.md` entry `COR_CAS_DIGEST_MISMATCH`. Re-exported
/// here so SDK FFI wrappers can map the rust-typed error onto language
/// idiomatic exceptions without taking a `corelink-hash` dep.
pub const COR_CAS_DIGEST_MISMATCH: &str = corelink_hash::COR_CAS_DIGEST_MISMATCH;

/// Stable error-taxonomy code returned by
/// [`VerifyError::VerifyDisabled`](crate::VerifyError::VerifyDisabled).
///
/// Surfaced when a caller explicitly constructed a verifier via
/// [`VerifyConfig::disabled`](crate::VerifyConfig::disabled) and then
/// invoked verify anyway. Documented in `error_taxonomy.md`.
pub const COR_CAS_VERIFY_DISABLED: &str = "COR_CAS_VERIFY_DISABLED";

/// Stable error-taxonomy code surfaced by stream verify when the
/// underlying `AsyncRead` errors before end-of-stream.
#[cfg(feature = "stream")]
pub const COR_CAS_VERIFY_IO: &str = "COR_CAS_VERIFY_IO";
