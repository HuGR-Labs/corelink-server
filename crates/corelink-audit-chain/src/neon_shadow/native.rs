use super::*;

// ---------------------------------------------------------------------------
// RealNeonShadowSink — the production NeonShadowSink impl.
// ---------------------------------------------------------------------------

/// Production [`NeonShadowSink`] impl. Binds a single
/// `(tenant_id, region)` pair to a [`NeonExecutor`] so cross-tenant /
/// cross-region writes are caught at the type system AND at the SQL
/// RLS layer (defense in depth — see module docs).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
#[non_exhaustive]
pub struct RealNeonShadowSink {
    tenant_id: Uuid,
    region: Region,
    executor: Arc<dyn NeonExecutor>,
    audit_sink: Arc<dyn ShadowSyncAuditSink>,
}

#[cfg(not(target_arch = "wasm32"))]
impl RealNeonShadowSink {
    /// Construct the sink. The executor is shared via `Arc` so a
    /// single connection pool services all per-region sinks (the pool
    /// itself partitions by region at the binder layer).
    #[must_use]
    pub fn new(
        tenant_id: Uuid,
        region: Region,
        executor: Arc<dyn NeonExecutor>,
        audit_sink: Arc<dyn ShadowSyncAuditSink>,
    ) -> Self {
        Self {
            tenant_id,
            region,
            executor,
            audit_sink,
        }
    }

    /// Helper: emit a shadow-sync audit row, lifting an audit-sink
    /// rejection to [`NeonShadowError::AuditEmitFailed`].
    ///
    /// Wave-21 (B-P1-02 closure): the wave-18 helper discarded the
    /// emit result via `let _ = ...`. On a paired audit-sink-down +
    /// cross-tenant attempt the SEV-2 anchor the security team
    /// subscribes to silently vanished. The wave-20 SQL `WITH CHECK`
    /// constraint is the structural backstop; this helper closes the
    /// trait-level gap so the route layer sees the failure and can
    /// translate to a 503 + SEV-1 alert.
    ///
    /// Ordering note: the helper is still called BEFORE the original
    /// violation `Err(_)` is returned, so the SEV-2 audit row lands on
    /// the happy path. Only an audit-pipeline failure escalates to
    /// `AuditEmitFailed` (SEV-1 > SEV-2 in the operator response
    /// matrix).
    fn emit_audit(&self, row: ShadowSyncAuditRow) -> Result<(), NeonShadowError> {
        self.audit_sink
            .emit(row)
            .map_err(NeonShadowError::AuditEmitFailed)
    }

    /// Constant-time tenant id comparison via `subtle::ConstantTimeEq`.
    /// Production wiring runs the comparison on every row so a wiring
    /// bug that drops the bound tenant id surfaces immediately
    /// (defense in depth on top of the SQL RLS).
    fn tenant_eq_ct(a: Uuid, b: Uuid) -> bool {
        a.as_bytes().ct_eq(b.as_bytes()).into()
    }

    /// Reconciliation count query (used by the daily reconcile cron
    /// via a Python harness — see
    /// `.github/workflows/neon-shadow-reconcile-daily.yml`).
    ///
    /// # Errors
    ///
    /// Returns [`NeonShadowError::Backend`] when the executor errors
    /// or the count row decodes to a non-integer.
    pub fn reconcile_count(&self, from_ms: u64, to_ms: u64) -> Result<u64, NeonShadowError> {
        // RLS GUC set inside the same txn so the read sees only the
        // bound tenant (defense in depth — the executor is also bound
        // per-region at construction).
        let _ = self
            .executor
            .execute(SQL_BEGIN_TXN, &[])
            .map_err(NeonShadowError::from)?;
        let _ = self
            .executor
            .execute(
                SQL_SET_RLS_TENANT_GUC,
                &[ExecutorParam::Text(self.tenant_id.to_string())],
            )
            .map_err(NeonShadowError::from)?;
        let rows = self
            .executor
            .query(
                SQL_RECONCILE_COUNT,
                &[
                    ExecutorParam::Int8(from_ms as i64),
                    ExecutorParam::Int8(to_ms as i64),
                ],
            )
            .map_err(NeonShadowError::from)?;
        let _ = self
            .executor
            .execute(SQL_COMMIT_TXN, &[])
            .map_err(NeonShadowError::from)?;
        let first = rows.first().ok_or_else(|| {
            NeonShadowError::Internal("reconcile_count returned no rows".to_string())
        })?;
        let cell = first
            .cells
            .first()
            .and_then(|c| c.as_ref())
            .ok_or_else(|| {
                NeonShadowError::Internal("reconcile_count: null count cell".to_string())
            })?;
        cell.parse::<u64>()
            .map_err(|e| NeonShadowError::Backend(format!("reconcile count parse: {}", e)))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl NeonShadowSink for RealNeonShadowSink {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn region(&self) -> Region {
        self.region
    }

    fn sync_chunk(
        &self,
        receipt: &ArchiveReceipt,
        rows: &[ShadowEventRow],
        now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError> {
        // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification
        // (audit-ordering-high-risk-seal §Escalation 5, 2026-05-27):
        // the failure-arm audits at lines 605/621/650/666 fire AFTER
        // the `executor.execute(...)` call they observe a failure of.
        // The INV cannot logically apply: you cannot audit
        // "BEGIN failed" before BEGIN was attempted. The success-arm
        // audit at line 685 fires after COMMIT.
        //
        // The shadow-sync layer is the AUDIT-CHAIN's mirror to Neon
        // Postgres; it is OBSERVATIONAL of the primary audit emit
        // (which itself is INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
        // governed by the D1 batch pattern at the primary write
        // site). Moving `emit_audit` (itself a SQL INSERT into
        // `shadow_audit`) inside the per-row INSERT txn would
        // change commit semantics (audit-of-sync becomes part of
        // sync atomicity), violating the layered defence
        // separation per RB-REPLICA-FAILOVER §audit-chain layered
        // defences. The txn-bracketed `BEGIN → SET → INSERT … →
        // COMMIT` is the canonical observability envelope for
        // shadow-sync; failure audits report SQL outcomes.
        //
        // 1. Tenant + residency pre-checks (mirrors the in-memory fake
        //    — fail-CLOSED + audit emit BEFORE returning err).
        if !Self::tenant_eq_ct(receipt.tenant_id, self.tenant_id) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms: 0,
                failure_reason: "tenant isolation violation".to_string(),
                sev: "sev-2",
            })?;
            return Err(NeonShadowError::TenantIsolationViolation {
                sink_tenant: self.tenant_id.to_string(),
                sink_tenant_redacted: redact_tenant_uuid(&self.tenant_id),
                observed_tenant: receipt.tenant_id.to_string(),
                observed_tenant_redacted: redact_tenant_uuid(&receipt.tenant_id),
            });
        }
        for row in rows {
            if !Self::tenant_eq_ct(row.tenant_id, self.tenant_id) {
                self.emit_audit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms: 0,
                    failure_reason: "row tenant isolation violation".to_string(),
                    sev: "sev-2",
                })?;
                return Err(NeonShadowError::TenantIsolationViolation {
                    sink_tenant: self.tenant_id.to_string(),
                    sink_tenant_redacted: redact_tenant_uuid(&self.tenant_id),
                    observed_tenant: row.tenant_id.to_string(),
                    observed_tenant_redacted: redact_tenant_uuid(&row.tenant_id),
                });
            }
            if row.region != self.region {
                self.emit_audit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms: 0,
                    failure_reason: "row residency violation".to_string(),
                    sev: "sev-2",
                })?;
                return Err(NeonShadowError::ResidencyViolation {
                    sink_region: self.region.as_str(),
                    observed_region: row.region.as_str(),
                });
            }
        }
        if rows.is_empty() {
            return Err(NeonShadowError::Internal("empty rows slice".to_string()));
        }

        // 2. Compute observed lag from the first event's wall-clock to
        //    the caller-supplied `now_ms`.
        let first_event_time_ms = rows.first().map(|r| r.event_time_ms).unwrap_or(0);
        let observed_lag_ms = now_ms.saturating_sub(first_event_time_ms);

        // 3. Open txn + bind RLS tenant GUC.
        if let Err(e) = self.executor.execute(SQL_BEGIN_TXN, &[]) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: format!("BEGIN failed: {}", e),
                sev: "sev-2",
            })?;
            return Err(e.into());
        }
        if let Err(e) = self.executor.execute(
            SQL_SET_RLS_TENANT_GUC,
            &[ExecutorParam::Text(self.tenant_id.to_string())],
        ) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: format!("set_config failed: {}", e),
                sev: "sev-2",
            })?;
            return Err(e.into());
        }

        // 4. Idempotent INSERT per row. The executor implementation
        //    is expected to pipeline; the sink stays sync.
        for row in rows {
            let prev_hex = hex::encode(row.prev_hash.0);
            let link_hex = hex::encode(row.link_hash.0);
            let params = [
                ExecutorParam::Uuid(row.tenant_id),
                ExecutorParam::Int8(row.seq as i64),
                ExecutorParam::Int8(row.event_time_ms as i64),
                ExecutorParam::Text(row.event_type.clone()),
                ExecutorParam::HexBytes(prev_hex),
                ExecutorParam::HexBytes(link_hex),
                ExecutorParam::Jsonb(row.payload_json.clone()),
                ExecutorParam::Text(row.region.as_str().to_string()),
            ];
            if let Err(e) = self.executor.execute(SQL_INSERT_SHADOW_ROW, &params) {
                self.emit_audit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms,
                    failure_reason: format!("INSERT failed at seq={}: {}", row.seq, e),
                    sev: "sev-2",
                })?;
                return Err(e.into());
            }
        }

        // 5. Commit txn.
        if let Err(e) = self.executor.execute(SQL_COMMIT_TXN, &[]) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: format!("COMMIT failed: {}", e),
                sev: "sev-2",
            })?;
            return Err(e.into());
        }

        // 6. Audit emit (success).
        let sev = if observed_lag_ms >= SHADOW_LAG_SEV2_THRESHOLD_MS {
            "sev-2"
        } else {
            "info"
        };
        self.emit_audit(ShadowSyncAuditRow {
            event_type: EVENT_TYPE_SHADOW_SYNCED,
            tenant_id: self.tenant_id,
            first_seq: receipt.first_sequence_number,
            last_seq: receipt.last_sequence_number,
            region: self.region,
            observed_lag_ms,
            failure_reason: String::new(),
            sev,
        })?;

        Ok(ShadowSyncReceipt {
            tenant_id: self.tenant_id,
            first_seq: receipt.first_sequence_number,
            last_seq: receipt.last_sequence_number,
            rows_persisted: rows.len() as u64,
            observed_lag_ms,
            region: self.region,
        })
    }

    fn aggregate_event_count(
        &self,
        from_ms: u64,
        to_ms: u64,
        event_type_filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
        self.executor
            .execute(SQL_BEGIN_TXN, &[])
            .map_err(NeonShadowError::from)?;
        self.executor
            .execute(
                SQL_SET_RLS_TENANT_GUC,
                &[ExecutorParam::Text(self.tenant_id.to_string())],
            )
            .map_err(NeonShadowError::from)?;
        let rows = match event_type_filter {
            Some(ty) => self
                .executor
                .query(
                    SQL_QUERY_EVENT_COUNT_FILTERED,
                    &[
                        ExecutorParam::Int8(from_ms as i64),
                        ExecutorParam::Int8(to_ms as i64),
                        ExecutorParam::Text(ty.to_string()),
                    ],
                )
                .map_err(NeonShadowError::from)?,
            None => self
                .executor
                .query(
                    SQL_QUERY_EVENT_COUNT,
                    &[
                        ExecutorParam::Int8(from_ms as i64),
                        ExecutorParam::Int8(to_ms as i64),
                    ],
                )
                .map_err(NeonShadowError::from)?,
        };
        self.executor
            .execute(SQL_COMMIT_TXN, &[])
            .map_err(NeonShadowError::from)?;

        let mut buckets = Vec::with_capacity(rows.len());
        for (idx, row) in rows.iter().enumerate() {
            let event_type = row
                .cells
                .first()
                .and_then(|c| c.as_ref())
                .ok_or_else(|| {
                    NeonShadowError::Backend(format!(
                        "event_count row {}: null event_type cell",
                        idx
                    ))
                })?
                .clone();
            let count_str = row.cells.get(1).and_then(|c| c.as_ref()).ok_or_else(|| {
                NeonShadowError::Backend(format!("event_count row {}: null count cell", idx))
            })?;
            let count: u64 = count_str
                .parse()
                .map_err(|e| NeonShadowError::Backend(format!("count parse: {}", e)))?;
            buckets.push(EventCountBucket { event_type, count });
        }
        Ok(buckets)
    }

    fn aggregate_timeline(
        &self,
        from_ms: u64,
        to_ms: u64,
        granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
        if granularity_ms == 0 {
            return Err(NeonShadowError::Internal(
                "granularity_ms must be > 0".to_string(),
            ));
        }
        self.executor
            .execute(SQL_BEGIN_TXN, &[])
            .map_err(NeonShadowError::from)?;
        self.executor
            .execute(
                SQL_SET_RLS_TENANT_GUC,
                &[ExecutorParam::Text(self.tenant_id.to_string())],
            )
            .map_err(NeonShadowError::from)?;
        let rows = self
            .executor
            .query(
                SQL_QUERY_TIMELINE,
                &[
                    ExecutorParam::Int8(from_ms as i64),
                    ExecutorParam::Int8(to_ms as i64),
                    ExecutorParam::Int8(granularity_ms as i64),
                ],
            )
            .map_err(NeonShadowError::from)?;
        self.executor
            .execute(SQL_COMMIT_TXN, &[])
            .map_err(NeonShadowError::from)?;

        let mut buckets = Vec::with_capacity(rows.len());
        for (idx, row) in rows.iter().enumerate() {
            let bucket_str = row.cells.first().and_then(|c| c.as_ref()).ok_or_else(|| {
                NeonShadowError::Backend(format!("timeline row {}: null bucket cell", idx))
            })?;
            let count_str = row.cells.get(1).and_then(|c| c.as_ref()).ok_or_else(|| {
                NeonShadowError::Backend(format!("timeline row {}: null count cell", idx))
            })?;
            let bucket_start_ms: u64 = bucket_str
                .parse()
                .map_err(|e| NeonShadowError::Backend(format!("bucket parse: {}", e)))?;
            let count: u64 = count_str
                .parse()
                .map_err(|e| NeonShadowError::Backend(format!("count parse: {}", e)))?;
            buckets.push(TimelineBucket {
                bucket_start_ms,
                count,
            });
        }
        Ok(buckets)
    }
}
