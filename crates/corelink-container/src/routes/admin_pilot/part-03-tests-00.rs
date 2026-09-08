use super::*;
use crate::wall_clock::InMemoryFakeWallClock;

fn sample_tenant(state: PilotState, signup_at: u64) -> PilotTenant {
    PilotTenant {
        tenant_id: Uuid::now_v7(),
        slug: "acme-builds".to_string(),
        tier: "free".to_string(),
        cap_bytes: 0,
        pilot_state: state,
        signup_at_ms: signup_at,
        tier_granted_at_ms: None,
        first_blob_at_ms: None,
    }
}

fn fixture() -> (
    PilotAdminRouteState,
    Arc<InMemoryPilotStore>,
    Arc<InMemoryPilotAuditSink>,
    Arc<InMemoryFakeWallClock>,
) {
    let store = Arc::new(InMemoryPilotStore::new());
    let audit = Arc::new(InMemoryPilotAuditSink::new());
    let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(1_700_000_000_000));
    let state = PilotAdminRouteState {
        store: store.clone() as Arc<dyn PilotStore>,
        audit_sink: audit.clone() as Arc<dyn PilotAuditSink>,
        wall_clock: clock.clone() as Arc<dyn crate::wall_clock::WallClock>,
        // Configured key so unit tests that exercise the store /
        // scope logic can supply the secret header. Gate behaviour is
        // covered by the dedicated internal_auth_ok tests below.
        internal_auth_key: Some(Arc::from("test-internal-auth-key-32-bytes-x")),
    };
    (state, store, audit, clock)
}

#[test]
fn pilot_state_round_trip_through_canonical_strings() {
    for s in [
        PilotState::New,
        PilotState::Reserved,
        PilotState::Provisioned,
        PilotState::Active,
        PilotState::Graduated,
        PilotState::Terminated,
    ] {
        let txt = s.as_str();
        let parsed = PilotState::parse(txt).expect("round trip");
        assert_eq!(parsed, s);
    }
    assert!(PilotState::parse("BOGUS").is_none());
}

#[test]
fn route_constants_match_canonical_paths() {
    assert_eq!(PILOTS_LIST_ROUTE, "/v1/admin/pilots");
    assert_eq!(
        PILOTS_GRANT_TIER_ROUTE,
        "/v1/admin/pilots/{tenant_id}/grant-tier"
    );
    assert_eq!(PILOTS_CHECKIN_ROUTE, "/v1/admin/pilots/{tenant_id}/checkin");
    // #218 §2.1 aliases — the same handlers behind the Worker's
    // `/_internal/*` forwarding channel.
    assert_eq!(INTERNAL_PILOTS_LIST_ROUTE, "/_internal/admin/pilots");
    assert_eq!(
        INTERNAL_PILOTS_GRANT_TIER_ROUTE,
        "/_internal/admin/pilots/{tenant_id}/grant-tier"
    );
    assert_eq!(
        INTERNAL_PILOTS_CHECKIN_ROUTE,
        "/_internal/admin/pilots/{tenant_id}/checkin"
    );
}

#[test]
fn in_memory_store_list_filters_by_state_and_orders_by_signup() {
    let (_st, store, _au, _c) = fixture();
    let mut a = sample_tenant(PilotState::New, 1_000);
    let mut b = sample_tenant(PilotState::Active, 2_000);
    let mut c = sample_tenant(PilotState::New, 500);
    a.slug = "a".into();
    b.slug = "b".into();
    c.slug = "c".into();
    store.seed(a.clone()).expect("seed");
    store.seed(b.clone()).expect("seed");
    store.seed(c.clone()).expect("seed");
    let rows = store.list_by_state(PilotState::New, 0, 50).expect("list");
    assert_eq!(rows.len(), 2);
    // Sorted by signup_at_ms ASC.
    assert_eq!(rows[0].slug, "c");
    assert_eq!(rows[1].slug, "a");
}

#[test]
fn in_memory_store_grant_tier_transitions_new_to_active() {
    let (_st, store, _au, _c) = fixture();
    let t = sample_tenant(PilotState::New, 1_000);
    let id = t.tenant_id;
    store.seed(t).expect("seed");
    let updated = store
        .apply_grant_tier(id, "pilot", 100_000_000_000, 5_000)
        .expect("grant");
    assert_eq!(updated.pilot_state, PilotState::Active);
    assert_eq!(updated.tier, "pilot");
    assert_eq!(updated.cap_bytes, 100_000_000_000);
    assert_eq!(updated.tier_granted_at_ms, Some(5_000));
}

#[test]
fn in_memory_store_grant_tier_rejects_already_active() {
    let (_st, store, _au, _c) = fixture();
    let t = sample_tenant(PilotState::Active, 1_000);
    let id = t.tenant_id;
    store.seed(t).expect("seed");
    let err = store
        .apply_grant_tier(id, "pilot", 1, 5_000)
        .expect_err("double grant");
    assert!(err.contains("not in grant-eligible state"));
}

#[test]
fn admin_scope_allows_tenant_when_global() {
    let scope = PilotAdminScope {
        principal: "ops@root".into(),
        bound_tenant: None,
    };
    assert!(scope.allows_tenant(Uuid::now_v7()));
}

#[test]
fn admin_scope_rejects_cross_tenant_when_bound() {
    let bound = Uuid::now_v7();
    let target = Uuid::now_v7();
    let scope = PilotAdminScope {
        principal: "ops@tenant".into(),
        bound_tenant: Some(bound),
    };
    assert!(scope.allows_tenant(bound));
    assert!(!scope.allows_tenant(target));
}

#[test]
fn audit_sink_inject_failure_surfaces_err() {
    let sink = InMemoryPilotAuditSink::new();
    sink.inject_failure("audit pipeline down").expect("inject");
    let err = sink
        .emit(PilotAuditRow {
            event_type: "x".into(),
            principal: "p".into(),
            tenant_id: None,
            at_unix_ms: 0,
            exit_status: "ok".into(),
            payload: serde_json::Value::Null,
        })
        .expect_err("inject");
    assert_eq!(err, "audit pipeline down");
}

#[test]
fn internal_auth_fails_closed_when_key_unset() {
    // Even with a forged x-corelink-internal-auth header, an unset
    // key means the operator gate fails CLOSED.
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "anything".parse().expect("header"),
    );
    assert!(!internal_auth_ok(None, &headers));
}

#[test]
fn internal_auth_matches_only_exact_secret() {
    let key: Arc<str> = Arc::from("test-internal-auth-key-32-bytes-x");
    let empty = HeaderMap::new();
    assert!(!internal_auth_ok(Some(&key), &empty));
    let mut wrong = HeaderMap::new();
    wrong.insert(ADMIN_INTERNAL_AUTH_HEADER, "wrong".parse().expect("header"));
    assert!(!internal_auth_ok(Some(&key), &wrong));
    let mut right = HeaderMap::new();
    right.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    assert!(internal_auth_ok(Some(&key), &right));
}

/// `require_admin_scope` rejects (403) when the internal-auth header
/// is absent even if a forged `x-admin-scope` is present — proving
/// the scope header is no longer the sole gate.
#[test]
fn require_admin_scope_rejects_without_internal_auth() {
    let (state, _store, audit, _c) = fixture();
    let mut headers = HeaderMap::new();
    // Forged scope + principal — the OLD sole gate. No internal-auth.
    headers.insert(
        ADMIN_SCOPE_HEADER,
        REQUIRED_ADMIN_SCOPE.parse().expect("header"),
    );
    headers.insert(ADMIN_PRINCIPAL_HEADER, "attacker".parse().expect("header"));
    let res = require_admin_scope(&state, &headers, None);
    assert!(res.is_err(), "must reject without internal-auth secret");
    // The unauthorized audit row was emitted BEFORE the 403.
    let rows = audit.snapshot().expect("audit");
    assert!(rows.iter().any(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
}

/// With the correct internal-auth secret AND scope + principal, the
/// gate passes.
#[test]
fn require_admin_scope_passes_with_internal_auth_and_scope() {
    let (state, _store, _audit, _c) = fixture();
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    headers.insert(
        ADMIN_SCOPE_HEADER,
        REQUIRED_ADMIN_SCOPE.parse().expect("header"),
    );
    headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
    let scope = require_admin_scope(&state, &headers, None).expect("authorized");
    assert_eq!(scope.principal, "ops@root");
}

/// #218 §2.2 (ratified Q3): internal-auth + server-set
/// `x-corelink-route-kind: internal` + NO principal/scope headers
/// (the Worker strips them on the `/_internal/*` path) → the gate
/// synthesizes the `internal-edge-operator` identity with a global
/// (unbound) tenant scope.
#[test]
fn require_admin_scope_synthesizes_internal_edge_identity() {
    let (state, _store, audit, _c) = fixture();
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    headers.insert(
        ROUTE_KIND_HEADER,
        ROUTE_KIND_INTERNAL.parse().expect("header"),
    );
    let scope = require_admin_scope(&state, &headers, None).expect("synthesized");
    assert_eq!(scope.principal, INTERNAL_EDGE_PRINCIPAL);
    assert!(scope.bound_tenant.is_none(), "global operator scope");
    // No unauthorized audit row — the gate admitted the request.
    let rows = audit.snapshot().expect("audit");
    assert!(rows.is_empty());
}

/// #218 §2.2: an empty principal WITHOUT the internal route-kind
/// keeps today's 403 (the synthesis never fires for direct /
/// non-internal forwards) and the unauthorized audit row is
/// emitted BEFORE the 403.
#[test]
fn require_admin_scope_rejects_empty_principal_without_internal_route_kind() {
    let (state, _store, audit, _c) = fixture();
    for route_kind in [None, Some("reapi_v1"), Some("onboarding")] {
        let mut headers = HeaderMap::new();
        headers.insert(
            ADMIN_INTERNAL_AUTH_HEADER,
            "test-internal-auth-key-32-bytes-x".parse().expect("header"),
        );
        if let Some(kind) = route_kind {
            headers.insert(ROUTE_KIND_HEADER, kind.parse().expect("header"));
        }
        let res = require_admin_scope(&state, &headers, None);
        assert!(
            res.is_err(),
            "empty principal must 403 (kind={route_kind:?})"
        );
    }
    let rows = audit.snapshot().expect("audit");
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
}

/// #218 §2.2: the synthesis is SECONDARY to the internal-auth gate —
/// `x-corelink-route-kind: internal` alone (no/wrong secret) is
/// still rejected with the unauthorized audit row first. The
/// route-kind header never substitutes for the PRIMARY boundary.
#[test]
fn internal_route_kind_without_internal_auth_still_403() {
    let (state, _store, audit, _c) = fixture();
    let mut headers = HeaderMap::new();
    headers.insert(
        ROUTE_KIND_HEADER,
        ROUTE_KIND_INTERNAL.parse().expect("header"),
    );
    let res = require_admin_scope(&state, &headers, None);
    assert!(res.is_err(), "must reject without the operator secret");
    let rows = audit.snapshot().expect("audit");
    assert!(rows.iter().any(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
}

/// #218 §2.2: an EXPLICIT principal keeps today's behavior
/// byte-identical even when the route-kind is `internal` — the
/// synthesis fires only on an EMPTY principal, so a direct
/// operator call with explicit headers keeps its own identity
/// (and still needs the scope label).
#[test]
fn explicit_principal_with_internal_route_kind_keeps_explicit_identity() {
    let (state, _store, _audit, _c) = fixture();
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    headers.insert(
        ROUTE_KIND_HEADER,
        ROUTE_KIND_INTERNAL.parse().expect("header"),
    );
    headers.insert(
        ADMIN_SCOPE_HEADER,
        REQUIRED_ADMIN_SCOPE.parse().expect("header"),
    );
    headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
    let scope = require_admin_scope(&state, &headers, None).expect("authorized");
    assert_eq!(scope.principal, "ops@root", "explicit identity wins");
}

/// #218 §2.2: explicit principal WITHOUT the scope label is still
/// 403 even under `route-kind: internal` — the explicit-header
/// path is unchanged; only the empty-principal case synthesizes.
#[test]
fn explicit_principal_without_scope_under_internal_route_kind_still_403() {
    let (state, _store, audit, _c) = fixture();
    let mut headers = HeaderMap::new();
    headers.insert(
        ADMIN_INTERNAL_AUTH_HEADER,
        "test-internal-auth-key-32-bytes-x".parse().expect("header"),
    );
    headers.insert(
        ROUTE_KIND_HEADER,
        ROUTE_KIND_INTERNAL.parse().expect("header"),
    );
    headers.insert(ADMIN_PRINCIPAL_HEADER, "ops@root".parse().expect("header"));
    let res = require_admin_scope(&state, &headers, None);
    assert!(
        res.is_err(),
        "explicit principal still requires the scope label"
    );
    let rows = audit.snapshot().expect("audit");
    assert!(rows.iter().any(|r| r.event_type == EVENT_TYPE_UNAUTHORIZED));
}
