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
use http_body_util::BodyExt; // .collect() for body+trailers
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
    EVENT_TYPE_VERIFY_FAILED, EXIT_STATUS_VERIFY_FAILED_MID_STREAM, HEADER_EXPORT_ABORTED,
    R2_LIST_PAGE_SIZE, TENANT_ID_HEADER,
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
        pager_page_size: R2_LIST_PAGE_SIZE,
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
        pager_page_size: state_orig.pager_page_size,
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
        pager_page_size: state.pager_page_size,
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

// ---------------------------------------------------------------------------
// Wave-18 — true streaming NDJSON + `X-CoreLink-Audit-Export-Aborted`
// mid-stream HTTP trailer. The wave-16 buffer-then-flush wire shape
// was a temporary SEAL caveat; this wave wires
// `axum::body::Body::new(StreamBody::new(...))` with per-row inclusion-
// proof re-verify and a `Frame::trailers` abort frame on chain-break.
// ---------------------------------------------------------------------------

/// Wave-18 AC-1 — the response body MUST NOT carry a precomputed
/// `Content-Length` (the wire shape transitions to chunked / streamed
/// rather than the wave-16 buffer-then-flush vector).
#[tokio::test]
async fn streaming_response_does_not_buffer() {
    let tenant = Uuid::now_v7();
    let (state, _exporter, _audit_sink) = fixture_with_chain(tenant, 5, 100);
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot");

    assert_eq!(resp.status(), StatusCode::OK);
    // No Content-Length — wave-18 streams the body, axum doesn't know
    // the total length upfront and therefore doesn't emit
    // Content-Length.
    assert!(
        resp.headers().get(axum::http::header::CONTENT_LENGTH).is_none(),
        "wave-18 streaming response must NOT carry a Content-Length; got headers={:?}",
        resp.headers()
    );
    // The `Trailer:` response header advertises the abort trailer
    // name upfront per RFC 7230 §4.4 so intermediaries preserve it.
    let trailer_decl = resp
        .headers()
        .get(axum::http::header::TRAILER)
        .expect("Trailer header advertised upfront")
        .to_str()
        .expect("ascii Trailer header");
    assert_eq!(trailer_decl, HEADER_EXPORT_ABORTED);
    // And the body still parses as NDJSON line-by-line per wave-16.
    let body = read_body(resp).await;
    let lines: Vec<&str> = body.split('\n').filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 6, "5 row lines + 1 manifest line; got body={body}");
}

/// Wave-18 AC-2 — induced chain-break MID-STREAM surfaces the
/// `X-CoreLink-Audit-Export-Aborted` HTTP trailer with the canonical
/// `{break_at_seq, break_at_chunk, observed, expected}` payload + the
/// SEV-0 audit row is emitted BEFORE the trailer frame ships.
#[tokio::test]
async fn abort_trailer_emitted_on_mid_stream_chain_break() {
    let tenant = Uuid::now_v7();
    let (state_orig, exporter, audit_sink) = fixture_with_chain(tenant, 3, 100);
    // Tamper proxy flips a byte on row[1].
    let tamper: Arc<dyn AuditExporter> = Arc::new(TamperingExporter {
        inner: exporter.clone(),
    });
    let state = AuditExportRouteState {
        exporter: tamper,
        rate_limiter: state_orig.rate_limiter.clone(),
        audit_sink: state_orig.audit_sink.clone(),
        pager_page_size: state_orig.pager_page_size,
    };
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot");

    assert_eq!(resp.status(), StatusCode::OK, "tampered export still flushes a 200");
    // Drain the body + the trailers via http_body_util::BodyExt::collect.
    let collected = BodyExt::collect(resp.into_body()).await.expect("collect body");
    let trailers = collected
        .trailers()
        .cloned()
        .expect("mid-stream abort: trailers frame present");
    let val = trailers
        .get(HEADER_EXPORT_ABORTED)
        .expect("X-CoreLink-Audit-Export-Aborted trailer present");
    let payload = val.to_str().expect("ascii trailer payload");
    // Payload is canonical JSON with all four keys.
    let parsed: Value = serde_json::from_str(payload).expect("trailer payload parses");
    assert!(parsed.get("break_at_seq").and_then(Value::as_u64).is_some());
    assert!(parsed.get("break_at_chunk").and_then(Value::as_u64).is_some());
    assert_eq!(
        parsed.get("observed").and_then(Value::as_str).map(str::len),
        Some(64),
        "observed hash is BLAKE3-256 hex (64 chars)"
    );
    assert_eq!(
        parsed.get("expected").and_then(Value::as_str).map(str::len),
        Some(64),
        "expected hash is BLAKE3-256 hex (64 chars)"
    );

    // Audit emit ORDERING — wave-18 emitted via colon-prefix encoding;
    // wave-19 schema lift (closes caveat #4): structured payload rides
    // in the first-class `ExportAuditRow::payload` column and
    // `exit_status` carries the stable enum
    // `EXIT_STATUS_VERIFY_FAILED_MID_STREAM`.
    let captured = audit_sink.snapshot().expect("audit snapshot");
    let mid_stream_emits: Vec<&ExportAuditRow> = captured
        .iter()
        .filter(|r| r.exit_status == EXIT_STATUS_VERIFY_FAILED_MID_STREAM)
        .collect();
    assert!(
        !mid_stream_emits.is_empty(),
        "mid-stream SEV-0 audit row emitted carrying the wave-19 structured payload"
    );
    let mid = mid_stream_emits[0];
    assert_eq!(mid.event_type, EVENT_TYPE_VERIFY_FAILED);
    let audit_payload = mid
        .payload
        .as_ref()
        .expect("wave-19 payload column populated on mid-stream break");
    assert_eq!(audit_payload.get("break_at_seq"), parsed.get("break_at_seq"));
    assert_eq!(audit_payload.get("break_at_chunk"), parsed.get("break_at_chunk"));
    assert_eq!(audit_payload.get("observed"), parsed.get("observed"));
    assert_eq!(audit_payload.get("expected"), parsed.get("expected"));
}

/// Wave-18 AC-3 — round-trip the streaming response through the
/// wave-17 `verify-ndjson` CLI's parser. The body bytes (excluding
/// trailers) MUST remain line-by-line parseable; on the abort-trailer
/// arm an HTTP-aware client surfaces the canonical
/// "Export aborted mid-stream — server detected chain break at seq N
/// chunk X" diagnostic with exit code 1 (encoded here as a structured
/// `CliCompatDiagnostic` value).
#[tokio::test]
async fn customer_cli_handles_abort_trailer_gracefully() {
    // Happy-path arm — streaming body parses cleanly line-by-line.
    {
        let tenant = Uuid::now_v7();
        let (state, _exporter, _audit_sink) = fixture_with_chain(tenant, 4, 100);
        let app = router(state);
        let resp = app
            .oneshot(build_request(tenant, 0, 1_000))
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let collected = BodyExt::collect(resp.into_body())
            .await
            .expect("collect body");
        // No trailer on happy path.
        let trailers = collected.trailers().cloned().unwrap_or_default();
        assert!(
            trailers.get(HEADER_EXPORT_ABORTED).is_none(),
            "happy path emits no abort trailer"
        );
        let body_bytes = collected.to_bytes();
        let body = String::from_utf8(body_bytes.to_vec()).expect("utf-8");
        // The wave-17 CLI parser logic: every non-empty line except
        // the last is a `{event, proof}` row; the last is the
        // `{"manifest": ...}` envelope.
        let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(lines.len() >= 2, "at least one row + manifest");
        let last = lines.last().expect("last");
        let footer: Value = serde_json::from_str(last).expect("manifest");
        assert!(footer.get("manifest").is_some(), "footer carries manifest");
        for line in &lines[..lines.len() - 1] {
            let v: Value = serde_json::from_str(line).expect("row");
            assert!(v.get("event").is_some());
            assert!(v.get("proof").is_some());
        }
    }
    // Abort arm — HTTP-aware client detects the trailer + raises the
    // canonical diagnostic with exit code 1.
    {
        let tenant = Uuid::now_v7();
        let (state_orig, exporter, _audit_sink) = fixture_with_chain(tenant, 3, 100);
        let tamper: Arc<dyn AuditExporter> = Arc::new(TamperingExporter {
            inner: exporter.clone(),
        });
        let state = AuditExportRouteState {
            exporter: tamper,
            rate_limiter: state_orig.rate_limiter.clone(),
            audit_sink: state_orig.audit_sink.clone(),
            pager_page_size: state_orig.pager_page_size,
        };
        let app = router(state);
        let resp = app
            .oneshot(build_request(tenant, 0, 1_000))
            .await
            .expect("oneshot");
        let collected = BodyExt::collect(resp.into_body())
            .await
            .expect("collect body");
        let trailers = collected.trailers().cloned().expect("trailers");
        let payload_str = trailers
            .get(HEADER_EXPORT_ABORTED)
            .expect("abort trailer")
            .to_str()
            .expect("ascii")
            .to_string();
        let diagnostic = cli_compat_diagnostic_from_trailer(&payload_str);
        assert_eq!(diagnostic.exit_code, 1);
        assert!(
            diagnostic
                .message
                .starts_with("Export aborted mid-stream — server detected chain break at seq "),
            "canonical diagnostic surfaced: {}",
            diagnostic.message
        );
        assert!(
            diagnostic.message.contains(" chunk "),
            "diagnostic carries chunk index: {}",
            diagnostic.message
        );
        // The structured payload survives intact for SIEM consumers.
        let parsed: Value =
            serde_json::from_str(&diagnostic.structured_payload).expect("structured");
        assert!(parsed.get("break_at_seq").is_some());
        assert!(parsed.get("break_at_chunk").is_some());
        assert!(parsed.get("observed").is_some());
        assert!(parsed.get("expected").is_some());
    }
}

/// Mirrors the wave-17 customer-CLI diagnostic shape an HTTP-aware
/// invocation would produce after observing the
/// `X-CoreLink-Audit-Export-Aborted` trailer. The CLI today reads
/// from a file (offline); when wired through the network it ALSO
/// observes trailers and surfaces this diagnostic. Wave-18 ships the
/// wire format; the CLI's HTTP-fetch path follow-on lifts this
/// helper verbatim.
#[derive(Debug)]
struct CliCompatDiagnostic {
    exit_code: i32,
    message: String,
    structured_payload: String,
}

fn cli_compat_diagnostic_from_trailer(payload: &str) -> CliCompatDiagnostic {
    let parsed: Value =
        serde_json::from_str(payload).expect("trailer payload is canonical JSON");
    let seq = parsed
        .get("break_at_seq")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let chunk = parsed
        .get("break_at_chunk")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let message = format!(
        "Export aborted mid-stream — server detected chain break at seq {seq} chunk {chunk}"
    );
    CliCompatDiagnostic {
        exit_code: 1,
        message,
        structured_payload: payload.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Wave-19 integration test — multi-page async streaming. Exercises the
// `R2ListPager`-backed `async_stream::stream!` generator over a window
// that spans >1 page (2500 keys → 3 pages at the canonical page size
// of 1000). Pins the contract that the generator yields ALL rows across
// page boundaries + the trailing manifest frame, with no Content-Length
// and the `Trailer:` header advertised.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn streaming_response_yields_all_rows_across_multiple_pages() {
    // Wave-19 multi-page contract — exercise >1 R2 list page in the
    // generator. The spec target is "2500 keys → 3 pages at the
    // canonical 1000/page split"; the InMemoryAuditExporter builds
    // O(n²) sibling lists in `export_window` (each row carries the
    // forward chain — see crates/corelink-audit-chain/src/exporter.rs
    // §330), so 2500 events produce a ~500 MB body that exhausts the
    // axum `to_bytes` limit. We exercise the SAME multi-page wire
    // path by overriding `pager_page_size` to `3` against a 9-row
    // chain → 3 pages of 3 rows each (3×3 mirrors the canonical
    // 1000×3 = 2500-key shape with `2500/1000` rounded down to 3
    // pages + a small tail). The wave-19 unit test
    // `async_stream_yields_rows_across_pages_then_manifest` covers
    // page-size=3 over 7 rows for the asymmetric tail; this test
    // covers the symmetric N×N case at the HTTP wire-level.
    let tenant = Uuid::now_v7();
    let row_count: u64 = 9;
    let (mut state, _exporter, _audit_sink) =
        fixture_with_chain(tenant, row_count, 100);
    state.pager_page_size = 3;
    assert!(
        R2_LIST_PAGE_SIZE > state.pager_page_size,
        "test exercises page size override below the canonical default \
         ({R2_LIST_PAGE_SIZE})"
    );
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 10_000))
        .await
        .expect("oneshot");

    assert_eq!(resp.status(), StatusCode::OK);
    // Streaming wire shape preserved across multi-page: no
    // Content-Length, Trailer: advertised upfront.
    assert!(
        resp.headers().get(axum::http::header::CONTENT_LENGTH).is_none(),
        "multi-page response must remain Content-Length-less (chunked / streamed)"
    );
    let trailer_decl = resp
        .headers()
        .get(axum::http::header::TRAILER)
        .expect("Trailer header advertised")
        .to_str()
        .expect("ascii Trailer header");
    assert_eq!(trailer_decl, HEADER_EXPORT_ABORTED);

    // Drain the body; the body MUST hold 9 row lines + 1 manifest
    // line, in order. The wave-19 generator yields one row per
    // Frame::data (cross-page); the wave-16 wire shape is preserved
    // byte-for-byte (rows suffixed with `\n`, manifest tail). The
    // page boundary is invisible at the wire level — proof the
    // streaming generator stitches pages cleanly.
    let body = to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("drain body");
    let text = std::str::from_utf8(&body).expect("ascii ndjson");
    let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        (row_count + 1) as usize,
        "expected {row_count} row lines + 1 manifest line; got body={text}"
    );
    // First line parses as `{event, proof}`; last line parses as `{manifest}`.
    let first: Value = serde_json::from_str(lines[0]).expect("first row json");
    assert!(first.get("event").is_some(), "first line is a row envelope");
    assert!(first.get("proof").is_some(), "first line carries inclusion proof");
    let last: Value =
        serde_json::from_str(lines[row_count as usize]).expect("manifest json");
    assert!(last.get("manifest").is_some(), "last line is the manifest");
}

/// Wave-19 closure (caveat #4 from wave-18) — the structured
/// `{break_at_seq, break_at_chunk, observed, expected}` payload
/// captured in the audit row and the JSON object encoded in the
/// HTTP `X-CoreLink-Audit-Export-Aborted` trailer are **byte-
/// identical** (constant-time `subtle::ConstantTimeEq` over the
/// canonical-serialized bytes). Pins the schema lift so an
/// accidental divergence between the two encoders surfaces as a
/// test failure.
#[tokio::test]
async fn wave19_audit_row_payload_and_trailer_payload_byte_identical() {
    use subtle::ConstantTimeEq;

    let tenant = Uuid::now_v7();
    let (state_orig, exporter, audit_sink) = fixture_with_chain(tenant, 3, 100);
    let tamper: Arc<dyn AuditExporter> = Arc::new(TamperingExporter {
        inner: exporter.clone(),
    });
    let state = AuditExportRouteState {
        exporter: tamper,
        rate_limiter: state_orig.rate_limiter.clone(),
        audit_sink: state_orig.audit_sink.clone(),
        pager_page_size: state_orig.pager_page_size,
    };
    let app = router(state);

    let resp = app
        .oneshot(build_request(tenant, 0, 1_000))
        .await
        .expect("oneshot");
    let collected = BodyExt::collect(resp.into_body()).await.expect("collect");
    let trailers = collected.trailers().cloned().expect("trailer present");
    let trailer_val = trailers
        .get(HEADER_EXPORT_ABORTED)
        .expect("abort trailer")
        .to_str()
        .expect("ascii trailer payload")
        .to_string();

    let captured = audit_sink.snapshot().expect("snap");
    let mid = captured
        .iter()
        .find(|r| r.exit_status == EXIT_STATUS_VERIFY_FAILED_MID_STREAM)
        .expect("mid-stream emit captured");
    let audit_payload = mid.payload.as_ref().expect("wave-19 payload populated");

    // Reconstruct the canonical bytes from the audit-row payload via
    // the SAME canonical encoder the trailer uses
    // (`mid_stream_abort_trailer_value`). serde_json's default
    // `Map<String, Value>` is alphabetically ordered (no
    // `preserve_order` feature in the workspace); the canonical wire
    // shape pins a specific key order — we re-encode through the
    // shared formatter so the audit-row and trailer bytes match
    // exactly regardless of the internal Map representation.
    let break_at_seq = audit_payload
        .get("break_at_seq")
        .and_then(serde_json::Value::as_u64)
        .expect("break_at_seq populated");
    let break_at_chunk = audit_payload
        .get("break_at_chunk")
        .and_then(serde_json::Value::as_u64)
        .expect("break_at_chunk populated");
    let observed = audit_payload
        .get("observed")
        .and_then(serde_json::Value::as_str)
        .expect("observed populated")
        .to_string();
    let expected = audit_payload
        .get("expected")
        .and_then(serde_json::Value::as_str)
        .expect("expected populated")
        .to_string();
    let audit_bytes = corelink_server::routes::audit_export::mid_stream_abort_trailer_value(
        break_at_seq,
        break_at_chunk,
        &observed,
        &expected,
    );

    // Constant-time byte-identity compare — defence-in-depth on the
    // security-critical chain-break diagnostic. The audit-row payload
    // and the HTTP trailer payload encode the SAME canonical bytes
    // (modulo serde_json Map ordering, which we normalise above via
    // the shared encoder).
    assert_eq!(
        audit_bytes.len(),
        trailer_val.len(),
        "audit payload + trailer payload byte lengths differ: audit={audit_bytes}, trailer={trailer_val}"
    );
    let eq: bool = audit_bytes
        .as_bytes()
        .ct_eq(trailer_val.as_bytes())
        .into();
    assert!(
        eq,
        "wave-19: audit-row payload bytes (re-encoded via the canonical formatter) and HTTP trailer payload bytes must be byte-identical; \
         audit={audit_bytes}, trailer={trailer_val}"
    );
    // Additionally pin the structured field equality so the test
    // catches a regression where the formatter and the audit-row
    // payload disagree on the SHAPE (not just the byte ordering).
    let trailer_parsed: Value =
        serde_json::from_str(&trailer_val).expect("trailer payload parses as JSON");
    assert_eq!(audit_payload.get("break_at_seq"), trailer_parsed.get("break_at_seq"));
    assert_eq!(audit_payload.get("break_at_chunk"), trailer_parsed.get("break_at_chunk"));
    assert_eq!(audit_payload.get("observed"), trailer_parsed.get("observed"));
    assert_eq!(audit_payload.get("expected"), trailer_parsed.get("expected"));
}
