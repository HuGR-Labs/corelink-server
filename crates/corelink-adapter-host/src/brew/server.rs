//! Axum router + `run_brew_adapter` entrypoint.
//!
//! The router exposes ONE route — a catch-all `GET /{*path}` — because
//! brew bottle requests are an opaque, path-only namespace. Every
//! request flows through:
//!
//! 1. `extract_bearer` → PAT plaintext (HTTP 401 on mismatch);
//! 2. `resolve_tenant` → tenant id (HTTP 401);
//! 3. `BottleService::fetch` → bytes (HTTP 200) or one of the
//!    structured failure modes mapped below.
//!
//! ## Status code mapping
//!
//! | `BrewAdapterError` variant | HTTP | Why |
//! |---|---|---|
//! | `Auth`              | 401 | bad / missing PAT |
//! | `Cas`               | 502 | upstream-CAS dependency failure |
//! | `Upstream`          | 502 | upstream bottle host failed |
//! | `BottleOversized`   | 413 | request exceeded `bottle_size_limit_bytes` |
//! | `Audit`             | 503 | audit chokepoint failed (fail-CLOSED) |
//! | `Bind`              | n/a | bind failures surface from `run_brew_adapter` directly |

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::brew::audit::AuditOrchestrator;
use crate::brew::auth::{extract_bearer, resolve_tenant};
use crate::brew::bottle::BottleService;
use crate::brew::config::BrewAdapterConfig;
use crate::brew::error::BrewAdapterError;
use crate::brew::ports::{SharedCasStore, SharedTenantResolver};
use crate::brew::upstream::UpstreamFetcher;

/// Shared router state. `Clone`-cheap (every field is `Arc`-shaped).
#[derive(Clone)]
pub struct BrewRouterState {
    bottle: Arc<BottleService>,
    tenant_resolver: SharedTenantResolver,
}

impl std::fmt::Debug for BrewRouterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrewRouterState")
            .field("bottle", &self.bottle)
            .field("tenant_resolver", &"Arc<dyn TenantResolver>")
            .finish()
    }
}

/// Build the axum [`Router`] without binding a listener — useful for
/// in-process tests (drive it directly with `tower::ServiceExt`).
///
/// # Errors
///
/// Returns [`BrewAdapterError::Upstream`] if the reqwest client cannot
/// be initialized.
pub fn build_router(config: BrewAdapterConfig) -> Result<Router, BrewAdapterError> {
    let upstream = Arc::new(UpstreamFetcher::new(config.upstream_domain.clone())?);
    let auditor = AuditOrchestrator::new(config.auditor.clone());
    let bottle = Arc::new(BottleService::new(
        Arc::<dyn crate::brew::ports::CasStore>::clone(&config.cas),
        upstream,
        auditor,
        config.bottle_size_limit_bytes,
    ));
    let state = BrewRouterState {
        bottle,
        tenant_resolver: Arc::<dyn crate::brew::ports::TenantResolver>::clone(
            &config.tenant_resolver,
        ),
    };
    Ok(Router::new()
        .route("/", get(handle_bottle_request))
        .route("/*path", get(handle_bottle_request))
        .with_state(state))
}

/// Bind the configured [`SocketAddr`] and serve forever.
///
/// # Errors
///
/// Returns [`BrewAdapterError::Bind`] if the listener cannot bind,
/// [`BrewAdapterError::Upstream`] if the reqwest client fails to
/// initialize, or propagates any axum / hyper serve failure as
/// `BrewAdapterError::Bind` (the only post-bind I/O error class that
/// surfaces to this layer).
pub async fn run_brew_adapter(config: BrewAdapterConfig) -> Result<(), BrewAdapterError> {
    let bind_addr: SocketAddr = config.bind_addr;
    let router = build_router(config)?;
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(BrewAdapterError::Bind)?;
    tracing::info!(?bind_addr, "corelink-adapter-brew listening");
    axum::serve(listener, router)
        .await
        .map_err(BrewAdapterError::Bind)?;
    Ok(())
}

/// Catch-all bottle request handler.
async fn handle_bottle_request(
    State(state): State<BrewRouterState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let raw_path = request.uri().path_and_query().map_or("/", |pq| pq.as_str());

    let pat = match extract_bearer(&headers) {
        Ok(pat) => pat,
        Err(err) => return err.into_response(),
    };

    let tenant_id = match resolve_tenant(&state.tenant_resolver, &pat).await {
        Ok(t) => t,
        Err(err) => return err.into_response(),
    };

    match state.bottle.fetch(&tenant_id, raw_path).await {
        Ok(bytes) => {
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = StatusCode::OK;
            response
        }
        Err(err) => err.into_response(),
    }
}

impl IntoResponse for BrewAdapterError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Auth(_) => StatusCode::UNAUTHORIZED,
            Self::Cas(_) | Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::BottleOversized(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Audit(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Bind(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Cluster E (A27/A29): scrub internal backend detail (raw D1/CF/R2
        // error, possibly SQL; R2 storage topology; derived per-tenant prefix;
        // upstream host/transport) from the client body. Mint a correlation id,
        // log the REAL detail server-side only, return an opaque ref-tagged msg.
        let request_id = uuid::Uuid::new_v4().simple().to_string();
        if self.leaks_internal_detail() {
            tracing::error!(
                request_id = %request_id,
                detail = %self,
                "brew: backend error (scrubbed from client response; see ref)"
            );
        }
        let body = self.client_message(&request_id);
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = status;
        response
    }
}

#[doc(hidden)]
pub fn _unused_marker(_: &SharedCasStore) {}
