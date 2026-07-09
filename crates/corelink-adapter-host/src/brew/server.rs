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
//! | `ForbiddenRepoPath` | 403 | path outside the allowed Homebrew repo namespace |
//! | `Cas`               | 502 | upstream-CAS dependency failure |
//! | `Upstream`          | 502 | upstream bottle host failed |
//! | `BottleOversized`   | 413 | request exceeded `bottle_size_limit_bytes` |
//! | `Audit`             | 503 | audit chokepoint failed (fail-CLOSED) |
//! | `Bind`              | n/a | bind failures surface from `run_brew_adapter` directly |

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
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
        .route("/{*path}", get(handle_bottle_request))
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
        Ok(fetch) => {
            // C-MOAT: surface the cache-hit signal as the `X-Cache` header so a
            // client (and the e2e shared_cache journey) can PROVE a
            // cross-tenant `_public` serve. `HIT` = served from CAS without
            // touching the network; `MISS` = filled from upstream this request
            // (or a served-but-not-cached tag-addressed path).
            let mut response = Response::new(Body::from(fetch.bytes));
            *response.status_mut() = StatusCode::OK;
            response.headers_mut().insert(
                "x-cache",
                if fetch.is_hit {
                    HeaderValue::from_static("HIT")
                } else {
                    HeaderValue::from_static("MISS")
                },
            );
            response
        }
        Err(err) => err.into_response(),
    }
}

impl IntoResponse for BrewAdapterError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Auth(_) => StatusCode::UNAUTHORIZED,
            Self::ForbiddenRepoPath(_) => StatusCode::FORBIDDEN,
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_docs_in_private_items
)]
mod tests {
    //! C-MOAT: the `X-Cache` response header must be `HIT` on a CAS serve and
    //! `MISS` on an upstream fill — the moat-proof signal asserted by the e2e
    //! `shared_cache` journey.
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use sha2::{Digest as _, Sha256};
    use url::Url;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::brew::config::DEFAULT_BOTTLE_SIZE_LIMIT_BYTES;
    use crate::brew::ports::{CasError, CasStore, TenantResolveError, TenantResolver};

    const PAT: &str = "corelink_tenant_xcache";
    const TENANT: &str = "tenant-xcache";
    const BOTTLE_BYTES: &[u8] = b"\x1f\x8b\x08\x00x-cache-payload";

    /// Digest-addressed (cacheable) bottle path whose sha256 matches the bytes.
    fn digest_path() -> String {
        let digest = hex::encode(Sha256::digest(BOTTLE_BYTES));
        format!("/v2/homebrew/core/curl/blobs/sha256:{digest}")
    }

    /// CAS preloaded so every `get` is a HIT (and `put` is a no-op).
    #[derive(Debug)]
    struct HitCas(Vec<u8>);
    #[async_trait]
    impl CasStore for HitCas {
        async fn get(&self, _tenant_id: &str, _cas_key: &str) -> Result<Option<Vec<u8>>, CasError> {
            Ok(Some(self.0.clone()))
        }
        async fn put(&self, _t: &str, _k: &str, _b: Vec<u8>) -> Result<(), CasError> {
            Ok(())
        }
    }

    /// Empty CAS: every `get` is a MISS, `put` succeeds (records nothing).
    #[derive(Debug, Default)]
    struct EmptyCas;
    #[async_trait]
    impl CasStore for EmptyCas {
        async fn get(&self, _tenant_id: &str, _cas_key: &str) -> Result<Option<Vec<u8>>, CasError> {
            Ok(None)
        }
        async fn put(&self, _t: &str, _k: &str, _b: Vec<u8>) -> Result<(), CasError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct StaticResolver(Mutex<HashMap<String, String>>);
    #[async_trait]
    impl TenantResolver for StaticResolver {
        async fn resolve(&self, pat: &str) -> Result<String, TenantResolveError> {
            self.0
                .lock()
                .unwrap()
                .get(pat)
                .cloned()
                .ok_or(TenantResolveError::InvalidPat)
        }
    }

    fn resolver() -> Arc<StaticResolver> {
        let mut m = HashMap::new();
        m.insert(PAT.to_owned(), TENANT.to_owned());
        Arc::new(StaticResolver(Mutex::new(m)))
    }

    async fn spin(cas: Arc<dyn CasStore>, upstream: Url) -> SocketAddr {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let config = BrewAdapterConfig::new(
            bind,
            upstream,
            DEFAULT_BOTTLE_SIZE_LIMIT_BYTES,
            cas,
            resolver(),
            Arc::new(corelink_audit::ports::InMemoryAuditEmitter::new()),
        );
        let router = build_router(config).unwrap();
        let listener = tokio::net::TcpListener::bind(bind).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cache_hit_sets_x_cache_hit() {
        // CAS already holds the bytes ⇒ the serve short-circuits upstream; the
        // dummy upstream URL is never contacted.
        let addr = spin(
            Arc::new(HitCas(BOTTLE_BYTES.to_vec())),
            Url::parse("https://ghcr.io").unwrap(),
        )
        .await;
        let resp = reqwest::Client::new()
            .get(format!("http://{addr}{}", digest_path()))
            .header("Authorization", format!("Bearer {PAT}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("x-cache").map(|v| v.as_bytes()),
            Some(&b"HIT"[..]),
            "a CAS serve MUST carry X-Cache: HIT"
        );
        assert_eq!(resp.bytes().await.unwrap().as_ref(), BOTTLE_BYTES);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cache_miss_sets_x_cache_miss() {
        // Empty CAS + a mock upstream serving the digest-matching bytes ⇒ the
        // request fills from upstream, which is a MISS.
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(BOTTLE_BYTES.to_vec()))
            .mount(&upstream)
            .await;
        let addr = spin(
            Arc::new(EmptyCas),
            Url::parse(&upstream.uri()).unwrap(),
        )
        .await;
        let resp = reqwest::Client::new()
            .get(format!("http://{addr}{}", digest_path()))
            .header("Authorization", format!("Bearer {PAT}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("x-cache").map(|v| v.as_bytes()),
            Some(&b"MISS"[..]),
            "an upstream fill MUST carry X-Cache: MISS"
        );
        assert_eq!(resp.bytes().await.unwrap().as_ref(), BOTTLE_BYTES);
    }
}
