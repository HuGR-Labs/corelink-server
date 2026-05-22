//! Deploy verifier (artifact integrity + SBOM cross-check) — wave-33
//! canonical surface.
//!
//! Re-exports the entire public API of `corelink-deploy-verifier`. The
//! actual implementation lives in `crates/corelink-deploy-verifier/`
//! (Stage 1 Stream C sub-step C.2 Option-A aggregator pattern).

pub use corelink_deploy_verifier::*;
