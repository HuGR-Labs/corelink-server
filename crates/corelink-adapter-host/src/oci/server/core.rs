//! Shared app-state + status mapping + error-envelope serializer used
//! by every endpoint handler in [`super::handlers`].

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use crate::oci::config::OciAdapterConfig;
use crate::oci::error::OciAdapterError;

/// Adapter app-state shared across handler dispatches.
#[derive(Clone)]
#[non_exhaustive]
pub struct AppState {
    /// Live config — `Arc` so axum's `State` can clone cheaply.
    pub config: Arc<OciAdapterConfig>,
    /// Clock — exposed as a fn pointer to keep tests deterministic.
    /// Defaults to wall-clock `SystemTime::now()` in
    /// [`crate::run_oci_adapter`]; tests inject a fixed value.
    pub clock_unix_ms: fn() -> u64,
}

impl AppState {
    /// Construct an [`AppState`] from explicit parts (the struct is
    /// `#[non_exhaustive]` so external tests cannot use struct-literal
    /// syntax).
    #[must_use]
    pub fn new(config: Arc<OciAdapterConfig>, clock_unix_ms: fn() -> u64) -> Self {
        Self {
            config,
            clock_unix_ms,
        }
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Default wall-clock function used by [`crate::run_oci_adapter`].
#[must_use]
pub fn wallclock_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// Map an [`OciAdapterError`] to its wire HTTP status.
#[must_use]
pub const fn status_for(err: &OciAdapterError) -> StatusCode {
    match err {
        OciAdapterError::Bind(_) => StatusCode::INTERNAL_SERVER_ERROR,
        OciAdapterError::Auth(_)
        | OciAdapterError::InvalidToken
        | OciAdapterError::CatalogDisabled => StatusCode::UNAUTHORIZED,
        OciAdapterError::Cas(_) | OciAdapterError::Kv(_) => StatusCode::INTERNAL_SERVER_ERROR,
        OciAdapterError::DigestMismatch { .. }
        | OciAdapterError::ManifestInvalid(_)
        | OciAdapterError::InvalidRepoName(_) => StatusCode::BAD_REQUEST,
        OciAdapterError::BlobOversized(_) => StatusCode::PAYLOAD_TOO_LARGE,
        OciAdapterError::UploadSessionMissing(_) | OciAdapterError::NotFound => {
            StatusCode::NOT_FOUND
        }
        OciAdapterError::Audit(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Build a wire-shape JSON error envelope `axum::Response`. Adds the
/// `Www-Authenticate: Bearer …` header on 401 responses.
pub fn err_response(
    err: &OciAdapterError,
    realm: Option<&str>,
    scope: Option<&str>,
) -> axum::response::Response {
    let status = status_for(err);
    let body = serde_json::to_vec(&err.to_envelope()).unwrap_or_default();
    let mut headers = HeaderMap::new();
    if let Ok(v) = "application/json".parse() {
        headers.insert(axum::http::header::CONTENT_TYPE, v);
    }
    if status == StatusCode::UNAUTHORIZED {
        if let (Some(r), Some(s)) = (realm, scope) {
            let h = crate::oci::auth::www_authenticate_header(r, "corelink-oci", s);
            if let Ok(v) = h.parse() {
                headers.insert(axum::http::header::WWW_AUTHENTICATE, v);
            }
        } else if let Some(r) = realm {
            let h = crate::oci::auth::www_authenticate_header(r, "corelink-oci", "");
            if let Ok(v) = h.parse() {
                headers.insert(axum::http::header::WWW_AUTHENTICATE, v);
            }
        }
    }
    (status, headers, Body::from(body)).into_response()
}
