//! In-memory fakes for CAS / D1 / PAT / idempotency / quota / rate-limit /
//! Stripe webhook subsystems. Each fake faithfully implements the
//! production tenant-isolation contract so adversarial scenarios can
//! exercise the real rejection + audit ordering.
//!
//! # Fail-CLOSED audit ordering
//!
//! Every fake that performs a tenant-bound authorization decision
//! emits a [`corelink_audit::AuthEvent`] BEFORE returning the
//! rejection. The [`AuditCapture`] wrapper makes this assertable from
//! scenarios via [`AuditCapture::last_deny`] / [`AuditCapture::count`].
//!
//! This mirrors the production wiring: `OutboxEmitter` writes the
//! deny row into the D1 batch alongside the handler's main mutation
//! BEFORE the handler returns 403 — so a successful 403 implies a
//! durable audit row. The fake makes the same guarantee in-memory.

use std::sync::{Arc, Mutex};

use corelink_audit::{
    AuthEvent, AuthEventData, AuthEventType, DenyReason, Emitter, InMemoryEmitter, PrincipalIdHash,
    RegionTag, RequestId, RetentionHint, TenantId as AuditTenantId, TenantTier, TokenKind,
};

// ── Errors ──────────────────────────────────────────────────────────────

/// Canonical taxonomy of fake-store deny reasons. Each variant maps
/// 1:1 onto a `corelink_audit::AuthEventType` so the harness can
/// assert the right event was emitted before the reject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyKind {
    /// The requester's authenticated tenant id does not match the
    /// resource's tenant id (CAS read/write/list, audit query, PAT
    /// usage, Stripe replay). The audit envelope is
    /// `auth.denied.scope` with `subject = requester.tenant_id`.
    AuthzTenantMismatch,
    /// JWT claimed tenant=A but request body asserted tenant=B —
    /// inconsistent envelope. Backend uses JWT tenant_id; body is
    /// rejected before any side effect.
    JwtBodyTenantConflict,
    /// PAT signature did not validate against the resource tenant's
    /// HMAC key — `auth.denied.signature_invalid`.
    PatSignatureInvalid,
    /// Cross-tenant idempotency-key replay attempted — body
    /// fingerprint mismatch under same idempotency key. `auth.denied.invalid`.
    IdempotencyConflict,
    /// Quota window exhausted; `auth.denied.rate_limit` with
    /// quota-class subreason.
    QuotaExhausted,
    /// Per-tenant rate-limit ceiling hit; `auth.denied.rate_limit`.
    RateLimited,
    /// Dual-approval (corelink-dual-approval) required and not present
    /// for a cross-tenant audit query under admin role.
    DualApprovalRequired,
    /// Stripe webhook with same `stripe_event_id` already processed
    /// for a different tenant — replay rejected.
    StripeReplay,
    /// CMK key rotation in flight — read attempted against a
    /// half-rotated envelope. `auth.denied.invalid`.
    CmkRotationInFlight,
    /// PAT revoke ToCToU — PAT was used between revoke decision and
    /// commit; revoke must be enforced post-commit on read side.
    PatRevoked,
    /// Cross-region request whose tenant residency does not permit
    /// processing in the receiving region — region routing rejection.
    RegionResidencyViolation,
    /// DSR (data subject request) submitted on behalf of a different
    /// tenant's principal — auth-context did not match DSR target.
    DsrAuthContextMismatch,
    /// Audit chain leaf forge: leaf hash does not chain to a known
    /// parent under the resource tenant's chain root.
    AuditChainForge,
    /// R2 multipart `upload_id` reused/forged across tenants — second
    /// tenant attempting to upload parts must be rejected by prefix
    /// ownership.
    MultipartUploadForge,
    /// Cross-tenant quota inheritance leak: child tenant exhaustion
    /// must not be visible to a sibling tenant under the same parent.
    QuotaInheritanceLeak,
    /// Audit query carried a tenant-scope filter parameter that was
    /// inconsistent with the requester's JWT tenant — query layer
    /// must enforce tenant scoping pre-filter (no user-supplied
    /// `tenant_id` filter override).
    AuditQueryInjection,
    /// Replication lag: tenant credential / PAT revoke not yet
    /// propagated to the receiving region. Read side must fail-CLOSED.
    KvReplicationLag,
}

/// A captured rejection attempt: rejection kind + the requester
/// tenant id (the tenant whose credential was used) + the resource
/// tenant id (the tenant whose namespace was targeted). Scenarios
/// assert both sides match the expected attack shape.
#[derive(Debug, Clone, Copy)]
pub struct AuditAttempt {
    /// Reason the operation was rejected.
    pub kind: DenyKind,
    /// Tenant id of the authenticated requester (subject of audit).
    pub requester: Uuid,
    /// Tenant id of the targeted resource (victim of attempted access).
    pub resource_owner: Uuid,
}

// ── Audit capture wrapper ───────────────────────────────────────────────

/// Wraps an [`InMemoryEmitter`] and records the canonical
/// [`AuditAttempt`] alongside the [`AuthEvent`] envelope.
///
/// Cloning is cheap (`Arc` internally) so scenarios can hold a
/// scenario-level handle and hand independent clones to each fake.
#[derive(Clone, Debug, Default)]
pub struct AuditCapture {
    emitter: InMemoryEmitter,
    attempts: Arc<Mutex<Vec<AuditAttempt>>>,
}

impl AuditCapture {
    /// Construct an empty capture.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the inner emitter (e.g. for snapshot assertions in
    /// scenarios that inspect the CloudEvents envelope directly).
    #[must_use]
    pub fn emitter(&self) -> &InMemoryEmitter {
        &self.emitter
    }

    /// Number of rejections captured so far.
    #[must_use]
    pub fn count(&self) -> usize {
        match self.attempts.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Snapshot of every captured rejection attempt.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditAttempt> {
        match self.attempts.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Last captured rejection attempt (if any).
    #[must_use]
    pub fn last_deny(&self) -> Option<AuditAttempt> {
        self.snapshot().last().copied()
    }

    /// Emit a deny event with the canonical audit envelope and append
    /// the [`AuditAttempt`] to the capture. Called BEFORE every
    /// rejection return path — see fail-CLOSED invariant in the
    /// module-level docs.
    pub fn record_deny(&self, attempt: AuditAttempt) -> Result<(), FakeError> {
        let principal =
            PrincipalIdHash::derive("user_fake").map_err(|e| FakeError::Audit(e.to_string()))?;
        let (event_type, data) = match attempt.kind {
            DenyKind::AuthzTenantMismatch => (
                AuthEventType::DeniedScope,
                AuthEventData::DeniedScope {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::JwtBodyTenantConflict => (
                AuthEventType::DeniedMalformed,
                AuthEventData::DeniedMalformed {
                    token_kind: TokenKind::ClerkJwt,
                },
            ),
            DenyKind::PatSignatureInvalid => (
                AuthEventType::DeniedSignatureInvalid,
                AuthEventData::DeniedSignatureInvalid {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::IdempotencyConflict => (
                AuthEventType::DeniedInvalid,
                AuthEventData::DeniedInvalid {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::QuotaExhausted | DenyKind::RateLimited => (
                AuthEventType::DeniedRateLimit,
                AuthEventData::DeniedRateLimit {
                    token_kind: TokenKind::Pat,
                    window_ms: 60_000,
                },
            ),
            DenyKind::DualApprovalRequired => (
                AuthEventType::DeniedScopeInsufficient,
                AuthEventData::DeniedScopeInsufficient {
                    token_kind: TokenKind::Pat,
                    required_scope_bitset: 0b10,
                    actual_scope_bitset: 0b00,
                },
            ),
            DenyKind::StripeReplay => (
                AuthEventType::DeniedInvalid,
                AuthEventData::DeniedInvalid {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::CmkRotationInFlight
            | DenyKind::AuditChainForge
            | DenyKind::MultipartUploadForge
            | DenyKind::AuditQueryInjection => (
                AuthEventType::DeniedInvalid,
                AuthEventData::DeniedInvalid {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::PatRevoked | DenyKind::KvReplicationLag => (
                AuthEventType::DeniedSignatureInvalid,
                AuthEventData::DeniedSignatureInvalid {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::RegionResidencyViolation | DenyKind::DsrAuthContextMismatch => (
                AuthEventType::DeniedScope,
                AuthEventData::DeniedScope {
                    token_kind: TokenKind::Pat,
                },
            ),
            DenyKind::QuotaInheritanceLeak => (
                AuthEventType::DeniedRateLimit,
                AuthEventData::DeniedRateLimit {
                    token_kind: TokenKind::Pat,
                    window_ms: 60_000,
                },
            ),
        };
        // Bind: noted DenyReason mirror just for module-completeness — kept
        // referenced via debug_assert so the import does not warn.
        debug_assert!(matches!(
            attempt.kind,
            DenyKind::AuthzTenantMismatch
                | DenyKind::JwtBodyTenantConflict
                | DenyKind::PatSignatureInvalid
                | DenyKind::IdempotencyConflict
                | DenyKind::QuotaExhausted
                | DenyKind::RateLimited
                | DenyKind::DualApprovalRequired
                | DenyKind::StripeReplay
                | DenyKind::CmkRotationInFlight
                | DenyKind::PatRevoked
                | DenyKind::RegionResidencyViolation
                | DenyKind::DsrAuthContextMismatch
                | DenyKind::AuditChainForge
                | DenyKind::MultipartUploadForge
                | DenyKind::QuotaInheritanceLeak
                | DenyKind::AuditQueryInjection
                | DenyKind::KvReplicationLag
        ));
        let _ = DenyReason::ScopeInsufficient; // touch import to keep linker happy under deny(unused)

        let event = AuthEvent::new(
            event_type,
            "corelink://test/e2e-tenant-isolation",
            AuditTenantId::from_uuid(attempt.requester),
            principal,
            RegionTag::Wnam,
            RequestId::new("req_test"),
            RetentionHint::for_tier(TenantTier::Enterprise),
            0,
            data,
        );

        // INVARIANT (fail-CLOSED ordering): emit BEFORE recording the
        // attempt — if the emitter fails the rejection is upgraded to
        // 503 by the caller (mirrors production OutboxEmitter contract).
        self.emitter
            .emit(event)
            .map_err(|e| FakeError::Audit(e.to_string()))?;

        match self.attempts.lock() {
            Ok(mut g) => {
                g.push(attempt);
                Ok(())
            }
            Err(_) => Err(FakeError::MutexPoisoned),
        }
    }
}

/// Errors surfaced by the in-memory fakes.
#[derive(Debug, thiserror::Error)]
pub enum FakeError {
    /// Tenant isolation deny — see [`DenyKind`] for the taxonomy.
    #[error("tenant isolation deny: {0:?}")]
    Deny(DenyKind),
    /// Underlying audit emitter rejected the event (mirrors
    /// production 503-class path).
    #[error("audit emitter error: {0}")]
    Audit(String),
    /// Internal mutex poisoned — test sink only.
    #[error("internal mutex poisoned")]
    MutexPoisoned,
    /// Resource not found at the supplied tenant-bound key.
    #[error("resource not found")]
    NotFound,
    /// Crypto envelope mismatch (AAD / tamper / wrong tenant unwrap).
    #[error("crypto envelope rejected: {0}")]
    Crypto(String),
}
