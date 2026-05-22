//! `corelink-signup` — signup orchestration backend (WI-S19-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! this crate ships the **pure-logic skeleton** of the customer signup
//! orchestration pipeline plus the Clerk webhook / D1 transaction / Stripe
//! customer-create client trait surfaces every production HTTPS Cloudflare
//! Worker handler will satisfy, plus in-memory fixtures that exercise every
//! load-bearing invariant the production wiring relies on. Property tests
//! cover INV-ONBOARD-ATOMIC-PROVISIONING via 10k PR / 100k nightly cases.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`correlation`] module ships [`CorrelationId`]
//!    (PAT-CORRELATION-ID-001 propagation token).
//! 2. The [`idempotency`] module ships [`IdempotencyKey`] (HTTP
//!    `Idempotency-Key` header value; duplicate requests resolve to the
//!    original `signup_id`).
//! 3. The [`region`] module ships [`Bcp47Locale`] +
//!    [`PrimaryRegion`] `#[non_exhaustive]` 3-canonical (`Enam` / `Sam` /
//!    `Eu`) + locale→region map per Lote 10.16 cookie-canonical fix.
//! 4. The [`tenant`] module ships [`TenantId`] + [`UserEmailHash`]
//!    (sha256 hex of normalized email; user_email NEVER persisted
//!    plaintext per CTRL-PRIV-001 + STRIDE Information Disclosure).
//! 5. The [`pat`] module ships [`PatHash`] + [`ShownOnceToken`]
//!    (UUID v7 invalidated after first GET; CTRL-CRED-001 — PAT material
//!    NEVER in signup payload; PAT issued via WI-S19-006 flow).
//! 6. The [`request`] module ships [`SignupRequest`] (clerk_event_id +
//!    email_hash + locale + idempotency_key + correlation_id) +
//!    [`SignupResponse`].
//! 7. The [`outcome`] module ships [`SignupOutcome`] `#[non_exhaustive]`
//!    4-canonical (`Provisioned` / `Deferred` / `Duplicate` / `Rejected`)
//!    plus [`OrchestrationStep`] (5-canonical staging marker for atomicity
//!    rollback debugging) plus [`BillingIntent`] `#[non_exhaustive]`
//!    3-canonical (`Linked` / `PendingBillingLink` / `Deferred`).
//! 8. The [`store`] module ships [`AtomicSignupStore`] trait with explicit
//!    `begin` / `commit` / `rollback` semantics plus
//!    [`InMemoryAtomicSignupStore`] plus [`FailingAtomicSignupStore`].
//! 9.  The [`billing`] module ships [`BillingClient`] trait plus
//!     [`InMemoryBillingClient`] plus [`StripeOutageBillingClient`]
//!     adversarial chaos fixture (returns transient outage on every call).
//! 10. The [`audit`] module ships [`SignupAuditEventType`]
//!     `#[non_exhaustive]` 4-canonical taxonomy (`corelink.signup.started`
//!     / `.completed` / `.failed` / `.deferred`) plus [`SignupAuditRecord`]
//!     plus [`SignupAuditSink`] trait plus [`InMemorySignupAuditSink`]
//!     plus [`FailingSignupAuditSink`] adversarial fixture.
//! 11. The [`orchestrator`] module ships [`SignupOrchestrator`] —
//!     audit-emit-BEFORE-mutation fail-CLOSED envelope per
//!     INV-AUDIT-APPEND-ONLY + Lote 10.6bis pattern; idempotency cache plus
//!     atomic D1 tx plus Stripe saga; chaos integration via
//!     [`BillingClient`] outage signalling → 503-equivalent plus deferred
//!     billing flag without leaking partial state.
//! 12. The [`error`] module ships [`OrchestrationError`] `#[non_exhaustive]`
//!     taxonomy (`Audit` / `Storage` / `Billing` / `InvalidInput` /
//!     `Internal`).
//!
//! # Why traits + fakes here, real HTTPS / D1 in PRR ship gate
//!
//! WI-S19-001 lands without Cloudflare Worker secrets bound to Clerk
//! webhook signing key / Stripe API key / D1 database binding (no remote
//! plus Cloudflare Workers plus Stripe staging are HARD inflection points
//! per `corelink_autonomous_execution_charter.md`). The fakes cover the
//! algorithmic invariants that a production wiring bug would expose:
//! idempotency replay, D1 transaction atomicity (all-or-nothing rollback
//! on any step failure), saga compensation (Stripe outage degrades to
//! `pending_billing_link` without orphaning the tenant), and audit-emit-
//! BEFORE-mutation fail-CLOSED envelope on every decision arm.
//!
//! # Invariants enforced
//!
//! - **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH; spec contract S-19 §8 +
//!   Lote 10.19 codex P0 canonical scope clarification): tenant + DPA
//!   acceptance + first PAT inserted in single atomic D1 transaction;
//!   ANY error in middle of orchestration → ROLLBACK ALL → zero partial
//!   tenant rows. Pinned by `prop_atomicity_any_failure_rolls_back`.
//! - **Idempotency** (R-S19-1): same `IdempotencyKey` → same `signup_id`
//!   regardless of payload variation in non-essential fields. Pinned by
//!   `prop_idempotency_key_same_signup_id`.
//! - **Chaos / Stripe outage** (R-S19-2): Stripe-down →
//!   `SignupOutcome::Deferred { billing: BillingIntent::Deferred, .. }`;
//!   tenant + DPA + first PAT remain provisioned (atomic D1 already
//!   committed); zero partial state leak. Pinned by
//!   `prop_chaos_stripe_outage_deferred_billing`.
//! - **PAT-CORRELATION-ID-001 propagation**: every audit event carries
//!   the inbound `correlation_id`; debug + audit chain integrity. Pinned
//!   by `prop_correlation_id_propagated_to_all_audit_events`.
//! - **CTRL-CRED-001**: no raw PAT material EVER appears in the signup
//!   payload (`SignupRequest`); PAT issued via WI-S19-006 flow. Type-
//!   enforced: `SignupRequest` has no PAT fields.
//! - **INV-AUDIT-APPEND-ONLY**: audit emit ordering `lookup → emit_audit
//!   → mutate_state`; audit failure aborts mutation. Pinned by
//!   `prop_audit_emit_before_mutate_fail_closed`.
//!
//! # Production wiring (deferred to PRR ship gate)
//!
//! - Cloudflare Worker handler binding `POST /v1/signup` to
//!   [`SignupOrchestrator::provision`].
//! - Clerk-Signature HMAC-SHA256 verify + nonce + 5-min timestamp window
//!   (replay rejected → 409).
//! - D1 `signup_orchestration` + `signup_attempts` migration
//!   `0037_signup_orchestration.sql` apply in production schema.
//! - Stripe customer create HTTPS via `worker::send_future` fire-and-
//!   forget (NEVER `tokio::spawn` per Lote 10.7bis R5 P0-3).
//! - Grafana Cloud dashboard `DASH-ONBOARDING` ingestion.
//! - Prometheus counters
//!   `corelink_signup_orchestration_total{outcome, region}` +
//!   `corelink_signup_atomicity_rollback_total{step}` (cardinality
//!   bounded per INV-OBS-CARDINALITY-BUDGET).
//!
//! # Wave-33 Stream B sub-step B.4 — canonical-path adoption review
//!
//! Per `specs/_audits/2026-05-22-w33-stream-b-policy.md` sub-step B.4,
//! the wave-33 reorg surveys this crate for adoption of the canonical
//! cross-cutting surfaces introduced at Stage 0:
//!
//! - `corelink_core` (apex cross-cutting types) — NOT adopted. This
//!   crate's `TenantId` / `CorrelationId` / `Bcp47Locale` etc. are
//!   signup-scoped newtype envelopes that intentionally encode signup
//!   semantics (e.g. `TenantId` here is a "freshly minted" UUIDv7
//!   that the orchestrator generates, not an externally-furnished
//!   tenant id). Migration to `corelink_core::TenantId` would alter
//!   the type's API contract (deserialization rules, format
//!   constraints) and is therefore behaviour-changing. Deferred to a
//!   consumer-migration follow-on.
//! - `corelink_crypto` — NOT applicable. The crate has no inline
//!   crypto path; SHA-256 email hashing happens at consumer wiring
//!   layer (Cloudflare Worker handler in `apps/worker/`).
//! - `corelink_audit::ports::AuditEmitter` — NOT adopted. The
//!   crate's `audit::SignupAuditSink::emit(&self, record:
//!   &SignupAuditRecord)` trait takes a strongly-typed
//!   `SignupAuditRecord` by reference; the wave-33 `AuditEmitter::emit`
//!   trait takes a generic `AuditEvent` envelope by value with
//!   `event_type: String` + `payload: serde_json::Value`. Migrating
//!   would change the public API surface (signup's trait downstream
//!   consumers would need to wrap records into JSON envelopes at
//!   every emit site, losing compile-time field validation). The
//!   migration is therefore behaviour-changing and deferred to the
//!   consumer-migration stream that owns the audit envelope
//!   harmonization across all 5 context-local `AuditSink` traits
//!   (cas / ac / admin / signup / billing).
//!
//! Crate health verified at wave-33 B.4: 67 tests pass (`cargo test
//! -p corelink-signup`); zero regressions; baseline preserved.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod billing;
pub mod correlation;
pub mod error;
pub mod idempotency;
pub mod orchestrator;
pub mod outcome;
pub mod pat;
pub mod region;
pub mod request;
pub mod store;
pub mod tenant;

pub use audit::{
    canonical_signup_audit_event_strings, FailingSignupAuditSink, InMemorySignupAuditSink,
    SignupAuditEmitError, SignupAuditEventType, SignupAuditRecord, SignupAuditSink,
};
pub use billing::{
    BillingClient, BillingError, InMemoryBillingClient, StripeCustomerId,
    StripeOutageBillingClient,
};
pub use correlation::CorrelationId;
pub use error::OrchestrationError;
pub use idempotency::IdempotencyKey;
pub use orchestrator::{ProvisionRecord, SignupOrchestrator, SignupResponse};
pub use outcome::{
    canonical_orchestration_steps, canonical_signup_outcomes, BillingIntent, OrchestrationStep,
    SignupOutcome,
};
pub use pat::{PatHash, ShownOnceToken};
pub use region::{canonical_regions, Bcp47Locale, PrimaryRegion};
pub use request::SignupRequest;
pub use store::{
    AtomicSignupStore, FailingAtomicSignupStore, InMemoryAtomicSignupStore, SignupTx,
    StorageError,
};
pub use tenant::{SignupId, TenantId, UserEmailHash};

/// Crate canonical schema version constant.
#[must_use]
pub const fn signup_schema_version() -> u32 {
    1
}

/// First PAT expiry seconds canonical (90 days; WI §9.4 + AWS access keys
/// rotation cadence parity).
pub const FIRST_PAT_EXPIRY_SECONDS: u64 = 90 * 24 * 60 * 60;

/// Webhook timestamp tolerance canonical (5 minutes; WI §6.1 + §8 replay
/// rejection window).
pub const WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS: u64 = 5 * 60;
