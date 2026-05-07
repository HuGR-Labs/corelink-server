//! `corelink-erasure` — DSR cross-backend erasure worker (WI-S11-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the DSR erasure cross-backend orchestrator. The trait
//! surfaces match what every production wiring (Cloudflare Queue
//! consumer of `dsr.queued.v1`, Neon multi-tabela DELETE cascade,
//! Neon billing fiscal pseudonymize, R2 CAS refcount-aware soft-delete
//! + GC sweep grace 72h, R2 AC mutable DELETE, D1 row purge, KV key
//! prefix DELETE, Stripe `Customer.update` PII nullify, Loki
//! `/loki/api/v1/delete` API, R2 audit Object Lock 7y HKDF
//! pseudonymization, Neon PITR backup tombstone replay, R2 CAS
//! legal_hold partition pseudonymize, R2 evidence-* buckets
//! pseudonymize, 24h cron worker verification sweep, R2 evidence-dsr
//! signed URL with BLAKE3-signed JCS-canonical erasure-report.json)
//! will satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on.
//!
//! Property tests pinned at 10k iter against the orchestrator cover
//! the fan-out-to-all-12-backends contract, the
//! same-request_id-twice → identical-plan-and-report idempotency
//! contract (PAT-RETRY-IDEMPOTENT-001), the >24h post-plan SLA breach
//! contract (LGPD Art. 18 §III + GDPR Art. 17.1 "without undue delay"),
//! the all-backends-verified → final-hash-matches-canonical-no-rows
//! contract, the any-backend-failure → VerifiedPartial contract, the
//! tenant-A-erasure-never-affects-tenant-B isolation contract
//! (INV-TENANT-ISOLATION canary), the audit-emit-per-decision-arm
//! fail-CLOSED contract (canonical Lote 10.6bis pattern + ADR-S11-002
//! split-tier; DSR is regulatory-grade fail-CLOSED; distinct from
//! billing fail-OPEN), the JCS-byte-stable report serialization
//! contract (RFC 8785 — same as S-09 audit chain + S-10 billing
//! replay), and the BLAKE3-signed-report verifies-post-facto contract.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`BackendKind`] `#[non_exhaustive]`
//!    12-arm taxonomy (8 effective + 4 pseudonymized — privacy_model.md
//!    §6.2 source-of-truth pós Lote 10.11.0-bis), [`ErasureDecision`]
//!    `#[non_exhaustive]` 6-arm taxonomy (PlanGenerated /
//!    FanoutInitiated / VerifiedComplete / VerifiedPartial /
//!    VerificationFailed / SlaBreached), [`BackendOutcome`]
//!    `#[non_exhaustive]` 5-arm taxonomy (Erased / Pseudonymized /
//!    PartialFailure / Failed / NotApplicable) aligned with sprint
//!    contract §5.2 R-S11-6 status enum, [`ErasureRequest`] (the
//!    canonical input shape: dsr_id UUIDv7 + tenant_id + subject_id +
//!    erasure_salt 32 bytes + queued_at_ms),
//!    [`ErasurePlan`] (per-backend ledger entry list ordered by the
//!    canonical 12-arm enum), [`BackendCompletion`] (per-backend
//!    completion record stored in the canonical D1 dsr_erasure_log
//!    tombstone), and the canonical SLA constants
//!    [`VERIFICATION_SLA_HOURS`] = 24 + [`ERASURE_SLA_DAYS`] = 30.
//! 2. The [`audit`] module ships [`audit::ErasureAuditEventType`]
//!    `#[non_exhaustive]` 8-event taxonomy:
//!    `corelink.erasure.{plan_generated, fanout_initiated,
//!    backend_completed, verification_started, verified_complete,
//!    verified_partial, sla_breached, report_generated}` +
//!    [`audit::ErasureAuditRecord`] + [`audit::ErasureAuditSink`]
//!    trait + [`audit::InMemoryErasureAuditSink`] capture sink +
//!    [`audit::FailingErasureAuditSink`] (the canonical fail-CLOSED
//!    envelope per ADR-S11-002 split-tier — DSR is regulatory-grade,
//!    NEVER tolerates silent loss; distinct from billing fail-OPEN at
//!    Lote 10.6bis split-tier).
//! 3. The [`pseudonymize`] module ships [`pseudonymize::pseudonymize`]
//!    helper — `sha256(subject_id || erasure_salt)` + marker
//!    `pii_redacted=true` insertion canonical per GDPR Recital 26 +
//!    Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization
//!    techniques) escape valve; SHA-256 chosen over BLAKE3 per WI §9.3
//!    DD-002 trade-off (NIST FIPS 180-4 regulatory defensibility wins
//!    Privacy Officer signature in ADR docs).
//! 4. The [`backend`] module ships [`backend::ErasureBackend`] trait +
//!    [`backend::InMemoryErasureBackend`] (the canonical per-backend
//!    adapter — every backend exposes the same trait surface so the
//!    orchestrator fans out 12-deep without per-backend special-case
//!    branching) + [`backend::FailingErasureBackend`] (always-failing
//!    fixture for adversarial fail-CLOSED tests) +
//!    [`backend::canonical_backends`] (the 12-arm canonical list).
//!    Each backend exposes `verification_hash() -> [u8; 32]` (BLAKE3
//!    of remaining-rows-for-tenant fingerprint); erasure complete when
//!    the per-backend hash equals the canonical "no rows for tenant"
//!    sentinel ([`backend::CANONICAL_EMPTY_TENANT_HASH`]).
//! 5. The [`report`] module ships [`report::ErasureReport`] (the
//!    JCS-canonical signed report payload per RFC 8785 — consistent
//!    with S-09 audit chain + S-10 billing replay), the
//!    [`report::ReportSigner`] trait + [`report::InMemoryReportSigner`]
//!    deterministic in-memory fake (BLAKE3-256 keyed-hash MAC over the
//!    JCS preimage so verify-post-facto is byte-identical to a
//!    production keyed-BLAKE3 verify), and the canonical R2
//!    object-key helper [`report::canonical_report_key`] formatting
//!    `dsr-reports/{tenant}/{request_id}/erasure-report.json`.
//! 6. The [`worker`] module ships [`worker::ErasureWorker`] trait +
//!    [`worker::InMemoryErasureWorker`] orchestrator (run pipeline:
//!    audit `plan_generated` BEFORE plan emission → audit
//!    `fanout_initiated` BEFORE backend fan-out → per-backend
//!    completion: audit `backend_completed` BEFORE backend mutation +
//!    insert tombstone in idempotency ledger → audit
//!    `verification_started` BEFORE the 24h verify sweep → on success
//!    audit `verified_complete` BEFORE returning the canonical
//!    [`ErasureDecision::VerifiedComplete`] / on partial-failure audit
//!    `verified_partial` BEFORE returning the canonical
//!    [`ErasureDecision::VerifiedPartial`] arm + SEV-1 alert hook / on
//!    elapsed > 24h audit `sla_breached` BEFORE returning the
//!    canonical [`ErasureDecision::SlaBreached`] arm + SEV-1 alert
//!    hook → audit `report_generated` BEFORE binding the canonical
//!    R2 evidence-dsr signed URL) plus the canonical
//!    [`worker::CANONICAL_VERIFICATION_DEADLINE_MS`] = 24h ×
//!    3_600_000 ms/h constant + per-instance `Arc<Mutex<()>>` F-001
//!    closure.
//! 7. The [`idempotency`] module ships
//!    [`idempotency::ErasureIdempotencyLedger`] trait +
//!    [`idempotency::InMemoryErasureIdempotencyLedger`] +
//!    [`idempotency::FailingErasureIdempotencyLedger`] (the canonical
//!    D1 `dsr_erasure_log` tombstone surface; production wiring at
//!    WI-S11-008 binds this to the additive D1 migration
//!    `migrations/d1/0022_dsr_erasure_log.sql` with UNIQUE
//!    `(dsr_id, backend)` constraint per WI §6.1.7). Replay-safe per
//!    PAT-RETRY-IDEMPOTENT-001 (sprint contract §9 14.s11.4).
//! 8. The [`error`] module ships the canonical [`error::ErasureError`]
//!    `#[non_exhaustive]` taxonomy (audit / backend / idempotency /
//!    report / config / internal) + per-component error enums.
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
//!   byte-identical plan + report; the canonical D1 dsr_erasure_log
//!   `(dsr_id, backend)` UNIQUE constraint short-circuits the second
//!   submission. Pinned by `prop_idempotent_re_erase`.
//! - **24h SLA breach alerts** — the canonical verification window is
//!   24h post-plan; > 24h elapsed on any non-verified backend fires a
//!   SEV-1 alert via the canonical
//!   [`event::ErasureDecision::SlaBreached`] arm. Rationale: LGPD
//!   Art. 18 §III + GDPR Art. 17.1 "without undue delay" + Loki cold
//!   archive settle is up to 24h (S-09 R-S09-5). Pinned by
//!   `prop_24h_sla_breach_alerts`.
//! - **Verification hash zero after complete** — every
//!   InMemoryErasureBackend exposes the per-backend remaining-rows
//!   fingerprint; on a fully-erased tenant the fingerprint equals the
//!   canonical [`backend::CANONICAL_EMPTY_TENANT_HASH`] (BLAKE3 of an
//!   empty row set). The 24h sweep verifies this per-backend; failure
//!   on any backend lands the canonical
//!   [`event::ErasureDecision::VerifiedPartial`] arm. Pinned by
//!   `prop_verification_hash_zero_after_complete`.
//! - **Partial failure flagged** — any single backend failure during
//!   the verify sweep transitions the orchestrator from
//!   `VerifiedComplete` to `VerifiedPartial` (no silent rollback;
//!   regulatory-grade transparency). Pinned by
//!   `prop_partial_failure_flagged`.
//! - **Tenant isolation** — every backend trait surface takes
//!   `tenant_id` first; cross-tenant erasure is structurally
//!   impossible. Pinned by `prop_tenant_isolation`.
//! - **Audit fail-CLOSED envelope** — every decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   pipeline fail-CLOSED at the trait surface. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **JCS report byte-stable** — the report serialization is RFC 8785
//!   canonical so the auditor evidence trail is byte-deterministic
//!   across re-runs (consistent with S-09 audit chain + S-10 billing
//!   replay). Pinned by `prop_jcs_report_byte_stable`.
//! - **Report signature verifies** — the BLAKE3-signed report is
//!   verifiable post-facto via the canonical issuer key; tampered
//!   reports reject. Pinned by `prop_report_signature_verifies`.
//!
//! The live `ErasureWorkerDO` Cloudflare Durable Object queue consumer
//! (Lote 10.7bis R5 P0-3: `worker::send_future`; NEVER `tokio::spawn`
//! in CF Workers), 12 backend bindings (Neon HTTP via wasm-bindgen,
//! R2 DeleteObject / refcount-aware S-07 dedup integration, D1 row
//! purge, KV key prefix DELETE, Stripe `Customer.update` via S-10
//! StripeClient wrapper, Loki HTTP delete API), R2 audit Object Lock
//! 7y HKDF info=`corelink/v1/audit-pseudonym` pseudonymization,
//! 24h cron worker verification sweep, BLAKE3-keyed report signing
//! key from KMS via HKDF info=`corelink/v1/erasure-report` (security
//! _model.md §7.2 + key_management.md §2), R2 evidence-dsr signed URL
//! 24h TTL, RB-DSR-ERASURE-INCOMPLETE runbook activation, FM-450 +
//! FM-452 SEV-1 alert wiring, and the full 30d SLO-FRESH-DSR-ERASURE
//! observation budget all run alongside WI-S11-008 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-DATA-ERASURE-COMPLETE** (CRITICAL — Lote 10.11.0-bis:
//!   HIGH→CRITICAL with TLA+ commit S-11 WI-S11-008;
//!   invariant_registry.md §3.5 L110): erasure cross-backend is
//!   effective on 12/12 canonical backends (8 effective + 4
//!   pseudonymized); 0 records remaining 30d post-request (effective)
//!   or 100% pseudonymized (Object Lock). Pinned by
//!   `prop_fanout_to_all_12_backends` +
//!   `prop_verification_hash_zero_after_complete`.
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL,
//!   invariant_registry.md §3.6 L116): every decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   request + propagates as [`error::ErasureError::Audit`].
//!   Pseudonymization preserves audit immutability via secondary
//!   index update (never deletes the original audit row). Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from
//!   invariant_registry.md §3.7): the canonical
//!   [`backend::ErasureBackend::erase`] surface is keyed by
//!   `(tenant_id, ...)`; cross-tenant mutation structurally
//!   impossible. Pinned by `prop_tenant_isolation`.
//! - **24h-verification-sla** (LGPD Art. 18 §III + GDPR Art. 17.1
//!   "without undue delay" + Loki cold archive settle ≤24h per S-09
//!   R-S09-5): the canonical verification window is 24h post-plan;
//!   > 24h elapsed on any non-verified backend fires a SEV-1 alert
//!   per FM-450. Pinned by `prop_24h_sla_breach_alerts`.
//! - **PAT-RETRY-IDEMPOTENT-001** (sprint contract §9 14.s11.4):
//!   replay-safe; same `dsr_id` re-submitted produces byte-identical
//!   plan + report. Pinned by `prop_idempotent_re_erase`.
//!
//! # Production wiring (deferred to WI-S11-008)
//!
//! - `ErasureWorkerDO` Cloudflare Durable Object queue consumer at
//!   the canonical `dsr.queued.v1` payload (Lote 10.7bis R5 P0-3:
//!   `worker::send_future`; NEVER `tokio::spawn`).
//! - 12 backend bindings: Neon multi-tabela DELETE cascade (HTTP via
//!   wasm-bindgen), Neon billing fiscal pseudonymize, R2 CAS
//!   refcount-aware soft-delete + S-07 dedup integration, R2 AC
//!   DELETE, D1 blob_meta + ac_meta row purge, KV key prefix DELETE,
//!   Stripe `Customer.update` via S-10 StripeClient wrapper (NOT
//!   delete; PCI scope GAAP ASC 606 + LGPD Art. 16 fiscal), Loki
//!   `/loki/api/v1/delete` HTTP API, R2 audit Object Lock 7y HKDF
//!   info=`corelink/v1/audit-pseudonym` pseudonymization, Neon PITR
//!   30d tombstone replay, R2 CAS legal_hold partition governance
//!   mode pseudonymize, R2 evidence-* buckets 7y pseudonymize.
//! - Real BLAKE3-keyed report signing key from KMS via HKDF
//!   info=`corelink/v1/erasure-report` (security_model.md §7.2 +
//!   key_management.md §2).
//! - Real D1 `dsr_erasure_log` tombstone (canonical migration
//!   `migrations/d1/0022_dsr_erasure_log.sql` per WI §6.1.7 +
//!   data_model.md §4.1; UNIQUE `(dsr_id, backend)` constraint).
//! - Real R2 evidence-dsr-`<region>` signed URL 24h TTL.
//! - 24h cron worker verification sweep gated by feature flag
//!   `dsr_erasure_verification_enabled` in DO config-singleton.
//! - 8 canonical CloudEvents types fan-out to S-09 audit-chain
//!   extension (every erasure decision arm is a separate `ChainEvent`
//!   per INV-OBS-AUDIT-CHAIN-INTEGRITY).
//! - SEV-1 alert wiring on FM-450 erasure-incomplete cross-backend +
//!   FM-452 consent-tampering-detected.
//! - RB-DSR-ERASURE-INCOMPLETE + RB-CONSENT-TAMPERING runbook
//!   activations.
//! - 30d staging chaos budget + 90d SLO-FRESH-DSR-ERASURE 99% ≤30d
//!   sustainment + DLP CI scan gate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod backend;
pub mod error;
pub mod event;
pub mod idempotency;
pub mod pseudonymize;
pub mod report;
pub mod worker;

pub use audit::{
    canonical_erasure_audit_event_strings, ErasureAuditEventType, ErasureAuditRecord,
    ErasureAuditSink, FailingErasureAuditSink, InMemoryErasureAuditSink,
};
pub use backend::{
    canonical_backends, ErasureBackend, FailingErasureBackend, InMemoryErasureBackend,
    CANONICAL_EMPTY_TENANT_HASH,
};
pub use error::{
    ErasureAuditSinkError, ErasureBackendError, ErasureError, ErasureIdempotencyError,
    ErasureReportError,
};
pub use event::{
    canonical_backend_kinds, BackendCompletion, BackendKind, BackendOutcome, ErasureDecision,
    ErasurePlan, ErasurePlanEntry, ErasureRequest, ErasureSalt, BACKEND_COUNT,
    EFFECTIVE_BACKEND_COUNT, ERASURE_SLA_DAYS, PSEUDONYMIZED_BACKEND_COUNT,
    VERIFICATION_SLA_HOURS, VERIFICATION_SLA_MS,
};
pub use idempotency::{
    ErasureIdempotencyLedger, FailingErasureIdempotencyLedger, InMemoryErasureIdempotencyLedger,
    LedgerOutcome,
};
pub use pseudonymize::{pseudonymize, pseudonymize_subject_id, PSEUDONYM_HEX_LEN};
pub use report::{
    canonical_report_key, ErasureReport, InMemoryReportSigner, ReportSigner, ReportSignerKey,
    ReportSignature, REPORT_SIGNATURE_LEN,
};
pub use worker::{
    ErasureWorker, InMemoryErasureWorker, CANONICAL_VERIFICATION_DEADLINE_MS,
    CANONICAL_VERIFICATION_DEADLINE_SECS,
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
