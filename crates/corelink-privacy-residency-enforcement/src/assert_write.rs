//! Backend write pre-flight assertion (insert check layer).
//!
//! Implements `assert_write_residency`: verifies that the backend write
//! target region matches `tenant.primary_region`.
//!
//! This is the Worker pre-flight layer (defensive depth-in-defense); the
//! D1 trigger (`trg_blob_meta_region_match`) provides the database-level
//! backstop for direct API bypass attempts.
//!
//! # Audit fail-CLOSED ordering
//!
//! `lookup → emit_audit → mutate_state` per S-06 P0-2 lesson.
//! State NEVER mutated if audit emit fails.

use crate::{
    audit_emit::{ResidencyAuditRecord, ResidencyAuditSink, WriteRejectedPayload},
    error::ResidencyViolation,
    BackendKind, Region, TenantCtx,
};

/// Result of a write assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteAssertionResult {
    /// Backend that was asserted.
    pub backend: BackendKind,
    /// Target region (matches tenant.primary_region).
    pub target_region: Region,
}

/// Assert that the backend write target region matches tenant.primary_region.
///
/// # Fail-CLOSED ordering
///
/// 1. `lookup` — resolve `TenantCtx::primary_region`.
/// 2. `emit_audit` — emit `write_rejected_cross_region.v1` CloudEvent if mismatch.
/// 3. Return `Ok` if match, `Err(ResidencyViolation)` if mismatch or audit fail.
///
/// **Note**: audit is only emitted on mismatch (a rejected cross-region write).
/// Accepted writes are silent at the audit layer to avoid noise; the routing
/// layer emits `request_routed.v1` for every request including accepted ones.
pub fn assert_write_residency(
    ctx: &TenantCtx,
    backend: BackendKind,
    target_region: Region,
    audit_sink: &dyn ResidencyAuditSink,
    event_id: &str,
    event_time: &str,
) -> Result<WriteAssertionResult, ResidencyViolation> {
    // Step 1: lookup
    let expected = ctx.primary_region;

    if target_region != expected {
        // Step 2: emit audit (fail-CLOSED — BEFORE returning error)
        let payload = WriteRejectedPayload {
            tenant_id: ctx.tenant_id.clone(),
            attempted_region: target_region,
            expected_region: expected,
            backend,
        };
        let record = ResidencyAuditRecord::write_rejected(event_id, event_time, payload);
        audit_sink
            .emit(record)
            .map_err(|reason| ResidencyViolation::AuditEmitFailure { reason })?;

        // Step 3: return violation error
        return Err(ResidencyViolation::WriteRegionMismatch {
            tenant_id: ctx.tenant_id.clone(),
            backend,
            target_region,
            expected,
        });
    }

    // Target region matches — no audit emit needed for accepted writes.
    Ok(WriteAssertionResult {
        backend,
        target_region,
    })
}
