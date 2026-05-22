//! Synthetic-pager drill primitives — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-synthetic-pager`
//! (weekly 3-region follow-the-sun drill + MTTA budget; WI-S20-006).
//! The actual implementation lives in
//! `crates/corelink-synthetic-pager/` (Stage 0 sub-step 3 Option-A
//! aggregator pattern; see crate-level rustdoc).

pub use corelink_synthetic_pager::*;

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::synthetic_pager"
}
