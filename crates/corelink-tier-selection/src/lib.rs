//! `corelink-tier-selection` — tier selection orchestrator + Stripe
//! Checkout activation + INV-ONBOARD-DPA-FIRST D1 row lock
//! (WI-S19-004).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the tier-selection orchestrator + Stripe Checkout
//! Session creation trait surface every production HTTPS Stripe API
//! call + Cloudflare Worker route registration (`POST
//! /v1/onboarding/tier-select` + `POST /v1/billing/stripe-webhook`)
//! will satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on. Property
//! tests pinned at 10k iter cover INV-ONBOARD-DPA-FIRST (HIGH;
//! `invariant_registry.md §3.12`; tier selection ALWAYS fails when
//! DPA not accepted, regardless of tier or concurrency), D1 row-lock
//! pessimistic semantics (`INSERT OR IGNORE` on
//! `tier_selection_locks` with 60s window; concurrent callers get
//! `TierError::LockHeld`), Stripe webhook idempotency + signature
//! replay rejection, and audit-emit-BEFORE-mutation fail-CLOSED.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`tier`] module ships [`TierKind`] `#[non_exhaustive]`
//!    5-canonical (`Free` / `Starter` / `Team` / `Pro` /
//!    `Enterprise` per WI §6.1 + spec contract §5.3 R-S19-7) +
//!    [`tier::canonical_tiers`] surface-stability list.
//! 2. The [`tenant`] module ships [`TenantId`], [`StripeCustomerId`]
//!    newtypes + [`TenantCtx`] handler payload.
//! 3. The [`dpa`] module ships [`DpaAcceptanceGate`] trait +
//!    [`InMemoryDpaGate`] fake + [`AlwaysDenyDpaGate`] adversarial
//!    fixture. Fail-CLOSED on transient errors (mutex poisoning /
//!    storage failure → deny).
//! 4. The [`stripe`] module ships [`StripeClient`] trait +
//!    [`InMemoryStripeClient`] fake + [`compute_stripe_signature`] +
//!    [`verify_stripe_signature`] (HMAC-SHA256 + canonical 5-min
//!    replay window per Stripe spec; constant-time compare via
//!    `subtle::ConstantTimeEq`).
//! 5. The [`audit`] module ships [`TierSelectionAuditEventType`]
//!    `#[non_exhaustive]` 8-canonical taxonomy + audit sink trait +
//!    in-memory + failing fixtures.
//! 6. The [`ledger`] module ships [`TierSelectionLedger`] orchestrator
//!    (in-process mirror of D1 tables `tier_selections` +
//!    `tier_selection_locks` + `stripe_checkout_sessions` + idempotency
//!    table; `Arc<Mutex<>>` per-instance F-001 closure; audit
//!    fail-CLOSED envelope BEFORE state mutation) +
//!    [`TierSelectionReceipt`] / [`SubscriptionActivationReceipt`] +
//!    [`TIER_SELECTION_LOCK_WINDOW_MS`] (60_000 ms canonical).
//! 7. The [`error`] module ships [`TierError`] `#[non_exhaustive]`
//!    taxonomy (`DpaRequired` / `UseInquiryForm` / `LockHeld` /
//!    `AlreadyActive` / `Stripe` / `InvalidSignature` /
//!    `DuplicateEvent` / `Audit` / `Internal`).
//!
//! # Invariants enforced (pinned by property tests)
//!
//! - **INV-ONBOARD-DPA-FIRST** (HIGH; registry §3.12): subscription
//!   activation requires DPA accepted FIRST. `select_tier()` calls
//!   [`dpa::DpaAcceptanceGate::is_accepted`] BEFORE any Stripe API
//!   call; on `false` returns [`error::TierError::DpaRequired`] +
//!   emits the violation audit event. Applies to ALL tiers including
//!   Free (WI §6.5; no exception). Pinned by
//!   `prop_dpa_first_never_bypassed_10k`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; Lote 10.6bis +
//!   S-07 P1-1 fix): audit envelope BEFORE state mutation on every
//!   decision arm; audit failure aborts. Pinned by
//!   `prop_audit_emit_before_mutation`.
//! - **D1 row lock**: `INSERT OR IGNORE` on `tier_selection_locks`
//!   with 60s `expires_ms` window prevents concurrent tier switches
//!   per tenant. Pinned by `prop_d1_lock_prevents_concurrent`.
//! - **Stripe webhook idempotency**: `event_id` dedup; double delivery
//!   yields [`ledger::SubscriptionActivationReceipt::DuplicateIgnored`].
//!   Pinned by `prop_webhook_idempotency`.
//! - **Enterprise route enforcement**: direct Stripe Checkout for
//!   enterprise tier rejected with [`error::TierError::UseInquiryForm`].
//!   Pinned by `prop_enterprise_route_enforcement`.
//!
//! # Production wiring (deferred to PRR ship gate)
//!
//! - Real HTTPS Stripe API client (reqwest / cf-worker fetch) +
//!   real Stripe API endpoints
//!   (`POST /v1/checkout/sessions` + webhook delivery).
//! - Cloudflare Worker routes
//!   (`POST /v1/onboarding/tier-select` + `POST /v1/billing/stripe-webhook`).
//! - D1 binding with `BEGIN IMMEDIATE TRANSACTION` (Lote 10.19 codex
//!   P0 canonical fix; SQLite/D1 does NOT support `SELECT ... FOR UPDATE`,
//!   use `BEGIN IMMEDIATE` + UNIQUE partial index defense-in-depth).
//! - Migration `migrations/d1/0039_tier_selection.sql` apply in
//!   production D1 schema.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod dpa;
pub mod error;
pub mod ledger;
pub mod stripe;
pub mod tenant;
pub mod tier;

pub use audit::{
    canonical_tier_selection_audit_event_strings, FailingTierSelectionAuditSink,
    InMemoryTierSelectionAuditSink, TierSelectionAuditEmitError, TierSelectionAuditEventType,
    TierSelectionAuditRecord, TierSelectionAuditSink,
};
pub use dpa::{AlwaysDenyDpaGate, DpaAcceptanceGate, InMemoryDpaGate};
pub use error::TierError;
pub use ledger::{
    CheckoutSessionRow, SubscriptionActivationReceipt, SubscriptionState, TierSelectionLedger,
    TierSelectionReceipt, TierSelectionRow, TIER_SELECTION_LOCK_WINDOW_MS,
};
pub use stripe::{
    compute_stripe_signature, parse_stripe_signature_header, verify_stripe_signature,
    CheckoutSessionRequest, CheckoutSessionResponse, InMemoryStripeClient, StripeCheckoutSessionCompletedEvent,
    StripeClient, STRIPE_REPLAY_WINDOW_MS,
};
pub use tenant::{StripeCustomerId, TenantCtx, TenantId};
pub use tier::{canonical_tiers, TierKind};

/// Crate canonical schema version constant.
#[must_use]
pub const fn tier_selection_schema_version() -> u32 {
    1
}
