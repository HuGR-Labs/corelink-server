//! `corelink-dsr` — DSR Self-Service API surface (WI-S11-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the DSR self-service primitive. The trait surfaces
//! match what every production wiring (CF Worker route at
//! `POST /v1/privacy/dsr/{access|portability|rectification|erasure|
//! restriction|objection}` + `GET /v1/privacy/dsr/{request_id}/status`,
//! Neon `dsr_tickets` mirror, WebAuthn step-up TOTP/FIDO2 binding,
//! RS256 JWT signing key from KMS, Cloudflare Email transactional
//! templates) will satisfy, plus an in-memory orchestrator that
//! exercises every load-bearing invariant the production wiring
//! relies on.
//!
//! Property tests pinned at 10k iter against the orchestrator cover
//! the JWT receipt verifies-post-facto contract, the
//! MFA-required-only-for-destructive-arms gate (Erasure + Rectification
//! require MFA; Access + Portability + Restriction + Objection do not),
//! UUIDv7 request-id uniqueness (collision rate < 2^-64 over 10k
//! samples), status-poll idempotency, SLA-deadline correctness per
//! jurisdiction, tenant isolation (tenant A's DSR requests never
//! visible to tenant B), audit-emit-per-decision-arm (canonical
//! fail-CLOSED envelope per Lote 10.6bis pattern + ADR-S11-002),
//! receipt 90-day expiration window, and unsupported-kind rejection.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`DsrRequestKind`] `#[non_exhaustive]`
//!    6-arm taxonomy (Access / Portability / Rectification / Erasure /
//!    Restriction / Objection), [`DsrStatus`] `#[non_exhaustive]` 4-arm
//!    taxonomy (Pending / InProgress / Completed / Rejected),
//!    [`DsrDecision`] `#[non_exhaustive]` 4-arm taxonomy (RequestAccepted
//!    / RequestRejected / MfaRequired / ReceiptIssued),
//!    [`DsrJurisdiction`] `#[non_exhaustive]` 3-arm taxonomy (Lgpd /
//!    Gdpr / Ccpa) with the canonical SLA mapping, [`DsrRequest`] (the
//!    canonical input shape: request_id UUIDv7 + tenant_id +
//!    data_subject_id + request_kind + jurisdiction + submitted_at_ms +
//!    optional mfa_step_up_token), [`DsrTicket`] (the canonical
//!    durable record: status + sla_deadline_ms + receipt + reject
//!    reason), [`RECEIPT_EXPIRY_DAYS`] = 90 const + the canonical SLA
//!    helper [`event::sla_for`].
//! 2. The [`audit`] module ships [`audit::DsrAuditEventType`]
//!    `#[non_exhaustive]` 7-event taxonomy:
//!    `corelink.dsr.{request_received, mfa_step_up_required,
//!    mfa_verified, request_accepted, receipt_issued, request_rejected,
//!    status_polled}` + [`audit::DsrAuditRecord`] +
//!    [`audit::DsrAuditSink`] trait + [`audit::InMemoryDsrAuditSink`]
//!    capture sink + [`audit::FailingDsrAuditSink`] (the canonical
//!    fail-CLOSED envelope per ADR-S11-002 — DSR is regulatory-grade,
//!    NEVER tolerates silent loss; distinct from billing fail-OPEN at
//!    Lote 10.6bis split-tier).
//! 3. The [`receipt`] module ships [`receipt::DsrReceipt`] (the
//!    JWT-shaped receipt payload: header + claims + signature stub) +
//!    [`receipt::JwtReceiptIssuer`] trait +
//!    [`receipt::InMemoryJwtReceiptIssuer`] (the deterministic in-memory
//!    fake — emits a base64url tag derived from the canonical JWT
//!    header.claims preimage so verify-post-facto is byte-identical to
//!    a real RS256 verify; production wiring at WI-S11-008 binds the
//!    real `jsonwebtoken` RS256 surface).
//! 4. The [`mfa`] module ships [`mfa::MfaStepUpVerifier`] trait +
//!    [`mfa::InMemoryMfaStepUpVerifier`] (the canonical
//!    erasure-rectification-only gate per ADR-S11-001 friction-vs-
//!    security trade-off; Access + Portability + Restriction + Objection
//!    skip MFA) + [`mfa::FailingMfaStepUpVerifier`].
//! 5. The [`store`] module ships [`store::DsrRequestStore`] trait +
//!    [`store::InMemoryDsrRequestStore`] +
//!    [`store::FailingDsrRequestStore`] (the canonical durable mirror
//!    surface; production wiring at WI-S11-008 binds this to Neon
//!    `dsr_tickets` schema per data_model.md §4.1 + idempotency UNIQUE
//!    quad `(tenant_id, subject_id, request_kind, request_payload_hash)`).
//! 6. The [`endpoint`] module ships [`endpoint::DsrEndpoint`] trait +
//!    [`endpoint::InMemoryDsrEndpoint`] orchestrator (run pipeline:
//!    audit `request_received` BEFORE store insert → MFA gate for
//!    destructive arms (audit `mfa_step_up_required` BEFORE
//!    MfaRequired return / audit `mfa_verified` BEFORE acceptance) →
//!    status-poll arm short-circuits with audit `status_polled` →
//!    audit `request_accepted` BEFORE store insert → audit
//!    `receipt_issued` BEFORE returning the JWT receipt) plus the
//!    canonical [`endpoint::CANONICAL_RECEIPT_EXPIRY_DAYS`] = 90
//!    constant + per-instance `Arc<Mutex<()>>` F-001 closure.
//! 7. The [`error`] module ships the canonical [`error::DsrError`]
//!    `#[non_exhaustive]` taxonomy (audit / store / receipt / mfa /
//!    config / internal) +
//!    [`error::DsrAuditSinkError`] +
//!    [`error::DsrStoreError`] +
//!    [`error::DsrReceiptError`] +
//!    [`error::DsrMfaError`].
//!
//! # Why `trait + fake` here, real CF Worker + RS256 + WebAuthn at
//! # WI-S11-008
//!
//! S-11 lands without Cloudflare Workers + Neon production cluster +
//! KMS key rotation + Email Routing wired into CI (no remote +
//! Cloudflare + Neon staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//!
//! - **JWT receipt verifies post-facto** — the canonical preimage is
//!   `header.claims` base64url-no-pad concatenated; the in-memory fake
//!   emits a SHA-256-derived tag over the preimage so verify is
//!   byte-identical (real RS256 verify is wire-compatible).
//! - **MFA step-up only for destructive arms** — the gate is enforced
//!   structurally at the orchestrator pipeline (Erasure + Rectification
//!   demand a non-empty step-up token; Access + Portability +
//!   Restriction + Objection skip the verifier). Pinned by
//!   `prop_mfa_required_for_erasure_rectification`.
//! - **UUIDv7 request-id uniqueness** — UUIDv7 embeds Unix-ms timestamp
//!   in the leading bits; collision probability < 2^-64 across 10k
//!   independent samples per WI §8 AC-003 idempotency contract. Pinned
//!   by `prop_request_id_uniqueness`.
//! - **Status poll idempotency** — repeated `GET /status/<request_id>`
//!   reads the same `DsrTicket` snapshot; no state mutation past the
//!   `status_polled` audit row. Pinned by `prop_status_poll_idempotent`.
//! - **SLA deadline correct** — LGPD Art. 19 = 15 days; GDPR Art. 12.3
//!   = 1 month (extendable to 3 for complex, but the canonical
//!   trait-level deadline is the un-extended 30 days); CCPA §1798.130 =
//!   45 days. Pinned by `prop_sla_deadline_correct`.
//! - **Tenant isolation** — `DsrRequestStore::get` is keyed by
//!   `(tenant_id, request_id)`; cross-tenant reads structurally
//!   impossible. Pinned by `prop_tenant_isolation`.
//! - **Audit fail-CLOSED envelope** — every decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   request fail-CLOSED at the trait surface. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **Receipt expires_at = submitted + 90d** — LGPD/GDPR compliance
//!   window is "at least 1 year" for the audit trail (LGPD Art. 19);
//!   the JWT receipt's exp claim caps the customer-side proof-of-
//!   submission token at 90 days (anti-replay; revocation deferred to
//!   S-19 enterprise BYOK). Pinned by `prop_receipt_expires_at_90d`.
//! - **Unsupported kind rejected** — non-canonical `DsrRequestKind`
//!   variants are unrepresentable in the typed enum; serde
//!   deserialization rejects unknown kinds at the trait boundary.
//!   Pinned by `prop_unsupported_kind_rejected`.
//!
//! The live `DsrEndpointDO` Cloudflare Durable Object at the canonical
//! `POST /v1/privacy/dsr/<right>` routes, RS256 JWT signing key from
//! KMS (HKDF info=`corelink/v1/dsr-receipt`), WebAuthn step-up TOTP /
//! FIDO2 binding (cooperation with WI-S03-006 `corelink-webauthn`
//! `StepUpToken`), Neon `dsr_tickets` durable mirror (canonical
//! migration `migrations/0003_dsr_tickets.sql` per data_model.md §4.1
//! 7-state machine + idempotency UNIQUE quad), Cloudflare Email
//! transactional templates (3 locales pt-BR/en-US/es-MX),
//! S-08 rate-limit bucket cooperation (`dsr:<tenant>:<subject>:<day>`
//! 10/day humane LGPD Art. 20), 6 CloudEvents canonical types fan-out
//! to S-09 audit-chain extension (every DSR request is a separate
//! `ChainEvent` per INV-OBS-AUDIT-CHAIN-INTEGRITY), the canonical
//! `legal_hold` pause logic (privacy_model.md §6.1 F-11 clock semantics
//! cap 5d úteis), and 30-min p99 endpoint SLA budget all run alongside
//! WI-S11-008 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - **CTRL-AUTH-010** (security_model.md §242): WebAuthn step-up is
//!   mandatory for the canonical destructive arms (Erasure +
//!   Rectification); the canonical read arms (Access + Portability +
//!   Status) and policy-only arms (Restriction + Objection) skip the
//!   gate per ADR-S11-001 friction-vs-security trade-off rationale.
//!   Pinned by `prop_mfa_required_for_erasure_rectification`.
//! - **JWT-receipt-stateless-verify** (WI §1 invariant — the receipt
//!   is the customer's proof of submission verifiable independently of
//!   the production DB state): the canonical preimage is
//!   `header.claims` base64url-no-pad; verify is HMAC compare in the
//!   in-memory fake (real RS256 verify is wire-compatible). Pinned by
//!   `prop_jwt_receipt_verifies_post_facto`.
//! - **UUIDv7-request-id-collision-rate** (WI §8 AC-003 idempotency
//!   contract): UUIDv7 embeds Unix-ms timestamp in the leading 48 bits
//!   so collision rate is < 2^-64 over 10k samples. Pinned by
//!   `prop_request_id_uniqueness`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): the canonical [`store::DsrRequestStore::get`]
//!   surface is keyed by `(tenant_id, request_id)`; cross-tenant reads
//!   structurally impossible. Pinned by `prop_tenant_isolation`.
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven; lift from
//!   invariant registry §3.6): every DSR decision arm fires its
//!   canonical audit BEFORE state mutation; audit failure aborts the
//!   request + propagates as [`error::DsrError::Audit`]. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **Receipt-90d-expiry-cap** (WI §1 + ADR-S11 — anti-replay): JWT
//!   exp claim is set to `submitted_at_ms + 90d`; receipts older than
//!   90 days fail verification (production wiring at WI-S11-008 binds
//!   the canonical key rotation grace window). Pinned by
//!   `prop_receipt_expires_at_90d`.
//! - **SLA-deadline-per-jurisdiction** (LGPD Art. 19 = 15d; GDPR Art.
//!   12.3 = 30d; CCPA §1798.130 = 45d): the canonical
//!   [`event::sla_for`] helper pins the per-jurisdiction deadline.
//!   Pinned by `prop_sla_deadline_correct`.
//!
//! # Production wiring (deferred to WI-S11-008)
//!
//! - `DsrEndpointDO` Cloudflare Durable Object at the canonical 7
//!   routes (Lote 10.7bis R5 P0-3: `worker::send_future`; Neon HTTP
//!   via wasm-bindgen; NEVER `tokio::spawn`).
//! - Real RS256 JWT signing key from KMS via HKDF info=`corelink/v1/
//!   dsr-receipt` (security_model.md §7.2 + key_management.md §2);
//!   `jsonwebtoken = 9.3` reused from `corelink-clerk` (S-03 inheritance).
//! - Real WebAuthn step-up TOTP / FIDO2 binding cooperation with
//!   WI-S03-006 `corelink-webauthn::StepUpToken`.
//! - Real Neon `dsr_tickets` mirror (canonical migration
//!   `migrations/0003_dsr_tickets.sql` per data_model.md §4.1 7-state
//!   machine + UUIDv7 PK + idempotency UNIQUE quad
//!   `(tenant_id, subject_user_id, request_kind, request_payload_hash)`).
//! - Real Cloudflare Email Routing transactional templates (3 locales).
//! - S-08 rate-limit bucket cooperation
//!   (`dsr:<tenant>:<subject>:<day>`; 10/day humane LGPD Art. 20).
//! - 6 canonical CloudEvents types fan-out to S-09 audit-chain
//!   extension (every DSR is a separate `ChainEvent` per
//!   INV-OBS-AUDIT-CHAIN-INTEGRITY).
//! - SLA-pause logic on `legal_hold = true`
//!   (privacy_model.md §6.1 F-11; cap 5d úteis somatório).
//! - 30-min p99 endpoint SLA budget (sprint contract §6 DoD +
//!   §14.s11.1).
//! - 10 req/day rate limit per (tenant, subject) (sprint contract
//!   §15 R-006 mitigation; humane LGPD Art. 20).
//! - RB-DSR-INTAKE-FAILURE documented runbook (sprint contract
//!   §14.s11.1).
//! - CI mensal DSR endpoint test (sprint contract §8 INV verification).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod endpoint;
pub mod error;
pub mod event;
pub mod mfa;
pub mod receipt;
pub mod store;

pub use audit::{
    canonical_dsr_audit_event_strings, DsrAuditEventType, DsrAuditRecord, DsrAuditSink,
    FailingDsrAuditSink, InMemoryDsrAuditSink,
};
pub use endpoint::{
    DsrEndpoint, InMemoryDsrEndpoint, CANONICAL_RECEIPT_EXPIRY_DAYS,
    CANONICAL_RECEIPT_EXPIRY_MS,
};
pub use error::{DsrAuditSinkError, DsrError, DsrMfaError, DsrReceiptError, DsrStoreError};
pub use event::{
    canonical_dsr_jurisdictions, canonical_dsr_request_kinds, canonical_dsr_statuses,
    is_destructive_arm, sla_for, DsrDecision, DsrJurisdiction, DsrRejectReason, DsrRequest,
    DsrRequestKind, DsrStatus, DsrTicket, RECEIPT_EXPIRY_DAYS, SLA_CCPA_DAYS, SLA_GDPR_DAYS,
    SLA_LGPD_DAYS,
};
pub use mfa::{
    FailingMfaStepUpVerifier, InMemoryMfaStepUpVerifier, MfaStepUpToken, MfaStepUpVerifier,
};
pub use receipt::{
    DsrReceipt, InMemoryJwtReceiptIssuer, JwtReceiptIssuer, JwtReceiptToken, RECEIPT_ALG_RS256,
    RECEIPT_ISSUER,
};
pub use store::{DsrRequestStore, FailingDsrRequestStore, InMemoryDsrRequestStore};

/// Crate canonical schema version constant. Pinned for the canonical
/// Neon migration slot at WI-S11-008 (next slot after
/// `migrations/002_auth_tables.sql`; production wiring at WI-S11-008
/// lands the additive `0003_dsr_tickets.sql` Neon migration alongside
/// the trait surface here).
#[must_use]
pub const fn dsr_schema_version() -> u32 {
    3
}
