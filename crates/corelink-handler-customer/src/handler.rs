//! Customer handler traits + `InMemoryCustomerHandler` deterministic
//! in-process fake.
//!
//! Six traits split the customer dashboard surface into independent
//! handler units that share the same `(audit, sli)` collaborator pair.
//! This keeps cross-handler invariants (audit-fail-CLOSED ordering,
//! SLI emit-on-entry via `Sli::AvailControlPlane`) provable by
//! composition rather than by inheritance.
//!
//! Implementors **MUST** on every return path:
//! 1. Emit one `Sli::AvailControlPlane` observation via the SLI
//!    observer (fail-CLOSED = error observation on denial/error paths).
//! 2. Emit the appropriate `Attempted` audit row BEFORE any lookup or
//!    mutation. Audit emit failure aborts with
//!    [`CustomerHandlerError::AuditFailed`].
//! 3. On cross-tenant denial: emit the `*Denied` audit row BEFORE
//!    returning [`CustomerHandlerError::CrossTenantDenied`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{AuditEvent, AuditEventKind, AuditSink};
use crate::error::CustomerHandlerError;
use crate::observer::{Sli, SliObservation, SliObserver};
use crate::request::{
    AuditQueryRequest, AuditQueryResponse, BillingRequest, BillingResponse, ByokStatus,
    CustomerAuditEventRow, KeyCreateRequest, KeyCreateResponse, KeyRevokeRequest,
    KeyRevokeResponse, KeysListRequest, KeysListResponse, OverviewRequest, OverviewResponse,
    PatRow, PortalRequest, PortalResponse, TeamInviteRequest, TeamInviteResponse, TeamListRequest,
    TeamListResponse, TeamMemberRow, TeamRemoveRequest, TeamRemoveResponse, UsageRequest,
    UsageResponse,
};

// ─── Trait definitions ────────────────────────────────────────────────────────

/// Trait every concrete customer overview handler implements.
///
/// Implementors **MUST**:
/// 1. Emit `AuditEventKind::OverviewAttempted` BEFORE the lookup.
/// 2. On `CrossTenantDenied`, emit `AuditEventKind::OverviewDenied`
///    BEFORE returning.
/// 3. Emit `Sli::AvailControlPlane` on EVERY return path.
pub trait CustomerOverviewHandler: Send + Sync + core::fmt::Debug {
    /// Serve one overview request.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn overview(&self, req: OverviewRequest) -> Result<OverviewResponse, CustomerHandlerError>;
}

/// Trait every concrete customer usage handler implements.
///
/// Implementors **MUST**:
/// 1. Emit `AuditEventKind::UsageAttempted` BEFORE the lookup.
/// 2. On `CrossTenantDenied`, emit `AuditEventKind::UsageDenied`
///    BEFORE returning.
/// 3. Emit `Sli::AvailControlPlane` on EVERY return path.
pub trait CustomerUsageHandler: Send + Sync + core::fmt::Debug {
    /// Serve one usage request.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn usage(&self, req: UsageRequest) -> Result<UsageResponse, CustomerHandlerError>;
}

/// Trait every concrete customer billing handler implements.
///
/// Implementors **MUST**:
/// 1. Emit `AuditEventKind::BillingAttempted` BEFORE the lookup.
/// 2. On `CrossTenantDenied`, emit `AuditEventKind::BillingDenied`
///    BEFORE returning.
/// 3. Emit `Sli::AvailControlPlane` on EVERY return path.
pub trait CustomerBillingHandler: Send + Sync + core::fmt::Debug {
    /// Serve one billing request.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn billing(&self, req: BillingRequest) -> Result<BillingResponse, CustomerHandlerError>;

    /// Generate a short-lived Stripe billing portal URL.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn portal_url(&self, req: PortalRequest) -> Result<PortalResponse, CustomerHandlerError>;
}

/// Trait every concrete customer keys handler implements.
///
/// Implementors **MUST** for mutations (create / revoke):
/// 1. Emit the appropriate `KeyCreate*` / `KeyRevoke*` audit row BEFORE
///    the mutation. Audit failure aborts with `AuditFailed`.
/// 2. On `CrossTenantDenied`, emit `AuditEventKind::KeysDenied` BEFORE
///    returning.
/// 3. Emit `Sli::AvailControlPlane` on EVERY return path.
/// 4. Revoke is idempotent: if the PAT is already revoked, return the
///    existing row with `revoked_at` still set (no error).
pub trait CustomerKeysHandler: Send + Sync + core::fmt::Debug {
    /// List all PATs for the caller's tenant.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn list(&self, req: KeysListRequest) -> Result<KeysListResponse, CustomerHandlerError>;

    /// Create a new PAT for the caller's tenant.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn create(&self, req: KeyCreateRequest) -> Result<KeyCreateResponse, CustomerHandlerError>;

    /// Revoke a PAT by id (idempotent).
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::NotFound`] if `pat_id` does not
    /// belong to `caller_tenant`. Other variants per the trait contract.
    fn revoke(&self, req: KeyRevokeRequest) -> Result<KeyRevokeResponse, CustomerHandlerError>;
}

/// Trait every concrete customer team handler implements.
///
/// Implementors **MUST** for invite:
/// 1. Emit `AuditEventKind::TeamInviteAttempted` BEFORE the mutation.
/// 2. On `CrossTenantDenied`, emit `AuditEventKind::TeamDenied` BEFORE
///    returning.
/// 3. Emit `Sli::AvailControlPlane` on EVERY return path.
pub trait CustomerTeamHandler: Send + Sync + core::fmt::Debug {
    /// List all team members for the caller's tenant.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn list(&self, req: TeamListRequest) -> Result<TeamListResponse, CustomerHandlerError>;

    /// Invite a new member to the caller's tenant.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn invite(&self, req: TeamInviteRequest) -> Result<TeamInviteResponse, CustomerHandlerError>;

    /// Remove a member's seat from the caller's tenant.
    ///
    /// Implementors **MUST**:
    /// 1. Emit `AuditEventKind::TeamRemoveAttempted` BEFORE the mutation.
    /// 2. Flip the member row to `removed` AND revoke every PAT the member holds
    ///    for this tenant — the load-bearing security effect (a removed seat must
    ///    lose data-plane access, not merely disappear from the list).
    /// 3. Reject removing the tenant `owner` (and self-removal of the owner).
    /// 4. Emit `AuditEventKind::TeamRemoveCommitted` AFTER the durable removal.
    /// 5. Emit `Sli::AvailControlPlane` on EVERY return path.
    ///
    /// # Errors
    ///
    /// `NotFound` when no such member; `CrossTenantDenied`/`Unauthorized` per the
    /// trait contract; other [`CustomerHandlerError`] variants on backend faults.
    fn remove(&self, req: TeamRemoveRequest) -> Result<TeamRemoveResponse, CustomerHandlerError>;
}

/// Trait every concrete customer audit handler implements.
///
/// Implementors **MUST**:
/// 1. Emit `AuditEventKind::AuditQueryAttempted` BEFORE the lookup.
/// 2. On `CrossTenantDenied`, emit `AuditEventKind::AuditQueryDenied`
///    BEFORE returning.
/// 3. Emit `Sli::AvailControlPlane` on EVERY return path.
pub trait CustomerAuditHandler: Send + Sync + core::fmt::Debug {
    /// Query audit events for the caller's tenant.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CustomerHandlerError`] per the trait
    /// contract.
    fn query(&self, req: AuditQueryRequest) -> Result<AuditQueryResponse, CustomerHandlerError>;
}

// ─── InMemoryCustomerHandler ──────────────────────────────────────────────────

/// Deterministic in-memory customer handler. Intended for unit tests
/// and the `apps/server` wire-up until the CF-Worker handler lands.
///
/// `InMemoryCustomerHandler` is `Clone`-free on purpose; it carries
/// `Arc<dyn AuditSink>` + `Arc<dyn SliObserver>` so the `apps/server`
/// router can hand the same instance to a per-request stack.
pub struct InMemoryCustomerHandler {
    /// Static per-tenant overview data (tenant_id → snapshot).
    overviews: Mutex<HashMap<String, OverviewResponse>>,
    /// Static per-tenant usage data (tenant_id → snapshot).
    usages: Mutex<HashMap<String, UsageResponse>>,
    /// Static per-tenant billing data (tenant_id → snapshot).
    billings: Mutex<HashMap<String, BillingResponse>>,
    /// PAT store: (tenant_id, pat_id) → PatRow.
    pats: Mutex<HashMap<(String, String), PatRow>>,
    /// PAT raw tokens: pat_id → token.
    pat_tokens: Mutex<HashMap<String, String>>,
    /// Team members: (tenant_id, user_id) → TeamMemberRow.
    team: Mutex<HashMap<(String, String), TeamMemberRow>>,
    /// Audit event store: tenant_id → Vec<CustomerAuditEventRow>.
    audit_store: Mutex<HashMap<String, Vec<CustomerAuditEventRow>>>,
    /// Audit sink collaborator.
    audit: Arc<dyn AuditSink>,
    /// SLI observer collaborator.
    sli: Arc<dyn SliObserver>,
}

impl core::fmt::Debug for InMemoryCustomerHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryCustomerHandler")
            .finish_non_exhaustive()
    }
}

impl InMemoryCustomerHandler {
    /// Construct an empty handler bound to the supplied collaborators.
    #[must_use]
    pub fn new(audit: Arc<dyn AuditSink>, sli: Arc<dyn SliObserver>) -> Self {
        Self {
            overviews: Mutex::new(HashMap::new()),
            usages: Mutex::new(HashMap::new()),
            billings: Mutex::new(HashMap::new()),
            pats: Mutex::new(HashMap::new()),
            pat_tokens: Mutex::new(HashMap::new()),
            team: Mutex::new(HashMap::new()),
            audit_store: Mutex::new(HashMap::new()),
            audit,
            sli,
        }
    }

    /// Seed an overview snapshot for a tenant (test fixture).
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed_overview(
        &self,
        tenant_id: impl Into<String>,
        overview: OverviewResponse,
    ) -> Result<(), CustomerHandlerError> {
        self.overviews
            .lock()
            .map_err(|_| CustomerHandlerError::Internal("overviews lock poisoned".into()))?
            .insert(tenant_id.into(), overview);
        Ok(())
    }

    /// Seed a usage snapshot for a tenant (test fixture).
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed_usage(
        &self,
        tenant_id: impl Into<String>,
        usage: UsageResponse,
    ) -> Result<(), CustomerHandlerError> {
        self.usages
            .lock()
            .map_err(|_| CustomerHandlerError::Internal("usages lock poisoned".into()))?
            .insert(tenant_id.into(), usage);
        Ok(())
    }

    /// Seed a billing snapshot for a tenant (test fixture).
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed_billing(
        &self,
        tenant_id: impl Into<String>,
        billing: BillingResponse,
    ) -> Result<(), CustomerHandlerError> {
        self.billings
            .lock()
            .map_err(|_| CustomerHandlerError::Internal("billings lock poisoned".into()))?
            .insert(tenant_id.into(), billing);
        Ok(())
    }

    /// Seed a PAT directly into storage (avoids audit row noise in
    /// fixtures that don't test the create path).
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed_pat(
        &self,
        tenant_id: impl Into<String>,
        pat: PatRow,
    ) -> Result<(), CustomerHandlerError> {
        let tenant = tenant_id.into();
        let pat_id = pat.pat_id.clone();
        self.pats
            .lock()
            .map_err(|_| CustomerHandlerError::Internal("pats lock poisoned".into()))?
            .insert((tenant, pat_id), pat);
        Ok(())
    }

    /// Seed a team member directly into storage.
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed_member(
        &self,
        tenant_id: impl Into<String>,
        member: TeamMemberRow,
    ) -> Result<(), CustomerHandlerError> {
        let tenant = tenant_id.into();
        let user_id = member.user_id.clone();
        self.team
            .lock()
            .map_err(|_| CustomerHandlerError::Internal("team lock poisoned".into()))?
            .insert((tenant, user_id), member);
        Ok(())
    }

    /// Seed audit rows directly for the audit query tests.
    ///
    /// # Errors
    ///
    /// Returns [`CustomerHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed_audit_rows(
        &self,
        tenant_id: impl Into<String>,
        rows: Vec<CustomerAuditEventRow>,
    ) -> Result<(), CustomerHandlerError> {
        self.audit_store
            .lock()
            .map_err(|_| CustomerHandlerError::Internal("audit_store lock poisoned".into()))?
            .insert(tenant_id.into(), rows);
        Ok(())
    }

    /// Emit one `Sli::AvailControlPlane` observation.
    fn emit_sli(&self, is_error: bool) {
        self.sli.observe(SliObservation {
            sli: Sli::AvailControlPlane,
            is_error,
            latency_us: 0,
        });
    }

    /// Emit an audit event; return `AuditFailed` on sink error.
    fn emit_audit(
        &self,
        kind: AuditEventKind,
        tenant: &str,
        principal: &str,
        resource: &str,
        at_unix_ms: u64,
    ) -> Result<(), CustomerHandlerError> {
        self.audit
            .emit(AuditEvent::new(
                kind, tenant, principal, resource, at_unix_ms,
            ))
            .map_err(CustomerHandlerError::AuditFailed)
    }

    /// Lock a mutex, emitting an error SLI observation on poison.
    fn lock_or_err<'a, T>(
        &self,
        m: &'a Mutex<T>,
        msg: &'static str,
    ) -> Result<std::sync::MutexGuard<'a, T>, CustomerHandlerError> {
        m.lock().map_err(|_| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(msg.into())
        })
    }
}

// ─── Trait impls ─────────────────────────────────────────────────────────────

impl CustomerOverviewHandler for InMemoryCustomerHandler {
    fn overview(&self, req: OverviewRequest) -> Result<OverviewResponse, CustomerHandlerError> {
        // Emit Attempted audit BEFORE lookup (fail-CLOSED: abort on error).
        self.emit_audit(
            AuditEventKind::OverviewAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let g = self.lock_or_err(&self.overviews, "overviews lock poisoned")?;

        let resp = g.get(&req.caller_tenant).cloned().ok_or_else(|| {
            self.emit_sli(true);
            CustomerHandlerError::NotFound {
                what: format!("overview for tenant={}", req.caller_tenant),
            }
        })?;
        drop(g);

        self.emit_audit(
            AuditEventKind::OverviewServed,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(resp)
    }
}

impl CustomerUsageHandler for InMemoryCustomerHandler {
    fn usage(&self, req: UsageRequest) -> Result<UsageResponse, CustomerHandlerError> {
        self.emit_audit(
            AuditEventKind::UsageAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let g = self.lock_or_err(&self.usages, "usages lock poisoned")?;

        let resp = g
            .get(&req.caller_tenant)
            .filter(|u| req.period.as_deref().map_or(true, |p| u.period == p))
            .cloned()
            .ok_or_else(|| {
                self.emit_sli(true);
                CustomerHandlerError::NotFound {
                    what: format!(
                        "usage for tenant={} period={:?}",
                        req.caller_tenant, req.period
                    ),
                }
            })?;
        drop(g);

        self.emit_audit(
            AuditEventKind::UsageServed,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(resp)
    }
}

impl CustomerBillingHandler for InMemoryCustomerHandler {
    fn billing(&self, req: BillingRequest) -> Result<BillingResponse, CustomerHandlerError> {
        self.emit_audit(
            AuditEventKind::BillingAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let g = self.lock_or_err(&self.billings, "billings lock poisoned")?;

        let resp = g.get(&req.caller_tenant).cloned().ok_or_else(|| {
            self.emit_sli(true);
            CustomerHandlerError::NotFound {
                what: format!("billing for tenant={}", req.caller_tenant),
            }
        })?;
        drop(g);

        self.emit_audit(
            AuditEventKind::BillingServed,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(resp)
    }

    fn portal_url(&self, req: PortalRequest) -> Result<PortalResponse, CustomerHandlerError> {
        // Portal URL generation is a billing sub-action; treated as
        // BillingAttempted / BillingServed for audit symmetry.
        self.emit_audit(
            AuditEventKind::BillingAttempted,
            &req.caller_tenant,
            &req.principal,
            "portal",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        // In-memory fake: return a deterministic stub URL.
        let url = format!(
            "https://billing.stripe.com/p/session/stub_{}",
            req.caller_tenant
        );

        self.emit_audit(
            AuditEventKind::BillingServed,
            &req.caller_tenant,
            &req.principal,
            "portal",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(PortalResponse::new(url))
    }
}

impl CustomerKeysHandler for InMemoryCustomerHandler {
    fn list(&self, req: KeysListRequest) -> Result<KeysListResponse, CustomerHandlerError> {
        // PAT list is read-only and always scoped to caller's own tenant;
        // no cross-tenant check required here (same pattern as overview).
        let g = self.lock_or_err(&self.pats, "pats lock poisoned")?;

        let pats: Vec<PatRow> = g
            .iter()
            .filter(|((tid, _), _)| tid == &req.caller_tenant)
            .map(|(_, v)| v.clone())
            .collect();
        drop(g);

        self.emit_sli(false);
        Ok(KeysListResponse::new(
            pats,
            ByokStatus::new("none", None, None),
        ))
    }

    fn create(&self, req: KeyCreateRequest) -> Result<KeyCreateResponse, CustomerHandlerError> {
        // Emit KeyCreateAttempted BEFORE mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::KeyCreateAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.name,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        // Generate deterministic fake PAT id + token.
        let pat_id = format!("pat_{}_{}", req.caller_tenant, req.at_unix_ms);
        let token = format!("clpat_{}_{}", req.caller_tenant, req.at_unix_ms);
        let created_at = format!("{}000", req.at_unix_ms);
        let row = PatRow::new(
            pat_id.clone(),
            req.name.clone(),
            req.scopes.clone(),
            created_at,
            None,
            None,
        );

        {
            let mut g = self.lock_or_err(&self.pats, "pats lock poisoned")?;
            g.insert((req.caller_tenant.clone(), pat_id.clone()), row.clone());
        }
        {
            let mut tg = self.lock_or_err(&self.pat_tokens, "pat_tokens lock poisoned")?;
            tg.insert(pat_id, token.clone());
        }

        self.emit_audit(
            AuditEventKind::KeyCreateCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.name,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(KeyCreateResponse::new(row, token))
    }

    fn revoke(&self, req: KeyRevokeRequest) -> Result<KeyRevokeResponse, CustomerHandlerError> {
        // Emit KeyRevokeAttempted BEFORE mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::KeyRevokeAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.pat_id,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let mut g = self.lock_or_err(&self.pats, "pats lock poisoned")?;

        let key = (req.caller_tenant.clone(), req.pat_id.clone());
        let row = g.get(&key).cloned().ok_or_else(|| {
            self.emit_sli(true);
            CustomerHandlerError::NotFound {
                what: format!("pat_id={} for tenant={}", req.pat_id, req.caller_tenant),
            }
        })?;

        // Idempotent: if already revoked, return the existing row.
        let updated = if row.revoked_at.is_some() {
            row
        } else {
            let revoked_at = format!("{}000", req.at_unix_ms);
            let updated = PatRow::new(
                row.pat_id.clone(),
                row.name.clone(),
                row.scopes.clone(),
                row.created_at.clone(),
                row.last_used_at.clone(),
                Some(revoked_at),
            );
            g.insert(key, updated.clone());
            updated
        };
        drop(g);

        self.emit_audit(
            AuditEventKind::KeyRevokeCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.pat_id,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(KeyRevokeResponse::new(updated))
    }
}

impl CustomerTeamHandler for InMemoryCustomerHandler {
    fn list(&self, req: TeamListRequest) -> Result<TeamListResponse, CustomerHandlerError> {
        let g = self.lock_or_err(&self.team, "team lock poisoned")?;

        let members: Vec<TeamMemberRow> = g
            .iter()
            .filter(|((tid, _), _)| tid == &req.caller_tenant)
            .map(|(_, v)| v.clone())
            .collect();
        drop(g);

        self.emit_sli(false);
        Ok(TeamListResponse::new(members))
    }

    fn invite(&self, req: TeamInviteRequest) -> Result<TeamInviteResponse, CustomerHandlerError> {
        // Emit TeamInviteAttempted BEFORE mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::TeamInviteAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.email,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let user_id = format!("user_{}_{}", req.caller_tenant, req.at_unix_ms);
        let joined_at = format!("{}000", req.at_unix_ms);
        let member = TeamMemberRow::new(
            user_id.clone(),
            req.email.clone(),
            req.role.clone(),
            joined_at,
            "invited",
        );

        {
            let mut g = self.lock_or_err(&self.team, "team lock poisoned")?;
            g.insert((req.caller_tenant.clone(), user_id), member.clone());
        }

        self.emit_audit(
            AuditEventKind::TeamInviteCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.email,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(TeamInviteResponse::new(member))
    }

    fn remove(&self, req: TeamRemoveRequest) -> Result<TeamRemoveResponse, CustomerHandlerError> {
        // Emit TeamRemoveAttempted BEFORE mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::TeamRemoveAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.target_user_id,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let key = (req.caller_tenant.clone(), req.target_user_id.clone());
        let member = {
            let mut g = self.lock_or_err(&self.team, "team lock poisoned")?;
            let mut m = match g.get(&key) {
                None => {
                    self.emit_sli(true);
                    return Err(CustomerHandlerError::NotFound {
                        what: "team member".to_owned(),
                    });
                }
                // The tenant owner's seat is not removable.
                Some(m) if m.role.eq_ignore_ascii_case("owner") => {
                    self.emit_sli(true);
                    return Err(CustomerHandlerError::Unauthorized(
                        "cannot remove the tenant owner".to_owned(),
                    ));
                }
                Some(m) => m.clone(),
            };
            // Flip to `removed` (retain as an audit tombstone).
            m.status = "removed".to_owned();
            g.insert(key, m.clone());
            m
        };

        // The InMemory handler does not bind PATs to a member principal (the keys
        // map is not principal-keyed), so it reports 0 revoked; the D1 handler is
        // where the real per-member PAT revocation happens (and is asserted).
        self.emit_audit(
            AuditEventKind::TeamRemoveCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.target_user_id,
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(TeamRemoveResponse::new(member, 0))
    }
}

impl CustomerAuditHandler for InMemoryCustomerHandler {
    fn query(&self, req: AuditQueryRequest) -> Result<AuditQueryResponse, CustomerHandlerError> {
        // Emit AuditQueryAttempted BEFORE lookup.
        self.emit_audit(
            AuditEventKind::AuditQueryAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        let g = self.lock_or_err(&self.audit_store, "audit_store lock poisoned")?;
        let all_rows = g.get(&req.caller_tenant).cloned().unwrap_or_default();
        drop(g);

        // Apply event_types filter if non-empty.
        let rows: Vec<CustomerAuditEventRow> = if req.event_types.is_empty() {
            all_rows
        } else {
            all_rows
                .into_iter()
                .filter(|r| req.event_types.iter().any(|et| et == &r.event_type))
                .collect()
        };

        self.emit_audit(
            AuditEventKind::AuditQueryServed,
            &req.caller_tenant,
            &req.principal,
            "",
            req.at_unix_ms,
        )
        .inspect_err(|_| self.emit_sli(true))?;

        self.emit_sli(false);
        Ok(AuditQueryResponse::new(rows))
    }
}
