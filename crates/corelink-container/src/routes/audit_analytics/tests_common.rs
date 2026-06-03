//! Shared test fixtures for the `audit_analytics/tests_*.rs` files.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Fixture bodies are verbatim copies of the original inline `mod tests`
//! block.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::{Arc, Mutex};

use corelink_analytics::Region;
use corelink_audit_chain::{
    ArchiveReceipt, EventCountBucket, InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink,
    NeonShadowError, NeonShadowSink, ShadowEventRow, ShadowSyncReceipt, TimelineBucket,
};
use uuid::Uuid;

use super::audit_sink::AnalyticsAuditSink;
use super::shadow_factory::ShadowSinkFactory;
use super::state::{build_state, AuditAnalyticsRouteState};
use super::types::AnalyticsAuditRow;

/// Audit sink that always errors on `emit` — drives the
/// `audit pipeline closed → 503` fail-CLOSED tests.
#[derive(Debug, Default)]
pub(super) struct FailingAnalyticsAuditSink;

impl AnalyticsAuditSink for FailingAnalyticsAuditSink {
    fn emit(&self, _row: AnalyticsAuditRow) -> Result<(), &'static str> {
        Err("injected analytics audit emit failure")
    }
}

/// Shadow sink that returns the bound tenant id BUT always errors
/// on `aggregate_timeline` — drives the `handle_timeline` error
/// arm test.
#[derive(Debug)]
pub(super) struct AggregateTimelineFailsShadow {
    pub(super) tenant_id: Uuid,
    pub(super) region: Region,
}

impl NeonShadowSink for AggregateTimelineFailsShadow {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    fn region(&self) -> Region {
        self.region
    }
    fn sync_chunk(
        &self,
        _receipt: &ArchiveReceipt,
        _rows: &[ShadowEventRow],
        _now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError> {
        Err(NeonShadowError::Backend("not used in test".to_string()))
    }
    fn aggregate_event_count(
        &self,
        _from_ms: u64,
        _to_ms: u64,
        _filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
        Err(NeonShadowError::Backend("injected".to_string()))
    }
    fn aggregate_timeline(
        &self,
        _from_ms: u64,
        _to_ms: u64,
        _granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
        Err(NeonShadowError::Backend(
            "injected aggregate_timeline failure".to_string(),
        ))
    }
}

/// Build route state with a tenant-mismatch shadow sink (the
/// factory returns a sink whose `tenant_id()` is DIFFERENT from
/// the resolved tenant) + a `FailingAnalyticsAuditSink`. Used by
/// `tenant_isolation_violation_returns_503_on_audit_sink_failure`.
pub(super) fn state_with_tenant_mismatch_and_failing_audit(
    requested_tenant: Uuid,
    bound_tenant: Uuid,
) -> AuditAnalyticsRouteState {
    let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
    let shadow: Arc<dyn NeonShadowSink> = Arc::new(InMemoryNeonShadowSink::new(
        bound_tenant,
        Region::Iad,
        audit,
    ));
    // Factory returns the wrongly-bound sink for the requested tenant.
    #[derive(Debug)]
    struct WrongBoundFactory {
        wanted: Uuid,
        shadow: Arc<dyn NeonShadowSink>,
    }
    impl ShadowSinkFactory for WrongBoundFactory {
        fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
            if tenant_id == self.wanted {
                Ok(self.shadow.clone())
            } else {
                Err("not bound")
            }
        }
    }
    let factory: Arc<dyn ShadowSinkFactory> = Arc::new(WrongBoundFactory {
        wanted: requested_tenant,
        shadow,
    });
    let mut state = build_state(factory);
    // Swap the audit sink for the failing one.
    state.audit_sink = Arc::new(FailingAnalyticsAuditSink);
    state
}

/// Test factory bound to a single tenant; returns its
/// pre-constructed shadow on match, error otherwise.
#[derive(Debug)]
pub(super) struct OneTenantFactory {
    pub(super) tenant: Uuid,
    pub(super) shadow: Arc<dyn NeonShadowSink>,
}

impl ShadowSinkFactory for OneTenantFactory {
    fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        if tenant_id == self.tenant {
            Ok(self.shadow.clone())
        } else {
            Err("tenant not bound in test factory")
        }
    }
}

/// Recording factory that captures which method was invoked
/// (`for_tenant` legacy path vs. `for_tenant_in_region` wave-27
/// shadow-aware path) so the tests can assert the consumer adoption
/// without relying on observable side effects on the shadow sink.
#[derive(Debug)]
pub(super) struct RecordingShadowFactory {
    tenant: Uuid,
    region: Region,
    audit: Arc<InMemoryShadowSyncAuditSink>,
    pub(super) for_tenant_calls: Arc<Mutex<u32>>,
    pub(super) for_tenant_in_region_calls: Arc<Mutex<(u32, Option<Region>)>>,
}

impl RecordingShadowFactory {
    pub(super) fn new(tenant: Uuid, region: Region) -> Self {
        Self {
            tenant,
            region,
            audit: Arc::new(InMemoryShadowSyncAuditSink::new()),
            for_tenant_calls: Arc::new(Mutex::new(0)),
            for_tenant_in_region_calls: Arc::new(Mutex::new((0, None))),
        }
    }
}

impl ShadowSinkFactory for RecordingShadowFactory {
    fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        *self.for_tenant_calls.lock().expect("legacy counter") += 1;
        if tenant_id == self.tenant {
            Ok(Arc::new(InMemoryNeonShadowSink::new(
                self.tenant,
                self.region,
                self.audit.clone(),
            )))
        } else {
            Err("tenant not bound in recording factory")
        }
    }

    fn for_tenant_in_region(
        &self,
        tenant_id: Uuid,
        region: Region,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        let mut g = self
            .for_tenant_in_region_calls
            .lock()
            .expect("shadow-aware counter");
        g.0 += 1;
        g.1 = Some(region);
        drop(g);
        if tenant_id == self.tenant {
            Ok(Arc::new(InMemoryNeonShadowSink::new(
                self.tenant,
                region,
                self.audit.clone(),
            )))
        } else {
            Err("tenant not bound in recording factory")
        }
    }
}
