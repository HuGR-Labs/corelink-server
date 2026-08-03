//! `corelink-privacy-consent-ledger` — Consent Ledger proof-of-informed
//! regulatory cornerstone (WI-S11-003).
//!
//! # ⛔ NOT WIRED — and the wire-up shape is CONTESTED (verified 2026-08-03)
//!
//! Nothing below is reachable in production. `corelink-privacy` is not a
//! dependency of `corelink-container` or of any Worker, so this consent
//! logic is not compiled into anything deployed; no `.route("…")` mount in
//! `crates/` and no branch in `worker/src/index.ts` serves any
//! `/v1/consent*` path. (A live `401` from `corelink-api.humangr.com` is
//! NOT evidence otherwise — it 401s every `/v1/*` path, known or not.)
//!
//! ⚠️ **Before wiring these traits to routes, read the ⛔ CONSENT block in
//! `tools/openapi/src/lib.rs`.** The endpoint shape named in the paragraph
//! immediately below (`POST /v1/consent/<purpose>`, `DELETE
//! /v1/consent/<purpose>`, `GET /v1/consent`) is what THIS crate and the
//! canonical OpenAPI spec design — but the admin-UI client that is supposed
//! to call it (`apps/admin-ui/src/lib/consent-api.ts`) calls a DIFFERENT
//! surface (`POST /v1/consent/grant`, `POST /v1/consent/{id}/withdraw`,
//! `GET /v1/consent/active`, `GET /v1/consent/history`, `GET
//! /v1/subprocessors`), keying revocation by an opaque `consent_id` rather
//! than by purpose. The two must be reconciled — pick one and change the
//! other — or the backend will ship a surface no client calls. The admin-UI
//! screens are RETIRED (404) until then; see
//! `apps/admin-ui/src/app/[locale]/consent/retired.ts`.
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the Consent Ledger primitive. The trait surfaces match
//! what every production wiring (CF Worker routes at
//! `POST /v1/consent/<purpose>`, `DELETE /v1/consent/<purpose>`,
//! `GET /v1/consent`, `GET /v1/consent/verify`,
//! `GET /v1/consent/revocation/verify`, Neon `consent_ledger` +
//! `consent_revocation` tables, Cloudflare Queue cascade fanout,
//! R2 audit Object Lock 7y, Cloudflare Email transactional templates)
//! will satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on.
//!
//! # GDPR Art. 7 + LGPD Art. 8 alignment
//!
//! "Freely given, specific, informed, unambiguous" consent requires
//! **proof** that the data subject saw (a) the correct text, (b) the
//! correct version, (c) in the correct locale, (d) at a traceable
//! moment. The 6-field [`schema::ConsentProofPayload`] satisfies this
//! per EDPB Guidelines 5/2020.
//!
//! # Schema symmetric grant ↔ revoke (Lote 9.4 Opus H-05)
//!
//! GDPR Art. 7.3 requires revoke "as easy as giving consent". The
//! `consent_revocation` table mirrors `consent_ledger` 6-field proof —
//! revoke carries proof of the notice in force at the time of
//! revocation. Without symmetric proof the revoke is cryptographically
//! weaker than the grant → defensibility gap.
//!
//! # Invariants enforced
//!
//! - **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL, Lote 10.11.0-bis:
//!   HIGH→CRITICAL with TLA+ symmetry, invariant_registry.md §3.12
//!   L168): every grant and revoke carries the canonical 6-field proof +
//!   HMAC-SHA256 tenant-scoped signature; tampering detected via
//!   signature mismatch in the stateless verify endpoint.
//!   Pinned by `prop_hmac_roundtrip`.
//!
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL, invariant_registry.md §3.6
//!   L116): every consent decision arm fires its canonical audit event
//!   BEFORE state mutation; audit failure aborts fail-CLOSED at the
//!   trait surface. Pinned by `chaos_audit_emit_failure`.
//!
//! # Modules
//!
//! 1. [`schema`] — [`schema::ConsentPurpose`] 12-arm closed enum
//!    (privacy_model.md §5.6.1; ADR-S11-006 cardinality discipline),
//!    [`schema::LegalBasis`] 4-arm enum, [`schema::ConsentProofPayload`]
//!    6-field canonical proof, [`schema::LocaleBcp47`] 3-locale closed
//!    enum, plus all receipt/response shapes.
//! 2. [`hmac_sign`] — HMAC-SHA256 per-consent signature via
//!    tenant-scoped HKDF (security_model.md §374; HKDF
//!    info=`corelink/v1/consent-hmac`); [`hmac_sign::ConsentHmacSigner`]
//!    trait + [`hmac_sign::InMemoryConsentHmacSigner`] fake.
//! 3. [`audit_emit`] — [`audit_emit::ConsentAuditSink`] trait +
//!    [`audit_emit::InMemoryConsentAuditSink`] capture sink +
//!    [`audit_emit::FailingConsentAuditSink`] (fail-CLOSED envelope per
//!    ADR-S11-002 split-tier; consent is regulatory-grade, NEVER
//!    tolerates silent loss).
//! 4. [`store`] — [`store::ConsentStore`] trait +
//!    [`store::InMemoryConsentStore`] + [`store::FailingConsentStore`]
//!    (Neon mirror surface; production wiring at WI-S11-008).
//! 5. [`cascade`] — [`cascade::CascadeSink`] trait +
//!    [`cascade::InMemoryCascadeSink`] (Cloudflare Queue fanout ≤24h
//!    SLA CTRL-PRIV-CONSENT-002; production wiring at WI-S11-008).
//! 6. [`locale_enforce`] — strict Accept-Language vs payload locale
//!    matching (CTRL-PRIV-CONSENT-005; mismatch → 422).
//! 7. [`notice_version_check`] — major version bump detection → force
//!    re-consent flow (30d grace + 60d cap = 90d total; purpose enters
//!    `consent_lapsed` state; NO fail-open swap to legitimate_interest).
//! 8. [`ledger`] — [`ledger::ConsentLedger`] trait +
//!    [`ledger::InMemoryConsentLedger`] orchestrator (run pipeline:
//!    locale_enforce → notice_version_check → idempotency lookup →
//!    audit emit BEFORE store insert → HMAC sign → store insert →
//!    return receipt).
//! 9. [`error`] — [`error::ConsentLedgerError`] `#[non_exhaustive]`
//!    taxonomy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
// S-13 sprint-close P1 cascade: rust 1.88 introduces stricter
// `uninlined_format_args` / `format_in_format_args` lints. Same
// rationale as supply-verify crate: multi-line error templates are
// more readable with intermediate format!() than inlined `{var}`.
#![allow(clippy::format_in_format_args, clippy::uninlined_format_args)]

pub mod audit_emit;
pub mod cascade;
pub mod error;
pub mod hmac_sign;
pub mod ledger;
pub mod locale_enforce;
pub mod notice_version_check;
pub mod schema;
pub mod store;

pub use audit_emit::{
    canonical_consent_audit_event_strings, ConsentAuditEventType, ConsentAuditRecord,
    ConsentAuditSink, FailingConsentAuditSink, InMemoryConsentAuditSink,
};
pub use cascade::{CascadeSink, CascadeTask, InMemoryCascadeSink};
pub use error::ConsentLedgerError;
pub use hmac_sign::{ConsentHmacSigner, HmacParams, HmacSignature, InMemoryConsentHmacSigner};
pub use ledger::{ConsentLedger, InMemoryConsentLedger, VerifyParams};
pub use locale_enforce::{enforce_locale, LocaleEnforceError};
pub use notice_version_check::{is_notice_version_stale, NoticeVersionCheckError};
pub use schema::{
    canonical_consent_purposes, ConsentGrantReceipt, ConsentListResponse, ConsentProofPayload,
    ConsentPurpose, ConsentRevokeReceipt, LegalBasis, LocaleBcp47, RevocationId, VerifyResponse,
};
pub use store::{ConsentRecord, ConsentRevocationRecord, ConsentStore, InMemoryConsentStore};

/// Crate canonical schema version constant. Pinned for Neon migration
/// slot N+3 at WI-S11-008 production wiring
/// (`migrations/N+3__consent_ledger_revocation.sql`).
#[must_use]
pub const fn consent_schema_version() -> u32 {
    4
}
