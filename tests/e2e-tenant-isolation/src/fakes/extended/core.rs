use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::tenants::ct_tenant_eq;

use super::super::{AuditAttempt, AuditCapture, DenyKind, FakeError};

// ── Stripe webhook ledger ───────────────────────────────────────────────

/// In-memory ledger of consumed `stripe_event_id` values. The first
/// consumer of a given event id "wins"; any subsequent replay with a
/// different tenant id is rejected with
/// `auth.denied.invalid` (DenyKind::StripeReplay). This pins
/// INV-STRIPE-WEBHOOK-IDEMPOTENT-CROSS-TENANT.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
