//! Admin handler traits + in-memory fake.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{AuditEvent, AuditEventKind, AuditSink};
use crate::error::AdminHandlerError;
use crate::observer::{Sli, SliObservation, SliObserver};

/// Admin read request shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdminReadRequest {
    /// What is being read (tenant / quota / audit-page / etc.).
    pub resource: String,
    /// Caller principal (admin-RBAC-validated upstream).
    pub principal: String,
    /// True if the principal has the admin role.
    pub is_admin: bool,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
}

impl AdminReadRequest {
    /// Construct from fields. The struct is `#[non_exhaustive]`.
    #[must_use]
    pub fn new(
        resource: impl Into<String>,
        principal: impl Into<String>,
        is_admin: bool,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            resource: resource.into(),
            principal: principal.into(),
            is_admin,
            at_unix_ms,
        }
    }
}

/// Admin read response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdminReadResponse {
    /// Resource the response is keyed to.
    pub resource: String,
    /// Opaque body (JSON-encoded; the handler is body-agnostic).
    pub body: Vec<u8>,
}

/// Dual-approval token attached to admin mutations.
///
/// Per `dual_approval.md §3`, the `approver` principal MUST differ
/// from the initiator and the token MUST be valid at the current
/// wall-clock (verified upstream; the handler trusts the upstream
/// validation but enforces the self-approval check structurally).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DualApprovalToken {
    /// Opaque approval-id (UUID from the dual-approval ledger).
    pub approval_id: String,
    /// Principal that approved (second approver).
    pub approver: String,
}

impl DualApprovalToken {
    /// Construct from fields. The struct is `#[non_exhaustive]`.
    #[must_use]
    pub fn new(approval_id: impl Into<String>, approver: impl Into<String>) -> Self {
        Self {
            approval_id: approval_id.into(),
            approver: approver.into(),
        }
    }
}

/// Admin mutation kind (closed taxonomy at compile-time per
/// `dual_approval.md §3.1`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutateOp {
    /// Set the tier of a tenant.
    SetTenantTier {
        /// Tenant ID.
        tenant: String,
        /// New tier label.
        tier: String,
    },
    /// Rotate an admin API token.
    RotateAdminToken {
        /// Token id.
        token_id: String,
    },
}

impl MutateOp {
    /// Canonical resource string used in audit rows.
    #[must_use]
    pub fn resource(&self) -> String {
        match self {
            Self::SetTenantTier { tenant, .. } => format!("tenant:{tenant}"),
            Self::RotateAdminToken { token_id } => format!("admin_token:{token_id}"),
        }
    }

    /// Build a [`MutateOp::SetTenantTier`] variant. Provided
    /// because the enum is `#[non_exhaustive]` and the variant has
    /// named fields — external crates cannot use the field-init
    /// syntax across the enum boundary.
    #[must_use]
    pub fn set_tenant_tier(tenant: impl Into<String>, tier: impl Into<String>) -> Self {
        Self::SetTenantTier {
            tenant: tenant.into(),
            tier: tier.into(),
        }
    }

    /// Build a [`MutateOp::RotateAdminToken`] variant.
    #[must_use]
    pub fn rotate_admin_token(token_id: impl Into<String>) -> Self {
        Self::RotateAdminToken {
            token_id: token_id.into(),
        }
    }
}

/// Admin mutation request shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdminMutateRequest {
    /// What is being mutated.
    pub op: MutateOp,
    /// Initiator principal.
    pub initiator: String,
    /// True if the initiator has admin role.
    pub initiator_is_admin: bool,
    /// Required dual-approval token (None = self-only; rejected).
    pub approval: Option<DualApprovalToken>,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
}

impl AdminMutateRequest {
    /// Construct from fields.
    #[must_use]
    pub fn new(
        op: MutateOp,
        initiator: impl Into<String>,
        initiator_is_admin: bool,
        approval: Option<DualApprovalToken>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            op,
            initiator: initiator.into(),
            initiator_is_admin,
            approval,
            at_unix_ms,
        }
    }
}

/// Admin mutation response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdminMutateResponse {
    /// Resource that was mutated.
    pub resource: String,
    /// Approval id that authorized the mutation.
    pub approval_id: String,
}

/// Trait every admin read handler implements.
///
/// Implementors **MUST**:
///
/// 1. Emit `AuditEventKind::ReadAttempted` BEFORE the lookup.
/// 2. Emit `Sli::AvailControlPlane` on EVERY return path.
/// 3. On RBAC denial emit `AuditEventKind::ReadDenied` BEFORE
///    the rejection.
pub trait AdminReadHandler: Send + Sync + core::fmt::Debug {
    /// Serve one admin read.
    ///
    /// # Errors
    ///
    /// See [`AdminHandlerError`].
    fn read(&self, req: AdminReadRequest) -> Result<AdminReadResponse, AdminHandlerError>;
}

/// Trait every admin mutate handler implements.
///
/// Implementors **MUST**:
///
/// 1. Emit `AuditEventKind::MutateAttempted` BEFORE any check.
/// 2. Reject with `DualApprovalMissing` if `approval` is None.
/// 3. Reject with `DualApprovalSelfApproval` if `approval.approver`
///    equals `initiator`.
/// 4. Emit `MutateDualApprovalRejected` audit row BEFORE returning
///    on either dual-approval failure path.
/// 5. On RBAC denial emit `MutateDenied` BEFORE returning.
/// 6. On success emit `MutateCommitted` AFTER the state change.
/// 7. Emit `Sli::AvailControlPlane` on EVERY return path.
pub trait AdminMutateHandler: Send + Sync + core::fmt::Debug {
    /// Serve one admin mutation.
    ///
    /// # Errors
    ///
    /// See [`AdminHandlerError`].
    fn mutate(&self, req: AdminMutateRequest) -> Result<AdminMutateResponse, AdminHandlerError>;
}

/// Deterministic in-memory admin handler. Read side serves a
/// keyed body map; mutate side records mutations as resource->op
/// rows. Used by tests and the apps/server wire-up.
pub struct InMemoryAdminHandler {
    read_bodies: Mutex<HashMap<String, Vec<u8>>>,
    /// Resource -> last applied op (debug shape only).
    applied: Mutex<HashMap<String, MutateOp>>,
    audit: Arc<dyn AuditSink>,
    sli: Arc<dyn SliObserver>,
}

impl core::fmt::Debug for InMemoryAdminHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryAdminHandler")
            .finish_non_exhaustive()
    }
}

impl InMemoryAdminHandler {
    /// Construct an empty handler bound to collaborators.
    #[must_use]
    pub fn new(audit: Arc<dyn AuditSink>, sli: Arc<dyn SliObserver>) -> Self {
        Self {
            read_bodies: Mutex::new(HashMap::new()),
            applied: Mutex::new(HashMap::new()),
            audit,
            sli,
        }
    }

    /// Seed a read body for `resource` (test fixture).
    ///
    /// # Errors
    ///
    /// Returns `Internal` on lock poisoning.
    pub fn seed_read(
        &self,
        resource: impl Into<String>,
        body: impl Into<Vec<u8>>,
    ) -> Result<(), AdminHandlerError> {
        let mut g = self
            .read_bodies
            .lock()
            .map_err(|_| AdminHandlerError::Internal("read body lock poisoned".into()))?;
        g.insert(resource.into(), body.into());
        Ok(())
    }

    /// Snapshot of applied mutations (test introspection).
    ///
    /// # Errors
    ///
    /// Returns `Internal` on lock poisoning.
    pub fn applied_snapshot(&self) -> Result<HashMap<String, MutateOp>, AdminHandlerError> {
        self.applied
            .lock()
            .map(|g| g.clone())
            .map_err(|_| AdminHandlerError::Internal("applied lock poisoned".into()))
    }
}

impl AdminReadHandler for InMemoryAdminHandler {
    fn read(&self, req: AdminReadRequest) -> Result<AdminReadResponse, AdminHandlerError> {
        let emit = |outcome_is_err: bool| {
            self.sli.observe(SliObservation {
                sli: Sli::AvailControlPlane,
                is_error: outcome_is_err,
                latency_us: 0,
            });
        };

        if !req.is_admin {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::ReadDenied,
                    principal: req.principal.clone(),
                    resource: req.resource.clone(),
                    approver: None,
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(AdminHandlerError::AuditFailed)?;
            emit(true);
            return Err(AdminHandlerError::Forbidden {
                principal: req.principal,
            });
        }

        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::ReadAttempted,
                principal: req.principal.clone(),
                resource: req.resource.clone(),
                approver: None,
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AdminHandlerError::AuditFailed)?;

        let g = self
            .read_bodies
            .lock()
            .map_err(|_| AdminHandlerError::Internal("read body lock poisoned".into()))?;
        let body = g.get(&req.resource).cloned();
        drop(g);

        match body {
            Some(b) => {
                self.audit
                    .emit(AuditEvent {
                        kind: AuditEventKind::ReadServed,
                        principal: req.principal.clone(),
                        resource: req.resource.clone(),
                        approver: None,
                        at_unix_ms: req.at_unix_ms,
                    })
                    .map_err(AdminHandlerError::AuditFailed)?;
                emit(false);
                Ok(AdminReadResponse {
                    resource: req.resource,
                    body: b,
                })
            }
            None => {
                emit(true);
                Err(AdminHandlerError::NotFound { what: req.resource })
            }
        }
    }
}

impl AdminMutateHandler for InMemoryAdminHandler {
    fn mutate(&self, req: AdminMutateRequest) -> Result<AdminMutateResponse, AdminHandlerError> {
        let resource = req.op.resource();
        let emit = |outcome_is_err: bool| {
            self.sli.observe(SliObservation {
                sli: Sli::AvailControlPlane,
                is_error: outcome_is_err,
                latency_us: 0,
            });
        };

        // MutateAttempted audit FIRST — records intent regardless of
        // approval outcome.
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::MutateAttempted,
                principal: req.initiator.clone(),
                resource: resource.clone(),
                approver: req.approval.as_ref().map(|t| t.approver.clone()),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AdminHandlerError::AuditFailed)?;

        // RBAC.
        if !req.initiator_is_admin {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::MutateDenied,
                    principal: req.initiator.clone(),
                    resource: resource.clone(),
                    approver: req.approval.as_ref().map(|t| t.approver.clone()),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(AdminHandlerError::AuditFailed)?;
            emit(true);
            return Err(AdminHandlerError::Forbidden {
                principal: req.initiator,
            });
        }

        // Dual-approval enforcement.
        let token = match req.approval {
            Some(t) => t,
            None => {
                self.audit
                    .emit(AuditEvent {
                        kind: AuditEventKind::MutateDualApprovalRejected,
                        principal: req.initiator.clone(),
                        resource: resource.clone(),
                        approver: None,
                        at_unix_ms: req.at_unix_ms,
                    })
                    .map_err(AdminHandlerError::AuditFailed)?;
                emit(true);
                return Err(AdminHandlerError::DualApprovalMissing);
            }
        };

        if token.approver == req.initiator {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::MutateDualApprovalRejected,
                    principal: req.initiator.clone(),
                    resource: resource.clone(),
                    approver: Some(token.approver.clone()),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(AdminHandlerError::AuditFailed)?;
            emit(true);
            return Err(AdminHandlerError::DualApprovalSelfApproval {
                initiator: req.initiator,
                approver: token.approver,
            });
        }

        // Apply mutation.
        {
            let mut g = self
                .applied
                .lock()
                .map_err(|_| AdminHandlerError::Internal("applied lock poisoned".into()))?;
            g.insert(resource.clone(), req.op);
        }

        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::MutateCommitted,
                principal: req.initiator.clone(),
                resource: resource.clone(),
                approver: Some(token.approver.clone()),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AdminHandlerError::AuditFailed)?;
        emit(false);
        Ok(AdminMutateResponse {
            resource,
            approval_id: token.approval_id,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::InMemoryAuditSink;
    use crate::observer::InMemorySliObserver;

    fn fixture() -> (
        Arc<InMemoryAuditSink>,
        Arc<InMemorySliObserver>,
        InMemoryAdminHandler,
    ) {
        let a = Arc::new(InMemoryAuditSink::new());
        let s = Arc::new(InMemorySliObserver::new());
        let h = InMemoryAdminHandler::new(a.clone(), s.clone());
        (a, s, h)
    }

    #[test]
    fn read_happy_emits_avail_cp() {
        let (audit, sli, h) = fixture();
        h.seed_read("tenant:t1", b"{}").expect("seed");
        let resp = h
            .read(AdminReadRequest {
                resource: "tenant:t1".into(),
                principal: "admin".into(),
                is_admin: true,
                at_unix_ms: 1,
            })
            .expect("read");
        assert_eq!(resp.resource, "tenant:t1");
        assert_eq!(resp.body, b"{}".to_vec());
        assert!(sli
            .snapshot()
            .expect("sli")
            .iter()
            .any(|o| o.sli == Sli::AvailControlPlane && !o.is_error));
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::ReadAttempted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::ReadServed));
    }

    #[test]
    fn read_forbidden_audits_before_returning() {
        let (audit, _sli, h) = fixture();
        let err = h
            .read(AdminReadRequest {
                resource: "tenant:t1".into(),
                principal: "alice".into(),
                is_admin: false,
                at_unix_ms: 1,
            })
            .expect_err("forbidden");
        assert!(matches!(err, AdminHandlerError::Forbidden { .. }));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
    }

    #[test]
    fn mutate_without_dual_approval_rejected() {
        let (audit, _sli, h) = fixture();
        let err = h
            .mutate(AdminMutateRequest {
                op: MutateOp::SetTenantTier {
                    tenant: "t1".into(),
                    tier: "Team".into(),
                },
                initiator: "alice".into(),
                initiator_is_admin: true,
                approval: None,
                at_unix_ms: 1,
            })
            .expect_err("dual approval missing");
        assert!(matches!(err, AdminHandlerError::DualApprovalMissing));
        // No mutation.
        assert!(h.applied_snapshot().expect("snap").is_empty());
        // Two audit rows: Attempted then DualApprovalRejected.
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::MutateAttempted);
        assert_eq!(rows[1].kind, AuditEventKind::MutateDualApprovalRejected);
    }

    #[test]
    fn mutate_self_approval_rejected() {
        let (audit, _sli, h) = fixture();
        let err = h
            .mutate(AdminMutateRequest {
                op: MutateOp::SetTenantTier {
                    tenant: "t1".into(),
                    tier: "Team".into(),
                },
                initiator: "alice".into(),
                initiator_is_admin: true,
                approval: Some(DualApprovalToken {
                    approval_id: "a1".into(),
                    approver: "alice".into(),
                }),
                at_unix_ms: 1,
            })
            .expect_err("self approval");
        assert!(matches!(
            err,
            AdminHandlerError::DualApprovalSelfApproval { .. }
        ));
        assert!(h.applied_snapshot().expect("snap").is_empty());
        let rows = audit.snapshot().expect("audit");
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::MutateDualApprovalRejected));
    }

    #[test]
    fn mutate_happy_commits_and_audits() {
        let (audit, sli, h) = fixture();
        let resp = h
            .mutate(AdminMutateRequest {
                op: MutateOp::SetTenantTier {
                    tenant: "t1".into(),
                    tier: "Team".into(),
                },
                initiator: "alice".into(),
                initiator_is_admin: true,
                approval: Some(DualApprovalToken {
                    approval_id: "a1".into(),
                    approver: "bob".into(),
                }),
                at_unix_ms: 1,
            })
            .expect("commit");
        assert_eq!(resp.resource, "tenant:t1");
        assert_eq!(resp.approval_id, "a1");
        assert_eq!(h.applied_snapshot().expect("snap").len(), 1);

        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::MutateAttempted);
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::MutateCommitted));
        // Avail control plane emit present + non-error.
        assert!(sli
            .snapshot()
            .expect("sli")
            .iter()
            .any(|o| o.sli == Sli::AvailControlPlane && !o.is_error));
    }

    #[test]
    fn mutate_audit_failure_aborts_before_state_change() {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let h = InMemoryAdminHandler::new(audit.clone(), sli.clone());
        audit.inject_failure("d1 down").expect("inject");
        let err = h
            .mutate(AdminMutateRequest {
                op: MutateOp::SetTenantTier {
                    tenant: "t1".into(),
                    tier: "Team".into(),
                },
                initiator: "alice".into(),
                initiator_is_admin: true,
                approval: Some(DualApprovalToken {
                    approval_id: "a1".into(),
                    approver: "bob".into(),
                }),
                at_unix_ms: 1,
            })
            .expect_err("audit closed");
        assert!(matches!(err, AdminHandlerError::AuditFailed(_)));
        assert!(h.applied_snapshot().expect("snap").is_empty());
    }
}
