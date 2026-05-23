//! Host-side test fake for [`RevocationStore`] backed by a `BTreeMap`.
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1):
//! this file owns the `InMemoryRevocationStore` adapter.

use core::fmt;
use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::Mutex;

use corelink_pat::{PatId, TenantId as PatTenantId};

use crate::region::Region;

use super::traits::RevocationStore;
use super::types::{PropagationStatus, RevocationError, RevokedEntry};

// ---------------------------------------------------------------------------
// In-memory test fakes
// ---------------------------------------------------------------------------

/// Host-side test fake for [`RevocationStore`] backed by a
/// `BTreeMap`. Mirrors the in-memory R2 / KV pattern: same
/// documented semantics as the production CF Durable Object adapter.
///
/// Concurrency model: a single `Mutex` wrapping the whole map. The
/// production DO is single-writer-per-region by construction; the
/// fake faithfully reproduces that contract.
pub struct InMemoryRevocationStore {
    inner: Mutex<InMemoryRevocationState>,
}

#[derive(Default)]
struct InMemoryRevocationState {
    entries: HashMap<PatId, RevokedEntry>,
    propagation: HashMap<PatId, PropagationStatus>,
}

impl fmt::Debug for InMemoryRevocationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryRevocationStore").finish_non_exhaustive()
    }
}

impl Default for InMemoryRevocationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRevocationStore {
    /// Construct a fresh, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InMemoryRevocationState::default()),
        }
    }

    /// Test-only snapshot of the entry map size.
    pub async fn entry_count(&self) -> usize {
        self.inner.lock().await.entries.len()
    }
}

#[async_trait]
impl RevocationStore for InMemoryRevocationStore {
    async fn upsert(&self, entry: RevokedEntry) -> Result<bool, RevocationError> {
        let mut guard = self.inner.lock().await;
        if let Some(existing) = guard.entries.get(&entry.pat_id) {
            // Canonical dedup: matching `(pat_id, revoked_at)` is a
            // no-op — preserves INV-AUTH-REVOCATION-IDEMPOTENT.
            if existing.revoked_at == entry.revoked_at {
                return Ok(false);
            }
            // Distinct revoke timestamps for the same pat_id are a
            // bug at a higher layer (Neon SoT enforces a single
            // revoke); we keep the earliest timestamp (canonical
            // SoT wins) and surface as no-op.
            return Ok(false);
        }
        let mut status = PropagationStatus::default();
        status.regions_propagated.insert(entry.origin_region);
        guard.propagation.insert(entry.pat_id, status);
        guard.entries.insert(entry.pat_id, entry);
        Ok(true)
    }

    async fn get(&self, pat_id: PatId) -> Result<Option<RevokedEntry>, RevocationError> {
        let guard = self.inner.lock().await;
        Ok(guard.entries.get(&pat_id).cloned())
    }

    async fn record_propagation_ack(
        &self,
        pat_id: PatId,
        peer: Region,
    ) -> Result<(), RevocationError> {
        let mut guard = self.inner.lock().await;
        let status = guard
            .propagation
            .entry(pat_id)
            .or_insert_with(PropagationStatus::default);
        status.regions_propagated.insert(peer);
        // We don't compute `completed_at` here — that requires the
        // expected peer-set from the orchestrator config. The
        // orchestrator's reconciliation Cron stamps it.
        Ok(())
    }

    async fn propagation_status(
        &self,
        pat_id: PatId,
    ) -> Result<Option<PropagationStatus>, RevocationError> {
        let guard = self.inner.lock().await;
        Ok(guard.propagation.get(&pat_id).cloned())
    }

    async fn list(
        &self,
        tenant_filter: Option<PatTenantId>,
    ) -> Result<Vec<RevokedEntry>, RevocationError> {
        let guard = self.inner.lock().await;
        let entries: Vec<RevokedEntry> = guard
            .entries
            .values()
            .filter(|e| match tenant_filter {
                Some(t) => e.tenant_id == t,
                None => true,
            })
            .cloned()
            .collect();
        Ok(entries)
    }
}
