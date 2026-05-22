//! Drift Tracker (DT) — wave-33 canonical ops aggregator.
//!
//! Three Drift-Tracker context crates absorbed under the canonical
//! `dt` submodule (Stage 1 Stream C sub-step C.2 Option-A aggregator
//! pattern):
//!
//! - **`corelink-dt-cli`** — Drift Tracker CLI. This is a binary-only
//!   crate (`src/main.rs`, no `src/lib.rs`); it cannot be re-exported
//!   into the umbrella's library surface. It remains a workspace
//!   binary target consumed via `cargo run --bin corelink-dt-cli`.
//!   Documented here for the canonical-import audit trail.
//! - **`corelink-dt-reconcile`** — Drift Tracker reconciler. Same
//!   shape as `corelink-dt-cli`: binary-only; documented here, not
//!   re-exported.
//! - [`webhook`] — `corelink-dt-webhook`: Drift Tracker webhook
//!   receiver (library; re-exported below).

/// Drift Tracker webhook receiver (HMAC verify + DLQ + severity
/// gating).
///
/// Re-exports the entire public API of `corelink-dt-webhook`.
pub mod webhook {
    pub use corelink_dt_webhook::*;
}
