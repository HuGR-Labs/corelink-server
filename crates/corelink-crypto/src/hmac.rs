//! HMAC primitives — wave-33 canonical surface.
//!
//! Surfaces the upstream `hmac::Hmac` + `sha2::Sha256` types under the
//! canonical `corelink_crypto::hmac::*` namespace so Stage 1 streams
//! have a single import path for HMAC-SHA-256 (the only HMAC variant
//! currently used in CoreLink: signup-token HMAC, survey-token HMAC,
//! audit-chain link hashing).
//!
//! No new abstractions are introduced — production callers continue to
//! use `hmac::Hmac::<Sha256>::new_from_slice(key)?` exactly as before.

pub use ::hmac::{Hmac, Mac};
pub use ::sha2::Sha256;

/// Canonical HMAC-SHA-256 type alias used across CoreLink (signup
/// tokens, survey tokens, audit-chain link hashing).
pub type HmacSha256 = Hmac<Sha256>;
