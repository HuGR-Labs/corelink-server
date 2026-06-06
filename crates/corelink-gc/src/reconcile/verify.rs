//! In-memory `RefcountSource` + `BlobMetaRefcountStore` fakes used by
//! property tests + unit tests to verify the reconcile pipeline's
//! invariants (`json_each`-equivalent membership; dual-condition auto-fix
//! gate; anti-ping-pong conditional UPDATE) without spinning up
//! miniflare. Extracted from the monolithic `reconcile.rs` per Wave 33
//! Stream A2.2 file-size discipline.

use std::sync::Mutex;

use uuid::Uuid;

use crate::mark::BlobDigest;

use super::{
    AcMetaReconcileRow, BlobMetaReconcileRow, BlobMetaRefcountStore, ReconcileError, RefcountSource,
};

/// In-memory `ac_meta` reference-source fake. Tests push rows per
/// tenant; the [`RefcountSource::expected_refcount`] scan filters by
/// tenant + `deleted_at_ms IS NULL` + `created_at_ms < snapshot` +
/// blob_refs *exact-membership* (mirrors `json_each` semantics).
#[derive(Debug, Default)]
pub struct InMemoryRefcountSource {
    inner: Mutex<std::collections::BTreeMap<Uuid, Vec<AcMetaReconcileRow>>>,
}

impl InMemoryRefcountSource {
    /// Construct an empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an `ac_meta` row tied to a tenant. The `blob_refs` vector
    /// drives the `json_each` membership semantic — the fake counts
    /// each digest occurrence in the vector exactly (mirrors
    /// `j.value = digest` SQL semantics; substring matches are
    /// structurally impossible).
    pub fn push_ac_row(&self, tenant_id: Uuid, row: AcMetaReconcileRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.entry(tenant_id).or_default().push(row);
    }

    /// Soft-delete an `ac_meta` row by `action_digest` (test mutator
    /// for the `deleted_at_ms IS NULL` filter property).
    pub fn soft_delete(&self, tenant_id: Uuid, action_digest: &str, deleted_at_ms: u64) -> bool {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let Some(rows) = g.get_mut(&tenant_id) else {
            return false;
        };
        let mut fired = false;
        for row in rows.iter_mut() {
            if row.action_digest == action_digest && row.deleted_at_ms.is_none() {
                row.deleted_at_ms = Some(deleted_at_ms);
                fired = true;
            }
        }
        fired
    }
}

impl RefcountSource for InMemoryRefcountSource {
    fn expected_refcount(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        snapshot_at_ms: u64,
    ) -> Result<u32, ReconcileError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| ReconcileError::Backend("refcount source mutex poisoned".to_owned()))?;
        let Some(rows) = g.get(&tenant_id) else {
            return Ok(0);
        };
        let mut count: u32 = 0;
        for row in rows {
            if row.deleted_at_ms.is_some() {
                continue;
            }
            if row.created_at_ms >= snapshot_at_ms {
                continue;
            }
            for r in &row.blob_refs {
                if r == digest {
                    count = count.saturating_add(1);
                }
            }
        }
        Ok(count)
    }
}

/// In-memory `blob_meta` refcount store. Mirrors the canonical
/// `(tenant_id, digest)` PK + the conditional UPDATE predicate
/// described in [`BlobMetaRefcountStore::conditional_set_refcount`].
#[derive(Debug, Default)]
pub struct InMemoryBlobMetaRefcountStore {
    inner: Mutex<std::collections::BTreeMap<(Uuid, BlobDigest), BlobMetaReconcileRow>>,
}

impl InMemoryBlobMetaRefcountStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a fresh `blob_meta` row (test wiring).
    pub fn push_row(&self, tenant_id: Uuid, row: BlobMetaReconcileRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.insert((tenant_id, row.digest.clone()), row);
    }

    /// Snapshot a single row (diagnostic).
    #[must_use]
    pub fn snapshot(&self, tenant_id: Uuid, digest: &BlobDigest) -> Option<BlobMetaReconcileRow> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(&(tenant_id, digest.clone())).cloned()
    }

    /// Test mutator: simulate a customer UpdateAR that incremented the
    /// stored refcount mid-reconcile. The conditional UPDATE in
    /// [`BlobMetaRefcountStore::conditional_set_refcount`] will then
    /// skip the row (anti-ping-pong predicate fails).
    pub fn set_stored_refcount(&self, tenant_id: Uuid, digest: &BlobDigest, refcount: u32) -> bool {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match g.get_mut(&(tenant_id, digest.clone())) {
            Some(row) => {
                row.stored_refcount = refcount;
                true
            }
            None => false,
        }
    }
}

impl BlobMetaRefcountStore for InMemoryBlobMetaRefcountStore {
    fn snapshot_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<BlobMetaReconcileRow>, ReconcileError> {
        let g = self.inner.lock().map_err(|_| {
            ReconcileError::Backend("blob_meta reconcile mutex poisoned".to_owned())
        })?;
        Ok(g.iter()
            .filter_map(|((t, _), row)| {
                if *t == tenant_id {
                    Some(row.clone())
                } else {
                    None
                }
            })
            .collect())
    }

    fn conditional_set_refcount(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        stored_refcount: u32,
        new_refcount: u32,
    ) -> Result<bool, ReconcileError> {
        let mut g = self.inner.lock().map_err(|_| {
            ReconcileError::Backend("blob_meta reconcile mutex poisoned".to_owned())
        })?;
        let key = (tenant_id, digest.clone());
        let Some(row) = g.get_mut(&key) else {
            return Ok(false);
        };
        if row.deleted_at_ms.is_some() {
            return Ok(false);
        }
        if row.stored_refcount != stored_refcount {
            return Ok(false);
        }
        row.stored_refcount = new_refcount;
        Ok(true)
    }
}
