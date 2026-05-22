//! Atlassian Statuspage real client — wave-33 canonical ops surface.
//!
//! Re-exports the entire public API of `corelink-statuspage-real`.
//! The actual implementation lives in `crates/corelink-statuspage-real/`
//! (Stage 1 Stream C sub-step C.2 Option-A aggregator pattern).
//!
//! **Note:** the HTTPS portion of `corelink-statuspage-real` is ALSO
//! re-exported by `corelink-adapters-cloud::statuspage` (C.3). The
//! pure-logic vs. binding decomposition is deferred to Stage 2.

pub use corelink_statuspage_real::*;
