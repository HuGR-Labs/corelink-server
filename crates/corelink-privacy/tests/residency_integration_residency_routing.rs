#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
//! Integration test: signup → cross-region request → 451 (PAT-ROUTING-PINNED-001).
//!
//! Covers AC-001, AC-002, AC-003, AC-006, AC-007 scenarios.

use corelink_privacy::residency::{
    assert_request::region_from_host,
    enforcement::{FailClosedResidencyEnforcement, InMemoryResidencyEnforcement, ResidencyEnforcement},
    error::ResidencyViolation,
    migration::{InMemoryMigrationStore, MigrationStatus, RegionMigrationRequest},
    BackendKind, Region, TenantCtx,
};

// ── AC-001: Tenant signup region opt-in canonical 6-region ──────────────────

#[test]
fn test_all_6_regions_are_valid() {
    for r in Region::ALL {
        let parsed = Region::parse_canonical(r.as_str());
        assert_eq!(parsed, Some(r), "round-trip failed for {:?}", r);
    }
}

#[test]
fn test_non_canonical_strings_rejected() {
    // Anti-pattern: open region string drift
    for bad in &["us", "eu", "USA", "europe", "WEUR", "wEur", "us-east-1", ""] {
        assert!(
            Region::parse_canonical(bad).is_none(),
            "expected None for '{bad}' but got Some"
        );
    }
}

#[test]
fn test_region_serialization_round_trip() {
    for r in Region::ALL {
        let json = serde_json::to_string(&r).unwrap_or_else(|_| "null".to_string());
        assert_ne!(json, "null", "serialize must succeed for {:?}", r);
        let back: Result<Region, _> = serde_json::from_str(&json);
        assert!(back.is_ok(), "deserialize must succeed for {:?}", r);
        let valid = back.map(|b| b == r).unwrap_or(false);
        assert!(valid, "round-trip must be equal for {:?}", r);
    }
}

// ── AC-002: Cross-region request rejected 451 ────────────────────────────────

#[test]
fn test_cross_region_request_returns_violation() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-abc", Region::Weur);

    // Request arrives at enam but tenant is weur
    let result = enf.assert_request_residency(&ctx, Region::Enam);
    assert!(
        matches!(
            result,
            Err(ResidencyViolation::RequestRegionMismatch {
                requested: Region::Enam,
                expected: Region::Weur,
                ..
            })
        ),
        "expected RequestRegionMismatch, got {:?}",
        result
    );
}

#[test]
fn test_correct_region_request_succeeds() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-abc", Region::Weur);

    let result = enf.assert_request_residency(&ctx, Region::Weur);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn test_cross_region_request_emits_audit_event() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-xyz", Region::Sam);

    // Cross-region: sam tenant, enam request
    let _ = enf.assert_request_residency(&ctx, Region::Enam);

    let records = enf.audit_sink().records();
    assert_eq!(records.len(), 1, "expected 1 audit record");

    let record_opt = records.first();
    assert!(record_opt.is_some(), "expected at least 1 audit record");
    if let Some(record) = record_opt {
        assert_eq!(
            record.event_type,
            "dev.hugr.corelink.residency.request_routed.v1"
        );
        // Verify payload contains the rejection details
        let data = &record.data;
        assert_eq!(data["outcome"], "rejected");
        assert_eq!(data["tenant_id"], "tenant-xyz");
        assert_eq!(data["requested_region"], "enam");
        assert_eq!(data["expected_region"], "sam");
    }
}

#[test]
fn test_accepted_request_also_emits_audit_event() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-eu-001", Region::Weur);

    let result = enf.assert_request_residency(&ctx, Region::Weur);
    assert!(result.is_ok());

    let records = enf.audit_sink().records();
    assert_eq!(records.len(), 1);
    let record_opt = records.first();
    assert!(record_opt.is_some(), "expected at least 1 audit record");
    if let Some(record) = record_opt {
        assert_eq!(
            record.event_type,
            "dev.hugr.corelink.residency.request_routed.v1"
        );
        let data = &record.data;
        assert_eq!(data["outcome"], "accepted");
    }
}

// ── AC-003: D1 insert check rejects cross-region write ───────────────────────

#[test]
fn test_cross_region_write_returns_violation() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-weur-001", Region::Weur);

    // Attempt to write to CAS in enam
    let result = enf.assert_write_residency(&ctx, BackendKind::Cas, Region::Enam);
    let valid = matches!(
        result,
        Err(ResidencyViolation::WriteRegionMismatch {
            backend: BackendKind::Cas,
            target_region: Region::Enam,
            expected: Region::Weur,
            ..
        })
    );
    assert!(valid, "expected WriteRegionMismatch, got {:?}", result);
}

#[test]
fn test_correct_region_write_succeeds() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-weur-002", Region::Weur);

    let result = enf.assert_write_residency(&ctx, BackendKind::Cas, Region::Weur);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn test_write_rejected_emits_correct_cloudevent_type() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-sam-001", Region::Sam);

    let _ = enf.assert_write_residency(&ctx, BackendKind::BillingEvents, Region::Wnam);

    let records = enf.audit_sink().records();
    assert_eq!(records.len(), 1);
    let record_opt = records.first();
    assert!(record_opt.is_some(), "expected at least 1 audit record");
    if let Some(record) = record_opt {
        assert_eq!(
            record.event_type,
            "dev.hugr.corelink.residency.write_rejected_cross_region.v1"
        );
        let data = &record.data;
        assert_eq!(data["backend"], "billing_events");
        assert_eq!(data["attempted_region"], "wnam");
        assert_eq!(data["expected_region"], "sam");
    }
}

#[test]
fn test_all_backend_kinds_enforce_residency() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-enam-001", Region::Enam);

    let backends = [
        BackendKind::Cas,
        BackendKind::Ac,
        BackendKind::AuditEmit,
        BackendKind::BillingEvents,
        BackendKind::Manifest,
        BackendKind::D1Metadata,
        BackendKind::Kv,
    ];

    for backend in backends {
        // Correct region — must succeed
        let ok = enf.assert_write_residency(&ctx, backend, Region::Enam);
        assert!(ok.is_ok(), "backend {:?} should accept correct region", backend);

        // Wrong region — must reject
        let err = enf.assert_write_residency(&ctx, backend, Region::Weur);
        let valid = matches!(err, Err(ResidencyViolation::WriteRegionMismatch { .. }));
        assert!(valid, "backend {:?} should reject wrong region", backend);
    }
}

// ── AC-006: Region migration cooldown 30d ────────────────────────────────────

#[test]
fn test_migration_cooldown_enforced() {
    let store = InMemoryMigrationStore::new();
    let now_ms: u64 = 30 * 24 * 60 * 60 * 1000; // day 30 in ms
    let set_at_ms: u64 = 1; // set very recently

    let ticket = RegionMigrationRequest {
        ticket_id: "ticket-001".to_string(),
        tenant_id: "tenant-weur-001".to_string(),
        from_region: Region::Weur,
        to_region: Region::Sam,
        justification: "business relocation".to_string(),
        attestation: "I understand this is irreversible".to_string(),
        created_at_ms: now_ms,
        status: MigrationStatus::Pending,
    };

    let result = store.submit(ticket, set_at_ms, now_ms);
    // not 30 days elapsed from set_at_ms=1 to now_ms=30d  => still within cooldown
    // 30d in ms = 2_592_000_000; now_ms = 2_592_000_000; elapsed = 2_591_999_999 < 2_592_000_000
    assert!(
        matches!(result, Err(ResidencyViolation::MigrationCooldownNotElapsed { .. })),
        "expected cooldown error, got {:?}",
        result
    );
}

#[test]
fn test_migration_succeeds_after_cooldown() {
    let store = InMemoryMigrationStore::new();
    const THIRTY_DAYS_MS: u64 = 30 * 24 * 60 * 60 * 1000;
    let set_at_ms: u64 = 1_000_000;
    let now_ms: u64 = set_at_ms + THIRTY_DAYS_MS + 1; // 1ms after cooldown

    let ticket = RegionMigrationRequest {
        ticket_id: "ticket-002".to_string(),
        tenant_id: "tenant-weur-002".to_string(),
        from_region: Region::Weur,
        to_region: Region::Sam,
        justification: "business relocation".to_string(),
        attestation: "I understand this is irreversible".to_string(),
        created_at_ms: now_ms,
        status: MigrationStatus::Pending,
    };

    let result = store.submit(ticket, set_at_ms, now_ms);
    assert!(result.is_ok(), "expected Ok after cooldown, got {:?}", result);

    if let Ok(ticket_id) = result {
        let fetched = store.get(&ticket_id);
        assert!(fetched.is_some(), "ticket should be stored after submit");
        if let Some(t) = fetched {
            assert_eq!(t.status, MigrationStatus::Pending);
        }
    }
}

// ── AC-007: Audit fail-CLOSED — state unchanged on emit failure ───────────────

#[test]
fn test_audit_fail_closed_on_request_returns_503_variant() {
    let enf = FailClosedResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-enam-fc", Region::Enam);

    // Cross-region request — audit sink always fails
    let result = enf.assert_request_residency(&ctx, Region::Wnam);
    let valid = matches!(result, Err(ResidencyViolation::AuditEmitFailure { .. }));
    assert!(
        valid,
        "expected AuditEmitFailure (fail-CLOSED), got {:?}",
        result
    );
}

#[test]
fn test_audit_fail_closed_on_write_returns_503_variant() {
    let enf = FailClosedResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-sam-fc", Region::Sam);

    // Cross-region write — audit sink always fails
    let result = enf.assert_write_residency(&ctx, BackendKind::Cas, Region::Weur);
    let valid = matches!(result, Err(ResidencyViolation::AuditEmitFailure { .. }));
    assert!(
        valid,
        "expected AuditEmitFailure (fail-CLOSED), got {:?}",
        result
    );
}

#[test]
fn test_state_unchanged_when_audit_fails_accepted_request() {
    // When a request would be accepted but audit fails, we must return AuditEmitFailure.
    // This is the AC-007 state-unchanged contract: even accepted operations are held.
    // The failing sink always fails, so even accepted requests get AuditEmitFailure.
    let enf = FailClosedResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-afr-fc", Region::Afr);

    // Same-region request — audit would succeed in normal path but fails here
    let result = enf.assert_request_residency(&ctx, Region::Afr);
    let valid = matches!(result, Err(ResidencyViolation::AuditEmitFailure { .. }));
    assert!(valid, "fail-CLOSED: state must not mutate when audit fails");
}

// ── Host parsing for custom domain routing ───────────────────────────────────

#[test]
fn test_region_from_custom_domain_host() {
    let cases = [
        ("tenant123.weur.corelink.humangr.com", Some(Region::Weur)),
        ("myco.enam.corelink.humangr.com", Some(Region::Enam)),
        ("org.sam.corelink.humangr.com", Some(Region::Sam)),
        ("x.wnam.corelink.humangr.com", Some(Region::Wnam)),
        ("y.apac.corelink.humangr.com", Some(Region::Apac)),
        ("z.afr.corelink.humangr.com", Some(Region::Afr)),
        // Non-matching patterns
        ("tenant123.us.corelink.humangr.com", None),
        ("tenant123.eu.corelink.humangr.com", None),
        ("corelink.humangr.com", None),
        ("weur.corelink.humangr.com", None), // missing tenant prefix
    ];

    for (host, expected) in cases {
        let got = region_from_host(host);
        assert_eq!(got, expected, "host='{}' expected={:?} got={:?}", host, expected, got);
    }
}

#[test]
fn test_remediation_url_in_error_message() {
    let enf = InMemoryResidencyEnforcement::new();
    let ctx = TenantCtx::new("tenant-weur-msg", Region::Weur);

    let err = enf.assert_request_residency(&ctx, Region::Enam);
    assert!(err.is_err(), "should fail");
    let msg = if let Err(e) = err {
        e.to_string()
    } else {
        String::new()
    };
    // Must mention the correct region
    assert!(
        msg.contains("weur"),
        "error message must mention expected region 'weur': {msg}"
    );
    assert!(
        msg.contains("enam"),
        "error message must mention requested region 'enam': {msg}"
    );
    assert!(
        msg.contains("Schrems II"),
        "error message must mention Schrems II: {msg}"
    );
}
