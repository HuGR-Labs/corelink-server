//! `corelink-quota-fsm` — 5-state quota state machine (WI-S10-005).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the canonical quota state machine: trait surfaces every
//! production CF Durable Object per-tenant singleton + real D1
//! `quota_fsm_state` durable mirror + real Stripe webhook adapter
//! integration + real S-13 admin/notifications consumer subscribing to
//! the audit chain will satisfy, plus an in-memory orchestrator that
//! exercises every load-bearing invariant the production wiring relies
//! on. Property tests pinned at 10k iter against the orchestrator cover
//! the 5-state ladder boundary semantics, idempotent-rerun discipline,
//! 3-invoice-failure suspension, per-tenant isolation, and the
//! audit-fail-CLOSED envelope on every state-mutating arm.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`QuotaState`] `#[non_exhaustive]`
//!    5-element taxonomy (`WithinPlan` / `SoftWarning80pct` /
//!    `SoftWarning95pct` / `OverQuota100pct` /
//!    `SuspendedForNonPayment`), [`QuotaTransition`]
//!    `#[non_exhaustive]` 6-element taxonomy (`NoChange` /
//!    `TransitionedTo80pct` / `TransitionedTo95pct` /
//!    `TransitionedTo100pct` / `Suspended` / `Reinstated`),
//!    [`UtilizationPct`] `[0, 200]`-bounded wrapper,
//!    [`InvoiceFailureCount`] saturating counter, [`QuotaFsmConfig`]
//!    with the canonical ladder constants
//!    ([`event::SOFT_WARNING_80PCT_THRESHOLD`] /
//!    [`event::SOFT_WARNING_95PCT_THRESHOLD`] /
//!    [`event::OVER_QUOTA_100PCT_THRESHOLD`] /
//!    [`event::SUSPENSION_INVOICE_FAILURE_THRESHOLD`]), plus the pure
//!    [`event::utilization_bucket`] mapper + canonical
//!    [`event::canonical_quota_states`] roster.
//! 2. The [`audit`] module ships [`audit::QuotaAuditEventType`]
//!    `#[non_exhaustive]` 4-event taxonomy
//!    `corelink.billing_quota.{state_changed,
//!    overage_telemetry_recorded, suspended, reinstated}` +
//!    [`audit::QuotaAuditRecord`] +
//!    [`audit::QuotaAuditSink`] trait +
//!    [`audit::InMemoryQuotaAuditSink`] capture sink +
//!    [`audit::FailingQuotaAuditSink`] +
//!    [`audit::audit_event_for_transition`] +
//!    [`audit::transition_emits_overage_telemetry`] canonical mappings
//!    (fail-CLOSED envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` from S-07 P1-1 lift +
//!    Lote 10.6bis pattern + S-09 inheritance).
//! 3. The [`store`] module ships [`store::QuotaFsmStateRow`] plus
//!    [`store::QuotaFsmStore`] trait + [`store::InMemoryQuotaFsmStore`]
//!    plus [`store::FailingQuotaFsmStore`] (the `(tenant_id)` UNIQUE PK
//!    durable per-tenant state mirror at the storage layer).
//! 4. The [`fsm`] module ships [`fsm::QuotaStateMachine`] trait +
//!    [`fsm::InMemoryQuotaStateMachine`] orchestrator (run pipeline:
//!    audit `state_changed` BEFORE store UPSERT on every utilization
//!    transition; sibling `overage_telemetry_recorded` BEFORE store
//!    UPSERT on 80pct + 95pct entries; audit `suspended` BEFORE store
//!    UPSERT on the terminal-arm transition; audit `reinstated`
//!    BEFORE store UPSERT on the operator-driven reinstatement;
//!    per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07
//!    DO-actor model).
//! 5. The [`error`] module ships the canonical [`error::QuotaFsmError`]
//!    `#[non_exhaustive]` taxonomy (audit / store / config / internal)
//!    plus [`error::QuotaFsmAuditSinkError`] +
//!    [`error::QuotaFsmStoreError`].
//!
//! # Why `trait + fake` here, real CF DO + D1 + Stripe webhook in WI-S10-007
//!
//! S-10 lands without Cloudflare D1 atomic batch + Cloudflare Worker
//! Durable Object actor model + real Stripe webhook adapter wiring into
//! CI (no remote + Cloudflare Workers + R2 staging are HARD inflection
//! points per `corelink_autonomous_execution_charter.md`). The fake
//! covers the algorithmic invariants that a production binding bug
//! would expose: 5-state ladder boundary semantics; idempotent re-fire
//! at every arm; suspension counter monotonicity; reinstate clears the
//! counter; per-tenant isolation; audit-fail-CLOSED envelope BEFORE
//! every state-store UPSERT. The live `QuotaFsmDO-<tenant>` per-tenant
//! Cloudflare Durable Object actor model (mirrors
//! `corelink-quota::check.rs` S-07 inheritance), real D1
//! `quota_fsm_state` row UPSERT, real Stripe webhook adapter integration
//! (the WI-S10-003 `WebhookEvent::InvoiceFailed` arm dispatches into
//! `record_invoice_failure`), real S-13 admin/notifications consumer
//! subscribing to the canonical audit chain (the
//! `OverageTelemetryRecorded` audit drives the customer-facing email
//! send; email send is REJECTED in S-10 per ADR-0020 FROZEN), real
//! Tower middleware enforcing the `billing_admin` role at the
//! reinstatement endpoint (per CTRL-AUTHZ-001 + CTRL-AUTHZ-002), and
//! real `corelink_time::next_month_first_utc_midnight()` boundary
//! primitive (the period-reset cron resets the per-tenant utilization
//! state) all run alongside WI-S10-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **CTRL-BILLING-001** (security_model.md): financial integrity
//!   quota enforcement; bypass = uncontrolled customer cost.
//! - **CTRL-AUTHZ-001 + CTRL-AUTHZ-002** (security_model.md): the
//!   reinstatement endpoint is `billing_admin`-protected at the
//!   production wiring's Tower middleware; this crate ships the
//!   state-machine contract; the role enforcement is the caller's
//!   responsibility per the trait surface contract.
//! - **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant quota;
//!   one tenant's state never affects another tenant's. Pinned by
//!   `prop_tenant_isolation`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07 P1-1
//!   fix + Lote 10.6bis pattern + S-09 inheritance + WI-S10-001/002/
//!   003/004 inheritance): every state-mutating transition arm fires
//!   its canonical audit BEFORE the state-store UPSERT; audit failure
//!   aborts the call + propagates as [`error::QuotaFsmError::Audit`].
//!   Pinned by `prop_audit_emit_per_decision_arm`.
//! - **Idempotent re-fire** (per WI brief: "re-firing same state
//!   transition is no-op"): same utilization snapshot → `NoChange` arm
//!   → no audit row, no store UPSERT. Pinned by
//!   `prop_idempotent_transition_no_change`.
//! - **3-invoice-failure suspension** (per WI brief): the canonical
//!   threshold (default 3) drives the terminal-arm transition into
//!   `SuspendedForNonPayment`. Pinned by `prop_3_invoice_failures_suspends`.
//! - **Lote 10.9-quinquies NEW-P0-2 absorbed** (typed payload):
//!   [`event::QuotaTransition`] is a typed enum (NOT
//!   `serde_json::Value`); the audit envelope written to the S-09 7y
//!   archive is byte-deterministic; PII is never surfaced through a
//!   permissive JSON field.
//! - **Fail-CLOSED at quota state transition** (Lote 10.6bis split-tier
//!   canonical): transition writes audit row + state UPSERT atomically
//!   per the audit-emit-BEFORE-mutation discipline. Failure halts the
//!   call; SEV-2 alert; manual replay. Distinct from the hot-path
//!   `is_hard_blocked()` query (fail-OPEN; lives at the Tower-layer
//!   boundary at WI-S10-007).
//!
//! # Production wiring (deferred to WI-S10-007)
//!
//! - `QuotaFsmDO-<tenant>` per-tenant Cloudflare Durable Object actor
//!   model (mirrors `corelink-quota::check.rs` S-07 inheritance).
//! - D1 `quota_fsm_state` row UPSERT (additive migration
//!   `migrations/d1/0020_quota_fsm_state.sql` ships canonical
//!   `(tenant_id)` UNIQUE PK + 5-element CHECK + invoice-failure
//!   counter NON-NEGATIVE).
//! - Stripe webhook adapter integration (the WI-S10-003
//!   `WebhookEvent::InvoiceFailed` arm dispatches into
//!   [`fsm::QuotaStateMachine::record_invoice_failure`]).
//! - S-13 admin/notifications consumer (subscribes to the canonical
//!   `corelink.billing_quota.overage_telemetry_recorded` plus
//!   `.suspended` plus `.reinstated` audit topics; dispatches
//!   customer-facing email + in-app notification).
//! - Tower middleware enforcing `billing_admin` role at the
//!   reinstatement endpoint (per CTRL-AUTHZ-001 + CTRL-AUTHZ-002).
//! - `corelink_time::next_month_first_utc_midnight()` boundary
//!   primitive (the period-reset cron resets per-tenant utilization
//!   state at the canonical billing-period boundary).
//! - Hot-path Tower middleware reading `OverQuota100pct` →
//!   `429 + X-RateLimit-Layer: quota + Retry-After:
//!   <seconds_until_period_reset>` per the S-08 alignment
//!   (cooperation `corelink-rate-headers` lift).
//! - Hot-path Tower middleware reading `SuspendedForNonPayment` →
//!   `403 forbidden` (terminal arm; service throttled until operator
//!   reinstatement).
//! - PagerDuty 2 services dispatch: `corelink-finance` (Suspended SEV-1
//!   page) / `corelink-sre` (audit-emit failure SEV-2 page).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod error;
pub mod event;
pub mod fsm;
pub mod store;

pub use audit::{
    audit_event_for_transition, canonical_quota_audit_event_strings,
    transition_emits_overage_telemetry, FailingQuotaAuditSink, InMemoryQuotaAuditSink,
    QuotaAuditEventType, QuotaAuditRecord, QuotaAuditSink,
};
pub use error::{QuotaFsmAuditSinkError, QuotaFsmError, QuotaFsmStoreError};
pub use event::{
    canonical_quota_states, utilization_bucket, InvoiceFailureCount, QuotaFsmConfig,
    QuotaState, QuotaTransition, UtilizationPct, OVER_QUOTA_100PCT_THRESHOLD,
    SOFT_WARNING_80PCT_THRESHOLD, SOFT_WARNING_95PCT_THRESHOLD,
    SUSPENSION_INVOICE_FAILURE_THRESHOLD,
};
pub use fsm::{InMemoryQuotaStateMachine, QuotaStateMachine};
pub use store::{
    FailingQuotaFsmStore, InMemoryQuotaFsmStore, QuotaFsmStateRow, QuotaFsmStore,
};

/// Crate canonical schema version constant. Mirrors the canonical D1
/// migration slot for the follow-on `quota_fsm_state` table (next slot
/// after `migrations/d1/0019_billing_reconciliation_drift.sql`;
/// production wiring at WI-S10-007 lands the additive 0020 production
/// migration alongside the per-tenant DO actor model binding).
#[must_use]
pub const fn quota_fsm_schema_version() -> u32 {
    20
}
