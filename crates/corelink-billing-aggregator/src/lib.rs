//! `corelink-billing-aggregator` — counter aggregator + BLAKE3-256
//! hash chain over JCS-canonical [`AggregatedCounter`] bytes
//! (WI-S10-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the counter-aggregator primitive: trait surfaces every
//! production CF Cron Durable Object cron-trigger 1h interval +
//! usage_event_staging drain + R2 hour bucket replay + atomic D1
//! transaction (counter row UPSERT + `hash_chain_head` UPDATE in the
//! same `db.batch`) + late-arrival routing will satisfy, plus an
//! in-memory orchestrator that exercises every load-bearing invariant
//! the production wiring relies on. Property tests pinned at 10k iter
//! against the orchestrator cover INV-BILLING-NO-LOSS Layer 1 (HIGH;
//! `invariant_registry.md §3.9 line 136`; Σ R2 events qty = Σ stored
//! aggregate `total_qty` per (tenant, billing_period, event_kind)),
//! INV-BILLING-NO-DUP (HIGH; `invariant_registry.md §3.9 line 137`;
//! re-aggregation of the same period reproduces the same digest +
//! `(tenant_id, billing_period, event_kind)` UNIQUE PRIMARY KEY guard),
//! INV-BILLING-CHAIN-INTEGRITY (HIGH; per-(tenant, billing_period)
//! BLAKE3 chain unbroken; tamper at any aggregate fails verify at the
//! first tampered sequence — Bitcoin block-header pattern inheritance
//! from `corelink-audit-chain` S-09), INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
//! (HIGH; audit envelope BEFORE state mutation on every decision arm),
//! and INV-TENANT-ISOLATION (CRITICAL, TLA+; per-tenant chain
//! partitioning; tenant A's aggregator never reads tenant B events).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`AggregatedCounter`] (CloudEvents 1.0
//!    aligned: `specversion="1.0"` /
//!    `type="corelink.billing.counter.aggregated"` / `source` /
//!    `subject="tenant:<uuid>"` / `id` UUIDv7 / `time_ms` /
//!    `datacontenttype="application/json"`) extended with the canonical
//!    chain link slot ([`ChainHash`] 32-byte BLAKE3 newtype +
//!    monotonic [`event::GENESIS_SEQUENCE_NUMBER`] / `prev_hash =
//!    [0u8; 32]` for genesis — Bitcoin-genesis-block convention) plus
//!    typed [`AggregatedCounterData`] payload `{tenant_id,
//!    billing_period, event_kind, total_qty, event_count,
//!    period_start_ms, period_end_ms, idem_keys_seen}` keyed by
//!    [`CounterGroupKey`] `(tenant_id, billing_period, event_kind)`. The
//!    [`AggregationDecision`] `#[non_exhaustive]` 3-element taxonomy
//!    (Aggregated / SkippedNoEvents / SkippedDuplicateRun) covers every
//!    canonical run outcome.
//! 2. The [`chain`] module ships [`HashChainBuilder`] (per-(tenant,
//!    billing_period) chain state machine: `head` + `next_sequence`;
//!    appends aggregates enforcing sequence + `prev_hash` integrity at
//!    the boundary; BLAKE3-256 link hash via RFC 8785 JCS
//!    canonicalization) + [`compute_canonical_bytes`] /
//!    [`link_chain_hash`] / [`link_chain_hash_from_canonical`] /
//!    [`verify_chain_link`] primitives.
//! 3. The [`aggregator`] module ships [`CounterAggregator`] trait +
//!    [`InMemoryCounterAggregator`] orchestrator (run pipeline:
//!    audit `run_started` → filter+sort by `(time_ms, idem_key)`
//!    deterministic order → SUM `qty` into `total_qty` → chain head
//!    lookup → idempotent re-run check → counter store UPSERT → audit
//!    `run_completed`) + [`AggregationRequest`] +
//!    [`PeriodWindow`] (inclusive start, exclusive end — canonical
//!    Prometheus boundary semantics; events at `time_ms == period_end_ms`
//!    belong to the NEXT period) + [`deterministic_event_order`].
//! 4. The [`store`] module ships [`AggregatedCounterStore`] trait +
//!    [`InMemoryAggregatedCounterStore`] (per-(tenant, billing_period,
//!    event_kind) UPSERT-safe append-only ledger; INV-BILLING-NO-DUP
//!    `(tenant_id, billing_period, event_kind)` UNIQUE PK enforcement;
//!    digest-mismatch divergence = SEV-1 replay corruption) +
//!    [`UpsertOutcome`] (Inserted / AlreadyExistsIdempotent) +
//!    [`ChainHeadRecord`] + [`FailingAggregatedCounterStore`].
//! 5. The [`audit`] module ships [`AggregatorAuditEventType`]
//!    `#[non_exhaustive]` 4-event taxonomy:
//!    `corelink.billing_aggregator.{run_started, run_completed,
//!    chain_break_detected, sink_failure}` +
//!    [`AggregatorAuditRecord`] + [`AggregatorAuditSink`] trait +
//!    [`InMemoryAggregatorAuditSink`] capture sink +
//!    [`FailingAggregatorAuditSink`] (fail-CLOSED envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` from S-07 P1-1 lift +
//!    Lote 10.6bis pattern + S-09 inheritance).
//! 6. The [`error`] module ships the canonical [`AggregatorError`]
//!    `#[non_exhaustive]` taxonomy (canonicalization / audit / store /
//!    chain-break / internal) + [`AggregatedCounterStoreError`] +
//!    [`AggregatorAuditSinkError`].
//!
//! # Why `trait + fake` here, real CF Cron DO + D1 in WI-S10-007
//!
//! S-10 lands without Cloudflare R2 + Cron + D1 atomic batch bindings
//! wired into CI (no remote + Cloudflare Workers + R2 staging are HARD
//! inflection points per `corelink_autonomous_execution_charter.md`).
//! The fake covers the algorithmic invariants that a production
//! binding bug would expose: per-(tenant, billing_period) chain
//! partitioning + cross-tenant isolation; BLAKE3 link recomputation
//! determinism; tamper detection at first divergence sequence;
//! sequence monotonicity; genesis-zero convention; JCS canonicalization
//! byte-stability; deterministic input ordering primitive; canonical
//! Prometheus period-boundary semantics; audit-fail-CLOSED envelope on
//! every decision arm. The live `BillingAggregatorCron-<region>` per-
//! region Durable Object cron-trigger 1h interval, usage_event_staging
//! drain, R2 hour bucket replay, atomic D1 batch (counter row UPSERT
//! plus `hash_chain_head` UPDATE), `usage_counter_late` separate D1
//! table for events with `ts < now - 6h`, watermark-based idempotent
//! replay, and Terraform IaC integration tests all run alongside
//! WI-S10-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-BILLING-NO-LOSS Layer 1** (HIGH;
//!   `invariant_registry.md §3.9 line 136`): Σ R2 events qty = Σ
//!   stored aggregate `total_qty` per (tenant, billing_period,
//!   event_kind). The orchestrator's pure SUM
//!   ([`aggregator::deterministic_event_order`] →
//!   `total_qty.saturating_add(u128::from(qty))`) preserves the Σ
//!   correspondence by construction. Pinned by
//!   `prop_aggregation_sum_correct` (10k iter PR gate; nightly 100k
//!   via `PROPTEST_CASES` env var).
//! - **INV-BILLING-NO-DUP** (HIGH;
//!   `invariant_registry.md §3.9 line 137`): re-aggregation of the
//!   same period reproduces the same canonical bytes + the same chain
//!   digest; the store's `(tenant_id, billing_period, event_kind)`
//!   UNIQUE PK rejects digest divergence with
//!   [`AggregatedCounterStoreError::DigestMismatch`] (replay corruption
//!   SEV-1 source). Pinned by `prop_idempotent_rerun_same_chain_hash`
//!   + `prop_jcs_canonicalization_byte_stable`.
//! - **INV-BILLING-CHAIN-INTEGRITY** (HIGH; analogous to
//!   `INV-OBS-AUDIT-CHAIN-INTEGRITY` from S-09 audit chain): per-
//!   (tenant, billing_period) BLAKE3 chain unbroken; the verifier
//!   recomputes the link from `(prev_hash, JCS(aggregate))` and rejects
//!   any tamper at the first divergent sequence. Pinned by
//!   `prop_chain_break_detected_on_tamper` +
//!   `prop_chain_genesis_zero_prev_hash`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07 P1-1
//!   fix + Lote 10.6bis pattern + S-09 inheritance): every aggregator
//!   decision arm fires its canonical audit BEFORE state mutation;
//!   audit failure aborts the run + propagates as
//!   [`AggregatorError::Audit`]. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): per-tenant chain partitioning; tenant A's
//!   aggregator never reads tenant B events. Pinned by
//!   `prop_tenant_isolation`.
//! - **JCS canonicalization byte-stability** (informational; RFC 8785
//!   canonical): the same `AggregatedCounter` value produces the same
//!   canonical bytes across all platforms, all serde versions
//!   compatible with `serde_jcs = "0.2"`, all process invocations.
//!   Pinned by `prop_jcs_canonicalization_byte_stable`.
//! - **Deterministic input ordering** (informational; orchestrator
//!   primitive): the canonical `(time_ms, idem_key)` lexicographic sort
//!   means re-running the aggregator over the same input set in any
//!   order reproduces the same chain digest. Pinned by
//!   `prop_deterministic_input_ordering`.
//! - **Canonical Prometheus period boundary** (informational; canonical
//!   inclusive-start / exclusive-end histogram bucket convention): an
//!   event at exactly `period_end_ms` belongs to the NEXT period —
//!   prevents double-counting at boundaries. Pinned by
//!   `prop_period_boundary_exclusive_end`.
//!
//! # Production wiring (deferred to WI-S10-007)
//!
//! - `BillingAggregatorCron-<region>` per-region Cloudflare Durable
//!   Object cron-trigger 1h interval; routing via `tenant.primary_region`
//!   per Lote 10.7bis P0-9.
//! - usage_event_staging drain (D1 `migrations/d1/0017_usage_event_idem.sql`
//!   from WI-S10-001) + R2 hour bucket replay (canonical
//!   `usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson` layout).
//! - Atomic D1 batch (counter row UPSERT + `hash_chain_head` UPDATE in
//!   the same `db.batch`) — both writes succeed or both roll back
//!   per WI-S10-002 §6.1.8.
//! - `usage_counter_late` separate D1 table for events with
//!   `ts < now - 6h` per sprint contract §5.2 R-S10-5 + SEV-3 alert.
//! - SLO-FRESH-BILLING ≤ 15min event-to-counter (sprint contract §5.7
//!   R-S10-14).
//! - Terraform IaC for D1 + R2 bucket Object Lock + Cron schedule (per
//!   WI-S10-002 §6.1.3).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod aggregator;
pub mod audit;
pub mod chain;
pub mod error;
pub mod event;
pub mod store;

pub use aggregator::{
    deterministic_event_order, AggregationRequest, CounterAggregator, InMemoryCounterAggregator,
    PeriodWindow,
};
pub use audit::{
    canonical_aggregator_audit_event_strings, AggregatorAuditEmitError, AggregatorAuditEventType,
    AggregatorAuditRecord, AggregatorAuditSink, FailingAggregatorAuditSink,
    InMemoryAggregatorAuditSink,
};
pub use chain::{
    compute_canonical_bytes, link_chain_hash, link_chain_hash_from_canonical, verify_chain_link,
    HashChainBuilder,
};
pub use error::{AggregatedCounterStoreError, AggregatorAuditSinkError, AggregatorError};
pub use event::{
    AggregatedCounter, AggregatedCounterData, AggregationDecision, ChainHash, CounterGroupKey,
    CLOUDEVENTS_DATACONTENTTYPE, CLOUDEVENTS_SPECVERSION, COUNTER_AGGREGATED_EVENT_TYPE,
    GENESIS_PREV_HASH, GENESIS_SEQUENCE_NUMBER,
};
pub use store::{
    AggregatedCounterStore, ChainHeadRecord, FailingAggregatedCounterStore,
    InMemoryAggregatedCounterStore, UpsertOutcome,
};

/// Crate canonical schema version constant. Mirrors WI-S10-002 §1
/// schema versioning policy + the canonical D1 migration slot for the
/// follow-on `usage_counter` + `hash_chain_head` tables (next slot
/// after `migrations/d1/0017_usage_event_idem.sql`; production wiring
/// at WI-S10-007 lands the additive 0018 migration).
#[must_use]
pub const fn aggregator_schema_version() -> u32 {
    18
}
