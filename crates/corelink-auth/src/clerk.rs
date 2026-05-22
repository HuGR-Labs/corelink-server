//! Clerk JWT verify surface — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-clerk`. The actual
//! implementation lives in `crates/corelink-clerk/` (Stage 1 Stream B
//! sub-step B.3 Option-A aggregator pattern).
//!
//! Production JWKS HTTPS fetch + jsonwebtoken adapter is gated by the
//! `corelink-auth/clerk-jwt-adapter` feature (forwards to
//! `corelink-clerk/jwt-adapter`).

pub use corelink_clerk::*;
