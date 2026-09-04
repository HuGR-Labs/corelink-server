use super::*;

// ── Route table smoke tests ───────────────────────────────────────────────

#[test]
fn build_handlers_returns_usable_state() {
    // Smoke: build_handlers() constructs without panic.
    let state = build_handlers();
    let _router = router(state);
}

#[test]
fn router_constructs_from_fixture() {
    let (state, _) = fixture();
    let _r = router(state);
}

// ── Tenant resolution ─────────────────────────────────────────────────────

#[tokio::test]
async fn overview_reads_tenant_from_header() {
    let (state, shared) = fixture();
    // Seed an overview for tenant-xyz so the handler returns OK.
    let overview = OverviewResponse::new(
        "tenant-xyz",
        "XYZ Corp",
        "starter",
        OverviewUsage::new("2026-05", 0, 1_000_000, 0, 0),
        OverviewBilling::new("active", "2026-06-01T00:00:00Z", 2900, "usd"),
        ByokStatus::new("none", None, None),
        vec![],
    );
    shared.seed_overview("tenant-xyz", overview).expect("seed");

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/overview")
        .method("GET")
        // F-018 (H17): the overview carries billing/PII — requires the billing
        // capability. Send the Worker's real non-viewer dashboard scope.
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "tenant-xyz")
        .header("x-corelink-token-prefix", "clpat_abc")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["tenant_id"], "tenant-xyz");
    assert_eq!(v["tenant_name"], "XYZ Corp");
    assert_eq!(v["plan"], "starter");
}

#[tokio::test]
async fn overview_unknown_tenant_returns_404() {
    // With no seed the InMemory handler returns NotFound -> 404.
    let (state, _) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/overview")
        .method("GET")
        // F-018 (H17): billing/PII surface — pass the billing-capable dashboard
        // scope so the request reaches the handler (asserting 404, not the 403).
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "ghost")
        .header("x-corelink-token-prefix", "clpat_x")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn missing_tenant_header_is_401_fail_closed() {
    // F-defense: a missing tenant header must FAIL-CLOSED with 401 — NOT
    // fall back to the `"_unknown"` sentinel (the old behavior, which let
    // an unauthenticated request reach the handler under a sentinel
    // tenant). The 401 fires BEFORE any handler/storage access.
    let (state, _) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/overview")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sentinel_tenant_header_is_401_fail_closed() {
    // A sentinel value in the header is not a real authenticated tenant
    // and must be rejected 401, same as a missing header.
    let (state, _) = fixture();
    let app = router(state);
    for sentinel in ["_unknown", "_anonymous", "_system", "_pending", "   "] {
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            .header("x-corelink-tenant-id", sentinel)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "sentinel {sentinel:?} must fail closed"
        );
    }
}

// ── Usage route ───────────────────────────────────────────────────────────

#[tokio::test]
async fn usage_route_returns_200_with_period() {
    let (state, shared) = fixture();
    let usage = UsageResponse::new(
        "2026-05",
        512,
        100,
        50,
        1_000_000,
        vec![],
        200,
        Some(0.75),
        900,
        1,
    );
    shared.seed_usage("t1", usage).expect("seed");

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/usage?period=2026-05")
        .method("GET")
        .header("x-corelink-tenant-id", "t1")
        .header("x-corelink-token-prefix", "clpat_t1")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["period"], "2026-05");
    assert_eq!(v["reads"], 100u64);
    assert_eq!(v["writes"], 50u64);
    assert_eq!(v["request_count"], 200u64);
    assert_eq!(v["hit_rate"], 0.75);
    assert_eq!(v["time_saved_seconds"], 900u64);
    assert_eq!(v["dollars_saved_cents"], 1u64);
}

// ── Billing routes ────────────────────────────────────────────────────────

#[tokio::test]
async fn billing_route_returns_200() {
    let (state, shared) = fixture();
    let billing = BillingResponse::new(
        "active",
        "starter",
        "2026-05-01",
        "2026-06-01",
        2900,
        "usd",
        vec![InvoiceRow::new(
            "inv_001",
            "2026-05-01",
            2900,
            "paid",
            "https://invoice.stripe.com/inv_001",
        )],
    );
    shared.seed_billing("t2", billing).expect("seed");

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/billing")
        .method("GET")
        // F-018 (H17): billing detail is financial PII — requires the billing
        // capability. The Worker forwards `read-write billing` for a non-viewer
        // dashboard session; a bare cache scope is denied (see H17 tests below).
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "t2")
        .header("x-corelink-token-prefix", "clpat_t2")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["status"], "active");
    assert_eq!(v["invoices"].as_array().expect("invoices").len(), 1);
}

#[tokio::test]
async fn billing_portal_returns_portal_url() {
    let (state, shared) = fixture();
    // Billing portal only needs billing seeded for happy path on the
    // real billing handler — but portal_url in InMemory doesn't look
    // up the billing store; it always returns a stub URL. However we
    // still seed billing to avoid any future path divergence.
    let billing = BillingResponse::new(
        "active",
        "team",
        "2026-05-01",
        "2026-06-01",
        4900,
        "usd",
        vec![],
    );
    shared.seed_billing("t3", billing).expect("seed");

    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/billing/portal")
        .method("POST")
        // F-018 (H17): opening the Stripe billing portal (cancel subscription /
        // manage payment methods) requires the billing capability — a bare
        // cache PAT must NOT reach it. Send the dashboard `read-write billing`
        // scope (what the Worker forwards for a non-viewer session).
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "t3")
        .header("x-corelink-token-prefix", "clpat_t3")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let url = v["portal_url"].as_str().expect("portal_url string");
    assert!(url.starts_with("https://billing.stripe.com/"));
}

// ── F-018: billing / account-PII scope gate ───────────────────────────────
//
// A read-only (`cas:r`) cache PAT must NOT open the Stripe billing portal
// (cancel subscription / manage payment methods) nor read billing / account /
// audit financial-PII. Before this gate the route's own happy-path test sent
// NO scope header and asserted 200 — proving a non-write principal succeeded.
// These tests assert the gate fires (403) for a read-only caller on EVERY
// billing/PII surface, and that a write-capable caller still passes.

/// A read-only caller MUST NOT open the Stripe billing portal (403).
#[tokio::test]
async fn read_only_caller_cannot_open_billing_portal() {
    let (state, _shared) = fixture(); // None gate isolates the scope check.
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/billing/portal")
        .method("POST")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r") // read-only caller
        .header("x-corelink-token-prefix", "clpat_ro")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// H17 regression: a leaked cache-WRITE PAT (`read-write` / `cas:rw` — what a
/// CI / `corelink put` job carries) MUST NOT open the Stripe billing portal
/// nor read billing detail. Before the fix the gate used `requires_cache_write`,
/// so `read-write` PASSED (403 fails-before / passes-after). The billing
/// capability (`read-write billing`, the Worker-forwarded dashboard scope) and
/// owner-grade `admin` still pass.
#[tokio::test]
async fn cache_write_pat_cannot_reach_billing_but_billing_capability_can() {
    for cache in ["read-write", "cas:rw", "cas:w"] {
        let (state, _shared) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            .header("x-corelink-tenant-id", "cw-tenant")
            .header(crate::scope::SCOPE_HEADER, cache)
            .header("x-corelink-token-prefix", "clpat_cw")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "cache scope {cache:?} must NOT open the billing portal (H17)",
        );
    }
    // The dashboard billing capability + an owner-grade admin token DO pass.
    for ok in ["read-write billing", "admin", "owner"] {
        let (state, shared) = fixture();
        let billing = BillingResponse::new(
            "active",
            "team",
            "2026-05-01",
            "2026-06-01",
            4900,
            "usd",
            vec![],
        );
        shared.seed_billing("cw-ok", billing).expect("seed");
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            .header("x-corelink-tenant-id", "cw-ok")
            .header(crate::scope::SCOPE_HEADER, ok)
            .header("x-corelink-token-prefix", "clpat_ok")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "billing-capable scope {ok:?} must open the billing portal",
        );
    }
}

/// A read-only caller MUST NOT read the account overview (billing/PII) (403).
#[tokio::test]
async fn read_only_caller_cannot_read_overview() {
    let (state, _shared) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/overview")
        .method("GET")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header("x-corelink-token-prefix", "clpat_ro")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// A read-only caller MUST NOT read billing detail (financial PII) (403).
#[tokio::test]
async fn read_only_caller_cannot_read_billing() {
    let (state, _shared) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/billing")
        .method("GET")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header("x-corelink-token-prefix", "clpat_ro")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// A read-only caller MUST NOT read the audit log (account PII) (403).
#[tokio::test]
async fn read_only_caller_cannot_read_audit() {
    let (state, _shared) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/audit")
        .method("GET")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header("x-corelink-token-prefix", "clpat_ro")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// A missing scope header (fail-CLOSED) ALSO denies the billing portal: a
/// bare cache PAT with no scope must not reach the money/PII surface.
#[tokio::test]
async fn missing_scope_cannot_open_billing_portal() {
    let (state, _shared) = fixture();
    let app = router(state);
    let req = Request::builder()
        .uri("/v1/customer/billing/portal")
        .method("POST")
        .header("x-corelink-tenant-id", "ro-tenant")
        .header("x-corelink-token-prefix", "clpat_ro")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// (Removed `write_capable_caller_can_open_billing_portal`: it asserted a
// `cas:rw` cache PAT COULD open the billing portal — exactly the H17 hole now
// closed. The correct inverted behavior — cache scope → 403, billing
// capability / owner-grade → 200 — is covered by
// `cache_write_pat_cannot_reach_billing_but_billing_capability_can` above.)
