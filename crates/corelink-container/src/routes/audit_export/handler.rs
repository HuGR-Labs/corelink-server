//! Axum handler for `GET /v1/audit/:tenant/export` and its private path
//! helpers.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Verbatim move of `handle_export`; supporting parse / serialize
//! helpers live in [`super::parse`].
//!
//! Wave-37 (fix): tenant is now extracted from the `:tenant` path
//! parameter following the `/v1/cas/:tenant/:hash` pattern. The Worker
//! extracts the PAT-resolved tenant id, routes the request to the
//! per-tenant DO, and forwards the full URL path to the container —
//! so the path tenant is the canonical authenticated tenant.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
};
use corelink_audit_chain::{verify_export_result, ExportWindow};
use corelink_ratelimit::{BucketKey, RateLimitDecision};
use http_body_util::StreamBody;
use uuid::Uuid;

use super::audit_sink::emit_or_503;
use super::parse::{parse_timestamp, serialize_ndjson_lines, uuid_eq_ct};
use super::state::{AuditExportQuery, AuditExportRouteState};
use super::stream::{build_audit_export_async_stream, InMemoryR2ListPager};
use super::types::{
    ExportAuditRow, EVENT_TYPE_CROSS_TENANT_ATTEMPT, EVENT_TYPE_EXPORT_REQUEST,
    EVENT_TYPE_VERIFY_FAILED, HEADER_CHAIN_HEAD_ANCHOR, HEADER_EXPORT_ABORTED,
};

/// `GET /v1/audit/:tenant/export` axum handler.
///
/// The `:tenant` path parameter is the canonical authenticated tenant
/// following the `/v1/cas/:tenant/:hash` pattern. The Worker extracts
/// the PAT-resolved tenant id, routes the request to the per-tenant DO,
/// and forwards the full URL path (including the tenant segment) to the
/// container — so the path tenant is the authoritative source of truth.
pub(super) async fn handle_export(
    State(state): State<AuditExportRouteState>,
    Path(path_tenant_str): Path<String>,
    Query(query): Query<AuditExportQuery>,
) -> axum::response::Response {
    // 1. Auth — tenant from the `:tenant` path segment (canonical,
    //    following /v1/cas/:tenant/:hash). The Worker injects the
    //    PAT-resolved tenant into the URL path before forwarding.
    let authenticated_tenant = match Uuid::parse_str(path_tenant_str.trim()) {
        Ok(t) => t,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "tenant: invalid uuid in path").into_response();
        }
    };

    // 2. Parse the window. Reject inverted / equal bounds at the
    //    route boundary so the exporter never sees a malformed shape.
    let from_ms = match parse_timestamp(&query.from) {
        Some(v) => v,
        None => {
            return (StatusCode::BAD_REQUEST, "from: invalid timestamp").into_response();
        }
    };
    let to_ms = match parse_timestamp(&query.to) {
        Some(v) => v,
        None => {
            return (StatusCode::BAD_REQUEST, "to: invalid timestamp").into_response();
        }
    };
    let window = match ExportWindow::new(from_ms, to_ms) {
        Ok(w) => w,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "from must be < to (inclusive, exclusive)",
            )
                .into_response();
        }
    };

    // 3. Cross-tenant attempt check. The optional `tenant` query
    //    parameter MUST equal the authenticated tenant (constant-
    //    time compare). A mismatch emits the SEV-1 security audit
    //    row BEFORE returning 403 (fail-CLOSED ordering).
    if let Some(attempted_str) = query.tenant.as_deref() {
        let attempted = match Uuid::parse_str(attempted_str.trim()) {
            Ok(t) => t,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "tenant: invalid uuid").into_response();
            }
        };
        if !uuid_eq_ct(&authenticated_tenant, &attempted) {
            // Wave-20 — fail-CLOSED audit emit (A-P1-02). On audit-sink
            // failure return 503 instead of 403 so the security team
            // never loses the cross-tenant anchor row to a silent
            // pipeline outage.
            let row = ExportAuditRow {
                event_type: EVENT_TYPE_CROSS_TENANT_ATTEMPT.to_string(),
                authenticated_tenant: Some(authenticated_tenant),
                attempted_tenant: Some(attempted),
                from_ms,
                to_ms,
                bytes_written: 0,
                events_written: 0,
                exit_status: "cross_tenant_reject".to_string(),
                payload: None,
            };
            if let Some(resp) = emit_or_503(&state.audit_sink, row) {
                return resp;
            }
            return (StatusCode::FORBIDDEN, "cross-tenant audit-export denied")
                .into_response();
        }
    }

    // 4. Rate-limit gate — 1 export per tenant per 60s
    //    (per WI-S09-008 §7 Q2 default + WI-S08-001 framework).
    let bucket_key = BucketKey::per_tenant_per_endpoint(
        authenticated_tenant,
        "audit.export",
    );
    // Wave-21 closure of A-P2-05: anchor the bucket `now_ms` to the
    // injected `WallClock` collaborator instead of the request window's
    // `until_ms`.
    //
    // Wave-23 closure of W21-R-P2-01 (adversarial review
    // `2026-05-16-wave21-adversarial-review.md` §3.3): the previous
    // implementation fell back to `now_ms_from_window(window).until_ms`
    // when `wall_clock.now_ms() == 0`. That re-introduced the same
    // attacker-controlled bucket clock the wave-21 stream was meant to
    // close — a customer querying a 1970-epoch window with `until_ms`
    // near 0 would still receive a window-derived bucket clock on the
    // saturating path. We now fail-CLOSED (503 + `clock_unavailable`
    // audit row) so the bucket clock NEVER couples to caller-controlled
    // request bytes, even on the structurally-unreachable pre-epoch
    // branch.
    //
    // Production reachability: `SystemWallClock` saturates to 0 only on
    // pre-1970 wall-clock instants — structurally impossible on any
    // production host (epoch is decades past). The fail-CLOSED 503
    // affects only (a) test fakes deliberately pinned at `unix_ms == 0`
    // and (b) exotic hosts with a pre-epoch system clock, which is an
    // operational failure the audit pipeline SHOULD surface.
    //
    // `now_ms_from_window` is retained at its def-site for archival
    // legacy use only — no production caller invokes it post-wave-23.
    let wall_now_ms = state.wall_clock.now_ms();
    if wall_now_ms == 0 {
        let row = ExportAuditRow {
            event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
            authenticated_tenant: Some(authenticated_tenant),
            attempted_tenant: None,
            from_ms,
            to_ms,
            bytes_written: 0,
            events_written: 0,
            exit_status: "clock_unavailable".to_string(),
            payload: None,
        };
        if let Some(resp) = emit_or_503(&state.audit_sink, row) {
            return resp;
        }
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "wall clock unavailable",
        )
            .into_response();
    }
    let now_ms = wall_now_ms;
    match state.rate_limiter.try_acquire(
        authenticated_tenant,
        bucket_key,
        1,
        now_ms,
    ) {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => {}
            // The decision enum is `#[non_exhaustive]`; the
            // Deny429 arm is the only non-Allow variant defined
            // today. Any future variant lands in the wildcard
            // and is treated as a deny path so the route never
            // serves bytes against an unknown decision shape.
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                // Wave-20 — fail-CLOSED audit emit (A-P2-01). On audit-
                // sink failure surface 503 instead of 429 so the analytics
                // dashboard's emit-count == 429-count parity assertion is
                // never silently broken.
                let row = ExportAuditRow {
                    event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
                    authenticated_tenant: Some(authenticated_tenant),
                    attempted_tenant: None,
                    from_ms,
                    to_ms,
                    bytes_written: 0,
                    events_written: 0,
                    exit_status: "rate_limited".to_string(),
                    payload: None,
                };
                if let Some(resp) = emit_or_503(&state.audit_sink, row) {
                    return resp;
                }
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut().insert(axum::http::header::RETRY_AFTER, val);
                }
                return resp;
            }
            // Future non-Allow decision variant — fail-CLOSED on
            // an unknown arm: deny the request with a generic 429
            // so the route never serves bytes under an unrecognised
            // decision shape. Wave-20 (A-P2-01 closure): emit a
            // `rate_limited` audit row before the response so the
            // analytics dashboard's `emit-count vs 429-count` parity
            // assertion holds across future variant growth — AND
            // fail-CLOSED via `emit_or_503` so the future variant
            // doesn't silently bypass the parity assertion either
            // (A-P1-05 + A-P2-01 jointly).
            _ => {
                let row = ExportAuditRow {
                    event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
                    authenticated_tenant: Some(authenticated_tenant),
                    attempted_tenant: None,
                    from_ms,
                    to_ms,
                    bytes_written: 0,
                    events_written: 0,
                    exit_status: "rate_limited".to_string(),
                    payload: None,
                };
                if let Some(resp) = emit_or_503(&state.audit_sink, row) {
                    return resp;
                }
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate-limit decision arm not handled",
                )
                    .into_response();
            }
        },
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate-limit pipeline failed",
            )
                .into_response();
        }
    }

    // 5. Run the export.
    let result = match state
        .exporter
        .export_window(&authenticated_tenant.to_string(), window)
    {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "audit-export pipeline failed",
            )
                .into_response();
        }
    };

    // 6. Verify the proof chain server-side BEFORE emitting bytes.
    //    Wave-16 upfront gate: catches the chain-break early so the
    //    `export_request.v1` row carries `exit_status="verify_failed"`
    //    (the security anchor preserved across wave-17 and wave-18).
    //    Wave-18 adds a complementary per-row re-verify INSIDE the
    //    streaming body — on detect we ALSO emit a SEV-0 with the
    //    mid-stream `{break_at_seq, break_at_chunk, observed,
    //    expected}` payload and flush the
    //    `X-CoreLink-Audit-Export-Aborted` HTTP trailer before
    //    closing the body. The customer-CLI surfaces the trailer as
    //    an actionable diagnostic.
    let verify_outcome = verify_export_result(&result);
    let verify_failed = verify_outcome.is_err();

    // 7. Pre-serialize the per-row NDJSON envelopes + the trailing
    //    manifest line so any serialization failure surfaces as a
    //    `500` BEFORE the response status is sent (we never want to
    //    flip to `200` and then discover row 3 can't serialize). The
    //    bytes themselves are streamed out one frame at a time
    //    further down — the HTTP wire shape is chunked / no
    //    Content-Length, NOT a buffer-then-flush 200.
    let manifest_json = match serde_json::to_string(&result.manifest) {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "manifest serialize failed",
            )
                .into_response();
        }
    };
    let row_lines = match serialize_ndjson_lines(&result.rows) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "ndjson serialize failed",
            )
                .into_response();
        }
    };
    let manifest_line = format!("{{\"manifest\":{manifest_json}}}");
    // bytes_written = sum of every row line length + an interleaving
    // newline between successive rows + a newline before the manifest
    // line + the manifest line bytes. Mirrors the wave-16 wire shape
    // verbatim so the wave-17 CLI sees the SAME byte stream.
    let row_bytes_sum: u64 = row_lines.iter().map(|l| l.len() as u64).sum();
    let interleave_newlines: u64 = if row_lines.is_empty() {
        0
    } else {
        row_lines.len() as u64 // (n-1) between rows + 1 before manifest = n
    };
    let body_bytes_len = row_bytes_sum + interleave_newlines + manifest_line.len() as u64;
    let ndjson_line_count = row_lines.len() as u64;

    let exit_status = if verify_failed {
        "verify_failed"
    } else if ndjson_line_count == 0 {
        "empty"
    } else {
        "ok"
    };

    // 8. Audit emit — BEFORE the byte stream. Audit failure on the
    //    `export_request.v1` arm aborts with 503 (fail-CLOSED per
    //    INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
    let request_row = ExportAuditRow {
        event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
        authenticated_tenant: Some(authenticated_tenant),
        attempted_tenant: None,
        from_ms,
        to_ms,
        bytes_written: body_bytes_len,
        events_written: ndjson_line_count,
        exit_status: exit_status.to_string(),
        payload: None,
    };
    if state.audit_sink.emit(request_row).is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "audit pipeline closed",
        )
            .into_response();
    }

    if verify_failed {
        // SEV-0 — emit the verify-failed security row alongside the
        // request row. The customer still receives the bytes (the
        // mid-stream abort trailer fires on the FIRST tampered row);
        // the security team gets paged off this SEV-0 emit.
        //
        // Wave-20 — fail-CLOSED (A-P1-05). On audit-sink failure we
        // surface 503 instead of streaming the (still-tampered) body
        // without the SEV-0 anchor — the customer can retry, but a
        // silent miss of the security-team page is unacceptable.
        let row = ExportAuditRow {
            event_type: EVENT_TYPE_VERIFY_FAILED.to_string(),
            authenticated_tenant: Some(authenticated_tenant),
            attempted_tenant: None,
            from_ms,
            to_ms,
            bytes_written: body_bytes_len,
            events_written: ndjson_line_count,
            exit_status: "verify_failed".to_string(),
            payload: None,
        };
        if let Some(resp) = emit_or_503(&state.audit_sink, row) {
            return resp;
        }
    }

    // 9. Build the streaming response body. Per-row NDJSON frames
    //    + the trailing manifest line. The per-row re-verify gate
    //    runs INSIDE the stream against the manifest anchor; on the
    //    FIRST chain-break we emit a SEV-0 carrying the mid-stream
    //    `{break_at_seq, break_at_chunk, observed, expected}` payload
    //    and ship the `X-CoreLink-Audit-Export-Aborted` trailer as
    //    the final body frame. The audit emit ALWAYS lands BEFORE
    //    the trailer frame on the wire.
    //
    // Wave-19 lift: the frame plan is no longer pre-materialized into
    // a `Vec<Frame<Bytes>>` — `build_audit_export_async_stream` wraps
    // an `async_stream::stream!` generator that yields one frame at a
    // time, driven by the [`super::stream::R2ListPager`] page boundary.
    // The generator respects axum's body-flow back-pressure (it
    // `yield`s and parks on the consumer poll), so memory in-flight is
    // bounded by ONE page of rows plus the current row buffer.
    let anchor_head = result.manifest.chain_head_at_export;
    let audit_sink_for_stream = Arc::clone(&state.audit_sink);
    let tenant_for_stream = authenticated_tenant;
    let pager = InMemoryR2ListPager::with_rows(result.rows, state.pager_page_size);
    let body_stream = build_audit_export_async_stream(
        Box::new(pager),
        manifest_line,
        anchor_head,
        audit_sink_for_stream,
        tenant_for_stream,
        from_ms,
        to_ms,
        body_bytes_len,
        ndjson_line_count,
    );
    let body = Body::new(StreamBody::new(body_stream));

    let mut resp = (StatusCode::OK, body).into_response();
    if let Ok(val) = HeaderValue::from_str("application/x-ndjson") {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, val);
    }
    if let (Ok(val), Ok(name)) = (
        HeaderValue::from_str(&result.manifest.chain_head_at_export.to_hex()),
        HeaderName::from_bytes(HEADER_CHAIN_HEAD_ANCHOR.as_bytes()),
    ) {
        resp.headers_mut().insert(name, val);
    }
    // Advertise the abort trailer upfront per RFC 7230 §4.4 so any
    // intermediary preserving only declared trailers keeps ours. The
    // trailer fires ONLY on mid-stream chain-break; on the happy path
    // the response closes with zero trailer frames + the advertised
    // trailer simply doesn't appear in the final block.
    if let Ok(val) = HeaderValue::from_str(HEADER_EXPORT_ABORTED) {
        resp.headers_mut().insert(axum::http::header::TRAILER, val);
    }
    resp
}
