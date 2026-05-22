//! Slack incoming-webhook HTTPS POST — wave-33 canonical cloud-adapter
//! surface.
//!
//! Re-exports the entire public API of `corelink-slack-real` — Block
//! Kit message formatting, per-channel routing, retry, and the
//! fail-CLOSED audit envelope. The actual implementation lives in
//! `crates/corelink-slack-real/` (Stage 1 Stream C sub-step C.3
//! Option-A aggregator pattern).
//!
//! **Note:** the pure-logic portion of `corelink-slack-real` is ALSO
//! re-exported by `corelink-ops::slack` (C.2). The decomposition
//! between pure-logic and HTTPS portions is deferred to Stage 2.

pub use corelink_slack_real::*;
