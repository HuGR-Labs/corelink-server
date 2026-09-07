use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_tenant_path::TenantPrefix;
use uuid::Uuid;

use crate::tenants::ct_tenant_eq;

use super::super::{AuditAttempt, AuditCapture, DenyKind, FakeError};

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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
