//! Personal Access Token (PAT) issuance + verify + revoke — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-pat`. The actual
//! implementation lives in `crates/corelink-pat/` (Stage 1 Stream B
//! sub-step B.3 Option-A aggregator pattern).
//!
//! Charter: `subtle::ConstantTimeEq` on the PAT verify path is
//! preserved by reference (the absorbed crate's `verify` module
//! never short-circuits the token compare; not touched by this
//! aggregator).

pub use corelink_pat::*;
