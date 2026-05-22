//! Tenant-scoped HMAC-SHA-256 pseudonymization — wave-33 canonical surface.
//!
//! Re-exports the entire public API of
//! `corelink-privacy-pseudonymize`. The actual implementation lives
//! in `crates/corelink-privacy-pseudonymize/` (Stage 1 Stream B
//! sub-step B.5 Option-A aggregator pattern).
//!
//! Charter: `subtle::ConstantTimeEq` on HMAC tag compare is preserved
//! by reference (the absorbed crate's `verify_tag` path never
//! short-circuits the byte compare).

pub use corelink_privacy_pseudonymize::*;
