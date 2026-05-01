//! Canonical [`AuthEvent`] CloudEvents 1.0 envelope + [`AuthEventType`]
//! taxonomy (33 variants per Lote 10.3bis P0 expansion) + [`AuthEventData`]
//! payload struct.
//!
//! ## CloudEvents 1.0 mandatory fields
//!
//! | CE 1.0 field | Rust field | Source |
//! |---|---|---|
//! | `specversion` | constant `"1.0"` | [`CLOUDEVENTS_SPECVERSION`] |
//! | `id` | UUIDv7 | [`AuthEvent::id`] |
//! | `source` | URI string `corelink://<region>/<emitter>` | [`AuthEvent::source`] |
//! | `type` | enum tag string | [`AuthEventType::as_str`] |
//! | `time` | RFC 3339 (computed from `time_unix_ms`) | [`AuthEvent::time_unix_ms`] |
//! | `datacontenttype` | constant `"application/json"` | [`EVENT_DATACONTENTTYPE`] |
//! | `subject` | tenant_id pseudonym (UUIDv7 hyphenated) | derived from [`AuthEvent::tenant_id`] |
//!
//! ## CoreLink extension fields
//!
//! Per CloudEvents 1.0 §3, extension attributes are top-level keys
//! alongside the mandatory fields. We use the canonical names:
//!
//! - `tenant_id` (UUIDv7 hyphenated lowercase string)
//! - `principal_id_hash` (16-hex pseudonym)
//! - `region` (canonical region tag string)
//! - `request_id` (correlation id)
//! - `retention_hint` (RetentionHint enum string)
//! - `data` (event-type-specific payload)
//!
//! ## Chain integrity (S-09 forward)
//!
//! The envelope intentionally **does not** carry the chain `prev_hash`
//! field. The chain is linked at the chain processor (WI-S09-004) by
//! computing `link_chain_hash(prev_chain_hash, content_hash)` —
//! re-canonicalizing the event at chain time would risk JCS double-
//! canonicalization drift if the chain processor's `serde_jcs` version
//! differs from the producer's. The producer persists the JCS-canonical
//! bytes (via `commit_audit` on the meta layer); the chain processor
//! reads the bytes verbatim and links via the `content_hash` string.
//! See `crate::chain` rustdoc for the formal property.
//!
//! ## INV-AUDIT-EVENT-TYPE-EXHAUSTIVE
//!
//! The 33 [`AuthEventType`] variants are paired 1:1 with [`AuthEventData`]
//! variants whose serde tag matches the canonical event-type string.
//! The exhaustive pairing is asserted by `tests/canonical_vectors.rs`
//! and exercised by every property test in `tests/prop_audit.rs`. New
//! variants land via additive enum growth behind `#[non_exhaustive]`;
//! a `match` over [`AuthEventType`] in S-09 chain processor is
//! intentionally non-default so a missing arm becomes a compile error
//! when the consumer is updated to the new event-type set.

use core::fmt;

use serde::Serialize;
use uuid::Uuid;

use crate::redact::{EmailHash, PatIdHash, PrincipalIdHash};
use crate::retention::RetentionHint;
use crate::{RequestId, TenantId};

/// CloudEvents 1.0 specversion string. Constant; never changes.
pub const CLOUDEVENTS_SPECVERSION: &str = "1.0";

/// CloudEvents 1.0 `datacontenttype` for our audit envelope. Constant;
/// every event payload is JSON (UTF-8) with JCS canonicalization.
pub const EVENT_DATACONTENTTYPE: &str = "application/json";

/// CoreLink region tag — canonical short string used in `source` URI
/// and as the top-level `region` extension attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RegionTag {
    /// Western North America (e.g. `wnam`).
    Wnam,
    /// Eastern North America.
    Enam,
    /// Western Europe.
    Weur,
    /// Eastern Europe.
    Eeur,
    /// Asia-Pacific.
    Apac,
    /// South America.
    Sam,
}

impl RegionTag {
    /// Canonical short string used in CloudEvents `source` URIs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wnam => "wnam",
            Self::Enam => "enam",
            Self::Weur => "weur",
            Self::Eeur => "eeur",
            Self::Apac => "apac",
            Self::Sam => "sam",
        }
    }
}

impl fmt::Display for RegionTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Token kind tagged on `auth.token.*` events. Distinguishes Clerk
/// JWT (RS256) from CoreLink PAT (HMAC) so SIEM can split metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    /// Clerk JWT (RS256, JWKS rotation).
    ClerkJwt,
    /// Personal Access Token (CoreLink-issued, HMAC verification).
    Pat,
}

/// Reason tag for `auth.denied.*` events. Mirrors the granular
/// taxonomy from Lote 10.3bis P0 expansion: signature_invalid /
/// expired / scope_insufficient / not_found / malformed / revoked /
/// rate_limit. Each reason is a distinct [`AuthEventType`] variant
/// for SIEM-friendly aggregation; this enum is the underlying enum
/// for the `data` payload's `reason` field where applicable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    /// Signature failed verification (HMAC mismatch / RS256 alg drift / etc.).
    SignatureInvalid,
    /// Token / session past `exp`.
    Expired,
    /// Required scope absent from token bitset.
    ScopeInsufficient,
    /// Token / session id not found in store (revoked / never existed).
    NotFound,
    /// Wire format unparseable (length / charset / structure).
    Malformed,
    /// Token / session present but revoked.
    Revoked,
    /// Per-PAT rate limit ceiling hit.
    RateLimit,
}

/// Membership role tagged on `auth.membership.*` events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    /// Owner — full admin rights.
    Owner,
    /// Admin — manage members + billing.
    Admin,
    /// Member — read/write CAS scope.
    Member,
    /// Viewer — read-only.
    Viewer,
}

/// Pseudonymized session id. Wraps a UUIDv7 (random-not-personal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Wrap a `Uuid` into a [`SessionId`].
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Canonical Clerk subject claim (`sub`) as it appears in JWT — wraps a
/// non-empty string. We do **not** expose this in serialized events;
/// the audit envelope carries the [`crate::PrincipalIdHash`] derived
/// from the subject. This newtype exists to make the producer's call
/// site type-explicit and to centralize derivation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClerkSubject(String);

impl ClerkSubject {
    /// Wrap a string into a [`ClerkSubject`].
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

/// Pseudonymized WebAuthn credential id hash. SHA-256 prefix-16-hex of
/// the `CredentialId` byte array; mirrors [`PrincipalIdHash`] semantics
/// for the WebAuthn surface (WI-S03-006).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct WebAuthnCredentialIdHash(String);

impl WebAuthnCredentialIdHash {
    /// Derive a [`WebAuthnCredentialIdHash`] from raw credential bytes.
    ///
    /// Empty input is rejected; the empty SHA-256 prefix would collapse
    /// pseudonymity across credentials.
    ///
    /// # Errors
    ///
    /// Returns [`crate::AuditError::EmptyHashInput`] if `raw` is empty.
    pub fn derive(raw: &[u8]) -> Result<Self, crate::AuditError> {
        if raw.is_empty() {
            return Err(crate::AuditError::EmptyHashInput {
                kind: "webauthn_credential_id",
            });
        }
        use sha2::Digest;
        let digest = sha2::Sha256::digest(raw);
        let full = hex::encode(digest);
        let prefix = full
            .get(..16)
            .ok_or(crate::AuditError::Canonicalization(
                "sha256 hex output shorter than 16 chars (impossible)".to_string(),
            ))?;
        Ok(Self(prefix.to_string()))
    }

    /// Borrow the canonical 16-hex-char string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WebAuthnCredentialIdHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Canonical 33-variant `auth.*` event type taxonomy (EVT-047 family).
///
/// Per Lote 10.3bis P0 expansion: token lifecycle (4) + auth flow (4) +
/// 6 granular `auth.denied.*` reasons + tenant + 4 membership +
/// 6 WebAuthn + 3 lifecycle + 2 admin op + 2 anomaly + 1 PAT scope
/// escalation = 33 total.
///
/// Marked `#[non_exhaustive]` so consumers (S-09 chain processor)
/// must handle the catch-all branch and additive variants stay
/// non-breaking.
///
/// Serializes via the canonical [`AuthEventType::as_str`] string
/// (e.g. `"auth.token.validated"`); the default serde derive would
/// emit the Rust variant name (`"TokenValidated"`), which would
/// silently break the chain consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AuthEventType {
    // EVT-047 family — token lifecycle (4)
    /// Token issued (new PAT minted; new Clerk session created).
    TokenIssued,
    /// Token validated successfully on the auth path.
    TokenValidated,
    /// Token explicitly revoked.
    TokenRevoked,
    /// Token expired (post-`exp`); deny variant emitted on next presentation.
    TokenExpired,

    // Auth flow events (3)
    /// New session created (Clerk login OR PAT bound to session).
    SessionCreated,
    /// Session explicitly revoked.
    SessionRevoked,
    /// Anomaly hint: token replay detected via challenge or sign_count.
    TokenReplayDetected,

    // Legacy generic denied variants (kept for backwards compat per WI §1
    // canonical enum — superseded by the 6 granular variants below; new
    // producers SHOULD prefer the granular variant for SIEM granularity).
    /// Generic scope-denied (legacy; prefer `DeniedScopeInsufficient`).
    DeniedScope,
    /// Generic invalid-token (legacy; prefer one of the granular
    /// `DeniedSignatureInvalid` / `DeniedMalformed` / `DeniedNotFound`).
    DeniedInvalid,

    // Auth denied — 6 granular variants (Lote 10.3bis P0 expansion)
    /// Signature verification failed (HMAC mismatch / RS256 alg drift).
    DeniedSignatureInvalid,
    /// Token past `exp`.
    DeniedExpired,
    /// Required scope absent from token bitset.
    DeniedScopeInsufficient,
    /// Token id not found in store.
    DeniedNotFound,
    /// Token wire format unparseable.
    DeniedMalformed,
    /// Token explicitly revoked.
    DeniedRevoked,
    /// Per-PAT rate limit ceiling hit.
    DeniedRateLimit,

    // Tenant + membership lifecycle (5)
    /// New tenant provisioned (S-19 onboarding).
    TenantProvisioned,
    /// Tenant deleted (DSR erasure).
    TenantDeleted,
    /// New user account created.
    AccountCreated,
    /// User account deleted (DSR erasure trigger; LGPD Art. 18).
    AccountDeleted,
    /// Membership added (user joins tenant).
    MembershipAdded,

    /// Membership removed.
    MembershipRemoved,
    /// Membership role changed (e.g. Member → Admin).
    MembershipRoleChanged,

    // WebAuthn events (6 — covers WI-S03-006 emit surface)
    /// New WebAuthn credential registered.
    WebauthnRegistered,
    /// WebAuthn step-up authentication succeeded.
    WebauthnAuthenticated,
    /// WebAuthn credential deleted by user.
    WebauthnDeleted,
    /// Sign-count regression (cloned-authenticator signal); SEV-1 alert.
    WebauthnSignCountRegression,
    /// Origin allowlist mismatch on a WebAuthn ceremony — active attack signal.
    WebauthnOriginAttackAttempt,
    /// Anomaly: passkey synced via iCloud Keychain to a new device.
    WebauthnNewDeviceUsed,

    // PAT lifecycle (1)
    /// PAT scope mutation detected (privileged op via WI-S03-004 OR insider abuse).
    PatScopeEscalated,

    // Admin op events (2)
    /// Admin op authenticated via WebAuthn step-up.
    AdminOpWebauthnAuthenticated,
    /// Mass-revocation operation (admin-initiated).
    AdminOpMassRevoke,

    // Anomaly events (1; cross_region_burst — token_replay folded above)
    /// Same principal_id_hash logging in > 3 regions in a 5-min window.
    CrossRegionBurst,
}

impl AuthEventType {
    /// Canonical CloudEvents `type` attribute string.
    ///
    /// **Stable surface** — these strings are the on-the-wire identity of
    /// the event; renaming requires a bump major + 1-yr deprecation per
    /// canonical ADR-0033.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TokenIssued => "auth.token.issued",
            Self::TokenValidated => "auth.token.validated",
            Self::TokenRevoked => "auth.token.revoked",
            Self::TokenExpired => "auth.token.expired",
            Self::SessionCreated => "auth.session.created",
            Self::SessionRevoked => "auth.session.revoked",
            Self::TokenReplayDetected => "auth.anomaly.token_replay_detected",
            Self::DeniedScope => "auth.denied.scope",
            Self::DeniedInvalid => "auth.denied.invalid",
            Self::DeniedSignatureInvalid => "auth.denied.signature_invalid",
            Self::DeniedExpired => "auth.denied.expired",
            Self::DeniedScopeInsufficient => "auth.denied.scope_insufficient",
            Self::DeniedNotFound => "auth.denied.not_found",
            Self::DeniedMalformed => "auth.denied.malformed",
            Self::DeniedRevoked => "auth.denied.revoked",
            Self::DeniedRateLimit => "auth.denied.rate_limit",
            Self::TenantProvisioned => "auth.tenant.provisioned",
            Self::TenantDeleted => "auth.tenant.deleted",
            Self::AccountCreated => "auth.account.created",
            Self::AccountDeleted => "auth.account.deleted",
            Self::MembershipAdded => "auth.membership.added",
            Self::MembershipRemoved => "auth.membership.removed",
            Self::MembershipRoleChanged => "auth.membership.role_changed",
            Self::WebauthnRegistered => "auth.webauthn.registered",
            Self::WebauthnAuthenticated => "auth.webauthn.authenticated",
            Self::WebauthnDeleted => "auth.webauthn.deleted",
            Self::WebauthnSignCountRegression => "auth.webauthn.sign_count_regression",
            Self::WebauthnOriginAttackAttempt => "auth.webauthn.origin_attack_attempt",
            Self::WebauthnNewDeviceUsed => "auth.webauthn.new_device_used",
            Self::PatScopeEscalated => "auth.pat.scope_escalated",
            Self::AdminOpWebauthnAuthenticated => "auth.admin_op.webauthn_authenticated",
            Self::AdminOpMassRevoke => "auth.admin_op.mass_revoke",
            Self::CrossRegionBurst => "auth.anomaly.cross_region_burst",
        }
    }

    /// Iterate over **every** canonical variant. Used by exhaustive
    /// property tests + canonical-vector tests; the iteration order
    /// is the canonical ordering of the taxonomy doc and is stable
    /// across compiler versions.
    pub fn canonical() -> impl Iterator<Item = Self> {
        [
            Self::TokenIssued,
            Self::TokenValidated,
            Self::TokenRevoked,
            Self::TokenExpired,
            Self::SessionCreated,
            Self::SessionRevoked,
            Self::TokenReplayDetected,
            Self::DeniedScope,
            Self::DeniedInvalid,
            Self::DeniedSignatureInvalid,
            Self::DeniedExpired,
            Self::DeniedScopeInsufficient,
            Self::DeniedNotFound,
            Self::DeniedMalformed,
            Self::DeniedRevoked,
            Self::DeniedRateLimit,
            Self::TenantProvisioned,
            Self::TenantDeleted,
            Self::AccountCreated,
            Self::AccountDeleted,
            Self::MembershipAdded,
            Self::MembershipRemoved,
            Self::MembershipRoleChanged,
            Self::WebauthnRegistered,
            Self::WebauthnAuthenticated,
            Self::WebauthnDeleted,
            Self::WebauthnSignCountRegression,
            Self::WebauthnOriginAttackAttempt,
            Self::WebauthnNewDeviceUsed,
            Self::PatScopeEscalated,
            Self::AdminOpWebauthnAuthenticated,
            Self::AdminOpMassRevoke,
            Self::CrossRegionBurst,
        ]
        .into_iter()
    }

    /// Whether this event type is SEV-1 — used by `MultiplexEmitter`
    /// at production-wiring time to decide dual fan-out (outbox + direct
    /// SIEM webhook). The set is intentionally small; the rest go through
    /// the outbox path only.
    #[must_use]
    pub const fn is_sev1(self) -> bool {
        matches!(
            self,
            Self::WebauthnSignCountRegression
                | Self::WebauthnOriginAttackAttempt
                | Self::TokenReplayDetected
                | Self::PatScopeEscalated
                | Self::CrossRegionBurst
                | Self::AdminOpMassRevoke
        )
    }
}

impl fmt::Display for AuthEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl serde::Serialize for AuthEventType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Per-variant payload type. Each [`AuthEventType`] has exactly one
/// matching [`AuthEventData`] variant; the serde tag is the canonical
/// CloudEvents `type` string. JCS canonicalization (chain hash compute)
/// orders keys lexicographically so the type-tag and payload fields
/// canonicalize deterministically.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "fields")]
#[non_exhaustive]
pub enum AuthEventData {
    /// `auth.token.issued`
    #[serde(rename = "auth.token.issued")]
    TokenIssued {
        /// Which token kind was issued.
        token_kind: TokenKind,
        /// Pseudonymized PAT id (only when `token_kind = Pat`).
        pat_id_hash: Option<PatIdHash>,
        /// Scope bitset (u64 — matches `corelink-pat`).
        scope_bitset: u64,
    },
    /// `auth.token.validated`
    #[serde(rename = "auth.token.validated")]
    TokenValidated {
        /// Which token kind was validated.
        token_kind: TokenKind,
        /// Scope bitset present on the validated token.
        scope_bitset: u64,
    },
    /// `auth.token.revoked`
    #[serde(rename = "auth.token.revoked")]
    TokenRevoked {
        /// Which token kind was revoked.
        token_kind: TokenKind,
        /// Pseudonymized PAT id (only when `token_kind = Pat`).
        pat_id_hash: Option<PatIdHash>,
    },
    /// `auth.token.expired`
    #[serde(rename = "auth.token.expired")]
    TokenExpired {
        /// Which token kind expired.
        token_kind: TokenKind,
    },
    /// `auth.session.created`
    #[serde(rename = "auth.session.created")]
    SessionCreated {
        /// Pseudonymized session id.
        session_id: SessionId,
    },
    /// `auth.session.revoked`
    #[serde(rename = "auth.session.revoked")]
    SessionRevoked {
        /// Pseudonymized session id.
        session_id: SessionId,
    },
    /// `auth.anomaly.token_replay_detected`
    #[serde(rename = "auth.anomaly.token_replay_detected")]
    TokenReplayDetected {
        /// Which token kind triggered the replay signal.
        token_kind: TokenKind,
        /// Detection signal: challenge replay vs sign_count regression.
        signal: &'static str,
    },
    /// `auth.denied.scope` (legacy generic)
    #[serde(rename = "auth.denied.scope")]
    DeniedScope {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.invalid` (legacy generic)
    #[serde(rename = "auth.denied.invalid")]
    DeniedInvalid {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.signature_invalid`
    #[serde(rename = "auth.denied.signature_invalid")]
    DeniedSignatureInvalid {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.expired`
    #[serde(rename = "auth.denied.expired")]
    DeniedExpired {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.scope_insufficient`
    #[serde(rename = "auth.denied.scope_insufficient")]
    DeniedScopeInsufficient {
        /// Which token kind was rejected.
        token_kind: TokenKind,
        /// Scope bitset that was required.
        required_scope_bitset: u64,
        /// Scope bitset actually present on the token.
        actual_scope_bitset: u64,
    },
    /// `auth.denied.not_found`
    #[serde(rename = "auth.denied.not_found")]
    DeniedNotFound {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.malformed`
    #[serde(rename = "auth.denied.malformed")]
    DeniedMalformed {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.revoked`
    #[serde(rename = "auth.denied.revoked")]
    DeniedRevoked {
        /// Which token kind was rejected.
        token_kind: TokenKind,
    },
    /// `auth.denied.rate_limit`
    #[serde(rename = "auth.denied.rate_limit")]
    DeniedRateLimit {
        /// Which token kind was rejected.
        token_kind: TokenKind,
        /// Burst window in milliseconds the limit applies over.
        window_ms: u32,
    },
    /// `auth.tenant.provisioned`
    #[serde(rename = "auth.tenant.provisioned")]
    TenantProvisioned {
        /// Pseudonymized email of the owner who provisioned the tenant.
        owner_email_hash: EmailHash,
    },
    /// `auth.tenant.deleted`
    #[serde(rename = "auth.tenant.deleted")]
    TenantDeleted {
        /// DSR-trigger: was this an LGPD/GDPR erasure?
        dsr_triggered: bool,
    },
    /// `auth.account.created`
    #[serde(rename = "auth.account.created")]
    AccountCreated {
        /// Pseudonymized email of the new account.
        email_hash: EmailHash,
    },
    /// `auth.account.deleted`
    #[serde(rename = "auth.account.deleted")]
    AccountDeleted {
        /// DSR-trigger: was this an LGPD/GDPR erasure?
        dsr_triggered: bool,
    },
    /// `auth.membership.added`
    #[serde(rename = "auth.membership.added")]
    MembershipAdded {
        /// Membership role assigned.
        role: MembershipRole,
    },
    /// `auth.membership.removed`
    #[serde(rename = "auth.membership.removed")]
    MembershipRemoved {
        /// Membership role removed.
        role: MembershipRole,
    },
    /// `auth.membership.role_changed`
    #[serde(rename = "auth.membership.role_changed")]
    MembershipRoleChanged {
        /// Previous role.
        from_role: MembershipRole,
        /// New role.
        to_role: MembershipRole,
    },
    /// `auth.webauthn.registered`
    #[serde(rename = "auth.webauthn.registered")]
    WebauthnRegistered {
        /// Pseudonymized credential id.
        credential_id_hash: WebAuthnCredentialIdHash,
        /// AAGUID model identifier (16 bytes hex).
        aaguid_hex: String,
    },
    /// `auth.webauthn.authenticated`
    #[serde(rename = "auth.webauthn.authenticated")]
    WebauthnAuthenticated {
        /// Pseudonymized credential id.
        credential_id_hash: WebAuthnCredentialIdHash,
        /// Sign count value the authenticator returned.
        sign_count: u32,
    },
    /// `auth.webauthn.deleted`
    #[serde(rename = "auth.webauthn.deleted")]
    WebauthnDeleted {
        /// Pseudonymized credential id.
        credential_id_hash: WebAuthnCredentialIdHash,
    },
    /// `auth.webauthn.sign_count_regression`
    #[serde(rename = "auth.webauthn.sign_count_regression")]
    WebauthnSignCountRegression {
        /// Pseudonymized credential id.
        credential_id_hash: WebAuthnCredentialIdHash,
        /// Previous sign_count value persisted.
        previous_sign_count: u32,
        /// Sign count value the authenticator returned.
        observed_sign_count: u32,
    },
    /// `auth.webauthn.origin_attack_attempt`
    #[serde(rename = "auth.webauthn.origin_attack_attempt")]
    WebauthnOriginAttackAttempt {
        /// Origin string the authenticator submitted (canonical form).
        submitted_origin: String,
    },
    /// `auth.webauthn.new_device_used`
    #[serde(rename = "auth.webauthn.new_device_used")]
    WebauthnNewDeviceUsed {
        /// Pseudonymized credential id.
        credential_id_hash: WebAuthnCredentialIdHash,
    },
    /// `auth.pat.scope_escalated`
    #[serde(rename = "auth.pat.scope_escalated")]
    PatScopeEscalated {
        /// Pseudonymized PAT id.
        pat_id_hash: PatIdHash,
        /// Previous scope bitset.
        previous_scope_bitset: u64,
        /// New scope bitset.
        new_scope_bitset: u64,
    },
    /// `auth.admin_op.webauthn_authenticated`
    #[serde(rename = "auth.admin_op.webauthn_authenticated")]
    AdminOpWebauthnAuthenticated {
        /// Op class (e.g. `mass_revoke`, `tenant_delete`, `byok_rotate`).
        op_class: String,
    },
    /// `auth.admin_op.mass_revoke`
    #[serde(rename = "auth.admin_op.mass_revoke")]
    AdminOpMassRevoke {
        /// Number of tokens / sessions revoked in the op.
        revoked_count: u64,
    },
    /// `auth.anomaly.cross_region_burst`
    #[serde(rename = "auth.anomaly.cross_region_burst")]
    CrossRegionBurst {
        /// Number of distinct regions the principal_id_hash logged into
        /// in the 5-minute window.
        distinct_regions: u8,
    },
}

impl AuthEventData {
    /// Canonical event-type string (matches the serde tag). Used by the
    /// chain processor to dispatch on event type without round-tripping
    /// JSON.
    #[must_use]
    pub fn event_type(&self) -> AuthEventType {
        match self {
            Self::TokenIssued { .. } => AuthEventType::TokenIssued,
            Self::TokenValidated { .. } => AuthEventType::TokenValidated,
            Self::TokenRevoked { .. } => AuthEventType::TokenRevoked,
            Self::TokenExpired { .. } => AuthEventType::TokenExpired,
            Self::SessionCreated { .. } => AuthEventType::SessionCreated,
            Self::SessionRevoked { .. } => AuthEventType::SessionRevoked,
            Self::TokenReplayDetected { .. } => AuthEventType::TokenReplayDetected,
            Self::DeniedScope { .. } => AuthEventType::DeniedScope,
            Self::DeniedInvalid { .. } => AuthEventType::DeniedInvalid,
            Self::DeniedSignatureInvalid { .. } => AuthEventType::DeniedSignatureInvalid,
            Self::DeniedExpired { .. } => AuthEventType::DeniedExpired,
            Self::DeniedScopeInsufficient { .. } => AuthEventType::DeniedScopeInsufficient,
            Self::DeniedNotFound { .. } => AuthEventType::DeniedNotFound,
            Self::DeniedMalformed { .. } => AuthEventType::DeniedMalformed,
            Self::DeniedRevoked { .. } => AuthEventType::DeniedRevoked,
            Self::DeniedRateLimit { .. } => AuthEventType::DeniedRateLimit,
            Self::TenantProvisioned { .. } => AuthEventType::TenantProvisioned,
            Self::TenantDeleted { .. } => AuthEventType::TenantDeleted,
            Self::AccountCreated { .. } => AuthEventType::AccountCreated,
            Self::AccountDeleted { .. } => AuthEventType::AccountDeleted,
            Self::MembershipAdded { .. } => AuthEventType::MembershipAdded,
            Self::MembershipRemoved { .. } => AuthEventType::MembershipRemoved,
            Self::MembershipRoleChanged { .. } => AuthEventType::MembershipRoleChanged,
            Self::WebauthnRegistered { .. } => AuthEventType::WebauthnRegistered,
            Self::WebauthnAuthenticated { .. } => AuthEventType::WebauthnAuthenticated,
            Self::WebauthnDeleted { .. } => AuthEventType::WebauthnDeleted,
            Self::WebauthnSignCountRegression { .. } => {
                AuthEventType::WebauthnSignCountRegression
            }
            Self::WebauthnOriginAttackAttempt { .. } => AuthEventType::WebauthnOriginAttackAttempt,
            Self::WebauthnNewDeviceUsed { .. } => AuthEventType::WebauthnNewDeviceUsed,
            Self::PatScopeEscalated { .. } => AuthEventType::PatScopeEscalated,
            Self::AdminOpWebauthnAuthenticated { .. } => AuthEventType::AdminOpWebauthnAuthenticated,
            Self::AdminOpMassRevoke { .. } => AuthEventType::AdminOpMassRevoke,
            Self::CrossRegionBurst { .. } => AuthEventType::CrossRegionBurst,
        }
    }
}

/// Canonical CloudEvents 1.0 envelope for `auth.*` events.
///
/// Fields land in JCS-canonical lexicographic order at chain-hash
/// compute time (`crate::chain::compute_content_hash`); serde derive
/// uses the field-declaration order, but the JCS canonicalizer sorts
/// keys irrespective of declaration order, so reordering fields here
/// is a non-breaking refactor (asserted by `tests/canonical_vectors.rs`).
///
/// `id` is a UUIDv7 — generated by the producer at emit-time. The
/// chain processor (S-09) does not regenerate it.
///
/// `time_unix_ms` is the producer's clock at emit-time (NTP-synced;
/// drift bounded by SLO). The chain renders this back to RFC 3339 for
/// human-readable queries; we persist the int64 form to avoid
/// timezone / locale drift.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct AuthEvent {
    /// CloudEvents `specversion` — constant `"1.0"`.
    pub specversion: &'static str,
    /// CloudEvents `id` — UUIDv7.
    pub id: Uuid,
    /// CloudEvents `source` — URI `corelink://<region>/<emitter>`.
    pub source: String,
    /// CoreLink event type (canonical taxonomy).
    #[serde(rename = "type")]
    pub event_type: AuthEventType,
    /// CloudEvents `time` — Unix milliseconds since epoch.
    pub time_unix_ms: i64,
    /// CloudEvents `datacontenttype` — constant `"application/json"`.
    pub datacontenttype: &'static str,
    /// CoreLink extension: tenant id (UUIDv7 hyphenated lowercase).
    pub tenant_id: TenantId,
    /// CoreLink extension: pseudonymized principal id (16-hex).
    pub principal_id_hash: PrincipalIdHash,
    /// CoreLink extension: region tag.
    pub region: RegionTag,
    /// CoreLink extension: correlation id.
    pub request_id: RequestId,
    /// CoreLink extension: per-tenant retention hint.
    pub retention_hint: RetentionHint,
    /// CoreLink extension: type-specific payload.
    pub data: AuthEventData,
}

impl AuthEvent {
    /// Construct a new [`AuthEvent`].
    ///
    /// Generates a fresh UUIDv7 for `id` (using the OS RNG via the
    /// `uuid` crate's `v7` feature). The producer should pass a
    /// `time_unix_ms` from a monotonic+wall-clock pair (typically
    /// `SystemTime::now().duration_since(UNIX_EPOCH)` rounded to ms).
    ///
    /// Asserts at debug-build time that `event_type` matches
    /// `data.event_type()` — a mismatch would corrupt the chain
    /// (consumer dispatches on `type`, fields shape on `data`). At
    /// release the assertion is compiled out; the producer is
    /// trusted to use the convenience constructors below or pair
    /// the type / data manually.
    ///
    /// The 9 positional arguments mirror the canonical CloudEvents 1.0
    /// envelope shape (`type` / `source` / `subject`-derivation /
    /// 4 CoreLink extensions / `time` / `data`); a builder would
    /// obscure the contract and cost an allocation.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "9-arg shape mirrors CloudEvents 1.0 envelope mandatory + CoreLink extension fields; documented in §6.1.2"
    )]
    pub fn new(
        event_type: AuthEventType,
        source: impl Into<String>,
        tenant_id: TenantId,
        principal_id_hash: PrincipalIdHash,
        region: RegionTag,
        request_id: RequestId,
        retention_hint: RetentionHint,
        time_unix_ms: i64,
        data: AuthEventData,
    ) -> Self {
        debug_assert_eq!(
            event_type,
            data.event_type(),
            "AuthEvent::new: event_type and data variant must match"
        );
        Self {
            specversion: CLOUDEVENTS_SPECVERSION,
            id: Uuid::now_v7(),
            source: source.into(),
            event_type,
            time_unix_ms,
            datacontenttype: EVENT_DATACONTENTTYPE,
            tenant_id,
            principal_id_hash,
            region,
            request_id,
            retention_hint,
            data,
        }
    }

    /// Construct a new [`AuthEvent`] with a caller-supplied UUIDv7.
    ///
    /// The deterministic-id variant — useful for retried emissions
    /// where the outbox idempotency key must remain stable across
    /// retries (production wiring derives `id` from
    /// `(request_id, event_type, slot_idx)`).
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "10-arg shape == AuthEvent::new + caller-supplied UUIDv7 id"
    )]
    pub fn with_id(
        id: Uuid,
        event_type: AuthEventType,
        source: impl Into<String>,
        tenant_id: TenantId,
        principal_id_hash: PrincipalIdHash,
        region: RegionTag,
        request_id: RequestId,
        retention_hint: RetentionHint,
        time_unix_ms: i64,
        data: AuthEventData,
    ) -> Self {
        debug_assert_eq!(
            event_type,
            data.event_type(),
            "AuthEvent::with_id: event_type and data variant must match"
        );
        Self {
            specversion: CLOUDEVENTS_SPECVERSION,
            id,
            source: source.into(),
            event_type,
            time_unix_ms,
            datacontenttype: EVENT_DATACONTENTTYPE,
            tenant_id,
            principal_id_hash,
            region,
            request_id,
            retention_hint,
            data,
        }
    }

    /// Borrow the canonical `subject` value for the CloudEvents `subject`
    /// extension. We use the tenant_id canonical text form — the audit
    /// query surface filters by tenant first, so making the subject the
    /// tenant id improves SIEM correlation without surfacing principal
    /// PII.
    #[must_use]
    pub fn subject(&self) -> String {
        self.tenant_id.to_canonical_text()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test module — assertions panic by design"
)]
mod tests {
    use super::*;

    fn principal() -> PrincipalIdHash {
        PrincipalIdHash::derive("user_xyz").expect("derive")
    }

    #[test]
    fn auth_event_type_canonical_count_is_33() {
        // Per WI v1.2.0 §1 + §31 changelog: the canonical taxonomy has
        // **33 distinct on-the-wire event-type strings**. Composition:
        //
        // - 4 token lifecycle (issued/validated/revoked/expired)
        // - 2 session (created/revoked)
        // - 2 legacy generic denied (scope/invalid) — kept for backwards
        //   compat; new producers prefer the granular variants below
        // - 1 generic denied (rate_limit) — has no granular sibling
        // - 6 granular denied (signature_invalid/expired/scope_insufficient/
        //   not_found/malformed/revoked) — Lote 10.3bis P0
        // - 5 tenant + membership (tenant_provisioned/tenant_deleted/
        //   account_created/account_deleted/membership_added/removed/
        //   role_changed; 5 distinct here because tenant_deleted +
        //   account_deleted are listed under "lifecycle" in §1)
        // - 6 webauthn (registered/authenticated/deleted/sign_count_regression/
        //   origin_attack_attempt/new_device_used)
        // - 1 PAT lifecycle (pat_scope_escalated)
        // - 2 admin op (webauthn_authenticated/mass_revoke)
        // - 2 anomaly (token_replay_detected/cross_region_burst)
        //
        // Total: 4+2+2+1+6+7+6+1+2+2 = 33.
        assert_eq!(AuthEventType::canonical().count(), 33);
    }

    #[test]
    fn each_variant_has_distinct_canonical_string() {
        let mut seen = std::collections::HashSet::new();
        for v in AuthEventType::canonical() {
            let s = v.as_str();
            assert!(s.starts_with("auth."), "non-canonical prefix: {s}");
            assert!(seen.insert(s), "duplicate canonical string: {s}");
        }
        assert_eq!(seen.len(), 33);
    }

    #[test]
    fn data_event_type_round_trips_for_every_variant() {
        // For each AuthEventType, pair with a synthetic AuthEventData
        // and assert event_type() returns the same variant.
        for t in AuthEventType::canonical() {
            let d = synthetic_data(t);
            assert_eq!(d.event_type(), t, "drift on variant: {t:?}");
        }
    }

    #[test]
    fn sev1_set_is_exactly_six() {
        let count = AuthEventType::canonical().filter(|t| t.is_sev1()).count();
        assert_eq!(count, 6, "SEV-1 fanout set must be exactly 6 events");
    }

    #[test]
    fn auth_event_subject_is_tenant_id_text() {
        let tenant = TenantId::from_uuid(Uuid::nil());
        let event = AuthEvent::new(
            AuthEventType::TokenValidated,
            "corelink://wnam/auth/middleware",
            tenant,
            principal(),
            RegionTag::Wnam,
            RequestId::new("req_abc"),
            RetentionHint::Team90d,
            1_700_000_000_000,
            AuthEventData::TokenValidated {
                token_kind: TokenKind::Pat,
                scope_bitset: 0b1,
            },
        );
        assert_eq!(event.subject(), tenant.to_canonical_text());
    }

    fn synthetic_data(t: AuthEventType) -> AuthEventData {
        let cred = WebAuthnCredentialIdHash::derive(b"cid_xyz").expect("derive");
        let pat = PatIdHash::derive("pat_xyz").expect("derive");
        let email = EmailHash::derive("user@example.com").expect("derive");
        match t {
            AuthEventType::TokenIssued => AuthEventData::TokenIssued {
                token_kind: TokenKind::Pat,
                pat_id_hash: Some(pat.clone()),
                scope_bitset: 1,
            },
            AuthEventType::TokenValidated => AuthEventData::TokenValidated {
                token_kind: TokenKind::ClerkJwt,
                scope_bitset: 1,
            },
            AuthEventType::TokenRevoked => AuthEventData::TokenRevoked {
                token_kind: TokenKind::Pat,
                pat_id_hash: Some(pat.clone()),
            },
            AuthEventType::TokenExpired => AuthEventData::TokenExpired {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::SessionCreated => AuthEventData::SessionCreated {
                session_id: SessionId::from_uuid(Uuid::nil()),
            },
            AuthEventType::SessionRevoked => AuthEventData::SessionRevoked {
                session_id: SessionId::from_uuid(Uuid::nil()),
            },
            AuthEventType::TokenReplayDetected => AuthEventData::TokenReplayDetected {
                token_kind: TokenKind::Pat,
                signal: "challenge_replay",
            },
            AuthEventType::DeniedScope => AuthEventData::DeniedScope {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedInvalid => AuthEventData::DeniedInvalid {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedSignatureInvalid => AuthEventData::DeniedSignatureInvalid {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedExpired => AuthEventData::DeniedExpired {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedScopeInsufficient => AuthEventData::DeniedScopeInsufficient {
                token_kind: TokenKind::Pat,
                required_scope_bitset: 0b10,
                actual_scope_bitset: 0b01,
            },
            AuthEventType::DeniedNotFound => AuthEventData::DeniedNotFound {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedMalformed => AuthEventData::DeniedMalformed {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedRevoked => AuthEventData::DeniedRevoked {
                token_kind: TokenKind::Pat,
            },
            AuthEventType::DeniedRateLimit => AuthEventData::DeniedRateLimit {
                token_kind: TokenKind::Pat,
                window_ms: 60_000,
            },
            AuthEventType::TenantProvisioned => AuthEventData::TenantProvisioned {
                owner_email_hash: email.clone(),
            },
            AuthEventType::TenantDeleted => AuthEventData::TenantDeleted {
                dsr_triggered: true,
            },
            AuthEventType::AccountCreated => AuthEventData::AccountCreated {
                email_hash: email.clone(),
            },
            AuthEventType::AccountDeleted => AuthEventData::AccountDeleted {
                dsr_triggered: false,
            },
            AuthEventType::MembershipAdded => AuthEventData::MembershipAdded {
                role: MembershipRole::Member,
            },
            AuthEventType::MembershipRemoved => AuthEventData::MembershipRemoved {
                role: MembershipRole::Member,
            },
            AuthEventType::MembershipRoleChanged => AuthEventData::MembershipRoleChanged {
                from_role: MembershipRole::Member,
                to_role: MembershipRole::Admin,
            },
            AuthEventType::WebauthnRegistered => AuthEventData::WebauthnRegistered {
                credential_id_hash: cred.clone(),
                aaguid_hex: "00000000000000000000000000000000".to_string(),
            },
            AuthEventType::WebauthnAuthenticated => AuthEventData::WebauthnAuthenticated {
                credential_id_hash: cred.clone(),
                sign_count: 1,
            },
            AuthEventType::WebauthnDeleted => AuthEventData::WebauthnDeleted {
                credential_id_hash: cred.clone(),
            },
            AuthEventType::WebauthnSignCountRegression => {
                AuthEventData::WebauthnSignCountRegression {
                    credential_id_hash: cred.clone(),
                    previous_sign_count: 5,
                    observed_sign_count: 3,
                }
            }
            AuthEventType::WebauthnOriginAttackAttempt => {
                AuthEventData::WebauthnOriginAttackAttempt {
                    submitted_origin: "https://evil.example.com".to_string(),
                }
            }
            AuthEventType::WebauthnNewDeviceUsed => AuthEventData::WebauthnNewDeviceUsed {
                credential_id_hash: cred.clone(),
            },
            AuthEventType::PatScopeEscalated => AuthEventData::PatScopeEscalated {
                pat_id_hash: pat.clone(),
                previous_scope_bitset: 0b1,
                new_scope_bitset: 0b11,
            },
            AuthEventType::AdminOpWebauthnAuthenticated => {
                AuthEventData::AdminOpWebauthnAuthenticated {
                    op_class: "mass_revoke".to_string(),
                }
            }
            AuthEventType::AdminOpMassRevoke => AuthEventData::AdminOpMassRevoke {
                revoked_count: 1000,
            },
            AuthEventType::CrossRegionBurst => AuthEventData::CrossRegionBurst {
                distinct_regions: 4,
            },
        }
    }
}

/// Test-only helper exposing the canonical synthetic data builder so
/// integration tests + property tests can construct events without
/// duplicating the variant pairing matrix.
///
/// # Errors
///
/// Returns [`crate::AuditError`] if the underlying hash derivation
/// rejects empty input. The caller passes hard-coded non-empty
/// constants below so this branch is unreachable in practice; it is
/// surfaced as a `Result` to satisfy the crate-strict lint that
/// forbids `unwrap`/`expect` outside `#[cfg(test)]` modules.
#[doc(hidden)]
#[allow(
    clippy::items_after_test_module,
    reason = "doc-hidden test helper intentionally lives after the unit-test module — production callers use the crate's typed constructors directly"
)]
pub fn synthetic_data_for(t: AuthEventType) -> Result<AuthEventData, crate::AuditError> {
    let cred = WebAuthnCredentialIdHash::derive(b"cid_xyz")?;
    let pat = PatIdHash::derive("pat_xyz")?;
    let email = EmailHash::derive("u@x.io")?;
    Ok(match t {
        AuthEventType::TokenIssued => AuthEventData::TokenIssued {
            token_kind: TokenKind::Pat,
            pat_id_hash: Some(pat.clone()),
            scope_bitset: 1,
        },
        AuthEventType::TokenValidated => AuthEventData::TokenValidated {
            token_kind: TokenKind::ClerkJwt,
            scope_bitset: 1,
        },
        AuthEventType::TokenRevoked => AuthEventData::TokenRevoked {
            token_kind: TokenKind::Pat,
            pat_id_hash: Some(pat.clone()),
        },
        AuthEventType::TokenExpired => AuthEventData::TokenExpired {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::SessionCreated => AuthEventData::SessionCreated {
            session_id: SessionId::from_uuid(Uuid::nil()),
        },
        AuthEventType::SessionRevoked => AuthEventData::SessionRevoked {
            session_id: SessionId::from_uuid(Uuid::nil()),
        },
        AuthEventType::TokenReplayDetected => AuthEventData::TokenReplayDetected {
            token_kind: TokenKind::Pat,
            signal: "challenge_replay",
        },
        AuthEventType::DeniedScope => AuthEventData::DeniedScope {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedInvalid => AuthEventData::DeniedInvalid {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedSignatureInvalid => AuthEventData::DeniedSignatureInvalid {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedExpired => AuthEventData::DeniedExpired {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedScopeInsufficient => AuthEventData::DeniedScopeInsufficient {
            token_kind: TokenKind::Pat,
            required_scope_bitset: 0b10,
            actual_scope_bitset: 0b01,
        },
        AuthEventType::DeniedNotFound => AuthEventData::DeniedNotFound {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedMalformed => AuthEventData::DeniedMalformed {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedRevoked => AuthEventData::DeniedRevoked {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedRateLimit => AuthEventData::DeniedRateLimit {
            token_kind: TokenKind::Pat,
            window_ms: 60_000,
        },
        AuthEventType::TenantProvisioned => AuthEventData::TenantProvisioned {
            owner_email_hash: email.clone(),
        },
        AuthEventType::TenantDeleted => AuthEventData::TenantDeleted {
            dsr_triggered: true,
        },
        AuthEventType::AccountCreated => AuthEventData::AccountCreated {
            email_hash: email.clone(),
        },
        AuthEventType::AccountDeleted => AuthEventData::AccountDeleted {
            dsr_triggered: false,
        },
        AuthEventType::MembershipAdded => AuthEventData::MembershipAdded {
            role: MembershipRole::Member,
        },
        AuthEventType::MembershipRemoved => AuthEventData::MembershipRemoved {
            role: MembershipRole::Member,
        },
        AuthEventType::MembershipRoleChanged => AuthEventData::MembershipRoleChanged {
            from_role: MembershipRole::Member,
            to_role: MembershipRole::Admin,
        },
        AuthEventType::WebauthnRegistered => AuthEventData::WebauthnRegistered {
            credential_id_hash: cred.clone(),
            aaguid_hex: "00000000000000000000000000000000".to_string(),
        },
        AuthEventType::WebauthnAuthenticated => AuthEventData::WebauthnAuthenticated {
            credential_id_hash: cred.clone(),
            sign_count: 1,
        },
        AuthEventType::WebauthnDeleted => AuthEventData::WebauthnDeleted {
            credential_id_hash: cred.clone(),
        },
        AuthEventType::WebauthnSignCountRegression => AuthEventData::WebauthnSignCountRegression {
            credential_id_hash: cred.clone(),
            previous_sign_count: 5,
            observed_sign_count: 3,
        },
        AuthEventType::WebauthnOriginAttackAttempt => AuthEventData::WebauthnOriginAttackAttempt {
            submitted_origin: "https://evil.example.com".to_string(),
        },
        AuthEventType::WebauthnNewDeviceUsed => AuthEventData::WebauthnNewDeviceUsed {
            credential_id_hash: cred.clone(),
        },
        AuthEventType::PatScopeEscalated => AuthEventData::PatScopeEscalated {
            pat_id_hash: pat.clone(),
            previous_scope_bitset: 0b1,
            new_scope_bitset: 0b11,
        },
        AuthEventType::AdminOpWebauthnAuthenticated => AuthEventData::AdminOpWebauthnAuthenticated {
            op_class: "mass_revoke".to_string(),
        },
        AuthEventType::AdminOpMassRevoke => AuthEventData::AdminOpMassRevoke {
            revoked_count: 1000,
        },
        AuthEventType::CrossRegionBurst => AuthEventData::CrossRegionBurst {
            distinct_regions: 4,
        },
    })
}
