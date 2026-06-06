//! `corelink-billing-replay` — audit-grade replay forensic engine
//! (WI-S10-006).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the replay forensic primitive. The trait surfaces
//! match what every production wiring (CF Worker route at
//! `POST /v1/billing/replay`, R2 NDJSON archive read at
//! `usage/{tenant_id}/{billing_period}/*.usage.ndjson`, admin RBAC
//! binding via the canonical Tower middleware) will satisfy, plus an
//! in-memory orchestrator that exercises every load-bearing invariant
//! the production wiring relies on.
//!
//! Property tests pinned at 10k iter against the orchestrator cover
//! authorization (only `billing_forensics_admin` requests reach the
//! executed arm), idempotency-on-`request_id` (same UUIDv7 re-submitted
//! reuses prior outcome), determinism (same `(tenant, billing_period,
//! archive)` produces byte-identical reconstruction), dry-run isolation
//! (no state mutation past the audit row), tenant isolation
//! (cross-tenant replays never observe each other's archive contents),
//! audit-emit-per-decision-arm (canonical fail-CLOSED envelope per
//! Lote 10.6bis pattern), and chain-event-per-replay (canonical S-09
//! audit chain extension per INV-OBS-AUDIT-CHAIN-INTEGRITY).
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`ReplayReason`] `#[non_exhaustive]`
//!    4-element taxonomy (DriftInvestigation / CustomerDispute /
//!    ComplianceAudit / DryRun), [`ReplayDecision`] `#[non_exhaustive]`
//!    4-element taxonomy (Authorized / Denied403 / DryRunPlan /
//!    Executed), [`ReplayRequest`] (canonical UUIDv7 request_id +
//!    requested_by + presented_role + tenant_id + billing_period +
//!    reason), [`ReconstructedLayers`] (3-layer u128 totals snapshot),
//!    [`ReplayOutcome`] (idempotency ledger row), [`LayerDriftSummary`]
//!    `#[non_exhaustive]` 5-element taxonomy (AllLayersMatch /
//!    Layer1Diverged / Layer2Diverged / Layer3Diverged /
//!    MultipleLayersDiverged), [`ReplayConfig`] (canonical defaults),
//!    plus the canonical [`event::BILLING_FORENSICS_ADMIN_ROLE`]
//!    string (per CTRL-AUTHZ-002; separate from regular admin).
//! 2. The [`audit`] module ships [`audit::ReplayAuditEventType`]
//!    `#[non_exhaustive]` 5-event taxonomy:
//!    `corelink.billing_replay.{request_authorized, request_denied,
//!    dry_run_planned, executed, layer_diverged}` + [`audit::ReplayAuditRecord`] +
//!    [`audit::ReplayAuditSink`] trait + [`audit::InMemoryReplayAuditSink`]
//!    capture sink + [`audit::FailingReplayAuditSink`] + the
//!    [`audit::audit_event_for_decision`] canonical mapping
//!    (fail-CLOSED envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` from S-07 P1-1 lift +
//!    Lote 10.6bis pattern + S-09 inheritance).
//! 3. The [`idempotency`] module ships
//!    [`idempotency::ReplayIdempotencyLedger`] trait +
//!    [`idempotency::InMemoryReplayIdempotencyLedger`] +
//!    [`idempotency::RecordOutcome`] (Inserted /
//!    AlreadyExistsIdempotent) +
//!    [`idempotency::FailingReplayIdempotencyLedger`] (the canonical
//!    `request_id` UNIQUE PK enforcement; tampering signal surface
//!    via [`error::ReplayIdempotencyError::DivergentPayload`]).
//! 4. The [`archive`] module ships [`archive::ReplayArchive`] trait +
//!    [`archive::InMemoryReplayArchive`] +
//!    [`archive::FailingReplayArchive`] (the canonical R2 NDJSON
//!    archive read surface; production wiring at WI-S10-007 binds
//!    this to `r2_get_object`-backed NDJSON line-by-line parse +
//!    canonical aggregator re-application).
//! 5. The [`engine`] module ships [`engine::ReplayEngine`] trait +
//!    [`engine::InMemoryReplayEngine`] orchestrator (run pipeline:
//!    role check → audit `request_denied` BEFORE Denied403 return /
//!    audit `request_authorized` for idempotent re-fire / audit
//!    `dry_run_planned` BEFORE DryRunPlan return / archive read →
//!    drift classification → audit `executed` BEFORE ledger UPSERT /
//!    audit `layer_diverged` for divergence anomaly → ledger UPSERT)
//!    plus the canonical [`engine::CANONICAL_LAYER_COUNT`] = 3
//!    constant + per-instance `Arc<Mutex<()>>` F-001 closure.
//! 6. The [`error`] module ships the canonical
//!    [`error::ReplayError`] `#[non_exhaustive]` taxonomy (audit /
//!    idempotency / config / internal) +
//!    [`error::ReplayAuditSinkError`] +
//!    [`error::ReplayIdempotencyError`].
//!
//! # Why `trait + fake` here, real CF Worker route + R2 NDJSON read +
//! # admin RBAC at WI-S10-007
//!
//! S-10 lands without Cloudflare R2 + Worker routes + admin RBAC
//! binding wired into CI (no remote + Cloudflare Workers + R2 staging
//! are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! authorization gate (only the canonical `billing_forensics_admin`
//! role reaches the executed arm); idempotency-on-`request_id`
//! (canonical UUIDv7 re-submission reuses prior outcome by
//! construction); determinism (same `(tenant, billing_period,
//! archive)` produces byte-identical reconstruction); dry-run isolation
//! (no idempotency ledger UPSERT past the audit row); tenant isolation
//! (cross-tenant archive reads structurally impossible); audit-fail-
//! CLOSED envelope BEFORE every state mutation; layer-divergence
//! supplemental audit row separate from the executed row.
//!
//! The live `BillingReplayDO` Cloudflare Durable Object at the
//! canonical `POST /v1/billing/replay` route, R2 NDJSON archive read
//! cooperation with WI-S10-001 emit surface
//! (`usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson`),
//! canonical aggregator re-application cooperation with WI-S10-002
//! `aggregate_for_period`, Stripe usage-record fetch cooperation with
//! WI-S10-003 `fetch_invoice` (FM-151 backoff + queue fallback),
//! reconciliation-decision cooperation with WI-S10-004
//! `BillingReconciler::reconcile`, real D1 `billing_replay_audit`
//! ledger (additive migration `migrations/d1/0021_billing_replay_audit.sql`),
//! canonical S-09 audit chain extension (every replay is a separate
//! `ChainEvent` per INV-OBS-AUDIT-CHAIN-INTEGRITY), Tower middleware
//! enforcing `billing_forensics_admin` role at the route per
//! CTRL-AUTHZ-002, and 30-min p99 reconstruction SLA budget all run
//! alongside WI-S10-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **CTRL-AUTHZ-002** (security_model.md): replay endpoint requires
//!   the canonical `billing_forensics_admin` role (separate from
//!   regular admin so the audit-grade replay capability is least-
//!   privilege-bounded). Pinned by
//!   `prop_authorized_role_only_executes`.
//! - **Idempotency-on-`request_id`** (forensic determinism rationale;
//!   WI-S10-006 §1 invariant 6): same UUIDv7 `request_id` re-submitted
//!   reuses the prior outcome by construction; never double-executes.
//!   Pinned by `prop_idempotent_replay_same_request_id`.
//! - **Determinism** (WI-S10-006 §1 invariant 7): same `(tenant,
//!   billing_period, archive)` produces byte-identical reconstruction
//!   across invocations. Pinned by `prop_replay_deterministic`.
//! - **Dry-run isolation** (WI-S10-006 §6.1.3): the
//!   [`event::ReplayReason::DryRun`] arm computes the plan + emits
//!   the `dry_run_planned` audit row but does NOT UPSERT the
//!   idempotency ledger. Pinned by `prop_dry_run_no_state_mutation`.
//! - **Layer-divergence flagged** (WI-S10-006 §6.1.5): when the
//!   reconstructed layers differ from the production reference
//!   values, the `layer_diverged` audit fires alongside the `executed`
//!   audit + the canonical [`event::ReplayDecision::Executed`] arm
//!   carries `layer_diverged = true`. Pinned by
//!   `prop_layer_diverged_flagged`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): the canonical
//!   [`archive::ReplayArchive::read_layers`] surface is keyed by
//!   `(tenant_id, billing_period)`; cross-tenant reads structurally
//!   impossible. Pinned by `prop_tenant_isolation`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07
//!   P1-1 fix + Lote 10.6bis pattern + S-09 inheritance + WI-S10-002
//!   /003 /004 /005 inheritance): every replay decision arm fires
//!   its canonical audit BEFORE state mutation; audit failure aborts
//!   the run + propagates as [`error::ReplayError::Audit`]. Pinned
//!   by `prop_audit_emit_per_decision_arm`.
//! - **INV-OBS-AUDIT-CHAIN-INTEGRITY** (S-09 inheritance): every
//!   replay is a separate `ChainEvent` in the canonical S-09 audit
//!   chain (production wiring at WI-S10-007 binds the canonical
//!   audit sink to the S-09 chain extension surface). Pinned by
//!   `prop_chain_event_appended_per_replay`.
//!
//! # Production wiring (deferred to WI-S10-007)
//!
//! - `BillingReplayDO` Cloudflare Durable Object at the canonical
//!   `POST /v1/billing/replay` route (Lote 10.7bis R5 P0-3:
//!   `worker::send_future`; R2 + Stripe HTTP via wasm-bindgen; NEVER
//!   `tokio::spawn`).
//! - R2 NDJSON archive read cooperation with WI-S10-001 emit surface
//!   (`usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson` +
//!   per-line canonical `UsageEvent` re-deserialization +
//!   canonical aggregator re-application).
//! - Canonical aggregator re-application cooperation with WI-S10-002
//!   `aggregate_for_period` (production wiring re-uses the SAME
//!   canonical code paths so reconstruction is deterministic by
//!   construction).
//! - Stripe usage-record fetch cooperation with WI-S10-003
//!   `fetch_invoice` (PAT-BACKOFF-001 retry + PAT-QUEUE-EVENTS-001
//!   fallback + 30-min p99 SLA budget).
//! - Reconciliation-decision cooperation with WI-S10-004
//!   `BillingReconciler::reconcile` (the canonical drift detection
//!   path produces the same `ReconcileDecision` shape the replay
//!   forensic engine consumes via
//!   [`engine::drift_summary_for_reconcile`]).
//! - Real D1 `billing_replay_audit` ledger (additive migration
//!   `migrations/d1/0021_billing_replay_audit.sql`).
//! - Canonical S-09 audit chain extension: every replay is a separate
//!   `ChainEvent` per INV-OBS-AUDIT-CHAIN-INTEGRITY; the canonical
//!   audit sink wraps the S-09 chain APPEND surface.
//! - Tower middleware enforcing the canonical
//!   `billing_forensics_admin` role at the route per CTRL-AUTHZ-002.
//! - 30-min p99 reconstruction SLA budget (sprint contract §6 DoD +
//!   §14.s10.3).
//! - 10 req/h rate limit per `billing_forensics_admin` user (sprint
//!   contract §15 R-007 mitigation against forensic abuse via S-08
//!   R-S08-3 cooperation).
//! - RB-BILLING-001 documented runbook (sprint contract §14.s10.3).
//! - CI mensal replay endpoint test (sprint contract §8 INV
//!   verification).

pub mod archive;
pub mod audit;
pub mod engine;
pub mod error;
pub mod event;
pub mod idempotency;

pub use archive::{FailingReplayArchive, InMemoryReplayArchive, ReplayArchive};
pub use audit::{
    audit_event_for_decision, canonical_replay_audit_event_strings, FailingReplayAuditSink,
    InMemoryReplayAuditSink, ReplayAuditEmitError, ReplayAuditEventType, ReplayAuditRecord,
    ReplayAuditSink,
};
pub use engine::{
    drift_summary_for_reconcile, InMemoryReplayEngine, ReplayEngine, CANONICAL_LAYER_COUNT,
};
pub use error::{ReplayAuditSinkError, ReplayError, ReplayIdempotencyError};
pub use event::{
    canonical_replay_reasons, drift_summary_from_reconcile, LayerDriftSummary, ReconstructedLayers,
    ReplayConfig, ReplayDecision, ReplayOutcome, ReplayReason, ReplayRequest,
    BILLING_FORENSICS_ADMIN_ROLE,
};
pub use idempotency::{
    FailingReplayIdempotencyLedger, InMemoryReplayIdempotencyLedger, RecordOutcome,
    ReplayIdempotencyLedger,
};

/// Crate canonical schema version constant. Mirrors WI-S10-006 §1
/// schema versioning policy + the canonical D1 migration slot for the
/// follow-on `billing_replay_audit` table (next slot after
/// `migrations/d1/0020_quota_fsm_state.sql`; production wiring at
/// WI-S10-007 lands the additive 0021 production migration alongside
/// this WI's idempotency-ledger staging table).
#[must_use]
pub const fn replay_schema_version() -> u32 {
    21
}
