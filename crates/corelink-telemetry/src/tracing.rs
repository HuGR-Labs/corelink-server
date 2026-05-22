//! Structured tracing primitives — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-tracing` (structured
//! `TenantContext`, span field helpers, redaction filters). The actual
//! implementation lives in `crates/corelink-tracing/` (Stage 0
//! sub-step 3 Option-A aggregator pattern; see crate-level rustdoc).

pub use corelink_tracing::*;

/// Module-path marker used by the crate-level smoke tests. Returns
/// the canonical wave-33 import path for this submodule.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::tracing"
}
