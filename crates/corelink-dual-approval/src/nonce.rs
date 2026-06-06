//! Nonce replay protection for dual-approval requests (WI-S13-002).
//!
//! Production: D1 `UNIQUE (caller_user_id, nonce)` constraint + insert.
//! Here: in-memory hash set per caller (for CI).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::error::DualApprovalError;

/// In-memory nonce store (production: D1 UNIQUE (caller_user_id, nonce)).
///
/// Thread-safe via `Arc<Mutex<>>` (F-001).
#[derive(Debug, Clone)]
pub struct InMemoryNonceStore {
    inner: Arc<Mutex<HashMap<Uuid, HashSet<[u8; 16]>>>>,
}

impl InMemoryNonceStore {
    /// Construct a fresh empty store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check that this (caller, nonce) pair has not been seen before,
    /// then record it.
    ///
    /// Returns `Ok(())` on first occurrence; `Err(NonceReplay)` on repeat.
    pub fn check_and_record(
        &self,
        caller_user_id: Uuid,
        nonce: [u8; 16],
        now_ms: u64,
    ) -> Result<(), DualApprovalError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| DualApprovalError::Internal(e.to_string()))?;
        let seen = guard.entry(caller_user_id).or_insert_with(HashSet::new);
        if seen.contains(&nonce) {
            return Err(DualApprovalError::NonceReplay { seen_at_ms: now_ms });
        }
        seen.insert(nonce);
        Ok(())
    }
}

impl Default for InMemoryNonceStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn fresh_nonce_accepted() {
        let store = InMemoryNonceStore::new();
        let caller = Uuid::now_v7();
        store.check_and_record(caller, [1u8; 16], 1_000).unwrap();
    }

    #[test]
    fn replay_nonce_rejected() {
        let store = InMemoryNonceStore::new();
        let caller = Uuid::now_v7();
        store.check_and_record(caller, [1u8; 16], 1_000).unwrap();
        let err = store
            .check_and_record(caller, [1u8; 16], 2_000)
            .unwrap_err();
        assert!(matches!(err, DualApprovalError::NonceReplay { .. }));
    }

    #[test]
    fn different_caller_same_nonce_ok() {
        let store = InMemoryNonceStore::new();
        let caller_a = Uuid::now_v7();
        let caller_b = Uuid::now_v7();
        store.check_and_record(caller_a, [1u8; 16], 1_000).unwrap();
        store.check_and_record(caller_b, [1u8; 16], 2_000).unwrap();
    }
}
