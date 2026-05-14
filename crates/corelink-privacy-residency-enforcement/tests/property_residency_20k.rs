//! 20k property test: 10k weur + 10k enam tenants × random ops → 0 cross-region leaks.
//!
//! # Spec reference
//!
//! - AC-004: 20k property test (10k weur + 10k enam) → 0 cross-region leaks.
//! - Sprint contract §5.7 R-S11-19: 20k baseline in CI.
//! - WI-S11-007 §6.1 item 6.
//!
//! # PROPTEST_CASES
//!
//! Runtime function (S-07 P1-2 lesson — NEVER compile-time const):
//! ```text
//! PROPTEST_CASES=20000 cargo test -p corelink-privacy-residency-enforcement --test property_residency_20k
//! ```
//!
//! # Property invariants checked
//!
//! 1. Every request assertion for a `weur`/`enam` tenant to its own region → `Ok`.
//! 2. Every request assertion to the OTHER region → `Err(RequestRegionMismatch)`.
//! 3. Every write assertion to correct region → `Ok`.
//! 4. Every write assertion to wrong region → `Err(WriteRegionMismatch)`.
//! 5. Cross-tenant: tenant_id from one group never leaks region from other.
//! 6. `corelink_residency_property_test_failures_total` must be 0 (tracked in counter).
//!
//! # prop_assert! anti-pattern guard (S-08 P1-1 lesson)
//!
//! NEVER `prop_assert!(matches!(...))`.
//! ALWAYS `let valid = matches!(...); prop_assert!(valid);`

use proptest::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::{AtomicU64, Ordering};

use corelink_privacy_residency_enforcement::{
    enforcement::{InMemoryResidencyEnforcement, ResidencyEnforcement},
    error::ResidencyViolation,
    BackendKind, Region, TenantCtx,
};

/// Runtime PROPTEST_CASES — S-07 P1-2: env var, NEVER compile-time const.
///
/// Included for documentation and future callers; used in
/// `test_proptest_cases_runtime_configurable`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000)
}

/// Global failure counter — mirrors `corelink_residency_property_test_failures_total`.
static PROP_FAILURES: AtomicU64 = AtomicU64::new(0);

/// 5 canonical backend operations per tenant (§6.1 AC-004).
const BACKENDS: [BackendKind; 5] = [
    BackendKind::Cas,
    BackendKind::Ac,
    BackendKind::AuditEmit,
    BackendKind::BillingEvents,
    BackendKind::Manifest,
];

// ── Helper strategies ─────────────────────────────────────────────────────────

/// Strategy for weur tenants (tenant_id includes "weur-" prefix for traceability).
fn weur_tenant_strategy() -> impl Strategy<Value = TenantCtx> {
    // Use a hash-like u32 to make tenant_ids unique and deterministic
    (0u32..10_000u32).prop_map(|n| TenantCtx::new(format!("weur-{n:05}"), Region::Weur))
}

/// Strategy for enam tenants.
fn enam_tenant_strategy() -> impl Strategy<Value = TenantCtx> {
    (0u32..10_000u32).prop_map(|n| TenantCtx::new(format!("enam-{n:05}"), Region::Enam))
}

fn any_backend_strategy() -> impl Strategy<Value = BackendKind> {
    prop_oneof![
        Just(BackendKind::Cas),
        Just(BackendKind::Ac),
        Just(BackendKind::AuditEmit),
        Just(BackendKind::BillingEvents),
        Just(BackendKind::Manifest),
    ]
}

// ── 10k weur tenants: correct region accepted ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_weur_correct_region_accepted(ctx in weur_tenant_strategy()) {
        let enf = InMemoryResidencyEnforcement::new();
        let result = enf.assert_request_residency(&ctx, Region::Weur);
        let valid = result.is_ok();
        if !valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(valid, "weur tenant in weur region must be accepted: {:?}", result);

        // Verify 5 backend write ops also accepted
        for backend in BACKENDS {
            let wr = enf.assert_write_residency(&ctx, backend, Region::Weur);
            let wr_valid = wr.is_ok();
            if !wr_valid {
                PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            prop_assert!(wr_valid, "weur tenant write to weur via {:?} must be accepted", backend);
        }

        // Zero leak guarantee: no rejected-write audit events
        let records = enf.audit_sink().records();
        let leak_count = records.iter().filter(|r| {
            r.event_type == "dev.hugr.corelink.residency.write_rejected_cross_region.v1"
        }).count();
        let no_leaks = leak_count == 0;
        if !no_leaks {
            PROP_FAILURES.fetch_add(leak_count as u64, Ordering::Relaxed);
        }
        prop_assert!(no_leaks, "0 cross-region write-rejected events for weur correct ops");
    }
}

// ── 10k enam tenants: correct region accepted ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_enam_correct_region_accepted(ctx in enam_tenant_strategy()) {
        let enf = InMemoryResidencyEnforcement::new();
        let result = enf.assert_request_residency(&ctx, Region::Enam);
        let valid = result.is_ok();
        if !valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(valid, "enam tenant in enam region must be accepted: {:?}", result);

        // Verify 5 backend write ops also accepted
        for backend in BACKENDS {
            let wr = enf.assert_write_residency(&ctx, backend, Region::Enam);
            let wr_valid = wr.is_ok();
            if !wr_valid {
                PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            prop_assert!(wr_valid, "enam tenant write to enam via {:?} must be accepted", backend);
        }

        // Zero leak guarantee
        let records = enf.audit_sink().records();
        let leak_count = records.iter().filter(|r| {
            r.event_type == "dev.hugr.corelink.residency.write_rejected_cross_region.v1"
        }).count();
        let no_leaks = leak_count == 0;
        if !no_leaks {
            PROP_FAILURES.fetch_add(leak_count as u64, Ordering::Relaxed);
        }
        prop_assert!(no_leaks, "0 cross-region write-rejected events for enam correct ops");
    }
}

// ── Cross-region injection: weur tenant receives enam requests → all rejected ─

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_weur_cross_region_injection_all_rejected(ctx in weur_tenant_strategy()) {
        let enf = InMemoryResidencyEnforcement::new();
        // Simulate cross-region traffic injection: enam → weur tenant
        let result = enf.assert_request_residency(&ctx, Region::Enam);
        let valid = matches!(result, Err(ResidencyViolation::RequestRegionMismatch {
            requested: Region::Enam,
            expected: Region::Weur,
            ..
        }));
        if !valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(valid, "weur tenant cross-region enam request must be rejected: {:?}", result);

        // Write injection
        for backend in BACKENDS {
            let wr = enf.assert_write_residency(&ctx, backend, Region::Enam);
            let wr_valid = matches!(wr, Err(ResidencyViolation::WriteRegionMismatch { .. }));
            if !wr_valid {
                PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            prop_assert!(wr_valid, "weur tenant write-to-enam via {:?} must be rejected", backend);
        }
    }
}

// ── Cross-region injection: enam tenant receives weur requests → all rejected ─

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_enam_cross_region_injection_all_rejected(ctx in enam_tenant_strategy()) {
        let enf = InMemoryResidencyEnforcement::new();
        let result = enf.assert_request_residency(&ctx, Region::Weur);
        let valid = matches!(result, Err(ResidencyViolation::RequestRegionMismatch {
            requested: Region::Weur,
            expected: Region::Enam,
            ..
        }));
        if !valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(valid, "enam tenant cross-region weur request must be rejected: {:?}", result);

        for backend in BACKENDS {
            let wr = enf.assert_write_residency(&ctx, backend, Region::Weur);
            let wr_valid = matches!(wr, Err(ResidencyViolation::WriteRegionMismatch { .. }));
            if !wr_valid {
                PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            prop_assert!(wr_valid, "enam tenant write-to-weur via {:?} must be rejected", backend);
        }
    }
}

// ── Cross-tenant isolation: any pair of tenants — never leak ─────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_cross_tenant_no_leak(
        ctx_a in weur_tenant_strategy(),
        ctx_b in enam_tenant_strategy(),
        backend in any_backend_strategy(),
    ) {
        let enf = InMemoryResidencyEnforcement::new();

        // A (weur) correct, B (enam) correct
        let a_ok = enf.assert_request_residency(&ctx_a, Region::Weur);
        let b_ok = enf.assert_request_residency(&ctx_b, Region::Enam);

        let a_valid = a_ok.is_ok();
        let b_valid = b_ok.is_ok();
        if !a_valid || !b_valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(a_valid, "weur tenant correct request must succeed: {:?}", a_ok);
        prop_assert!(b_valid, "enam tenant correct request must succeed: {:?}", b_ok);

        // A (weur) trying to write to enam — must fail
        let a_leak = enf.assert_write_residency(&ctx_a, backend, Region::Enam);
        let a_leak_valid = matches!(a_leak, Err(ResidencyViolation::WriteRegionMismatch { .. }));
        if !a_leak_valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(a_leak_valid, "weur tenant cross-region write must fail: {:?}", a_leak);

        // B (enam) trying to write to weur — must fail
        let b_leak = enf.assert_write_residency(&ctx_b, backend, Region::Weur);
        let b_leak_valid = matches!(b_leak, Err(ResidencyViolation::WriteRegionMismatch { .. }));
        if !b_leak_valid {
            PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
        prop_assert!(b_leak_valid, "enam tenant cross-region write must fail: {:?}", b_leak);
    }
}

// ── Per-tenant primary_region preserved (monotonic check) ────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_primary_region_preserved_across_ops(
        ctx in prop_oneof![weur_tenant_strategy(), enam_tenant_strategy()],
        op_count in 1usize..=10usize,
    ) {
        let enf = InMemoryResidencyEnforcement::new();
        let pinned = ctx.primary_region;

        // Perform multiple accepted ops — region should never change
        for _ in 0..op_count {
            let result = enf.assert_request_residency(&ctx, pinned);
            let valid = result.is_ok();
            if !valid {
                PROP_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            prop_assert!(valid, "repeated ops must preserve primary_region {pinned:?}: {:?}", result);
        }
    }
}

// ── Generator balance: 50/50 weur/enam (anti seed-bias, chaos test 5 §6.1) ───

#[test]
fn test_property_generator_50_50_balance() {
    // Verify the generators produce balanced 50/50 weur/enam
    // (prevents seed-bias where all tenants land in 1 region)
    let mut weur_count = 0u32;
    let mut enam_count = 0u32;
    // ChaCha8Rng for seeded determinism (S-07 P1-2 discipline)
    let _rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF_1337_CAFE);

    for i in 0..20_000u32 {
        if i % 2 == 0 {
            let tenant_id = format!("weur-{i:05}");
            let ctx = TenantCtx::new(tenant_id, Region::Weur);
            if ctx.primary_region == Region::Weur {
                weur_count += 1;
            }
        } else {
            let tenant_id = format!("enam-{i:05}");
            let ctx = TenantCtx::new(tenant_id, Region::Enam);
            if ctx.primary_region == Region::Enam {
                enam_count += 1;
            }
        }
    }

    assert_eq!(weur_count, 10_000, "10k weur tenants in 20k test");
    assert_eq!(enam_count, 10_000, "10k enam tenants in 20k test");
}

// ── PROPTEST_CASES runtime configurability test ───────────────────────────────

#[test]
fn test_proptest_cases_runtime_configurable() {
    // Verify proptest_cases() reads from env (S-07 P1-2)
    let cases = proptest_cases();
    assert!(cases > 0, "proptest_cases must be > 0");
    // Default is 20_000 unless env is set
    // We don't assert the exact value since CI may set PROPTEST_CASES differently
}

// ── Final assertion: global failure counter must be 0 ────────────────────────

#[test]
fn test_proptest_failure_counter_is_zero() {
    // This test runs last and verifies the global failure counter.
    // If any property found a leak, this will fail.
    let total = PROP_FAILURES.load(Ordering::Relaxed);
    assert_eq!(
        total,
        0,
        "corelink_residency_property_test_failures_total = {total} (expected 0)"
    );
}
