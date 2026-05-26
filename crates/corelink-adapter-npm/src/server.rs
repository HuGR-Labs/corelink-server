//! `axum` router + `run_npm_adapter` entry point.
//!
//! Wires the pure-logic handlers in [`crate::metadata`] /
//! [`crate::tarball`] into the HTTP surface. The router is
//! intentionally tiny — every decision (auth, TTL, CAS / KV /
//! upstream) is made in pure-logic modules and tested independently;
//! this layer just marshalls bytes.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::config::NpmAdapterConfig;
use crate::error::NpmAdapterError;
use crate::metadata::serve_metadata;
use crate::tarball::serve_tarball;
use crate::upstream::UpstreamClient;
use corelink_core::types::tenant::TenantId;

/// Shared router state passed by `axum`.
#[derive(Clone)]
#[non_exhaustive]
pub struct AdapterState {
    /// Adapter configuration (Arc-shared collaborator handles).
    pub config: Arc<NpmAdapterConfig>,
    /// `registry.npmjs.org` upstream HTTP client.
    pub upstream: Arc<UpstreamClient>,
}

impl AdapterState {
    /// Construct a new [`AdapterState`].
    #[must_use]
    pub fn new(config: Arc<NpmAdapterConfig>, upstream: Arc<UpstreamClient>) -> Self {
        Self { config, upstream }
    }
}

impl std::fmt::Debug for AdapterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterState")
            .field("config", &self.config)
            .field("upstream", &self.upstream)
            .finish()
    }
}

/// Query params for the search endpoint (not implemented, returns 501).
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct SearchQuery {
    /// Search text (ignored; endpoint returns 501).
    #[allow(dead_code)]
    pub text: Option<String>,
}

/// Build the `axum` router. Public so integration tests can mount
/// the same routes without running the full TCP listener.
pub fn build_router(state: AdapterState) -> Router {
    Router::new()
        .route("/-/ping", get(handle_ping))
        .route("/-/v1/search", get(handle_search_not_implemented))
        .route("/:pkg", get(handle_metadata))
        // Tarball: /<pkg>/-/<tarball>.tgz  — axum wildcard captures
        // the full suffix after the package name.
        .route("/:pkg/-/:tarball", get(handle_tarball))
        .with_state(state)
}

/// `GET /-/ping` — health probe.
async fn handle_ping() -> Response {
    (StatusCode::OK, "{}").into_response()
}

/// `GET /-/v1/search` — not implemented (spec §1 out of scope).
async fn handle_search_not_implemented(
    _query: Query<SearchQuery>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        "search is not implemented in this CoreLink adapter",
    )
        .into_response()
}

/// `GET /<pkg>` — package metadata (JSON).
async fn handle_metadata(
    State(state): State<AdapterState>,
    Path(pkg): Path<String>,
    headers: HeaderMap,
) -> Response {
    let cfg = state.config.clone();
    let tenant = match crate::auth::authenticate(
        &headers,
        &cfg.tenant_resolver,
        &cfg.auditor,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => return error_response(&e),
    };

    match serve_metadata(
        &pkg,
        cfg.metadata_ttl_seconds,
        &tenant,
        &cfg.metadata_kv,
        &state.upstream,
        &cfg.auditor,
    )
    .await
    {
        Ok(resp) => {
            let mut headers_out = HeaderMap::new();
            headers_out.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            (StatusCode::OK, headers_out, resp.body).into_response()
        }
        Err(e) => error_response(&e),
    }
}

/// `GET /<pkg>/-/<tarball>.tgz`
async fn handle_tarball(
    State(state): State<AdapterState>,
    Path((pkg, tarball_file)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let cfg = state.config.clone();
    let tenant = match crate::auth::authenticate(
        &headers,
        &cfg.tenant_resolver,
        &cfg.auditor,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => return error_response(&e),
    };

    // To get dist.shasum we need the package metadata. Fetch from KV
    // (must have been cached by a prior metadata request) or from
    // upstream if not yet cached.
    let meta_result = serve_metadata(
        &pkg,
        cfg.metadata_ttl_seconds,
        &tenant,
        &cfg.metadata_kv,
        &state.upstream,
        &cfg.auditor,
    )
    .await;

    let meta_body = match meta_result {
        Ok(r) => r.body,
        Err(e) => return error_response(&e),
    };

    let meta_json: serde_json::Value = match serde_json::from_slice(&meta_body) {
        Ok(v) => v,
        Err(e) => {
            return error_response(&NpmAdapterError::MetadataParse(format!(
                "metadata re-parse: {e}"
            )))
        }
    };

    // Derive version from tarball filename: <pkg>-<version>.tgz
    let version = derive_version_from_filename(&tarball_file, &pkg);

    let dist_shasum = match version
        .as_deref()
        .and_then(|v| {
            meta_json
                .get("versions")
                .and_then(|vs| vs.get(v))
                .and_then(|vobj| vobj.get("dist"))
                .and_then(|d| d.get("shasum"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_owned())
        }) {
        Some(s) => s,
        None => {
            return error_response(&NpmAdapterError::MetadataParse(format!(
                "could not find dist.shasum for tarball `{tarball_file}`"
            )))
        }
    };

    // Construct the canonical upstream tarball URL.
    let tarball_url = format!(
        "{}/{pkg}/-/{tarball_file}",
        cfg.upstream_registry.as_str().trim_end_matches('/')
    );

    match serve_tarball(
        &tarball_url,
        &pkg,
        version.as_deref().unwrap_or("unknown"),
        &dist_shasum,
        &tenant,
        &cfg.cas,
        &state.upstream,
        &cfg.auditor,
        cfg.tarball_size_limit_bytes,
    )
    .await
    {
        Ok(resp) => {
            let mut headers_out = HeaderMap::new();
            headers_out.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            if let Ok(v) = HeaderValue::from_str(&resp.digest_hex) {
                headers_out.insert(
                    http::HeaderName::from_static("x-corelink-cache-digest"),
                    v,
                );
            }
            if let Ok(v) = HeaderValue::from_str(&format!(
                "attachment; filename=\"{tarball_file}\""
            )) {
                headers_out.insert(header::CONTENT_DISPOSITION, v);
            }
            (StatusCode::OK, headers_out, resp.body).into_response()
        }
        Err(e) => error_response(&e),
    }
}

/// Map a [`NpmAdapterError`] to an HTTP response.
fn error_response(err: &NpmAdapterError) -> Response {
    let code =
        StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = err.to_string();
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    (code, h, body).into_response()
}

/// Derive the semver version string from an npm tarball filename.
/// npm tarball filenames follow the pattern `<pkg>-<version>.tgz`.
fn derive_version_from_filename(filename: &str, pkg: &str) -> Option<String> {
    // Strip the `.tgz` suffix.
    let stem = filename.strip_suffix(".tgz")?;
    // Strip the `<pkg>-` prefix (handle scoped packages via simple
    // prefix stripping after the last `-` delimiter).
    let prefix = format!("{pkg}-");
    if stem.starts_with(&prefix) {
        Some(stem[prefix.len()..].to_owned())
    } else {
        // Fall back: everything after the last `-`.
        stem.rsplit_once('-').map(|(_, ver)| ver.to_owned())
    }
}

/// Run the npm adapter. Binds the configured socket, starts the
/// `axum` server, and returns when the listener exits.
///
/// # Errors
///
/// Returns [`NpmAdapterError::Bind`] if the listener cannot bind;
/// [`NpmAdapterError::Upstream`] if the upstream client cannot be
/// constructed.
pub async fn run_npm_adapter(config: NpmAdapterConfig) -> Result<(), NpmAdapterError> {
    let upstream = UpstreamClient::new(config.upstream_registry.clone())?;
    let state = AdapterState::new(Arc::new(config.clone()), Arc::new(upstream));
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .map_err(NpmAdapterError::Bind)?;
    // Boot audit row so operators can correlate adapter lifecycle events.
    let zero_tenant = TenantId::from_uuid(uuid::Uuid::nil());
    let _ = emit_npm_audit(
        &config.auditor,
        event_types::METADATA_REFRESHED,
        &zero_tenant,
        now_unix_ms(),
        serde_json::json!({ "event": "boot", "bind": config.bind_addr.to_string() }),
    );
    axum::serve(listener, app)
        .await
        .map_err(|e| NpmAdapterError::Bind(std::io::Error::other(e.to_string())))?;
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
    fn derive_version_from_standard_filename() {
        assert_eq!(
            derive_version_from_filename("lodash-4.17.21.tgz", "lodash"),
            Some("4.17.21".into())
        );
    }

    #[test]
    fn derive_version_fallback_when_pkg_differs() {
        // Scoped package edge case.
        assert_eq!(
            derive_version_from_filename("react-dom-18.2.0.tgz", "react-dom"),
            Some("18.2.0".into())
        );
    }

    #[test]
    fn error_response_status_matches_enum() {
        let resp = error_response(&NpmAdapterError::Auth("x".into()));
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn error_response_502_for_upstream() {
        let resp = error_response(&NpmAdapterError::Upstream("x".into()));
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
