//! Hourly billing aggregation — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-billing-aggregator`.
//! The actual implementation lives in `crates/corelink-billing-aggregator/`
//! (Stage 1 Stream B sub-step B.1 Option-A aggregator pattern; see
//! crate-level rustdoc).

pub use corelink_billing_aggregator::*;
