//! Property: re-acceptance idempotency + version-skip resilience.
//!
//! - First call with the latest published version succeeds.
//! - Second call (replay) fails with `NoPendingReacceptance` (the
//!   tenant is no longer pending after the first call).
//! - Stale-version submit fails with `VersionMismatch`.
//! - After a Major bump while the tenant is mid-grace, the tenant's
//!   state remains pending until they accept the LATEST version
//!   (version-skip prevention).
//!
//! In CI: `PROPTEST_CASES=10000 cargo test -p corelink-dpa-versioning
//! --test prop_re_accept_idempotency`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use std::sync::Arc;

use proptest::prelude::*;
use uuid::Uuid;

use corelink_dpa_versioning::broadcast::{BroadcastSink, InMemoryBroadcastSink};
use corelink_dpa_versioning::error::DpaVersioningError;
use corelink_dpa_versioning::re_accept::ReAcceptHandler;
use corelink_dpa_versioning::schema::TenantDpaState;
use corelink_dpa_versioning::store::{DpaStore, InMemoryDpaStore};
use corelink_dpa_versioning::version::SemverVersion;
use corelink_dpa_versioning::versioning::{DpaVersioning, InMemoryDpaVersioning};

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512)
}

fn setup() -> (Arc<InMemoryDpaStore>, InMemoryDpaVersioning, Uuid) {
    let store = Arc::new(InMemoryDpaStore::new());
    let sink = Arc::new(InMemoryBroadcastSink::new());
    let sink_dyn: Arc<dyn BroadcastSink> = sink;
    let orch = InMemoryDpaVersioning::new(store.clone(), sink_dyn);
    let tenant = Uuid::from_u128(42);
    let v1 = SemverVersion::new(1, 0, 0);
    orch.on_dpa_version_bumped(v1, "h".into(), "u".into(), 0)
        .unwrap();
    store
        .seed_tenant(TenantDpaState::fresh(tenant, v1))
        .unwrap();
    (store, orch, tenant)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// First-call succeeds; second-call (replay) errors.
    #[test]
    fn re_accept_replay_returns_no_pending(
        now_offset in 1_i64..100_000,
    ) {
        let (store, orch, tenant) = setup();
        let v2 = SemverVersion::new(2, 0, 0);
        orch.on_dpa_version_bumped(v2, "h2".into(), "u2".into(), 100).unwrap();
        let handler = ReAcceptHandler::new(store.clone());
        let now = 100 + now_offset;
        let r1 = handler.re_accept(tenant, v2, now);
        prop_assert!(r1.is_ok(), "first call must succeed");
        let r2 = handler.re_accept(tenant, v2, now + 1);
        prop_assert!(matches!(r2, Err(DpaVersioningError::NoPendingReacceptance)));
    }

    /// Stale-version submit fails with VersionMismatch.
    #[test]
    fn stale_version_rejected(
        stale_minor in 0_u32..5,
    ) {
        let (store, orch, tenant) = setup();
        let v2 = SemverVersion::new(2, 0, 0);
        orch.on_dpa_version_bumped(v2, "h2".into(), "u2".into(), 100).unwrap();
        let handler = ReAcceptHandler::new(store);
        let stale = SemverVersion::new(1, stale_minor, 0);
        let r = handler.re_accept(tenant, stale, 200);
        let is_mismatch = matches!(r, Err(DpaVersioningError::VersionMismatch { .. }));
        prop_assert!(is_mismatch);
    }

    /// Version-skip: after v1→v2→v3 the tenant must submit v3 (latest).
    /// Submitting v2 fails.
    #[test]
    fn version_skip_forces_latest(
        _seed in 0_u32..100,
    ) {
        let (store, orch, tenant) = setup();
        let v2 = SemverVersion::new(2, 0, 0);
        let v3 = SemverVersion::new(3, 0, 0);
        orch.on_dpa_version_bumped(v2, "h2".into(), "u2".into(), 100).unwrap();
        orch.on_dpa_version_bumped(v3, "h3".into(), "u3".into(), 200).unwrap();
        let handler = ReAcceptHandler::new(store.clone());
        let r = handler.re_accept(tenant, v2, 300);
        let is_mismatch = matches!(r, Err(DpaVersioningError::VersionMismatch { .. }));
        prop_assert!(is_mismatch);
        let r = handler.re_accept(tenant, v3, 300);
        prop_assert!(r.is_ok());
        let t = store.read_tenant(tenant).unwrap().unwrap();
        prop_assert_eq!(t.current_dpa_version, v3);
        prop_assert!(!t.re_acceptance_pending);
    }
}
