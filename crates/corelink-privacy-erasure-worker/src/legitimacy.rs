//! DSR legitimacy pre-check (rt-nuclear #18/#19 — GDPR mass-erase authz).
//!
//! ## The authz gate this trait defends
//!
//! `/_internal/dsr/erase` is reached over the shared internal-auth key.
//! Possession of that key alone MUST NOT let a caller erase ANY tenant's
//! entire dataset by simply asserting a `tenant_id` in the request body
//! (GDPR Art. 17 mass-erase / cross-tenant destruction). The orchestrator
//! therefore binds every erase to a durable, D1-authenticated legitimacy
//! anchor: the `dsr_requested` row that the legitimate Clerk `user.deleted`
//! path writes at ENQUEUE time (`migrations/d1/0069_dsr_requested.sql`,
//! populated by `apps/signup-worker/src/webhooks/clerk.ts` with a
//! D1-authenticated `tenant_id`). A legitimate erasure ALWAYS has a
//! `dsr_requested` row matching `(dsr_id, tenant_id)`; a forged
//! body-asserted `(dsr_id, tenant_id)` does not.
//!
//! ## Fail-CLOSED on ambiguity
//!
//! Erasure is IRREVERSIBLE. Per ADR-S11-002 split-tier the DSR pipeline is
//! regulatory-grade fail-CLOSED, and this gate is the strongest expression
//! of that: a store error (D1 fault) is NOT treated as "allow" — it is
//! surfaced to the orchestrator which REJECTS. Ambiguous legitimacy ⇒ deny.
//! This is the OPPOSITE of a fail-open availability gate.

use uuid::Uuid;

/// Legitimacy-store failure surface. The D1-backed implementation lifts a
/// transport failure here; the orchestrator maps an `Err(_)` to a
/// `Rejected` decision (fail-CLOSED — see the module docs).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DsrLegitimacyError {
    /// Backend transport failure (D1 SELECT rejected / connection pool
    /// exhausted / row-decode failure). Fail-CLOSED: the orchestrator
    /// REJECTS rather than risk an unverified irreversible erase.
    #[error("dsr legitimacy store backend error: {0}")]
    Backend(String),
}

/// Legitimacy pre-check store. Production wiring binds this to the D1
/// `dsr_requested` table; the in-memory implementations below back the
/// crate's tests (mirroring the `ErasureIdempotencyLedger` /
/// `ErasureAuditSink` in-memory + failing fixtures).
pub trait DsrLegitimacyStore: Send + Sync + core::fmt::Debug {
    /// Return `true` iff a durable `dsr_requested` row exists for
    /// `(dsr_id, tenant_id)` with `status IN ('requested','verified')` —
    /// i.e. the erasure was legitimately enqueued by the D1-authenticated
    /// Clerk `user.deleted` path for THIS tenant.
    ///
    /// # Errors
    ///
    /// - [`DsrLegitimacyError::Backend`] for transport failures. The
    ///   orchestrator treats an error as fail-CLOSED (REJECT).
    fn is_requested(
        &self,
        dsr_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<bool, DsrLegitimacyError>;
}

/// In-memory legitimacy store keyed by `(dsr_id, tenant_id)`. Cloning
/// shares the underlying set so tests can seed legitimacy then hand the
/// store to the worker (mirrors `InMemoryErasureIdempotencyLedger`).
#[derive(Clone, Default, Debug)]
pub struct InMemoryDsrLegitimacyStore {
    inner: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<(Uuid, Uuid)>>>,
}

impl InMemoryDsrLegitimacyStore {
    /// Construct an empty store (no legitimate requests → every erase
    /// REJECTS until one is seeded).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a legitimate `(dsr_id, tenant_id)` request (the in-memory
    /// analogue of the Clerk `user.deleted` `INSERT OR IGNORE INTO
    /// dsr_requested`).
    pub fn insert_requested(&self, dsr_id: Uuid, tenant_id: Uuid) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert((dsr_id, tenant_id));
        }
    }
}

impl DsrLegitimacyStore for InMemoryDsrLegitimacyStore {
    fn is_requested(
        &self,
        dsr_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<bool, DsrLegitimacyError> {
        let g = self.inner.lock().map_err(|_| {
            DsrLegitimacyError::Backend("legitimacy store mutex poisoned".to_string())
        })?;
        Ok(g.contains(&(dsr_id, tenant_id)))
    }
}

/// Legacy / test-fixture store that treats EVERY `(dsr_id, tenant_id)` as
/// legitimate. This is the default installed by the 3-arg
/// [`crate::orchestrator::InMemoryErasureWorker::try_new`] so the crate's
/// existing fan-out tests (which exercise the erasure pipeline, NOT the
/// authz gate) keep passing unchanged.
///
/// ⛔ NEVER wire this into the production `/_internal/dsr/erase` route — the
/// real path MUST use a D1-backed [`DsrLegitimacyStore`]. The container
/// route uses [`crate::orchestrator::InMemoryErasureWorker::try_new_with_legitimacy`]
/// with a D1 store (and fail-CLOSES when D1 is absent), never this.
#[derive(Clone, Copy, Default, Debug)]
pub struct AllowAllDsrLegitimacyStore;

impl AllowAllDsrLegitimacyStore {
    /// Construct the allow-all (test/legacy) store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DsrLegitimacyStore for AllowAllDsrLegitimacyStore {
    fn is_requested(
        &self,
        _dsr_id: Uuid,
        _tenant_id: Uuid,
    ) -> Result<bool, DsrLegitimacyError> {
        Ok(true)
    }
}

/// Always-failing legitimacy store for adversarial tests of the
/// fail-CLOSED envelope (a store error MUST REJECT, never erase).
#[derive(Clone, Copy, Default, Debug)]
pub struct FailingDsrLegitimacyStore;

impl FailingDsrLegitimacyStore {
    /// Construct the always-failing store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DsrLegitimacyStore for FailingDsrLegitimacyStore {
    fn is_requested(
        &self,
        _dsr_id: Uuid,
        _tenant_id: Uuid,
    ) -> Result<bool, DsrLegitimacyError> {
        Err(DsrLegitimacyError::Backend(
            "induced legitimacy store failure (test fixture)".to_string(),
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    #[test]
    fn in_memory_absent_is_not_requested() {
        let s = InMemoryDsrLegitimacyStore::new();
        assert!(!s.is_requested(uuid(1), uuid(2)).unwrap());
    }

    #[test]
    fn in_memory_seeded_is_requested() {
        let s = InMemoryDsrLegitimacyStore::new();
        s.insert_requested(uuid(1), uuid(2));
        assert!(s.is_requested(uuid(1), uuid(2)).unwrap());
        // A DIFFERENT tenant for the same dsr_id is NOT legitimate
        // (cross-tenant forgery defense).
        assert!(!s.is_requested(uuid(1), uuid(9)).unwrap());
    }

    #[test]
    fn allow_all_always_true() {
        let s = AllowAllDsrLegitimacyStore::new();
        assert!(s.is_requested(uuid(1), uuid(2)).unwrap());
    }

    #[test]
    fn failing_store_errors() {
        let s = FailingDsrLegitimacyStore::new();
        assert!(s.is_requested(uuid(1), uuid(2)).is_err());
    }
}
