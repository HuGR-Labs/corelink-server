#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
//! Adversarial regression tests for the config admin API (WI-S13-001).
//!
//! # Coverage
//!
//! 1. Schema field injection (unknown field) → 400 SchemaInvalid.
//! 2. CAS version conflict (concurrent callers same expected_version) → 409.
//! 3. Rollback without dual-approver header → 403 DualApprovalMissing.
//! 4. MFA stale (>30 min) → 401 MfaStale.
//! 5. Rollback to unknown version → 404 VersionUnknown.
//! 6. Rollback to expired version (>90d) → 410 VersionExpired.
//! 7. Audit chain break (FailingAuditSink) → update aborted, state unchanged.
//! 8. Non-admin caller → 403 NotAdmin.

use std::sync::Arc;
use uuid::Uuid;

use corelink_ops::config::api::{
    ApiError,
    handlers::{AdminContext, PutConfigRequest, handle_put, handle_rollback},
};
use corelink_config_do::{
    AdminActor, ConfigError, ConfigPayload,
    metrics::NoopMetrics,
    store::{ConfigSingletonStore, FailingAuditSink, InMemoryAuditSink, InMemoryConfigSingletonStore},
};

fn admin_ctx(now_ms: u64) -> AdminContext {
    AdminContext {
        actor: AdminActor { user_id: Uuid::nil(), email_hash: [0u8; 32] },
        mfa_ts_ms: now_ms - 5 * 60 * 1000, // 5 min ago — fresh
        is_admin: true,
        dual_approver_user_id: Some(Uuid::nil()),
    }
}

fn admin_ctx_no_dual(now_ms: u64) -> AdminContext {
    AdminContext {
        dual_approver_user_id: None,
        ..admin_ctx(now_ms)
    }
}

fn non_admin_ctx(now_ms: u64) -> AdminContext {
    AdminContext {
        is_admin: false,
        ..admin_ctx(now_ms)
    }
}

fn stale_mfa_ctx(now_ms: u64) -> AdminContext {
    AdminContext {
        mfa_ts_ms: now_ms - 31 * 60 * 1000, // 31 min ago — stale
        ..admin_ctx(now_ms)
    }
}

fn fresh_store() -> InMemoryConfigSingletonStore {
    InMemoryConfigSingletonStore::new(
        Arc::new(InMemoryAuditSink::new()),
        Arc::new(NoopMetrics),
    )
}

// ── 1. Schema field injection (unknown field) ─────────────────────────────────
// serde deny_unknown_fields is enforced at deserialization time; at the API
// layer we simulate the equivalent by sending a payload with an invalid field
// via schema_version mismatch (schema drift). The handler calls validate_payload
// which rejects schema_version != 1.
#[test]
fn adversarial_schema_drift_rejected() {
    let store = fresh_store();
    let now_ms = 10_000_000u64;
    let ctx = admin_ctx(now_ms);

    let mut payload = ConfigPayload::genesis();
    payload.schema_version = 99; // drift

    let req = PutConfigRequest { expected_version: 0, new_payload: payload };
    let result = tokio_test::block_on(handle_put(&store, &ctx, req, now_ms));
    assert!(result.is_err(), "schema drift must be rejected");
    match result {
        Err(ApiError::Config(ConfigError::SchemaInvalid(_))) => {}
        other => panic!("expected SchemaInvalid, got {other:?}"),
    }
    // Status code 400.
    let err = result.expect_err("err");
    assert_eq!(err.status_code(), 400);
}

// ── 2. CAS version conflict ───────────────────────────────────────────────────
#[test]
fn adversarial_cas_conflict() {
    let store = fresh_store();
    let now_ms = 10_000_000u64;
    let ctx = admin_ctx(now_ms);

    // First caller wins.
    let r1 = tokio_test::block_on(handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: 0, new_payload: ConfigPayload::genesis() },
        now_ms,
    ));
    assert!(r1.is_ok(), "first caller must succeed");

    // Second caller uses stale expected_version=0.
    let r2 = tokio_test::block_on(handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: 0, new_payload: ConfigPayload::genesis() },
        now_ms,
    ));
    assert!(r2.is_err(), "second caller with stale version must fail");
    match r2 {
        Err(ApiError::Config(ConfigError::VersionConflict { .. })) => {}
        other => panic!("expected VersionConflict, got {other:?}"),
    }
    let err = r2.expect_err("err");
    assert_eq!(err.status_code(), 409);
}

// ── 3. Rollback without dual-approver ────────────────────────────────────────
#[test]
fn adversarial_rollback_no_dual_approver() {
    let store = fresh_store();
    let now_ms = 10_000_000u64;
    let ctx_no_dual = admin_ctx_no_dual(now_ms);

    let result = tokio_test::block_on(handle_rollback(&store, &ctx_no_dual, 1, now_ms));
    assert!(result.is_err());
    match result {
        Err(ApiError::DualApprovalMissing) => {}
        other => panic!("expected DualApprovalMissing, got {other:?}"),
    }
    let err = result.expect_err("err");
    assert_eq!(err.status_code(), 403);
}

// ── 4. MFA stale ─────────────────────────────────────────────────────────────
#[test]
fn adversarial_mfa_stale() {
    let store = fresh_store();
    let now_ms = 10_000_000u64;
    let ctx = stale_mfa_ctx(now_ms);

    let result = tokio_test::block_on(handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: 0, new_payload: ConfigPayload::genesis() },
        now_ms,
    ));
    assert!(result.is_err());
    match result {
        Err(ApiError::MfaStale { .. }) => {}
        other => panic!("expected MfaStale, got {other:?}"),
    }
    let err = result.expect_err("err");
    assert_eq!(err.status_code(), 401);
}

// ── 5. Rollback to unknown version ───────────────────────────────────────────
#[test]
fn adversarial_rollback_unknown_version() {
    let store = fresh_store();
    let now_ms = 10_000_000u64;
    let ctx = admin_ctx(now_ms);

    let result = tokio_test::block_on(handle_rollback(&store, &ctx, 99_999, now_ms));
    assert!(result.is_err());
    match result {
        Err(ApiError::Config(ConfigError::VersionUnknown(_))) => {}
        other => panic!("expected VersionUnknown, got {other:?}"),
    }
    let err = result.expect_err("err");
    assert_eq!(err.status_code(), 404);
}

// ── 6. Rollback to expired version (>90d) ────────────────────────────────────
#[test]
fn adversarial_rollback_expired_version() {
    let store = fresh_store();
    let base_ms = 1_000_000u64;

    // Create version 1 at base_ms.
    let actor = AdminActor { user_id: Uuid::nil(), email_hash: [0u8; 32] };
    let v1 = tokio_test::block_on(
        store.update(0, ConfigPayload::genesis(), &actor, base_ms)
    )
    .expect("update v1");
    assert_eq!(v1, 1);

    // Attempt rollback 91d later.
    let ninety_one_days_ms: u64 = 91 * 24 * 60 * 60 * 1000;
    let future_now = base_ms + ninety_one_days_ms + 1;

    let ctx = admin_ctx(future_now);
    let result = tokio_test::block_on(handle_rollback(&store, &ctx, 1, future_now));
    assert!(result.is_err());
    match result {
        Err(ApiError::Config(ConfigError::VersionExpired(_))) => {}
        other => panic!("expected VersionExpired, got {other:?}"),
    }
    let err = result.expect_err("err");
    assert_eq!(err.status_code(), 410);
}

// ── 7. Audit chain break (fail-CLOSED) ───────────────────────────────────────
#[test]
fn adversarial_audit_fail_closed() {
    let store = InMemoryConfigSingletonStore::new(
        Arc::new(FailingAuditSink),
        Arc::new(NoopMetrics),
    );
    let now_ms = 10_000_000u64;
    let ctx = admin_ctx(now_ms);

    let result = tokio_test::block_on(handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: 0, new_payload: ConfigPayload::genesis() },
        now_ms,
    ));
    assert!(result.is_err(), "audit failure must abort update");
    assert_eq!(result.expect_err("err").status_code(), 500);

    // State unchanged.
    let (v, _) = tokio_test::block_on(store.current()).expect("current");
    assert_eq!(v, 0, "state must be unchanged after audit failure");
}

// ── 8. Non-admin caller ───────────────────────────────────────────────────────
#[test]
fn adversarial_non_admin_put_rejected() {
    let store = fresh_store();
    let now_ms = 10_000_000u64;
    let ctx = non_admin_ctx(now_ms);

    let result = tokio_test::block_on(handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: 0, new_payload: ConfigPayload::genesis() },
        now_ms,
    ));
    assert!(result.is_err());
    match result {
        Err(ApiError::NotAdmin) => {}
        other => panic!("expected NotAdmin, got {other:?}"),
    }
    let err = result.expect_err("err");
    assert_eq!(err.status_code(), 403);
}
