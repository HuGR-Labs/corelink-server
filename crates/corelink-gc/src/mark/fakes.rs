use super::*;

/// In-memory [`ReachableSetSource`] fake. Each tenant carries 3
/// independent buffers (mirrors per-tenant SQL filtering).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct InMemoryReachableSetSource {
    inner: Mutex<ReachableSetSourceInner>,
}

#[derive(Debug, Default)]
struct ReachableSetSourceInner {
    blob_meta: std::collections::BTreeMap<Uuid, Vec<BlobMetaRow>>,
    ac_meta: std::collections::BTreeMap<Uuid, Vec<AcMetaRow>>,
    manifest_chunks: std::collections::BTreeMap<Uuid, Vec<ManifestChunkRow>>,
}

impl InMemoryReachableSetSource {
    /// Construct an empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a `blob_meta` row for the given tenant. Tests build the
    /// corpus row-by-row.
    pub fn push_blob_meta(&self, tenant_id: Uuid, row: BlobMetaRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.blob_meta.entry(tenant_id).or_default().push(row);
    }

    /// Push an `ac_meta` row.
    pub fn push_ac_meta(&self, tenant_id: Uuid, row: AcMetaRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.ac_meta.entry(tenant_id).or_default().push(row);
    }

    /// Push a `manifest_chunks` row.
    pub fn push_manifest_chunk(&self, tenant_id: Uuid, row: ManifestChunkRow) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.manifest_chunks.entry(tenant_id).or_default().push(row);
    }

    fn slice<T: Clone>(rows: &[T], offset: u32, batch_size: u32) -> Vec<T> {
        let off = offset as usize;
        if off >= rows.len() {
            return Vec::new();
        }
        let end = off.saturating_add(batch_size as usize).min(rows.len());
        rows.get(off..end).map_or_else(Vec::new, <[T]>::to_vec)
    }
}

impl ReachableSetSource for InMemoryReachableSetSource {
    fn pass_blob_meta(
        &self,
        tenant_id: Uuid,
        _snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<BlobMetaRow>, MarkError> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let rows = g.blob_meta.get(&tenant_id).cloned().unwrap_or_default();
        Ok(Self::slice(&rows, offset, batch_size))
    }

    fn pass_ac_meta(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<AcMetaRow>, MarkError> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let rows = g
            .ac_meta
            .get(&tenant_id)
            .map(|r| {
                r.iter()
                    .filter(|row| row.created_at_ms >= snapshot_lower_bound_ms)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Self::slice(&rows, offset, batch_size))
    }

    fn pass_manifest_chunks(
        &self,
        tenant_id: Uuid,
        snapshot_lower_bound_ms: u64,
        batch_size: u32,
        offset: u32,
    ) -> Result<Vec<ManifestChunkRow>, MarkError> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let rows = g
            .manifest_chunks
            .get(&tenant_id)
            .map(|r| {
                r.iter()
                    .filter(|row| row.created_at_ms >= snapshot_lower_bound_ms)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Self::slice(&rows, offset, batch_size))
    }
}

/// In-memory [`GcCandidatesStore`] fake. Mirrors the composite PK
/// `(tenant_id, digest, mark_run_id)` semantics — duplicate INSERT is
/// a no-op.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct InMemoryGcCandidatesStore {
    inner: Mutex<std::collections::BTreeMap<(Uuid, BlobDigest, RunId), GcCandidate>>,
}

impl InMemoryGcCandidatesStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (diagnostic).
    #[must_use]
    pub fn snapshot(&self) -> Vec<GcCandidate> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.values().cloned().collect()
    }
}

impl GcCandidatesStore for InMemoryGcCandidatesStore {
    fn insert_candidate(&self, candidate: GcCandidate) -> Result<(), MarkError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        let key = (
            candidate.tenant_id,
            candidate.digest.clone(),
            candidate.mark_run_id,
        );
        // Idempotent: composite PK conflict is a no-op (mirrors `INSERT
        // ... ON CONFLICT DO NOTHING`).
        g.entry(key).or_insert(candidate);
        Ok(())
    }

    fn snapshot_for_run(
        &self,
        tenant_id: Uuid,
        mark_run_id: RunId,
    ) -> Result<Vec<GcCandidate>, MarkError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        Ok(g.values()
            .filter(|c| c.tenant_id == tenant_id && c.mark_run_id == mark_run_id)
            .cloned()
            .collect())
    }

    fn count_for_run(&self, tenant_id: Uuid, mark_run_id: RunId) -> Result<u64, MarkError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        Ok(g.values()
            .filter(|c| c.tenant_id == tenant_id && c.mark_run_id == mark_run_id)
            .count() as u64)
    }

    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_run_id: RunId,
    ) -> Result<Option<GcCandidate>, MarkError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        Ok(g.get(&(tenant_id, digest.clone(), mark_run_id)).cloned())
    }

    fn transition_status(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        mark_run_id: RunId,
        from_status: CandidateStatus,
        to_status: CandidateStatus,
        now_ms: u64,
        protected_reason: Option<String>,
    ) -> Result<bool, MarkError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| MarkError::Backend("gc_candidates store mutex poisoned".to_owned()))?;
        let Some(row) = g.get_mut(&(tenant_id, digest.clone(), mark_run_id)) else {
            return Ok(false);
        };
        // Defense-in-depth Layer 4 envelope (composite PK already binds
        // tenant_id, but a smuggled tenant_id should fail-closed).
        if row.tenant_id != tenant_id {
            return Err(MarkError::Backend(
                "cross_tenant_candidate: row tenant_id mismatch".to_owned(),
            ));
        }
        // Idempotent guard — only fire when current status matches.
        if row.status != from_status {
            return Ok(false);
        }
        row.status = to_status;
        match to_status {
            CandidateStatus::Swept => {
                row.swept_at_ms = Some(now_ms);
            }
            CandidateStatus::ProtectedReRef => {
                row.protected_at_ms = Some(now_ms);
                row.protected_reason = protected_reason;
            }
            CandidateStatus::PhysicallyDeleted | CandidateStatus::Candidate => {
                // PhysicallyDeleted is set by WI-S06-004; re-arming
                // Candidate is a programmer error but the trait surface
                // does not forbid it.
            }
        }
        Ok(true)
    }
}

/// Deterministic counter [`MarkClock`] for tests + property tests.
/// Each [`MarkClock::now_ms`] read advances the internal counter by 1
/// ms (mirrors per-batch wall-clock progression). Jitter accounting
/// adds the supplied ms verbatim.
#[derive(Debug)]
#[non_exhaustive]
pub struct CountingMarkClock {
    inner: Mutex<u64>,
}

impl CountingMarkClock {
    /// Construct with the given starting wall-clock instant.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            inner: Mutex::new(start_ms),
        }
    }
}

impl MarkClock for CountingMarkClock {
    fn now_ms(&self) -> u64 {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = *g;
        *g = g.saturating_add(1);
        now
    }

    fn advance_jitter(&self, ms: u64) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = g.saturating_add(ms);
    }
}
