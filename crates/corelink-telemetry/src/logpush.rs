//! Cloudflare Logpush sink primitives — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-logpush`. The actual
//! implementation lives in `crates/corelink-logpush/` (Stage 0
//! sub-step 3 Option-A aggregator pattern; see crate-level rustdoc).

pub use corelink_logpush::*;

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::logpush"
}
