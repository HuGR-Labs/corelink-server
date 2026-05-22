//! Key rotation — wave-33 canonical ops aggregator.
//!
//! Two rotation-context crates folded under the canonical `rotation`
//! submodule as two sub-submodules (Stage 1 Stream C sub-step C.2
//! Option-A aggregator pattern):
//!
//! - [`adapters`] — `corelink-rotation-adapters`: per-provider
//!   rotation adapters.
//! - [`worker`] — `corelink-rotation-worker`: rotation worker
//!   orchestrator.

/// Per-provider rotation adapters.
///
/// Re-exports the entire public API of `corelink-rotation-adapters`.
pub mod adapters {
    pub use corelink_rotation_adapters::*;
}

/// Rotation worker orchestrator.
///
/// Re-exports the entire public API of `corelink-rotation-worker`.
pub mod worker {
    pub use corelink_rotation_worker::*;
}
