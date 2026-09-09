
fn candidate_from_row(row: &D1Row) -> Result<GcCandidate, String> {
    let status = match text(row, "status")?.as_str() {
        "candidate" => CandidateStatus::Candidate,
        "swept" => CandidateStatus::Swept,
        "physically_deleted" => CandidateStatus::PhysicallyDeleted,
        "protected_re_ref" => CandidateStatus::ProtectedReRef,
        other => return Err(format!("unknown candidate status {other:?}")),
    };
    Ok(GcCandidate {
        tenant_id: uuid_column(row, "tenant_id")?,
        digest: BlobDigest::parse(&text(row, "digest")?).map_err(|e| e.to_string())?,
        mark_started_at_ms: number(row, "mark_started_at_ms")?,
        mark_run_id: RunId(uuid_column(row, "mark_run_id")?),
        blob_size_bytes: number(row, "blob_size_bytes")?,
        blob_last_referenced_at_ms: number(row, "blob_last_referenced_at_ms")?,
        status,
        created_at_ms: number(row, "created_at_ms")?,
        swept_at_ms: optional_number(row, "swept_at_ms")?,
        protected_at_ms: optional_number(row, "protected_at_ms")?,
        protected_reason: optional_text(row, "protected_reason")?,
    })
}
/// D1-backed `gc_candidates` implementation.
#[derive(Debug)]
#[non_exhaustive]
pub struct D1GcCandidatesStore {
    d1: Arc<D1HttpClient>,
    region: GcRegion,
    max_candidates: u32,
}
impl D1GcCandidatesStore {
    /// Construct a bounded observation store over a shared D1 client.
    ///
    /// # Errors
    ///
    /// Rejects a zero limit or any limit above the production ceiling.
    pub fn new(
        d1: Arc<D1HttpClient>,
        region: GcRegion,
        max_candidates: u32,
    ) -> Result<Self, String> {
        validate_observation_candidate_limit(max_candidates)?;
        Ok(Self {
            d1,
            region,
            max_candidates,
        })
    }
}
impl GcCandidatesStore for D1GcCandidatesStore {
    fn insert_candidate(&self, c: GcCandidate) -> Result<(), MarkError> {
        query_sync(&self.d1, "INSERT OR IGNORE INTO gc_candidates (tenant_id, digest, mark_started_at_ms, mark_run_id, blob_size_bytes, blob_last_referenced_at_ms, status, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", &[json!(c.tenant_id.to_string()), json!(c.digest.to_string()), json!(c.mark_started_at_ms), json!(c.mark_run_id.as_text()), json!(c.blob_size_bytes), json!(c.blob_last_referenced_at_ms), json!(c.status.as_str()), json!(c.created_at_ms)]).map_err(MarkError::Backend).map(|_| ())
    }
    fn snapshot_for_run(
        &self,
        tenant_id: Uuid,
        run_id: RunId,
    ) -> Result<Vec<GcCandidate>, MarkError> {
        let query_limit = self.max_candidates.saturating_add(1);
        let rows = query_sync(&self.d1, "SELECT c.tenant_id, c.digest, c.mark_started_at_ms, c.mark_run_id, c.blob_size_bytes, c.blob_last_referenced_at_ms, c.status, c.created_at_ms, c.swept_at_ms, c.protected_at_ms, c.protected_reason FROM gc_candidates AS c WHERE c.tenant_id = ?1 AND c.mark_run_id = ?2 AND EXISTS (SELECT 1 FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id = bm.tenant_id WHERE bm.tenant_id = c.tenant_id AND bm.digest = c.digest AND bm.region = t.primary_region AND ((?3 = 'iad' AND bm.region IN ('wnam', 'enam')) OR (?3 = 'lhr' AND bm.region = 'weur') OR (?3 = 'nrt' AND bm.region = 'apac') OR (?3 = 'sam' AND bm.region = 'sam'))) ORDER BY c.digest LIMIT ?4", &[json!(tenant_id.to_string()), json!(run_id.as_text()), json!(self.region.as_str()), json!(query_limit)]).map_err(MarkError::Backend)?;
        if rows.len() > self.max_candidates as usize {
            return Err(MarkError::Backend(format!(
                "GC observation candidate population exceeds configured limit {}",
                self.max_candidates
            )));
        }
        rows.iter()
            .map(candidate_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(MarkError::Backend)
    }
    fn count_for_run(&self, tenant_id: Uuid, run_id: RunId) -> Result<u64, MarkError> {
        let rows = query_sync(
            &self.d1,
            "SELECT COUNT(*) AS count FROM gc_candidates WHERE tenant_id = ?1 AND mark_run_id = ?2",
            &[json!(tenant_id.to_string()), json!(run_id.as_text())],
        )
        .map_err(MarkError::Backend)?;
        rows.first().map_or(Ok(0), |row| {
            number(row, "count").map_err(MarkError::Backend)
        })
    }
    fn lookup(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        run_id: RunId,
    ) -> Result<Option<GcCandidate>, MarkError> {
        let rows = query_sync(&self.d1, "SELECT tenant_id, digest, mark_started_at_ms, mark_run_id, blob_size_bytes, blob_last_referenced_at_ms, status, created_at_ms, swept_at_ms, protected_at_ms, protected_reason FROM gc_candidates WHERE tenant_id = ?1 AND digest = ?2 AND mark_run_id = ?3", &[json!(tenant_id.to_string()), json!(digest.to_string()), json!(run_id.as_text())]).map_err(MarkError::Backend)?;
        rows.first()
            .map(candidate_from_row)
            .transpose()
            .map_err(MarkError::Backend)
    }
    fn transition_status(
        &self,
        tenant_id: Uuid,
        digest: &BlobDigest,
        run_id: RunId,
        from: CandidateStatus,
        to: CandidateStatus,
        now_ms: u64,
        reason: Option<String>,
    ) -> Result<bool, MarkError> {
        let rows = query_sync(&self.d1, "UPDATE gc_candidates SET status = ?4, swept_at_ms = CASE WHEN ?4 = 'swept' THEN ?6 ELSE swept_at_ms END, physically_deleted_at_ms = CASE WHEN ?4 = 'physically_deleted' THEN ?6 ELSE physically_deleted_at_ms END, protected_at_ms = CASE WHEN ?4 = 'protected_re_ref' THEN ?6 ELSE protected_at_ms END, protected_reason = CASE WHEN ?4 = 'protected_re_ref' THEN ?7 ELSE protected_reason END WHERE tenant_id = ?1 AND digest = ?2 AND mark_run_id = ?3 AND status = ?5 RETURNING 1 AS changed", &[json!(tenant_id.to_string()), json!(digest.to_string()), json!(run_id.as_text()), json!(to.as_str()), json!(from.as_str()), json!(now_ms), json!(reason)]).map_err(MarkError::Backend)?;
        Ok(!rows.is_empty())
    }
}
