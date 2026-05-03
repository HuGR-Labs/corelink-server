//! DSR ticket store trait + in-memory fake.
//!
//! ## Why idempotency on `(tenant_id, request_id)` is the canonical
//! ## forensic determinism rationale (WI-S11-001 §1 invariant)
//!
//! DSR submissions may be re-played multiple times across the
//! retention window (network retries; customer re-submission; SLA
//! poll timeouts). The canonical contract is
//!
//! > **same `(tenant_id, request_id)` → byte-identical ticket**
//!
//! A re-submission of the same logical request that produced a
//! divergent ticket (different request_kind / different
//! data_subject_id) would itself be a tampering signal — the auditor
//! evidence trail would lose its anchor. The store short-circuits the
//! second submission to the prior ticket so the orchestrator NEVER
//! double-inserts.
//!
//! Production wiring at WI-S11-008 binds this to the canonical Neon
//! `dsr_tickets` table per data_model.md §4.1 (UUIDv7 PK; idempotency
//! UNIQUE quad `(tenant_id, subject_user_id, request_kind,
//! request_payload_hash)`); the in-memory fake here pins the same
//! trait surface so the orchestrator's idempotency arm exercises the
//! same fail-CLOSED envelope discipline.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::error::DsrStoreError;
use crate::event::DsrTicket;

/// DSR ticket store trait. Production wiring composes the canonical
/// Neon `dsr_tickets` table at WI-S11-008 PRR ship gate per the
/// `trait-abstraction-defer` charter pattern.
pub trait DsrRequestStore: Send + Sync + core::fmt::Debug {
    /// Look up the canonical prior ticket for `(tenant_id,
    /// request_id)` (returns `Ok(None)` when this is a first-sighting
    /// pair).
    ///
    /// # Errors
    ///
    /// Returns [`DsrStoreError::Backend`] on backend failure.
    fn get(
        &self,
        tenant_id: Uuid,
        request_id: Uuid,
    ) -> Result<Option<DsrTicket>, DsrStoreError>;

    /// Insert a fresh ticket. Returns `Ok(true)` on first-sighting;
    /// returns `Ok(false)` when a ticket with the same (tenant_id,
    /// request_id) already exists with a matching payload (idempotent
    /// re-submission); returns
    /// [`DsrStoreError::DivergentPayload`] when a ticket with the
    /// same (tenant_id, request_id) exists with a divergent payload
    /// (SEV-1 forensic anomaly).
    ///
    /// # Errors
    ///
    /// - [`DsrStoreError::Backend`] on backend failure.
    /// - [`DsrStoreError::DivergentPayload`] when a row with the same
    ///   `(tenant_id, request_id)` exists with a divergent
    ///   `(data_subject_id, request_kind, jurisdiction)` tuple.
    fn insert(&self, ticket: DsrTicket) -> Result<bool, DsrStoreError>;
}

/// In-memory DSR ticket store backed by `BTreeMap<(tenant_id,
/// request_id), DsrTicket>`. Cloning shares the underlying map so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Debug, Default)]
pub struct InMemoryDsrRequestStore {
    inner: std::sync::Arc<Mutex<BTreeMap<(Uuid, Uuid), DsrTicket>>>,
}

impl InMemoryDsrRequestStore {
    /// Construct a fresh store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct (tenant_id, request_id) rows.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot every ticket stored so far (for tests + dashboard
    /// observability).
    #[must_use]
    pub fn snapshot(&self) -> Vec<DsrTicket> {
        match self.inner.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(p) => p.into_inner().values().cloned().collect(),
        }
    }

    /// Snapshot tickets scoped to a single tenant. Per-tenant tests +
    /// per-tenant dashboard widget surface.
    #[must_use]
    pub fn snapshot_for_tenant(&self, tenant_id: Uuid) -> Vec<DsrTicket> {
        match self.inner.lock() {
            Ok(g) => g
                .iter()
                .filter(|((t, _), _)| *t == tenant_id)
                .map(|(_, v)| v.clone())
                .collect(),
            Err(p) => p
                .into_inner()
                .iter()
                .filter(|((t, _), _)| *t == tenant_id)
                .map(|(_, v)| v.clone())
                .collect(),
        }
    }
}

impl DsrRequestStore for InMemoryDsrRequestStore {
    fn get(
        &self,
        tenant_id: Uuid,
        request_id: Uuid,
    ) -> Result<Option<DsrTicket>, DsrStoreError> {
        let guard = self.inner.lock().map_err(|_| {
            DsrStoreError::Backend("dsr store mutex poisoned (get)".to_string())
        })?;
        Ok(guard.get(&(tenant_id, request_id)).cloned())
    }

    fn insert(&self, ticket: DsrTicket) -> Result<bool, DsrStoreError> {
        let mut guard = self.inner.lock().map_err(|_| {
            DsrStoreError::Backend("dsr store mutex poisoned (insert)".to_string())
        })?;
        let key = (ticket.tenant_id, ticket.request_id);
        if let Some(existing) = guard.get(&key) {
            // Divergent-payload check: the canonical idempotency
            // contract is "same (tenant_id, request_id) → same
            // payload"; a replayed pair with a different
            // (data_subject_id, request_kind, jurisdiction) tuple is
            // a SEV-1 forensic anomaly the orchestrator surfaces
            // immediately.
            if existing.data_subject_id != ticket.data_subject_id
                || existing.request_kind != ticket.request_kind
                || existing.jurisdiction != ticket.jurisdiction
            {
                return Err(DsrStoreError::DivergentPayload(
                    ticket.request_id.to_string(),
                ));
            }
            return Ok(false);
        }
        guard.insert(key, ticket);
        Ok(true)
    }
}

/// Always-failing DSR ticket store for adversarial tests of the
/// fail-CLOSED envelope.
#[derive(Debug, Default)]
pub struct FailingDsrRequestStore;

impl FailingDsrRequestStore {
    /// Construct a fresh always-failing store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DsrRequestStore for FailingDsrRequestStore {
    fn get(
        &self,
        _tenant_id: Uuid,
        _request_id: Uuid,
    ) -> Result<Option<DsrTicket>, DsrStoreError> {
        Err(DsrStoreError::Backend(
            "induced dsr store failure (test fixture)".to_string(),
        ))
    }

    fn insert(&self, _ticket: DsrTicket) -> Result<bool, DsrStoreError> {
        Err(DsrStoreError::Backend(
            "induced dsr store failure (test fixture)".to_string(),
        ))
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
    use crate::event::{
        sla_for, DsrJurisdiction, DsrRequest, DsrRequestKind, DsrTicket,
    };
    use crate::receipt::JwtReceiptToken;

    fn req(t: Uuid, sub: Uuid, kind: DsrRequestKind) -> DsrRequest {
        DsrRequest::new(
            Uuid::now_v7(),
            t,
            sub,
            kind,
            DsrJurisdiction::Lgpd,
            1_000_000_000_000,
        )
    }

    fn accepted_ticket(req: &DsrRequest) -> DsrTicket {
        DsrTicket::accepted(
            req,
            sla_for(DsrJurisdiction::Lgpd, req.submitted_at_ms),
            JwtReceiptToken::synthetic_for_test("token"),
        )
    }

    #[test]
    fn first_sighting_inserts() {
        let s = InMemoryDsrRequestStore::new();
        let r = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let t = accepted_ticket(&r);
        let inserted = s.insert(t.clone()).unwrap();
        assert!(inserted);
        assert_eq!(s.len(), 1);
        let prior = s.get(t.tenant_id, t.request_id).unwrap();
        assert_eq!(prior, Some(t));
    }

    #[test]
    fn re_submission_idempotent_returns_false() {
        let s = InMemoryDsrRequestStore::new();
        let r = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let t = accepted_ticket(&r);
        s.insert(t.clone()).unwrap();
        let inserted = s.insert(t).unwrap();
        assert!(!inserted);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn divergent_payload_surfaces_error() {
        let s = InMemoryDsrRequestStore::new();
        let r1 = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let mut r2 = r1.clone();
        r2.request_kind = DsrRequestKind::Erasure;
        let t1 = accepted_ticket(&r1);
        let t2 = accepted_ticket(&r2);
        s.insert(t1).unwrap();
        let err = s.insert(t2).unwrap_err();
        assert!(matches!(err, DsrStoreError::DivergentPayload(_)));
        // Store len unchanged.
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn divergent_data_subject_surfaces_error() {
        let s = InMemoryDsrRequestStore::new();
        let r1 = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let mut r2 = r1.clone();
        r2.data_subject_id = Uuid::now_v7();
        let t1 = accepted_ticket(&r1);
        let t2 = accepted_ticket(&r2);
        s.insert(t1).unwrap();
        let err = s.insert(t2).unwrap_err();
        assert!(matches!(err, DsrStoreError::DivergentPayload(_)));
    }

    #[test]
    fn cross_tenant_isolation_pin() {
        let s = InMemoryDsrRequestStore::new();
        let ta = Uuid::now_v7();
        let tb = Uuid::now_v7();
        let same_rid = Uuid::now_v7();
        let mut ra = req(ta, Uuid::now_v7(), DsrRequestKind::Access);
        ra.request_id = same_rid;
        let mut rb = req(tb, Uuid::now_v7(), DsrRequestKind::Access);
        rb.request_id = same_rid;
        let ticket_a = accepted_ticket(&ra);
        let ticket_b = accepted_ticket(&rb);
        // Same request_id but different tenant — both insert
        // independently (tenant-leftmost key).
        assert!(s.insert(ticket_a.clone()).unwrap());
        assert!(s.insert(ticket_b.clone()).unwrap());
        assert_eq!(s.len(), 2);
        // Tenant A cannot see tenant B's ticket via its own scope.
        let a_view = s.snapshot_for_tenant(ta);
        assert_eq!(a_view.len(), 1);
        assert_eq!(a_view[0].tenant_id, ta);
    }

    #[test]
    fn lookup_returns_none_for_unknown_pair() {
        let s = InMemoryDsrRequestStore::new();
        let r = s.get(Uuid::now_v7(), Uuid::now_v7()).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn cloned_store_shares_storage() {
        let s1 = InMemoryDsrRequestStore::new();
        let s2 = s1.clone();
        let r = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let t = accepted_ticket(&r);
        s1.insert(t).unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn failing_store_returns_backend_error_on_get() {
        let s = FailingDsrRequestStore::new();
        let err = s.get(Uuid::now_v7(), Uuid::now_v7()).unwrap_err();
        assert!(matches!(err, DsrStoreError::Backend(_)));
    }

    #[test]
    fn failing_store_returns_backend_error_on_insert() {
        let s = FailingDsrRequestStore::new();
        let r = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let t = accepted_ticket(&r);
        let err = s.insert(t).unwrap_err();
        assert!(matches!(err, DsrStoreError::Backend(_)));
    }

    #[test]
    fn snapshot_returns_all_tickets() {
        let s = InMemoryDsrRequestStore::new();
        let r1 = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Access);
        let r2 = req(Uuid::now_v7(), Uuid::now_v7(), DsrRequestKind::Erasure);
        s.insert(accepted_ticket(&r1)).unwrap();
        s.insert(accepted_ticket(&r2)).unwrap();
        let snap = s.snapshot();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn empty_store_reports_empty() {
        let s = InMemoryDsrRequestStore::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }
}
