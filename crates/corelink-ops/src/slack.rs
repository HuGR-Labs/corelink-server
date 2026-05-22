//! Slack incoming-webhook real client — wave-33 canonical ops surface.
//!
//! Re-exports the entire public API of `corelink-slack-real`. The
//! actual implementation lives in `crates/corelink-slack-real/`
//! (Stage 1 Stream C sub-step C.2 Option-A aggregator pattern).
//!
//! **Note:** the HTTPS portion of `corelink-slack-real` is ALSO
//! re-exported by `corelink-adapters-cloud::slack` (C.3). The
//! pure-logic vs. binding decomposition is deferred to Stage 2.

pub use corelink_slack_real::*;
