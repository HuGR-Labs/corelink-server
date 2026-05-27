//! `corelink-ac` — canonical Action Cache (AC) bounded-context surface
//! for the CoreLink Rust workspace.
//!
//! Wave-35 Phase 2 physically absorbs the 2 absorbed AC primitives
//! crates into this umbrella as inline submodules:
//!
//! - `ac_core` (formerly `corelink-ac-core`, originally `corelink-ac`
//!   per Wave-33 A.2a rename) — Merkle codec + dual-side verifier +
//!   HKDF-SHA256 signature.
//! - `schema` (formerly `corelink-ac-schema`) — Region + simulation
//!   schema.
//!
//! ```text
//! use corelink_ac::*;             // Merkle codec + verifier + sig (top-level via `pub use ac_core::*`)
//! use corelink_ac::schema::*;     // Region + simulation schema
//! use corelink_ac::handler::*;    // AC handlers + audit sink (still re-exported from `corelink-handler-ac`)
//! ```
//!
//! The `corelink-handler-ac` crate remains a workspace member; the
//! umbrella keeps a [`handler`] re-export shim until a future
//! consolidation lands.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 2 physically absorbed crates remains
//! reachable at its canonical path. Zero behavioural change —
//! purely a workspace-member consolidation.
//!
//! ## Charter constraints
//!
//! - `#![forbid(unsafe_code)]`: enforced at crate root.
//! - No `unwrap()`/`expect()`/`panic!()` in `src/`: every absorbed
//!   module carries the same `[lints.clippy]` deny-list from its
//!   former crate-root attributes; nothing is relaxed.
//! - Audit fail-CLOSED preserved: not touched (handler-ac retains its
//!   own `AuditSink`).
//! - `#[non_exhaustive]` on every public enum + struct: untouched.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ac_core;
/// AC D1 region + simulation schema (formerly `corelink-ac-schema`).
pub mod schema;

// Historical top-level surface: existing consumers writing
// `use corelink_ac::MerkleVerifier;` etc. continue to resolve through
// this glob, now sourced from the physically absorbed `ac_core`
// submodule.
pub use ac_core::*;

/// Re-export of the `corelink-handler-ac` public surface.
///
/// `corelink-handler-ac` is NOT yet physically absorbed (out of scope
/// for Wave-35 Phase 2 batch §1 — see
/// `specs/_audits/2026-05-26-wave-33-34-closure-followups.md` §4).
pub mod handler {
    pub use corelink_handler_ac::*;
}

#[cfg(test)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new behaviour introduced; the absorbed
    //! modules own their own correctness tests.

    #[test]
    fn schema_path_resolves() {
        let _ = std::any::type_name::<crate::schema::AcRegion>();
    }

    #[test]
    fn handler_path_resolves() {
        let _ = std::any::type_name::<crate::handler::AcHandlerError>();
    }

    #[test]
    fn historical_top_level_path_resolves() {
        // `corelink_ac::MerkleError` (i.e. resolving through the
        // top-level `pub use ac_core::*` glob) must continue to
        // resolve so existing call sites need zero edit beyond a
        // possible Cargo.toml dep swap.
        let _ = std::any::type_name::<crate::MerkleError>();
    }
}
