//! FIDO2 WebAuthn registration + assertion — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-webauthn`. The actual
//! implementation lives in `crates/corelink-webauthn/` (Stage 1
//! Stream B sub-step B.3 Option-A aggregator pattern).

pub use corelink_webauthn::*;
