//! Worker pre-flight assertion for request routing.
//!
//! Implements `assert_request_residency`: verifies that the inbound request
//! is routed to the tenant's `primary_region`. Failure → 451
//! `legal_residency_violation` (PAT-ROUTING-PINNED-001 fail-CLOSED canonical).
//!
//! Custom domain pattern: `<tenant_id>.<region>.corelink.humangr.com`
//!
//! # Audit fail-CLOSED ordering
//!
//! Per S-06 P0-2 lesson: `lookup → emit_audit → mutate_state`.
//! State is never mutated if audit emit fails (ResidencyViolation::AuditEmitFailure).

use super::{
    audit_emit::{RequestRoutedPayload, ResidencyAuditRecord, ResidencyAuditSink, RoutingOutcome},
    error::ResidencyViolation,
    Region, TenantCtx,
};

/// Parse the region from a custom domain host string.
///
/// Accepts `<tenant_id>.<region>.corelink.humangr.com` or bare `<region>` for tests.
/// Returns `None` if the host does not match the custom domain pattern.
///
/// This is the canonical host-parsing entry point used by the Worker routing
/// layer (PAT-ROUTING-PINNED-001 §3.4).
pub fn region_from_host(host: &str) -> Option<Region> {
    // Pattern: <tenant_id>.<region>.corelink.humangr.com
    // We split on `.` and look for the second-to-last segment being a valid region
    // when the last two segments are `corelink.humangr.com`.
    let parts: Vec<&str> = host.split('.').collect();
    let n = parts.len();
    // Canonical post-`4094165b` form: `<tenant>.<region>.corelink.humangr.com`
    // (n == 5 with the last three segments `corelink`, `humangr`, `com`
    // and the region candidate at index `n-4`). Pre-existing implementation
    // matched only the legacy `.corelink.dev` 4-segment form; this branch
    // restores parity with the test fixtures + the rest of the docs
    // (`integration_residency_routing::test_region_from_custom_domain_host`).
    if n >= 5 {
        let last = parts.get(n - 1).copied().unwrap_or("");
        let second_last = parts.get(n - 2).copied().unwrap_or("");
        let third_last = parts.get(n - 3).copied().unwrap_or("");
        let region_candidate = parts.get(n - 4).copied().unwrap_or("");
        let suffix_ok = third_last == "corelink" && second_last == "humangr" && last == "com";
        if suffix_ok {
            return Region::parse_canonical(region_candidate);
        }
    }
    if n >= 4 {
        // Legacy `<tenant>.<region>.corelink.dev` form (kept for in-place
        // upgrades that haven't migrated their DNS yet).
        let last = parts.get(n - 1).copied().unwrap_or("");
        let second_last = parts.get(n - 2).copied().unwrap_or("");
        let region_candidate = parts.get(n - 3).copied().unwrap_or("");
        let suffix_ok = second_last == "corelink" && last == "dev";
        if suffix_ok {
            return Region::parse_canonical(region_candidate);
        }
    }
    // Fallback: bare region string (used in tests / internal routing).
    Region::parse_canonical(host)
}

/// Result of a routing assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestAssertionResult {
    /// Outcome: accepted or rejected.
    pub outcome: RoutingOutcome,
    /// The region the request arrived at.
    pub requested_region: Region,
    /// The tenant's canonical region.
    pub expected_region: Region,
}

/// Assert that the inbound request region matches tenant.primary_region.
///
/// # Fail-CLOSED ordering
///
/// 1. `lookup` — resolve `TenantCtx::primary_region`.
/// 2. `emit_audit` — emit `request_routed.v1` CloudEvent.
/// 3. Return `Ok` or `Err(ResidencyViolation)`.
///
/// State mutation (e.g. incrementing a counter) MUST only happen after
/// this function returns `Ok`. If `emit_audit` fails, returns
/// `ResidencyViolation::AuditEmitFailure` and no state is mutated.
pub fn assert_request_residency(
    ctx: &TenantCtx,
    request_region: Region,
    audit_sink: &dyn ResidencyAuditSink,
    event_id: &str,
    event_time: &str,
) -> Result<RequestAssertionResult, ResidencyViolation> {
    // Step 1: lookup
    let expected = ctx.primary_region;
    let outcome = if request_region == expected {
        RoutingOutcome::Accepted
    } else {
        RoutingOutcome::Rejected
    };

    // Step 2: emit audit (fail-CLOSED — BEFORE returning result)
    let payload = RequestRoutedPayload {
        tenant_id: ctx.tenant_id.clone(),
        requested_region: request_region,
        expected_region: expected,
        outcome,
    };
    let record = ResidencyAuditRecord::request_routed(event_id, event_time, payload);
    audit_sink
        .emit(record)
        .map_err(|reason| ResidencyViolation::AuditEmitFailure { reason })?;

    // Step 3: return result (state mutation must happen AFTER this)
    if outcome == RoutingOutcome::Rejected {
        return Err(ResidencyViolation::RequestRegionMismatch {
            tenant_id: ctx.tenant_id.clone(),
            requested: request_region,
            expected,
        });
    }

    Ok(RequestAssertionResult {
        outcome,
        requested_region: request_region,
        expected_region: expected,
    })
}
