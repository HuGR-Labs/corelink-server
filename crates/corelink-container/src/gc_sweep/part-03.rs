/// D1-backed soft-delete lookup and conditional purge.
#[non_exhaustive]
pub struct D1BlobMetaPurgeStore {
    d1: Arc<D1HttpClient>,
    region: GcRegion,
    tdk: [u8; 32],
}
impl core::fmt::Debug for D1BlobMetaPurgeStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1BlobMetaPurgeStore")
            .field("d1", &self.d1)
            .field("region", &self.region)
            .field("tdk", &"[REDACTED]")
            .finish()
    }
}
impl D1BlobMetaPurgeStore {
    /// Construct a tenant-key-aware purge adapter.
    pub fn new(d1: Arc<D1HttpClient>, region: GcRegion, tdk: [u8; 32]) -> Self {
        Self { d1, region, tdk }
    }
}
impl BlobMetaPurgeStore for D1BlobMetaPurgeStore {
    fn lookup_purge_state(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
    ) -> Result<Option<PurgeState>, PhysicalDeleteError> {
        // `blob_meta.region` is the macro-region vocabulary while `self.region`
        // is the physical GC colo.  Filter with the explicit mapping (not a
        // literal equality), then validate the returned pair again below.
        let rows = query_sync(
            &self.d1,
            "SELECT bm.refcount, bm.size_bytes, bm.deleted_at, \
             bm.region AS blob_region, t.primary_region AS tenant_region \
             FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id = bm.tenant_id \
             WHERE bm.tenant_id = ?1 AND bm.digest = ?2 \
             AND bm.deleted_at IS NOT NULL AND bm.region = t.primary_region \
             AND ((?3 = 'iad' AND bm.region IN ('wnam', 'enam')) \
                  OR (?3 = 'lhr' AND bm.region = 'weur') \
                  OR (?3 = 'nrt' AND bm.region = 'apac') \
                  OR (?3 = 'sam' AND bm.region = 'sam'))",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(self.region.as_str()),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        rows.first()
            .map(|row| {
                let blob_region = text(row, "blob_region")?;
                let tenant_region = text(row, "tenant_region")?;
                validate_gc_residency(self.region, &blob_region, &tenant_region)?;
                Ok(PurgeState {
                    r2_key: canonical_blob_key(self.region, &self.tdk, tenant_id, digest)?,
                    refcount: number(row, "refcount")?
                        .try_into()
                        .map_err(|_| "refcount exceeds u32".to_owned())?,
                    size_bytes: number(row, "size_bytes")?,
                    deleted_at_ms: number(row, "deleted_at")?,
                })
            })
            .transpose()
            .map_err(PhysicalDeleteError::Backend)
    }
    fn conditional_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<bool, PhysicalDeleteError> {
        let allowed_macro_regions = match self.region {
            GcRegion::Iad => "'wnam', 'enam'",
            GcRegion::Lhr => "'weur'",
            GcRegion::Nrt => "'apac'",
            GcRegion::Sam => "'sam'",
            GcRegion::Syd => {
                return Err(PhysicalDeleteError::Backend(
                    "no residency mapping exists for GC region syd".to_owned(),
                ));
            }
        };
        let rows = query_sync(
            &self.d1,
            &format!(
                "DELETE FROM blob_meta WHERE tenant_id = ?1 AND digest = ?2 \
                 AND refcount = 0 AND deleted_at IS NOT NULL \
                 AND region = (SELECT primary_region FROM tenant WHERE tenant_id = ?1) \
                 AND region IN ({allowed_macro_regions}) \
                 AND (?3 - deleted_at) > ?4 \
                 AND EXISTS (SELECT 1 FROM gc_purge_intent AS pi \
                            WHERE pi.tenant_id = blob_meta.tenant_id AND pi.digest = blob_meta.digest \
                              AND pi.state = 'r2_deleted') RETURNING 1 AS purged"
            ),
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(now_ms),
                json!(grace_period_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        Ok(!rows.is_empty())
    }

    fn begin_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        run_id: RunId,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<Option<PurgeStage>, PhysicalDeleteError> {
        let allowed_macro_regions = match self.region {
            GcRegion::Iad => "'wnam', 'enam'",
            GcRegion::Lhr => "'weur'",
            GcRegion::Nrt => "'apac'",
            GcRegion::Sam => "'sam'",
            GcRegion::Syd => {
                return Err(PhysicalDeleteError::Backend(
                    "no residency mapping exists for GC region syd".to_owned(),
                ));
            }
        };
        let r2_key = canonical_blob_key(self.region, &self.tdk, tenant_id, digest)
            .map_err(PhysicalDeleteError::Backend)?;
        let inserted = query_sync(
            &self.d1,
            &format!(
                "INSERT INTO gc_purge_intent \
                 (tenant_id, digest, epoch, state, r2_key, gc_region, residency_region, mark_run_id, started_at, updated_at) \
                 SELECT ?1, ?2, COALESCE((SELECT MAX(epoch) FROM gc_purge_intent WHERE tenant_id = ?1 AND digest = ?2), 0) + 1, \
                        'purging', ?3, ?6, t.primary_region, ?5, ?4, ?4 \
                 FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id = bm.tenant_id \
                 WHERE bm.tenant_id = ?1 AND bm.digest = ?2 AND bm.refcount = 0 \
                   AND bm.deleted_at IS NOT NULL AND (?4 - bm.deleted_at) > ?7 \
                   AND bm.region = t.primary_region AND bm.region IN ({allowed_macro_regions}) \
                   AND NOT EXISTS (SELECT 1 FROM gc_purge_intent AS pi \
                                  WHERE pi.tenant_id = bm.tenant_id AND pi.digest = bm.digest \
                                    AND pi.state IN ('purging', 'r2_deleted', 'retry')) \
                   AND NOT EXISTS (SELECT 1 FROM cas_write_intent AS wi \
                                  WHERE wi.tenant_id = bm.tenant_id AND wi.digest = bm.digest \
                                    AND wi.state = 'writing' \
                                    AND (?4 - wi.updated_at) <= {CAS_WRITE_LEASE_MS}) \
                 ON CONFLICT (tenant_id, digest) DO NOTHING \
                 RETURNING epoch, state"
            ),
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(r2_key),
                json!(now_ms),
                json!(run_id.as_text()),
                json!(self.region.as_str()),
                json!(grace_period_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        if let Some(row) = inserted.first() {
            let epoch = number(row, "epoch").map_err(PhysicalDeleteError::Backend)?;
            return Ok(Some(PurgeStage::Acquired { epoch }));
        }
        let existing = query_sync(
            &self.d1,
            "SELECT epoch, state, updated_at FROM gc_purge_intent WHERE tenant_id = ?1 AND digest = ?2",
            &[json!(tenant_id.to_string()), json!(digest.to_string())],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        let Some(row) = existing.first() else {
            return Ok(None);
        };
        let epoch = number(row, "epoch").map_err(PhysicalDeleteError::Backend)?;
        match text(row, "state")
            .map_err(PhysicalDeleteError::Backend)?
            .as_str()
        {
            // A live `purging` row belongs to another worker.  Do not issue
            // a second remote delete concurrently.  A crashed owner is
            // reclaimed after the bounded lease and then receives a new
            // epoch, fencing the stale worker's later updates.
            "purging" => {
                const PURGE_LEASE_MS: u64 = CAS_WRITE_LEASE_MS;
                let updated_at = number(row, "updated_at").map_err(PhysicalDeleteError::Backend)?;
                if now_ms.saturating_sub(updated_at) <= PURGE_LEASE_MS {
                    return Ok(None);
                }
                let expired = query_sync(
                    &self.d1,
                    "UPDATE gc_purge_intent SET state = 'retry', updated_at = ?4, last_error = 'owner lease expired' \
                     WHERE tenant_id = ?1 AND digest = ?2 AND epoch = ?3 AND state = 'purging' \
                       AND (?4 - updated_at) > ?5 RETURNING epoch",
                    &[
                        json!(tenant_id.to_string()),
                        json!(digest.to_string()),
                        json!(epoch),
                        json!(now_ms),
                        json!(PURGE_LEASE_MS),
                    ],
                )
                .map_err(PhysicalDeleteError::Backend)?;
                if expired.is_empty() {
                    return Ok(None);
                }
                self.claim_retry_epoch(tenant_id, digest, now_ms)
            }
            "retry" => self.claim_retry_epoch(tenant_id, digest, now_ms),
            "r2_deleted" => Ok(Some(PurgeStage::R2Deleted { epoch })),
            "finalized" => Ok(None),
            state => Err(PhysicalDeleteError::Backend(format!(
                "unknown gc purge intent state {state:?}"
            ))),
        }
    }

    /// Atomically claim a retry and advance the epoch.  The UPDATE is the
    /// ownership boundary: exactly one concurrent worker can receive the new
    /// `purging` epoch and proceed to R2.
    fn claim_retry_epoch(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        now_ms: u64,
    ) -> Result<Option<PurgeStage>, PhysicalDeleteError> {
        let rows = query_sync(
            &self.d1,
            "UPDATE gc_purge_intent SET state = 'purging', epoch = epoch + 1, updated_at = ?3, last_error = NULL \
             WHERE tenant_id = ?1 AND digest = ?2 AND state = 'retry' RETURNING epoch",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(now_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        rows.first()
            .map(|row| number(row, "epoch").map(|epoch| PurgeStage::Acquired { epoch }))
            .transpose()
            .map_err(PhysicalDeleteError::Backend)
    }

    fn mark_r2_deleted(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        epoch: u64,
        now_ms: u64,
    ) -> Result<(), PhysicalDeleteError> {
        let rows = query_sync(
            &self.d1,
            "UPDATE gc_purge_intent SET state = 'r2_deleted', updated_at = ?4, last_error = NULL \
             WHERE tenant_id = ?1 AND digest = ?2 AND epoch = ?3 AND state IN ('purging', 'retry') RETURNING epoch",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(epoch),
                json!(now_ms),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        if rows.is_empty() {
            return Err(PhysicalDeleteError::Backend(
                "purge epoch lost before R2 completion".to_owned(),
            ));
        }
        Ok(())
    }

    fn mark_r2_retry(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        epoch: u64,
        now_ms: u64,
        error: &str,
    ) -> Result<(), PhysicalDeleteError> {
        query_sync(
            &self.d1,
            "UPDATE gc_purge_intent SET state = 'retry', updated_at = ?4, last_error = ?5 \
             WHERE tenant_id = ?1 AND digest = ?2 AND epoch = ?3 AND state IN ('purging', 'retry')",
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(epoch),
                json!(now_ms),
                json!(error),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)
        .map(|_| ())
    }

    fn finalize_purge(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        epoch: u64,
        now_ms: u64,
        grace_period_ms: u64,
    ) -> Result<bool, PhysicalDeleteError> {
        let allowed_macro_regions = match self.region {
            GcRegion::Iad => "'wnam', 'enam'",
            GcRegion::Lhr => "'weur'",
            GcRegion::Nrt => "'apac'",
            GcRegion::Sam => "'sam'",
            GcRegion::Syd => {
                return Err(PhysicalDeleteError::Backend(
                    "no residency mapping exists for GC region syd".to_owned(),
                ));
            }
        };
        let rows = query_sync(
            &self.d1,
            &format!(
                "DELETE FROM blob_meta WHERE tenant_id = ?1 AND digest = ?2 \
                 AND refcount = 0 AND deleted_at IS NOT NULL \
                 AND region = (SELECT primary_region FROM tenant WHERE tenant_id = ?1) \
                 AND region IN ({allowed_macro_regions}) AND (?3 - deleted_at) > ?4 \
                 AND EXISTS (SELECT 1 FROM gc_purge_intent AS pi \
                            WHERE pi.tenant_id = blob_meta.tenant_id AND pi.digest = blob_meta.digest \
                              AND pi.epoch = ?5 AND pi.state = 'r2_deleted') \
                 RETURNING 1 AS purged"
            ),
            &[
                json!(tenant_id.to_string()),
                json!(digest.to_string()),
                json!(now_ms),
                json!(grace_period_ms),
                json!(epoch),
            ],
        )
        .map_err(PhysicalDeleteError::Backend)?;
        Ok(!rows.is_empty())
    }
}
