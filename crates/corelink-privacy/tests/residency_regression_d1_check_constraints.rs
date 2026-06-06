#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
//! Regression tests: D1 CHECK + trigger constraint enforcement.
//!
//! These tests verify that:
//! - Insert checks correctly fire for all 5+ paths (C-1.5).
//! - Audit fail-CLOSED: state is UNCHANGED when audit emit fails (S-06 P0-2).
//! - The canonical 6-region enum rejects non-canonical strings.
//! - Cross-tenant isolation: tenant A's region does not affect tenant B.
//!
//! Corresponds to AC-003, AC-007, C-1.5, S-5.2, S-5.3.

use corelink_privacy::residency::{
    enforcement::{
        FailClosedResidencyEnforcement, InMemoryResidencyEnforcement, ResidencyEnforcement,
    },
    error::ResidencyViolation,
    BackendKind, Region, TenantCtx,
};

// ── D1 insert checks: all 5+ backend paths ───────────────────────────────────

#[test]
fn test_insert_check_all_5_paths_correct_region() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-d1-001", Region::Sam);

    let paths = [
        BackendKind::Cas,
        BackendKind::Ac,
        BackendKind::AuditEmit,
        BackendKind::BillingEvents,
        BackendKind::Manifest,
    ];

    for backend in paths {
        let result = enf.assert_write_residency(&ctx, backend, Region::Sam);
        assert!(
            result.is_ok(),
            "insert check should pass for backend {:?} in correct region",
            backend
        );
    }
    // No rejected-write audit events should be emitted
    assert_eq!(
        enf.audit_sink().records().len(),
        0,
        "no audit records for accepted writes"
    );
}

#[test]
fn test_insert_check_all_5_paths_wrong_region_fires() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-d1-002", Region::Sam);

    let paths = [
        BackendKind::Cas,
        BackendKind::Ac,
        BackendKind::AuditEmit,
        BackendKind::BillingEvents,
        BackendKind::Manifest,
    ];

    for backend in paths {
        let result = enf.assert_write_residency(&ctx, backend, Region::Enam);
        let valid = matches!(result, Err(ResidencyViolation::WriteRegionMismatch { .. }));
        assert!(
            valid,
            "insert check should fire for backend {:?} in wrong region",
            backend
        );
    }

    // Each rejected write should emit one audit event
    assert_eq!(
        enf.audit_sink().records().len(),
        paths.len(),
        "each rejected write must emit one audit event"
    );
}

// ── Audit fail-CLOSED: state unchanged on emit failure ───────────────────────

#[test]
fn test_state_unchanged_on_audit_emit_failure() {
    // Invariant: when audit emit fails, we must return AuditEmitFailure,
    // not the underlying ResidencyViolation. This ensures state is held.
    let enf = FailClosedResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-fc-write", Region::Weur);

    let result = enf.assert_write_residency(&ctx, BackendKind::Cas, Region::Enam);
    let valid = matches!(result, Err(ResidencyViolation::AuditEmitFailure { .. }));
    assert!(
        valid,
        "must return AuditEmitFailure (not WriteRegionMismatch) when audit fails: {:?}",
        result
    );
}

#[test]
fn test_state_unchanged_request_audit_failure() {
    let enf = FailClosedResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-fc-req", Region::Wnam);

    // Even a cross-region request must return AuditEmitFailure when audit is down
    let result = enf.assert_request_residency(&ctx, Region::Afr);
    let valid = matches!(result, Err(ResidencyViolation::AuditEmitFailure { .. }));
    assert!(
        valid,
        "must return AuditEmitFailure when audit fails on cross-region request: {:?}",
        result
    );
}

// ── Cross-tenant isolation ────────────────────────────────────────────────────

#[test]
fn test_cross_tenant_isolation() {
    let enf = InMemoryResidencyEnforcement::new();

    let tenant_eu = TenantCtx::new("tenant-eu-iso", Region::Weur);
    let tenant_br = TenantCtx::new("tenant-br-iso", Region::Sam);

    // Each tenant's assertion is fully independent
    let eu_ok = enf.assert_request_residency(&tenant_eu, Region::Weur);
    let br_ok = enf.assert_request_residency(&tenant_br, Region::Sam);
    assert!(eu_ok.is_ok());
    assert!(br_ok.is_ok());

    // Cross-tenant: eu tenant cannot be "accepted" in sam
    let eu_err = enf.assert_request_residency(&tenant_eu, Region::Sam);
    let valid = matches!(
        eu_err,
        Err(ResidencyViolation::RequestRegionMismatch { .. })
    );
    assert!(valid, "eu tenant must be rejected in sam region");

    // And br tenant in weur
    let br_err = enf.assert_request_residency(&tenant_br, Region::Weur);
    let valid2 = matches!(
        br_err,
        Err(ResidencyViolation::RequestRegionMismatch { .. })
    );
    assert!(valid2, "br tenant must be rejected in weur region");
}

// ── Closed enum cardinality discipline ───────────────────────────────────────

#[test]
fn test_closed_enum_no_open_strings() {
    // Verify that known bad strings that look like regions but aren't canonical
    // are rejected (cardinality discipline anti-pattern §32)
    let bad_strings = [
        "US",
        "EU",
        "us-east-1",
        "eu-west-1",
        "us-central",
        "brazil",
        "europe",
        "northamerica",
        "southamerica",
        "asia",
        "africa",
        "wEUR",
        "WEUR",
    ];

    for bad in bad_strings {
        assert!(
            Region::parse_canonical(bad).is_none(),
            "'{bad}' should be rejected — closed enum cardinality discipline"
        );
    }
}

// ── Error message contains regulatory reference ───────────────────────────────

#[test]
fn test_write_error_contains_regulatory_reference() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-reg-test", Region::Weur);

    let err = enf.assert_write_residency(&ctx, BackendKind::AuditEmit, Region::Enam);
    assert!(err.is_err(), "should fail");
    let msg = if let Err(e) = err {
        e.to_string()
    } else {
        String::new()
    };
    assert!(
        msg.contains("INV-DATA-RESIDENCY"),
        "write error must reference INV-DATA-RESIDENCY: {msg}"
    );
}

// ── Region enum all 6 arms have distinct as_str() ────────────────────────────

#[test]
fn test_region_as_str_distinct() {
    use std::collections::HashSet;
    let strs: HashSet<&str> = Region::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(
        strs.len(),
        6,
        "all 6 regions must have distinct as_str values"
    );
}

#[test]
fn test_region_display_equals_as_str() {
    for r in Region::ALL {
        assert_eq!(
            r.to_string(),
            r.as_str(),
            "Display must match as_str for {:?}",
            r
        );
    }
}
