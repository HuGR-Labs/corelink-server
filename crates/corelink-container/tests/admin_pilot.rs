//! Integration tests for the pilot-admin HTTP surface (Wave-29 stream-3).
//!
//! Coverage matrix:
//!
//!   1. **Happy path** — admin lists pilots, then grants a pilot tier,
//!      then runs a 24h checkin → 200 + audit row + state transitions.
//!   2. **Non-admin JWT → 403** — caller without the
//!      `corelink:admin:pilots` scope claim is rejected and an
//!      `EVENT_TYPE_UNAUTHORIZED` audit row is emitted BEFORE the 403.
//!   3. **Cross-tenant admin → 403** — admin scoped to tenant A
//!      probing tenant B's grant-tier endpoint is rejected with
//!      `EVENT_TYPE_CROSS_TENANT` audit BEFORE the 403.
//!   4. **Audit fail-CLOSED → 503** — sink unavailable on the
//!      grant-tier route surfaces `503` AND the store mutation does
//!      NOT land.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::print_stderr)]

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use corelink_server::routes::admin_pilot::{
    router, InMemoryPilotAuditSink, InMemoryPilotStore, PilotAdminRouteState, PilotAuditSink,
    PilotState, PilotStore, PilotTenant, ADMIN_PRINCIPAL_HEADER, ADMIN_SCOPE_HEADER,
    ADMIN_TENANT_HEADER, EVENT_TYPE_CHECKIN, EVENT_TYPE_CROSS_TENANT, EVENT_TYPE_TIER_GRANTED,
    EVENT_TYPE_UNAUTHORIZED, REQUIRED_ADMIN_SCOPE,
};
use corelink_server::wall_clock::InMemoryFakeWallClock;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

/// Build a fresh fixture pinned at `now_ms`.
fn fixture(
    now_ms: u64,
) -> (
    PilotAdminRouteState,
    Arc<InMemoryPilotStore>,
    Arc<InMemoryPilotAuditSink>,
    Arc<InMemoryFakeWallClock>,
) {
    let store = Arc::new(InMemoryPilotStore::new());
    let audit = Arc::new(InMemoryPilotAuditSink::new());
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(now_ms));
    let state = PilotAdminRouteState {
        store: store.clone() as Arc<dyn PilotStore>,
        audit_sink: audit.clone() as Arc<dyn PilotAuditSink>,
        wall_clock: clock.clone() as Arc<dyn corelink_server::wall_clock::WallClock>,
    };
    (state, store, audit, clock)
}

fn seed_tenant(
    store: &InMemoryPilotStore,
    state: PilotState,
    slug: &str,
    signup_at_ms: u64,
    granted_at_ms: Option<u64>,
) -> Uuid {
    let id = Uuid::now_v7();
    store
        .seed(PilotTenant {
            tenant_id: id,
            slug: slug.to_string(),
            tier: if granted_at_ms.is_some() {
                "pilot".into()
            } else {
                "free".into()
            },
            cap_bytes: if granted_at_ms.is_some() {
                100_000_000_000
            } else {
                0
            },
            pilot_state: state,
            signup_at_ms,
            tier_granted_at_ms: granted_at_ms,
            first_blob_at_ms: None,
        })
        .expect("seed");
    id
}

fn admin_headers(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder
        .header(ADMIN_SCOPE_HEADER, REQUIRED_ADMIN_SCOPE)
        .header(ADMIN_PRINCIPAL_HEADER, "ops@root")
}

/// 1. Happy path — admin lists NEW pilots and observes the
///    canonical JSON shape + audit row emit.
#[tokio::test]
async fn list_pilots_happy_path_returns_rows_and_emits_audit() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    seed_tenant(&store, PilotState::New, "tenant-a", 1_000, None);
    seed_tenant(&store, PilotState::Active, "tenant-b", 2_000, Some(1_000));
    seed_tenant(&store, PilotState::New, "tenant-c", 3_000, None);

    let app = router(state);
    let req = admin_headers(Request::builder().uri("/v1/admin/pilots?state=NEW"))
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["state"], "NEW");
    let rows = json["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2);
    // ordered by signup_at_ms ASC
    assert_eq!(rows[0]["slug"], "tenant-a");
    assert_eq!(rows[1]["slug"], "tenant-c");

    let emitted = audit.snapshot().expect("audit");
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].event_type, "corelink.admin.pilot_list.v1");
    assert_eq!(emitted[0].exit_status, "ok");
}

/// 1b. Happy path — admin grants pilot tier.
#[tokio::test]
async fn grant_tier_happy_path_transitions_state_and_emits_attempt_plus_ok() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    let id = seed_tenant(&store, PilotState::New, "acme-builds", 1_000, None);

    let app = router(state);
    let body = serde_json::json!({"tier": "pilot", "cap_bytes": 100_000_000_000_u64});
    let req = admin_headers(
        Request::builder()
            .method("POST")
            .uri(format!("/v1/admin/pilots/{id}/grant-tier"))
            .header("content-type", "application/json"),
    )
    .body(Body::from(body.to_string()))
    .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["tenant"]["pilot_state"], "ACTIVE");
    assert_eq!(json["tenant"]["tier"], "pilot");
    assert_eq!(json["tenant"]["cap_bytes"], 100_000_000_000_u64);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].event_type, EVENT_TYPE_TIER_GRANTED);
    assert_eq!(snap[0].exit_status, "attempt");
    assert_eq!(snap[1].exit_status, "ok");

    // Store reflects the mutation.
    let after = store.get(id).expect("get").expect("present");
    assert_eq!(after.pilot_state, PilotState::Active);
    assert_eq!(after.tier_granted_at_ms, Some(1_700_000_000_000));
}

/// 1c. Happy path — 24h checkin emits alert for ACTIVE tenant with
///     no first blob 25h after grant.
#[tokio::test]
async fn checkin_emits_alert_for_overdue_no_blob_tenant() {
    let grant_at: u64 = 1_700_000_000_000;
    let now: u64 = grant_at + 25 * 3_600_000; // +25h
    let (state, store, audit, _clock) = fixture(now);
    let id = seed_tenant(
        &store,
        PilotState::Active,
        "silent-tenant",
        1_000,
        Some(grant_at),
    );

    let app = router(state);
    let req = admin_headers(
        Request::builder()
            .method("POST")
            .uri(format!("/v1/admin/pilots/{id}/checkin")),
    )
    .body(Body::empty())
    .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["alert_emitted"], true);
    assert_eq!(json["tenant_id"], id.to_string());

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].event_type, EVENT_TYPE_CHECKIN);
    assert_eq!(snap[0].exit_status, "alert");
    let _ = store; // keep alive
}

/// 2. Non-admin JWT → 403 + unauthorized audit row BEFORE the
///    response.
#[tokio::test]
async fn non_admin_jwt_rejected_with_audit_before_403() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);

    let app = router(state);
    // Missing X-Admin-Scope entirely.
    let req = Request::builder()
        .uri("/v1/admin/pilots?state=NEW")
        .header(ADMIN_PRINCIPAL_HEADER, "alice@tenant")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1, "audit row MUST be emitted BEFORE the 403");
    assert_eq!(snap[0].event_type, EVENT_TYPE_UNAUTHORIZED);
    assert_eq!(snap[0].principal, "alice@tenant");
    assert_eq!(snap[0].exit_status, "forbidden");
}

/// 2b. Non-admin JWT (no principal) → 403 with empty-principal row.
#[tokio::test]
async fn missing_principal_rejected_with_audit() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/admin/pilots?state=NEW")
        .header(ADMIN_SCOPE_HEADER, REQUIRED_ADMIN_SCOPE)
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].event_type, EVENT_TYPE_UNAUTHORIZED);
}

/// 3. Cross-tenant attempt — admin bound to tenant A attempts to
///    grant tier on tenant B → 403 + cross-tenant audit BEFORE the
///    response. The store mutation MUST NOT land.
#[tokio::test]
async fn cross_tenant_grant_tier_rejected_with_audit_before_403() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    let tenant_a = Uuid::now_v7();
    let tenant_b = seed_tenant(&store, PilotState::New, "victim", 1_000, None);

    let app = router(state);
    let body = serde_json::json!({"tier": "pilot", "cap_bytes": 100_000_000_000_u64});
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/admin/pilots/{tenant_b}/grant-tier"))
        .header("content-type", "application/json")
        .header(ADMIN_SCOPE_HEADER, REQUIRED_ADMIN_SCOPE)
        .header(ADMIN_PRINCIPAL_HEADER, "tenant-a-admin")
        .header(ADMIN_TENANT_HEADER, tenant_a.to_string())
        .body(Body::from(body.to_string()))
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].event_type, EVENT_TYPE_CROSS_TENANT);
    assert_eq!(snap[0].tenant_id, Some(tenant_b));
    assert_eq!(snap[0].principal, "tenant-a-admin");

    // Store mutation MUST NOT have landed.
    let after = store.get(tenant_b).expect("get").expect("present");
    assert_eq!(after.pilot_state, PilotState::New, "no state change");
    assert_eq!(after.tier_granted_at_ms, None);
}

/// 3b. Same-tenant admin → grant-tier succeeds (control case for
///     the cross-tenant rejection above).
#[tokio::test]
async fn same_tenant_admin_grant_tier_succeeds() {
    let (state, store, _audit, _clock) = fixture(1_700_000_000_000);
    let tenant = seed_tenant(&store, PilotState::New, "self-admin", 1_000, None);

    let app = router(state);
    let body = serde_json::json!({"tier": "pilot", "cap_bytes": 100_000_000_000_u64});
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/admin/pilots/{tenant}/grant-tier"))
        .header("content-type", "application/json")
        .header(ADMIN_SCOPE_HEADER, REQUIRED_ADMIN_SCOPE)
        .header(ADMIN_PRINCIPAL_HEADER, "self-admin")
        .header(ADMIN_TENANT_HEADER, tenant.to_string())
        .body(Body::from(body.to_string()))
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
}

/// 4. Audit fail-CLOSED → 503. The grant-tier route MUST surface
///    `503` and the store mutation MUST NOT land.
#[tokio::test]
async fn audit_fail_closed_aborts_grant_tier_with_503() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    let id = seed_tenant(&store, PilotState::New, "audit-down", 1_000, None);
    audit.inject_failure("audit pipeline down").expect("inject");

    let app = router(state);
    let body = serde_json::json!({"tier": "pilot", "cap_bytes": 100_000_000_000_u64});
    let req = admin_headers(
        Request::builder()
            .method("POST")
            .uri(format!("/v1/admin/pilots/{id}/grant-tier"))
            .header("content-type", "application/json"),
    )
    .body(Body::from(body.to_string()))
    .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Mutation MUST NOT have landed.
    let after = store.get(id).expect("get").expect("present");
    assert_eq!(after.pilot_state, PilotState::New);
    assert_eq!(after.tier_granted_at_ms, None);
}

/// 4b. Audit fail-CLOSED → 503 on list route as well.
#[tokio::test]
async fn audit_fail_closed_aborts_list_with_503() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);
    audit.inject_failure("audit pipeline down").expect("inject");

    let app = router(state);
    let req = admin_headers(Request::builder().uri("/v1/admin/pilots?state=NEW"))
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// Edge: invalid state filter → 400 (input validation L4).
#[tokio::test]
async fn invalid_state_filter_400() {
    let (state, _store, _audit, _clock) = fixture(1_700_000_000_000);
    let app = router(state);
    let req = admin_headers(Request::builder().uri("/v1/admin/pilots?state=BOGUS"))
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Edge: invalid tenant_id capture → 400.
#[tokio::test]
async fn invalid_tenant_uuid_400() {
    let (state, _store, _audit, _clock) = fixture(1_700_000_000_000);
    let app = router(state);
    let req = admin_headers(
        Request::builder()
            .method("POST")
            .uri("/v1/admin/pilots/not-a-uuid/checkin"),
    )
    .body(Body::empty())
    .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Edge: grant-tier on already-ACTIVE tenant → 409 conflict.
#[tokio::test]
async fn double_grant_tier_returns_conflict() {
    let (state, store, _audit, _clock) = fixture(1_700_000_000_000);
    let id = seed_tenant(
        &store,
        PilotState::Active,
        "already-active",
        1_000,
        Some(1_000),
    );

    let app = router(state);
    let body = serde_json::json!({"tier": "pilot", "cap_bytes": 100_000_000_000_u64});
    let req = admin_headers(
        Request::builder()
            .method("POST")
            .uri(format!("/v1/admin/pilots/{id}/grant-tier"))
            .header("content-type", "application/json"),
    )
    .body(Body::from(body.to_string()))
    .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}
