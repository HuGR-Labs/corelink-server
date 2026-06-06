//! Atomic signup store trait + in-memory fakes.
//!
//! Models the D1 single-transaction insert pipeline (`tenant` →
//! `dpa_acceptance_pending` → `pat` → `usage_counter` → `COMMIT`) plus
//! the rollback arm. Per Lote 10.19 codex P0 canonical scope clarification
//! on `INV-ONBOARD-ATOMIC-PROVISIONING`: the atomic boundary covers
//! tenant + DPA + first PAT (+ usage counter) only; Stripe customer link
//! is eventually-consistent via saga compensation.
//!
//! The trait exposes explicit `begin` → step-wise insert → `commit` /
//! `rollback` semantics so the orchestrator can fail-CLOSED on any step
//! and the property test can inject failure at every step boundary to
//! verify zero orphan rows.
//!
//! Production wiring (deferred to PRR ship gate): a Cloudflare D1
//! binding that issues `BEGIN IMMEDIATE; ... COMMIT;` (or `ROLLBACK;`
//! on any step error) for the `signup_orchestration` + `tenant` +
//! `signup_attempts` + `pat` + `usage_counter` tables defined by
//! migration `0037_signup_orchestration.sql`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::pat::{PatHash, ShownOnceToken};
use crate::region::PrimaryRegion;
use crate::tenant::{SignupId, TenantId, UserEmailHash};

/// Storage layer error taxonomy.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageError {
    /// Statement-level error (FK constraint violation, UNIQUE
    /// violation, NOT NULL violation). The orchestrator MUST trigger
    /// the `rollback` arm.
    #[error("storage statement aborted: {0}")]
    Aborted(String),
    /// Tx begin / commit failed (D1 lock contention, schema
    /// corruption). Treated identically to `Aborted` by the
    /// orchestrator (rollback + emit `corelink.signup.failed`).
    #[error("storage tx fault: {0}")]
    TxFault(String),
    /// Internal invariant violation (e.g. `Mutex` poisoning).
    #[error("storage internal fault: {0}")]
    Internal(String),
}

/// Records persisted by an atomic signup tx — populated incrementally
/// as the orchestrator walks the step chain. On `commit` the store
/// flushes the records to the durable backing; on `rollback` the
/// records are discarded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingSignupRows {
    /// Tenant row (signup_id + tenant_id + email_hash + region).
    pub tenant: Option<TenantRow>,
    /// DPA acceptance pending marker row.
    pub dpa_pending: Option<DpaPendingRow>,
    /// First PAT row.
    pub first_pat: Option<PatRow>,
    /// Usage counter row.
    pub usage_counter: Option<UsageCounterRow>,
}

/// Tenant row content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantRow {
    /// Tenant id (UUID v7 in prod).
    pub tenant_id: TenantId,
    /// Signup id (idempotency cache key target).
    pub signup_id: SignupId,
    /// User-email hash (sha256 hex).
    pub email_hash: UserEmailHash,
    /// Primary region (Lote 10.16 cookie-canonical pin; immutable
    /// post-commit per INV-REGION-NO-CROSS-LEAK).
    pub primary_region: PrimaryRegion,
}

/// DPA pending row content (WI-S19-002 owns the rendered click-through
/// surface plus JWT receipt; this WI persists the pending marker
/// atomically with the tenant row).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpaPendingRow {
    /// Tenant id FK.
    pub tenant_id: TenantId,
}

/// First PAT row content (PAT material itself NEVER stored plaintext;
/// only the hash + shown-once token id appear here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatRow {
    /// Tenant id FK.
    pub tenant_id: TenantId,
    /// PAT hash (hybrid HMAC + Argon2id in prod).
    pub pat_hash: PatHash,
    /// Shown-once reveal token; first GET invalidates per WI §6.1.4.
    pub shown_once_token: ShownOnceToken,
}

/// Usage counter row content (initialised at 0 for both axes; reset_at
/// = now + 1 month in production wiring).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageCounterRow {
    /// Tenant id FK.
    pub tenant_id: TenantId,
}

/// Transaction-guard returned by [`AtomicSignupStore::begin`]. The
/// guard offers step-wise insert methods; the orchestrator commits or
/// rolls back via the trait methods on the store. Dropping the guard
/// without `commit` is **not** a rollback signal — the orchestrator
/// MUST explicitly call `rollback` to flush the audit trail. (The
/// in-memory fake leaks the guard if dropped without action; the
/// production wiring rolls back automatically on `Drop`, but the
/// orchestrator never relies on that.)
#[derive(Debug)]
#[non_exhaustive]
pub struct SignupTx {
    /// Opaque tx id (in-memory fake uses a monotonic counter; prod uses
    /// the D1 transaction handle id).
    pub tx_id: u64,
    /// Pending rows accumulated so far.
    pub pending: PendingSignupRows,
}

/// Atomic signup store trait. Production wiring is a Cloudflare D1
/// binding; the in-memory fake exercises the same step + commit /
/// rollback semantics.
pub trait AtomicSignupStore: core::fmt::Debug + Send + Sync {
    /// Open a new atomic transaction. Returns a fresh [`SignupTx`]
    /// guard.
    fn begin(&self) -> Result<SignupTx, StorageError>;

    /// Insert the `tenant` row. The fake may inject a synthetic failure
    /// at this boundary to verify the orchestrator's rollback arm.
    fn insert_tenant(&self, tx: &mut SignupTx, row: TenantRow) -> Result<(), StorageError>;

    /// Insert the `dpa_acceptance_pending` row.
    fn insert_dpa_pending(&self, tx: &mut SignupTx, row: DpaPendingRow)
        -> Result<(), StorageError>;

    /// Insert the first `pat` row.
    fn insert_first_pat(&self, tx: &mut SignupTx, row: PatRow) -> Result<(), StorageError>;

    /// Insert the `usage_counter` row.
    fn insert_usage_counter(
        &self,
        tx: &mut SignupTx,
        row: UsageCounterRow,
    ) -> Result<(), StorageError>;

    /// Commit the atomic tx. On success, the pending rows are flushed
    /// to the durable backing AND the idempotency cache is updated
    /// with `(IdempotencyKey, SignupId)` so duplicate requests return
    /// the original `signup_id`.
    fn commit(
        &self,
        tx: SignupTx,
        idempotency_key: &crate::idempotency::IdempotencyKey,
    ) -> Result<(), StorageError>;

    /// Roll back the atomic tx. All pending rows are discarded; the
    /// idempotency cache is NOT updated (the caller may retry).
    fn rollback(&self, tx: SignupTx) -> Result<(), StorageError>;

    /// Idempotency cache lookup. Returns the `(SignupId, TenantId)`
    /// pair previously committed for `idempotency_key`, if any.
    fn lookup_idempotent(
        &self,
        idempotency_key: &crate::idempotency::IdempotencyKey,
    ) -> Result<Option<(SignupId, TenantId)>, StorageError>;

    /// Snapshot the count of committed tenants (test introspection).
    fn committed_tenant_count(&self) -> Result<usize, StorageError>;
}

/// In-memory atomic signup store. Implements the full transaction
/// state machine and the idempotency cache.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAtomicSignupStore {
    state: Arc<Mutex<InMemoryState>>,
    /// Optional injected failure: if set, the matching step fails
    /// with [`StorageError::Aborted`] when invoked. Used by the
    /// property test to drive rollback at every step boundary.
    inject_failure_at: Arc<Mutex<Option<crate::outcome::OrchestrationStep>>>,
}

#[derive(Debug, Default)]
struct InMemoryState {
    next_tx_id: u64,
    tenants: Vec<TenantRow>,
    idempotency: HashMap<String, (SignupId, TenantId)>,
}

impl InMemoryAtomicSignupStore {
    /// Construct an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a synthetic failure at the given step. Subsequent
    /// matching insert calls return [`StorageError::Aborted`].
    pub fn inject_failure_at(&self, step: crate::outcome::OrchestrationStep) {
        if let Ok(mut g) = self.inject_failure_at.lock() {
            *g = Some(step);
        }
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, InMemoryState>, StorageError> {
        self.state
            .lock()
            .map_err(|e| StorageError::Internal(format!("state poisoned: {e}")))
    }

    fn current_failure(&self) -> Option<crate::outcome::OrchestrationStep> {
        match self.inject_failure_at.lock() {
            Ok(g) => *g,
            Err(p) => *p.into_inner(),
        }
    }

    fn maybe_inject(&self, step: crate::outcome::OrchestrationStep) -> Result<(), StorageError> {
        if self.current_failure() == Some(step) {
            return Err(StorageError::Aborted(format!("injected failure at {step}")));
        }
        Ok(())
    }
}

impl AtomicSignupStore for InMemoryAtomicSignupStore {
    fn begin(&self) -> Result<SignupTx, StorageError> {
        let mut g = self.lock_state()?;
        g.next_tx_id = g.next_tx_id.saturating_add(1);
        Ok(SignupTx {
            tx_id: g.next_tx_id,
            pending: PendingSignupRows::default(),
        })
    }

    fn insert_tenant(&self, tx: &mut SignupTx, row: TenantRow) -> Result<(), StorageError> {
        self.maybe_inject(crate::outcome::OrchestrationStep::InsertTenant)?;
        tx.pending.tenant = Some(row);
        Ok(())
    }

    fn insert_dpa_pending(
        &self,
        tx: &mut SignupTx,
        row: DpaPendingRow,
    ) -> Result<(), StorageError> {
        self.maybe_inject(crate::outcome::OrchestrationStep::InsertDpa)?;
        tx.pending.dpa_pending = Some(row);
        Ok(())
    }

    fn insert_first_pat(&self, tx: &mut SignupTx, row: PatRow) -> Result<(), StorageError> {
        self.maybe_inject(crate::outcome::OrchestrationStep::InsertFirstPat)?;
        tx.pending.first_pat = Some(row);
        Ok(())
    }

    fn insert_usage_counter(
        &self,
        tx: &mut SignupTx,
        row: UsageCounterRow,
    ) -> Result<(), StorageError> {
        self.maybe_inject(crate::outcome::OrchestrationStep::InsertUsageCounterAndCommit)?;
        tx.pending.usage_counter = Some(row);
        Ok(())
    }

    fn commit(
        &self,
        tx: SignupTx,
        idempotency_key: &crate::idempotency::IdempotencyKey,
    ) -> Result<(), StorageError> {
        // Per Lote 10.19 atomic boundary: all 4 rows MUST be present
        // on commit; missing rows is a programmer error.
        let tenant = tx
            .pending
            .tenant
            .ok_or_else(|| StorageError::TxFault("commit without tenant row".to_string()))?;
        if tx.pending.dpa_pending.is_none() {
            return Err(StorageError::TxFault(
                "commit without dpa_pending row".to_string(),
            ));
        }
        if tx.pending.first_pat.is_none() {
            return Err(StorageError::TxFault(
                "commit without first_pat row".to_string(),
            ));
        }
        if tx.pending.usage_counter.is_none() {
            return Err(StorageError::TxFault(
                "commit without usage_counter row".to_string(),
            ));
        }
        let mut g = self.lock_state()?;
        // UNIQUE constraint: same email_hash cannot appear twice.
        if g.tenants.iter().any(|t| t.email_hash == tenant.email_hash) {
            return Err(StorageError::Aborted(
                "UNIQUE constraint email_hash".to_string(),
            ));
        }
        g.idempotency.insert(
            idempotency_key.as_str().to_string(),
            (tenant.signup_id.clone(), tenant.tenant_id.clone()),
        );
        g.tenants.push(tenant);
        Ok(())
    }

    fn rollback(&self, _tx: SignupTx) -> Result<(), StorageError> {
        // In-memory store: pending rows live only on the tx guard; drop
        // discards them.  Production wiring issues `ROLLBACK`.
        Ok(())
    }

    fn lookup_idempotent(
        &self,
        idempotency_key: &crate::idempotency::IdempotencyKey,
    ) -> Result<Option<(SignupId, TenantId)>, StorageError> {
        let g = self.lock_state()?;
        Ok(g.idempotency.get(idempotency_key.as_str()).cloned())
    }

    fn committed_tenant_count(&self) -> Result<usize, StorageError> {
        let g = self.lock_state()?;
        Ok(g.tenants.len())
    }
}

/// Adversarial fixture — every `begin` call fails with
/// [`StorageError::TxFault`]. Used by the orchestrator tests to verify
/// the pre-tx fail-CLOSED arm.
#[derive(Clone, Debug, Default)]
pub struct FailingAtomicSignupStore;

impl AtomicSignupStore for FailingAtomicSignupStore {
    fn begin(&self) -> Result<SignupTx, StorageError> {
        Err(StorageError::TxFault(
            "adversarial fixture: begin always fails".to_string(),
        ))
    }

    fn insert_tenant(&self, _tx: &mut SignupTx, _row: TenantRow) -> Result<(), StorageError> {
        Err(StorageError::TxFault(
            "adversarial fixture: insert_tenant always fails".to_string(),
        ))
    }

    fn insert_dpa_pending(
        &self,
        _tx: &mut SignupTx,
        _row: DpaPendingRow,
    ) -> Result<(), StorageError> {
        Err(StorageError::TxFault(
            "adversarial fixture: insert_dpa_pending always fails".to_string(),
        ))
    }

    fn insert_first_pat(&self, _tx: &mut SignupTx, _row: PatRow) -> Result<(), StorageError> {
        Err(StorageError::TxFault(
            "adversarial fixture: insert_first_pat always fails".to_string(),
        ))
    }

    fn insert_usage_counter(
        &self,
        _tx: &mut SignupTx,
        _row: UsageCounterRow,
    ) -> Result<(), StorageError> {
        Err(StorageError::TxFault(
            "adversarial fixture: insert_usage_counter always fails".to_string(),
        ))
    }

    fn commit(
        &self,
        _tx: SignupTx,
        _idempotency_key: &crate::idempotency::IdempotencyKey,
    ) -> Result<(), StorageError> {
        Err(StorageError::TxFault(
            "adversarial fixture: commit always fails".to_string(),
        ))
    }

    fn rollback(&self, _tx: SignupTx) -> Result<(), StorageError> {
        Ok(())
    }

    fn lookup_idempotent(
        &self,
        _idempotency_key: &crate::idempotency::IdempotencyKey,
    ) -> Result<Option<(SignupId, TenantId)>, StorageError> {
        Ok(None)
    }

    fn committed_tenant_count(&self) -> Result<usize, StorageError> {
        Ok(0)
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
    use crate::idempotency::IdempotencyKey;

    fn tenant_row(id: &str, email: &str) -> TenantRow {
        TenantRow {
            tenant_id: TenantId::new(id),
            signup_id: SignupId::new(format!("s-{id}")),
            email_hash: UserEmailHash::new(email),
            primary_region: PrimaryRegion::Enam,
        }
    }

    fn pat_row(id: &str) -> PatRow {
        PatRow {
            tenant_id: TenantId::new(id),
            pat_hash: PatHash::new(format!("h-{id}")),
            shown_once_token: ShownOnceToken::new(format!("tok-{id}")),
        }
    }

    fn dpa_row(id: &str) -> DpaPendingRow {
        DpaPendingRow {
            tenant_id: TenantId::new(id),
        }
    }

    fn uc_row(id: &str) -> UsageCounterRow {
        UsageCounterRow {
            tenant_id: TenantId::new(id),
        }
    }

    #[test]
    fn happy_path_commits_all_four_rows() {
        let s = InMemoryAtomicSignupStore::new();
        let mut tx = s.begin().unwrap();
        s.insert_tenant(&mut tx, tenant_row("t-1", "a")).unwrap();
        s.insert_dpa_pending(&mut tx, dpa_row("t-1")).unwrap();
        s.insert_first_pat(&mut tx, pat_row("t-1")).unwrap();
        s.insert_usage_counter(&mut tx, uc_row("t-1")).unwrap();
        s.commit(tx, &IdempotencyKey::new("idem-1")).unwrap();
        assert_eq!(s.committed_tenant_count().unwrap(), 1);
        let cached = s.lookup_idempotent(&IdempotencyKey::new("idem-1")).unwrap();
        assert!(cached.is_some());
    }

    #[test]
    fn rollback_discards_pending_rows() {
        let s = InMemoryAtomicSignupStore::new();
        let mut tx = s.begin().unwrap();
        s.insert_tenant(&mut tx, tenant_row("t-1", "a")).unwrap();
        s.rollback(tx).unwrap();
        assert_eq!(s.committed_tenant_count().unwrap(), 0);
    }

    #[test]
    fn commit_missing_row_is_tx_fault() {
        let s = InMemoryAtomicSignupStore::new();
        let mut tx = s.begin().unwrap();
        s.insert_tenant(&mut tx, tenant_row("t-1", "a")).unwrap();
        // Missing dpa + first_pat + usage_counter.
        let err = s.commit(tx, &IdempotencyKey::new("idem-1")).unwrap_err();
        assert!(matches!(err, StorageError::TxFault(_)));
        assert_eq!(s.committed_tenant_count().unwrap(), 0);
    }

    #[test]
    fn unique_email_hash_rejected_on_commit() {
        let s = InMemoryAtomicSignupStore::new();
        // First commit OK.
        let mut tx = s.begin().unwrap();
        s.insert_tenant(&mut tx, tenant_row("t-1", "same")).unwrap();
        s.insert_dpa_pending(&mut tx, dpa_row("t-1")).unwrap();
        s.insert_first_pat(&mut tx, pat_row("t-1")).unwrap();
        s.insert_usage_counter(&mut tx, uc_row("t-1")).unwrap();
        s.commit(tx, &IdempotencyKey::new("idem-1")).unwrap();
        // Second commit with same email_hash MUST be rejected.
        let mut tx2 = s.begin().unwrap();
        s.insert_tenant(&mut tx2, tenant_row("t-2", "same"))
            .unwrap();
        s.insert_dpa_pending(&mut tx2, dpa_row("t-2")).unwrap();
        s.insert_first_pat(&mut tx2, pat_row("t-2")).unwrap();
        s.insert_usage_counter(&mut tx2, uc_row("t-2")).unwrap();
        let err = s.commit(tx2, &IdempotencyKey::new("idem-2")).unwrap_err();
        assert!(matches!(err, StorageError::Aborted(_)));
        assert_eq!(s.committed_tenant_count().unwrap(), 1);
    }

    #[test]
    fn inject_failure_at_insert_tenant() {
        let s = InMemoryAtomicSignupStore::new();
        s.inject_failure_at(crate::outcome::OrchestrationStep::InsertTenant);
        let mut tx = s.begin().unwrap();
        let err = s
            .insert_tenant(&mut tx, tenant_row("t-1", "a"))
            .unwrap_err();
        assert!(matches!(err, StorageError::Aborted(_)));
    }

    #[test]
    fn failing_store_fails_everything() {
        let s = FailingAtomicSignupStore;
        assert!(s.begin().is_err());
    }
}
