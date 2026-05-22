//! Atlassian Statuspage HTTPS POST — wave-33 canonical cloud-adapter
//! surface.
//!
//! Re-exports the entire public API of `corelink-statuspage-real` —
//! the `StatuspageBackend` trait, the `DsrCompletionReport`
//! 24h-rolling payload, the `reqwest::blocking` POST client, the
//! 1-per-5-min rate-limiter, the 3-retry exponential-backoff loop,
//! and the fail-CLOSED audit envelope. The actual implementation
//! lives in `crates/corelink-statuspage-real/` (Stage 1 Stream C
//! sub-step C.3 Option-A aggregator pattern).
//!
//! **Note:** the pure-logic portion of `corelink-statuspage-real` is
//! ALSO re-exported by `corelink-ops::statuspage` (C.2). The
//! decomposition between pure-logic and HTTPS portions is deferred to
//! Stage 2.

pub use corelink_statuspage_real::*;
