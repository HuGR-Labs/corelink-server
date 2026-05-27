//! Regression: bump kind classification table + only Major triggers
//! broadcast.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "regression tests are allowed to use these primitives"
)]

use std::sync::Arc;

use uuid::Uuid;

use corelink_privacy::dpa::versioning::broadcast::{BroadcastSink, InMemoryBroadcastSink};
use corelink_privacy::dpa::versioning::error::DpaVersioningError;
use corelink_privacy::dpa::versioning::schema::TenantDpaState;
use corelink_privacy::dpa::versioning::store::{DpaStore, InMemoryDpaStore};
use corelink_privacy::dpa::versioning::version::{
    classify_bump, BumpClassification, BumpKind, SemverVersion,
};
use corelink_privacy::dpa::versioning::versioning::{DpaVersioning, InMemoryDpaVersioning};

#[test]
fn classification_table() {
    // (old, new, expected)
    let cases = [
        ((1, 0, 0), (2, 0, 0), BumpKind::Major),
        ((1, 0, 0), (2, 5, 9), BumpKind::Major),
        ((1, 4, 0), (1, 5, 0), BumpKind::Minor),
        ((1, 4, 7), (1, 5, 0), BumpKind::Minor),
        ((1, 4, 0), (1, 4, 1), BumpKind::Patch),
        ((0, 0, 0), (0, 0, 1), BumpKind::Patch),
    ];
    for ((om, oi, op), (nm, ni, np), expected) in cases {
        let actual = classify_bump(
            SemverVersion::new(om, oi, op),
            SemverVersion::new(nm, ni, np),
        );
        assert_eq!(
            actual,
            BumpClassification::Detected(expected),
            "old=v{}.{}.{} new=v{}.{}.{}",
            om,
            oi,
            op,
            nm,
            ni,
            np
        );
    }
}

#[test]
fn no_bump_detected_on_equal() {
    let v = SemverVersion::new(1, 2, 3);
    assert_eq!(classify_bump(v, v), BumpClassification::NoBump);
}

#[test]
fn regression_detected_on_backward() {
    let old = SemverVersion::new(2, 0, 0);
    let new = SemverVersion::new(1, 99, 99);
    assert_eq!(classify_bump(old, new), BumpClassification::Regression);
}

#[test]
fn minor_bump_does_not_broadcast() {
    let store = Arc::new(InMemoryDpaStore::new());
    let sink = Arc::new(InMemoryBroadcastSink::new());
    let sink_dyn: Arc<dyn BroadcastSink> = sink.clone();
    let orch = InMemoryDpaVersioning::new(store.clone(), sink_dyn);

    orch.on_dpa_version_bumped(SemverVersion::new(1, 0, 0), "h".into(), "u".into(), 0)
        .unwrap();
    store
        .seed_tenant(TenantDpaState::fresh(
            Uuid::from_u128(1),
            SemverVersion::new(1, 0, 0),
        ))
        .unwrap();

    let receipt = orch
        .on_dpa_version_bumped(SemverVersion::new(1, 1, 0), "h2".into(), "u2".into(), 100)
        .unwrap();
    assert_eq!(receipt.tenants_flagged, 0);
    assert!(!receipt.broadcast_dispatched);
    assert!(sink.captured().unwrap().is_empty());

    // And tenant is NOT flagged pending.
    let t = store.read_tenant(Uuid::from_u128(1)).unwrap().unwrap();
    assert!(!t.re_acceptance_pending);
    assert_eq!(t.current_dpa_version, SemverVersion::new(1, 0, 0));
}

#[test]
fn regression_publish_rejected() {
    let store = Arc::new(InMemoryDpaStore::new());
    let sink = Arc::new(InMemoryBroadcastSink::new());
    let sink_dyn: Arc<dyn BroadcastSink> = sink;
    let orch = InMemoryDpaVersioning::new(store, sink_dyn);

    orch.on_dpa_version_bumped(SemverVersion::new(2, 0, 0), "h".into(), "u".into(), 0)
        .unwrap();
    let err = orch
        .on_dpa_version_bumped(SemverVersion::new(1, 9, 9), "h2".into(), "u2".into(), 1)
        .unwrap_err();
    assert!(matches!(err, DpaVersioningError::InvalidBump { .. }));
}
