//! Audit analytics — wave-33 canonical re-export.
//!
//! Re-exports the entire public API of `corelink-analytics` (audit
//! analytics surface: time-series + aggregates + filters). The
//! actual implementation lives in `crates/corelink-analytics/`
//! (Stage 0 sub-step 4 Option-A aggregator pattern; see crate-level
//! rustdoc and `specs/_audits/sealed/2026-05-22-w33-stage0-foundation.md`
//! §4).

pub use corelink_analytics::*;

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_audit::analytics"
}
