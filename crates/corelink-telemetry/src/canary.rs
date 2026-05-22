//! Canary rollout signal primitives — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-canary` (canary
//! decision arms + health components). The actual implementation lives
//! in `crates/corelink-canary/` (Stage 0 sub-step 3 Option-A
//! aggregator pattern; see crate-level rustdoc).

pub use corelink_canary::*;

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::canary"
}
