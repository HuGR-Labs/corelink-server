//! Axum router + `run_cargo_adapter` entrypoint.
//!
//! The router exposes three routes mirroring the sccache HTTP storage
//! backend wire (from <https://github.com/mozilla/sccache/blob/main/docs/HTTP.md>):
//!
//! - `GET  /<key>` — read artifact: 200 + bytes on hit; 404 on miss.
//! - `PUT  /<key>` — write artifact: 200 on success.
//! - `HEAD /<key>` — existence check: 200 on hit; 404 on miss.
//!
//! Every request flows through:
//!
//! 1. `extract_bearer` → PAT plaintext (HTTP 401 on mismatch);
//! 2. `resolve_tenant` → tenant id (HTTP 401);
//! 3. Route handler → CAS read / write via adapter-local `CasStore` port.
//!
//! ## Status code mapping
//!
//! | `CargoAdapterError` variant | HTTP | Why |
//! |---|---|---|
//! | `Auth`           | 401 | bad / missing PAT |
//! | `Cas`            | 502 | CAS dependency failure |
//! | `Audit`          | 503 | audit chokepoint failed (fail-CLOSED) |
//! | `BodyOversized`  | 413 | PUT body > `body_size_limit_bytes` |
//! | `Bind`           | n/a | bind failures from `run_cargo_adapter` directly |

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};

use crate::cargo::audit::AuditOrchestrator;
use crate::cargo::auth::{extract_bearer, resolve_tenant};
use crate::cargo::config::CargoAdapterConfig;
use crate::cargo::error::CargoAdapterError;
use crate::cargo::ports::{CasError, SharedCasStore, SharedTenantResolver};
use crate::cargo::translate::key_from_path;

/// Shared router state. `Clone`-cheap (every field is `Arc`-shaped).
#[derive(Clone)]
#[non_exhaustive]
pub struct CargoRouterState {
    cas: SharedCasStore,
    tenant_resolver: SharedTenantResolver,
    auditor: Arc<AuditOrchestrator>,
    body_size_limit_bytes: u64,
}

impl std::fmt::Debug for CargoRouterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CargoRouterState")
            .field("cas", &"Arc<dyn CasStore>")
            .field("tenant_resolver", &"Arc<dyn TenantResolver>")
            .field("body_size_limit_bytes", &self.body_size_limit_bytes)
            .finish()
    }
}

/// Hash a value to a short, stable hex correlation handle for logging
/// (INV-NO-PII-IN-LOGS). First 8 bytes of SHA-256, hex-encoded —
/// consistent with the `hash_for_log` convention in `internal_pat.rs`
/// and `durable_object.ts`. Never log the raw tenant UUID; log this
/// handle instead.
#[must_use]
fn hash_for_log(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(16);
    for b in digest.iter().take(8) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Build the axum [`Router`] without binding a listener — useful for
/// in-process tests (drive it directly via `tower::ServiceExt`).
pub fn build_router(config: CargoAdapterConfig) -> Router {
    let auditor = Arc::new(AuditOrchestrator::new(config.auditor.clone()));
    let state = CargoRouterState {
        cas: Arc::<dyn crate::cargo::ports::CasStore>::clone(&config.cas),
        tenant_resolver: Arc::<dyn crate::cargo::ports::TenantResolver>::clone(
            &config.tenant_resolver,
        ),
        auditor,
        body_size_limit_bytes: config.body_size_limit_bytes,
    };
    // axum 0.7: HEAD is served automatically by the GET handler when
    // not explicitly registered. We register an explicit HEAD handler
    // to avoid serving body bytes on HEAD requests.
    //
    // The route is a catch-all (`/*path`), not `/:key`, so the adapter can be
    // mounted UNCHANGED behind a container prefix (`/cargo/<tenant>/<key>`):
    // `handle_*` resolve the actual key via `key_from_path`, which already
    // takes the LAST path segment. In standalone sccache mode the client hits
    // `/<key>` (a single segment), which the catch-all matches identically.
    Router::new()
        .route("/*path", get(handle_get).put(handle_put).head(handle_head))
        .with_state(state)
}

/// Bind the configured [`SocketAddr`] and serve forever.
///
/// # Errors
///
/// Returns [`CargoAdapterError::Bind`] if the listener cannot bind, or
/// propagates any axum / hyper serve failure as `CargoAdapterError::Bind`.
pub async fn run_cargo_adapter(config: CargoAdapterConfig) -> Result<(), CargoAdapterError> {
    let bind_addr: SocketAddr = config.bind_addr;
    let router = build_router(config);
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(CargoAdapterError::Bind)?;
    tracing::info!(?bind_addr, "corelink-adapter-cargo listening");
    axum::serve(listener, router)
        .await
        .map_err(CargoAdapterError::Bind)?;
    Ok(())
}

/// `GET /<key>` — read artifact; 200 + bytes on hit, 404 on miss.
async fn handle_get(
    State(state): State<CargoRouterState>,
    Path(raw_key): Path<String>,
    headers: HeaderMap,
) -> Response {
    let key = match key_from_path(&raw_key) {
        Some(k) => k,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let pat = match extract_bearer(&headers) {
        Ok(pat) => pat,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    let tenant_id = match resolve_tenant(&state.tenant_resolver, &pat).await {
        Ok(t) => t,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    match state.cas.get(&tenant_id, &key).await {
        Ok(Some(bytes)) => {
            tracing::debug!(tenant_hash = %hash_for_log(&tenant_id), key = %key, "cargo CAS hit");
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = StatusCode::OK;
            response
        }
        Ok(None) => {
            tracing::debug!(tenant_hash = %hash_for_log(&tenant_id), key = %key, "cargo CAS miss");
            StatusCode::NOT_FOUND.into_response()
        }
        Err(CasError::Backend(msg)) => CargoAdapterError::Cas(msg).into_response(),
    }
}

/// `HEAD /<key>` — existence check; 200 on hit, 404 on miss.
///
/// Per spec §3: use `cas.get` and ignore the body — no separate
/// `exists()` on the port.
async fn handle_head(
    State(state): State<CargoRouterState>,
    Path(raw_key): Path<String>,
    headers: HeaderMap,
) -> Response {
    let key = match key_from_path(&raw_key) {
        Some(k) => k,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let pat = match extract_bearer(&headers) {
        Ok(pat) => pat,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    let tenant_id = match resolve_tenant(&state.tenant_resolver, &pat).await {
        Ok(t) => t,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    match state.cas.get(&tenant_id, &key).await {
        Ok(Some(_)) => StatusCode::OK.into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(CasError::Backend(msg)) => CargoAdapterError::Cas(msg).into_response(),
    }
}

/// `PUT /<key>` — write artifact; 200 on success.
async fn handle_put(
    State(state): State<CargoRouterState>,
    Path(raw_key): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let key = match key_from_path(&raw_key) {
        Some(k) => k,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let pat = match extract_bearer(&headers) {
        Ok(pat) => pat,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    let tenant_id = match resolve_tenant(&state.tenant_resolver, &pat).await {
        Ok(t) => t,
        Err(err) => {
            state.auditor.emit_auth_failed(&err.to_string());
            return err.into_response();
        }
    };

    // Collect body with size enforcement.
    let bytes = match collect_body(request, state.body_size_limit_bytes).await {
        Ok(b) => b,
        Err(err) => return err.into_response(),
    };

    // Audit emit BEFORE CAS put (fail-CLOSED contract).
    if let Err(err) = state
        .auditor
        .emit_cache_write(&tenant_id, &key, bytes.len() as u64)
    {
        return err.into_response();
    }

    match state.cas.put(&tenant_id, &key, bytes).await {
        Ok(()) => {
            tracing::debug!(tenant_hash = %hash_for_log(&tenant_id), key = %key, "cargo CAS put");
            StatusCode::OK.into_response()
        }
        Err(CasError::Backend(msg)) => CargoAdapterError::Cas(msg).into_response(),
    }
}

/// Collect the request body up to `limit` bytes.
///
/// Returns [`CargoAdapterError::BodyOversized`] if the streamed total
/// exceeds `limit`.
async fn collect_body(request: Request, limit: u64) -> Result<Vec<u8>, CargoAdapterError> {
    // axum 0.7 / http-body-util: collect the full body into Bytes.
    // We enforce the size limit here rather than using axum's
    // `DefaultBodyLimit` so we can return a structured error.
    let body = request.into_body();
    let collected = body
        .collect()
        .await
        .map_err(|e| CargoAdapterError::Cas(format!("body read: {e}")))?;
    let bytes = collected.to_bytes();
    let len = bytes.len() as u64;
    if len > limit {
        return Err(CargoAdapterError::BodyOversized(len));
    }
    Ok(bytes.to_vec())
}

impl IntoResponse for CargoAdapterError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            Self::Auth(msg) => (StatusCode::UNAUTHORIZED, format!("auth: {msg}")),
            Self::Cas(msg) => (StatusCode::BAD_GATEWAY, format!("cas: {msg}")),
            Self::Audit(msg) => (StatusCode::SERVICE_UNAVAILABLE, format!("audit: {msg}")),
            Self::BodyOversized(bytes) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("body exceeds limit: {bytes} bytes"),
            ),
            Self::Bind(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("bind: {err}")),
        };
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = status;
        response
    }
}

#[doc(hidden)]
pub fn _unused_marker(_: &SharedCasStore) {}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// F4 regression: `hash_for_log` emits a short hex handle, not the
    /// raw UUID, and is stable (same input → same output, different
    /// input → different output).
    #[test]
    fn hash_for_log_is_short_hex_not_raw_uuid() {
        let raw = "550e8400-e29b-41d4-a716-446655440000";
        let handle = hash_for_log(raw);
        // 16 hex chars (8 bytes).
        assert_eq!(handle.len(), 16, "handle must be 16 hex chars");
        assert!(
            handle.bytes().all(|b| b.is_ascii_hexdigit()),
            "handle must be hex"
        );
        // Must NOT be the raw UUID.
        assert_ne!(handle, raw, "handle must not be the raw tenant id");
        // Stability: same input always produces the same handle.
        assert_eq!(handle, hash_for_log(raw), "hash_for_log must be stable");
        // Different UUIDs produce different handles (collision-free for
        // any realistic tenant space).
        let other = hash_for_log("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert_ne!(
            handle, other,
            "distinct UUIDs must produce distinct handles"
        );
    }

    /// F4 regression: verify the first 8 bytes of SHA-256 match the
    /// expected value for a known input.
    #[test]
    fn hash_for_log_matches_sha256_first_8_bytes() {
        use sha2::{Digest as _, Sha256};
        let input = "550e8400-e29b-41d4-a716-446655440000";
        let full = Sha256::digest(input.as_bytes());
        let expected: String = full.iter().take(8).map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash_for_log(input), expected);
    }
}
