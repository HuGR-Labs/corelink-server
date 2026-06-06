//! `corelink-logpush` — structured logs + PII redaction (WI-S09-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the log emit + PII redaction primitive: trait
//! surfaces every production CF Logpush + R2 + Loki binding will
//! satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on. Property
//! tests pinned at 10k iter against the orchestrator cover
//! CTRL-PRIV-001 (zero raw-PII leakage), redaction idempotency,
//! schema serialization round-trip, tenant isolation, and audit
//! fail-closed envelope discipline.
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact `migrations/d1/0016_log_schema.sql`
//!    embedded via [`MIGRATION_0016_LOG_SCHEMA`] so production code
//!    can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors per-region log-schema
//!    version + redaction pattern config; cold-start hydration
//!    detects drift between the in-memory `SCHEMA_VERSION` const +
//!    the canonical pattern set + the durable mirror.
//! 2. The [`record`] module ships [`LogRecord`] (CloudEvents 1.0
//!    aligned: `specversion` / `type` / `source` / `id` / `time_ms` +
//!    `tenant_id` pseudonymous + `region` + `data`) +
//!    [`LogEventType`] `#[non_exhaustive]` 4-event canonical taxonomy
//!    (`request_served` / `auth_attempt` / `admin_action` /
//!    `billing_event`) + canonical NDJSON serializer.
//! 3. The [`redaction`] module ships [`PiiRedactor`] trait +
//!    [`InMemoryPiiRedactor`] capture sink applying 5 canonical
//!    patterns (Email / Ip / Token / Pan / CpfCnpj) BEFORE
//!    serialization on every emit arm. Hand-rolled scanners (no
//!    regex catastrophic backtracking risk; INV-AVAIL-DOS canary).
//! 4. The [`sink`] module ships [`LogSink`] trait +
//!    [`InMemoryLogSink`] orchestrator (cardinality guard, redaction,
//!    audit-emit-BEFORE-mutation fail-closed envelope, NDJSON
//!    serialization, and buffer push).
//! 5. The [`audit`] module ships [`LogAuditEventType`]
//!    (`#[non_exhaustive]` 4-event taxonomy:
//!    `corelink.logpush.{record_emitted, redaction_applied,
//!    redaction_failure, sink_failure}`) + [`LogAuditRecord`] +
//!    [`LogAuditSink`] + [`InMemoryLogAuditSink`] capture sink
//!    (fail-closed envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 6. The [`error`] module ships the canonical [`LogpushError`]
//!    `#[non_exhaustive]` taxonomy.
//!
//! # Why log emit + redaction is `trait + fake` here, real CF Logpush
//! # binding in WI-S09-007
//!
//! S-09 lands without Cloudflare Logpush + R2 + Loki bindings wired
//! into CI (no remote + Cloudflare Workers + AE staging are HARD
//! inflection points per `corelink_autonomous_execution_charter.md`).
//! The fake covers the algorithmic invariants that a production
//! binding bug would expose: zero PII leakage in 100k synthetic
//! seeded ChaCha20Rng samples (CTRL-PRIV-001 falsifiability target;
//! WI §6.1.5 DoD gate); redaction idempotency; per-tenant log
//! isolation (no cross-tenant body leak); CloudEvents 1.0 schema
//! round-trip; cardinality budget integration with the analytics
//! validator; audit fail-closed envelope on every decision arm. The
//! live Logpush + R2 + Loki + Terraform IaC + CI gate (ajv-cli
//! validate) integration tests run alongside WI-S09-007 (PRR ship
//! gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - CTRL-PRIV-001 (CRITICAL; privacy_model.md canonical + sprint
//!   contract §5.2 R-S09-4): zero raw-PII leakage in any serialized
//!   `LogRecord`. The 5-pattern redactor covers email + IPv4/v6 +
//!   bearer/JWT/API key + Luhn-validated PAN + Brazilian CPF/CNPJ.
//!   Pinned by `prop_pii_redaction_no_leakage` (10k iter PR gate;
//!   100k nightly + the canonical
//!   `pii_redaction_100k_synthetic` integration test that
//!   exhaustively iterates 100k samples).
//! - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from S-07 P1-1
//!   fix plus Lote 10.6bis pattern): audit emit BEFORE state
//!   mutation on every decision arm (`record_emitted`,
//!   `redaction_applied`, `redaction_failure`, `sink_failure`);
//!   audit failure aborts the emit plus returns a typed error.
//!   Pinned by `prop_audit_emit_per_event_type`.
//! - INV-TENANT-ISOLATION (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): tenant A logs never reference tenant B body
//!   data. The orchestrator pins `tenant_id` from the auth-context
//!   middleware (S-03 WI-S03-003) ONLY; raw tenant identifiers
//!   never reach the surface. Pinned by `prop_tenant_isolation`.
//! - INV-OBS-CARDINALITY-BUDGET (HIGH; lift from WI-S09-001): log
//!   emit fails-closed if the per-event-type label tuple would
//!   breach the analytics cardinality budget. Pinned by
//!   `prop_cardinality_budget_respected`.
//! - Redaction idempotency (informational invariant): the placeholder
//!   strings contain no characters that match any of the 5 patterns.
//!   Pinned by `prop_redaction_idempotent`.
//! - Schema serialization round-trip (informational invariant):
//!   `serialize → deserialize → equal`. Pinned by
//!   `prop_log_schema_serialization_roundtrip`.
//!
//! # Production wiring (deferred to WI-S09-007)
//!
//! - CF Logpush job + R2 PUT + Loki push (`worker::send_future`
//!   fire-and-forget per Lote 10.7bis R5 P0-3; NEVER `tokio::spawn`).
//! - Terraform IaC for R2 lifecycle (single-tier 400d expiration per
//!   Lote 10.9-quaters NEW-P0-3 corrected; CF R2 single storage
//!   class).
//! - Loki tenant config + LogQL canonical queries.
//! - CI hook `.github/workflows/log-schema-gate.yml` ajv-cli
//!   validation against `specs/_schemas/log_event.schema.json`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod error;
pub mod record;
pub mod redaction;
pub mod sink;

pub use audit::{
    canonical_audit_event_strings, FailingLogAuditSink, InMemoryLogAuditSink, LogAuditEmitError,
    LogAuditEventType, LogAuditRecord, LogAuditSink,
};
pub use error::{LogAuditSinkError, LogSinkError, LogpushError};
pub use record::{
    canonical_log_event_types, LogEventType, LogRecord, TenantId, CLOUDEVENTS_SPECVERSION,
    SCHEMA_VERSION,
};
pub use redaction::{
    canonical_pii_pattern_kinds, InMemoryPiiRedactor, PiiPatternKind, PiiRedactor,
    RedactionOutcome, CNPJ_PLACEHOLDER, CPF_PLACEHOLDER, EMAIL_PLACEHOLDER, IP_PLACEHOLDER,
    PAN_PLACEHOLDER, TOKEN_PLACEHOLDER,
};
pub use sink::{CapturedLogSink, FailingLogSink, InMemoryLogSink, LogSink, PersistedLogLine};

/// Canonical SQL DDL for the log-schema versioning + redaction
/// pattern config durable mirror (D1 migration 0016).
///
/// Production wiring at WI-S09-007 passes this string to
/// `wrangler d1 migrations apply --remote`; the same DDL is replayed
/// during local miniflare integration tests.
pub const MIGRATION_0016_LOG_SCHEMA: &str =
    include_str!("../../../migrations/d1/0016_log_schema.sql");

/// Canonical D1 schema version for the logpush mirror tables.
///
/// Mirrors the migration filename prefix; lifted into a typed surface
/// so the production wiring asserts the binding-side schema version
/// matches the embedded migration before accepting any emit.
#[must_use]
pub const fn logpush_schema_version() -> u32 {
    16
}

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::logpush"
}
