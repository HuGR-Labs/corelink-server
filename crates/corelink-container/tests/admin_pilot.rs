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
    PilotState, PilotStore, PilotTenant, ADMIN_INTERNAL_AUTH_HEADER, ADMIN_PRINCIPAL_HEADER,
    ADMIN_SCOPE_HEADER, ADMIN_TENANT_HEADER, EVENT_TYPE_CHECKIN, EVENT_TYPE_CROSS_TENANT,
    EVENT_TYPE_TIER_GRANTED, EVENT_TYPE_UNAUTHORIZED, INTERNAL_EDGE_PRINCIPAL,
    REQUIRED_ADMIN_SCOPE, ROUTE_KIND_HEADER, ROUTE_KIND_INTERNAL,
};

/// Operator shared secret for these integration tests. The pilot-admin
/// control plane is now operator-only (gated behind
/// `CORELINK_INTERNAL_AUTH_KEY`, mirroring `/_internal/pat/mint`): every
/// admin request carries this secret in the `x-corelink-internal-auth`
/// header to clear the PRIMARY gate. The previous SOLE gate
/// (`x-admin-scope`) is now only a secondary defence-in-depth label.
const TEST_INTERNAL_AUTH_KEY: &str = "test-internal-auth-key-32-bytes-x";
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
        internal_auth_key: Some(Arc::from(TEST_INTERNAL_AUTH_KEY)),
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
        // PRIMARY operator gate (constant-time shared secret).
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
        // Secondary defence-in-depth label (no longer the sole gate).
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

/// 2. Tenant PAT (no operator secret) → 403 + unauthorized audit row
///    BEFORE the response. This is the SECURITY-CRITICAL case: even a
///    caller that forges the (now-secondary) `x-admin-scope` +
///    `x-admin-principal` headers is rejected because it cannot supply
///    the operator shared secret. (Previously the scope header was the
///    sole gate and this request would have been ADMITTED.)
#[tokio::test]
async fn tenant_pat_without_operator_secret_rejected_with_audit_before_403() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);

    let app = router(state);
    // Forged scope + principal, but NO x-corelink-internal-auth secret.
    let req = Request::builder()
        .uri("/v1/admin/pilots?state=NEW")
        .header(ADMIN_SCOPE_HEADER, REQUIRED_ADMIN_SCOPE)
        .header(ADMIN_PRINCIPAL_HEADER, "alice@tenant")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1, "audit row MUST be emitted BEFORE the 403");
    assert_eq!(snap[0].event_type, EVENT_TYPE_UNAUTHORIZED);
    assert_eq!(snap[0].exit_status, "forbidden");
}

/// 2b. Wrong operator secret → 403 with unauthorized audit row. Pins
///     that an INCORRECT secret value is rejected (constant-time
///     compare), not just an absent one.
#[tokio::test]
async fn wrong_operator_secret_rejected_with_audit() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/admin/pilots?state=NEW")
        .header(ADMIN_INTERNAL_AUTH_HEADER, "wrong-secret-value")
        .header(ADMIN_SCOPE_HEADER, REQUIRED_ADMIN_SCOPE)
        .header(ADMIN_PRINCIPAL_HEADER, "ops@root")
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
        // Operator gate cleared; the cross-tenant check (L3) is what
        // rejects this request.
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
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
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
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

// ---------------------------------------------------------------------------
// `/_internal/admin/pilots…` alias + internal-edge identity synthesis
// (#218 §2.1-§2.2, ratified Q3 2026-06-10)
// ---------------------------------------------------------------------------

/// Headers as the Worker's `/_internal/*` channel delivers them: the
/// client-suppliable trust headers (`x-admin-principal`,
/// `x-admin-scope`, `x-admin-tenant`) are STRIPPED, and the Worker
/// re-injects internal-auth + the server-set
/// `x-corelink-route-kind: internal`.
fn internal_edge_headers(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
        .header(ROUTE_KIND_HEADER, ROUTE_KIND_INTERNAL)
}

/// 5. Alias happy path — `GET /_internal/admin/pilots` with the
///    Worker-shaped header set (internal-auth + route-kind internal,
///    NO principal/scope) → 200 and the audit row carries the
///    synthesized `internal-edge-operator` principal.
#[tokio::test]
async fn internal_alias_list_synthesizes_edge_operator_principal() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    seed_tenant(&store, PilotState::New, "tenant-a", 1_000, None);

    let app = router(state);
    let req = internal_edge_headers(Request::builder().uri("/_internal/admin/pilots?state=NEW"))
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["rows"].as_array().expect("rows").len(), 1);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].event_type, "corelink.admin.pilot_list.v1");
    assert_eq!(
        snap[0].principal, INTERNAL_EDGE_PRINCIPAL,
        "audit row carries the synthesized principal"
    );
}

/// 5b. Alias grant-tier end-to-end under the synthesized identity —
///     both the `attempt` and `ok` audit rows carry
///     `internal-edge-operator` and the mutation lands.
#[tokio::test]
async fn internal_alias_grant_tier_succeeds_with_synthesized_principal() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    let id = seed_tenant(&store, PilotState::New, "edge-pilot", 1_000, None);

    let app = router(state);
    let body = serde_json::json!({"tier": "pilot", "cap_bytes": 100_000_000_000_u64});
    let req = internal_edge_headers(
        Request::builder()
            .method("POST")
            .uri(format!("/_internal/admin/pilots/{id}/grant-tier"))
            .header("content-type", "application/json"),
    )
    .body(Body::from(body.to_string()))
    .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].event_type, EVENT_TYPE_TIER_GRANTED);
    assert_eq!(snap[0].exit_status, "attempt");
    assert_eq!(snap[0].principal, INTERNAL_EDGE_PRINCIPAL);
    assert_eq!(snap[1].exit_status, "ok");
    assert_eq!(snap[1].principal, INTERNAL_EDGE_PRINCIPAL);

    let after = store.get(id).expect("get").expect("present");
    assert_eq!(after.pilot_state, PilotState::Active);
}

/// 5c. Alias WITHOUT internal-auth → 403 and the unauthorized audit
///     row is emitted BEFORE the response. The alias never weakens
///     the PRIMARY gate: a forged `x-corelink-route-kind: internal`
///     alone admits nothing.
#[tokio::test]
async fn internal_alias_without_internal_auth_rejected_with_audit_before_403() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);

    let app = router(state);
    let req = Request::builder()
        .uri("/_internal/admin/pilots?state=NEW")
        .header(ROUTE_KIND_HEADER, ROUTE_KIND_INTERNAL)
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1, "audit row MUST be emitted BEFORE the 403");
    assert_eq!(snap[0].event_type, EVENT_TYPE_UNAUTHORIZED);
    assert_eq!(snap[0].exit_status, "forbidden");
}

/// 5d. Internal-auth OK but empty principal + route-kind NOT internal
///     (e.g. a PAT-routed forward) → 403 unchanged. The synthesis is
///     strictly conditioned on the server-set internal route-kind.
#[tokio::test]
async fn empty_principal_with_non_internal_route_kind_still_403() {
    let (state, _store, audit, _clock) = fixture(1_700_000_000_000);

    let app = router(state);
    let req = Request::builder()
        .uri("/_internal/admin/pilots?state=NEW")
        .header(ADMIN_INTERNAL_AUTH_HEADER, TEST_INTERNAL_AUTH_KEY)
        .header(ROUTE_KIND_HEADER, "reapi_v1")
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].event_type, EVENT_TYPE_UNAUTHORIZED);
}

/// 5e. Explicit-header regression pin — the canonical path with the
///     full explicit header set behaves byte-identically with the
///     alias mounted: explicit principal is preserved in audit rows
///     (NOT replaced by the synthesized identity), including when the
///     request also carries `route-kind: internal`.
#[tokio::test]
async fn explicit_headers_keep_identity_on_alias_path() {
    let (state, store, audit, _clock) = fixture(1_700_000_000_000);
    seed_tenant(&store, PilotState::New, "tenant-a", 1_000, None);

    let app = router(state);
    let req = admin_headers(Request::builder().uri("/_internal/admin/pilots?state=NEW"))
        .header(ROUTE_KIND_HEADER, ROUTE_KIND_INTERNAL)
        .body(Body::empty())
        .expect("build req");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let snap = audit.snapshot().expect("audit");
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].principal, "ops@root", "explicit identity wins");
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
