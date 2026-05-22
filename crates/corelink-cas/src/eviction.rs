//! Eviction policy — wave-33 canonical CAS surface.
//!
//! Re-exports the entire public API of `corelink-eviction`. The actual
//! implementation lives in `crates/corelink-eviction/` (Stage 1 Stream
//! A sub-step A.1 Option-A aggregator pattern; see crate-level rustdoc).
//!
//! The absorbed crate organizes its surface as a set of `pub mod`
//! submodules (`audit`, `blob_meta`, `error`, `metrics`, `phase`,
//! `reachable`, `region`, `reservation`, `storage_state`, `tier`).
//! Glob-re-exporting from a crate exposes its top-level public items;
//! the submodules themselves remain reachable through the
//! canonical-but-shadowed `corelink_eviction::*` path, which keeps
//! working unchanged.

pub use corelink_eviction::*;

// Re-surface the `corelink_eviction` submodules at the canonical path
// so consumers can write `corelink_cas::eviction::tier::StorageTier`.
pub use corelink_eviction::audit;
pub use corelink_eviction::blob_meta;
pub use corelink_eviction::error;
pub use corelink_eviction::metrics;
pub use corelink_eviction::phase;
pub use corelink_eviction::reachable;
pub use corelink_eviction::region;
pub use corelink_eviction::reservation;
pub use corelink_eviction::storage_state;
pub use corelink_eviction::tier;
