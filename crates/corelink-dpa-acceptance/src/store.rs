//! Backing store for accepted DPAs. Production wires this to D1
//! (`migrations/d1/0038_dpa_acceptances.sql`); the in-memory fake is
//! sufficient for unit + property + integration tests.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::DpaAcceptanceError;
use crate::schema::{ConsentProofPayload, SignupId, TenantId};

/// One row of `dpa_acceptances` (D1 mirror).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DpaAcceptanceRecord {
    /// Idempotency key (PRIMARY KEY in D1).
    pub signup_id: SignupId,
    /// Owning tenant.
    pub tenant_id: TenantId,
    /// 6-field consent proof payload (server-stamped `submission_ts`).
    pub proof: ConsentProofPayload,
    /// Hex64 SHA-256 over `ip || salt` (CTRL-PRIV-001 — never the raw
    /// IP).
    pub accepted_ip_hash: String,
    /// JWT receipt `jti` for verifiability cross-reference.
    pub jwt_receipt_jti: String,
    /// Acceptance time (ms since epoch).
    pub accepted_at_ms: i64,
}

/// Persistence surface for accepted DPAs. Production wires to D1; the
/// trait stays minimal so future stores can be swapped in.
pub trait DpaAcceptanceStore: Send + Sync + std::fmt::Debug {
    /// Insert a fresh record. Returns
    /// [`DpaAcceptanceError::IdempotencyConflict`] if `signup_id`
    /// already exists with a **diverging** payload; returns
    /// `Ok(existing)` if the existing record matches the supplied one
    /// (PAT-RETRY-IDEMPOTENT-001).
    ///
    /// # Errors
    ///
    /// Returns [`DpaAcceptanceError::IdempotencyConflict`] on conflict
    /// or [`DpaAcceptanceError::Store`] on backend failure.
    fn insert_idempotent(
        &self,
        record: DpaAcceptanceRecord,
    ) -> Result<DpaAcceptanceRecord, DpaAcceptanceError>;

    /// Lookup an existing record by `signup_id`.
    ///
    /// # Errors
    ///
    /// Returns [`DpaAcceptanceError::Store`] on backend failure.
    fn lookup(&self, signup_id: &SignupId)
        -> Result<Option<DpaAcceptanceRecord>, DpaAcceptanceError>;
}

/// In-memory implementation for unit + integration tests.
#[derive(Debug, Default)]
pub struct InMemoryDpaAcceptanceStore {
    rows: Mutex<HashMap<SignupId, DpaAcceptanceRecord>>,
}

impl InMemoryDpaAcceptanceStore {
    /// Build an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: Mutex::new(HashMap::new()),
        }
    }

    /// Number of rows stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.lock().map(|g| g.len()).unwrap_or_default()
    }

    /// `true` if the store has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl DpaAcceptanceStore for InMemoryDpaAcceptanceStore {
    fn insert_idempotent(
        &self,
        record: DpaAcceptanceRecord,
    ) -> Result<DpaAcceptanceRecord, DpaAcceptanceError> {
        let mut guard = self
            .rows
            .lock()
            .map_err(|e| DpaAcceptanceError::Store(format!("poisoned: {}", e)))?;
        if let Some(existing) = guard.get(&record.signup_id) {
            // Payload + receipt-binding equality defines idempotency.
            // IP-hash drift between honest retries (different egress
            // path) is tolerated; the audit trail records the first
            // accepted IP only.
            if existing.proof == record.proof
                && existing.tenant_id == record.tenant_id
                && existing.jwt_receipt_jti == record.jwt_receipt_jti
            {
                return Ok(existing.clone());
            }
            return Err(DpaAcceptanceError::IdempotencyConflict);
        }
        guard.insert(record.signup_id.clone(), record.clone());
        Ok(record)
    }

    fn lookup(
        &self,
        signup_id: &SignupId,
    ) -> Result<Option<DpaAcceptanceRecord>, DpaAcceptanceError> {
        let guard = self
            .rows
            .lock()
            .map_err(|e| DpaAcceptanceError::Store(format!("poisoned: {}", e)))?;
        Ok(guard.get(signup_id).cloned())
    }
}
