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
