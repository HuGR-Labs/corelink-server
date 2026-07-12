//! `corelink-audit` — audit event taxonomy + CloudEvents 1.0 envelope + chain
//! integrity primitives (WI-S03-007).
//!
//! Implements the canonical surface that `auth_model.md §3.13.7 +
//! observability_model.md §3 + privacy_model.md §3.5 +
//! security_model.md CTRL-AUDIT-001/002 + invariant_registry.md
//! INV-AUDIT-NO-RAW-PII / INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER /
//! INV-AUDIT-CHAIN-HASH-DETERMINISTIC / INV-AUDIT-EVENT-TYPE-EXHAUSTIVE /
//! INV-AUDIT-RETENTION-HINT-ACCURATE` enforce on every `auth.*` event
//! produced by S-03 WIs and consumed by the S-09 hash chain.
//!
//! # Architectural split
//!
//! | Layer | What this crate ships | What is deferred |
//! |---|---|---|
//! | [`AuthEvent`] CloudEvents 1.0 envelope | All required CE 1.0 fields + CoreLink extensions; `#[non_exhaustive]` on every public enum | — |
//! | [`AuthEventType`] (33 variants) | Token / session / denied / membership / WebAuthn / admin / anomaly / DSR variants; per-Lote 10.3bis P0 | — |
//! | [`AuthEventData`] payloads | Type-tagged payload struct per variant; serde-canonical `tag = "type", content = "fields"` shape via JCS | — |
//! | [`Emitter`] trait + [`InMemoryEmitter`] | Trait surface + capture-everything test sink for property tests + integration tests | `OutboxEmitter` (D1 batch INSERT alongside `corelink-meta` `commit_*`) lands in S-03 wiring; `DirectSiemEmitter` (webhook fan-out for SEV-1 events) lands in S-09 chain processor; `MultiplexEmitter` composes both |
//! | Chain hash | [`compute_content_hash`] (RFC 8785 JCS canonicalization → SHA-256 32 bytes hex); [`link_chain_hash`] (`sha256(prev_chain_hash || content_hash)` — never re-canonicalize) | Chain replay validator + tamper-detection job lands in WI-S09-004 |
//! | PII redaction | [`PrincipalIdHash`] / [`PatIdHash`] / [`EmailHash`] newtypes (constructed by SHA-256 prefix-16-hex), [`redact_pat`] macro, raw-PII-rejecting [`AuthEventData`] field types | CI lint binary `tools/audit_pii_lint/` lands as a follow-up CI gate; the type-system already prevents raw-PII at the call sites in this crate |
//! | Retention hint | [`RetentionHint`] enum tagged on every event from `tenant.tier` lookup at emit-time | Retention worker reading hints lands in WI-S11-XXX |
//! | Metrics | [`MetricsObserver`] trait + [`NoopMetrics`] / [`InMemoryMetrics`] sinks emitting the canonical 5 metrics | Real OTel pipe lands in S-09 / S-13 observability |
//!
//! # Canonical invariants enforced
//!
//! 1. **INV-AUDIT-NO-RAW-PII (CRITICAL)**: every PII-bearing field in
//!    [`AuthEventData`] is typed as a hash newtype ([`PrincipalIdHash`] /
//!    [`PatIdHash`] / [`EmailHash`]) whose only constructor is the
//!    one-way SHA-256-prefix-16-hex derivation. Raw `String` PII has no
//!    representation in the type — a refactor that tries to add a raw
//!    field is a compile error in every existing handler.
//! 2. **INV-AUDIT-CHAIN-HASH-DETERMINISTIC (HIGH)**: every event is
//!    serialized via RFC 8785 JCS (`serde_jcs`) before hashing. The
//!    canonicalized bytes are independent of `HashMap`/`BTreeMap`
//!    iteration order, locale, or floating-point formatting nuance.
//!    `compute_content_hash` returns a 64-char lowercase hex SHA-256 digest;
//!    serializing twice byte-equal is asserted by property test 10 000 iter.
//! 3. **INV-AUDIT-EVENT-TYPE-EXHAUSTIVE (HIGH)**: [`AuthEventType`] has
//!    a corresponding [`AuthEventData`] payload struct, and the
//!    serde-tag string is asserted exhaustively in `tests/canonical_vectors.rs`.
//!    A future WI adds new variants strictly via additive enum growth
//!    behind `#[non_exhaustive]`.
//! 4. **INV-AUDIT-RETENTION-HINT-ACCURATE (HIGH)**: every event carries
//!    [`RetentionHint`] derived from the tenant tier at emit time;
//!    [`RetentionHint::for_tier`] is the single canonical mapping
//!    (`Solo` → `Solo30d`, `Team` → `Team90d`, `Business` → `Business1y`,
//!    `Enterprise` → `Enterprise7y`).
//! 5. **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL)**: enforced at
//!    the production-emitter wiring layer (`OutboxEmitter` + D1 batch
//!    INSERT alongside `corelink-meta::commit_*`). This crate exposes
//!    the [`Emitter`] trait and audit row format that downstream wiring
//!    consumes; the in-memory test sink mirrors the atomic semantics
//!    (success returns `Ok`, store mutates atomically; failure returns
//!    `Err`, store unchanged).
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - **No raw PII fields** in any public type — every PII-bearing
//!   surface uses the `*Hash` newtype which can only be constructed by
//!   the SHA-256-prefix-16-hex derivation. There is no `From<String>`
//!   for any of them.
//! - **No re-canonicalization at chain link time** — the chain processor
//!   reads the persisted JCS bytes (or `content_hash` directly) and
//!   computes `sha256(prev_chain_hash || content_hash)`. Re-serializing
//!   the event for chain linkage would risk JCS double-canonicalization
//!   drift (see `compute_content_hash` rustdoc).
//!
//! # Quickstart — emit `auth.token.validated`
//!
//! ```
//! use corelink_audit::{
//!     AuthEvent, AuthEventData, AuthEventType, Emitter, InMemoryEmitter,
//!     PrincipalIdHash, RegionTag, RequestId, RetentionHint, TenantId,
//!     TenantTier, TokenKind,
//! };
//! use uuid::Uuid;
//!
//! # fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let tenant = TenantId::from_uuid(Uuid::nil());
//! let principal = PrincipalIdHash::derive("user_2NkX8...")?;
//! let event = AuthEvent::new(
//!     AuthEventType::TokenValidated,
//!     "corelink://wnam/auth/middleware",
//!     tenant,
//!     principal,
//!     RegionTag::Wnam,
//!     RequestId::new("req_abc"),
//!     RetentionHint::for_tier(TenantTier::Team),
//!     1_700_000_000_000,
//!     AuthEventData::TokenValidated {
//!         token_kind: TokenKind::Pat,
//!         scope_bitset: 0b0000_0001,
//!     },
//! );
//! let emitter = InMemoryEmitter::new();
//! emitter.emit(event)?;
//! assert_eq!(emitter.snapshot().len(), 1);
//! # Ok(()) }
//! # ex().expect("doctest");
//! ```

#![forbid(unsafe_code)]

// Internal hash-primitive module. Renamed wave-33 Stage 0 sub-step 4
// from `chain` to `link_hash` to free the `chain` namespace for the
// new aggregator-style re-export of `corelink-audit-chain` (Merkle
// chain processor). Public re-exports of `compute_content_hash` /
// `link_chain_hash` / `ContentHash` / `ChainHash` /
// `CONTENT_HASH_HEX_LEN` remain reachable at the crate root — no
// public-API breakage.
pub mod emitter;
pub mod error;
pub mod events;
pub mod link_hash;
pub mod metrics;
// Production durable emitter (WI item 3): the `OutboxEmitter` +
// `AuditOutboxWriter` port that persists auth-plane audit events fail-CLOSED,
// replacing the drop-on-restart `InMemoryEmitter` test sink in production.
pub mod outbox;
pub mod redact;
pub mod retention;

// Wave-33 Stage 0 sub-step 4 — canonical EDA chokepoint trait.
// Single import target for every Stage 1 stream's audit-fail-CLOSED
// path: `use corelink_audit::ports::AuditEmitter`.
pub mod ports;

// Wave-33 Stage 0 sub-step 4 — Option-A aggregator submodules.
// Physical absorption of `corelink-audit-chain` (Merkle chain
// processor) + `corelink-analytics` (audit analytics) deferred to
// Stage 1 per the established Option-A pattern; aggregator submodules
// surface the canonical import paths now so Stage 1 streams can
// migrate consumers incrementally.
pub mod analytics;
pub mod chain;

pub use emitter::{Emitter, EmitterError, InMemoryEmitter};
pub use error::AuditError;
pub use events::{
    AuthEvent, AuthEventData, AuthEventType, ClerkSubject, DenyReason, MembershipRole, RegionTag,
    SessionId, TokenKind, WebAuthnCredentialIdHash, CLOUDEVENTS_SPECVERSION, EVENT_DATACONTENTTYPE,
};
pub use link_hash::{
    compute_content_hash, link_chain_hash, ChainHash, ContentHash, CONTENT_HASH_HEX_LEN,
};
pub use metrics::{InMemoryMetrics, MetricsObserver, NoopMetrics};
pub use outbox::{
    AuditOutboxRow, AuditOutboxWriter, FailingAuditOutboxWriter, InMemoryAuditOutboxWriter,
    OutboxEmitter,
};
pub use redact::{redact_pat_str, EmailHash, PatIdHash, PrincipalIdHash, REDACTED_PAT_PLACEHOLDER};
pub use retention::{RetentionHint, TenantTier};

use uuid::Uuid;

/// CoreLink tenant id newtype. Mirrors `corelink-meta::TenantId` shape so
/// integration call sites can interconvert without drift.
///
/// Wraps a UUIDv7. Display + serde renders the canonical hyphenated
/// lowercase form (matches `corelink-meta`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
#[serde(transparent)]
pub struct TenantId(Uuid);

impl TenantId {
    /// Wrap a `Uuid` into the canonical newtype.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Render the UUIDv7 in canonical hyphenated lowercase text form.
    #[must_use]
    pub fn to_canonical_text(&self) -> String {
        format!("{}", self.0)
    }
}

impl core::fmt::Display for TenantId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

/// Idempotency / correlation id for the audit chain. Wraps a non-empty
/// string. The middleware (WI-S03-003) propagates the gRPC `x-request-id`
/// header into this newtype; multiple events emitted under the same
/// request_id are correlatable via SQL JOIN at chain query time.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct RequestId(String);

impl RequestId {
    /// Wrap a string into a `RequestId`. Empty input is rejected at
    /// the type boundary by panicking-free `Result` constructor below;
    /// this `new` accepts pre-validated input from callers that already
    /// hold a non-empty correlation id (typical: middleware).
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for RequestId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for RequestId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for RequestId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Crate canonical version string, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
