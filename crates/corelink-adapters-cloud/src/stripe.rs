//! Stripe HTTPS Checkout / webhooks — wave-33 canonical cloud-adapter
//! surface.
//!
//! Re-exports the entire public API of `corelink-stripe-real` — the
//! HTTPS Stripe client covering Checkout, Subscriptions, Customers,
//! and Billing Portal, the webhook HMAC-SHA256 verification path, the
//! idempotent POST retry loop, and the exponential-backoff +
//! Retry-After handling. Native-only (wasm32 is gated out inside the
//! absorbed crate — production callers run from `apps/server`, not
//! the CF Worker). The actual implementation lives in
//! `crates/corelink-stripe-real/` (Stage 1 Stream C sub-step C.3
//! Option-A aggregator pattern).

pub use corelink_stripe_real::*;
