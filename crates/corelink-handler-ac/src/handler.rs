//! AC lookup + update handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{AuditEvent, AuditEventKind, AuditSink};
use crate::error::AcHandlerError;
use crate::observer::{Sli, SliObservation, SliObserver};

/// AC lookup request — `GET /v1/ac/{tenant}/{action_digest}`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcLookupRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Canonical action digest from URL path.
    pub action_digest: String,
    /// Caller principal.
    pub principal: String,
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
}

impl AcLookupRequest {
    /// Construct from fields. The struct is `#[non_exhaustive]` per
    /// charter so callers outside the crate use this constructor.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        action_digest: impl Into<String>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            action_digest: action_digest.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
        }
    }
}

/// AC lookup response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcLookupResponse {
    /// Action digest that hit.
    pub action_digest: String,
    /// Action result payload (opaque bytes).
    pub result_payload: Vec<u8>,
}

impl AcLookupResponse {
    /// Construct from fields. `#[non_exhaustive]` means this is the only
    /// out-of-crate construction path (required by out-of-crate AC handler
    /// impls like the R2-backed one in `corelink-container`).
    #[must_use]
    pub fn new(action_digest: impl Into<String>, result_payload: impl Into<Vec<u8>>) -> Self {
        Self {
            action_digest: action_digest.into(),
            result_payload: result_payload.into(),
        }
    }
}

/// AC update request — `PUT /v1/ac/{tenant}/{action_digest}`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcUpdateRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Canonical action digest.
    pub action_digest: String,
    /// Result payload bytes.
    pub result_payload: Vec<u8>,
    /// Caller principal.
    pub principal: String,
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
}

impl AcUpdateRequest {
    /// Construct from fields.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        action_digest: impl Into<String>,
        result_payload: impl Into<Vec<u8>>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            action_digest: action_digest.into(),
            result_payload: result_payload.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
        }
    }
}

/// AC update response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcUpdateResponse {
    /// Action digest stored.
    pub action_digest: String,
    /// True if the entry was a fresh insert.
    pub durable: bool,
}

impl AcUpdateResponse {
    /// Construct from fields. `#[non_exhaustive]` means this is the only
    /// out-of-crate construction path (required by out-of-crate AC handler
    /// impls like the R2-backed one in `corelink-container`).
    #[must_use]
    pub fn new(action_digest: impl Into<String>, durable: bool) -> Self {
        Self {
            action_digest: action_digest.into(),
            durable,
        }
    }
}

/// Trait every AC lookup handler implements.
///
/// Implementors **MUST**:
///
/// 1. Emit `AuditEventKind::LookupAttempted` BEFORE storage read.
/// 2. Emit `Sli::AvailAcLookup` + `Sli::LatencyAcHitP99` observations
///    on EVERY return path.
/// 3. On cross-tenant access emit `LookupDenied` BEFORE the rejection.
pub trait AcLookupHandler: Send + Sync + core::fmt::Debug {
    /// Serve one AC lookup.
    ///
    /// # Errors
    ///
    /// See [`AcHandlerError`].
    fn lookup(&self, req: AcLookupRequest) -> Result<AcLookupResponse, AcHandlerError>;
}

/// Trait every AC update handler implements.
///
/// Implementors **MUST**:
///
/// 1. Emit `AuditEventKind::UpdateAttempted` BEFORE mutation.
/// 2. On audit failure abort with `AcHandlerError::AuditFailed`
///    **without** mutating the AC entry.
/// 3. Emit `Sli::AvailAcLookup` observation on every entry (the
///    update path's availability folds into the same SLO).
pub trait AcUpdateHandler: Send + Sync + core::fmt::Debug {
    /// Serve one AC update.
    ///
    /// # Errors
    ///
    /// See [`AcHandlerError`].
    fn update(&self, req: AcUpdateRequest) -> Result<AcUpdateResponse, AcHandlerError>;
}

/// Deterministic in-memory AC handler.
pub struct InMemoryAcHandler {
    entries: Mutex<HashMap<(String, String), Vec<u8>>>,
    audit: Arc<dyn AuditSink>,
    sli: Arc<dyn SliObserver>,
}

impl core::fmt::Debug for InMemoryAcHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryAcHandler").finish_non_exhaustive()
    }
}

impl InMemoryAcHandler {
    /// Construct an empty handler bound to collaborators.
    #[must_use]
    pub fn new(audit: Arc<dyn AuditSink>, sli: Arc<dyn SliObserver>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            audit,
            sli,
        }
    }

    fn check_tenant(req_tenant: &str, caller_tenant: &str) -> bool {
        req_tenant == caller_tenant
    }
}

impl AcLookupHandler for InMemoryAcHandler {
    fn lookup(&self, req: AcLookupRequest) -> Result<AcLookupResponse, AcHandlerError> {
        let emit = |outcome_is_err: bool| {
            self.sli.observe(SliObservation {
                sli: Sli::AvailAcLookup,
                is_error: outcome_is_err,
                latency_us: 0,
            });
            self.sli.observe(SliObservation {
                sli: Sli::LatencyAcHitP99,
                is_error: outcome_is_err,
                latency_us: 0,
            });
        };

        if !Self::check_tenant(&req.tenant, &req.caller_tenant) {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::LookupDenied,
                    tenant: req.tenant.clone(),
                    action_digest: req.action_digest.clone(),
                    principal: req.principal.clone(),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(AcHandlerError::AuditFailed)?;
            emit(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::LookupAttempted,
                tenant: req.tenant.clone(),
                action_digest: req.action_digest.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AcHandlerError::AuditFailed)?;

        let g = self
            .entries
            .lock()
            .map_err(|_| AcHandlerError::Internal("entry lock poisoned".into()))?;
        let payload = g.get(&(req.tenant.clone(), req.action_digest.clone())).cloned();
        drop(g);

        match payload {
            Some(p) => {
                self.audit
                    .emit(AuditEvent {
                        kind: AuditEventKind::LookupHit,
                        tenant: req.tenant.clone(),
                        action_digest: req.action_digest.clone(),
                        principal: req.principal.clone(),
                        at_unix_ms: req.at_unix_ms,
                    })
                    .map_err(AcHandlerError::AuditFailed)?;
                // Hit is NOT an error (Sli::AvailAcLookup numerator).
                emit(false);
                Ok(AcLookupResponse {
                    action_digest: req.action_digest,
                    result_payload: p,
                })
            }
            None => {
                self.audit
                    .emit(AuditEvent {
                        kind: AuditEventKind::LookupMiss,
                        tenant: req.tenant.clone(),
                        action_digest: req.action_digest.clone(),
                        principal: req.principal.clone(),
                        at_unix_ms: req.at_unix_ms,
                    })
                    .map_err(AcHandlerError::AuditFailed)?;
                // Miss is NOT an availability error (the handler
                // served correctly); but the latency SLO numerator
                // for hits-only ignores misses — we still emit the
                // latency observation so the histogram has the
                // sample. Downstream calculator buckets miss-only.
                emit(false);
                Err(AcHandlerError::Miss {
                    tenant: req.tenant,
                    action_digest: req.action_digest,
                })
            }
        }
    }
}

impl AcUpdateHandler for InMemoryAcHandler {
    fn update(&self, req: AcUpdateRequest) -> Result<AcUpdateResponse, AcHandlerError> {
        // AC updates fold availability into AvailAcLookup (single
        // bucket per the canonical-15 metric registry discipline).
        let emit = |outcome_is_err: bool| {
            self.sli.observe(SliObservation {
                sli: Sli::AvailAcLookup,
                is_error: outcome_is_err,
                latency_us: 0,
            });
        };

        if !Self::check_tenant(&req.tenant, &req.caller_tenant) {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::UpdateDenied,
                    tenant: req.tenant.clone(),
                    action_digest: req.action_digest.clone(),
                    principal: req.principal.clone(),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(AcHandlerError::AuditFailed)?;
            emit(true);
            return Err(AcHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::UpdateAttempted,
                tenant: req.tenant.clone(),
                action_digest: req.action_digest.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AcHandlerError::AuditFailed)?;

        let durable = {
            let mut g = self
                .entries
                .lock()
                .map_err(|_| AcHandlerError::Internal("entry lock poisoned".into()))?;
            g.insert(
                (req.tenant.clone(), req.action_digest.clone()),
                req.result_payload,
            )
            .is_none()
        };

        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::UpdateCommitted,
                tenant: req.tenant.clone(),
                action_digest: req.action_digest.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AcHandlerError::AuditFailed)?;
        emit(false);
        Ok(AcUpdateResponse {
            action_digest: req.action_digest,
            durable,
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

    fn fixture() -> (Arc<InMemoryAuditSink>, Arc<InMemorySliObserver>, InMemoryAcHandler) {
        let a = Arc::new(InMemoryAuditSink::new());
        let s = Arc::new(InMemorySliObserver::new());
        let h = InMemoryAcHandler::new(a.clone(), s.clone());
        (a, s, h)
    }

    #[test]
    fn lookup_hit_emits_sli_and_audit() {
        let (audit, sli, h) = fixture();
        h.update(AcUpdateRequest {
            tenant: "t1".into(),
            action_digest: "d1".into(),
            result_payload: b"r".to_vec(),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            at_unix_ms: 1,
        })
        .expect("update");
        let resp = h
            .lookup(AcLookupRequest {
                tenant: "t1".into(),
                action_digest: "d1".into(),
                principal: "p1".into(),
                caller_tenant: "t1".into(),
                at_unix_ms: 2,
            })
            .expect("hit");
        assert_eq!(resp.action_digest, "d1");
        assert_eq!(resp.result_payload, b"r".to_vec());

        let obs = sli.snapshot().expect("sli");
        assert!(obs.iter().any(|o| o.sli == Sli::AvailAcLookup && !o.is_error));
        assert!(obs.iter().any(|o| o.sli == Sli::LatencyAcHitP99 && !o.is_error));

        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupAttempted));
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupHit));
    }

    #[test]
    fn lookup_miss_returns_miss_and_audits_miss() {
        let (audit, sli, h) = fixture();
        let err = h
            .lookup(AcLookupRequest {
                tenant: "t1".into(),
                action_digest: "nope".into(),
                principal: "p1".into(),
                caller_tenant: "t1".into(),
                at_unix_ms: 2,
            })
            .expect_err("miss");
        assert!(matches!(err, AcHandlerError::Miss { .. }));
        let rows = audit.snapshot().expect("audit");
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::LookupMiss));
        // SLI observation present (miss is NOT an availability error).
        let obs = sli.snapshot().expect("sli");
        assert!(obs.iter().any(|o| o.sli == Sli::AvailAcLookup));
    }

    #[test]
    fn lookup_cross_tenant_audits_before_denial() {
        let (audit, _sli, h) = fixture();
        let err = h
            .lookup(AcLookupRequest {
                tenant: "victim".into(),
                action_digest: "d1".into(),
                principal: "attacker".into(),
                caller_tenant: "attacker_t".into(),
                at_unix_ms: 2,
            })
            .expect_err("denied");
        assert!(matches!(err, AcHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::LookupDenied);
    }

    #[test]
    fn update_audit_failure_aborts_mutation() {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let h = InMemoryAcHandler::new(audit.clone(), sli.clone());
        audit.inject_failure("d1 down").expect("inject");
        let err = h
            .update(AcUpdateRequest {
                tenant: "t1".into(),
                action_digest: "d1".into(),
                result_payload: b"r".to_vec(),
                principal: "p1".into(),
                caller_tenant: "t1".into(),
                at_unix_ms: 1,
            })
            .expect_err("audit closed");
        assert!(matches!(err, AcHandlerError::AuditFailed(_)));
        let g = h.entries.lock().expect("lock");
        assert!(g.is_empty(), "no mutation under audit-fail-CLOSED");
    }

    #[test]
    fn update_cross_tenant_audits_before_denial() {
        let (audit, _sli, h) = fixture();
        let err = h
            .update(AcUpdateRequest {
                tenant: "victim".into(),
                action_digest: "d1".into(),
                result_payload: b"r".to_vec(),
                principal: "attacker".into(),
                caller_tenant: "attacker_t".into(),
                at_unix_ms: 1,
            })
            .expect_err("denied");
        assert!(matches!(err, AcHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::UpdateDenied);
    }
}
