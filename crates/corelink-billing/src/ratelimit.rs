//! Token-bucket + circuit-breaker rate limiting — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-ratelimit`. The
//! actual implementation lives in `crates/corelink-ratelimit/`
//! (Stage 1 Stream B sub-step B.1 Option-A aggregator pattern).

pub use corelink_ratelimit::*;
