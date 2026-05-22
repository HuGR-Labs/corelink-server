//! Lighthouse customer state-machine tracker — wave-33 canonical
//! surface.
//!
//! Re-exports the entire public API of `corelink-lighthouse-tracker`
//! (3-customer recruiting → engaged → migrating → observing →
//! attested → case-study-signed flow; WI-S20-004). The actual
//! implementation lives in `crates/corelink-lighthouse-tracker/`
//! (Stage 0 sub-step 3 Option-A aggregator pattern; see crate-level
//! rustdoc).

pub use corelink_lighthouse_tracker::*;

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::lighthouse"
}
