#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
//! Property tests for the DO config-singleton (WI-S13-001).
//!
//! # Coverage
//!
//! - `prop_config_cas_concurrent_no_lost_writes`: sequential simulation of
//!   concurrent CAS updates — exactly 1 winner per round, 0 lost writes.
//! - `prop_config_schema_drift_rejected`: payloads with `schema_version != 1`
//!   are always rejected.
//! - `prop_config_rollback_target_validation`: VersionExpired for >90d,
//!   VersionUnknown for non-existent, Ok for valid.
//! - `prop_config_payload_validation`: `rollout_pct > 100` rejected,
//!   `refill_rate == 0` rejected, `ttl_days > 365` rejected.
//!
//! Nightly CI overrides `PROPTEST_CASES=100000`.

use std::sync::Arc;

use corelink_config_do::{
    AdminActor, ConfigError, ConfigPayload, FeatureFlag, RateLimitKey,
    RateLimitTunable, RetentionPolicy, SUPPORTED_SCHEMA_VERSION,
    metrics::NoopMetrics,
    store::{ConfigSingletonStore, InMemoryAuditSink, InMemoryConfigSingletonStore},
    validation::validate_payload,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

fn dummy_actor() -> AdminActor {
    AdminActor {
        user_id: Uuid::nil(),
        email_hash: [0u8; 32],
    }
}

fn genesis_payload() -> ConfigPayload {
    ConfigPayload::genesis()
}

fn payload_with_flag(rollout_pct: u8) -> ConfigPayload {
    let mut p = genesis_payload();
    p.feature_flags.insert(
        "test-flag".into(),
        FeatureFlag {
            enabled: true,
            rollout_pct,
            allowlist_tenants: vec![],
        },
    );
    p
}

fn payload_with_rate_limit(refill: u32) -> ConfigPayload {
    let mut p = genesis_payload();
    p.rate_limits.insert(
        RateLimitKey {
            layer: "cas_put".into(),
            tier: "Solo".into(),
        },
        RateLimitTunable {
            refill_rate_per_sec: refill,
            burst: 10,
        },
    );
    p
}

fn payload_with_retention(ttl_days: u32) -> ConfigPayload {
    let mut p = genesis_payload();
    p.retention_policies
        .insert("blobs".into(), RetentionPolicy { ttl_days });
    p
}

fn new_store() -> InMemoryConfigSingletonStore {
    InMemoryConfigSingletonStore::new(
        Arc::new(InMemoryAuditSink::new()),
        Arc::new(NoopMetrics),
    )
}

// ── prop_config_cas_concurrent_no_lost_writes ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1, // outer loop drives iteration via proptest_cases()
        ..Default::default()
    })]

    #[test]
    fn prop_config_cas_concurrent_no_lost_writes(_dummy in 0u8..1) {
        let cases = proptest_cases();
        for _ in 0..cases {
            // Simulate 2 concurrent callers both reading version=N and trying to
            // CAS update. Exactly 1 must win; the other gets VersionConflict.
            let store = new_store();
            let actor = dummy_actor();
            let now_ms = 1_000_000_u64;

            // Round 0: store starts at version 0.
            let (current_v, _) = tokio_test::block_on(store.current()).expect("current");
            assert_eq!(current_v, 0, "initial version must be 0");

            // Both callers read version=0 and attempt update.
            let r1 = tokio_test::block_on(store.update(0, genesis_payload(), &actor, now_ms));
            let r2 = tokio_test::block_on(store.update(0, genesis_payload(), &actor, now_ms));

            // Exactly one must succeed and one must return VersionConflict.
            let successes = [&r1, &r2]
                .iter()
                .filter(|r| r.is_ok())
                .count();
            let conflicts = [&r1, &r2]
                .iter()
                .filter(|r| matches!(r, Err(ConfigError::VersionConflict { .. })))
                .count();
            assert_eq!(successes, 1, "exactly 1 winner per round");
            assert_eq!(conflicts, 1, "exactly 1 conflict per round");

            let (new_v, _) = tokio_test::block_on(store.current()).expect("current after update");
            assert_eq!(new_v, 1, "version must advance to 1 after one winner");
        }
    }
}

// ── prop_config_schema_drift_rejected ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1,
        ..Default::default()
    })]

    #[test]
    fn prop_config_schema_drift_rejected(_dummy in 0u8..1) {
        let cases = proptest_cases();
        for bad_version in [0u32, 2, 100, u32::MAX]
            .iter()
            .cycle()
            .take(cases as usize)
        {
            let mut payload = genesis_payload();
            payload.schema_version = *bad_version;
            let result = validate_payload(&payload);
            assert!(
                result.is_err(),
                "schema_version={bad_version} must be rejected"
            );
            match result {
                Err(ConfigError::SchemaInvalid(_)) => {}
                other => panic!("expected SchemaInvalid, got {other:?}"),
            }
        }
    }
}

// ── prop_config_rollback_target_validation ───────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1,
        ..Default::default()
    })]

    #[test]
    fn prop_config_rollback_target_validation(_dummy in 0u8..1) {
        let cases = proptest_cases();
        let store = new_store();
        let actor = dummy_actor();
        let base_ms: u64 = 10_000_000;

        // Build up 5 historical versions.
        for i in 0..5u64 {
            let v = tokio_test::block_on(
                store.update(i, genesis_payload(), &actor, base_ms + i * 1000),
            )
            .expect("update");
            assert_eq!(v, i + 1);
        }
        // current version = 5

        let (cur, _) = tokio_test::block_on(store.current()).expect("current");
        assert_eq!(cur, 5);

        let mut ok_count = 0u64;
        let mut expired_count = 0u64;
        let mut unknown_count = 0u64;

        for idx in 0..cases {
            let scenario = idx % 3;
            if scenario == 0 {
                // Valid rollback target (version 1–5).
                let target = (idx % 5) + 1;
                let now_ms = base_ms + 5_000; // well within 90d
                let result =
                    tokio_test::block_on(store.rollback_to(target.into(), &actor, now_ms));
                match result {
                    Ok(_) => ok_count += 1,
                    other => panic!("expected Ok for valid rollback, got {other:?}"),
                }
            } else if scenario == 1 {
                // Expired: pretend 91d have passed since a version was created.
                // We simulate by using a now_ms far in the future.
                // All existing versions were created at base_ms; add 91d.
                let target = 1;
                let ninety_one_days_ms: u64 = 91 * 24 * 60 * 60 * 1000;
                let future_now = base_ms + ninety_one_days_ms + 1;
                let result =
                    tokio_test::block_on(store.rollback_to(target, &actor, future_now));
                match result {
                    Err(ConfigError::VersionExpired(_)) => expired_count += 1,
                    other => panic!("expected VersionExpired, got {other:?}"),
                }
            } else {
                // Unknown version.
                let target = 99_999;
                let result =
                    tokio_test::block_on(store.rollback_to(target, &actor, base_ms));
                match result {
                    Err(ConfigError::VersionUnknown(_)) => unknown_count += 1,
                    other => panic!("expected VersionUnknown, got {other:?}"),
                }
            }
        }

        // Each bucket must have been exercised.
        assert!(ok_count > 0, "ok path must fire");
        assert!(expired_count > 0, "expired path must fire");
        assert!(unknown_count > 0, "unknown path must fire");
    }
}

// ── prop_config_payload_validation ───────────────────────────────────────────

proptest! {
    #[test]
    fn prop_config_payload_validation(
        rollout_pct in 101u8..=255u8,
    ) {
        let payload = payload_with_flag(rollout_pct);
        let result = validate_payload(&payload);
        assert!(result.is_err(), "rollout_pct={rollout_pct} must be rejected");
        match result {
            Err(ConfigError::SchemaInvalid(_)) => {}
            other => panic!("expected SchemaInvalid, got {other:?}"),
        }
    }
}

proptest! {
    #[test]
    fn prop_config_refill_rate_zero_rejected(_dummy in 0u8..1) {
        let payload = payload_with_rate_limit(0);
        let result = validate_payload(&payload);
        assert!(result.is_err(), "refill_rate=0 must be rejected");
        match result {
            Err(ConfigError::SchemaInvalid(_)) => {}
            other => panic!("expected SchemaInvalid, got {other:?}"),
        }
    }
}

proptest! {
    #[test]
    fn prop_config_ttl_days_over_365_rejected(
        ttl in 366u32..=1000u32,
    ) {
        let payload = payload_with_retention(ttl);
        let result = validate_payload(&payload);
        assert!(result.is_err(), "ttl_days={ttl} must be rejected");
        match result {
            Err(ConfigError::SchemaInvalid(_)) => {}
            other => panic!("expected SchemaInvalid, got {other:?}"),
        }
    }
}

// ── audit fail-CLOSED regression ─────────────────────────────────────────────

#[test]
fn test_audit_fail_closed_aborts_mutation() {
    use corelink_config_do::store::{ConfigSingletonStore, FailingAuditSink};
    let store = InMemoryConfigSingletonStore::new(
        Arc::new(FailingAuditSink),
        Arc::new(NoopMetrics),
    );
    let actor = dummy_actor();

    let result = tokio_test::block_on(store.update(0, genesis_payload(), &actor, 1_000));
    assert!(result.is_err(), "audit failure must abort update");

    // State must remain at version 0 (mutation aborted).
    let (v, _) = tokio_test::block_on(store.current()).expect("current");
    assert_eq!(v, 0, "state must be unchanged after audit failure");
}

// ── schema_version=1 valid ───────────────────────────────────────────────────

#[test]
fn test_schema_version_1_valid() {
    let payload = genesis_payload();
    assert_eq!(payload.schema_version, SUPPORTED_SCHEMA_VERSION);
    let result = validate_payload(&payload);
    assert!(result.is_ok(), "genesis payload must be valid");
}

// ── rollout_pct boundary ─────────────────────────────────────────────────────

#[test]
fn test_rollout_pct_100_valid() {
    let payload = payload_with_flag(100);
    let result = validate_payload(&payload);
    assert!(result.is_ok(), "rollout_pct=100 must be valid");
}

#[test]
fn test_rollout_pct_0_valid() {
    let payload = payload_with_flag(0);
    let result = validate_payload(&payload);
    assert!(result.is_ok(), "rollout_pct=0 must be valid");
}
