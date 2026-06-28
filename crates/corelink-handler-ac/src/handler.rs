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
    /// The tenant's **resolved per-tier storage cap in bytes** (from the
    /// Worker-trusted `x-corelink-storage-quota-bytes` header), threaded to the
    /// byte-accounting reservation to seed a fresh `tenant_storage_state` row.
    ///
    /// - `Some(n)`, `n > 0` — finite cap; a fresh row seeds `bytes_quota = n`.
    /// - `Some(0)` — genuinely-unlimited tier; a fresh row seeds the `0` sentinel.
    /// - `None` — indeterminate; a fresh row FAILS CLOSED (never created
    ///   uncapped). An existing row keeps its already-seeded cap.
    ///
    /// Defaults to `None` so existing `::new` call sites compile unchanged and
    /// inherit the fail-closed default; set with [`Self::with_storage_quota_bytes`].
    pub storage_quota_bytes: Option<i64>,
}

impl AcUpdateRequest {
    /// Construct from fields.
    ///
    /// `storage_quota_bytes` defaults to `None` (indeterminate cap → fail-closed
    /// on a fresh row); set the resolved cap with
    /// [`Self::with_storage_quota_bytes`].
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
            storage_quota_bytes: None,
        }
    }

    /// Attach the resolved per-tier storage cap (bytes) used to seed a fresh
    /// `tenant_storage_state` row. See [`Self::storage_quota_bytes`] for the
    /// `Some(n)` / `Some(0)` / `None` semantics.
    #[must_use]
    pub fn with_storage_quota_bytes(mut self, cap: Option<i64>) -> Self {
        self.storage_quota_bytes = cap;
        self
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

/// AC delete request — `DELETE /v1/ac/{tenant}/{action_digest}` (D-1).
///
/// DELETE is **idempotent**: deleting a present ref and deleting an
/// absent one both succeed (the route maps both to HTTP 204).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcDeleteRequest {
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

impl AcDeleteRequest {
    /// Construct from fields.
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

/// AC delete response — idempotent acknowledgement.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcDeleteResponse {
    /// True if the ref existed (and was removed); false if absent
    /// (idempotent no-op). The route returns 204 either way.
    pub existed: bool,
    /// Bytes reclaimed by the delete (the size of the removed AC payload),
    /// `0` when absent or unknown. The storage byte-accounting decorator
    /// releases exactly this many bytes from `tenant_storage_state.bytes_used`
    /// (red-team finding #1 / cluster-C).
    pub reclaimed_bytes: u64,
}

impl AcDeleteResponse {
    /// Construct reporting only existence (reclaimed size unknown ⇒ `0`).
    #[must_use]
    pub fn new(existed: bool) -> Self {
        Self {
            existed,
            reclaimed_bytes: 0,
        }
    }

    /// Construct carrying the reclaimed byte size.
    #[must_use]
    pub fn with_reclaimed(existed: bool, reclaimed_bytes: u64) -> Self {
        Self {
            existed,
            reclaimed_bytes,
        }
    }
}

/// AC list request — `GET /v1/ac/{tenant}` paginated ref enumeration (D-7).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcListRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Caller principal.
    pub principal: String,
    /// Caller's authenticated tenant.
    pub caller_tenant: String,
    /// Max entries to return this page (route-clamped to `1..=1000`).
    pub limit: u32,
    /// Opaque pagination cursor from a prior page (`None` ⇒ first page).
    pub cursor: Option<String>,
    /// Wall-clock unix-millis.
    pub at_unix_ms: u64,
}

impl AcListRequest {
    /// Construct from fields.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        limit: u32,
        cursor: Option<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            limit,
            cursor,
            at_unix_ms,
        }
    }
}

/// One enumerated AC ref (the per-entry shape of the D-7 list body).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcRefEntry {
    /// The ref key (action digest, tenant-prefix stripped — never the
    /// raw storage key).
    pub ref_key: String,
    /// RFC-3339 last-update timestamp (storage object last-modified).
    pub updated_at: String,
    /// Stored result-payload size in bytes.
    pub size: u64,
}

impl AcRefEntry {
    /// Construct from fields.
    #[must_use]
    pub fn new(ref_key: impl Into<String>, updated_at: impl Into<String>, size: u64) -> Self {
        Self {
            ref_key: ref_key.into(),
            updated_at: updated_at.into(),
            size,
        }
    }
}

/// AC list response — one page of refs + an opaque continuation cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AcListResponse {
    /// Refs on this page (already tenant-scoped + key-stripped).
    pub refs: Vec<AcRefEntry>,
    /// Opaque cursor for the next page, or `None` when exhausted.
    pub next_cursor: Option<String>,
}

impl AcListResponse {
    /// Construct from fields.
    #[must_use]
    pub fn new(refs: Vec<AcRefEntry>, next_cursor: Option<String>) -> Self {
        Self { refs, next_cursor }
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

/// Trait every AC delete handler implements (D-1).
///
/// DELETE is **idempotent** (present + absent both `Ok`; the route maps
/// both to 204). Implementors **MUST**:
///
/// 1. On cross-tenant access emit `DeleteDenied` BEFORE the rejection.
/// 2. Emit `DeleteAttempted` BEFORE mutation.
/// 3. Emit `Sli::AvailAcLookup` on every entry (delete folds into the
///    AC availability bucket per the canonical-18 SLI registry).
pub trait AcDeleteHandler: Send + Sync + core::fmt::Debug {
    /// Delete one AC ref (idempotent).
    ///
    /// # Errors
    ///
    /// See [`AcHandlerError`].
    fn delete(&self, req: AcDeleteRequest) -> Result<AcDeleteResponse, AcHandlerError>;
}

/// Trait every AC list handler implements (D-7).
///
/// Enumeration MUST stay within the authenticated tenant's derived
/// storage prefix. Implementors **MUST**:
///
/// 1. On cross-tenant access emit `ListDenied` BEFORE the rejection.
/// 2. Emit `ListAttempted` BEFORE enumeration.
/// 3. Emit `Sli::AvailAcLookup` on every entry.
pub trait AcListHandler: Send + Sync + core::fmt::Debug {
    /// Enumerate one page of the tenant's AC refs.
    ///
    /// # Errors
    ///
    /// See [`AcHandlerError`].
    fn list(&self, req: AcListRequest) -> Result<AcListResponse, AcHandlerError>;
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
        let payload = g
            .get(&(req.tenant.clone(), req.action_digest.clone()))
            .cloned();
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
            let key = (req.tenant.clone(), req.action_digest.clone());
            if let Some(existing) = g.get(&key) {
                if *existing != req.result_payload {
                    // Idempotency violation: same key, divergent body. NEVER
                    // overwrite a proven AC result. UpdateAttempted already
                    // fired; no UpdateCommitted follows — the 409 is the signal.
                    drop(g);
                    emit(false);
                    return Err(AcHandlerError::DivergentBody {
                        tenant: req.tenant,
                        action_digest: req.action_digest,
                    });
                }
                // Byte-identical re-PUT → idempotent no-op.
                false
            } else {
                g.insert(key, req.result_payload);
                true
            }
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

impl AcDeleteHandler for InMemoryAcHandler {
    fn delete(&self, req: AcDeleteRequest) -> Result<AcDeleteResponse, AcHandlerError> {
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
                    kind: AuditEventKind::DeleteDenied,
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
                kind: AuditEventKind::DeleteAttempted,
                tenant: req.tenant.clone(),
                action_digest: req.action_digest.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AcHandlerError::AuditFailed)?;

        // Idempotent delete: remove if present, no-op if absent. Capture the
        // removed payload's length so the response can report `reclaimed_bytes`
        // (byte-accounting release).
        let (existed, reclaimed_bytes) = {
            let mut g = self
                .entries
                .lock()
                .map_err(|_| AcHandlerError::Internal("entry lock poisoned".into()))?;
            match g.remove(&(req.tenant.clone(), req.action_digest.clone())) {
                Some(payload) => (true, payload.len() as u64),
                None => (false, 0u64),
            }
        };

        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::DeleteCommitted,
                tenant: req.tenant.clone(),
                action_digest: req.action_digest.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AcHandlerError::AuditFailed)?;
        emit(false);
        Ok(AcDeleteResponse::with_reclaimed(existed, reclaimed_bytes))
    }
}

impl AcListHandler for InMemoryAcHandler {
    fn list(&self, req: AcListRequest) -> Result<AcListResponse, AcHandlerError> {
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
                    kind: AuditEventKind::ListDenied,
                    tenant: req.tenant.clone(),
                    action_digest: String::new(),
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
                kind: AuditEventKind::ListAttempted,
                tenant: req.tenant.clone(),
                action_digest: String::new(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(AcHandlerError::AuditFailed)?;

        let g = self
            .entries
            .lock()
            .map_err(|_| AcHandlerError::Internal("entry lock poisoned".into()))?;
        let mut refs: Vec<(String, usize)> = g
            .iter()
            .filter(|((t, _), _)| t == &req.tenant)
            .map(|((_, d), payload)| (d.clone(), payload.len()))
            .collect();
        drop(g);
        refs.sort_by(|a, b| a.0.cmp(&b.0));

        let after = req.cursor.clone();
        let limit = req.limit.max(1) as usize;
        let mut out = Vec::new();
        let mut next_cursor: Option<String> = None;
        for (d, size) in refs
            .into_iter()
            .filter(|(d, _)| after.as_ref().map_or(true, |c| d > c))
        {
            if out.len() == limit {
                next_cursor = out.last().map(|e: &AcRefEntry| e.ref_key.clone());
                break;
            }
            out.push(AcRefEntry::new(d, "1970-01-01T00:00:00Z", size as u64));
        }

        emit(false);
        Ok(AcListResponse {
            refs: out,
            next_cursor,
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
        InMemoryAcHandler,
    ) {
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
            storage_quota_bytes: None,
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
        assert!(obs
            .iter()
            .any(|o| o.sli == Sli::AvailAcLookup && !o.is_error));
        assert!(obs
            .iter()
            .any(|o| o.sli == Sli::LatencyAcHitP99 && !o.is_error));

        let rows = audit.snapshot().expect("audit");
        assert!(rows
            .iter()
            .any(|r| r.kind == AuditEventKind::LookupAttempted));
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
                storage_quota_bytes: None,
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
                storage_quota_bytes: None,
            })
            .expect_err("denied");
        assert!(matches!(err, AcHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::UpdateDenied);
    }

    #[test]
    fn update_divergent_body_returns_conflict_and_preserves_original() {
        let (_audit, _sli, h) = fixture();
        // First PUT stores "v1".
        let r1 = h
            .update(AcUpdateRequest::new("t1", "d1", b"v1".to_vec(), "p1", "t1", 1))
            .expect("first");
        assert!(r1.durable, "fresh insert is durable");
        // Byte-identical re-PUT → idempotent no-op (Ok, durable=false).
        let r2 = h
            .update(AcUpdateRequest::new("t1", "d1", b"v1".to_vec(), "p1", "t1", 2))
            .expect("identical replay is Ok");
        assert!(!r2.durable, "identical re-PUT is an idempotent no-op");
        // Divergent body for the SAME key → 409 conflict.
        let err = h
            .update(AcUpdateRequest::new(
                "t1",
                "d1",
                b"v2-DIFFERENT".to_vec(),
                "p1",
                "t1",
                3,
            ))
            .expect_err("divergent body must conflict");
        assert!(matches!(err, AcHandlerError::DivergentBody { .. }));
        // The proven result must survive — NO silent overwrite.
        let hit = h
            .lookup(AcLookupRequest::new("t1", "d1", "p1", "t1", 4))
            .expect("original still present");
        assert_eq!(hit.result_payload, b"v1".to_vec(), "original not overwritten");
    }
}
