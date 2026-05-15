//! DO-to-D1 sync-age SLI integration tests
//! (closes DEBT-011 R-PREP-REPL-P1-003).
//!
//! Covers:
//! 1. Metric name + per-class ceilings match `slo_catalog.md §4.26` exactly
//!    (LOAD-BEARING for the verifier).
//! 2. `DoClass::RateLimiter` is excluded from the SLO (`budget_seconds() == None`)
//!    per audit §3.5 edge case (a) "intentional reset on failover".
//! 3. `TenantQuota` and `ConfigSingleton` budgets enforce strict `<= ceiling`
//!    semantics at the boundary.
//! 4. InMemory probe is deterministic across all (DoClass, Region) pairs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests use direct assertions"
)]

use corelink_region::do_sync_age::{
    DoClass, DoSyncAgeProbe, DoSyncAgeSample, FailingDoSyncAgeProbe, InMemoryDoSyncAgeProbe,
    DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS, DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS,
    DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS, METRIC_DO_SYNC_AGE_SECONDS,
};
use corelink_region::Region;

#[test]
fn metric_name_matches_verifier() {
    // LOAD-BEARING for `scripts/verify-replication-lag.py::PROM_METRIC["do"]`.
    assert_eq!(METRIC_DO_SYNC_AGE_SECONDS, "corelink_do_sync_age_seconds");
}

#[test]
fn slo_constants_match_catalog_4_26() {
    assert_eq!(DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS, 300);
    assert_eq!(DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS, 60);
    assert!(DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS.is_none());
}

#[test]
fn rate_limiter_excluded_from_slo_per_audit_3_5() {
    // Audit §3.5 edge case (a) — RateLimiter intentionally resets on failover.
    // Therefore it must never be flagged by the verifier.
    assert!(DoClass::RateLimiter.budget_seconds().is_none());

    // Even an absurdly stale sample is "within budget" for RateLimiter
    // (verifier ignores it entirely).
    let stale = DoSyncAgeSample {
        do_class: DoClass::RateLimiter,
        region: Region::Wnam,
        age_seconds: 999_999.0,
        sample_timestamp_ms: 0,
    };
    assert!(stale.within_budget());
}

#[test]
fn tenant_quota_boundary_within_budget() {
    let exact = DoSyncAgeSample {
        do_class: DoClass::TenantQuota,
        region: Region::Wnam,
        age_seconds: 300.0,
        sample_timestamp_ms: 0,
    };
    assert!(exact.within_budget(), "exactly 300s is within budget (<=)");

    let over = DoSyncAgeSample {
        do_class: DoClass::TenantQuota,
        region: Region::Wnam,
        age_seconds: 300.001,
        sample_timestamp_ms: 0,
    };
    assert!(!over.within_budget(), "300.001s violates budget");
}

#[test]
fn config_singleton_tight_60s_budget() {
    // Consent propagation must be tight (60s) per CTRL-PRIV-CONSENT-002.
    let breach = DoSyncAgeSample {
        do_class: DoClass::ConfigSingleton,
        region: Region::Weur,
        age_seconds: 90.0,
        sample_timestamp_ms: 0,
    };
    assert!(!breach.within_budget());
}

#[test]
fn inmemory_probe_all_classes_all_regions() {
    let p = InMemoryDoSyncAgeProbe::new();
    let classes = [
        DoClass::TenantQuota,
        DoClass::ConfigSingleton,
        DoClass::RateLimiter,
    ];
    // Seed deterministic ages — all under budget.
    for c in classes {
        for r in Region::ALL {
            p.set_age(c, *r, 30.0);
        }
    }
    let mut samples = Vec::new();
    for c in classes {
        for r in Region::ALL {
            samples.push(p.probe(c, *r, 0).expect("probe"));
        }
    }
    assert_eq!(samples.len(), 3 * 4, "3 classes × 4 regions = 12 samples");
    assert!(samples.iter().all(|s| s.within_budget()));
}

#[test]
fn inmemory_probe_breach_detected_for_non_excluded_classes() {
    let p = InMemoryDoSyncAgeProbe::new();
    // Inject breaches across both non-excluded classes.
    p.set_age(DoClass::TenantQuota, Region::Wnam, 400.0); // > 300s
    p.set_age(DoClass::ConfigSingleton, Region::Weur, 75.0); // > 60s
    p.set_age(DoClass::RateLimiter, Region::Sam, 9_999.0); // excluded

    let s1 = p.probe(DoClass::TenantQuota, Region::Wnam, 0).unwrap();
    let s2 = p.probe(DoClass::ConfigSingleton, Region::Weur, 0).unwrap();
    let s3 = p.probe(DoClass::RateLimiter, Region::Sam, 0).unwrap();

    assert!(!s1.within_budget(), "TenantQuota 400s breaches 300s");
    assert!(!s2.within_budget(), "ConfigSingleton 75s breaches 60s");
    assert!(s3.within_budget(), "RateLimiter excluded — always within");
}

#[test]
fn failing_probe_returns_err_for_sev3_path() {
    let p = FailingDoSyncAgeProbe::new("simulated DO outage");
    let r = p.probe(DoClass::TenantQuota, Region::Wnam, 0);
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("simulated"));
}

#[test]
fn class_labels_are_canonical_for_prometheus() {
    // LOAD-BEARING for alerting rule label matchers.
    assert_eq!(DoClass::TenantQuota.as_str(), "tenant_quota");
    assert_eq!(DoClass::ConfigSingleton.as_str(), "config_singleton");
    assert_eq!(DoClass::RateLimiter.as_str(), "rate_limiter");
}
