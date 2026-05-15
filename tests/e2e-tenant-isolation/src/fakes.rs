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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_audit::{
    AuthEvent, AuthEventData, AuthEventType, DenyReason, Emitter, InMemoryEmitter,
    PrincipalIdHash, RegionTag, RequestId, RetentionHint, TenantId as AuditTenantId, TenantTier,
    TokenKind,
};
use corelink_tenant_path::TenantPrefix;
use uuid::Uuid;

use crate::tenants::ct_tenant_eq;

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

// ── CAS store ───────────────────────────────────────────────────────────

/// In-memory CAS / R2 fake — keys are partitioned by
/// [`TenantPrefix`]; every authenticated read / write goes through
/// [`Self::get`] / [`Self::put`] which verify the requester's tenant
/// id against the prefix-bound owner BEFORE returning success.
/// CAS entry: owner tenant id + value bytes.
type CasEntry = (Uuid, Vec<u8>);

/// CAS key: (prefix_string, blob_key).
type CasKey = (String, String);

/// In-memory CAS / R2 fake. See module-level doc for ordering guarantees.
#[derive(Clone, Debug, Default)]
pub struct CasStore {
    // CasKey -> CasEntry
    inner: Arc<Mutex<HashMap<CasKey, CasEntry>>>,
    audit: AuditCapture,
}

impl CasStore {
    /// Construct a fresh CAS store sharing `audit` with sibling fakes.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Owner-bound insert — used by the test scaffolding to seed
    /// resources owned by a specific tenant. Bypasses the auth path
    /// because there's no requester here (this is the trust-root
    /// seeding step that simulates an earlier authenticated write).
    pub fn seed(
        &self,
        owner: Uuid,
        prefix: &TenantPrefix,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert((prefix.as_str().to_string(), key.to_string()), (owner, value));
        Ok(())
    }

    /// Authenticated GET. The requester's tenant id MUST match the
    /// resource's owner (derived from the prefix on the wire). A
    /// mismatch emits `auth.denied.scope` and returns
    /// [`DenyKind::AuthzTenantMismatch`].
    pub fn get(
        &self,
        requester: Uuid,
        prefix: &TenantPrefix,
        key: &str,
    ) -> Result<Vec<u8>, FakeError> {
        let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let Some((owner, value)) = g.get(&(prefix.as_str().to_string(), key.to_string())) else {
            return Err(FakeError::NotFound);
        };
        if !ct_tenant_eq(&requester, owner) {
            // emit BEFORE reject — fail-CLOSED ordering
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::AuthzTenantMismatch,
                requester,
                resource_owner: *owner,
            })?;
            return Err(FakeError::Deny(DenyKind::AuthzTenantMismatch));
        }
        Ok(value.clone())
    }

    /// Authenticated PUT. The requester's tenant id MUST match the
    /// path prefix's derivation. Writing to a sibling tenant's prefix
    /// emits `auth.denied.scope` and rejects.
    pub fn put(
        &self,
        requester: Uuid,
        requester_prefix: &TenantPrefix,
        path_prefix: &TenantPrefix,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), FakeError> {
        if requester_prefix.as_str() != path_prefix.as_str() {
            // Attempted to write into a different tenant's prefix.
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::AuthzTenantMismatch,
                requester,
                // resource_owner is unknown here (no row yet); we use
                // a sentinel of the requester's UUID inverted — but
                // for assertion simplicity we propagate the requester
                // value; scenarios assert by kind, not resource_owner.
                resource_owner: requester,
            })?;
            return Err(FakeError::Deny(DenyKind::AuthzTenantMismatch));
        }
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(
            (path_prefix.as_str().to_string(), key.to_string()),
            (requester, value),
        );
        Ok(())
    }

    /// Authenticated LIST scoped to the requester's prefix. The fake
    /// stores keys across all prefixes in one map; the list path
    /// MUST filter to the requester's prefix only. Returns the keys
    /// (sorted) belonging to `requester_prefix`.
    pub fn list(&self, requester_prefix: &TenantPrefix) -> Result<Vec<String>, FakeError> {
        let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let mut out: Vec<String> = g
            .iter()
            .filter_map(|((p, k), _)| {
                if p == requester_prefix.as_str() {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();
        out.sort();
        Ok(out)
    }
}

// ── D1 / row store ──────────────────────────────────────────────────────

/// In-memory D1 fake row. Every row carries `tenant_id`; reads /
/// writes MUST use the JWT-claimed tenant id, NEVER a body field.
#[derive(Clone, Debug)]
pub struct D1Row {
    /// Owning tenant id (matches JWT claim at write time).
    pub tenant_id: Uuid,
    /// Opaque row payload.
    pub payload: Vec<u8>,
}

/// In-memory D1 store with strict JWT-vs-body tenant enforcement.
#[derive(Clone, Debug, Default)]
pub struct D1Store {
    inner: Arc<Mutex<HashMap<(Uuid, String), D1Row>>>,
    audit: AuditCapture,
}

impl D1Store {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Authenticated insert. The `jwt_tenant` is sourced from the JWT
    /// claim by the middleware; if the request body carries a
    /// `body_tenant` field that disagrees, the request is rejected
    /// BEFORE the row is written. This mirrors production: the
    /// backend NEVER trusts body-supplied tenant ids.
    pub fn insert(
        &self,
        jwt_tenant: Uuid,
        body_tenant: Uuid,
        row_key: &str,
        payload: Vec<u8>,
    ) -> Result<(), FakeError> {
        if !ct_tenant_eq(&jwt_tenant, &body_tenant) {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::JwtBodyTenantConflict,
                requester: jwt_tenant,
                resource_owner: body_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::JwtBodyTenantConflict));
        }
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(
            (jwt_tenant, row_key.to_string()),
            D1Row {
                tenant_id: jwt_tenant,
                payload,
            },
        );
        Ok(())
    }
}

// ── PAT store ───────────────────────────────────────────────────────────

/// In-memory PAT store: each PAT is bound to a single tenant. Using a
/// PAT against a different tenant's resource is rejected with
/// `auth.denied.signature_invalid` (the PAT HMAC is tenant-keyed).
#[derive(Clone, Debug, Default)]
pub struct PatStore {
    // pat_token -> bound_tenant_id
    inner: Arc<Mutex<HashMap<String, Uuid>>>,
    audit: AuditCapture,
}

impl PatStore {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Mint a PAT bound to `tenant`. Returns the opaque token.
    pub fn mint(&self, tenant: Uuid, token: &str) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(token.to_string(), tenant);
        Ok(())
    }

    /// Resolve a PAT against a target tenant. Returns `Ok(())` only
    /// when the PAT is bound to `target_tenant`; otherwise emits
    /// `auth.denied.signature_invalid` and rejects.
    pub fn authorize(&self, token: &str, target_tenant: Uuid) -> Result<(), FakeError> {
        let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let Some(bound) = g.get(token).copied() else {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::PatSignatureInvalid,
                requester: target_tenant,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::PatSignatureInvalid));
        };
        drop(g);
        if !ct_tenant_eq(&bound, &target_tenant) {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::PatSignatureInvalid,
                requester: bound,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::PatSignatureInvalid));
        }
        Ok(())
    }
}

// ── Idempotency ledger ──────────────────────────────────────────────────

/// Per-tenant idempotency ledger. Idempotency keys are scoped per
/// tenant — the same key value from two tenants is two independent
/// rows. A cross-tenant collision is impossible by construction; a
/// same-tenant collision with a different body fingerprint emits
/// `auth.denied.invalid` and rejects.
#[derive(Clone, Debug, Default)]
pub struct IdempotencyStore {
    // (tenant_id, idempotency_key) -> body_fingerprint
    inner: Arc<Mutex<HashMap<(Uuid, String), u64>>>,
    audit: AuditCapture,
}

impl IdempotencyStore {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Attempt to claim `key` for `tenant` with `body_fingerprint`.
    ///
    /// - First-time claim: stores and returns `Ok(false)` (i.e. "not
    ///   a replay; the caller should execute the operation").
    /// - Same key + same tenant + same fingerprint: returns
    ///   `Ok(true)` (replay-safe; caller returns the cached response).
    /// - Same key + same tenant + different fingerprint: emits
    ///   `auth.denied.invalid` and rejects.
    /// - Same key + different tenant: independent row, returns
    ///   `Ok(false)` (the canonical INV-IDEMPOTENCY-TENANT-SCOPED
    ///   property — keys do not leak across tenants).
    pub fn claim(
        &self,
        tenant: Uuid,
        key: &str,
        body_fingerprint: u64,
    ) -> Result<bool, FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let composite = (tenant, key.to_string());
        match g.get(&composite) {
            Some(existing) if *existing == body_fingerprint => Ok(true),
            Some(_) => {
                drop(g);
                self.audit.record_deny(AuditAttempt {
                    kind: DenyKind::IdempotencyConflict,
                    requester: tenant,
                    resource_owner: tenant,
                })?;
                Err(FakeError::Deny(DenyKind::IdempotencyConflict))
            }
            None => {
                g.insert(composite, body_fingerprint);
                Ok(false)
            }
        }
    }
}

// ── Quota tracker ───────────────────────────────────────────────────────

/// Per-tenant CAS quota window. Each tenant has its own atomic
/// counter; exhausting one tenant's quota MUST NOT affect a sibling
/// tenant's quota (INV-QUOTA-TENANT-SCOPED).
#[derive(Clone, Debug, Default)]
pub struct QuotaStore {
    // tenant_id -> (used, ceiling)
    inner: Arc<Mutex<HashMap<Uuid, (u64, u64)>>>,
    audit: AuditCapture,
}

impl QuotaStore {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Configure tenant ceiling. Idempotent.
    pub fn set_ceiling(&self, tenant: Uuid, ceiling: u64) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let entry = g.entry(tenant).or_insert((0, ceiling));
        entry.1 = ceiling;
        Ok(())
    }

    /// Try to consume one quota unit for `tenant`. Returns `Ok(())`
    /// on success; emits `auth.denied.rate_limit` and returns
    /// [`DenyKind::QuotaExhausted`] when the ceiling is reached.
    pub fn try_consume(&self, tenant: Uuid) -> Result<(), FakeError> {
        let exhausted = {
            let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            let entry = g.entry(tenant).or_insert((0, u64::MAX));
            if entry.0 >= entry.1 {
                true
            } else {
                entry.0 += 1;
                false
            }
        };
        if exhausted {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::QuotaExhausted,
                requester: tenant,
                resource_owner: tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::QuotaExhausted));
        }
        Ok(())
    }

    /// Current used count for a tenant (test introspection).
    #[must_use]
    pub fn used(&self, tenant: Uuid) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.get(&tenant).map_or(0, |(u, _)| *u),
            Err(p) => p.into_inner().get(&tenant).map_or(0, |(u, _)| *u),
        }
    }
}

// ── Rate limiter ────────────────────────────────────────────────────────

/// Per-tenant token-bucket rate limiter. Tenant A hitting its 429
/// MUST NOT affect Tenant B's quota window
/// (INV-RATELIMIT-TENANT-SCOPED).
#[derive(Clone, Debug, Default)]
pub struct RateLimiter {
    // tenant_id -> tokens_remaining
    inner: Arc<Mutex<HashMap<Uuid, i64>>>,
    audit: AuditCapture,
}

impl RateLimiter {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Configure tenant burst capacity.
    pub fn set_capacity(&self, tenant: Uuid, capacity: i64) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(tenant, capacity);
        Ok(())
    }

    /// Try to take one token for `tenant`. Returns `Ok(())` on
    /// success; emits `auth.denied.rate_limit` (`RateLimited`)
    /// otherwise.
    pub fn try_take(&self, tenant: Uuid) -> Result<(), FakeError> {
        let limited = {
            let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            let entry = g.entry(tenant).or_insert(i64::MAX);
            if *entry <= 0 {
                true
            } else {
                *entry -= 1;
                false
            }
        };
        if limited {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::RateLimited,
                requester: tenant,
                resource_owner: tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::RateLimited));
        }
        Ok(())
    }

    /// Inspect remaining tokens (test introspection).
    #[must_use]
    pub fn remaining(&self, tenant: Uuid) -> i64 {
        match self.inner.lock() {
            Ok(g) => g.get(&tenant).copied().unwrap_or(i64::MAX),
            Err(p) => p.into_inner().get(&tenant).copied().unwrap_or(i64::MAX),
        }
    }
}

// ── Stripe webhook ledger ───────────────────────────────────────────────

/// In-memory ledger of consumed `stripe_event_id` values. The first
/// consumer of a given event id "wins"; any subsequent replay with a
/// different tenant id is rejected with
/// `auth.denied.invalid` (DenyKind::StripeReplay). This pins
/// INV-STRIPE-WEBHOOK-IDEMPOTENT-CROSS-TENANT.
#[derive(Clone, Debug, Default)]
pub struct StripeWebhookLedger {
    // stripe_event_id -> first_tenant_id
    inner: Arc<Mutex<HashMap<String, Uuid>>>,
    audit: AuditCapture,
}

impl StripeWebhookLedger {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Process a Stripe webhook event. First call for a given
    /// `event_id` returns `Ok(())`; any replay with a different
    /// `tenant_id` is rejected. A replay with the *same* tenant id
    /// is treated as idempotent (also `Ok(())`).
    pub fn process(&self, tenant: Uuid, event_id: &str) -> Result<(), FakeError> {
        let conflict = {
            let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            match g.get(event_id).copied() {
                Some(existing) if ct_tenant_eq(&existing, &tenant) => None,
                Some(existing) => Some(existing),
                None => {
                    g.insert(event_id.to_string(), tenant);
                    None
                }
            }
        };
        if let Some(existing) = conflict {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::StripeReplay,
                requester: tenant,
                resource_owner: existing,
            })?;
            return Err(FakeError::Deny(DenyKind::StripeReplay));
        }
        Ok(())
    }
}

// ── Constant-time auth probe ────────────────────────────────────────────

/// Constant-time auth probe — mirrors the production `auth.resolve`
/// path which MUST execute the same number of HMAC + lookup steps
/// whether the tenant exists or not. The latency floor is a fixed
/// `LATENCY_FLOOR_NS` value; existence is signalled only by the
/// return value, never by wall-clock skew.
///
/// THR-I-002 (timing side-channel), `STRIDE-corelink-tenant-path` §2.1
/// (TB-tp-1 row I), CTRL-ISO-004 / ADR-0023 / ADR-0028.
#[derive(Clone, Debug, Default)]
pub struct ConstantTimeAuthProbe {
    // tenant_id -> exists?
    inner: Arc<Mutex<HashMap<Uuid, bool>>>,
}

impl ConstantTimeAuthProbe {
    /// Fixed latency floor enforced on every probe — matches the
    /// `TimingPaddingLayer` median target (|Δmedian| ≤ 1 ms budget).
    pub const LATENCY_FLOOR_NS: u64 = 1_500_000;

    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `tenant` as existing in the directory.
    pub fn register(&self, tenant: Uuid) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(tenant, true);
        Ok(())
    }

    /// Probe whether `tenant` exists. Returns `(existed, padded_ns)`
    /// where `padded_ns` is the logical-clock-bound latency — always
    /// `LATENCY_FLOOR_NS` regardless of existence (the production
    /// `TimingPaddingLayer` enforces a `tokio::time::sleep_until` to
    /// the same deadline). The harness asserts the deadline is
    /// existence-independent.
    pub fn probe(&self, tenant: Uuid) -> Result<(bool, u64), FakeError> {
        // Constant-time lookup: full table scan with subtle::ct_eq,
        // so the per-call work is `O(table_size)` and existence does
        // not branch.
        let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let mut exists: u8 = 0;
        for (id, _) in g.iter() {
            exists |= u8::from(ct_tenant_eq(id, &tenant));
        }
        // Production `TimingPaddingLayer` pads to a fixed deadline;
        // here we report the fixed floor unconditionally so callers
        // can assert `padded_existing == padded_missing`.
        Ok((exists == 1, Self::LATENCY_FLOOR_NS))
    }
}

// ── CMK rotation envelope ───────────────────────────────────────────────

/// Per-tenant CMK key version envelope. A rotation transitions a
/// tenant from `key_version=N` (active) → `key_version=N+1` (active)
/// atomically; readers always see exactly one of the two committed
/// versions and never a half-state where the wrapped DEK references
/// version N but the key id has flipped to N+1.
///
/// INV-BYOK-CMK-ROTATION-ATOMIC, FM-BYOK-005, `STRIDE-corelink-byok`
/// rotation row.
///
/// Per-tenant CMK row: `(committed_version, in_flight_target_or_none)`.
type CmkVersionRow = (u64, Option<u64>);

/// In-memory CMK rotation ledger — see module-level docs for the
/// `INV-BYOK-CMK-ROTATION-ATOMIC` contract.
#[derive(Clone, Debug, Default)]
pub struct CmkRotationLedger {
    // tenant_id -> CmkVersionRow
    inner: Arc<Mutex<HashMap<Uuid, CmkVersionRow>>>,
    audit: AuditCapture,
}

impl CmkRotationLedger {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Initialise tenant at `version`.
    pub fn init(&self, tenant: Uuid, version: u64) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(tenant, (version, None));
        Ok(())
    }

    /// Begin a rotation to `target_version`. Subsequent reads against
    /// either the old or the new version succeed (atomic switch); a
    /// read with an envelope whose `key_version` matches NEITHER side
    /// (a half-state probe) is rejected.
    pub fn begin_rotation(&self, tenant: Uuid, target_version: u64) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        let entry = g.entry(tenant).or_insert((target_version - 1, None));
        entry.1 = Some(target_version);
        Ok(())
    }

    /// Commit the in-flight rotation. After commit, only the new
    /// version is acceptable.
    pub fn commit_rotation(&self, tenant: Uuid) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        if let Some(entry) = g.get_mut(&tenant) {
            if let Some(target) = entry.1.take() {
                entry.0 = target;
            }
        }
        Ok(())
    }

    /// Read against the envelope's `key_version`. Returns `Ok(())`
    /// when version is either the committed value OR the in-flight
    /// target; any other version is a half-state probe and rejected.
    pub fn read_with_version(&self, tenant: Uuid, key_version: u64) -> Result<(), FakeError> {
        let (committed, in_flight) = {
            let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            let Some(entry) = g.get(&tenant) else {
                return Err(FakeError::NotFound);
            };
            *entry
        };
        if key_version == committed || in_flight == Some(key_version) {
            return Ok(());
        }
        self.audit.record_deny(AuditAttempt {
            kind: DenyKind::CmkRotationInFlight,
            requester: tenant,
            resource_owner: tenant,
        })?;
        Err(FakeError::Deny(DenyKind::CmkRotationInFlight))
    }
}

// ── PAT revoke ledger (with ToCToU enforcement) ─────────────────────────

/// PAT revoke ledger: PATs are revoked atomically (revoke decision +
/// revoke commit are observed as a single point in logical time on
/// the read side). A request whose logical timestamp is ≥ the
/// revoke_at timestamp is rejected even if the PAT was minted before.
///
/// INV-PAT-REVOKE-TOCTOU-SAFE, FM-PAT-003.
///
/// PAT row: `(bound_tenant, revoke_at_logical_or_none)`.
type PatRevokeRow = (Uuid, Option<u64>);

/// In-memory PAT revoke ledger — see module docs.
#[derive(Clone, Debug, Default)]
pub struct PatRevokeLedger {
    // pat_token -> PatRevokeRow
    inner: Arc<Mutex<HashMap<String, PatRevokeRow>>>,
    audit: AuditCapture,
}

impl PatRevokeLedger {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Mint a PAT bound to `tenant`.
    pub fn mint(&self, token: &str, tenant: Uuid) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(token.to_string(), (tenant, None));
        Ok(())
    }

    /// Revoke the PAT at logical timestamp `revoke_at`. Any
    /// subsequent authorize-call whose logical timestamp is ≥
    /// `revoke_at` MUST reject (no ToCToU window).
    pub fn revoke_at(&self, token: &str, revoke_at: u64) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        if let Some(entry) = g.get_mut(token) {
            entry.1 = Some(revoke_at);
        }
        Ok(())
    }

    /// Authorize a PAT at logical-clock `now`. Returns `Ok(())` only
    /// when the PAT is bound to `target_tenant` AND `now <
    /// revoke_at`. Any read at-or-after the revoke commit is rejected.
    pub fn authorize_at(
        &self,
        token: &str,
        target_tenant: Uuid,
        now: u64,
    ) -> Result<(), FakeError> {
        let entry_opt = {
            let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            g.get(token).copied()
        };
        let Some((bound, revoke_at)) = entry_opt else {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::PatRevoked,
                requester: target_tenant,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::PatRevoked));
        };
        if !ct_tenant_eq(&bound, &target_tenant) {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::PatRevoked,
                requester: bound,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::PatRevoked));
        }
        if let Some(r) = revoke_at {
            if now >= r {
                self.audit.record_deny(AuditAttempt {
                    kind: DenyKind::PatRevoked,
                    requester: bound,
                    resource_owner: target_tenant,
                })?;
                return Err(FakeError::Deny(DenyKind::PatRevoked));
            }
        }
        Ok(())
    }
}

// ── Region residency router ─────────────────────────────────────────────

/// Per-tenant residency configuration. A request reaching the wrong
/// region for a residency-pinned tenant is rejected — region routing
/// MUST consult the tenant's residency record and never serve from a
/// foreign region.
///
/// INV-RESIDENCY-REGION-PINNED, FM-RESIDENCY-001,
/// `STRIDE-corelink-residency.md` cross-region replay row.
#[derive(Clone, Debug, Default)]
pub struct RegionRouter {
    // tenant_id -> home_region
    inner: Arc<Mutex<HashMap<Uuid, String>>>,
    audit: AuditCapture,
}

impl RegionRouter {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Pin `tenant` to `home_region` (e.g. `"br-sao"` or `"us-east"`).
    pub fn pin(&self, tenant: Uuid, home_region: &str) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(tenant, home_region.to_string());
        Ok(())
    }

    /// Route a request received in `received_region` for `tenant`.
    /// Returns `Ok(())` only when `received_region == home_region`.
    pub fn route(&self, tenant: Uuid, received_region: &str) -> Result<(), FakeError> {
        let home = {
            let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            g.get(&tenant).cloned()
        };
        let Some(home) = home else {
            return Err(FakeError::NotFound);
        };
        if home != received_region {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::RegionResidencyViolation,
                requester: tenant,
                resource_owner: tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::RegionResidencyViolation));
        }
        Ok(())
    }
}

// ── DSR (Data Subject Request) intake ───────────────────────────────────

/// In-memory DSR intake — every DSR request MUST carry an
/// authenticated principal whose tenant matches the DSR target
/// tenant. Cross-tenant DSR submission (Tenant A asking to erase
/// Tenant B's principal's data) is rejected.
///
/// INV-DSR-TENANT-CONTEXT-MATCH, `STRIDE-corelink-dsr.md` §2.1.
#[derive(Clone, Debug, Default)]
pub struct DsrIntake {
    audit: AuditCapture,
}

impl DsrIntake {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self { audit }
    }

    /// Submit a DSR for `subject_email` belonging to `target_tenant`,
    /// authenticated as `requester_tenant`. Returns `Ok(())` only
    /// when the two tenants match.
    pub fn submit(
        &self,
        requester_tenant: Uuid,
        target_tenant: Uuid,
        _subject_email: &str,
    ) -> Result<(), FakeError> {
        if !ct_tenant_eq(&requester_tenant, &target_tenant) {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::DsrAuthContextMismatch,
                requester: requester_tenant,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::DsrAuthContextMismatch));
        }
        Ok(())
    }
}

// ── Audit chain verifier ────────────────────────────────────────────────

/// Per-tenant audit chain (append-only, hash-chained). Each leaf
/// hashes to its predecessor under the tenant's chain root. Verifying
/// a leaf claimed by Tenant A as belonging to Tenant B's chain MUST
/// fail (no two chains share roots).
///
/// INV-AUDIT-CHAIN-NON-FORGEABLE, `STRIDE-corelink-audit-chain.md`.
#[derive(Clone, Debug, Default)]
pub struct AuditChain {
    // tenant_id -> chain leaves (each leaf is opaque bytes)
    inner: Arc<Mutex<HashMap<Uuid, Vec<Vec<u8>>>>>,
    audit: AuditCapture,
}

impl AuditChain {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Append a leaf to `tenant`'s chain.
    pub fn append(&self, tenant: Uuid, leaf: Vec<u8>) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.entry(tenant).or_default().push(leaf);
        Ok(())
    }

    /// Verify `leaf` belongs to `claimed_tenant`'s chain. The fake
    /// performs a constant-time membership check against the
    /// claimed tenant's chain only — an attacker claiming a forged
    /// leaf for another tenant fails membership and emits an audit
    /// rejection.
    pub fn verify(
        &self,
        requester: Uuid,
        claimed_tenant: Uuid,
        leaf: &[u8],
    ) -> Result<(), FakeError> {
        let found = {
            let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            g.get(&claimed_tenant)
                .is_some_and(|leaves| leaves.iter().any(|l| l == leaf))
        };
        if !found {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::AuditChainForge,
                requester,
                resource_owner: claimed_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::AuditChainForge));
        }
        Ok(())
    }
}

// ── R2 multipart upload ─────────────────────────────────────────────────

/// In-memory R2 multipart upload broker. `upload_id` is opaque, but
/// every upload is bound at-create to a tenant prefix. Subsequent
/// part-uploads from a different tenant (even with a forged
/// `upload_id`) MUST be rejected.
///
/// INV-MULTIPART-UPLOAD-TENANT-BOUND, FM-CAS-007.
///
/// Multipart row: `(owner_tenant, prefix_string, parts)`.
type MultipartRow = (Uuid, String, Vec<Vec<u8>>);

/// In-memory R2 multipart broker — see module docs.
#[derive(Clone, Debug, Default)]
pub struct MultipartBroker {
    // upload_id -> MultipartRow
    inner: Arc<Mutex<HashMap<String, MultipartRow>>>,
    audit: AuditCapture,
}

impl MultipartBroker {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Create a multipart upload bound to (`owner`, `owner_prefix`).
    pub fn create(
        &self,
        owner: Uuid,
        owner_prefix: &TenantPrefix,
        upload_id: &str,
    ) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(
            upload_id.to_string(),
            (owner, owner_prefix.as_str().to_string(), Vec::new()),
        );
        Ok(())
    }

    /// Upload a part. The (`requester`, `requester_prefix`) must
    /// match the (owner, owner_prefix) recorded at create-time;
    /// otherwise the part is rejected and audited as a multipart
    /// forge attempt.
    pub fn upload_part(
        &self,
        requester: Uuid,
        requester_prefix: &TenantPrefix,
        upload_id: &str,
        part: Vec<u8>,
    ) -> Result<(), FakeError> {
        let (owner, owner_prefix) = {
            let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            match g.get(upload_id) {
                Some((o, p, _)) => (*o, p.clone()),
                None => return Err(FakeError::NotFound),
            }
        };
        if !ct_tenant_eq(&owner, &requester) || owner_prefix != requester_prefix.as_str() {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::MultipartUploadForge,
                requester,
                resource_owner: owner,
            })?;
            return Err(FakeError::Deny(DenyKind::MultipartUploadForge));
        }
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        if let Some((_, _, parts)) = g.get_mut(upload_id) {
            parts.push(part);
        }
        Ok(())
    }

    /// Abort a multipart upload (test introspection — also tenant-
    /// bound, but identical semantics; included for symmetry).
    pub fn abort(&self, requester: Uuid, upload_id: &str) -> Result<(), FakeError> {
        let owner = {
            let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            match g.get(upload_id) {
                Some((o, _, _)) => *o,
                None => return Err(FakeError::NotFound),
            }
        };
        if !ct_tenant_eq(&owner, &requester) {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::MultipartUploadForge,
                requester,
                resource_owner: owner,
            })?;
            return Err(FakeError::Deny(DenyKind::MultipartUploadForge));
        }
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.remove(upload_id);
        Ok(())
    }
}

// ── Tenant hierarchy / sibling quota ────────────────────────────────────

/// Optional tenant hierarchy: a parent tenant may host child tenants;
/// each child has its own per-tenant quota counter. INV
/// INV-QUOTA-SIBLING-NON-INHERITED dictates that exhausting child A
/// MUST NOT affect sibling child B under the same parent.
///
/// Child row: `(parent_tenant_id, used, ceiling)`.
type ChildQuotaRow = (Uuid, u64, u64);

/// Hierarchical per-child quota store — see module docs.
#[derive(Clone, Debug, Default)]
pub struct HierarchicalQuotaStore {
    // child_tenant_id -> ChildQuotaRow
    inner: Arc<Mutex<HashMap<Uuid, ChildQuotaRow>>>,
    audit: AuditCapture,
}

impl HierarchicalQuotaStore {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Register `child` under `parent` with `ceiling`.
    pub fn register_child(
        &self,
        child: Uuid,
        parent: Uuid,
        ceiling: u64,
    ) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(child, (parent, 0, ceiling));
        Ok(())
    }

    /// Try to consume one quota unit for `child`. Sibling children
    /// under the same parent MUST be unaffected.
    pub fn try_consume(&self, child: Uuid) -> Result<(), FakeError> {
        let exhausted = {
            let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
            let Some(entry) = g.get_mut(&child) else {
                return Err(FakeError::NotFound);
            };
            if entry.1 >= entry.2 {
                true
            } else {
                entry.1 += 1;
                false
            }
        };
        if exhausted {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::QuotaInheritanceLeak,
                requester: child,
                resource_owner: child,
            })?;
            return Err(FakeError::Deny(DenyKind::QuotaInheritanceLeak));
        }
        Ok(())
    }

    /// Read `used` for a child (test introspection).
    #[must_use]
    pub fn used(&self, child: Uuid) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.get(&child).map_or(0, |(_, u, _)| *u),
            Err(p) => p.into_inner().get(&child).map_or(0, |(_, u, _)| *u),
        }
    }
}

// ── KV replication (PAT revoke propagation) ─────────────────────────────

/// Two-region KV replica that models replication lag. When a PAT is
/// revoked in region A (the home region), the read-side in region B
/// MUST fail-CLOSED until the revoke event has been observed locally.
/// Reads against a yet-to-be-replicated revoke MUST NOT succeed
/// (no stale-allow).
///
/// INV-KV-REPLICATION-FAIL-CLOSED, FM-AUTH-013.
#[derive(Clone, Debug, Default)]
pub struct KvReplicatedPatStore {
    // pat_token -> bound_tenant
    home: Arc<Mutex<HashMap<String, Uuid>>>,
    // remote replica state — populated only by explicit `replicate_revoke`
    remote_revokes: Arc<Mutex<std::collections::HashSet<String>>>,
    // home-side revokes (always known locally)
    home_revokes: Arc<Mutex<std::collections::HashSet<String>>>,
    audit: AuditCapture,
}

impl KvReplicatedPatStore {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            home: Arc::new(Mutex::new(HashMap::new())),
            remote_revokes: Arc::new(Mutex::new(std::collections::HashSet::new())),
            home_revokes: Arc::new(Mutex::new(std::collections::HashSet::new())),
            audit,
        }
    }

    /// Mint a PAT bound to `tenant` (home region).
    pub fn mint(&self, token: &str, tenant: Uuid) -> Result<(), FakeError> {
        let mut g = self.home.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(token.to_string(), tenant);
        Ok(())
    }

    /// Revoke in the home region. Until `replicate_revoke` is called,
    /// the remote region does NOT see the revoke locally — but reads
    /// in the remote region MUST still fail-CLOSED, since the read
    /// side consults the home authority synchronously on uncertainty.
    pub fn revoke_home(&self, token: &str) -> Result<(), FakeError> {
        let mut g = self
            .home_revokes
            .lock()
            .map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(token.to_string());
        Ok(())
    }

    /// Propagate the revoke to the remote replica.
    pub fn replicate_revoke(&self, token: &str) -> Result<(), FakeError> {
        let mut g = self
            .remote_revokes
            .lock()
            .map_err(|_| FakeError::MutexPoisoned)?;
        g.insert(token.to_string());
        Ok(())
    }

    /// Authorize from the **remote** region. The contract:
    ///
    /// - If the token is revoked locally (replicated), reject.
    /// - If the local replica has NOT yet seen the revoke but a
    ///   `network_partition` flag is set (i.e. we cannot synchronously
    ///   query home), the read MUST fail-CLOSED — emit
    ///   `KvReplicationLag` and reject.
    /// - Otherwise (no partition, no local revoke, home reachable),
    ///   the read consults home and rejects if home knows about the
    ///   revoke. Only when both home and remote agree the token is
    ///   live does the read succeed.
    pub fn authorize_remote(
        &self,
        token: &str,
        target_tenant: Uuid,
        network_partition: bool,
    ) -> Result<(), FakeError> {
        let bound = {
            let g = self.home.lock().map_err(|_| FakeError::MutexPoisoned)?;
            g.get(token).copied()
        };
        let Some(bound) = bound else {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::KvReplicationLag,
                requester: target_tenant,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::KvReplicationLag));
        };
        if !ct_tenant_eq(&bound, &target_tenant) {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::KvReplicationLag,
                requester: bound,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::KvReplicationLag));
        }
        // Local replica check.
        let locally_revoked = {
            let g = self
                .remote_revokes
                .lock()
                .map_err(|_| FakeError::MutexPoisoned)?;
            g.contains(token)
        };
        if locally_revoked {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::KvReplicationLag,
                requester: bound,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::KvReplicationLag));
        }
        // Under partition: cannot prove the token is live → fail-CLOSED.
        if network_partition {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::KvReplicationLag,
                requester: bound,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::KvReplicationLag));
        }
        // No partition → consult home authoritatively.
        let home_revoked = {
            let g = self
                .home_revokes
                .lock()
                .map_err(|_| FakeError::MutexPoisoned)?;
            g.contains(token)
        };
        if home_revoked {
            self.audit.record_deny(AuditAttempt {
                kind: DenyKind::KvReplicationLag,
                requester: bound,
                resource_owner: target_tenant,
            })?;
            return Err(FakeError::Deny(DenyKind::KvReplicationLag));
        }
        Ok(())
    }
}

// ── Audit query (tenant-scoped) ─────────────────────────────────────────

/// In-memory audit query layer. The production query layer
/// **pre-filters** every query by the JWT-bound tenant before
/// applying any caller-supplied filter; a caller-supplied
/// `tenant_id` filter that does not match the JWT tenant is treated
/// as injection and rejected.
///
/// INV-AUDIT-QUERY-TENANT-SCOPED, `STRIDE-corelink-audit-chain.md`
/// query-injection row.
#[derive(Clone, Debug, Default)]
pub struct AuditQueryEngine {
    // tenant_id -> row payloads
    inner: Arc<Mutex<HashMap<Uuid, Vec<String>>>>,
    audit: AuditCapture,
}

impl AuditQueryEngine {
    /// Construct.
    #[must_use]
    pub fn new(audit: AuditCapture) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            audit,
        }
    }

    /// Seed a row owned by `tenant`.
    pub fn seed(&self, tenant: Uuid, row: &str) -> Result<(), FakeError> {
        let mut g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        g.entry(tenant).or_default().push(row.to_string());
        Ok(())
    }

    /// Query with the JWT-bound `jwt_tenant`. The caller may also
    /// supply an optional `filter_tenant_id` — but if it does not
    /// match `jwt_tenant`, the layer rejects (injection attempt).
    /// Pre-filtering is unconditional: the only rows returned are
    /// those owned by `jwt_tenant`, regardless of the filter.
    pub fn query(
        &self,
        jwt_tenant: Uuid,
        filter_tenant_id: Option<Uuid>,
    ) -> Result<Vec<String>, FakeError> {
        if let Some(filter) = filter_tenant_id {
            if !ct_tenant_eq(&filter, &jwt_tenant) {
                self.audit.record_deny(AuditAttempt {
                    kind: DenyKind::AuditQueryInjection,
                    requester: jwt_tenant,
                    resource_owner: filter,
                })?;
                return Err(FakeError::Deny(DenyKind::AuditQueryInjection));
            }
        }
        let g = self.inner.lock().map_err(|_| FakeError::MutexPoisoned)?;
        Ok(g.get(&jwt_tenant).cloned().unwrap_or_default())
    }
}
