/// R2 adapter with an exact tenant-key check before every delete.
pub struct R2GcDelete {
    r2: Arc<R2S3Client>,
    tdk: [u8; 32],
}

impl core::fmt::Debug for R2GcDelete {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2GcDelete")
            .field("r2", &self.r2)
            .field("tdk", &"[REDACTED]")
            .finish()
    }
}
impl R2GcDelete {
    /// Construct over the production R2 client.
    pub fn new(r2: Arc<R2S3Client>, tdk: [u8; 32]) -> Self {
        Self { r2, tdk }
    }
}
impl R2Delete for R2GcDelete {
    fn delete(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
        key: &str,
    ) -> Result<R2DeleteOutcome, R2DeleteError> {
        let digest = key
            .rsplit('/')
            .next()
            .ok_or_else(|| R2DeleteError::Backend("R2 key has no digest".to_owned()))?;
        let digest =
            BlobDigest::parse(digest).map_err(|e| R2DeleteError::Backend(e.to_string()))?;
        let expected = canonical_blob_key(region, &self.tdk, tenant_id, &digest)
            .map_err(R2DeleteError::Backend)?;
        if key != expected {
            return Err(R2DeleteError::Backend(
                "R2 key is outside the configured tenant/region namespace".to_owned(),
            ));
        }
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            R2DeleteError::Backend("R2 adapter requires a running Tokio runtime".to_owned())
        })?;
        let result =
            tokio::task::block_in_place(|| handle.block_on(self.r2.delete_if_present(key)))
                .map_err(R2DeleteError::Backend)?;
        Ok(if result.is_some() {
            R2DeleteOutcome::Deleted
        } else {
            R2DeleteOutcome::NotFound
        })
    }
}

/// D1 outbox audit sink for GC events.
#[derive(Debug)]
pub struct D1GcAuditSink {
    d1: Arc<D1HttpClient>,
}
impl D1GcAuditSink {
    /// Construct over D1.
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}
impl GcAuditSink for D1GcAuditSink {
    fn emit(&self, record: GcAuditRecord) -> Result<(), GcAuditSinkError> {
        // B071's D1 finalize DELETE fires `trg_gc_purge_finalize_atomic`,
        // which inserts this event in the same SQLite transaction as the
        // metadata deletion. Emitting it again here would create a second
        // audit row with a different request id; the trigger is authoritative.
        if record.event_type == GcEventType::PhysicalDeleted {
            return Ok(());
        }
        let event = record.event_type.as_str();
        let residency_region = tenant_residency_region(&self.d1, record.tenant_id, record.region)
            .map_err(GcAuditSinkError::Store)?;
        let payload = json!({ "event_type": event, "run_id": record.run_id.as_text(), "tenant_id": record.tenant_id.to_string(), "region": record.region.as_str(), "gc_region": record.region.as_str(), "residency_region": residency_region, "status": record.status.as_str(), "reason": record.reason, "now_ms": record.now_ms }).to_string();
        query_sync(&self.d1, "INSERT OR IGNORE INTO audit_outbox (id, tenant_id, region, request_id, event_type, payload_json, enqueued_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", &[json!(Uuid::new_v4().to_string()), json!(record.tenant_id.to_string()), json!(residency_region), json!(format!("gc:{}:{}", record.run_id, event)), json!(event), json!(payload), json!(record.now_ms)]).map_err(GcAuditSinkError::Store).map(|_| ())
    }
}

/// Metrics adapter: labels are emitted to the container's structured log.
#[derive(Debug)]
pub struct TracingGcMetrics;
impl GcMetricsObserver for TracingGcMetrics {
    fn record_cron_fired(&self, region: GcRegion) -> Result<(), GcMetricsObserverError> {
        info!(
            metric = GcMetricKind::CronFired.as_str(),
            region = region.as_str(),
            "gc metric"
        );
        Ok(())
    }
    fn record_run_started(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<(), GcMetricsObserverError> {
        info!(metric = GcMetricKind::RunStarted.as_str(), %tenant_id, region = region.as_str(), "gc metric");
        Ok(())
    }
    fn record_run_completed(
        &self,
        tenant_id: Uuid,
        region: GcRegion,
        status: GcStatus,
    ) -> Result<(), GcMetricsObserverError> {
        info!(metric = GcMetricKind::RunCompleted.as_str(), %tenant_id, region = region.as_str(), status = status.as_str(), "gc metric");
        Ok(())
    }
    fn record_phase_duration_ms(
        &self,
        phase: GcPhase,
        tenant_id: Uuid,
        region: GcRegion,
        duration_ms: u64,
    ) -> Result<(), GcMetricsObserverError> {
        info!(metric = GcMetricKind::PhaseDurationMs.as_str(), %tenant_id, region = region.as_str(), phase = phase.as_str(), duration_ms, "gc metric");
        Ok(())
    }
    fn record_degrade_mode_active(
        &self,
        kind: corelink_gc::DegradeKind,
    ) -> Result<(), GcMetricsObserverError> {
        info!(
            metric = GcMetricKind::DegradeModeActive.as_str(),
            kind = kind.as_str(),
            "gc metric"
        );
        Ok(())
    }
    fn record_stale_running_count(&self, count: u64) -> Result<(), GcMetricsObserverError> {
        info!(
            metric = GcMetricKind::StaleRunningCount.as_str(),
            count, "gc metric"
        );
        Ok(())
    }
}

/// Wall clock used by the native adapter; it never moves backwards.
#[derive(Debug, Default)]
pub struct SystemGcClock(Mutex<u64>);
impl PhysicalDeleteClock for SystemGcClock {
    fn now_ms(&self) -> u64 {
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut last = self.0.lock().unwrap_or_else(|p| p.into_inner());
        *last = (*last).max(wall);
        *last
    }
}

/// Structured D1 report sink. Reports are audit-outbox records, not stdout-only.
#[derive(Debug)]
pub struct D1GcReportSink {
    d1: Arc<D1HttpClient>,
}
impl D1GcReportSink {
    /// Construct over D1.
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}
impl GcSweepReportSink for D1GcReportSink {
    fn emit_report(&self, report: &SweepReport) -> Result<(), GcSweepReportError> {
        let residency_region = tenant_residency_region(&self.d1, report.tenant_id, report.region)
            .map_err(|e| GcSweepReportError::Store(e.to_string()))?;
        let payload = json!({ "mode": report.mode.as_str(), "run_id": report.run_id.as_text(), "tenant_id": report.tenant_id.to_string(), "region": report.region.as_str(), "gc_region": report.region.as_str(), "residency_region": residency_region, "candidates_scanned": report.candidates_scanned, "reclaimable_count": report.reclaimable_count, "reclaimable_bytes": report.reclaimable_bytes, "deleted_count": report.deleted_count, "deleted_bytes": report.deleted_bytes, "reclaimable_keys": report.reclaimable_keys, "now_ms": report.now_ms }).to_string();
        query_sync(&self.d1, "INSERT OR IGNORE INTO audit_outbox (id, tenant_id, region, request_id, event_type, payload_json, enqueued_at) VALUES (?1, ?2, ?3, ?4, 'corelink.gc.sweep.report', ?5, ?6)", &[json!(Uuid::new_v4().to_string()), json!(report.tenant_id.to_string()), json!(residency_region), json!(format!("gc-report:{}", report.run_id)), json!(payload), json!(report.now_ms)]).map_err(|e| GcSweepReportError::Store(e.to_string())).map(|_| ())
    }
}
