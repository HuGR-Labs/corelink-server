//! Admin handler traits + in-memory fake.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{AuditEvent, AuditEventKind, AuditSink};
use crate::error::AdminHandlerError;
use crate::ledger::{ApprovalLedger, ApprovalRejection, InMemoryApprovalLedger};
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
/// The **`approval_id`** is the load-bearing field: it names a record in the
/// [`ApprovalLedger`](crate::ledger::ApprovalLedger) that was created by an
/// independently-authenticated approver. At mutate time the handler looks the
/// id up in the ledger and the LEDGER — not this token — is the authority for
/// the second approver's identity, the approval's scope, and its single-use
/// state.
///
/// The **`approver`** field is **advisory only** (kept for wire-compat and
/// audit context). It is NOT trusted for the authorization decision: a client
/// can no longer self-grant by putting an arbitrary string here, because the
/// handler ignores it and consults the ledger's recorded, authenticated
/// approver instead.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DualApprovalToken {
    /// Opaque approval-id — the key into the persisted approval ledger.
    pub approval_id: String,
    /// Client-declared approver principal. **Advisory only** — the handler
    /// authorizes against the ledger's recorded approver, never this field.
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
/// 3. Verify the approval against the persisted
///    [`ApprovalLedger`](crate::ledger::ApprovalLedger) and **consume** it
///    (single-use). The ledger — NOT the request body — is the authority for
///    the second approver. Reject with `DualApprovalUnknown` (no record),
///    `DualApprovalSelfApproval` (recorded approver == initiator),
///    `DualApprovalScopeMismatch` (recorded for another resource),
///    `DualApprovalConsumed` (replay), or `ApprovalLedgerUnavailable`
///    (fail-CLOSED backend error).
/// 4. Emit `MutateDualApprovalRejected` audit row BEFORE returning
///    on ANY dual-approval failure path.
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
    /// Persisted second-approver ledger. The mutate path verifies +
    /// consumes an approval here before committing — this is what makes
    /// dual-approval a real two-person control rather than a free-text
    /// string compare.
    ledger: Arc<dyn ApprovalLedger>,
}

impl core::fmt::Debug for InMemoryAdminHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryAdminHandler")
            .finish_non_exhaustive()
    }
}

impl InMemoryAdminHandler {
    /// Construct an empty handler bound to collaborators, backed by a fresh
    /// **empty** in-memory approval ledger.
    ///
    /// Because the ledger starts empty, every mutation fails CLOSED
    /// (`DualApprovalUnknown`) until an approval is independently recorded.
    /// Callers that need a shared / durable ledger (the production D1 path,
    /// or tests that seed approvals) use [`new_with_ledger`](Self::new_with_ledger).
    #[must_use]
    pub fn new(audit: Arc<dyn AuditSink>, sli: Arc<dyn SliObserver>) -> Self {
        Self::new_with_ledger(audit, sli, Arc::new(InMemoryApprovalLedger::new()))
    }

    /// Construct a handler bound to collaborators AND an explicit approval
    /// ledger. The production container injects a D1-backed ledger here so
    /// approvals recorded by the independently-authenticated approver
    /// endpoint are what the mutate path verifies + consumes.
    #[must_use]
    pub fn new_with_ledger(
        audit: Arc<dyn AuditSink>,
        sli: Arc<dyn SliObserver>,
        ledger: Arc<dyn ApprovalLedger>,
    ) -> Self {
        Self {
            read_bodies: Mutex::new(HashMap::new()),
            applied: Mutex::new(HashMap::new()),
            audit,
            sli,
            ledger,
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

        // Dual-approval verification against the PERSISTED ledger, then
        // single-use CONSUME. The ledger — NOT the free-text request body —
        // is the authority for the second approver: a client can no longer
        // self-grant by supplying an arbitrary `approver` string, because the
        // handler ignores `token.approver` and verifies the ledger's recorded,
        // authenticated approver. Consume happens here (before the state
        // change) so a concurrent replay of the same approval cannot
        // double-spend.
        let verified = match self.ledger.verify_and_consume(
            &token.approval_id,
            &req.initiator,
            &resource,
        ) {
            Ok(v) => v,
            Err(rejection) => {
                // Audit the rejection BEFORE returning (fail-CLOSED ordering).
                let recorded_approver = match &rejection {
                    ApprovalRejection::SelfApproval { approver } => Some(approver.clone()),
                    _ => None,
                };
                self.audit
                    .emit(AuditEvent {
                        kind: AuditEventKind::MutateDualApprovalRejected,
                        principal: req.initiator.clone(),
                        resource: resource.clone(),
                        approver: recorded_approver,
                        at_unix_ms: req.at_unix_ms,
                    })
                    .map_err(AdminHandlerError::AuditFailed)?;
                emit(true);
                return Err(match rejection {
                    ApprovalRejection::Unknown => AdminHandlerError::DualApprovalUnknown {
                        approval_id: token.approval_id,
                    },
                    ApprovalRejection::ScopeMismatch => {
                        AdminHandlerError::DualApprovalScopeMismatch {
                            approval_id: token.approval_id,
                            resource,
                        }
                    }
                    ApprovalRejection::SelfApproval { approver } => {
                        AdminHandlerError::DualApprovalSelfApproval {
                            initiator: req.initiator,
                            approver,
                        }
                    }
                    ApprovalRejection::Consumed => AdminHandlerError::DualApprovalConsumed {
                        approval_id: token.approval_id,
                    },
                    ApprovalRejection::Backend(msg) => {
                        AdminHandlerError::ApprovalLedgerUnavailable(msg)
                    }
                });
            }
        };

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
                // The audit records the LEDGER-verified approver, never the
                // advisory request-body value.
                approver: Some(verified.approver.clone()),
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
    use crate::ledger::InMemoryApprovalLedger;
    use crate::observer::InMemorySliObserver;

    /// Fixture returning the shared approval ledger so tests can `record`
    /// a genuine second-approver approval before exercising `mutate`.
    fn fixture() -> (
        Arc<InMemoryAuditSink>,
        Arc<InMemorySliObserver>,
        Arc<InMemoryApprovalLedger>,
        InMemoryAdminHandler,
    ) {
        let a = Arc::new(InMemoryAuditSink::new());
        let s = Arc::new(InMemorySliObserver::new());
        let ledger = Arc::new(InMemoryApprovalLedger::new());
        let h = InMemoryAdminHandler::new_with_ledger(a.clone(), s.clone(), ledger.clone());
        (a, s, ledger, h)
    }

    #[test]
    fn read_happy_emits_avail_cp() {
        let (audit, sli, _ledger, h) = fixture();
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
        let (audit, _sli, _ledger, h) = fixture();
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
        let (audit, _sli, _ledger, h) = fixture();
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

    /// Self-approval — but based on the LEDGER's recorded, authenticated
    /// approver, not the free-text body. Even though the request body claims
    /// `approver="bob"`, the ledger record for `a1` was created by `alice`
    /// (== initiator), so it is rejected. This proves the check is against the
    /// authenticated identity, not the client string.
    #[test]
    fn mutate_self_approval_rejected() {
        let (audit, _sli, ledger, h) = fixture();
        // The RECORDED approver is the initiator — a laundered self-approval.
        ledger.record("a1", "alice", "tenant:t1").expect("record");
        let err = h
            .mutate(AdminMutateRequest {
                op: MutateOp::SetTenantTier {
                    tenant: "t1".into(),
                    tier: "Team".into(),
                },
                initiator: "alice".into(),
                initiator_is_admin: true,
                // Body lies about the approver; the handler ignores it.
                approval: Some(DualApprovalToken {
                    approval_id: "a1".into(),
                    approver: "bob".into(),
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

    /// A single key-holder self-grant WITH a forged approver string and NO
    /// ledger record MUST be rejected `DualApprovalUnknown`. This is the exact
    /// H5 exploit: previously `approver != initiator` was the only check, so a
    /// fabricated approver string committed the mutation. Now, absent a
    /// recorded approval, it fails CLOSED.
    #[test]
    fn mutate_forged_approver_no_ledger_record_rejected() {
        let (audit, _sli, _ledger, h) = fixture();
        let err = h
            .mutate(AdminMutateRequest {
                op: MutateOp::SetTenantTier {
                    tenant: "t1".into(),
                    tier: "enterprise".into(),
                },
                initiator: "operator@internal".into(),
                initiator_is_admin: true,
                // Fabricated: distinct string, but no ledger record backs it.
                approval: Some(DualApprovalToken {
                    approval_id: "fabricated".into(),
                    approver: "totally-not-me".into(),
                }),
                at_unix_ms: 1,
            })
            .expect_err("forged approver rejected");
        assert!(matches!(
            err,
            AdminHandlerError::DualApprovalUnknown { .. }
        ));
        assert!(h.applied_snapshot().expect("snap").is_empty());
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::MutateAttempted);
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::MutateDualApprovalRejected));
    }

    /// An approval recorded for a DIFFERENT resource cannot authorize this
    /// mutation (scope-bound).
    #[test]
    fn mutate_scope_mismatch_rejected() {
        let (_audit, _sli, ledger, h) = fixture();
        ledger
            .record("a1", "bob", "tenant:OTHER")
            .expect("record");
        let err = h
            .mutate(AdminMutateRequest {
                op: MutateOp::set_tenant_tier("t1", "max"),
                initiator: "operator@internal".into(),
                initiator_is_admin: true,
                approval: Some(DualApprovalToken::new("a1", "bob")),
                at_unix_ms: 1,
            })
            .expect_err("scope mismatch");
        assert!(matches!(
            err,
            AdminHandlerError::DualApprovalScopeMismatch { .. }
        ));
        assert!(h.applied_snapshot().expect("snap").is_empty());
    }

    /// A consumed approval cannot be replayed (single-use anti-replay).
    #[test]
    fn mutate_replay_rejected() {
        let (_audit, _sli, ledger, h) = fixture();
        ledger.record("a1", "bob", "tenant:t1").expect("record");
        let mk = || AdminMutateRequest {
            op: MutateOp::set_tenant_tier("t1", "pro"),
            initiator: "operator@internal".into(),
            initiator_is_admin: true,
            approval: Some(DualApprovalToken::new("a1", "bob")),
            at_unix_ms: 1,
        };
        h.mutate(mk()).expect("first spend commits");
        let err = h.mutate(mk()).expect_err("replay rejected");
        assert!(matches!(err, AdminHandlerError::DualApprovalConsumed { .. }));
        // Exactly one applied mutation despite two attempts.
        assert_eq!(h.applied_snapshot().expect("snap").len(), 1);
    }

    /// Happy path: a GENUINE, distinct, recorded approval commits — and the
    /// `MutateCommitted` audit records the LEDGER approver (`bob`), not the
    /// body's advisory value.
    #[test]
    fn mutate_happy_commits_and_audits() {
        let (audit, sli, ledger, h) = fixture();
        ledger.record("a1", "bob", "tenant:t1").expect("record");
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
        let committed = rows
            .iter()
            .find(|r| r.kind == AuditEventKind::MutateCommitted)
            .expect("committed row");
        assert_eq!(
            committed.approver.as_deref(),
            Some("bob"),
            "audit records the ledger-verified approver"
        );
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
