//! Chaos scheduler — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-chaos-scheduler`
//! (WI-S17-001: 8 experiment types + safe-mode auto-abort +
//! deterministic seed + 7y archive; foundation for WI-S17-002..006).
//! The actual implementation lives in
//! `crates/corelink-chaos-scheduler/` (Stage 1 Stream C sub-step C.2
//! Option-A aggregator pattern).

pub use corelink_chaos_scheduler::*;
