//! Merkle chain processor — wave-33 canonical re-export.
//!
//! Re-exports the entire public API of `corelink-audit-chain` (the
//! S-09 Merkle chain processor + tamper-detection job). The actual
//! implementation lives in `crates/corelink-audit-chain/` (Stage 0
//! sub-step 4 Option-A aggregator pattern; see crate-level rustdoc
//! and `specs/_audits/2026-05-22-w33-stage0-foundation.md` §4).
//!
//! Note: the in-crate hash-primitive helpers
//! (`compute_content_hash` / `link_chain_hash` / `ContentHash` /
//! `ChainHash`) previously lived under `corelink_audit::chain::*`.
//! They were renamed to `corelink_audit::link_hash::*` and are still
//! re-exported at the crate root for backwards-compat; the
//! `corelink_audit::chain` namespace now belongs to the absorbed
//! `corelink-audit-chain` crate.

pub use corelink_audit_chain::*;

/// Module-path marker used by the crate-level smoke tests. Returns
/// the canonical wave-33 import path for this submodule.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_audit::chain"
}
