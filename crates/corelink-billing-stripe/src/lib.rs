//! `corelink-billing-stripe` — Stripe billing adapter +
//! `Idempotency-Key` derivation + HMAC-SHA256 webhook signature
//! verification (WI-S10-003).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the Stripe adapter primitive: trait surfaces every
//! production wiring (real Stripe HTTP client + Cloudflare Worker route
//! at `POST /v1/webhooks/stripe` + `reqwest`-based usage record
//! POSTs to `/v1/subscription_items/{id}/usage_records` + Worker secret
//! `STRIPE_WEBHOOK_SECRET` / `STRIPE_TEST_SECRET_KEY` /
//! `STRIPE_LIVE_SECRET_KEY` provisioning) will satisfy, plus an
//! in-memory orchestrator that exercises every load-bearing invariant
//! the production wiring relies on. Property tests pinned at 10k iter
//! against the orchestrator cover INV-BILLING-NO-DUP at the Stripe
//! adapter layer (HIGH; `invariant_registry.md §3.9 line 137`; same
//! aggregate → same canonical `Idempotency-Key` → single Stripe charge
//! regardless of retry storms), webhook signature integrity (HMAC-SHA256
//! verify + 5-min canonical replay window per Stripe spec; constant-
//! time compare via `subtle::ConstantTimeEq`), and INV-AUDIT-EMIT-
//! ATOMIC-WITH-HANDLER (HIGH; audit envelope BEFORE state mutation on
//! every decision arm).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`IdempotencyKey`] (32-byte BLAKE3-256
//!    newtype; hex-rendered on the wire), [`SubscriptionItemId`]
//!    newtype, [`UsageRecordRequest`] typed payload,
//!    [`StripeAdapterDecision`] `#[non_exhaustive]` 4-element taxonomy
//!    (UsageRecorded / DuplicateRejected / WebhookProcessed /
//!    SignatureRejected), [`WebhookEventKind`] `#[non_exhaustive]`
//!    5-element taxonomy (InvoiceCreated / InvoicePaid / InvoiceFailed
//!    / SubscriptionUpdated / CustomerCreated), and [`WebhookEvent`].
//! 2. The [`idempotency`] module ships
//!    [`compute_canonical_aggregate_bytes`] +
//!    [`derive_idempotency_key`] +
//!    [`derive_idempotency_key_from_canonical`] — the canonical
//!    `Idempotency-Key = BLAKE3-256(JCS(aggregate))` derivation per
//!    WI-S10-003 §6.1 + sprint contract §5.3 R-S10-6.
//! 3. The [`signature`] module ships [`StripeSignatureHeader::parse`] +
//!    [`compute_signature`] + [`verify_stripe_signature`] —
//!    canonical Stripe `Stripe-Signature: t=<ts>,v1=<hex>` HMAC-SHA256
//!    verification + canonical 5-min replay window (300_000 ms) per
//!    Stripe spec; constant-time signature compare via
//!    [`subtle::ConstantTimeEq`] (`subtle` is the canonical Rust
//!    constant-time primitive).
//! 4. The [`audit`] module ships [`StripeAuditEventType`]
//!    `#[non_exhaustive]` 6-event taxonomy:
//!    `corelink.billing_stripe.{usage_recorded, duplicate_rejected,
//!    webhook_received, signature_rejected, signature_verified,
//!    signature_skew_rejected}` + [`StripeAuditRecord`] +
//!    [`StripeAuditSink`] trait + [`InMemoryStripeAuditSink`] capture
//!    sink + [`FailingStripeAuditSink`] (fail-CLOSED envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` from S-07 P1-1 lift +
//!    Lote 10.6bis pattern + S-09 inheritance).
//! 5. The [`ledger`] module ships [`StripeUsageLedger`] trait +
//!    [`InMemoryStripeUsageLedger`] (per-`IdempotencyKey` UNIQUE PK
//!    enforcement; INV-BILLING-NO-DUP at the Stripe adapter layer) +
//!    [`RecordOutcome`] (Recorded / AlreadyExistsIdempotent) +
//!    [`FailingStripeUsageLedger`].
//! 6. The [`webhook_log`] module ships [`StripeWebhookLog`] trait +
//!    [`InMemoryStripeWebhookLog`] (`stripe_event_id` UNIQUE PK; webhook
//!    redelivery dedup per Stripe spec) +
//!    [`WebhookInsertOutcome`] (Inserted / AlreadyExists) +
//!    [`FailingStripeWebhookLog`].
//! 7. The [`adapter`] module ships [`StripeBillingAdapter`] trait +
//!    [`InMemoryStripeBillingAdapter`] orchestrator (record pipeline:
//!    canonicalize → derive idempotency_key → audit BEFORE ledger
//!    mutation → ledger record).
//! 8. The [`webhook`] module ships [`StripeWebhookHandler`] trait +
//!    [`InMemoryStripeWebhookHandler`] orchestrator (dispatch pipeline:
//!    audit `webhook_received` → signature verify → audit
//!    `signature_verified` / `signature_rejected` /
//!    `signature_skew_rejected` → webhook log INSERT).
//! 9. The [`error`] module ships the canonical [`StripeError`]
//!    `#[non_exhaustive]` taxonomy (Canonicalization / Audit / Ledger /
//!    WebhookLog / SignatureRejected / SignatureSkewRejected /
//!    Internal) + [`StripeAuditSinkError`] +
//!    [`StripeUsageLedgerError`] + [`StripeWebhookLogError`].
//!
//! # Why `trait + fake` here, real HTTP client + CF Worker in WI-S10-007
//!
//! S-10 lands without Stripe API credentials wired into CI (no remote +
//! Stripe / Clerk credentials are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! `Idempotency-Key` determinism over the canonical aggregate bytes;
//! HMAC-SHA256 correctness; constant-time signature compare via
//! `subtle::ConstantTimeEq`; canonical 5-min replay window enforcement;
//! webhook event-id dedup; tenant isolation; audit-fail-CLOSED envelope
//! on every decision arm. The live Stripe HTTP POST to
//! `/v1/subscription_items/{id}/usage_records` + Worker route
//! registration at `POST /v1/webhooks/stripe` + Worker secret env
//! separation `CORELINK_STRIPE_MODE=test|live` + production rate-limit
//! (PAT-BACKOFF-001) + queue fallback (PAT-QUEUE-EVENTS-001) + RB-FM-151
//! (Stripe outage) dry-run all run alongside WI-S10-007 (PRR ship
//! gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-BILLING-NO-DUP** (HIGH; `invariant_registry.md §3.9 line
//!   137`): the canonical `Idempotency-Key = BLAKE3-256(JCS(aggregate))`
//!   is deterministic — same aggregate reproduces the same key; the
//!   Stripe API (and the in-memory ledger fake) returns the SAME usage
//!   record on the second sight (`RecordOutcome::AlreadyExistsIdempotent`).
//!   Pinned by `prop_idempotency_key_deterministic` +
//!   `prop_idempotency_key_diverges_per_aggregate`.
//! - **Webhook signature integrity** (HMAC-SHA256 + 5-min replay
//!   window): the signature primitive verifies `HMAC-SHA256(secret,
//!   "<t>.<payload>")` constant-time against EVERY `v1=` candidate
//!   field (key rotation tolerance) + rejects `(now - t) > 300s` per
//!   Stripe spec. Pinned by `prop_webhook_signature_verifies_valid` +
//!   `prop_webhook_signature_rejects_tampered_payload` +
//!   `prop_webhook_signature_rejects_tampered_signature` +
//!   `prop_webhook_signature_rejects_expired` +
//!   `prop_replay_window_exact_5min_boundary`.
//! - **Constant-time signature compare** (timing-attack defense): the
//!   verifier uses `subtle::ConstantTimeEq::ct_eq` (NOT `==`); the
//!   `subtle` crate is the canonical Rust primitive for constant-time
//!   bytewise comparison. The `prop_constant_time_signature_compare`
//!   property test verifies the API surface; an adversarial
//!   timing-attack microbenchmark is informational only (the canonical
//!   defense is the API call itself).
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07 P1-1
//!   fix + Lote 10.6bis pattern + S-09 inheritance): every adapter +
//!   webhook decision arm fires its canonical audit BEFORE state
//!   mutation; audit failure aborts the run + propagates as
//!   `StripeError::Audit`. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): per-tenant `Idempotency-Key` partitioning; tenant
//!   A's idempotency key never collides with tenant B's (the canonical
//!   aggregate bytes include `data.tenant_id` so distinct tenants
//!   produce distinct keys by construction). Pinned by
//!   `prop_tenant_isolation`.
//!
//! # Production wiring (deferred to WI-S10-007)
//!
//! - `HttpStripeBillingAdapter` — `reqwest::Client` POSTs to
//!   `/v1/subscription_items/{id}/usage_records` with the canonical
//!   `Idempotency-Key` HTTP header; production HTTP client respects
//!   PAT-BACKOFF-001 retry budget (sprint contract §14.s10.2: max 5
//!   retries, exponential 1s/2s/4s/8s/16s + jitter ±20%).
//! - `WorkerStripeWebhookHandler` — Cloudflare Worker `fetch` route at
//!   `POST /v1/webhooks/stripe`; reads canonical `Stripe-Signature`
//!   header + raw body bytes via `worker::Request`.
//! - `D1StripeUsageLedger` — D1 INSERT into Neon `invoice_line_item`
//!   with `(stripe_invoice_item_id) UNIQUE` (sprint contract §5.3
//!   R-S10-7).
//! - `D1StripeWebhookLog` — D1 INSERT into `stripe_event_log` with
//!   `(stripe_event_id) UNIQUE` (sprint contract §5.3 R-S10-7).
//! - Worker secret `STRIPE_WEBHOOK_SECRET` + `STRIPE_TEST_SECRET_KEY` /
//!   `STRIPE_LIVE_SECRET_KEY` env separation per
//!   `CORELINK_STRIPE_MODE=test|live` (sprint contract §5.3 R-S10-6).
//! - PAT-QUEUE-EVENTS-001 fallback Cloudflare Queue
//!   `stripe-pending-${region}` per region for FM-151 Stripe outage
//!   recovery; RB-FM-151 dry-run runbook prerequisite per sprint
//!   contract §6 DoD.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod adapter;
pub mod audit;
pub mod error;
pub mod event;
pub mod idempotency;
pub mod ledger;
pub mod signature;
pub mod webhook;
pub mod webhook_log;

pub use adapter::{InMemoryStripeBillingAdapter, StripeBillingAdapter};
pub use audit::{
    canonical_stripe_audit_event_strings, FailingStripeAuditSink, InMemoryStripeAuditSink,
    StripeAuditEmitError, StripeAuditEventType, StripeAuditRecord, StripeAuditSink,
};
pub use error::{
    StripeAuditSinkError, StripeError, StripeUsageLedgerError, StripeWebhookLogError,
};
pub use event::{
    IdempotencyKey, StripeAdapterDecision, SubscriptionItemId, UsageRecordRequest,
    WebhookEvent, WebhookEventKind,
};
pub use idempotency::{
    compute_canonical_aggregate_bytes, derive_idempotency_key,
    derive_idempotency_key_from_canonical,
};
pub use ledger::{
    FailingStripeUsageLedger, InMemoryStripeUsageLedger, RecordOutcome, StripeUsageLedger,
};
pub use signature::{
    compute_signature, verify_stripe_signature, StripeSignatureHeader, REPLAY_WINDOW_MS,
};
pub use webhook::{
    InMemoryStripeWebhookHandler, StripeWebhookHandler, WebhookHandleRequest,
};
pub use webhook_log::{
    FailingStripeWebhookLog, InMemoryStripeWebhookLog, StripeWebhookLog, WebhookInsertOutcome,
};

/// Crate canonical schema version constant. Mirrors WI-S10-003 §1
/// schema versioning policy + the canonical D1 migration slot for the
/// follow-on `stripe_event_log` + `stripe_idempotency_keys` tables
/// (next slot after `migrations/d1/0017_usage_event_idem.sql`;
/// production wiring at WI-S10-007 lands the additive 0019 production
/// migration alongside this WI's 0018 idempotency-keys staging table).
#[must_use]
pub const fn stripe_schema_version() -> u32 {
    18
}
