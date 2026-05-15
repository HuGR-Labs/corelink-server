//! Integration tests for the customer-facing `/v1/audit/export`
//! route (Wave-15.3 wiring of WI-S09-008).
//!
//! Coverage matrix per WI-S09-008 §4 DELIVERABLES + spec §6 AC:
//!
//! 1. **Happy path** — tenant A exports its own audit logs, receives
//!    a 200 + NDJSON stream + inclusion proofs + manifest footer.
//! 2. **Cross-tenant reject** — tenant A's JWT requesting tenant B's
//!    range surfaces `403 Forbidden` + emits the SEV-1 security
//!    audit row `corelink.security.audit_export_cross_tenant_attempt.v1`
//!    BEFORE the response (fail-CLOSED ordering).
//! 3. **Empty range** — tenant A with zero matching events returns
//!    `200 OK` (NOT `404`) + zero-line body + manifest footer.
//! 4. **Chain integrity tamper** — a tampered exported chain surfaces
//!    the `corelink.audit.export_verify_failed.v1` SEV-0 audit row.
//!
//! Auth model: the production `apps/server` slot pulls the
//! authenticated tenant id from a tower middleware that injects
//! the `X-Tenant-Id` header AFTER validating the Clerk JWT. Tests
//! stub the header directly.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::print_stderr)]
#![allow(clippy::indexing_slicing)]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use corelink_audit_chain::{
    AuditEvent, AuditEventKind, AuditExporter, ChainHash, ExportedAuditEvent,
    HashChainBuilder, InMemoryAuditExporter,
};
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimiter,
};
use corelink_server::routes::audit_export::{
    audit_export_rate_limit_config, router, AuditExportRouteState, ExportAuditRow,
    ExportAuditSink, InMemoryExportAuditSink, AUDIT_EXPORT_ROUTE,
    EVENT_TYPE_CROSS_TENANT_ATTEMPT, EVENT_TYPE_EXPORT_REQUEST,
    EVENT_TYPE_VERIFY_FAILED, TENANT_ID_HEADER,
};
use corelink_analytics::Region;
use serde_json::{json, Value};
use tower::ServiceExt; // .oneshot
use uuid::Uuid;

/// Build a fresh fixture: exporter pre-seeded with a per-tenant chain
/// of `count` events at `time_ms = base_time + i`.
fn fixture_with_chain(
    tenant: Uuid,
    count: u64,
    base_time_ms: u64,
) -> (
    AuditExportRouteState,
    Arc<InMemoryAuditExporter>,
    Arc<InMemoryExportAuditSink>,
) {
    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = ChainHash::genesis();
    for i in 0..count {
        let e = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            base_time_ms.saturating_add(i),
            tenant,
            Region::Iad,
            i,
            prev,
            json!({"i": i}),
        );
        prev = builder.append(&e).expect("append");
        exporter.append_event(e).expect("seed");
    }
    let exporter = Arc::new(exporter);
    let audit_sink = Arc::new(InMemoryExportAuditSink::new());
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_export_rate_limit_config(),
    ));
    let state = AuditExportRouteState {
        exporter: exporter.clone() as Arc<dyn AuditExporter>,
        rate_limiter: limiter,
        audit_sink: audit_sink.clone() as Arc<dyn ExportAuditSink>,
    };
    (state, exporter, audit_sink)
}

fn build_request(tenant: Uuid, from_ms: u64, to_ms: u64) -> Request<Body> {
    let uri = format!(
        "{}?from={from_ms}&to={to_ms}",
        AUDIT_EXPORT_ROUTE
    );
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(TENANT_ID_HEADER, tenant.to_string())
        .body(Body::empty())
        .expect("request")
}

fn build_request_with_attempted_tenant(
    auth_tenant: Uuid,
    attempted: Uuid,
    from_ms: u64,
    to_ms: u64,
) -> Request<Body> {
    let uri = format!(
        "{}?from={from_ms}&to={to_ms}&tenant={attempted}",
        AUDIT_EXPORT_ROUTE
    );
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(TENANT_ID_HEADER, auth_tenant.to_string())
        .body(Body::empty())
        .expect("request")
}

async fn read_body(resp: axum::response::Response) -> String {
    let bytes = to_bytes(resp.into_body(), 16 * 1024 * 1024)
        .await
        .expect("body bytes");
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// AC-1: Happy path — tenant exports its own audit logs, receives
/// 200 + NDJSON stream + manifest + audit emit `corelink.audit.export_request.v1`.
#[tokio::test]
async fn happy_path_tenant_exports_own_audit_logs() {
    let tenant = Uuid::now_v7();
    let (state, _exporter, audit_sink) = fixture_with_chain(tenant, 5, 100);
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot");

    assert_eq!(resp.status(), StatusCode::OK, "happy path returns 200");
    // Content-type must pin the NDJSON shape.
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .expect("content-type header")
        .to_str()
        .expect("ascii ct");
    assert_eq!(ct, "application/x-ndjson");
    let anchor = resp
        .headers()
        .get("x-corelink-audit-export-chain-head-anchor")
        .expect("anchor header")
        .to_str()
        .expect("ascii anchor")
        .to_string();
    assert_eq!(anchor.len(), 64, "anchor is 32-byte BLAKE3 hex");
    let body = read_body(resp).await;
    let lines: Vec<&str> = body.split('\n').collect();
    // 5 NDJSON row lines + 1 manifest footer line = 6.
    assert_eq!(lines.len(), 6, "5 rows + 1 manifest line; got body={body}");
    // Each row parses as `{event, proof}`.
    for line in &lines[..5] {
        let v: Value = serde_json::from_str(line).expect("ndjson");
        assert!(v.get("event").is_some(), "row carries event: {line}");
        assert!(v.get("proof").is_some(), "row carries proof: {line}");
    }
    // The final line is the manifest envelope.
    let footer: Value = serde_json::from_str(lines[5]).expect("manifest");
    let manifest = footer.get("manifest").expect("manifest key");
    assert_eq!(manifest.get("event_count").and_then(Value::as_u64), Some(5));

    // Audit emit captured: `corelink.audit.export_request.v1` with
    // tenant + from + to + byte count + exit_status="ok".
    let captured = audit_sink.snapshot().expect("audit snapshot");
    assert_eq!(captured.len(), 1, "exactly one audit row emitted");
    let row = &captured[0];
    assert_eq!(row.event_type, EVENT_TYPE_EXPORT_REQUEST);
    assert_eq!(row.authenticated_tenant, Some(tenant));
    assert_eq!(row.from_ms, 0);
    assert_eq!(row.to_ms, 1_000);
    assert_eq!(row.events_written, 5);
    assert!(row.bytes_written > 0, "byte count populated");
    assert_eq!(row.exit_status, "ok");
}

/// AC-2: Cross-tenant reject — tenant A's JWT requesting tenant B's
/// range surfaces 403 + audit emit `corelink.security.audit_export_cross_tenant_attempt.v1`.
#[tokio::test]
async fn cross_tenant_attempt_emits_security_audit_and_403() {
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    // Fixture chain belongs to tenant_a (tenant_b never appears).
    let (state, _exporter, audit_sink) = fixture_with_chain(tenant_a, 3, 100);
    let app = router(state);

    // Tenant A's JWT requesting tenant B's range.
    let resp = app
        .oneshot(build_request_with_attempted_tenant(
            tenant_a, tenant_b, 0, 1_000,
        ))
        .await
        .expect("oneshot");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "cross-tenant attempt rejected with 403"
    );

    let captured = audit_sink.snapshot().expect("audit snapshot");
    assert_eq!(captured.len(), 1, "security audit row emitted");
    let row = &captured[0];
    assert_eq!(row.event_type, EVENT_TYPE_CROSS_TENANT_ATTEMPT);
    assert_eq!(row.authenticated_tenant, Some(tenant_a));
    assert_eq!(row.attempted_tenant, Some(tenant_b));
    assert_eq!(row.exit_status, "cross_tenant_reject");
    assert_eq!(row.bytes_written, 0, "no bytes emitted on reject");
    assert_eq!(row.events_written, 0, "no events emitted on reject");
}

/// AC-3: Empty range — tenant exists but window has zero events;
/// returns 200 (NOT 404) with an empty body + manifest footer. An
/// empty audit history is a legitimate state per WI-S09-008 §4.
#[tokio::test]
async fn empty_range_returns_200_with_manifest_footer() {
    let tenant = Uuid::now_v7();
    // Chain at base_time 100..105, query window 10_000..20_000.
    let (state, _exporter, audit_sink) = fixture_with_chain(tenant, 5, 100);
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 10_000, 20_000))
        .await
        .expect("oneshot");

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "empty audit history is a legitimate state (200, not 404)"
    );
    let body = read_body(resp).await;
    // No NDJSON rows; only the manifest envelope.
    let footer: Value = serde_json::from_str(&body).expect("manifest");
    let manifest = footer.get("manifest").expect("manifest key");
    assert_eq!(manifest.get("event_count").and_then(Value::as_u64), Some(0));

    let captured = audit_sink.snapshot().expect("audit snapshot");
    assert_eq!(captured.len(), 1);
    let row = &captured[0];
    assert_eq!(row.event_type, EVENT_TYPE_EXPORT_REQUEST);
    assert_eq!(row.events_written, 0);
    assert_eq!(row.exit_status, "empty");
}

/// AC-4: Chain integrity tampered — a tampered exporter surfaces the
/// `corelink.audit.export_verify_failed.v1` SEV-0 audit row alongside
/// delivering the bytes (caller decides what to trust).
///
/// We construct a malicious exporter that returns a rows vec where
/// row 0's event body is rotated after the proof was computed, so the
/// server-side `verify_export_result` returns ChainBreak. The route
/// still flushes the bytes and emits the SEV-0 row.
#[tokio::test]
async fn chain_tamper_emits_verify_failed_sev0() {
    let tenant = Uuid::now_v7();
    let (state_orig, exporter, audit_sink) = fixture_with_chain(tenant, 3, 100);
    // Wrap the exporter in a tamper proxy that flips a byte on row[1].
    let tamper: Arc<dyn AuditExporter> = Arc::new(TamperingExporter {
        inner: exporter.clone(),
    });
    let state = AuditExportRouteState {
        exporter: tamper,
        rate_limiter: state_orig.rate_limiter.clone(),
        audit_sink: state_orig.audit_sink.clone(),
    };
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot");

    // The bytes ARE delivered (200) — caller decides what to trust.
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "tampered export still flushes bytes; the audit emit anchors detect"
    );
    let _body = read_body(resp).await;

    let captured = audit_sink.snapshot().expect("audit snapshot");
    // Expect at minimum the export_request.v1 AND verify_failed.v1
    // rows. Order: request first, then verify_failed.
    assert!(
        captured.len() >= 2,
        "expected request + verify_failed audit rows, got {captured:?}"
    );
    let types: Vec<&str> = captured.iter().map(|r| r.event_type.as_str()).collect();
    assert!(
        types.contains(&EVENT_TYPE_VERIFY_FAILED),
        "SEV-0 verify_failed audit emitted; rows={types:?}"
    );
    // The request row records exit_status="verify_failed" because
    // the verify gate failed before the request row was emitted.
    let request_row = captured
        .iter()
        .find(|r| r.event_type == EVENT_TYPE_EXPORT_REQUEST)
        .expect("export request row");
    assert_eq!(request_row.exit_status, "verify_failed");
}

/// AC-5 (bonus): Unauthenticated request (missing `X-Tenant-Id`) → 401.
#[tokio::test]
async fn missing_tenant_header_returns_401() {
    let tenant = Uuid::now_v7();
    let (state, _exporter, _audit_sink) = fixture_with_chain(tenant, 1, 100);
    let app = router(state);

    let req = Request::builder()
        .method("GET")
        .uri(format!("{AUDIT_EXPORT_ROUTE}?from=0&to=1000"))
        .body(Body::empty())
        .expect("req");

    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// AC-6 (bonus): Rate-limit second-within-minute → 429 + audit emit
/// `exit_status="rate_limited"`.
#[tokio::test]
async fn rate_limit_second_request_returns_429() {
    let tenant = Uuid::now_v7();
    let (state, _exporter, audit_sink) = fixture_with_chain(tenant, 1, 100);
    let app = router(state.clone());

    // First request: succeeds (bucket starts full).
    let resp1 = app
        .clone()
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot 1");
    assert_eq!(resp1.status(), StatusCode::OK);

    // Second request inside the same 60s window: 429.
    let resp2 = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot 2");
    assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
    // Retry-After header populated.
    assert!(resp2
        .headers()
        .get(axum::http::header::RETRY_AFTER)
        .is_some());

    let captured = audit_sink.snapshot().expect("audit snapshot");
    // 2 audit rows: first OK, second rate_limited.
    let rate_limited = captured
        .iter()
        .find(|r| r.exit_status == "rate_limited")
        .expect("rate_limited audit row");
    assert_eq!(rate_limited.event_type, EVENT_TYPE_EXPORT_REQUEST);
}

/// AC-7 (bonus): Audit-sink failure on the request emit aborts with
/// 503 (fail-CLOSED — never serve bytes without the audit row).
#[tokio::test]
async fn audit_failure_aborts_with_503() {
    let tenant = Uuid::now_v7();
    let (state, _exporter, _) = fixture_with_chain(tenant, 1, 100);
    // Replace audit sink with one that returns Err on emit.
    let bad = Arc::new(InMemoryExportAuditSink::new());
    bad.inject_failure("pipeline down").expect("inject");
    let state = AuditExportRouteState {
        exporter: state.exporter.clone(),
        rate_limiter: state.rate_limiter.clone(),
        audit_sink: bad as Arc<dyn ExportAuditSink>,
    };
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ---------------------------------------------------------------------------
// Tampering exporter — wraps a clean exporter and flips a byte on
// row 1's `event.data` AFTER the proof has been computed so the
// server-side `verify_export_result` returns `ChainBreak`. Used by
// the chain-tamper integration test.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TamperingExporter {
    inner: Arc<InMemoryAuditExporter>,
}

impl AuditExporter for TamperingExporter {
    fn export_window(
        &self,
        tenant_id: &str,
        window: corelink_audit_chain::ExportWindow,
    ) -> Result<corelink_audit_chain::ExportResult, corelink_audit_chain::AuditChainError> {
        let mut r = self.inner.export_window(tenant_id, window)?;
        if let Some(row) = r.rows.get_mut(1) {
            // Replace the event data with a different shape; the
            // proof was computed off the ORIGINAL bytes so a recompute
            // surfaces a chain-break.
            row.event.data = json!({"tampered": true});
        }
        Ok(r)
    }
}

// Force-link to ExportedAuditEvent so unused-import lints stay quiet
// when the type appears only via trait objects above.
#[allow(dead_code)]
fn _force_link_exported_event(_v: ExportedAuditEvent) {}

// Force-link to ExportAuditRow for the same reason.
#[allow(dead_code)]
fn _force_link_audit_row(_v: ExportAuditRow) {}
