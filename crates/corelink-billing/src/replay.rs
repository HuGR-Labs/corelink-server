//! Webhook replay + dead-letter harness — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-billing-replay`. The
//! actual implementation lives in `crates/corelink-billing-replay/`
//! (Stage 1 Stream B sub-step B.1 Option-A aggregator pattern).

pub use corelink_billing_replay::*;
