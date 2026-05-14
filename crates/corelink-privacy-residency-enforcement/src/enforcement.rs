//! `ResidencyEnforcement` trait + `InMemoryResidencyEnforcement` orchestrator.
//!
//! Orchestrates the full residency enforcement stack:
//! - Tenant store lookup.
//! - Pre-flight request assertion (`assert_request_residency`).
//! - Pre-flight write assertion (`assert_write_residency`).
//! - Audit emit fail-CLOSED.
//!
//! Per-instance `Arc<Mutex<>>` (F-001 closure — NEVER `static LazyLock<Mutex<>>`).

use std::sync::{Arc, Mutex};

use crate::{
    assert_request::{assert_request_residency, RequestAssertionResult},
    assert_write::{assert_write_residency, WriteAssertionResult},
    audit_emit::{InMemoryResidencyAuditSink, ResidencyAuditSink},
    error::ResidencyViolation,
    BackendKind, Region, TenantCtx,
};

/// Residency enforcement orchestrator trait.
///
/// Production implementation wires:
/// - Cloudflare D1 for tenant lookup.
/// - R2 `audit-<region>` Object Lock 7y for audit emit.
///
/// The in-memory implementation is used in property tests and unit tests.
pub trait ResidencyEnforcement: Send + Sync {
    /// Pre-flight request assertion.
    ///
    /// Verifies that `request_region` matches `tenant_ctx.primary_region`.
    ///
    /// Fail-CLOSED: audit emitted BEFORE result returned. Returns
    /// `Err(ResidencyViolation::RequestRegionMismatch)` on mismatch or
    /// `Err(ResidencyViolation::AuditEmitFailure)` if audit infra unavailable.
    fn assert_request_residency(
        &self,
        tenant_ctx: &TenantCtx,
        request_region: Region,
    ) -> Result<RequestAssertionResult, ResidencyViolation>;

    /// Insert check pre-flight assertion.
    ///
    /// Verifies that the backend write `target_region` matches `tenant_ctx.primary_region`.
    /// Audit emitted only on mismatch. Returns
    /// `Err(ResidencyViolation::WriteRegionMismatch)` on mismatch or
    /// `Err(ResidencyViolation::AuditEmitFailure)` if audit infra unavailable.
    fn assert_write_residency(
        &self,
        tenant_ctx: &TenantCtx,
        backend: BackendKind,
        target_region: Region,
    ) -> Result<WriteAssertionResult, ResidencyViolation>;
}

/// In-memory residency enforcement orchestrator for tests.
///
/// Per-instance `Arc<Mutex<>>` (F-001 closure).
#[derive(Debug, Clone)]
pub struct InMemoryResidencyEnforcement {
    audit_sink: Arc<InMemoryResidencyAuditSink>,
    /// Counter used for deterministic event IDs in tests.
    counter: Arc<Mutex<u64>>,
}

impl InMemoryResidencyEnforcement {
    /// Create a new orchestrator with a fresh in-memory audit sink.
    pub fn new() -> Self {
        Self {
            audit_sink: Arc::new(InMemoryResidencyAuditSink::new()),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Access the captured audit records.
    pub fn audit_sink(&self) -> &InMemoryResidencyAuditSink {
        &self.audit_sink
    }

    fn next_event_id(&self) -> String {
        let mut c = self.counter.lock().unwrap_or_else(|e| e.into_inner());
        *c += 1;
        format!("evt-{:08x}", *c)
    }
}

impl Default for InMemoryResidencyEnforcement {
    fn default() -> Self {
        Self::new()
    }
}

impl ResidencyEnforcement for InMemoryResidencyEnforcement {
    fn assert_request_residency(
        &self,
        tenant_ctx: &TenantCtx,
        request_region: Region,
    ) -> Result<RequestAssertionResult, ResidencyViolation> {
        let event_id = self.next_event_id();
        assert_request_residency(
            tenant_ctx,
            request_region,
            self.audit_sink.as_ref() as &dyn ResidencyAuditSink,
            &event_id,
            "2026-05-13T00:00:00Z",
        )
    }

    fn assert_write_residency(
        &self,
        tenant_ctx: &TenantCtx,
        backend: BackendKind,
        target_region: Region,
    ) -> Result<WriteAssertionResult, ResidencyViolation> {
        let event_id = self.next_event_id();
        assert_write_residency(
            tenant_ctx,
            backend,
            target_region,
            self.audit_sink.as_ref() as &dyn ResidencyAuditSink,
            &event_id,
            "2026-05-13T00:00:00Z",
        )
    }
}

/// Fail-CLOSED enforcement wrapper — wraps any `ResidencyAuditSink` that always fails.
///
/// Used in chaos tests to verify that state is NOT mutated when audit emit fails.
#[derive(Debug)]
pub struct FailClosedResidencyEnforcement {
    audit_sink: Arc<crate::audit_emit::FailingResidencyAuditSink>,
    counter: Arc<Mutex<u64>>,
}

impl FailClosedResidencyEnforcement {
    /// Create an orchestrator whose audit sink always fails.
    pub fn new() -> Self {
        Self {
            audit_sink: Arc::new(crate::audit_emit::FailingResidencyAuditSink),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    fn next_event_id(&self) -> String {
        let mut c = self.counter.lock().unwrap_or_else(|e| e.into_inner());
        *c += 1;
        format!("fail-evt-{:08x}", *c)
    }
}

impl Default for FailClosedResidencyEnforcement {
    fn default() -> Self {
        Self::new()
    }
}

impl ResidencyEnforcement for FailClosedResidencyEnforcement {
    fn assert_request_residency(
        &self,
        tenant_ctx: &TenantCtx,
        request_region: Region,
    ) -> Result<RequestAssertionResult, ResidencyViolation> {
        let event_id = self.next_event_id();
        assert_request_residency(
            tenant_ctx,
            request_region,
            self.audit_sink.as_ref() as &dyn ResidencyAuditSink,
            &event_id,
            "2026-05-13T00:00:00Z",
        )
    }

    fn assert_write_residency(
        &self,
        tenant_ctx: &TenantCtx,
        backend: BackendKind,
        target_region: Region,
    ) -> Result<WriteAssertionResult, ResidencyViolation> {
        let event_id = self.next_event_id();
        assert_write_residency(
            tenant_ctx,
            backend,
            target_region,
            self.audit_sink.as_ref() as &dyn ResidencyAuditSink,
            &event_id,
            "2026-05-13T00:00:00Z",
        )
    }
}
