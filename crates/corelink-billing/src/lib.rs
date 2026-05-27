//! `corelink-billing` — canonical billing-context surface for the
//! CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream B sub-step B.1 lands this crate as the
//! **single import target** for every billing primitive that
//! previously lived across 14 separate crates:
//!
//! ```text
//! use corelink_billing::aggregator::*;          // hourly aggregation
//! use corelink_billing::emit::*;                // usage-event emit
//! use corelink_billing::reconcile::*;           // Stripe ↔ D1 reconcile
//! use corelink_billing::replay::*;              // webhook replay harness
//! use corelink_billing::stripe::*;              // Stripe schema + real HTTPS
//! use corelink_billing::stripe_materializer::*; // D1 state materializer
//! use corelink_billing::tier::*;                // tier-selection orchestrator
//! use corelink_billing::quota::*;               // quota trait + CAS + FSM
//! use corelink_billing::rate_headers::*;        // RFC 6585 retry-after headers
//! use corelink_billing::ratelimit::*;           // token-bucket + circuit-breaker
//! use corelink_billing::abuse::*;               // abuse classifier + reputation
//! ```
//!
//! ## Stage 1 Stream B absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream B and the Stage 0 SEAL audit §4 (Option-A aggregator
//! interpretation), this crate "absorbs" 14 existing crates by
//! re-exporting them at canonical submodule paths. The absorbed
//! crates remain the canonical sources of truth — their src/, tests/,
//! benches/, and fuzz/ harnesses are unchanged. Consumer migration
//! (apps/server routes, worker bindings) proceeds incrementally.
//!
//! ### Absorbed crates (14)
//!
//! - `corelink-billing-aggregator` — hourly usage aggregation +
//!   monthly billing rows; re-exported at [`aggregator`].
//! - `corelink-billing-emit` — usage-event emit with idempotency +
//!   audit envelope; re-exported at [`emit`].
//! - `corelink-billing-reconcile` — Stripe ↔ D1 reconcile harness;
//!   re-exported at [`reconcile`].
//! - `corelink-billing-replay` — webhook replay + dead-letter
//!   harness; re-exported at [`replay`].
//! - `corelink-billing-stripe` — Stripe API schema + signature
//!   verify; re-exported at [`stripe::schema`].
//! - `corelink-stripe-real` — real HTTPS Stripe client (Checkout /
//!   Subscriptions / Customers / Billing Portal + webhook dispatcher
//!   + wave-31 wallet broker dual-mode); re-exported at [`stripe::real`].
//! - `corelink-billing-stripe-materializer` — D1 state materializer
//!   that writes Stripe-derived rows; re-exported at
//!   [`stripe_materializer`].
//! - `corelink-tier-selection` — tier-selection orchestrator + DPA
//!   gate + Stripe Checkout integration; re-exported at [`tier`].
//! - `corelink-quota` + `corelink-quota-cas` + `corelink-quota-fsm` —
//!   quota traits + CAS write-time enforcement + soft/hard quota
//!   state machine; re-exported under [`quota`] as three submodules
//!   (`quota::core`, `quota::cas`, `quota::fsm`).
//! - `corelink-rate-headers` — RFC 6585 + RateLimit IETF draft
//!   response headers; re-exported at [`rate_headers`].
//! - `corelink-ratelimit` — token-bucket + leaky-bucket + 3-state
//!   circuit-breaker (Closed/Open/HalfOpen) + back-pressure queue;
//!   re-exported at [`ratelimit`].
//! - `corelink-abuse` — abuse classifier + tenant reputation +
//!   anomaly detection; re-exported at [`abuse`].
//!
//! ### Why aggregator rather than physical move
//!
//! 1. **Behaviour preservation** — `corelink-stripe-real` carries the
//!    wave-31 wallet broker dual-mode (commit `e3c564df` in main)
//!    that production-wires Stripe credentials through the HuGR
//!    Wallet broker. Physically relocating that module without an
//!    atomic consumer migration risks regressing the dual-mode
//!    fallback path (charter Hard Pause Trigger 7).
//! 2. **`corelink-billing-stripe-materializer` D1 writer trait
//!    surface** is consumed by `apps/server/src/routes/stripe_webhook.rs`
//!    via `path = "../../crates/..."` references; a physical move
//!    requires atomic updates to the apps/server route layer that
//!    belongs to Stage 2 binding consolidation.
//! 3. **Quota CAS write-time enforcement** is consumed by
//!    `corelink-handler-cas::AuditSink` callers (Stream A territory);
//!    coordinated absorption with Stream A's CAS-path work avoids
//!    cross-stream serialization.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 14 absorbed crates remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken. Stage 2 / Stage 3 consumers may adopt the new
//! canonical paths incrementally without coordination cost.
//!
//! ## What this crate does NOT do
//!
//! - Define new types. Every type, trait, and constant surfaced is a
//!   re-export of an absorbed crate.
//! - Introduce async or HTTPS dependencies directly. The transitive
//!   `corelink-stripe-real` re-export pulls `reqwest` only when its
//!   own `native` feature is active; the aggregator itself has zero
//!   direct async/HTTPS deps. Stage 3 cargo-deny lockdown will weigh
//!   whether to enforce at direct or transitive level (flagged in
//!   Stage 0 SEAL audit §7).
//! - Migrate audit emit sites. The wave-33 `AuditEmitter` chokepoint
//!   trait (`corelink_audit::ports::AuditEmitter`) is available for
//!   billing-context adoption; Stream B's Stage 1 commits do NOT
//!   migrate any existing emit site to preserve the "behaviour-
//!   preserving refactor ONLY" charter rule. AuditEmitter adoption
//!   is tracked for Stage 2 / Stage 3 follow-on.
//!
//! ## Hard pause triggers honoured (charter §7 wave-33 spec §7)
//!
//! - Trigger 7 (Stripe wallet broker dual-mode regression): NOT
//!   ACTIVATED — `corelink-stripe-real` is re-exported as-is; the
//!   wave-31 dual-mode logic is preserved by reference.
//! - Trigger 4 (Audit fail-CLOSED softened): NOT ACTIVATED — no
//!   audit emit site is touched.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod abuse;
pub mod aggregator;
pub mod emit;
pub mod quota;
pub mod rate_headers;
pub mod ratelimit;
pub mod reconcile;
pub mod replay;
pub mod stripe;
pub mod stripe_materializer;
pub mod tier;

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
    fn aggregator_path_resolves() {
        // Surface a type id from the re-exported module to prove the
        // canonical aggregator path resolves at compile time.
        let _ = std::any::TypeId::of::<crate::aggregator::InMemoryAggregatedCounterStore>();
    }

    #[test]
    fn stripe_real_dual_mode_preserved() {
        // The wave-31 wallet broker dual-mode lives in
        // `corelink-stripe-real::StripeAuthMode` (Direct /
        // WalletBroker). Re-exporting at `stripe::real` preserves the
        // public surface without any behaviour change. This smoke
        // test confirms the dual-mode marker enum is reachable
        // through the canonical wave-33 path (charter Hard Pause
        // Trigger 7 — Stripe wallet broker dual-mode regression
        // tripwire).
        let _ = std::any::TypeId::of::<crate::stripe::real::StripeAuthMode>();
    }

    #[test]
    fn quota_three_submodules_resolve() {
        // `corelink-quota` + `corelink-quota-cas` + `corelink-quota-fsm`
        // are folded into the `quota` submodule as three sub-submodules
        // (`core`, `cas`, `fsm`). Smoke: every sub-submodule resolves.
        // We rely on the fact that `pub use foo::*` re-exports at least
        // one item from each absorbed crate; if any absorbed crate is
        // empty the workspace-wide build catches it via `dead_code`
        // lints. Here we simply confirm module-path traversal compiles.
        #[allow(unused_imports)]
        use crate::quota::{cas as _cas, core as _core, fsm as _fsm};
    }
}
