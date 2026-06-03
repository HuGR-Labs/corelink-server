//! End-to-end DPA lifecycle integration test.
//!
//! Covers: initial onboarding → Major bump → tenants flagged + emails
//! broadcast → cron reminder → cron degrade → re-accept → degrade
//! lifted.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use std::sync::Arc;

use uuid::Uuid;

use corelink_privacy::dpa::versioning::broadcast::{
    BroadcastKind, BroadcastSink, InMemoryBroadcastSink,
};
use corelink_privacy::dpa::versioning::cron::GraceExpirationCron;
use corelink_privacy::dpa::versioning::error::DpaVersioningError;
use corelink_privacy::dpa::versioning::middleware::{
    GateDecision, HttpMethod, ReadOnlyDegradeGate,
};
use corelink_privacy::dpa::versioning::re_accept::ReAcceptHandler;
use corelink_privacy::dpa::versioning::schema::{
    TenantDpaState, GRACE_PERIOD_SECONDS, REMINDER_WINDOW_SECONDS,
};
use corelink_privacy::dpa::versioning::store::{DpaStore, InMemoryDpaStore};
use corelink_privacy::dpa::versioning::version::SemverVersion;
use corelink_privacy::dpa::versioning::versioning::{DpaVersioning, InMemoryDpaVersioning};

fn fresh_setup() -> (
    Arc<InMemoryDpaStore>,
    Arc<InMemoryBroadcastSink>,
    InMemoryDpaVersioning,
) {
    let store = Arc::new(InMemoryDpaStore::new());
    let sink = Arc::new(InMemoryBroadcastSink::new());
    let sink_dyn: Arc<dyn BroadcastSink> = sink.clone();
    let orch = InMemoryDpaVersioning::new(store.clone(), sink_dyn);
    (store, sink, orch)
}

#[test]
fn full_v1_to_v2_lifecycle() {
    let (store, sink, orch) = fresh_setup();
    let tenant_a = Uuid::from_u128(0xA);
    let tenant_b = Uuid::from_u128(0xB);
    let v1 = SemverVersion::new(1, 0, 0);
    let v2 = SemverVersion::new(2, 0, 0);

    // T+0: publish v1 (initial).
    orch.on_dpa_version_bumped(v1, "hash-v1".into(), "url-v1".into(), 0)
        .expect("v1 publish");
    // Two tenants onboard on v1.
    store
        .seed_tenant(TenantDpaState::fresh(tenant_a, v1))
        .unwrap();
    store
        .seed_tenant(TenantDpaState::fresh(tenant_b, v1))
        .unwrap();

    // T+1day: publish v2 Major.
    let publish_at: i64 = 86_400;
    let receipt = orch
        .on_dpa_version_bumped(v2, "hash-v2".into(), "url-v2".into(), publish_at)
        .expect("v2 publish");
    assert_eq!(receipt.tenants_flagged, 2);
    assert!(receipt.broadcast_dispatched);
    let env = sink.captured().unwrap();
    assert_eq!(env.len(), 2);
    assert!(env.iter().all(|e| e.kind == BroadcastKind::MajorBumpNotice));
    assert!(env.iter().all(|e| e.version == v2));

    // Tenants both have grace_expires_at = publish_at + 30d, both pending.
    let a = store.read_tenant(tenant_a).unwrap().unwrap();
    assert!(a.re_acceptance_pending);
    assert_eq!(a.grace_expires_at, Some(publish_at + GRACE_PERIOD_SECONDS));

    // T+24days: cron pass → both tenants within reminder window.
    let sink_dyn: Arc<dyn BroadcastSink> = sink.clone();
    let cron = GraceExpirationCron::new(store.clone(), sink_dyn);
    let reminder_now = publish_at + GRACE_PERIOD_SECONDS - REMINDER_WINDOW_SECONDS + 60;
    let r = cron.run(reminder_now).unwrap();
    assert_eq!(r.reminded, 2);
    assert_eq!(r.newly_degraded, 0);
    let env = sink.captured().unwrap();
    assert!(env.iter().any(|e| e.kind == BroadcastKind::GraceReminder));

    // Tenant A re-accepts before grace expiry.
    let handler = ReAcceptHandler::new(store.clone());
    let accepted_at = reminder_now + 3600;
    let receipt = handler.re_accept(tenant_a, v2, accepted_at).unwrap();
    assert_eq!(receipt.from_version, v1);
    assert_eq!(receipt.to_version, v2);
    let a = store.read_tenant(tenant_a).unwrap().unwrap();
    assert_eq!(a.current_dpa_version, v2);
    assert!(!a.re_acceptance_pending);
    assert!(a.grace_expires_at.is_none());

    // T+30days+1s: cron pass — A is settled, B degrades.
    let degrade_now = publish_at + GRACE_PERIOD_SECONDS + 1;
    let r = cron.run(degrade_now).unwrap();
    assert_eq!(r.newly_degraded, 1);

    // PAT-DEGRADE-001: B blocked on writes, A unblocked.
    let a = store.read_tenant(tenant_a).unwrap().unwrap();
    let b = store.read_tenant(tenant_b).unwrap().unwrap();
    assert_eq!(
        ReadOnlyDegradeGate::evaluate(&b, HttpMethod::Post, "/v1/objects/x", degrade_now),
        GateDecision::DenyReAcceptanceRequired
    );
    assert_eq!(
        ReadOnlyDegradeGate::evaluate(&b, HttpMethod::Get, "/v1/objects/x", degrade_now),
        GateDecision::Allow
    );
    assert_eq!(
        ReadOnlyDegradeGate::evaluate(&b, HttpMethod::Post, "/v1/dpa/re-accept", degrade_now),
        GateDecision::Allow
    );
    assert_eq!(
        ReadOnlyDegradeGate::evaluate(&a, HttpMethod::Post, "/v1/objects/x", degrade_now),
        GateDecision::Allow
    );

    // B late-accepts → degrade lifted.
    let _ = handler.re_accept(tenant_b, v2, degrade_now + 60).unwrap();
    let b = store.read_tenant(tenant_b).unwrap().unwrap();
    assert!(!b.re_acceptance_pending);
    assert_eq!(
        ReadOnlyDegradeGate::evaluate(&b, HttpMethod::Post, "/v1/objects/x", degrade_now + 60),
        GateDecision::Allow
    );
}

#[test]
fn version_mismatch_rejected_on_re_accept() {
    let (store, _sink, orch) = fresh_setup();
    let tenant = Uuid::from_u128(0xC);
    let v1 = SemverVersion::new(1, 0, 0);
    let v2 = SemverVersion::new(2, 0, 0);
    orch.on_dpa_version_bumped(v1, "h1".into(), "u1".into(), 0)
        .unwrap();
    store
        .seed_tenant(TenantDpaState::fresh(tenant, v1))
        .unwrap();
    orch.on_dpa_version_bumped(v2, "h2".into(), "u2".into(), 86_400)
        .unwrap();

    let handler = ReAcceptHandler::new(store);
    let err = handler.re_accept(tenant, v1, 86_500).unwrap_err();
    assert!(matches!(err, DpaVersioningError::VersionMismatch { .. }));
}

#[test]
fn re_accept_without_pending_is_error() {
    let (store, _sink, orch) = fresh_setup();
    let tenant = Uuid::from_u128(0xD);
    let v1 = SemverVersion::new(1, 0, 0);
    orch.on_dpa_version_bumped(v1, "h1".into(), "u1".into(), 0)
        .unwrap();
    store
        .seed_tenant(TenantDpaState::fresh(tenant, v1))
        .unwrap();
    let handler = ReAcceptHandler::new(store);
    let err = handler.re_accept(tenant, v1, 1).unwrap_err();
    assert!(matches!(err, DpaVersioningError::NoPendingReacceptance));
}
