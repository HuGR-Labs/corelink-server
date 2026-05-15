//! `e2e-billing-flow` — R3-5 end-to-end billing-flow harness.
//!
//! # Pipeline
//!
//! Per the corelink autonomous execution charter R3 phase scope, this
//! crate is **NOT a production component** — it is a test-only harness
//! that composes the WI-S19 signup orchestrator, the WI-S07 tier
//! selection ledger, the WI-S10-003 billing-stripe webhook handler,
//! and the WI-S11-001 DSR endpoint behind a single [`BillingHarness`]
//! fixture so the full customer billing journey
//!
//! ```text
//! signup → tier_select(Starter)
//!   → Stripe Checkout Session
//!     → checkout.session.completed webhook → subscription activation
//!       → cancel-while-active (subscription.updated → cancelled)
//!         → refund webhook
//!           → DSR Erasure (blocked while subscription is active)
//! ```
//!
//! runs in a single `cargo test -p e2e-billing-flow` without any
//! network IO. Live-mode Stripe paths are out of scope for this crate
//! — see `e2e-signup-flow` for the `STRIPE_SECRET_KEY_TEST`-gated
//! live-checkout variant.
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` at the crate root.
//! - No `unwrap()` / `expect()` / `panic!()` in non-test code; the
//!   helper surface returns typed [`BillingHarnessError`] on every
//!   failure arm.
//! - Audit-emit ordering is fail-CLOSED at every state mutation
//!   (delegated to the composed ledger ledgers — every primitive used
//!   here already audits BEFORE the mutation per ADR-S10-001 +
//!   ADR-S11-002).
//! - Webhook HMAC verify uses the canonical
//!   `corelink_billing_stripe::verify_stripe_signature` primitive
//!   which is built on `subtle::ConstantTimeEq` (constant-time
//!   comparison — no timing oracle).
//! - All identifiers (tenant slug, Stripe event ids, DSR request id,
//!   data subject id) are deterministic functions of the test-tenant
//!   name slug so the harness is byte-reproducible across runs.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod harness;

pub use harness::{
    audit_chain_contains, build_signed_webhook, derive_data_subject_id, derive_request_id,
    derive_tenant_uuid, make_test_tenant, BillingError, BillingHarness, BillingHarnessError,
    BillingTenant, HarnessGateEvent, SignedWebhook, SubscriptionLifecycleState, FIXED_NOW_MS,
    FIXED_NOW_SECONDS, TEST_DPA_VERSION, TEST_WEBHOOK_SECRET,
};
