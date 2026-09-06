use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_tenant_path::TenantPrefix;
use uuid::Uuid;

use crate::tenants::ct_tenant_eq;

use super::{AuditAttempt, AuditCapture, DenyKind, FakeError};

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
    pub fn register_child(&self, child: Uuid, parent: Uuid, ceiling: u64) -> Result<(), FakeError> {
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
