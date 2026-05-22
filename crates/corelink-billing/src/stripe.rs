//! Stripe API surface — wave-33 canonical surface.
//!
//! Combined re-export of two absorbed Stripe-related crates under the
//! canonical `stripe` submodule (Stage 1 Stream B sub-step B.1
//! Option-A aggregator pattern). The two sub-submodules are kept
//! separate so consumers can pick the schema-only surface OR the full
//! HTTPS real-client surface without pulling the wrong transitive
//! dependency tree:
//!
//! - [`schema`] — `corelink-billing-stripe`: Stripe API schema +
//!   webhook signature verify primitives.
//! - [`real`] — `corelink-stripe-real`: real HTTPS Stripe client
//!   (Checkout / Subscriptions / Customers / Billing Portal +
//!   webhook dispatcher). Carries the **wave-31 wallet broker
//!   dual-mode** (`StripeAuthMode::Direct` / `StripeAuthMode::WalletBroker`,
//!   commit `e3c564df` in main) which is preserved by reference —
//!   the wave-33 reorg does NOT touch the dual-mode implementation
//!   (charter Hard Pause Trigger 7).

/// Stripe API schema + webhook signature verify primitives.
///
/// Re-exports the entire public API of `corelink-billing-stripe`.
pub mod schema {
    pub use corelink_billing_stripe::*;
}

/// Real HTTPS Stripe client + wave-31 wallet broker dual-mode.
///
/// Re-exports the entire public API of `corelink-stripe-real`.
/// The wave-31 wallet broker dual-mode lives in
/// `corelink_stripe_real::StripeAuthMode`; preserved as-is.
pub mod real {
    pub use corelink_stripe_real::*;
}
