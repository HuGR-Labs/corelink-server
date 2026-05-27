//! `corelink-survey` — customer-health survey infrastructure.
//!
//! R-prep retention lane. Closes the survey-infrastructure gap surfaced
//! after `marketing/retention/customer-health/NPS-SURVEY-SCHEDULE` was
//! sealed: the schedule documents **when** surveys go out across waves
//! 1-6; this crate provides the actual primitives those waves call into.
//!
//! # Surface
//!
//! | Trait | What it does | Production impl deferred? |
//! |---|---|---|
//! | [`SurveyLinkSigner`] | Mints an HMAC-SHA256-signed one-shot URL token tying `(survey_id, recipient_hash, expires_at_ms)` to a per-environment signing key. The token authenticates the recipient on click-through **without a password** — the HMAC stands in for the password. | Production `D1SurveyLinkSigner` deferred (key rotation lives in `corelink-rotation-*` lane). The signing math is final. |
//! | [`SurveyResponseRecorder`] | Verifies the token in constant time (subtle::ConstantTimeEq), enforces TTL, rejects replay (per-token dedup), validates the response payload by kind, then **emits audit before insert** (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER pattern) and persists the row. | Production `D1SurveyResponseRecorder` deferred. The [`InMemoryFake`] sink mirrors the atomic semantics for unit + property tests + e2e harnesses. |
//!
//! Four canonical survey kinds:
//!
//! - [`SurveyResponse::Nps`] — 0..=10 promoter / passive / detractor score.
//! - [`SurveyResponse::Csat`] — 1..=5 satisfaction score.
//! - [`SurveyResponse::FreeText`] — sanitized + length-capped UTF-8 text.
//! - [`SurveyResponse::MultiChoice`] — bounded option set (≤ 16 indices,
//!   each ≤ 255) so the SQL row column stays compact.
//!
//! # Charter compliance
//!
//! - `#![forbid(unsafe_code)]`.
//! - No `unwrap` / `expect` / `panic` / direct `[i]` indexing in library
//!   code (all crate-strict clippy lints `deny`).
//! - HMAC verify is constant-time via [`subtle::ConstantTimeEq`].
//! - The token never includes raw email / recipient identity — only the
//!   `recipient_hash` derived by salted SHA-256 (privacy-by-design,
//!   matches the `corelink-audit::EmailHash` newtype pattern).
//! - Every successful record path emits an audit event **before** the
//!   insert lands; if audit emit fails, the recorder MUST fail the call
//!   (`audit_fail_closed`). The [`InMemoryFake`] enforces this ordering
//!   so the property test can prove it.
//! - Replay-rejection: the recorder remembers `token.jti` (the v7 UUID
//!   minted at sign time) and refuses any second `record` call against
//!   it, even with a still-valid HMAC + TTL.
//!
//! # Anti-abuse
//!
//! Per-IP rate limit + per-recipient dedup + analyst review of low-quality
//! free-text are owned by the production wiring + `RB-SURVEY-ABUSE`
//! runbook (`specs/_runbooks/RB-SURVEY-ABUSE.md`). This crate's trait
//! surface composes cleanly with `corelink-ratelimit` + `corelink-abuse`
//! at the apps/server route layer; the in-memory fake does NOT enforce
//! rate limits (that's the route's job).
//!
//! # Quickstart
//!
//! ```
//! use corelink_ops::survey::{
//!     InMemoryFake, RecipientHash, SigningKey, SurveyId, SurveyKind,
//!     SurveyLinkSigner, SurveyResponse, SurveyResponseRecorder, TenantId,
//! };
//! use uuid::Uuid;
//!
//! # fn ex() -> Result<(), corelink_ops::survey::SurveyError> {
//! let signing_key = SigningKey::from_bytes([0x55; 32]);
//! let tenant = TenantId::from_uuid(Uuid::nil());
//! let survey_id = SurveyId::new("nps-w1-2026q2");
//! let recipient = RecipientHash::derive_with_salt(
//!     "user@example.com",
//!     b"survey-recipient-salt-v1",
//! );
//!
//! let fake = InMemoryFake::new(signing_key.clone());
//! let token = fake.sign_invite(
//!     tenant,
//!     recipient.clone(),
//!     survey_id.clone(),
//!     SurveyKind::Nps,
//!     1_700_000_000_000,
//!     60 * 60 * 24 * 1_000, // 1 day TTL
//! )?;
//!
//! fake.record(
//!     token.as_str(),
//!     SurveyResponse::Nps { score: 9 },
//!     1_700_000_010_000,
//!     /* ip_hash    */ [0u8; 32],
//!     /* ua_hash    */ [0u8; 32],
//! )?;
//!
//! assert_eq!(fake.snapshot().len(), 1);
//! # Ok(()) }
//! # ex().expect("doctest");
//! ```

#![forbid(unsafe_code)]

pub mod error;
pub mod hash;
pub mod ids;
pub mod recorder;
pub mod signer;
pub mod token;
pub mod types;

pub use error::SurveyError;
pub use hash::RecipientHash;
pub use ids::{SurveyId, TenantId};
pub use recorder::{InMemoryFake, SurveyResponseRecorder};
pub use signer::{SigningKey, SurveyLinkSigner};
pub use token::{SurveyToken, TOKEN_HMAC_LEN};
pub use types::{
    sanitize_free_text, validate_csat, validate_multi_choice, validate_nps, SurveyKind,
    SurveyResponse, FREE_TEXT_MAX_LEN, MULTI_CHOICE_MAX_OPTIONS,
};

/// Crate version (sourced from `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
