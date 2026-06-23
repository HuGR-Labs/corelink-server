//! `corelink-billing-emit` — CloudEvents 1.0 usage-event emitter +
//! BLAKE3-derived idempotency-key tracker + R2 append-only NDJSON sink
//! (WI-S10-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the billing emit primitive: trait surfaces every
//! production CF R2 PutObject + Cloudflare Queue retry drain +
//! `(tenant_id, request_id) UNIQUE` D1 staging table mirror will
//! satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on. Property
//! tests pinned at 10k iter against the orchestrator cover
//! INV-BILLING-NO-DUP (HIGH; idempotent-replay tracker; per-tenant
//! partition; collision rate < 2^-128 under BLAKE3-256),
//! INV-BILLING-APPEND-ONLY (HIGH; append-only NDJSON layout
//! `usage/{tenant}/{billing_period}/{seq:08}.usage.ndjson`),
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; audit envelope BEFORE
//! state mutation on every decision arm), and the canonical 7-element
//! [`UsageEventKind`] taxonomy.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`UsageEvent`] (CloudEvents 1.0
//!    aligned: `specversion="1.0"` / `type="corelink.billing.usage.recorded"` /
//!    `source` / `subject="tenant:<uuid>"` / `id` UUIDv7 / `time_ms` /
//!    `datacontenttype="application/json"` / `region` / typed `data`
//!    `{tenant_id, event_kind, qty, unit, billing_period}`) extended
//!    with the canonical [`IdemKey`] slot (32-byte BLAKE3-256 of the
//!    JCS-canonical bytes with the slot zeroed for the link input —
//!    Bitcoin-genesis-block convention from S-09 audit chain) +
//!    [`UsageEventKind`] `#[non_exhaustive]` 7-canonical taxonomy
//!    (`storage_bytes_hourly` / `egress_bytes` / `ac_lookup` /
//!    `cas_get` / `cas_put` / `replay_request` /
//!    `runner_slot_seconds`) + [`UsageUnit`] canonical (Bytes /
//!    OpCount) + [`validate_billing_period`] `YYYY-MM` shape guard.
//! 2. The [`idempotency`] module ships
//!    [`derive_idem_key`] + [`derive_idem_key_from_canonical`]
//!    (canonical BLAKE3-256 of JCS bytes with the slot zeroed) +
//!    [`compute_canonical_bytes_for_idem`] +
//!    [`IdempotencyTracker`] trait + [`IdempotencyDecision`] (Accepted
//!    / DuplicateRejected) + [`InMemoryIdempotencyTracker`] orchestrator
//!    (per-tenant set membership; canonical-byte collision detection).
//! 3. The [`sink`] module ships [`R2UsageSink`] trait +
//!    [`InMemoryR2UsageSink`] orchestrator (canonicalize → R2 NDJSON
//!    write at canonical
//!    `usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson` key
//!    per WI-S10-001 §1; per-(tenant, billing_period) sequence ledger;
//!    INV-BILLING-APPEND-ONLY enforcement at the trait surface) +
//!    [`canonical_r2_key`] +
//!    [`PersistedUsageLine`] + [`FailingR2UsageSink`].
//! 4. The [`emitter`] module ships [`UsageEventEmitter`] trait +
//!    [`InMemoryUsageEventEmitter`] orchestrator (canonicalize → derive
//!    idem_key → idempotency check → audit envelope BEFORE state
//!    mutation → R2 PutObject) + [`EmitOutcome`] (Persisted /
//!    DuplicateRejected).
//! 5. The [`audit`] module ships [`BillingAuditEventType`]
//!    (`#[non_exhaustive]` 4-event taxonomy:
//!    `corelink.billing.{usage_emitted, duplicate_rejected,
//!    sink_failure, idempotency_collision}`) +
//!    [`BillingAuditRecord`] + [`BillingAuditSink`] trait +
//!    [`InMemoryBillingAuditSink`] capture sink (fail-CLOSED envelope
//!    per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` from S-07 P1-1 lift +
//!    Lote 10.6bis pattern).
//! 6. The [`error`] module ships the canonical [`BillingEmitError`]
//!    `#[non_exhaustive]` taxonomy (canonicalization / audit / sink /
//!    duplicate-rejected / idempotency-collision / internal).
//!
//! # Why `trait + fake` here, real CF R2 + Cron in WI-S10-007
//!
//! S-10 lands without Cloudflare R2 + Cron + Queue bindings wired into
//! CI (no remote + Cloudflare Workers + R2 staging are HARD inflection
//! points per `corelink_autonomous_execution_charter.md`). The fake
//! covers the algorithmic invariants that a production binding bug
//! would expose: per-tenant idempotency partitioning + cross-tenant
//! isolation; BLAKE3 derivation determinism + collision detection;
//! INV-BILLING-APPEND-ONLY at the canonical key boundary; sequence
//! monotonicity per (tenant, billing_period); JCS canonicalization
//! byte-stability; audit-fail-CLOSED envelope on every decision arm.
//! The live R2 PutObject (with Object Lock Governance Mode 7y
//! retention) + Cloudflare Queue retry drain + `(tenant_id, request_id)
//! UNIQUE` D1 staging table + Terraform IaC integration tests run
//! alongside WI-S10-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-BILLING-NO-DUP** (HIGH; invariant_registry.md §3.9 line 137):
//!   the same canonical event reproduces the same `idem_key` (BLAKE3-256
//!   of JCS bytes with slot zeroed); the
//!   [`IdempotencyTracker`] per-tenant set rejects replays at the
//!   trait surface. Pinned by `prop_idem_key_deterministic` +
//!   `prop_duplicate_rejected_on_replay` (10k iter PR gate; nightly
//!   100k via `PROPTEST_CASES` env var).
//! - **INV-BILLING-APPEND-ONLY** (HIGH; analogous to
//!   INV-AUDIT-APPEND-ONLY from invariant_registry §3.6): the R2 sink
//!   rejects any attempt to overwrite an existing
//!   `(tenant, billing_period, seq)` key. Pinned by
//!   `prop_append_only_no_overwrite`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07 P1-1
//!   fix + Lote 10.6bis pattern + S-09 inheritance): every emit
//!   decision arm fires its canonical audit BEFORE state mutation;
//!   audit failure aborts state mutation + propagates as
//!   `BillingEmitError::Audit`. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): per-tenant idempotency partitioning; tenant A's
//!   set never affects tenant B's emit decision. Pinned by
//!   `prop_tenant_isolation`.
//! - **JCS canonicalization byte-stability** (informational; RFC 8785
//!   canonical): the same `UsageEvent` value produces the same
//!   canonical bytes across all platforms, all serde versions
//!   compatible with `serde_jcs = "0.2"`, all process invocations.
//!   Pinned by `prop_jcs_canonicalization_byte_stable`.
//! - **billing_period `YYYY-MM` invariant** (informational): the
//!   canonical period string MUST match the `dddd-dd` shape with month
//!   in `01..=12`. Pinned by `prop_billing_period_format_yyyy_mm`.
//!
//! # Production wiring (deferred to WI-S10-007)
//!
//! - CF R2 PutObject + Object Lock Governance Mode 7y retention via
//!   `worker::send_future` fire-and-forget per Lote 10.7bis R5 P0-3
//!   (NEVER `tokio::spawn`).
//! - `(tenant_id, request_id) UNIQUE` D1 staging table mirror — see
//!   `migrations/d1/0017_usage_event_idem.sql` (this WI ships the
//!   schema; production wiring at WI-S10-007 binds the orchestrator to
//!   the D1 mirror).
//! - Cloudflare Queue retry drain: staging table → R2 PutObject;
//!   SLO-FRESH-BILLING ≤ 15min event-to-counter (sprint contract §5.7
//!   R-S10-14).
//! - Terraform IaC for R2 bucket Object Lock + lifecycle (per WI-S10-001
//!   §6.1.3).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod emitter;
pub mod error;
pub mod event;
pub mod idempotency;
pub mod sink;

pub use audit::{
    canonical_billing_audit_event_strings, BillingAuditEmitError, BillingAuditEventType,
    BillingAuditRecord, BillingAuditSink, FailingBillingAuditSink, InMemoryBillingAuditSink,
};
pub use emitter::{EmitOutcome, InMemoryUsageEventEmitter, UsageEventEmitter};
pub use error::{BillingAuditSinkError, BillingEmitError, R2UsageSinkError};
pub use event::{
    canonical_usage_event_kinds, validate_billing_period, IdemKey, InvalidBillingPeriod,
    UsageEvent, UsageEventData, UsageEventKind, UsageUnit, CLOUDEVENTS_DATACONTENTTYPE,
    CLOUDEVENTS_SPECVERSION, GENESIS_IDEM_KEY, USAGE_EVENT_TYPE,
};
pub use idempotency::{
    compute_canonical_bytes_for_idem, derive_idem_key, derive_idem_key_from_canonical,
    IdempotencyDecision, IdempotencyTracker, InMemoryIdempotencyTracker,
};
pub use sink::{
    canonical_r2_key, FailingR2UsageSink, InMemoryR2UsageSink, PersistedUsageLine, R2UsageSink,
};

/// Crate canonical schema version constant. Mirrors WI-S10-001 §1
/// schema versioning policy (sprint contract §5.1 R-S10-3): events
/// emitted at v1 are compatible with consumers at v1.
#[must_use]
pub const fn billing_emit_schema_version() -> u32 {
    1
}
