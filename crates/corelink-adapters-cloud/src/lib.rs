//! `corelink-adapters-cloud` — canonical cloud-adapters surface for
//! the CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream C sub-step C.3 lands this crate as the
//! **single import target** for every cloud-binding portion (HTTPS /
//! CF Worker JsValue) that previously lived across 5 separate
//! adapter crates:
//!
//! ```text
//! use corelink_adapters_cloud::cf::*;          // CF R2/D1/KV/DO bindings
//! use corelink_adapters_cloud::clerk::*;       // Clerk JWKS on CF KV / Fetch
//! use corelink_adapters_cloud::stripe::*;      // Stripe HTTPS Checkout / webhooks
//! use corelink_adapters_cloud::statuspage::*;  // Atlassian Statuspage HTTPS POST
//! use corelink_adapters_cloud::slack::*;       // Slack incoming-webhook HTTPS POST
//! ```
//!
//! ## Stage 1 Stream C absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream C and the Stage 0 SEAL audit §4 (Option-A aggregator
//! interpretation), this crate "absorbs" the **binding portions** of 5
//! existing adapter crates by re-exporting them at canonical submodule
//! paths. The pure-logic portions of those crates are simultaneously
//! re-exported by sibling umbrellas:
//!
//! - `corelink-stripe-real` pure-logic → `corelink_billing::stripe`
//!   (Stream B sub-step B.1).
//! - `corelink-statuspage-real` pure-logic → `corelink_ops::statuspage`
//!   (Stream C sub-step C.2).
//! - `corelink-slack-real` pure-logic → `corelink_ops::slack`
//!   (Stream C sub-step C.2).
//! - `corelink-clerk-cf` pure-logic → `corelink_auth::clerk_cf`
//!   (Stream B sub-step B.3).
//!
//! The **physical decomposition** between pure-logic and HTTPS /
//! CF-binding portions is **deferred to Stage 2** (the consumer-
//! migration stream that owns the `apps/server` route layer + the CF
//! Worker entry-point atomically). Stage 1 lands the canonical import
//! surface so consumer migration can proceed incrementally; the
//! absorbed crates remain the canonical sources of truth.
//!
//! ### Absorbed crates — binding portion (5)
//!
//! - [`cf`] ← `corelink-cf-bindings` — CF Worker R2 / D1 / KV / DO
//!   binding adapters (wasm32 only; native build produces empty rlib
//!   via per-module `#[cfg(target_arch = "wasm32")]` gates).
//! - [`clerk`] ← `corelink-clerk-cf` — Clerk JWKS cache on CF KV +
//!   JwksFetcher on CF Fetch + GET /health proof-of-concept handler.
//!   Compiles native (pure-logic) + wasm32 (`#[durable_object]` actor
//!   class gated wasm32-only).
//! - [`stripe`] ← `corelink-stripe-real` — HTTPS Stripe client
//!   (Checkout / Subscriptions / Customers / Billing Portal + webhook
//!   HMAC-SHA256 verify + idempotent POST retries with exponential
//!   backoff + Retry-After). Native-only.
//! - [`statuspage`] ← `corelink-statuspage-real` — Atlassian Statuspage
//!   HTTPS client (`reqwest::blocking` POST + 1-per-5-min rate-limiter
//!   + 3-retry exp-backoff + fail-CLOSED audit envelope).
//! - [`slack`] ← `corelink-slack-real` — Slack incoming-webhook real
//!   client (Block Kit + per-channel routing + retry + audit envelope).
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 5 absorbed adapter crates remains
//! reachable at its original path AND at the new canonical path. No
//! public-API contract is broken. The wasm32-only types remain
//! wasm32-only (per-module `#[cfg(target_arch = "wasm32")]` gates in
//! the absorbed crates flow through the re-exports unchanged).
//!
//! ## Charter compliance (preserved by reference)
//!
//! - `SecretString` on every credential bytes (Stripe secret key,
//!   Statuspage page_id, Slack webhook URL, CF API token, Clerk JWKS
//!   shared secret) — preserved by reference.
//! - `subtle::ConstantTimeEq` on Stripe webhook HMAC compare —
//!   preserved by reference.
//! - Audit-emit-BEFORE-mutation fail-CLOSED envelopes across every
//!   absorbed adapter — preserved by reference.
//! - wasm32-only types remain wasm32-only — preserved by re-export
//!   propagation.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cf;
pub mod clerk;
pub mod slack;
pub mod statuspage;
pub mod stripe;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new state is introduced.

    #[test]
    fn cf_path_resolves() {
        #[allow(unused_imports)]
        use crate::cf as _c;
    }

    #[test]
    fn clerk_path_resolves() {
        #[allow(unused_imports)]
        use crate::clerk as _ck;
    }

    #[test]
    fn stripe_path_resolves() {
        #[allow(unused_imports)]
        use crate::stripe as _s;
    }

    #[test]
    fn statuspage_path_resolves() {
        #[allow(unused_imports)]
        use crate::statuspage as _sp;
    }

    #[test]
    fn slack_path_resolves() {
        #[allow(unused_imports)]
        use crate::slack as _sl;
    }
}
