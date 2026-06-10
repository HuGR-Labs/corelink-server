//! `/brew/<tenant>/<bottle-path…>` — Homebrew bottle cache surface.
//!
//! Mounts the `corelink_adapter_host::brew` adapter (a read-through HTTPS
//! bottle proxy: `GET /<bottle-path>`, cache-fill from upstream on miss)
//! into the container router, backed by the 2-level content-dedup MOAT store.
//!
//! # Path shape
//!
//! The Worker forwards the FULL path `/brew/<tenant>/<bottle-path>` (the
//! `<tenant>` segment is the tenant namespace, used only for DO routing;
//! `worker/src/index.ts`). `nest_service("/brew", …)` strips the static
//! `/brew` prefix and hands `/<tenant>/<bottle-path>` to [`brew_gate`], which:
//!
//! 1. enforces the per-operation cache scope (GET/HEAD ⇒ read) from the
//!    Worker-set, server-trusted `x-corelink-scope` header; and
//! 2. rewrites `/<tenant>/<bottle-path>` → `/<bottle-path>` so the adapter's
//!    catch-all matches AND the upstream fetch targets the real bottle path
//!    (`ghcr.io/<bottle-path>`, never `ghcr.io/<tenant>/…`).
//!
//! `nest_service` (not `nest`) is required so the adapter's catch-all
//! `/{*path}` is preserved rather than flattened.
//!
//! # Trust + storage model
//!
//! Tenant identity comes from the bearer PAT, re-verified in the container
//! ([`crate::adapter_pat::PatVerifier`], Option B). The path `<tenant>` is
//! NEVER trusted. Homebrew bottles are PUBLIC, so the bytes are stored under
//! the shared [`crate::adapter_cache::PUBLIC_NAMESPACE`] via the 2-level
//! [`MoatCache`] — identical bottles dedup across tenants (the network-effect
//! moat). The PAT gates ACCESS (only authenticated tenants may use the cache);
//! the cached public CONTENT is shared (safe — it is public upstream).

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::Request;
use axum::http::{Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use url::Url;

use corelink_adapter_host::brew::config::DEFAULT_BOTTLE_SIZE_LIMIT_BYTES;
use corelink_adapter_host::brew::ports::{CasError, CasStore, TenantResolveError, TenantResolver};
use corelink_adapter_host::brew::server::build_router;
use corelink_adapter_host::brew::BrewAdapterConfig;
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore, PUBLIC_NAMESPACE};
use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::scope::{requires_cache_read, requires_cache_write, SCOPE_HEADER};

/// Service principal stamped on the adapter's CAS operations. Identifies the
/// adapter-host service, NOT the end-user PAT.
const BREW_SERVICE_PRINCIPAL: &str = "brew-adapter-host";

/// Upstream Homebrew bottle host. Fixed; the adapter joins the
/// (tenant-stripped) bottle path onto this base. Per-request override is not
/// possible — see the SSRF guard in `corelink_adapter_host::brew::upstream`.
const BREW_UPSTREAM_DOMAIN: &str = "https://ghcr.io";

/// Brew's `CasStore` port → the 2-level [`MoatCache`], always under the shared
/// [`PUBLIC_NAMESPACE`] (Homebrew bottles are public ⇒ cross-tenant dedup).
///
/// Pre-store integrity for this SHARED namespace is enforced upstream of this
/// store, in the adapter's fetch path (`corelink_adapter_host::brew::bottle`):
/// content-addressed ghcr.io paths (`…/sha256:<hex>`) are verified against the
/// URL-declared digest BEFORE `put` is ever called — a mismatched fetch is
/// refused + audited, so corrupt bytes cannot be persisted here. The read path
/// additionally re-verifies blake3 against the stored mapping (self-healing).
#[derive(Debug)]
struct BrewMoatStore {
    moat: Arc<MoatCache>,
}

#[async_trait]
impl CasStore for BrewMoatStore {
    async fn get(&self, _tenant_id: &str, cas_key: &str) -> Result<Option<Vec<u8>>, CasError> {
        // brew is all-public: ignore the per-request tenant and use the shared
        // PUBLIC namespace so identical bottles dedup across tenants. `cas_key`
        // is the adapter's `blake3(canonical-URL)` ⇒ the moat map key.
        self.moat
            .get(PUBLIC_NAMESPACE, cas_key)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => CasError::Backend(m),
            })
    }

    async fn put(&self, _tenant_id: &str, cas_key: &str, bytes: Vec<u8>) -> Result<(), CasError> {
        self.moat
            .put(PUBLIC_NAMESPACE, cas_key, bytes)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => CasError::Backend(m),
            })
    }
}

/// Thin shell wrapping the shared [`PatVerifier`] as brew's `TenantResolver`.
#[derive(Debug)]
struct BrewPatResolver(Arc<PatVerifier>);

#[async_trait]
impl TenantResolver for BrewPatResolver {
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
        self.0.verify(pat_plaintext).await.map_err(|e| match e {
            VerifyError::InvalidPat => TenantResolveError::InvalidPat,
            VerifyError::Backend(m) => TenantResolveError::Backend(m),
        })
    }
}

/// Build the `/brew/*` sub-router from shared CAS handlers + the url→hash map
/// + the PAT verifier.
///
/// The `cas_read`/`cas_write` are the SAME trait objects the cas/ac/bazel/turbo
/// surfaces use; `map` is the D1-backed url→content-hash store; `verifier` is
/// shared across cache adapters. On a construction error the route is simply
/// NOT mounted (empty sub-router + logged) so the container still boots.
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    verifier: Arc<PatVerifier>,
) -> Router {
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        BREW_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn CasStore> = Arc::new(BrewMoatStore { moat });
    let resolver: Arc<dyn TenantResolver> = Arc::new(BrewPatResolver(verifier));
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());

    let upstream = match Url::parse(BREW_UPSTREAM_DOMAIN) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "/brew upstream URL invalid; brew NOT mounted");
            return Router::new();
        }
    };

    let config = BrewAdapterConfig::new(
        // bind_addr is unused by `build_router` (only `run_brew_adapter` binds).
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        upstream,
        DEFAULT_BOTTLE_SIZE_LIMIT_BYTES,
        cas,
        resolver,
        auditor,
    );

    let adapter = match build_router(config) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = ?e, "/brew build_router failed; brew NOT mounted");
            return Router::new();
        }
    };

    let adapter = adapter.layer(middleware::from_fn(brew_gate));
    Router::new().nest_service("/brew", adapter)
}

/// Gate layer: per-operation cache-scope enforcement + tenant-segment strip.
///
/// Runs AFTER `nest_service` strips `/brew`, so `req.uri().path()` is
/// `/<tenant>/<bottle-path>`. (1) enforces the per-op scope from the
/// server-trusted `x-corelink-scope` header (GET/HEAD ⇒ read); (2) rewrites the
/// path to `/<bottle-path>` so both the adapter's catch-all and the upstream
/// fetch see the real bottle path (never the tenant segment).
async fn brew_gate(mut req: Request, next: Next) -> Response {
    let scope = req
        .headers()
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let scope_ok = match *req.method() {
        // brew is a read-through cache: clients only GET. The transparent
        // cache-fill (a CAS write by the service) is not a client write.
        Method::GET | Method::HEAD => requires_cache_read(scope),
        // brew exposes no client write surface; gate defensively anyway.
        Method::PUT => requires_cache_write(scope),
        // Fail-CLOSED: the adapter only routes GET (axum's `get` also serves
        // HEAD), so anything else would 405 downstream today — but the gate
        // must not assume that. An unmapped method is denied here so a future
        // adapter route can never ship without an explicit scope decision. No
        // browser/CORS clients exist on this surface (brew CLI only), so
        // OPTIONS is not legitimate traffic.
        _ => false,
    };
    if !scope_ok {
        return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
    }

    // Strip the leading `<tenant>` segment: `/<tenant>/<rest>` → `/<rest>`.
    let after = req.uri().path().strip_prefix('/').unwrap_or("");
    let rewritten = match after.split_once('/') {
        Some((_tenant, rest)) => format!("/{rest}"),
        // Just `/<tenant>` with no bottle path → root (adapter returns empty/404).
        None => "/".to_owned(),
    };
    let new_path_and_query = match req.uri().query() {
        Some(q) => format!("{rewritten}?{q}"),
        None => rewritten,
    };
    match Uri::builder().path_and_query(new_path_and_query).build() {
        Ok(u) => *req.uri_mut() = u,
        Err(e) => {
            tracing::warn!(error = %e, "brew_gate path rewrite failed");
            return (StatusCode::BAD_REQUEST, "bad path").into_response();
        }
    }

    next.run(req).await
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
    use std::collections::HashMap;
    use std::sync::Mutex;

    use axum::body::Body;
    use axum::http::{Method, Request as HttpRequest, StatusCode};
    use corelink_adapter_host::brew::bottle::{canonical_bottle_path, cas_key_for};
    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW,
    };
    use tower::ServiceExt; // for `.oneshot`
    use uuid::Uuid;

    use crate::adapter_cache::canonical_hash_hex;
    use crate::adapter_pat::{PatRow, PatRowLookup};

    use super::*;

    const SCOPE_RW: &str = "cas:rw";

    /// PatRowLookup that knows ONE token_id → row; everything else unknown.
    struct OneTokenLookup {
        token_id: String,
        row: PatRow,
    }
    #[async_trait]
    impl PatRowLookup for OneTokenLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            Ok((token_id == self.token_id).then(|| self.row.clone()))
        }
    }

    /// PatRowLookup that knows nothing (rejects every token).
    struct EmptyLookup;
    #[async_trait]
    impl PatRowLookup for EmptyLookup {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            Ok(None)
        }
    }

    /// In-memory url→content-hash map.
    #[derive(Default)]
    struct FakeMap(Mutex<HashMap<(String, String), String>>);
    #[async_trait]
    impl UrlMapStore for FakeMap {
        async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), url_hash.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            ns: &str,
            url_hash: &str,
            content_hash: &str,
            _len: u64,
        ) -> Result<(), String> {
            self.0.lock().unwrap().insert(
                (ns.to_owned(), url_hash.to_owned()),
                content_hash.to_owned(),
            );
            Ok(())
        }
    }

    /// Non-verifying CAS stub (accepts any claimed_hash) — `brew::router` wires
    /// the production `canonical_hash_hex`, which a verifying in-memory handler
    /// would reject. Keyed by `(tenant, hash)`.
    #[derive(Debug, Default)]
    struct StubCas(Mutex<HashMap<(String, String), Vec<u8>>>);
    impl CasReadHandler for StubCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            match self
                .0
                .lock()
                .unwrap()
                .get(&(req.tenant.clone(), req.hash.clone()))
            {
                Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
                None => Err(CasHandlerError::Internal("stub: absent".into())),
            }
        }
    }
    impl CasWriteHandler for StubCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            self.0
                .lock()
                .unwrap()
                .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Router whose verifier rejects ALL PATs (empty lookup); cas/map unused.
    fn router_rejecting() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            verifier,
        )
    }

    fn get(uri: &str, pat: Option<&str>, scope: Option<&str>) -> HttpRequest<Body> {
        let mut b = HttpRequest::builder().method(Method::GET).uri(uri);
        if let Some(p) = pat {
            b = b.header("authorization", format!("Bearer {p}"));
        }
        if let Some(s) = scope {
            b = b.header(SCOPE_HEADER, s);
        }
        b.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn missing_scope_is_403_at_the_gate() {
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/brew/t/v2/x", Some("corelink_whatever"), None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unmapped_methods_are_403_even_with_rw_scope() {
        // Fail-CLOSED method gate: DELETE/PATCH carry a FULL cas:rw scope but
        // are not a mapped cache operation, so the gate must deny them (403)
        // rather than fall through to downstream routing.
        for method in [Method::DELETE, Method::PATCH] {
            let app = router_rejecting();
            let req = HttpRequest::builder()
                .method(method.clone())
                .uri("/brew/t/v2/x")
                .header("authorization", "Bearer corelink_whatever")
                .header(SCOPE_HEADER, SCOPE_RW)
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "{method} with cas:rw must fail closed at the gate"
            );
        }
    }

    #[tokio::test]
    async fn missing_pat_is_401_reaches_adapter() {
        // scope present → gate passes → adapter extract_bearer fails → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/brew/t/v2/x", None, Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_prefix_pat_is_401() {
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/brew/t/v2/x", Some("ghp_github"), Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_pat_is_401_resolver_runs() {
        // corelink_-prefixed but HMAC-invalid → reaches the resolver (proves
        // nest_service routed to the adapter) → InvalidPat → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get(
                "/brew/t/v2/x",
                Some("corelink_not-a-real-token"),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cache_hit_round_trip_with_tenant_stripped() {
        // Mint a real PAT; seed the 2-level moat so a GET is a cache HIT (no
        // upstream). Proves end-to-end: nest_service mount + tenant-strip +
        // scope + Option-B resolve + moat get.
        let key = test_key();
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            TenantId(Uuid::from_u128(0xBEEF)),
            PrincipalId(Uuid::from_u128(0xF00D)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

        let bottle_path = "v2/homebrew/core/curl-8.5.0.bottle.tar.gz";
        let bytes = b"\x1f\x8bfake-bottle".to_vec();
        let content_hash = canonical_hash_hex(&bytes);
        let url_hash = cas_key_for(&canonical_bottle_path(&format!("/{bottle_path}")));

        // Seed both levels under the PUBLIC namespace.
        let cas = Arc::new(StubCas::default());
        cas.0.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), content_hash.clone()),
            bytes.clone(),
        );
        let map = Arc::new(FakeMap::default());
        map.0
            .lock()
            .unwrap()
            .insert((PUBLIC_NAMESPACE.to_owned(), url_hash), content_hash);

        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            verifier,
        );

        // path-tenant "ignored" ≠ the PAT tenant → proves the path tenant is
        // stripped + untrusted (storage is the shared PUBLIC namespace).
        let resp = app
            .oneshot(get(
                &format!("/brew/ignored/{bottle_path}"),
                Some(&pt),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), bytes.as_slice());
    }
}
