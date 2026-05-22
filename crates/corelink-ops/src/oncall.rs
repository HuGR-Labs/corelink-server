//! PagerDuty oncall scheduler + fatigue tracking — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-oncall` (WI-S17-005).
//! The actual implementation lives in `crates/corelink-oncall/` (Stage 1
//! Stream C sub-step C.2 Option-A aggregator pattern).

pub use corelink_oncall::*;
