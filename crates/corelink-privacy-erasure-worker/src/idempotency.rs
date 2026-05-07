//! Idempotency ledger trait + in-memory implementations + the
//! always-failing fixture for adversarial tests of the
//! `Idempotency` failure-mode lifting.
//!
//! The canonical D1 surface is `migrations/d1/0022_dsr_erasure_log.sql`
//! per WI-S11-002 §6.1.7. The trait shape mirrors the D1 contract:
//! `INSERT OR IGNORE INTO dsr_erasure_log (log_id, dsr_id, tenant_id,
//! subject_id_hash, backend, outcome, ...)` with UNIQUE
//! `(dsr_id, backend)` constraint enforcing replay-safe per
//! PAT-RETRY-IDEMPOTENT-001 (sprint contract §9 14.s11.4).
//!
//! ## Why a typed `LedgerOutcome` (NOT a `bool`)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 + WI-S11-002 §1: the canonical
//! re-run signal is a 3-arm enum (Inserted / Replayed / Divergent) so
//! the orchestrator can distinguish a true idempotent re-run (same
//! payload byte-equal) from a forensic anomaly (same key, divergent
//! payload — SEV-1 tampering signal). A `bool` collapses these into
//! a silent loss.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use uuid::Uuid;

use crate::error::ErasureIdempotencyError;
use crate::event::{BackendCompletion, BackendKind};

/// Canonical 3-arm UPSERT outcome from a successful idempotency
/// ledger insert. `#[non_exhaustive]` so additive growth lands without
/// breaking downstream `match` sites.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LedgerOutcome {
    /// New tombstone row inserted; this is the first time the
    /// orchestrator has processed `(dsr_id, backend)`.
    Inserted,
    /// A prior tombstone row exists for `(dsr_id, backend)` and the
    /// canonical typed payload byte-equals the new payload — a true
    /// idempotent replay (PAT-RETRY-IDEMPOTENT-001). The orchestrator
    /// short-circuits without re-firing the per-backend mutation.
    Replayed {
        /// The prior tombstone payload (returned for caller logging
        /// + dashboard widget grouping).
        prior: BackendCompletion,
    },
}

/// Idempotency ledger trait. Production wiring at WI-S11-008 binds
/// this to the canonical D1 `dsr_erasure_log` table per WI-S11-002
/// §6.1.7 + UNIQUE `(dsr_id, backend)` constraint.
pub trait ErasureIdempotencyLedger: Send + Sync + core::fmt::Debug {
    /// Persist `completion` durably. Returns:
    ///
    /// - [`LedgerOutcome::Inserted`] if the canonical
    ///   `(dsr_id, backend)` slot was empty pre-call.
    /// - [`LedgerOutcome::Replayed`] if the canonical
    ///   `(dsr_id, backend)` slot already holds a row with the same
    ///   `outcome` + `idempotency_key` + `tenant_id` + `subject_id_hash`
    ///   etc. (true idempotent replay).
    ///
    /// # Errors
    ///
    /// - [`ErasureIdempotencyError::Backend`] for transport failures.
    /// - [`ErasureIdempotencyError::DivergentPayload`] when the
    ///   canonical `(dsr_id, backend)` slot holds a row with a
    ///   divergent canonical payload (forensic tampering signal).
    fn upsert(
        &self,
        completion: BackendCompletion,
    ) -> Result<LedgerOutcome, ErasureIdempotencyError>;

    /// Look up the canonical tombstone for `(dsr_id, backend)`.
    /// Returns `None` if the slot is empty.
    ///
    /// # Errors
    ///
    /// - [`ErasureIdempotencyError::Backend`] for transport failures.
    fn get(
        &self,
        dsr_id: Uuid,
        backend: BackendKind,
    ) -> Result<Option<BackendCompletion>, ErasureIdempotencyError>;

    /// Snapshot every tombstone for `dsr_id` (canonical 12-arm
    /// completion ledger; ordered to match
    /// [`crate::event::canonical_backend_kinds`]).
    ///
    /// # Errors
    ///
    /// - [`ErasureIdempotencyError::Backend`] for transport failures.
    fn snapshot(
        &self,
        dsr_id: Uuid,
    ) -> Result<Vec<BackendCompletion>, ErasureIdempotencyError>;
}

/// In-memory ledger. Cloning shares the underlying buffer so
/// orchestrator + verifier can hold separate handles.
#[derive(Clone, Default, Debug)]
pub struct InMemoryErasureIdempotencyLedger {
    inner: Arc<Mutex<HashMap<(Uuid, BackendKind), BackendCompletion>>>,
}

impl InMemoryErasureIdempotencyLedger {
    /// Construct a fresh ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ErasureIdempotencyLedger for InMemoryErasureIdempotencyLedger {
    fn upsert(
        &self,
        completion: BackendCompletion,
    ) -> Result<LedgerOutcome, ErasureIdempotencyError> {
        let key = (completion.dsr_id, completion.backend);
        let mut guard = self.inner.lock().map_err(|_| {
            ErasureIdempotencyError::Backend("ledger mutex poisoned".to_string())
        })?;
        match guard.get(&key) {
            Some(prior) => {
                // Compare on canonical typed payload (excludes the
                // mutable timestamps which would diverge under retry).
                if prior.outcome == completion.outcome
                    && prior.tenant_id == completion.tenant_id
                    && prior.idempotency_key == completion.idempotency_key
                {
                    Ok(LedgerOutcome::Replayed {
                        prior: prior.clone(),
                    })
                } else {
                    Err(ErasureIdempotencyError::DivergentPayload)
                }
            }
            None => {
                guard.insert(key, completion);
                Ok(LedgerOutcome::Inserted)
            }
        }
    }

    fn get(
        &self,
        dsr_id: Uuid,
        backend: BackendKind,
    ) -> Result<Option<BackendCompletion>, ErasureIdempotencyError> {
        let guard = self.inner.lock().map_err(|_| {
            ErasureIdempotencyError::Backend("ledger mutex poisoned".to_string())
        })?;
        Ok(guard.get(&(dsr_id, backend)).cloned())
    }

    fn snapshot(
        &self,
        dsr_id: Uuid,
    ) -> Result<Vec<BackendCompletion>, ErasureIdempotencyError> {
        let guard = self.inner.lock().map_err(|_| {
            ErasureIdempotencyError::Backend("ledger mutex poisoned".to_string())
        })?;
        let mut out: Vec<BackendCompletion> = crate::event::canonical_backend_kinds()
            .iter()
            .filter_map(|k| guard.get(&(dsr_id, *k)).cloned())
            .collect();
        out.sort_by(|a, b| {
            a.backend
                .as_str()
                .cmp(b.backend.as_str())
                .then_with(|| a.dsr_id.cmp(&b.dsr_id))
        });
        // Re-sort by canonical_backend_kinds ordering (lookup by
        // index of backend kind in the canonical array).
        out.sort_by_key(|c| {
            crate::event::canonical_backend_kinds()
                .iter()
                .position(|k| *k == c.backend)
                .unwrap_or(usize::MAX)
        });
        Ok(out)
    }
}

/// Always-failing ledger for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingErasureIdempotencyLedger;

impl FailingErasureIdempotencyLedger {
    /// Construct a fresh always-failing ledger.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ErasureIdempotencyLedger for FailingErasureIdempotencyLedger {
    fn upsert(
        &self,
        _completion: BackendCompletion,
    ) -> Result<LedgerOutcome, ErasureIdempotencyError> {
        Err(ErasureIdempotencyError::Backend(
            "induced ledger failure (test fixture)".to_string(),
        ))
    }

    fn get(
        &self,
        _dsr_id: Uuid,
        _backend: BackendKind,
    ) -> Result<Option<BackendCompletion>, ErasureIdempotencyError> {
        Err(ErasureIdempotencyError::Backend(
            "induced ledger failure (test fixture)".to_string(),
        ))
    }

    fn snapshot(
        &self,
        _dsr_id: Uuid,
    ) -> Result<Vec<BackendCompletion>, ErasureIdempotencyError> {
        Err(ErasureIdempotencyError::Backend(
            "induced ledger failure (test fixture)".to_string(),
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
    use crate::event::BackendErasureOutcome;

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    fn completion(dsr: Uuid, backend: BackendKind) -> BackendCompletion {
        BackendCompletion {
            dsr_id: dsr,
            tenant_id: fixed_uuid(99),
            backend,
            outcome: BackendErasureOutcome::Erased {
                records_deleted: 1,
            },
            idempotency_key: format!(
                "corelink-{}-{}-000",
                "abcdef01",
                backend.as_str()
            ),
            started_at_ms: 1_000,
            completed_at_ms: 2_000,
            retry_count: 0,
            verification_hash: [0u8; 32],
        }
    }

    #[test]
    fn first_upsert_inserts() {
        let l = InMemoryErasureIdempotencyLedger::new();
        let c = completion(fixed_uuid(1), BackendKind::D1);
        let r = l.upsert(c).unwrap();
        let inserted = matches!(r, LedgerOutcome::Inserted);
        assert!(inserted);
    }

    #[test]
    fn second_upsert_same_payload_replays() {
        let l = InMemoryErasureIdempotencyLedger::new();
        let dsr = fixed_uuid(1);
        let c1 = completion(dsr, BackendKind::D1);
        let c2 = completion(dsr, BackendKind::D1);
        l.upsert(c1).unwrap();
        let r = l.upsert(c2).unwrap();
        let replayed = matches!(r, LedgerOutcome::Replayed { .. });
        assert!(replayed);
    }

    #[test]
    fn divergent_payload_returns_error() {
        let l = InMemoryErasureIdempotencyLedger::new();
        let dsr = fixed_uuid(1);
        let mut c1 = completion(dsr, BackendKind::D1);
        let mut c2 = completion(dsr, BackendKind::D1);
        c1.outcome = BackendErasureOutcome::Erased {
            records_deleted: 1,
        };
        c2.outcome = BackendErasureOutcome::Erased {
            records_deleted: 99,
        };
        l.upsert(c1).unwrap();
        let err = l.upsert(c2).unwrap_err();
        let divergent = matches!(err, ErasureIdempotencyError::DivergentPayload);
        assert!(divergent);
    }

    #[test]
    fn get_missing_returns_none() {
        let l = InMemoryErasureIdempotencyLedger::new();
        let r = l.get(fixed_uuid(1), BackendKind::D1).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn snapshot_returns_canonical_order() {
        let l = InMemoryErasureIdempotencyLedger::new();
        let dsr = fixed_uuid(1);
        l.upsert(completion(dsr, BackendKind::Stripe)).unwrap();
        l.upsert(completion(dsr, BackendKind::NeonMain)).unwrap();
        l.upsert(completion(dsr, BackendKind::R2EvidencePseudo)).unwrap();
        let snap = l.snapshot(dsr).unwrap();
        assert_eq!(snap.len(), 3);
        // canonical ordering: NeonMain (idx 0), Stripe (idx 6),
        // R2EvidencePseudo (idx 11).
        assert_eq!(snap.first().unwrap().backend, BackendKind::NeonMain);
        assert_eq!(snap.get(1).unwrap().backend, BackendKind::Stripe);
        assert_eq!(snap.get(2).unwrap().backend, BackendKind::R2EvidencePseudo);
    }

    #[test]
    fn failing_ledger_propagates() {
        let l = FailingErasureIdempotencyLedger::new();
        let err = l.upsert(completion(fixed_uuid(1), BackendKind::D1)).unwrap_err();
        let backend = matches!(err, ErasureIdempotencyError::Backend(_));
        assert!(backend);
    }
}
