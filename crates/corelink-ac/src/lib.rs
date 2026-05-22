//! `corelink-ac` — canonical Action Cache (AC) bounded-context surface
//! for the CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream A sub-step A.2b lands this crate as the
//! **single import target** for every AC-tier primitive that
//! historically lived in 3 separate fragmented crates:
//!
//! - `corelink-ac-core` (renamed from the original `corelink-ac` by
//!   A.2a) — Merkle codec + dual-side verifier + HKDF-SHA256 signature.
//! - `corelink-ac-schema` — Region + simulation schema.
//! - `corelink-handler-ac` — AC lookup/update handlers + audit sink.
//!
//! ```text
//! use corelink_ac::core::*;        // Merkle codec + verifier
//! use corelink_ac::schema::*;      // Region + simulation schema
//! use corelink_ac::handler::*;     // AC handlers + audit sink
//! ```
//!
//! The original `corelink_ac::*` top-level paths (e.g.
//! `corelink_ac::MerkleVerifier`, `corelink_ac::sig::*`) ALSO continue
//! to resolve through the top-level [`pub use corelink_ac_core::*`]
//! glob; no historical caller needs to change a single line of source
//! once they update their `Cargo.toml` dep entry to point at this
//! umbrella instead of `corelink-ac-core` (or, equivalently, keep
//! pointing at `corelink-ac-core` directly until they're ready).
//!
//! ## Stage 1 absorption strategy — Option-A aggregator + A1 rename
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stream
//! A sub-step A.2 Option A1: the original `corelink-ac` crate was
//! renamed to `corelink-ac-core` in commit A.2a so the `corelink-ac`
//! package name could be reused by this umbrella. The 3 absorbed
//! crates remain the source of truth (Option-A aggregator pattern,
//! per Stage 0 SEAL §4); physical absorption is deferred to Stage 2.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 3 absorbed crates remains reachable at
//! its original path AND at the new canonical path. Zero behavioural
//! change — purely additive re-export façade.
//!
//! ## Charter constraints
//!
//! - `#![forbid(unsafe_code)]`: enforced at crate root.
//! - No `unwrap()`/`expect()`/`panic!()` in `src/`: zero source code
//!   beyond `pub use` re-exports lives here, so trivially satisfied.
//! - Audit fail-CLOSED preserved: not touched (handler-ac retains its
//!   own `AuditSink`; Stage 1+ migration to
//!   `corelink_audit::ports::AuditEmitter` is a separate consumer
//!   step).
//! - `#[non_exhaustive]` on every public enum + struct: zero new
//!   structs/enums introduced.
//! - L2.10 file-size: this `lib.rs` ≤ 100 LOC; sweet-spot green.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

// Historical top-level surface: existing consumers writing
// `use corelink_ac::MerkleVerifier;` etc. continue to resolve through
// this glob. This is the single most load-bearing line of this crate
// — keep it.
pub use corelink_ac_core::*;

pub mod core;
pub mod handler;
pub mod schema;

#[cfg(test)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new behaviour introduced; the absorbed crates
    //! own their own correctness tests.

    #[test]
    fn core_path_resolves() {
        let _ = std::any::type_name::<crate::core::MerkleError>();
    }

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
        // `corelink_ac::MerkleError` (i.e. WITHOUT the `::core::`
        // intermediate hop) must continue to resolve so existing
        // call sites need zero edit beyond a possible Cargo.toml dep
        // swap.
        let _ = std::any::type_name::<crate::MerkleError>();
    }
}
