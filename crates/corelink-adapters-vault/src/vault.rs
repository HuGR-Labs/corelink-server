//! HashiCorp Vault Transit BYOK adapter — wave-33 canonical Vault-
//! adapter surface.
//!
//! Re-exports the entire public API of `corelink-byok-vault`
//! (WI-S14-005 — Vault Enterprise FIPS 140-3 Level 1 build; mTLS auth
//! via client cert + CA cert, both `SecretString`-wrapped).
//!
//! The actual implementation lives in `crates/corelink-byok-vault/`
//! (Stage 1 Stream C sub-step C.4 Option-A aggregator pattern).
//!
//! **Note:** the pure-logic portion of `corelink-byok-vault` is ALSO
//! re-exported by `corelink-byok::vault` (Stream B sub-step B.2b)
//! behind the `vault` cargo feature. The decomposition between
//! pure-logic and HTTPS / mTLS portions is deferred to Stage 2.

pub use corelink_byok::vault::*;
