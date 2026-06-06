//! HTTP REST surface for CAS read (`GET /v1/cas/<digest>`).
//!
//! Per WI-S02-001 §6.1.2 the HTTP surface is a 1:1 wrap of the gRPC
//! `ByteStream::Read` orchestration: the same auth + AuthZ + R2 GET
//! pipeline, surfaced over HTTP/1.1 / HTTP/2 for browsers + curl +
//! admin tools. The handler reuses [`crate::read::CasReadOrchestrator`]
//! verbatim — there is no second copy of the AuthZ logic.
//!
//! ## Auth
//!
//! Same `Authorization: Bearer <token>` header semantics as gRPC; the
//! [`crate::pat::PatValidator`] is invoked identically. The HTTP
//! surface does NOT accept query-string tokens (URL-leak risk).
//!
//! ## Response shape
//!
//! - `200 OK` — `Content-Type: application/octet-stream`, body is the
//!   blob. Bytes are streamed in 1 MiB chunks via the response body
//!   (axum buffers up to the chunk size in memory; bounded peak).
//! - `400 Bad Request` — malformed digest path segment (not 64 lowercase
//!   hex chars).
//! - `401 Unauthorized` — `COR_AUTH_PAT_INVALID`.
//! - `403 Forbidden` — `COR_AUTH_SCOPE_INSUFFICIENT` (PAT scope lacks
//!   `cache:r`).
//! - `404 Not Found` — `COR_CAS_BLOB_NOT_FOUND` (uniform per ADR-0028
//!   for all [`crate::read::MissReason`] variants).
//! - `500 Internal Server Error` — `COR_INTERNAL` (programmer / region
//!   misroute / blob_meta size mismatch).
//! - `503 Service Unavailable` — `COR_SERVICE_DEGRADED` (R2 / D1
//!   transient fault).
//!
//! Every non-200 response carries an `x-corelink-error-code` header
//! mirroring the gRPC `metadata` channel so tooling can trigger
//! retries on the same canonical taxonomy.
//!
//! ## Range headers
//!
//! Per WI §7 (anti-scope) HTTP `Range` headers are NOT supported in
//! S-02 — REAPI ByteStream::Read carries the canonical `read_offset` /
//! `read_limit` semantics; the HTTP surface is a "download whole blob"
//! convenience. A `Range:` header on the request is silently ignored
//! (NOT an error — that would break existing browsers / curl); the
//! response is unconditional 200 with the full body.

#![allow(
    clippy::result_large_err,
    reason = "axum response handlers consume tonic::Status indirectly; sized by tonic"
)]

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use corelink_auth::middleware::{MissArm, MissMarker, TimingPaddingLayer};
use corelink_cas::r2_storage::R2Backend;
use corelink_hash::Digest;
use corelink_meta::MetaStore;

use crate::error_map::{
    miss_mapping, ReadErrorMapping, COR_AUTH_PAT_INVALID, COR_AUTH_SCOPE_INSUFFICIENT,
    COR_CAS_BAD_DIGEST, COR_CAS_BAD_RESOURCE_NAME,
};
use crate::handler::{Clock, HandlerCore};
use crate::orchestrator::OrphanReconciler;
use crate::pat::{AuthScope, AuthStubError, PatValidator};
use crate::read::{CasReadOrchestrator, ReadOutcome};

/// Shared state passed into the axum handler. Wraps an
/// `Arc<HandlerCore>` so all dependencies (auth + reader + meta + TDK)
/// are available without a second wiring layer.
pub struct HttpReadState<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    /// Shared handler core; inherits the same wiring as the gRPC stack.
    pub core: Arc<HandlerCore<V, B, M, R, C>>,
}

impl<V, B, M, R, C> Clone for HttpReadState<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

impl<V, B, M, R, C> std::fmt::Debug for HttpReadState<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpReadState").finish_non_exhaustive()
    }
}

/// Build an axum `Router` exposing `GET /v1/cas/:digest` with the
/// canonical [`TimingPaddingLayer`] applied (WI-S02-004 / ADR-0023
/// constant-time 404 [`crate::read::MissReason`] parity defense).
///
/// The layer mounts at the router boundary so EVERY 404 response
/// (regardless of which arm of the orchestrator emitted it: digest
/// parse, AuthZ scope, [`crate::read::MissReason::NeverExisted`], `Tombstoned`,
/// `R2OrphanRow`) flows through the padding pipeline. `200 OK` and
/// `4xx≠404` / `5xx` responses are passed through unchanged.
///
/// Mount via:
/// ```ignore
/// let app = corelink_reapi::cas_get_router(state);
/// axum::serve(listener, app).await?;
/// ```
pub fn cas_get_router<V, B, M, R, C>(state: HttpReadState<V, B, M, R, C>) -> Router
where
    V: PatValidator + Clone + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    cas_get_router_with_padding(state, TimingPaddingLayer::canonical())
}

/// Variant of [`cas_get_router`] that accepts an explicit
/// [`TimingPaddingLayer`] (canonical-defaults-overridden config /
/// jitter policy / predicate kind). Production callers use the
/// [`cas_get_router`] convenience; tests + chaos suites use this to
/// inject deterministic / disabled / tighter padding.
pub fn cas_get_router_with_padding<V, B, M, R, C>(
    state: HttpReadState<V, B, M, R, C>,
    padding_layer: TimingPaddingLayer,
) -> Router
where
    V: PatValidator + Clone + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    // Canonical predicate is `PredicateKind::Any` (the
    // `TimingPaddingLayer::canonical` default) for the REST stack so
    // a future refactor that rewrites 404 → 200 + body via a router-
    // level `map_response` would still trigger padding via the
    // `MissMarker` extension that `handle_cas_get` inserts on every
    // miss path.
    Router::new()
        .route("/v1/cas/:digest", get(handle_cas_get::<V, B, M, R, C>))
        .layer(padding_layer)
        .with_state(state)
}

async fn handle_cas_get<V, B, M, R, C>(
    State(state): State<HttpReadState<V, B, M, R, C>>,
    headers: HeaderMap,
    Path(digest_hex): Path<String>,
) -> Response
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    // Step 0 — Accept header content negotiation per WI §6.1.2 +
    // RFC 7230 §3.2.2 + RFC 7231 §5.3.2. Codex round-2 P3 fix: parse
    // Accept properly (q=0 exclusions, application/* wildcards,
    // multiple types) so explicit `Accept: application/octet-stream;q=0`
    // returns 406. Codex round-3 P3 fix: handle repeated `Accept:`
    // headers — RFC 7230 §3.2.2 treats them as a single comma-joined
    // list, so we collect all values and feed the joined string into
    // `accept_allows_octet_stream`.
    let accept_iter: Vec<&str> = headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    if !accept_iter.is_empty() {
        let joined = accept_iter.join(", ");
        if !accept_allows_octet_stream(&joined) {
            return error_response(
                StatusCode::NOT_ACCEPTABLE,
                COR_CAS_BAD_RESOURCE_NAME,
                "Accept header excludes application/octet-stream",
            );
        }
    }

    // Step 1 — extract bearer token.
    let token = match extract_bearer_http(&headers) {
        Ok(t) => t,
        Err(()) => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                COR_AUTH_PAT_INVALID,
                "PAT invalid or expired",
            );
        }
    };
    let request_id = extract_request_id_http(&headers);

    // Step 2 — authenticate.
    let pat_ctx = match state.core.pat().authenticate(&token, &request_id) {
        Ok(ctx) => ctx,
        Err(AuthStubError::PatInvalid) => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                COR_AUTH_PAT_INVALID,
                "PAT invalid or expired",
            );
        }
        Err(AuthStubError::ScopeInsufficient { .. }) => {
            return error_response(
                StatusCode::FORBIDDEN,
                COR_AUTH_SCOPE_INSUFFICIENT,
                "PAT scope insufficient",
            );
        }
    };

    // Step 3 — require cache:r scope.
    if pat_ctx.require_scope(AuthScope::CacheRead).is_err() {
        return error_response(
            StatusCode::FORBIDDEN,
            COR_AUTH_SCOPE_INSUFFICIENT,
            "PAT scope insufficient (cache:r required)",
        );
    }

    // Step 4 — parse digest path segment (64 lowercase hex chars).
    let digest = match Digest::from_hex(&digest_hex) {
        Ok(d) => d,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                COR_CAS_BAD_DIGEST,
                "digest hex is malformed (expected 64 lowercase hex chars)",
            );
        }
    };

    // Step 5 — derive storage ctx.
    let storage_ctx =
        corelink_worker::TenantCtx::new(state.core.tdk(), pat_ctx.tenant_id(), pat_ctx.region());

    // Step 6 — orchestrate.
    let orch = CasReadOrchestrator::new(state.core.reader(), state.core.meta());
    let outcome = match orch.read_blob(&storage_ctx, &digest).await {
        Ok(o) => o,
        Err(e) => {
            let m = e.mapping();
            return error_response(grpc_status_to_http(m.grpc_code), m.taxonomy_code, m.message);
        }
    };

    let (body, size_bytes) = match outcome {
        ReadOutcome::Hit { body, size_bytes } => (body, size_bytes),
        ReadOutcome::NotFound(reason) => {
            // Forensic-only audit: client sees the SAME 404 as a never-
            // existed digest. The `reason` discriminator emits a
            // structured CE envelope identical to the gRPC handler's
            // emitter — codex round-1 P1(b) fix: HTTP and gRPC now
            // share `corelink-audit` envelope shape + event_type
            // splits (NeverExisted → corelink.cas.read_miss [info];
            // Tombstoned → corelink.cas.tombstoned_read_attempt [info];
            // R2OrphanRow → corelink.cas.r2_orphan_detected [error]).
            // The SEV-1-driving REAPI_CROSS_TENANT_ATTEMPT is reserved
            // for the offline S-09 chain consumer per round-4 P2 split.
            crate::handler::emit_read_miss_audit_pub(
                &storage_ctx,
                pat_ctx.principal_id(),
                pat_ctx.region(),
                pat_ctx.request_id(),
                &digest,
                reason,
            );
            let m = miss_mapping();
            // Codex round-5 P1 fix: thread the MissArm through the
            // MissMarker extension so the timing-padding emit hook
            // can attribute the per-request padded latency to the
            // canonical arm — the load-bearing field for the S-09
            // pairwise-medians aggregation that derives
            // `corelink_cas_side_channel_timing_diff_ms`.
            let arm = miss_reason_to_arm(reason);
            return error_response_with_arm(
                StatusCode::NOT_FOUND,
                m.taxonomy_code,
                m.message,
                Some(arm),
            );
        }
    };

    // Step 7 — defense-in-depth size sanity check.
    if body.len() as u64 != size_bytes {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::error_map::COR_INTERNAL,
            "blob_meta size_bytes does not match R2 body length",
        );
    }

    // Step 8 — stream the body via a Drop-guarded chunk stream so the
    // `read_completed` audit envelope is emitted ONLY after the last
    // chunk has been polled by the client; client-cancellation /
    // partial-read paths emit `corelink.cas.read_aborted` instead via
    // the guard's Drop impl. Codex round-2 P2 fix: HTTP and gRPC now
    // both have post-stream audit semantics.

    let body_len = body.len();
    let body = chunked_http_body_with_audit(
        body,
        HttpReadAuditTail {
            tenant_id: storage_ctx.tenant_id(),
            principal_id: pat_ctx.principal_id(),
            region: pat_ctx.region(),
            client_request_id: pat_ctx.request_id().to_owned(),
            digest,
            size_bytes,
            payload_len: body_len as u64,
        },
    );

    let mut resp = Response::new(body);
    *resp.status_mut() = StatusCode::OK;
    let headers_mut = resp.headers_mut();
    headers_mut.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(cl) = HeaderValue::from_str(&body_len.to_string()) {
        headers_mut.insert(header::CONTENT_LENGTH, cl);
    }
    if let Ok(d) = HeaderValue::from_str(&format!("blake3:{}", digest.to_hex())) {
        headers_mut.insert("x-corelink-digest", d);
    }
    resp
}

/// Audit emission tail for HTTP — mirrors the gRPC `ReadAuditTail`.
/// Captured into [`chunked_http_body_with_audit`] so the
/// `read_completed` audit envelope is emitted post-stream-completion,
/// with the actually-delivered `bytes_sent` count. Client-cancellation
/// (browser back-button, curl ctrl+c, axum dropping the body) routes
/// through the `HttpReadAuditGuard` `Drop` path and emits
/// `corelink.cas.read_aborted` instead. Codex round-2 P2 fix.
struct HttpReadAuditTail {
    tenant_id: uuid::Uuid,
    principal_id: uuid::Uuid,
    region: corelink_worker::Region,
    client_request_id: String,
    digest: Digest,
    size_bytes: u64,
    payload_len: u64,
}

/// Build a chunked HTTP response body with a Drop-guarded audit-emit
/// tail. For S-01 single-blob 5 MiB cap the chunk size is
/// informational (one body frame is fine); we still chunk so the
/// future multipart read path's plumbing is in place.
fn chunked_http_body_with_audit(body: Bytes, audit: HttpReadAuditTail) -> Body {
    use futures::stream::StreamExt;
    let chunks = chunk_bytes(body, super::handler::READ_CHUNK_SIZE_BYTES);
    let total_chunks = chunks.len();
    let bytes_sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let bytes_sent_for_unfold = std::sync::Arc::clone(&bytes_sent);

    let chunk_iter = chunks.into_iter();
    let unfold_state = (chunk_iter, bytes_sent_for_unfold);

    let stream = futures::stream::unfold(unfold_state, |(mut iter, bytes_sent)| async move {
        match iter.next() {
            Some(c) => {
                bytes_sent.fetch_add(c.len() as u64, std::sync::atomic::Ordering::AcqRel);
                Some((Ok::<Bytes, std::io::Error>(c), (iter, bytes_sent)))
            }
            None => None,
        }
    });

    let guard = HttpReadAuditGuard {
        audit,
        bytes_sent,
        total_chunks,
        emitted: false,
    };
    let stream_with_guard = futures::stream::unfold(
        (
            Box::pin(stream)
                as std::pin::Pin<
                    Box<dyn futures::Stream<Item = Result<Bytes, std::io::Error>> + Send>,
                >,
            guard,
        ),
        |(mut s, mut guard)| async move {
            match s.as_mut().next().await {
                Some(item) => Some((item, (s, guard))),
                None => {
                    guard.emit_completion();
                    None
                }
            }
        },
    );
    Body::from_stream(stream_with_guard.boxed())
}

struct HttpReadAuditGuard {
    audit: HttpReadAuditTail,
    bytes_sent: std::sync::Arc<std::sync::atomic::AtomicU64>,
    total_chunks: usize,
    emitted: bool,
}

impl HttpReadAuditGuard {
    fn emit_completion(&mut self) {
        if self.emitted {
            return;
        }
        self.emitted = true;
        let bytes_sent = self.bytes_sent.load(std::sync::atomic::Ordering::Acquire);
        crate::handler::emit_read_completed_audit_pub(
            self.audit.tenant_id,
            self.audit.principal_id,
            self.audit.region,
            &self.audit.client_request_id,
            &self.audit.digest,
            self.audit.size_bytes,
            self.audit.payload_len,
            bytes_sent,
            self.total_chunks,
        );
    }
}

impl Drop for HttpReadAuditGuard {
    fn drop(&mut self) {
        if self.emitted {
            return;
        }
        let bytes_sent = self.bytes_sent.load(std::sync::atomic::Ordering::Acquire);
        // Codex round-3 P3 fix: include `region` for per-region
        // forensic triage parity with the gRPC `read_aborted` line.
        let region_label = match self.audit.region {
            corelink_worker::Region::Wnam => "wnam",
            corelink_worker::Region::Weur => "weur",
            corelink_worker::Region::Sam => "sam",
        };
        tracing::warn!(
            target: "corelink.audit",
            event_type = "corelink.cas.read_aborted",
            tenant = %self.audit.tenant_id,
            principal = %self.audit.principal_id,
            region = %region_label,
            digest = %self.audit.digest.to_hex(),
            size_bytes = self.audit.size_bytes,
            payload_len = self.audit.payload_len,
            bytes_sent = bytes_sent,
            "HTTP CAS read aborted (client cancelled stream before completion)"
        );
    }
}

/// Split a `Bytes` into 1 MiB chunks without re-allocating. Returns a
/// `Vec<Bytes>` so the chunk stream is `'static` (axum requires the
/// body type be `'static`).
fn chunk_bytes(body: Bytes, chunk_size: usize) -> Vec<Bytes> {
    if chunk_size == 0 || body.is_empty() {
        return if body.is_empty() { vec![] } else { vec![body] };
    }
    let mut out = Vec::with_capacity(body.len().div_ceil(chunk_size));
    let mut start = 0usize;
    while start < body.len() {
        let end = start.saturating_add(chunk_size).min(body.len());
        out.push(body.slice(start..end));
        start = end;
    }
    out
}

/// RFC 7231 §5.3.2 Accept header parser — returns true iff
/// `application/octet-stream` is acceptable to the client (q > 0).
///
/// The canonical algorithm:
/// 1. Split on `,` into media-range entries.
/// 2. For each entry: parse `type/subtype` + optional `;q=<n>` weight.
/// 3. Track best match across `application/octet-stream` (specific
///    match), `application/*` (wildcard subtype), `*/*` (wildcard).
/// 4. Specific match dominates wildcard; q=0 explicitly excludes the
///    matched range.
///
/// Empty Accept header (no `Accept:` at all) is treated as `*/*` per
/// the RFC default.
fn accept_allows_octet_stream(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return true;
    }
    // Track q for each level of specificity:
    //   - specific = `application/octet-stream`
    //   - wildcard_subtype = `application/*`
    //   - wildcard_all = `*/*`
    let mut specific: Option<f32> = None;
    let mut wildcard_subtype: Option<f32> = None;
    let mut wildcard_all: Option<f32> = None;
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.split(';').map(str::trim);
        let media = match parts.next() {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };
        let mut q: f32 = 1.0;
        for param in parts {
            // RFC 7231 §3.1.1.1 — parameter names are case-insensitive.
            // Codex round-4 P3 fix: `Q=0` must reject just like `q=0`.
            let lower = param.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("q=") {
                if let Ok(parsed) = rest.parse::<f32>() {
                    q = parsed;
                }
            }
        }
        // Decide specificity bucket.
        let media_lower = media.to_ascii_lowercase();
        if media_lower == "application/octet-stream" {
            specific = Some(specific.map_or(q, |old| old.max(q)));
        } else if media_lower == "application/*" {
            wildcard_subtype = Some(wildcard_subtype.map_or(q, |old| old.max(q)));
        } else if media_lower == "*/*" {
            wildcard_all = Some(wildcard_all.map_or(q, |old| old.max(q)));
        }
    }
    // Most-specific match wins. q > 0 ⇒ acceptable; q == 0 ⇒ explicit
    // exclusion. Absent ⇒ fall through to next level.
    if let Some(q) = specific {
        return q > 0.0;
    }
    if let Some(q) = wildcard_subtype {
        return q > 0.0;
    }
    if let Some(q) = wildcard_all {
        return q > 0.0;
    }
    // No relevant media-range matched — explicit Accept that doesn't
    // mention any of our types is a 406.
    false
}

fn extract_bearer_http(headers: &HeaderMap) -> Result<String, ()> {
    let raw = headers.get(header::AUTHORIZATION).ok_or(())?;
    let s = raw.to_str().map_err(|_| ())?.trim();
    let space_pos = s.find(' ').ok_or(())?;
    #[allow(
        clippy::indexing_slicing,
        reason = "byte indices come from str::find; always valid char boundaries"
    )]
    let scheme = &s[..space_pos];
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(());
    }
    #[allow(
        clippy::indexing_slicing,
        reason = "byte indices come from str::find; always valid char boundaries"
    )]
    let token = s[space_pos + 1..].trim_start();
    if token.is_empty() {
        return Err(());
    }
    Ok(token.to_owned())
}

fn extract_request_id_http(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string())
}

fn error_response(status: StatusCode, taxonomy_code: &'static str, message: &str) -> Response {
    error_response_with_arm(status, taxonomy_code, message, None)
}

fn error_response_with_arm(
    status: StatusCode,
    taxonomy_code: &'static str,
    message: &str,
    miss_arm: Option<MissArm>,
) -> Response {
    let mut resp = Response::new(Body::from(format!(
        "{{\"error_code\":\"{taxonomy_code}\",\"message\":\"{message}\"}}"
    )));
    *resp.status_mut() = status;
    if let Ok(v) = HeaderValue::from_str(taxonomy_code) {
        resp.headers_mut().insert("x-corelink-error-code", v);
    }
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // WI-S02-004 / ADR-0023: every 404 emitted by this handler is a
    // [`MissReason`] arm and MUST flow through the timing-padding
    // layer. Insert the canonical `MissMarker` extension (with the
    // optional `MissArm` discriminator when known) so the
    // `PredicateKind::Any` / `ExtensionMarker` predicates pad even
    // if a downstream transformer rewrites the status code, AND so
    // the timing-padding emit hook can attribute the padded
    // latency to the canonical arm.
    if status == StatusCode::NOT_FOUND {
        let marker = miss_arm.map(MissMarker::for_arm).unwrap_or_default();
        resp.extensions_mut().insert(marker);
    }
    resp
}

/// Map `corelink-reapi::read::MissReason` → canonical
/// `corelink-worker::middleware::MissArm` for the timing-padding
/// emit hook (WI-S02-004 §10.4.3 + ADR-0023 §"Operational métrica").
fn miss_reason_to_arm(reason: crate::read::MissReason) -> MissArm {
    match reason {
        crate::read::MissReason::NeverExisted => MissArm::NeverExisted,
        crate::read::MissReason::Tombstoned => MissArm::Tombstoned,
        crate::read::MissReason::R2OrphanRow => MissArm::R2OrphanRow,
    }
}

fn grpc_status_to_http(grpc_code: i32) -> StatusCode {
    match grpc_code {
        3 | 9 => StatusCode::BAD_REQUEST, // INVALID_ARGUMENT / FAILED_PRECONDITION
        5 => StatusCode::NOT_FOUND,       // NOT_FOUND
        7 => StatusCode::FORBIDDEN,       // PERMISSION_DENIED
        8 => StatusCode::PAYLOAD_TOO_LARGE, // RESOURCE_EXHAUSTED
        10 => StatusCode::CONFLICT,       // ABORTED
        11 => StatusCode::RANGE_NOT_SATISFIABLE, // OUT_OF_RANGE
        14 => StatusCode::SERVICE_UNAVAILABLE, // UNAVAILABLE
        16 => StatusCode::UNAUTHORIZED,   // UNAUTHENTICATED
        _ => StatusCode::INTERNAL_SERVER_ERROR, // INTERNAL / UNKNOWN
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn chunk_bytes_canonical() {
        let b = Bytes::from(vec![0u8; 3 * 1024 * 1024 + 7]);
        let chunks = chunk_bytes(b.clone(), 1024 * 1024);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].len(), 1024 * 1024);
        assert_eq!(chunks[3].len(), 7);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, b.len());
    }

    #[test]
    fn chunk_bytes_empty_body() {
        let chunks = chunk_bytes(Bytes::new(), 1024);
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_bytes_single_chunk() {
        let b = Bytes::from(vec![0u8; 1024]);
        let chunks = chunk_bytes(b.clone(), 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 1024);
    }

    #[test]
    fn extract_bearer_http_canonical() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert_eq!(extract_bearer_http(&h).unwrap(), "secret");
    }

    #[test]
    fn extract_bearer_http_case_insensitive() {
        for variant in ["Bearer", "bearer", "BEARER", "BeArEr"] {
            let mut h = HeaderMap::new();
            h.insert(
                header::AUTHORIZATION,
                format!("{variant} t").parse().unwrap(),
            );
            assert_eq!(extract_bearer_http(&h).unwrap(), "t");
        }
    }

    #[test]
    fn extract_bearer_http_rejects_basic() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().unwrap());
        assert!(extract_bearer_http(&h).is_err());
    }

    /// Pin the EXACT extracted token so the `s[space_pos + 1..]` slice
    /// arithmetic is asserted byte-for-byte. This kills the index-offset
    /// mutants on the token slice that the prefix-only assertions above
    /// miss:
    ///
    /// - `+` → `-` ⇒ `s[space_pos - 1..]` = `"r corelink-pat-AbC123"`
    ///   (the trailing `r` of the scheme leaks in); `.trim_start()` is a
    ///   no-op (leading char is `r`, not whitespace) ⇒ wrong token ⇒ killed.
    /// - literal `1` → `2` ⇒ `s[space_pos + 2..]` = `"orelink-pat-AbC123"`
    ///   (first token char is dropped) ⇒ wrong token ⇒ killed.
    ///
    /// (`+` → `*` and `1` → `0` both collapse to `s[space_pos..]`, whose
    /// only extra leading char is the matched ASCII space — `trim_start()`
    /// removes it, so those two are EQUIVALENT mutants, unkillable by any
    /// value assertion; documented so future readers don't chase a ghost.)
    #[test]
    fn extract_bearer_http_exact_token_value() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            "Bearer corelink-pat-AbC123".parse().unwrap(),
        );
        assert_eq!(extract_bearer_http(&h).unwrap(), "corelink-pat-AbC123");
    }

    /// Lock the scheme/token boundary from both sides:
    ///   - a single-char token immediately after the space (`"Bearer Z"`
    ///     ⇒ `"Z"`) pins the right edge of the slice;
    ///   - a token that itself contains interior spaces (`"a b c"`)
    ///     proves ONLY the scheme + its one delimiter space is stripped —
    ///     the `+` → `-` mutant would yield `"r a b c"` here ⇒ killed.
    #[test]
    fn extract_bearer_http_prefix_boundary_exact() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer Z".parse().unwrap());
        assert_eq!(extract_bearer_http(&h).unwrap(), "Z");

        let mut h2 = HeaderMap::new();
        h2.insert(header::AUTHORIZATION, "Bearer a b c".parse().unwrap());
        assert_eq!(extract_bearer_http(&h2).unwrap(), "a b c");
    }

    /// `extract_request_id_http` must echo the client-supplied
    /// `x-request-id` verbatim. Asserting the EXACT value kills both
    /// return-value mutants:
    ///   - `String::new()`  ⇒ returns `""`      ⇒ `!= "req-2026-..."` ⇒ killed.
    ///   - `"xyzzy".into()` ⇒ returns `"xyzzy"` ⇒ `!= "req-2026-..."` ⇒ killed.
    #[test]
    fn extract_request_id_http_present_exact() {
        let mut h = HeaderMap::new();
        h.insert("x-request-id", "req-2026-06-03-abc".parse().unwrap());
        assert_eq!(extract_request_id_http(&h), "req-2026-06-03-abc");
    }

    /// Absent `x-request-id` ⇒ a freshly minted UUIDv7 correlation id.
    /// We can't pin the exact value (it's random), but asserting it parses
    /// as a UUID is a tight-enough invariant to ALSO kill both mutants:
    ///   - `String::new()`  ⇒ `""`      ⇒ `Uuid::parse_str` fails ⇒ killed.
    ///   - `"xyzzy".into()` ⇒ `"xyzzy"` ⇒ `Uuid::parse_str` fails ⇒ killed.
    #[test]
    fn extract_request_id_http_absent_is_fresh_uuid() {
        let h = HeaderMap::new();
        let id = extract_request_id_http(&h);
        assert!(!id.is_empty());
        assert_ne!(id, "xyzzy");
        // A real UUIDv7 string round-trips through the parser.
        assert!(
            uuid::Uuid::parse_str(&id).is_ok(),
            "absent x-request-id must mint a parseable UUID, got {id:?}"
        );
    }

    #[test]
    fn grpc_status_to_http_canonical() {
        assert_eq!(grpc_status_to_http(5), StatusCode::NOT_FOUND);
        assert_eq!(grpc_status_to_http(7), StatusCode::FORBIDDEN);
        assert_eq!(grpc_status_to_http(16), StatusCode::UNAUTHORIZED);
        assert_eq!(grpc_status_to_http(14), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn accept_parser_canonical_cases() {
        // Canonical accept patterns.
        assert!(accept_allows_octet_stream(""));
        assert!(accept_allows_octet_stream("*/*"));
        assert!(accept_allows_octet_stream("application/*"));
        assert!(accept_allows_octet_stream("application/octet-stream"));
        assert!(accept_allows_octet_stream("application/octet-stream;q=0.9"));
        assert!(accept_allows_octet_stream("text/plain, application/*"));
        assert!(accept_allows_octet_stream(
            "text/plain;q=0.5, application/octet-stream;q=1.0"
        ));
        // Multiple-spaces / mixed-case.
        assert!(accept_allows_octet_stream("Application/Octet-Stream"));
    }

    #[test]
    fn accept_parser_rejects_explicit_exclusions() {
        // codex round-2 P3: q=0 exclusions MUST surface as 406.
        assert!(!accept_allows_octet_stream("application/octet-stream;q=0"));
        assert!(!accept_allows_octet_stream("application/*;q=0"));
        assert!(!accept_allows_octet_stream("*/*;q=0, text/plain"));
        // Specificity: explicit application/octet-stream;q=0
        // overrides */*;q=1.
        assert!(!accept_allows_octet_stream(
            "application/octet-stream;q=0, */*;q=1"
        ));
    }

    #[test]
    fn accept_parser_rejects_unrelated_types() {
        assert!(!accept_allows_octet_stream("text/plain"));
        assert!(!accept_allows_octet_stream("text/html, image/png"));
    }

    #[test]
    fn accept_parser_specificity_wins() {
        // application/* present, application/octet-stream present too
        // — specific match dominates.
        assert!(accept_allows_octet_stream(
            "application/*;q=0, application/octet-stream"
        ));
    }

    #[test]
    fn accept_parser_q_param_is_case_insensitive() {
        // Codex round-4 P3 fix: RFC 7231 §3.1.1.1 parameter names are
        // case-insensitive; uppercase `Q=` must be honored.
        assert!(!accept_allows_octet_stream("application/octet-stream;Q=0"));
        assert!(!accept_allows_octet_stream("application/*;Q=0"));
    }
}
