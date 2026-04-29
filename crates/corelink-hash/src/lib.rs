//! CoreLink CAS digest types — BLAKE3 + constant-time verify.
//!
//! Implements the type-driven integrity layer for CTRL-CAS-001 / INV-CAS-INTEGRITY
//! (`security_model.md §6.1`, `invariant_registry.md §3.2`). The crate is a
//! deliberately small public surface:
//!
//! - [`Digest`] — 32-byte BLAKE3 output, newtype with a private field. Only
//!   constructible via [`Digest::compute`] (server-side hash) or
//!   [`Digest::from_hex`] (parsing untrusted client input).
//! - [`VerifiedBody`] — body + digest envelope. Only constructible via
//!   [`VerifiedBody::new`], which **always** runs the verify step. By
//!   construction, no caller can write a body to storage without first having
//!   proved that the body matches its claimed digest.
//! - [`HashMismatch`] / [`ParseError`] — error types.
//!
//! # Quickstart
//!
//! ```rust
//! use bytes::Bytes;
//! use corelink_hash::{Digest, VerifiedBody};
//!
//! # fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let body = Bytes::from_static(b"hello world");
//! let claimed = Digest::from_hex(
//!     "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
//! )?;
//!
//! // Constructor runs the verify; mismatch returns Err(HashMismatch).
//! let vb = VerifiedBody::new(body, claimed)?;
//! assert_eq!(vb.body().len(), 11);
//! # Ok(())
//! # }
//! ```
//!
//! # Anti-patterns (do NOT)
//!
//! - **Do not** construct [`Digest`] or [`VerifiedBody`] from raw bytes
//!   outside this crate. Both newtypes have private fields precisely so
//!   callers go through the verifying constructors.
//! - **Do not** compare digests with `PartialEq` for security-sensitive
//!   paths. `PartialEq` is byte-by-byte and short-circuiting; use
//!   [`Digest::verify_constant_time`] when an attacker may observe timing.
//!   `PartialEq` is acceptable for non-adversarial uses (e.g. set membership,
//!   D1 indexing) but the rustdoc on each digest comparison call site should
//!   say which mode applies.
//! - **Do not** log raw digest bytes; render via [`Digest::to_hex`] or the
//!   `Display` impl, which the audit and observability layers expect.

#![forbid(unsafe_code)]

mod digest;
mod error;
mod store;
mod verified_body;

pub use digest::{Digest, DIGEST_LEN};
pub use error::{HashMismatch, ParseError, COR_CAS_DIGEST_MISMATCH};
pub use store::BlobStoreWrite;
pub use verified_body::VerifiedBody;
