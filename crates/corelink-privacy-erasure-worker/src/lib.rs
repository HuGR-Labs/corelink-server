//! `corelink-privacy-erasure-worker` — DSR cross-backend erasure
//! orchestrator (WI-S11-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the DSR erasure cross-backend pipeline. The trait
//! surfaces match what every production wiring (Cloudflare Queue
//! consumer of `dsr.queued.v1`, Neon multi-tabela DELETE cascade,
//! Neon billing fiscal pseudonymize, R2 CAS refcount-aware
//! soft-delete + GC sweep grace 72h, R2 AC mutable DELETE, D1 row
//! purge, KV key prefix DELETE, Stripe `Customer.update` PII nullify,
//! Loki `/loki/api/v1/delete` API, R2 audit Object Lock 7y HKDF
//! pseudonymization, Neon PITR backup tombstone replay, R2 CAS
//! legal_hold partition pseudonymize, R2 evidence-* buckets
//! pseudonymize, 24h cron worker verification sweep, R2 evidence-dsr
//! signed URL with BLAKE3-signed JCS-canonical erasure-report.json)
//! will satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on.
//!
//! Property tests pinned at 10k iter against the orchestrator cover
//! the fan-out-to-all-12-backends contract, the
//! same-`dsr_id`-twice → identical-tombstone idempotency contract
//! (PAT-RETRY-IDEMPOTENT-001), the >24h post-plan SLA breach contract
//! (LGPD Art. 18 §III + GDPR Art. 17.1 "without undue delay"), the
//! all-backends-verified → final-report-byte-stable contract, the
//! any-backend-failure → VerifiedPartial contract, the
//! tenant-A-erasure-never-affects-tenant-B isolation contract
//! (INV-TENANT-ISOLATION canary), the audit-emit-per-decision-arm
//! fail-CLOSED contract (Lote 10.6bis pattern + ADR-S11-002 split-tier;
//! DSR is regulatory-grade fail-CLOSED; distinct from billing
//! fail-OPEN), the JCS-byte-stable report serialization contract
//! (RFC 8785 — same as S-09 audit chain + S-10 billing replay), and
//! the BLAKE3-signed-report verifies-post-facto contract.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`event::BackendKind`]
//!    `#[non_exhaustive]` 12-arm taxonomy (8 effective + 4
//!    pseudonymized — privacy_model.md §6.2 source-of-truth pós Lote
//!    10.11.0-bis), [`event::ErasureDecision`] `#[non_exhaustive]`
//!    6-arm taxonomy (Started / VerifiedComplete / VerifiedPartial /
//!    VerificationFailed / SlaBreached / Rejected),
//!    [`event::BackendErasureOutcome`] `#[non_exhaustive]` 5-arm
//!    taxonomy (Erased / Pseudonymized / PartialFailure / Failed /
//!    NotApplicable) aligned with sprint contract §5.2 R-S11-6 status
//!    enum, [`event::ErasureCloudEventType`] 5-arm canonical
//!    CloudEvents type taxonomy
//!    (`dev.hugr.corelink.dsr.erasure.{started, backend_completed,
//!    verification_passed, verification_failed, completed}.v1`),
//!    [`event::ErasureRequest`] (the canonical input shape: dsr_id +
//!    tenant_id + subject_id + erasure_salt 32 bytes + queued_at_ms
//!    + legal_hold), [`event::ErasurePlan`] (per-backend ledger entry
//!    list ordered by the canonical 12-arm enum),
//!    [`event::BackendCompletion`] (per-backend completion record
//!    stored in the canonical D1 dsr_erasure_log tombstone), and the
//!    canonical SLA constants [`event::VERIFICATION_SLA_HOURS`] = 24
//!    + [`event::ERASURE_SLA_DAYS`] = 30.
//! 2. The [`audit_emit`] module ships [`audit_emit::ErasureAuditRecord`]
//!    + [`audit_emit::ErasureAuditSink`] trait +
//!    [`audit_emit::InMemoryErasureAuditSink`] capture sink +
//!    [`audit_emit::FailingErasureAuditSink`] (the canonical
//!    fail-CLOSED envelope per ADR-S11-002 split-tier — DSR is
//!    regulatory-grade, NEVER tolerates silent loss; distinct from
//!    billing fail-OPEN at Lote 10.6bis split-tier).
//! 3. The [`pseudonymize`] module re-exports the canonical
//!    `corelink-privacy-pseudonymize` helper —
//!    `sha256(subject_id || erasure_salt)` + marker
//!    `pii_redacted=true` insertion canonical per GDPR Recital 26 +
//!    Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization
//!    techniques) escape valve; SHA-256 chosen over BLAKE3 per WI §9.3
//!    DD-002 trade-off.
//! 4. The [`backends`] module ships [`backends::BackendErasureAdapter`]
//!    trait + [`backends::InMemoryBackendErasureAdapter`] (the
//!    canonical per-backend adapter — every backend exposes the same
//!    trait surface so the orchestrator fans out 12-deep without
//!    per-backend special-case branching) +
//!    [`backends::canonical_in_memory_adapters`] (the 12-arm
//!    canonical list). Each backend exposes
//!    `verification_hash() -> [u8; 32]` (BLAKE3 of remaining-rows-for-
//!    tenant fingerprint); erasure complete when the per-backend hash
//!    equals the canonical
//!    [`backends::CANONICAL_EMPTY_TENANT_HASH`] sentinel.
//! 5. The [`report`] module ships [`report::ErasureReport`] (the
//!    JCS-canonical signed report payload per RFC 8785 — consistent
//!    with S-09 audit chain + S-10 billing replay), the
//!    [`report::ReportSigner`] trait + [`report::InMemoryReportSigner`]
//!    deterministic in-memory fake (BLAKE3-256 keyed-hash MAC over the
//!    JCS preimage so verify-post-facto is byte-identical to a
//!    production keyed-BLAKE3 verify), and the canonical R2 object-key
//!    helper [`report::canonical_report_key`] formatting
//!    `dsr-reports/{tenant}/{dsr_id}/erasure-report.json`.
//! 6. The [`orchestrator`] module ships [`orchestrator::ErasureWorker`]
//!    trait + [`orchestrator::InMemoryErasureWorker`] orchestrator
//!    (run pipeline: audit `started` BEFORE plan emission → for each
//!    canonical backend: idempotency lookup-first → audit
//!    `backend_completed` BEFORE backend mutation + insert tombstone
//!    in idempotency ledger → audit `verification_passed` /
//!    `verification_failed` BEFORE the canonical decision return →
//!    audit `completed` BEFORE the dsr_tickets.status transition)
//!    plus the per-instance `Arc<Mutex<()>>` F-001 closure.
//! 7. The [`idempotency`] module ships
//!    [`idempotency::ErasureIdempotencyLedger`] trait +
//!    [`idempotency::InMemoryErasureIdempotencyLedger`] +
//!    [`idempotency::FailingErasureIdempotencyLedger`] (the canonical
//!    D1 `dsr_erasure_log` tombstone surface; production wiring at
//!    WI-S11-008 binds this to the additive D1 migration
//!    `migrations/d1/0022_dsr_erasure_log.sql` with UNIQUE
//!    `(dsr_id, backend)` constraint per WI §6.1.7). Replay-safe per
//!    PAT-RETRY-IDEMPOTENT-001 (sprint contract §9 14.s11.4).
//! 8. The [`verification_job`] module ships
//!    [`verification_job::VerificationJob`] entry point +
//!    [`verification_job::VerificationOutcome`] result shape +
//!    [`verification_job::elapsed_dsr_ids`] cron tick helper.
//! 9. The [`error`] module ships the canonical
//!    [`error::ErasureWorkerError`] `#[non_exhaustive]` taxonomy
//!    (audit / backend / idempotency / report / config / internal) +
//!    per-component error enums.
//!
//! # Why `trait + fake` here, real CF Queue + 12 backend bindings + 24h
//! # cron + R2 evidence-dsr at WI-S11-008
//!
//! S-11 lands without Cloudflare Queue + Neon production cluster + R2
//! buckets + Stripe API key + Loki cluster wired into CI (no remote +
//! Cloudflare + Neon staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//!
//! - **12-backend fan-out** — every erasure plan has exactly 12 entries
//!   (8 effective + 4 pseudonymized canonical pós Lote 10.11.0-bis) so
//!   no backend is silently skipped on the regulatory-grade pipeline
//!   (multa LGPD 2% revenue / GDPR 4% global on incomplete erasure).
//!   Pinned by `prop_fanout_to_all_12_backends`.
//! - **Idempotent re-erase** — same `dsr_id` re-submitted produces a
//!   byte-identical tombstone; the canonical D1 dsr_erasure_log
//!   `(dsr_id, backend)` UNIQUE constraint short-circuits the second
//!   submission. Pinned by `prop_idempotency_replay_100x`.
//! - **24h SLA breach alerts** — > 24h elapsed on any non-verified
//!   backend fires a SEV-1 alert via the canonical
//!   [`event::ErasureDecision::SlaBreached`] arm.
//! - **Pseudonymization correctness** — every pseudonymized backend
//!   row carries the canonical `pii_redacted=true` marker +
//!   sha256(subject_id || erasure_salt) pseudonym; verification sweep
//!   asserts 100% marker presence.
//! - **Refcount-aware R2 CAS scrub** — blob shared between tenants is
//!   never hard-deleted (cross-tenant break prevented; CRITICAL per
//!   §28 R-003).
//! - **Tenant isolation** — every backend trait surface takes
//!   `tenant_id` first; cross-tenant erasure is structurally
//!   impossible.
//! - **Audit fail-CLOSED envelope** — every decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   pipeline fail-CLOSED at the trait surface (per S-06 P0-2 / S-07
//!   P1-1 lessons absorbed).
//! - **JCS report byte-stable** — the report serialization is RFC
//!   8785 canonical so the auditor evidence trail is byte-deterministic
//!   across re-runs (consistent with S-09 audit chain + S-10 billing
//!   replay). Pinned by `prop_jcs_report_byte_stable`.
//! - **BLAKE3-signed report verifies post-facto** — tampered reports
//!   reject; wrong key rejects.
//!
//! The live Cloudflare Durable Object queue consumer (Lote 10.7bis R5
//! P0-3: `worker::send_future`; NEVER `tokio::spawn`), 12 backend
//! bindings, R2 audit Object Lock 7y HKDF info=`corelink/v1/audit-
//! pseudonym` pseudonymization, 24h cron worker verification sweep,
//! BLAKE3-keyed report signing key from KMS via HKDF
//! info=`corelink/v1/erasure-report` (security_model.md §7.2 +
//! key_management.md §2), R2 evidence-dsr signed URL 24h TTL,
//! RB-DSR-ERASURE-INCOMPLETE runbook activation, FM-450 + FM-452
//! SEV-1 alert wiring, and the full 30d SLO-FRESH-DSR-ERASURE
//! observation budget all run alongside WI-S11-008 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-DATA-ERASURE-COMPLETE** (CRITICAL — Lote 10.11.0-bis:
//!   HIGH→CRITICAL with TLA+ commit S-11 WI-S11-008;
//!   invariant_registry.md §3.5 L110): erasure cross-backend is
//!   effective on 12/12 canonical backends (8 effective + 4
//!   pseudonymized).
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL,
//!   invariant_registry.md §3.6 L116): every decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   request + propagates as
//!   [`error::ErasureWorkerError::Audit`].
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from
//!   invariant_registry.md §3.7): the canonical
//!   [`backends::BackendErasureAdapter::erase`] surface is keyed by
//!   `(tenant_id, ...)`; cross-tenant mutation structurally
//!   impossible.
//! - **24h-verification-sla** (LGPD Art. 18 §III + GDPR Art. 17.1
//!   "without undue delay" + Loki cold archive settle ≤24h per S-09
//!   R-S09-5): the canonical verification window is 24h post-plan;
//!   > 24h elapsed on any non-verified backend fires a SEV-1 alert
//!   per FM-450.
//! - **PAT-RETRY-IDEMPOTENT-001** (sprint contract §9 14.s11.4):
//!   replay-safe; same `dsr_id` re-submitted produces byte-identical
//!   tombstone.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
// Doc comments use the canonical `+ continuation` style (mirrors
// the wider CoreLink doc tradition where typed-enum surfaces list
// canonical taxonomies). Clippy's `doc_lazy_continuation` flags any
// `+ ` line continuation as a list item; the explicit allow keeps
// the existing doc style stable across the crate.
#![allow(clippy::doc_lazy_continuation)]

pub mod audit_emit;
pub mod backends;
pub mod error;
pub mod event;
pub mod idempotency;
pub mod orchestrator;
pub mod pseudonymize;
pub mod report;
pub mod verification_job;

pub use audit_emit::{
    ErasureAuditRecord, ErasureAuditSink, FailingErasureAuditSink, InMemoryErasureAuditSink,
};
pub use backends::{
    canonical_in_memory_adapters, BackendErasureAdapter, InMemoryBackendErasureAdapter,
    InMemoryRow, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
pub use error::{
    ErasureAuditSinkError, ErasureBackendError, ErasureIdempotencyError, ErasureReportError,
    ErasureWorkerError,
};
pub use event::{
    canonical_backend_kinds, canonical_cloudevent_types, BackendCompletion, BackendErasureOutcome,
    BackendKind, ErasureCloudEventType, ErasureDecision, ErasurePlan, ErasurePlanEntry,
    ErasureRequest, ErasureSalt, BACKEND_COUNT, EFFECTIVE_BACKEND_COUNT, ERASURE_SLA_DAYS,
    PSEUDONYMIZED_BACKEND_COUNT, VERIFICATION_SLA_HOURS, VERIFICATION_SLA_MS,
};
pub use idempotency::{
    ErasureIdempotencyLedger, FailingErasureIdempotencyLedger,
    InMemoryErasureIdempotencyLedger, LedgerOutcome,
};
pub use orchestrator::{ErasureWorker, InMemoryErasureWorker};
pub use report::{
    canonical_bytes as canonical_report_bytes, canonical_report_key, ErasureReport,
    InMemoryReportSigner, ReportSignature, ReportSigner, ReportSignerKey, REPORT_OBJECT_KEY_PREFIX,
    REPORT_SIGNATURE_LEN,
};
pub use verification_job::{
    dsr_resolution_hours, elapsed_dsr_ids, within_sla_window, VerificationJob, VerificationOutcome,
    METRIC_DSR_RESOLUTION_HOURS, SLA_WINDOW_HOURS,
};

/// Crate canonical schema version constant. Pinned for the canonical
/// D1 migration slot at WI-S11-008 (next slot after
/// `migrations/d1/0021_billing_replay_audit.sql`; production wiring
/// at WI-S11-008 lands the additive `0022_dsr_erasure_log.sql` D1
/// migration alongside the trait surface here per WI §6.1.7).
#[must_use]
pub const fn erasure_schema_version() -> u32 {
    22
}
