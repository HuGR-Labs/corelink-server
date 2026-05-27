//! `corelink-cas` — canonical Content-Addressable Storage (CAS)
//! bounded-context surface for the CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream A sub-step A.1 lands this crate as the
//! **single import target** for every CAS-tier primitive that
//! historically lived in 10 separate fragmented crates:
//!
//! ```text
//! use corelink_cas::chunker::*;            // Variable-size chunking
//! use corelink_cas::dedup::*;              // Content dedup index
//! use corelink_cas::edge::*;               // Edge cache adapter
//! use corelink_cas::eviction::*;           // Eviction policy
//! use corelink_cas::lru_tracker::*;        // LRU stats tracker
//! use corelink_cas::r2_multipart::*;       // R2 multipart upload state
//! use corelink_cas::multipart_schema::*;   // Multipart wire schema
//! use corelink_cas::meta::*;               // CAS meta-store ports
//! use corelink_cas::manifest::*;           // CAS manifest types
//! use corelink_cas::handler::*;            // CAS handler (write/read)
//! ```
//!
//! ## Stage 1 absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stream
//! A sub-step A.1, this crate "absorbs" 10 existing crates. Stage 0
//! sub-steps 2 + 3 + 4 established the Option-A aggregator pattern
//! (see `specs/_audits/2026-05-22-w33-stage0-foundation.md` §4): the
//! new umbrella crate `pub use`s each absorbed crate at the canonical
//! submodule path while the originals remain the source of truth
//! unchanged. This keeps every existing `INV-CAS-*` test green and
//! avoids touching benches / fuzz / examples on the CAS hot path.
//!
//! Stage 2 will physically absorb the source files once every consumer
//! has migrated to the canonical `corelink_cas::*` paths. Until then
//! both the original crate paths (e.g. `corelink_chunker::*`) and the
//! canonical paths (e.g. `corelink_cas::chunker::*`) resolve to the
//! same types.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 10 absorbed crates remains reachable at
//! its original path AND at the new canonical path. Zero behavioural
//! change — purely additive re-export façade.
//!
//! ## Charter constraints
//!
//! - `#![forbid(unsafe_code)]`: enforced at crate root (line below).
//! - No `unwrap()`/`expect()`/`panic!()` in `src/`: zero source code
//!   beyond `pub use` re-exports lives here, so trivially satisfied.
//! - `subtle::ConstantTimeEq` preserved: not touched (consumer crates
//!   keep their own use).
//! - Audit fail-CLOSED preserved: not touched. Any NEW audit-emit
//!   site that lands inside this crate post-aggregator MUST consume
//!   `corelink_audit::ports::AuditEmitter` (Stage 0 chokepoint).
//! - `#[non_exhaustive]` on every public enum + struct: trivially
//!   satisfied (zero new structs/enums introduced here).
//! - L2.10 file-size discipline: this `lib.rs` + 10 re-export files
//!   are tiny (~5 LOC each); deeply under the 200 LOC sweet spot.
//!
//! ## Wave 35 Phase 2 absorption
//!
//! Per `specs/_audits/2026-05-26-w35-p2-cas-absorption.md` (SEALED
//! 2026-05-26), 6 of the Wave-33 Option-A re-export tenants were
//! physically absorbed into this crate as inline submodules (13,673
//! LOC + 508 tests moved in-tree); workspace.members dropped by 6.
//! Public-API contract (`corelink_cas::<mod>::*`) is preserved 1:1;
//! INV-CAS-* invariants, `#![forbid(unsafe_code)]`, audit fail-CLOSED,
//! and `#[non_exhaustive]` discipline all preserved by reference.
//!
//! Absorbed crates (6):
//!
//! - `corelink-chunker` → [`chunker`] — variable-size chunking.
//! - `corelink-dedup` → [`dedup`] — content dedup index.
//! - `corelink-edge` → [`edge`] — edge cache adapter.
//! - `corelink-lru-tracker` → [`lru_tracker`] — LRU stats tracker.
//! - `corelink-manifest` → [`manifest`] — CAS manifest types.
//! - `corelink-multipart-schema` → [`multipart_schema`] — multipart
//!   wire schema.
//!
//! The remaining Wave-33 Option-A tenants (eviction, r2_multipart,
//! meta, handler) stay external for now — they retain live external
//! consumers and their absorption is a future sweep.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod chunker;
pub mod dedup;
pub mod edge;
pub mod eviction;
pub mod handler;
pub mod lru_tracker;
pub mod manifest;
pub mod meta;
pub mod multipart_schema;
pub mod r2_multipart;

pub mod r2_storage {
    //! Wave 33 Stage 2.A-v2 additive aggregator (per 2.A HALT audit §9(b)):
    //! canonical wave-33 path for R2 storage adapters that physically live
    //! in `corelink-worker::storage::r2`. Re-export rather than physical
    //! move preserves 63-consumer surface + Stage 1 Option-A façade design.
    //!
    //! Wave-33 Stage 2.E §11 follow-up: also surface the sibling
    //! `corelink_worker::storage::error::R2Error` type here so that
    //! consumers of the canonical surface do not have to dual-import
    //! `corelink_worker::storage::error::R2Error` alongside this module.
    //! Without this, the dispatch packet's mapping table would leave
    //! 7 R2Error-only consumer lines stranded on the impl crate path.
    pub use corelink_worker::storage::error::R2Error;
    pub use corelink_worker::storage::r2::*;
}

pub mod cache {
    //! Wave 33 Stage 2.A-v2 additive aggregator: canonical wave-33 path
    //! for CAS edge cache adapters that physically live in
    //! `corelink-worker::cache`.
    pub use corelink_worker::cache::*;
}

#[cfg(test)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new behaviour introduced; the absorbed crates
    //! own their own correctness tests (which remain green and run in
    //! their original crate `cargo test -p` scopes).

    #[test]
    fn chunker_path_resolves() {
        let _ = std::any::type_name::<crate::chunker::ChunkerConfig>();
    }

    #[test]
    fn dedup_path_resolves() {
        let _ = std::any::type_name::<crate::dedup::DedupError>();
    }

    #[test]
    fn edge_path_resolves() {
        let _ = std::any::type_name::<crate::edge::EdgeError>();
    }

    #[test]
    fn eviction_path_resolves() {
        // Prove the eviction module re-export is wired (uses a glob
        // re-export; surface a module path via type_name for any item
        // the absorbed crate exposes at the top level).
        let n = std::any::type_name::<crate::eviction::tier::Tier>();
        assert!(n.contains("corelink_eviction"));
    }

    #[test]
    fn lru_tracker_path_resolves() {
        let _ = std::any::type_name::<crate::lru_tracker::LruError>();
    }

    #[test]
    fn r2_multipart_path_resolves() {
        let _ = std::any::type_name::<crate::r2_multipart::MultipartError>();
    }

    #[test]
    fn multipart_schema_path_resolves() {
        let _ = std::any::type_name::<crate::multipart_schema::MultipartRegion>();
    }

    #[test]
    fn meta_path_resolves() {
        let _ = std::any::type_name::<crate::meta::MetaError>();
    }

    #[test]
    fn manifest_path_resolves() {
        let _ = std::any::type_name::<crate::manifest::ManifestError>();
    }

    #[test]
    fn handler_path_resolves() {
        let _ = std::any::type_name::<crate::handler::CasHandlerError>();
    }
}
