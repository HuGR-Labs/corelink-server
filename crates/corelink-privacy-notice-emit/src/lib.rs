//! `corelink-privacy-notice-emit` — Privacy notice versioning + 3-locale
//! publication emitter (WI-S11-004 — S-11 Privacy Pipeline HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! this crate ships the **pure-logic skeleton** of the privacy notice
//! publication primitive. The trait surfaces match what every production
//! wiring (CI hook `validate_privacy_notice.py`, CD pipeline Cloudflare Pages
//! deploy, R2 audit Object Lock 7y emit, WI-S11-003 stale_consent_check
//! trigger) will satisfy, plus an in-memory orchestrator that exercises every
//! load-bearing invariant the production wiring relies on.
//!
//! Property tests pinned at 10k iter against the orchestrator cover:
//! - **notice_text_hash determinism**: 100 random content samples × CRLF/LF
//!   variants → 100% hash parity (AC-006 cross-platform determinism contract).
//! - **audit fail-CLOSED**: audit emit failure → state UNCHANGED + deploy
//!   aborted (AC-008 + INV-AUDIT-APPEND-ONLY).
//! - **3-locales sync enforcement**: publication request missing any locale →
//!   `LocaleSyncViolation` error (AC-004).
//! - **major bump triggers re-consent decision**: `NoticeEmitDecision::
//!   MajorBumpPublished.requires_re_consent_trigger() == true` (AC-002).
//! - **minor bump silent**: no deprecation record, no re-consent trigger
//!   (AC-003).
//! - **native speaker review enforcement**: checkbox false → error (AC-005).
//! - **tenant isolation**: per-instance `Arc<Mutex<>>` (F-001); NEVER static.
//!
//! # Canonical audit ordering (AC-008 + INV-AUDIT-APPEND-ONLY)
//!
//! Per Lote 10.6bis split-tier discipline: `lookup → emit_audit → mutate_state`.
//! The audit sink [`audit::FailingNoticeAuditSink`] forces the fail-CLOSED
//! chaos test path.
//!
//! # 2 CloudEvents canonical types
//!
//! Per Lote 10.9bis P0-G prefix:
//! - `dev.hugr.corelink.privacy_notice.published.v1` — every publication.
//! - `dev.hugr.corelink.privacy_notice.deprecated.v1` — major bump only
//!   (previous version superseded).
//!
//! # notice_text_hash deterministic algorithm (AC-006)
//!
//! CRLF→LF + trim trailing whitespace per line + SHA-256 hex64.
//! Cross-validates with `scripts/notice_text_hash_canonical.py` (Python).
//! The property test `prop_notice_hash_determinism` verifies 100 random
//! content samples × CRLF/LF variants → 100% parity.
//!
//! # INV-CONSENT-PROOF-VERIFIABLE integration (WI-S11-003)
//!
//! The `notice_text_hashes` BTreeMap in [`emitter::NoticeEmitDecision`] is
//! the direct input for WI-S11-003 `ConsentProofPayload.notice_text_hash`.
//! Cross-validation: regulator re-computes hash from `legal/privacy-notice/
//! v<M.m.0>/<locale>.md` → matches `consent_ledger.notice_text_hash` in D1.
//!
//! # wasm32-clean
//!
//! This crate has no C dependencies, no `ring`, no `jsonwebtoken`. The `sha2`
//! + `hex` dependencies are pure-Rust and wasm32-clean.
//!
//! # Production wiring (deferred to WI-S11-008)
//!
//! - Real R2 audit Object Lock 7y emit (production `NoticeAuditSink`).
//! - Real Cloudflare Pages deploy (production `NoticeStateStore`).
//! - WI-S11-003 `stale_consent_check` job trigger on major bump.
//! - Cloudflare Email transactional templates 9 (3 locales × 3 types).
//! - CI hook `validate_privacy_notice.py` Python (Legal Review EVT-044 check).
//! - Grafana dashboard `corelink-privacy-notice` 4 panels.
//! - 3 Prom metrics: `corelink_privacy_notice_version_published_total` +
//!   `corelink_privacy_notice_diff_publication_lag_seconds` +
//!   `corelink_privacy_notice_re_consent_pending_total`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod emitter;
pub mod error;
pub mod event;
pub mod store;

pub use audit::{
    FailingNoticeAuditSink, InMemoryNoticeAuditSink, NoticeAuditRecord, NoticeAuditSink,
};
pub use emitter::{
    InMemoryNoticeEmitter, NoticeEmitDecision, NoticeEmitter, NoticePublishRequest,
};
pub use error::{NoticeAuditSinkError, NoticeEmitterError, NoticeStoreError};
pub use event::{
    canonical_notice_locales, notice_text_hash, NoticeCloudEventType,
    NoticeDeprecatedCloudEvent, NoticeDeprecatedPayload, NoticeLocale,
    NoticePublishedCloudEvent, NoticePublishedPayload, NoticeVersion, VersionBump,
};
pub use store::{InMemoryNoticeStateStore, NoticePublicationState, NoticeStateStore};

/// Canonical 30-day grace period (ms) for re-consent after a major bump.
/// Per WI-S11-004 §2.2: 30d grace + 60d cap = 90d total before
/// legitimate_interest fallback or cascade unsubscribe (Privacy Officer
/// decision per LIA EVT-046).
pub const RE_CONSENT_GRACE_PERIOD_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

/// Canonical 60-day cap (ms) for cascade unsubscribe after grace period.
pub const RE_CONSENT_CAP_PERIOD_MS: u64 = 60 * 24 * 60 * 60 * 1_000;

/// Canonical total window (ms) = 30d grace + 60d cap = 90d.
pub const RE_CONSENT_TOTAL_WINDOW_MS: u64 =
    RE_CONSENT_GRACE_PERIOD_MS + RE_CONSENT_CAP_PERIOD_MS;

/// Diff publication SLA (ms) — p99 ≤ 24h per AC-007 and Prom metric
/// `corelink_privacy_notice_diff_publication_lag_seconds`.
pub const DIFF_PUBLICATION_SLA_MS: u64 = 24 * 60 * 60 * 1_000;

/// Crate schema version. Pinned for audit log + evidence trail versioning.
#[must_use]
pub const fn notice_emit_schema_version() -> u32 {
    1
}
