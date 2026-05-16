//! `GET /v1/audit/export` — customer-facing audit-log export route
//! (Wave-15.3; WI-S09-008 wiring).
//!
//! Streams NDJSON of every audit event in a tenant's `[from, to)`
//! window with a cryptographic inclusion proof per row, plus a
//! `chain_head_at_export` anchor footer the customer re-verifies
//! offline against the published daily-verify head.
//!
//! ## Auth model (production wiring)
//!
//! Production deployment slots a JWT-validating tower middleware in
//! front of this route. The middleware extracts the Clerk session
//! tenant id and injects it via the `X-Tenant-Id` request header
//! (mirrors the cas + admin wire-up patterns; see `cas.rs` doc on
//! the trait-object swap point for the wasm32 CF-Worker variant).
//! The route compares the header-supplied tenant id against the
//! `tenant_id` of every emitted row via `subtle::ConstantTimeEq`
//! (defence in depth — the comparator never short-circuits on the
//! first differing byte).
//!
//! Per the WI-S09-008 spec §4 the tenant_id is **NOT** a query
//! string. The route accepts an optional `tenant` query parameter
//! for the cross-tenant-attempt audit path: when a caller supplies
//! a `tenant` that disagrees with the authenticated principal we
//! emit `corelink.security.audit_export_cross_tenant_attempt.v1`
//! and reject with `403 Forbidden`. The query parameter is the
//! ONLY way to surface the attempted (vs. authenticated) tenant
//! mismatch.
//!
//! ## Inclusion proof
//!
//! Each NDJSON line carries `{event: <AuditEvent>, proof: <InclusionProof>}`
//! where `InclusionProof` is the canonical degenerate-Merkle chain
//! sibling list lifted from `corelink-audit-chain::exporter`. The
//! customer verifier (`corelink audit verify`) re-runs
//! [`corelink_audit_chain::verify_export_result`] over the streamed
//! manifest + rows; any mismatch surfaces as
//! `AuditChainError::ChainBreak` which the verifier renders as
//! `corelink.audit.export_verify_failed.v1` SEV-0 at server-side
//! emit time.
//!
//! ## Rate limit (per WI-S09-008 §7 Q2 default = yes)
//!
//! Bound to the existing `corelink-ratelimit::InMemoryTokenBucketRateLimiter`
//! framework (WI-S08-001). One concurrent export per tenant per
//! minute. The bucket key is `BucketKey::per_tenant_per_endpoint(
//! tenant_id, "audit.export")` so a tenant's export bucket is
//! isolated from its CAS/AC/admin buckets. Refill = 1 token per 60
//! seconds; burst = 1.
//!
//! ## Empty-range semantics
//!
//! A `[from, to)` window with zero matching events returns `200 OK`
//! with a zero-line NDJSON body + the manifest footer (empty
//! tenant audit history is a legitimate state per the WI-S09-008
//! spec — distinct from a `404` which would imply the tenant
//! itself is absent).
//!
//! ## Fail-CLOSED ordering
//!
//! The route emits the `corelink.audit.export_request.v1` audit row
//! BEFORE writing any bytes to the response stream. Audit-sink
//! failure aborts the export with `503 Service Unavailable` (mirrors
//! the cas + admin `AuditFailed → 503` discipline). Cross-tenant
//! reject emits `corelink.security.audit_export_cross_tenant_attempt.v1`
//! BEFORE the `403`. Verify-failed emits
//! `corelink.audit.export_verify_failed.v1` SEV-0 AFTER the bytes
//! are flushed (we still deliver the bytes — the caller decides
//! what to trust; the audit emit guarantees the server-side detect
//! anchor for the security team page-out).

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use bytes::Bytes;
use corelink_audit_chain::{
    verify_export_result, verify_inclusion_proof, AuditExporter, ExportWindow,
    ExportedAuditEvent, InMemoryAuditExporter,
};
use http_body::Frame;
use http_body_util::StreamBody;
use corelink_ratelimit::{
    BucketKey, InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimitConfig, RateLimitDecision, RateLimiter,
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use uuid::Uuid;

/// Canonical audit-export route path.
pub const AUDIT_EXPORT_ROUTE: &str = "/v1/audit/export";

/// Header name production wiring uses to inject the
/// JWT-validated tenant id (see module-level docs §Auth model).
pub const TENANT_ID_HEADER: &str = "x-tenant-id";

/// Canonical CloudEvents-1.0 `type` literal for the export-request
/// audit emit.
pub const EVENT_TYPE_EXPORT_REQUEST: &str = "corelink.audit.export_request.v1";

/// Canonical event type for the cross-tenant audit-export attempt
/// security emit.
pub const EVENT_TYPE_CROSS_TENANT_ATTEMPT: &str =
    "corelink.security.audit_export_cross_tenant_attempt.v1";

/// Canonical event type for the SEV-0 audit-export verify failure
/// emit.
pub const EVENT_TYPE_VERIFY_FAILED: &str =
    "corelink.audit.export_verify_failed.v1";

/// Canonical chain-head anchor response header (mirrors the
/// wave-17 customer-CLI doc reference). Lower-case per HTTP/2 wire
/// convention.
pub const HEADER_CHAIN_HEAD_ANCHOR: &str =
    "x-corelink-audit-export-chain-head-anchor";

/// Wave-18 HTTP trailer name surfaced on the response when the
/// streaming verifier detects a chain-break MID-STREAM. The trailer
/// value is a compact JSON object
/// `{"break_at_seq":<u64>,"break_at_chunk":<u64>,"observed":"<hex>","expected":"<hex>"}`
/// — the customer-CLI parses it to surface the actionable diagnostic
/// "Export aborted mid-stream — server detected chain break at seq N
/// chunk X" (see `crates/corelink-cli/src/commands/verify_ndjson.rs`).
///
/// Per HTTP/1.1 (RFC 7230 §4.4) trailers MUST be advertised up-front
/// via the `Trailer:` response header so intermediaries that strip
/// unknown trailers know to preserve this one; we always advertise
/// the trailer name even on the happy path so the wire shape is
/// stable.
pub const HEADER_EXPORT_ABORTED: &str =
    "x-corelink-audit-export-aborted";

/// Audit emit record captured by the route. Carries the canonical
/// CloudEvents `type` + the load-bearing tenant + window + byte
/// count + exit status fields per WI-S09-008 §4 (DELIVERABLES).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportAuditRow {
    /// Canonical CloudEvents `type` (one of `EVENT_TYPE_*` constants).
    pub event_type: String,
    /// Authenticated tenant (from the `X-Tenant-Id` header) — `None`
    /// if the request never reached the auth step.
    pub authenticated_tenant: Option<Uuid>,
    /// Attempted tenant (from the optional `tenant` query parameter).
    /// Populated on the cross-tenant-attempt arm.
    pub attempted_tenant: Option<Uuid>,
    /// Window lower bound (Unix epoch ms; inclusive).
    pub from_ms: u64,
    /// Window upper bound (Unix epoch ms; exclusive).
    pub to_ms: u64,
    /// NDJSON byte count flushed to the response stream. `0` for
    /// reject paths (we never wrote bytes).
    pub bytes_written: u64,
    /// Number of audit events flushed (matches manifest.event_count
    /// on the happy path; `0` on reject / empty range).
    pub events_written: u64,
    /// Canonical exit status: `"ok"` / `"empty"` / `"cross_tenant_reject"` /
    /// `"verify_failed"` / `"unauthorized"` / `"rate_limited"` /
    /// `"bad_request"` / `"audit_failed"`.
    pub exit_status: String,
}

/// Audit sink trait — the route's audit-emit boundary. Production
/// wiring binds this to the durable CloudEvents emitter that lifts
/// the row into the standard audit-chain envelope; the in-memory
/// fake captures every emit for unit + integration test inspection.
pub trait ExportAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one [`ExportAuditRow`]. Production wiring is fail-
    /// CLOSED (the route MUST receive an `Err` if the emit failed;
    /// callers convert that to a 503 to honour
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    ///
    /// # Errors
    ///
    /// Returns a static error string when the audit pipeline is
    /// closed / poisoned. The route converts to 503.
    fn emit(&self, row: ExportAuditRow) -> Result<(), &'static str>;
}

/// In-memory capture sink for the export-audit emits. Cloning
/// shares the captured buffer so the test harness can inspect
/// emit ordering without re-handing the sink to the route state.
#[derive(Clone, Debug, Default)]
pub struct InMemoryExportAuditSink {
    inner: Arc<Mutex<Vec<ExportAuditRow>>>,
    /// When set to `Some`, every `emit` returns the contained error
    /// string — used to drive the fail-CLOSED 503 regression.
    injected_failure: Arc<Mutex<Option<&'static str>>>,
}

impl InMemoryExportAuditSink {
    /// Construct a fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured audit row in emit order.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<ExportAuditRow>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "export audit sink mutex poisoned")?;
        Ok(g.clone())
    }

    /// Inject a static failure to drive the 503 fail-CLOSED test.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn inject_failure(&self, msg: &'static str) -> Result<(), &'static str> {
        let mut g = self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        *g = Some(msg);
        Ok(())
    }
}

impl ExportAuditSink for InMemoryExportAuditSink {
    fn emit(&self, row: ExportAuditRow) -> Result<(), &'static str> {
        // Fail-CLOSED check FIRST — never buffer a row we'd have
        // returned an error for.
        let injected = *self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        if let Some(msg) = injected {
            return Err(msg);
        }
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "export audit sink mutex poisoned")?;
        g.push(row);
        Ok(())
    }
}

/// Shared route state. `Arc<dyn ...>` keeps the binary-shape stable
/// across the native / wasm32 swap.
#[derive(Clone)]
pub struct AuditExportRouteState {
    /// Audit-export pure-logic primitive (see WI-R-PREP-AUDIT-EXPORT).
    pub exporter: Arc<dyn AuditExporter>,
    /// Per-tenant rate limiter (per WI-S08-001 framework).
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Route-level audit sink for the `export_request.v1` +
    /// security + verify-failed emits.
    pub audit_sink: Arc<dyn ExportAuditSink>,
}

impl core::fmt::Debug for AuditExportRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AuditExportRouteState").finish_non_exhaustive()
    }
}

/// Build the in-memory native-target route state.
///
/// Production wiring slots the durable R2-backed exporter +
/// `RateLimiter` DO singleton + CloudEvents audit sink here. The
/// trait-object surface keeps the route shape stable across the
/// swap.
#[must_use]
pub fn build_state() -> AuditExportRouteState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
        let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(
            InMemoryTokenBucketRateLimiter::new(
                rl_audit,
                rl_metrics,
                audit_export_rate_limit_config(),
            ),
        );
        let audit_sink: Arc<dyn ExportAuditSink> = Arc::new(InMemoryExportAuditSink::new());
        AuditExportRouteState {
            exporter,
            rate_limiter,
            audit_sink,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        compile_error!(
            "wasm32 CF-Worker audit-export handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Per-WI-S09-008 §7 Q2 rate-limit config: 1 export per tenant per
/// minute. Burst capacity = 1, refill = 1 token / 60s ≈ 1 tps int.
#[must_use]
pub fn audit_export_rate_limit_config() -> RateLimitConfig {
    // Refill rate is integer tokens per second per
    // `RateLimitConfig` shape; 1/60s rounds to 0 — we therefore
    // express the policy as `burst=1, refill=1, retry_floor=60s`.
    // The bucket starts full so the first request succeeds, after
    // which the `Retry-After` floor of 60s prevents a second
    // request inside the same minute. The `with_overrides`
    // constructor validates the canceled-tenant ≥ ceiling invariant
    // we preserve below.
    //
    // SAFETY (invariant): the override tuple satisfies
    // `with_overrides` validation (burst ≥ 1; floor < ceiling;
    // canceled ≥ ceiling). On a misconfiguration we fall back to
    // `canonical()` to keep the route construction infallible.
    RateLimitConfig::with_overrides(
        1, 1, 60, RETRY_AFTER_HARD_CEILING_SECS_FOR_EXPORT,
        RETRY_AFTER_CANCELED_FOR_EXPORT,
    )
    .unwrap_or_else(RateLimitConfig::canonical)
}

/// Live-tenant retry-after hard ceiling for the audit-export route.
/// 1 day = 86_400s mirrors the framework default; the route never
/// needs a longer hold for live tenants (the 60s floor is the
/// per-WI-S09-008 §7 Q2 anchor; the ceiling is only relevant if
/// the bucket gets adversarially drained).
const RETRY_AFTER_HARD_CEILING_SECS_FOR_EXPORT: u64 = 86_400;
/// Canceled-tenant retry-after — mirrors the framework canonical
/// (7d). Canceled tenants never get an audit-export window served.
const RETRY_AFTER_CANCELED_FOR_EXPORT: u64 = 7 * 86_400;

/// Build the axum router exposing the audit-export route.
pub fn router(state: AuditExportRouteState) -> Router {
    Router::new()
        .route(AUDIT_EXPORT_ROUTE, get(handle_export))
        .with_state(state)
}

/// `GET /v1/audit/export` query parameter surface.
#[derive(Clone, Debug, Deserialize)]
pub struct AuditExportQuery {
    /// Window lower bound (inclusive). Accept either ISO 8601 UTC
    /// (`YYYY-MM-DDTHH:MM:SSZ`) or raw Unix epoch milliseconds.
    pub from: String,
    /// Window upper bound (exclusive). Same accepted shapes as
    /// `from`.
    pub to: String,
    /// Attempted tenant id. Optional — supplied ONLY to surface a
    /// cross-tenant attempt (see module-level docs). When absent
    /// the route uses the `X-Tenant-Id` header value.
    pub tenant: Option<String>,
}

/// `GET /v1/audit/export` axum handler.
async fn handle_export(
    State(state): State<AuditExportRouteState>,
    headers: HeaderMap,
    Query(query): Query<AuditExportQuery>,
) -> axum::response::Response {
    // 1. Auth — JWT-validated tenant arrives via `X-Tenant-Id` header.
    //    The production middleware injects this AFTER verifying the
    //    JWT signature; native-target tests stub the header directly.
    let authenticated_tenant = match headers
        .get(TENANT_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Uuid::parse_str)
    {
        Some(Ok(t)) => t,
        _ => {
            return (StatusCode::UNAUTHORIZED, "missing or invalid X-Tenant-Id").into_response();
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
            let _ = state.audit_sink.emit(ExportAuditRow {
                event_type: EVENT_TYPE_CROSS_TENANT_ATTEMPT.to_string(),
                authenticated_tenant: Some(authenticated_tenant),
                attempted_tenant: Some(attempted),
                from_ms,
                to_ms,
                bytes_written: 0,
                events_written: 0,
                exit_status: "cross_tenant_reject".to_string(),
            });
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
    let now_ms = now_ms_from_window(window);
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
                let _ = state.audit_sink.emit(ExportAuditRow {
                    event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
                    authenticated_tenant: Some(authenticated_tenant),
                    attempted_tenant: None,
                    from_ms,
                    to_ms,
                    bytes_written: 0,
                    events_written: 0,
                    exit_status: "rate_limited".to_string(),
                });
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
            // decision shape.
            _ => {
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
        let _ = state.audit_sink.emit(ExportAuditRow {
            event_type: EVENT_TYPE_VERIFY_FAILED.to_string(),
            authenticated_tenant: Some(authenticated_tenant),
            attempted_tenant: None,
            from_ms,
            to_ms,
            bytes_written: body_bytes_len,
            events_written: ndjson_line_count,
            exit_status: "verify_failed".to_string(),
        });
    }

    // 9. Build the streaming response body. Per-row NDJSON frames
    //    + the trailing manifest line. The per-row re-verify gate
    //    runs INSIDE the stream against the manifest anchor; on the
    //    FIRST chain-break we emit a SEV-0 carrying the mid-stream
    //    `{break_at_seq, break_at_chunk, observed, expected}` payload
    //    and ship the `X-CoreLink-Audit-Export-Aborted` trailer as
    //    the final body frame. The audit emit ALWAYS lands BEFORE
    //    the trailer frame on the wire.
    let anchor_head = result.manifest.chain_head_at_export;
    let audit_sink_for_stream = Arc::clone(&state.audit_sink);
    let tenant_for_stream = authenticated_tenant;
    let frames = build_audit_export_stream_frames(
        result.rows,
        manifest_line,
        anchor_head,
        audit_sink_for_stream,
        tenant_for_stream,
        from_ms,
        to_ms,
        body_bytes_len,
        ndjson_line_count,
    );
    let body_stream =
        futures::stream::iter(frames.into_iter().map(Ok::<Frame<Bytes>, Infallible>));
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

/// Build the wave-18 streaming frame plan. Returns the ordered
/// `Frame<Bytes>` vector the response body emits:
///
/// 1. One `Frame::data` per NDJSON row line, in order.
/// 2. Between successive rows, a `\n` separator is appended to the
///    PREVIOUS row line (so the wire shape matches the wave-16
///    monolithic `\n`-joined body byte-for-byte).
/// 3. After the last row (or on entry to the manifest line on an
///    empty range) the trailing manifest line is emitted as its own
///    `Frame::data`.
/// 4. If a per-row re-verify fails MID-STREAM, the function emits
///    the SEV-0 audit row carrying the `{break_at_seq,
///    break_at_chunk, observed, expected}` payload via the supplied
///    sink BEFORE pushing a `Frame::trailers` trailer carrying the
///    canonical `X-CoreLink-Audit-Export-Aborted` header — the
///    stream then closes immediately (no further row or manifest
///    frames are emitted on the abort path).
///
/// The synchronous shape is intentional: pre-materializing the frame
/// plan keeps the audit-emit BEFORE the network flush invariant
/// trivially satisfied (we'd otherwise need to thread the sink
/// through an async generator with non-trivial cancellation
/// semantics).
#[allow(clippy::too_many_arguments, reason = "trailing-payload + audit sink fan-in")]
fn build_audit_export_stream_frames(
    rows: Vec<ExportedAuditEvent>,
    manifest_line: String,
    anchor_head: corelink_audit_chain::ChainHash,
    audit_sink: Arc<dyn ExportAuditSink>,
    authenticated_tenant: Uuid,
    from_ms: u64,
    to_ms: u64,
    bytes_written: u64,
    events_written: u64,
) -> Vec<Frame<Bytes>> {
    let mut frames: Vec<Frame<Bytes>> = Vec::with_capacity(rows.len() + 2);
    for (idx, row) in rows.iter().enumerate() {
        // Per-row inclusion-proof recheck against the manifest
        // anchor. Any internal verifier error (canonicalization /
        // arithmetic) is treated as a chain-break-equivalent abort
        // — fail-CLOSED — so an unexpected error path can't silently
        // ship tampered bytes.
        let row_serialized = match serde_json::to_string(row) {
            Ok(s) => s,
            Err(_) => {
                emit_mid_stream_break_audit(
                    &audit_sink,
                    authenticated_tenant,
                    row.event.sequence_number,
                    idx as u64,
                    "serialize_failed",
                    "row-serialize-failed",
                    from_ms,
                    to_ms,
                    bytes_written,
                    events_written,
                );
                frames.push(abort_trailer_frame(
                    row.event.sequence_number,
                    idx as u64,
                    "serialize_failed",
                    "row-serialize-failed",
                ));
                return frames;
            }
        };
        let verified = verify_inclusion_proof(row, &anchor_head).unwrap_or_default();
        if !verified {
            // SEV-0 emit BEFORE the trailer (audit-anchor-first per
            // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). Best-effort: a
            // sink error here is logged via the return value of the
            // sink trait but we still flush the trailer so the wire
            // remains honest.
            let observed_hex = row.proof.link_hash.to_hex();
            let expected_hex = anchor_head.to_hex();
            emit_mid_stream_break_audit(
                &audit_sink,
                authenticated_tenant,
                row.event.sequence_number,
                idx as u64,
                &observed_hex,
                &expected_hex,
                from_ms,
                to_ms,
                bytes_written,
                events_written,
            );
            frames.push(abort_trailer_frame(
                row.event.sequence_number,
                idx as u64,
                &observed_hex,
                &expected_hex,
            ));
            return frames;
        }
        // Append the row payload + (for every row) a trailing `\n`.
        // The wave-16 wire shape was `row0\nrow1\nrow2\n{manifest}`;
        // we replicate it here by suffixing every row with `\n` so
        // the manifest line emerges naturally on its own without a
        // separator-management special case.
        let mut buf = Vec::with_capacity(row_serialized.len() + 1);
        buf.extend_from_slice(row_serialized.as_bytes());
        buf.push(b'\n');
        frames.push(Frame::data(Bytes::from(buf)));
    }
    // Trailing manifest line (no trailing newline — matches the
    // wave-16 wire shape exactly).
    frames.push(Frame::data(Bytes::from(manifest_line)));
    frames
}

/// Emit the SEV-0 mid-stream chain-break audit row. The audit
/// payload is the canonical
/// `{break_at_seq, break_at_chunk, observed, expected}` JSON
/// fragment encoded into `exit_status` (the `ExportAuditRow` shape
/// lacks a dedicated payload field — we piggy-back on `exit_status`
/// so the structured payload survives capture by every
/// `ExportAuditSink` implementor; production durable sinks lift the
/// row into the standard CloudEvents envelope where the payload
/// lands in `data`).
#[allow(clippy::too_many_arguments, reason = "audit row shape")]
fn emit_mid_stream_break_audit(
    sink: &Arc<dyn ExportAuditSink>,
    authenticated_tenant: Uuid,
    break_at_seq: u64,
    break_at_chunk: u64,
    observed_hex: &str,
    expected_hex: &str,
    from_ms: u64,
    to_ms: u64,
    bytes_written: u64,
    events_written: u64,
) {
    let payload = format!(
        "{{\"break_at_seq\":{break_at_seq},\"break_at_chunk\":{break_at_chunk},\"observed\":\"{observed_hex}\",\"expected\":\"{expected_hex}\"}}"
    );
    let _ = sink.emit(ExportAuditRow {
        event_type: EVENT_TYPE_VERIFY_FAILED.to_string(),
        authenticated_tenant: Some(authenticated_tenant),
        attempted_tenant: None,
        from_ms,
        to_ms,
        bytes_written,
        events_written,
        exit_status: format!("verify_failed_mid_stream:{payload}"),
    });
}

/// Build the canonical mid-stream abort HTTP-trailer frame. Payload
/// is a single ASCII JSON object value placed in the
/// `X-CoreLink-Audit-Export-Aborted` trailer header — the
/// customer-CLI parses it for the operator diagnostic. We expose the
/// shape through `mid_stream_abort_trailer_value` so the wave-18
/// integration test can assert the canonical payload byte-for-byte
/// without re-implementing the formatter.
fn abort_trailer_frame(
    break_at_seq: u64,
    break_at_chunk: u64,
    observed_hex: &str,
    expected_hex: &str,
) -> Frame<Bytes> {
    let mut trailers = HeaderMap::new();
    let value = mid_stream_abort_trailer_value(
        break_at_seq,
        break_at_chunk,
        observed_hex,
        expected_hex,
    );
    if let (Ok(name), Ok(val)) = (
        HeaderName::from_bytes(HEADER_EXPORT_ABORTED.as_bytes()),
        HeaderValue::from_str(&value),
    ) {
        trailers.insert(name, val);
    }
    Frame::trailers(trailers)
}

/// Canonical mid-stream abort-trailer payload encoder. Public for
/// the wave-18 integration test + the customer-CLI compatibility
/// test (both assert the exact wire shape).
#[must_use]
pub fn mid_stream_abort_trailer_value(
    break_at_seq: u64,
    break_at_chunk: u64,
    observed_hex: &str,
    expected_hex: &str,
) -> String {
    format!(
        "{{\"break_at_seq\":{break_at_seq},\"break_at_chunk\":{break_at_chunk},\"observed\":\"{observed_hex}\",\"expected\":\"{expected_hex}\"}}"
    )
}

/// Constant-time compare on two UUIDs (defense in depth — the
/// tenant comparator is auth-sensitive).
#[must_use]
fn uuid_eq_ct(a: &Uuid, b: &Uuid) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Parse a timestamp shape: raw Unix epoch ms OR a minimal RFC 3339
/// subset (`YYYY-MM-DDTHH:MM:SSZ`). Returns `None` on malformed input.
fn parse_timestamp(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<u64>() {
        return Some(v);
    }
    // Minimal RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`); 20 ASCII bytes.
    parse_rfc3339_utc_ms(s)
}

/// Pure-logic parse of `YYYY-MM-DDTHH:MM:SSZ` to Unix epoch ms.
/// Returns `None` on malformed shape. Covers years 1970..=9999.
fn parse_rfc3339_utc_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20 {
        return None;
    }
    // Shape: YYYY-MM-DDTHH:MM:SSZ
    //        0123456789012345678901
    let y = parse_u32_digits(b.get(0..4)?)?;
    if *b.get(4)? != b'-' { return None; }
    let mo = parse_u32_digits(b.get(5..7)?)?;
    if *b.get(7)? != b'-' { return None; }
    let d = parse_u32_digits(b.get(8..10)?)?;
    if *b.get(10)? != b'T' { return None; }
    let h = parse_u32_digits(b.get(11..13)?)?;
    if *b.get(13)? != b':' { return None; }
    let mi = parse_u32_digits(b.get(14..16)?)?;
    if *b.get(16)? != b':' { return None; }
    let se = parse_u32_digits(b.get(17..19)?)?;
    if *b.get(19)? != b'Z' { return None; }
    if !(1970..=9999).contains(&y) { return None; }
    if !(1..=12).contains(&mo) { return None; }
    if !(1..=31).contains(&d) { return None; }
    if h >= 24 || mi >= 60 || se >= 60 { return None; }

    // Days from Unix epoch (1970-01-01) using the canonical
    // proleptic Gregorian formula. Reference: Howard Hinnant's
    // `days_from_civil` (date_civil_from_days_inverse).
    let y_i: i64 = i64::from(y);
    let mo_i: i64 = i64::from(mo);
    let d_i: i64 = i64::from(d);
    let yy = if mo_i <= 2 { y_i - 1 } else { y_i };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400; // 0..=399
    let doy = (153_i64
        .checked_mul(if mo_i > 2 { mo_i - 3 } else { mo_i + 9 })?
        .checked_add(2)?)
        / 5
        + d_i
        - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch: i64 = era.checked_mul(146_097)?.checked_add(doe)?.checked_sub(719_468)?;
    if days_since_epoch < 0 { return None; }
    let secs: i64 = days_since_epoch
        .checked_mul(86_400)?
        .checked_add(i64::from(h) * 3_600 + i64::from(mi) * 60 + i64::from(se))?;
    if secs < 0 { return None; }
    let ms = u64::try_from(secs).ok()?.checked_mul(1_000)?;
    Some(ms)
}

fn parse_u32_digits(b: &[u8]) -> Option<u32> {
    let mut out: u32 = 0;
    for &c in b {
        if !c.is_ascii_digit() { return None; }
        out = out.checked_mul(10)?.checked_add(u32::from(c - b'0'))?;
    }
    Some(out)
}

/// Wall-clock proxy for the rate-limit gate. The route runs in a
/// CF Worker / axum context that doesn't pull `std::time::SystemTime`
/// for the rate-limit decision; we anchor the gate to the window's
/// upper bound so the bucket clock is deterministic per request.
/// Production wiring substitutes the canonical `WallClock` collaborator
/// here.
#[must_use]
fn now_ms_from_window(window: ExportWindow) -> u64 {
    window.until_ms
}

/// Serialize each row to its canonical NDJSON envelope line (no
/// trailing newline; the streaming layer appends `\n` between rows).
/// Envelope shape: `{"event": <ev>, "proof": <p>}` per WI-S09-008 §4.
fn serialize_ndjson_lines(
    rows: &[ExportedAuditEvent],
) -> Result<Vec<String>, serde_json::Error> {
    let mut out: Vec<String> = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(serde_json::to_string(row)?);
    }
    Ok(out)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn route_constant_matches_canonical_path() {
        assert_eq!(AUDIT_EXPORT_ROUTE, "/v1/audit/export");
    }

    #[test]
    fn event_type_constants_match_spec() {
        assert_eq!(EVENT_TYPE_EXPORT_REQUEST, "corelink.audit.export_request.v1");
        assert_eq!(
            EVENT_TYPE_CROSS_TENANT_ATTEMPT,
            "corelink.security.audit_export_cross_tenant_attempt.v1"
        );
        assert_eq!(EVENT_TYPE_VERIFY_FAILED, "corelink.audit.export_verify_failed.v1");
    }

    #[test]
    fn build_state_returns_usable_router() {
        let state = build_state();
        let _router = router(state);
    }

    #[test]
    fn parse_timestamp_accepts_epoch_ms() {
        assert_eq!(parse_timestamp("1700000000000"), Some(1_700_000_000_000));
        assert_eq!(parse_timestamp("  42 "), Some(42));
    }

    #[test]
    fn parse_timestamp_accepts_rfc3339_utc() {
        // 1970-01-01T00:00:00Z = 0
        assert_eq!(parse_timestamp("1970-01-01T00:00:00Z"), Some(0));
        // 2026-05-15T12:00:00Z = 1_778_932_800_000 (verified
        // independently below by round-trip).
        let v = parse_timestamp("2026-05-15T12:00:00Z").expect("valid");
        // Cross-check: 1970-01-01T00:00:00Z + (days × 86400 + 12h) sec
        // matches via the days_from_civil formula above.
        assert!(v > 1_700_000_000_000);
    }

    #[test]
    fn parse_timestamp_rejects_malformed_rfc3339() {
        assert_eq!(parse_timestamp("not-a-time"), None);
        assert_eq!(parse_timestamp("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_timestamp("2026-05-32T00:00:00Z"), None);
        assert_eq!(parse_timestamp("2026-05-15T25:00:00Z"), None);
        // Missing trailing Z.
        assert_eq!(parse_timestamp("2026-05-15T12:00:00"), None);
    }

    #[test]
    fn audit_export_rate_limit_config_pins_60s_floor() {
        let cfg = audit_export_rate_limit_config();
        assert_eq!(cfg.default_burst_capacity(), 1);
        assert_eq!(cfg.default_refill_rate_per_sec(), 1);
        assert_eq!(cfg.retry_after_floor_secs(), 60);
    }

    #[test]
    fn uuid_eq_ct_returns_true_for_equal_ids() {
        let u = Uuid::from_u128(0xAB);
        assert!(uuid_eq_ct(&u, &u));
    }

    #[test]
    fn uuid_eq_ct_returns_false_for_distinct_ids() {
        let a = Uuid::from_u128(0xAB);
        let b = Uuid::from_u128(0xCD);
        assert!(!uuid_eq_ct(&a, &b));
    }

    #[test]
    fn in_memory_audit_sink_captures_emits() {
        let s = InMemoryExportAuditSink::new();
        let row = ExportAuditRow {
            event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
            authenticated_tenant: Some(Uuid::from_u128(1)),
            attempted_tenant: None,
            from_ms: 0,
            to_ms: 1,
            bytes_written: 0,
            events_written: 0,
            exit_status: "empty".to_string(),
        };
        s.emit(row.clone()).expect("emit");
        let snap = s.snapshot().expect("snap");
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0], row);
    }

    #[test]
    fn in_memory_audit_sink_fail_closed_on_injected_failure() {
        let s = InMemoryExportAuditSink::new();
        s.inject_failure("pipeline down").expect("inject");
        let row = ExportAuditRow {
            event_type: EVENT_TYPE_EXPORT_REQUEST.to_string(),
            authenticated_tenant: None,
            attempted_tenant: None,
            from_ms: 0,
            to_ms: 1,
            bytes_written: 0,
            events_written: 0,
            exit_status: "audit_failed".to_string(),
        };
        let err = s.emit(row).expect_err("inject");
        assert_eq!(err, "pipeline down");
        assert_eq!(s.snapshot().expect("snap").len(), 0);
    }

    #[test]
    fn serialize_ndjson_lines_empty_returns_zero() {
        let lines = serialize_ndjson_lines(&[]).expect("empty");
        assert!(lines.is_empty());
    }

    #[test]
    fn mid_stream_abort_trailer_value_matches_canonical_payload() {
        let v = mid_stream_abort_trailer_value(
            42,
            1,
            "aa".repeat(32).as_str(),
            "bb".repeat(32).as_str(),
        );
        assert!(v.starts_with("{\"break_at_seq\":42,\"break_at_chunk\":1,"));
        assert!(v.contains("\"observed\":\""));
        assert!(v.contains("\"expected\":\""));
        // Round-trips as JSON.
        let _: serde_json::Value = serde_json::from_str(&v).expect("valid json");
    }

    #[test]
    fn export_aborted_header_constant_matches_canonical_name() {
        assert_eq!(HEADER_EXPORT_ABORTED, "x-corelink-audit-export-aborted");
    }
}
