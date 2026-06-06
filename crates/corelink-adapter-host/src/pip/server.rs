//! `axum` router + `run_pip_adapter` entry point.
//!
//! Wires the pure-logic handlers in [`crate::pip::index`] / [`crate::pip::wheel`]
//! into the HTTP surface. The router is intentionally tiny — every
//! decision (auth, format negotiation, CAS / KV / upstream) is made
//! in pure-logic modules and tested independently; this layer just
//! marshalls bytes.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::pip::audit::{emit_pip_audit, event_types, now_unix_ms};
use crate::pip::config::PipAdapterConfig;
use crate::pip::error::PipAdapterError;
use crate::pip::index::{serve_index, IndexFormat};
use crate::pip::pep503_html::{parse_json_index, ProjectIndex};
use crate::pip::upstream::UpstreamClient;
use crate::pip::wheel::serve_wheel;
use corelink_core::types::tenant::TenantId;

/// Shared router state passed by `axum`.
#[derive(Clone)]
pub struct AdapterState {
    /// Adapter configuration (Arc-shared collaborator handles).
    pub config: Arc<PipAdapterConfig>,
    /// `pypi.org` upstream HTTP client.
    pub upstream: Arc<UpstreamClient>,
}

impl std::fmt::Debug for AdapterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterState")
            .field("config", &self.config)
            .field("upstream", &self.upstream)
            .finish()
    }
}

/// Build the `axum` router. Public so integration tests can mount
/// the same routes without running the full TCP listener.
pub fn build_router(state: AdapterState) -> Router {
    Router::new()
        .route("/simple/:project/", get(handle_index))
        .route("/pkg/:sha256/:filename", get(handle_wheel))
        .route("/healthz", get(handle_health))
        .with_state(state)
}

/// `GET /healthz` — process liveness only (no upstream / KV touch).
async fn handle_health() -> Response {
    (StatusCode::OK, "ok").into_response()
}

/// `GET /simple/<project>/`.
async fn handle_index(
    State(state): State<AdapterState>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    let cfg = state.config.clone();
    let tenant =
        match crate::pip::auth::authenticate(&headers, &cfg.tenant_resolver, &cfg.auditor).await {
            Ok(t) => t,
            Err(e) => return error_response(&e),
        };
    let format = IndexFormat::negotiate(
        headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()),
        cfg.prefer_json_index,
    );
    match serve_index(
        &project,
        format,
        cfg.index_ttl_seconds,
        &tenant,
        &cfg.metadata_kv,
        &state.upstream,
        &cfg.auditor,
    )
    .await
    {
        Ok(resp) => {
            let mut headers_out = HeaderMap::new();
            if let Ok(ct) = HeaderValue::from_str(resp.content_type) {
                headers_out.insert(header::CONTENT_TYPE, ct);
            }
            (StatusCode::OK, headers_out, resp.body).into_response()
        }
        Err(e) => error_response(&e),
    }
}

/// `GET /pkg/<sha256>/<filename>`.
async fn handle_wheel(
    State(state): State<AdapterState>,
    Path((sha256, filename)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let cfg = state.config.clone();
    let tenant =
        match crate::pip::auth::authenticate(&headers, &cfg.tenant_resolver, &cfg.auditor).await {
            Ok(t) => t,
            Err(e) => return error_response(&e),
        };
    // To look up the upstream URL for a miss, we need the cached
    // index. We derive the project name from the filename (PEP 491
    // wheel-name spec: project is the first `-`-separated token,
    // sdist is `name-version.tar.gz`).
    let project = derive_project(&filename);
    let kv_key = crate::pip::index::kv_key_for_project(&project);
    let index_bytes = match cfg.metadata_kv.get(&tenant, &kv_key).await {
        Ok(Some((b, _))) => b,
        Ok(None) => {
            return error_response(&PipAdapterError::IndexParse(format!(
            "no cached index for project `{project}`; client must request /simple/{project}/ first"
        )))
        }
        Err(e) => return error_response(&e),
    };
    let index: ProjectIndex = match parse_json_index(&index_bytes) {
        Ok(i) => i,
        Err(e) => return error_response(&e),
    };

    let resp = match serve_wheel(
        &sha256,
        &filename,
        &project,
        &index,
        &tenant,
        &cfg.cas,
        &state.upstream,
        &cfg.auditor,
        cfg.wheel_size_limit_bytes,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return error_response(&e),
    };

    let mut headers_out = HeaderMap::new();
    headers_out.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(v) = HeaderValue::from_str(&resp.sha256) {
        headers_out.insert(http::HeaderName::from_static("x-corelink-cache-digest"), v);
    }
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", resp.filename)) {
        headers_out.insert(header::CONTENT_DISPOSITION, v);
    }
    (StatusCode::OK, headers_out, resp.body).into_response()
}

/// Map a [`PipAdapterError`] to an HTTP response.
fn error_response(err: &PipAdapterError) -> Response {
    let code = StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = err.to_string();
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    (code, h, body).into_response()
}

/// Derive the PEP 503-normalised project name from a wheel/sdist
/// filename. Wheel filename grammar (PEP 491):
///
/// `{distribution}-{version}(-{build_tag})?-{python}-{abi}-{platform}.whl`
///
/// Sdist filename: `{name}-{version}.tar.gz`. In both cases the
/// distribution name is the slice up to the first `-`.
fn derive_project(filename: &str) -> String {
    let raw = filename.split('-').next().unwrap_or(filename).to_owned();
    crate::pip::pep503_html::normalise_project_name(&raw)
}

/// Run the pip adapter. Binds the configured socket, starts the
/// `axum` server, and returns when the listener exits (typically
/// never — caller awaits this future indefinitely).
///
/// # Errors
///
/// Returns [`PipAdapterError::Bind`] if the listener cannot bind to
/// `config.bind_addr`; [`PipAdapterError::Upstream`] if the upstream
/// client cannot be constructed (e.g. TLS init failure).
pub async fn run_pip_adapter(config: PipAdapterConfig) -> Result<(), PipAdapterError> {
    let upstream = UpstreamClient::new(config.upstream_pypi.clone())?;
    let state = AdapterState {
        config: Arc::new(config.clone()),
        upstream: Arc::new(upstream),
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .map_err(PipAdapterError::Bind)?;
    // Emit a boot audit row so operators can correlate adapter
    // lifecycle events with downstream traffic.
    let zero_tenant = TenantId::from_uuid(uuid::Uuid::nil());
    let _ = emit_pip_audit(
        &config.auditor,
        event_types::INDEX_REFRESHED,
        &zero_tenant,
        now_unix_ms(),
        serde_json::json!({ "event": "boot", "bind": config.bind_addr.to_string() }),
    );
    axum::serve(listener, app)
        .await
        .map_err(|e| PipAdapterError::Bind(std::io::Error::other(e.to_string())))?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn derive_project_handles_wheel_filename() {
        assert_eq!(
            derive_project("requests-2.31.0-py3-none-any.whl"),
            "requests"
        );
    }

    #[test]
    fn derive_project_handles_sdist_filename() {
        assert_eq!(derive_project("Flask-2.3.2.tar.gz"), "flask");
    }

    #[test]
    fn derive_project_normalises_underscores() {
        assert_eq!(
            derive_project("scikit_learn-1.0.0-cp310-cp310-manylinux.whl"),
            "scikit-learn"
        );
    }

    #[test]
    fn error_response_status_matches_enum() {
        let resp = error_response(&PipAdapterError::Auth("x".into()));
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
