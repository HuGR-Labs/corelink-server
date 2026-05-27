//! Key rotation — wave-33 canonical ops aggregator (W35-P2 update).
//!
//! - [`adapters`] — `corelink-rotation-adapters` (still external):
//!   per-provider rotation adapters.
//! - [`worker`] — physically absorbed (W35-P2-OPS): rotation worker
//!   orchestrator. Was `corelink-rotation-worker`.

/// Per-provider rotation adapters.
///
/// Re-exports the entire public API of `corelink-rotation-adapters`.
pub mod adapters {
    pub use corelink_rotation_adapters::*;
}

pub mod worker;
