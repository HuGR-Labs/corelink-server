//! `/pip/<tenant>/<pep-path…>` — PyPI (pip / uv / poetry / pdm) cache surface.
//!
//! Mounts the `corelink_adapter_host::pip` adapter (a read-through PyPI
//! caching mirror: PEP 503/691 simple index + content-addressed
//! wheels/sdists) into the container router, backed by the 2-level
//! content-dedup MOAT store (for the immutable wheel/sdist bytes) plus a
//! D1-backed per-tenant key-value store (for the mutable simple index).
//!
//! # Path shape
//!
//! The Worker forwards the FULL path `/pip/<tenant>/<rest>` (the
//! `<tenant>` segment is the tenant namespace, used only for DO routing).
//! `nest_service("/pip", …)` strips the static `/pip` prefix and hands
//! `/<tenant>/<rest>` to [`pip_gate`], which:
//!
//! 1. enforces the per-operation cache scope (GET ⇒ read) from the
//!    Worker-set, server-trusted `x-corelink-scope` header; and
//! 2. rewrites `/<tenant>/<rest>` → `/<rest>` so the adapter's routes
//!    (`/simple/:project/`, `/pkg/:sha256/:filename`, `/healthz`) match
//!    AND the upstream fetch targets the real PyPI path (never
//!    `pypi.org/<tenant>/…`).
//!
//! `nest_service` (not `nest`) is required so the adapter's typed routes
//! are preserved rather than flattened.
//!
//! # Trust + storage model
//!
//! Tenant identity comes from the bearer PAT, re-verified in the
//! container ([`crate::adapter_pat::PatVerifier`], Option B). The path
//! `<tenant>` is NEVER trusted.
//!
//! - **Wheels / sdists are immutable, content-addressed (sha256) and
//!   PUBLIC** (public PyPI), so the bytes are stored under the shared
//!   [`crate::adapter_cache::PUBLIC_NAMESPACE`] via the 2-level
//!   [`MoatCache`] — identical wheels dedup across tenants (the
//!   network-effect moat). The PAT gates ACCESS; the cached public
//!   CONTENT is shared (safe — it is public upstream).
//! - **The simple index is MUTABLE** (re-fetched on TTL expiry) and so
//!   cannot live in the content-addressed moat. It is cached in a
//!   per-tenant D1 key-value table ([`PipIndexKvStore`]) keyed by
//!   `(tenant_uuid, index_key)` (private per tenant — isolated).
//!
//! # Digest fork (sha256 vs blake3) — load-bearing
//!
//! The pip adapter content-addresses wheels by their PyPI-supplied
//! **sha256** (`#sha256=` index fragment; verified pre-store). The moat's
//! CAS layer content-addresses bytes by **blake3** (`canonical_hash_hex`).
//! These are reconciled in [`PipMoatStore`]: the adapter's sha256
//! [`Digest`] is used as the moat's *level-2 url-hash key*
//! (`(namespace, sha256) → blake3(content)`), and the moat
//! content-addresses + dedups the bytes by blake3 underneath. The
//! adapter's mandatory sha256 integrity check still runs before `put`, so
//! the sha256↔bytes binding is preserved end-to-end; blake3 dedup is an
//! orthogonal storage-layer win.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::Request;
use axum::http::{Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use url::Url;

use corelink_adapter_host::pip::config::{
    PipAdapterConfig, DEFAULT_INDEX_TTL_SECONDS, DEFAULT_UPSTREAM_PYPI,
    DEFAULT_WHEEL_SIZE_LIMIT_BYTES,
};
use corelink_adapter_host::pip::error::PipAdapterError;
use corelink_adapter_host::pip::ports::{CasStore, KvStore, TenantResolver};
use corelink_adapter_host::pip::server::{build_router, AdapterState};
use corelink_adapter_host::pip::upstream::UpstreamClient;
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore, PUBLIC_NAMESPACE};
use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::scope::{requires_cache_read, requires_cache_write, SCOPE_HEADER};
use crate::storage::d1_http::D1HttpClient;

/// Service principal stamped on the adapter's CAS operations. Identifies
/// the adapter-host service, NOT the end-user PAT.
const PIP_SERVICE_PRINCIPAL: &str = "pip-adapter-host";

/// Default upstream PyPI metadata host. The wheel/sdist BYTES live on a
/// SECOND host (`files.pythonhosted.org`) — see the SSRF note in
/// [`router`] and `open_decisions`.
const PIP_UPSTREAM_DEFAULT: &str = DEFAULT_UPSTREAM_PYPI;

/// Pip's `CasStore` port → the 2-level [`MoatCache`], always under the
/// shared [`PUBLIC_NAMESPACE`] (public PyPI wheels ⇒ cross-tenant dedup).
///
/// The adapter's [`Digest`] is the wheel's **sha256** (verified by the
/// adapter before `put`); we use its hex as the moat's *url-hash* key and
/// let the moat content-address the bytes by blake3 underneath (see the
/// module-level "Digest fork" note).
#[derive(Debug)]
struct PipMoatStore {
    moat: Arc<MoatCache>,
}

#[async_trait]
impl CasStore for PipMoatStore {
    async fn get(
        &self,
        _tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, PipAdapterError> {
        // pip wheels are public: ignore the per-request tenant and use the
        // shared PUBLIC namespace so identical wheels dedup across tenants.
        self.moat
            .get(PUBLIC_NAMESPACE, &digest.to_hex())
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => PipAdapterError::Cas(m),
            })
    }

    async fn put(
        &self,
        _tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), PipAdapterError> {
        self.moat
            .put(PUBLIC_NAMESPACE, &digest.to_hex(), bytes)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => PipAdapterError::Cas(m),
            })
    }
}

/// Pip's `KvStore` port (the MUTABLE simple-index cache) → a per-tenant
/// D1 table. The simple index is re-fetched on TTL expiry, so it cannot
/// live in the content-addressed moat; it is stored per-tenant (private,
/// isolated) keyed by `(namespace = tenant_uuid, kv_key)`.
///
/// Reuses the SAME [`D1HttpClient`] the moat map uses (no new
/// connection). Requires D1 migration `adapter_pip_index` (see
/// `new_files`). The `(value, inserted_at_unix_ms)` tuple backs the
/// adapter's TTL freshness check.
#[derive(Debug)]
struct PipIndexKvStore {
    d1: Arc<D1HttpClient>,
}

#[async_trait]
impl KvStore for PipIndexKvStore {
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, PipAdapterError> {
        let ns = tenant.to_string();
        let rows = self
            .d1
            .query(
                "SELECT value, inserted_ms FROM adapter_pip_index \
                 WHERE namespace = ?1 AND kv_key = ?2 LIMIT 1",
                &[
                    serde_json::Value::String(ns),
                    serde_json::Value::String(key.to_owned()),
                ],
            )
            .await
            .map_err(PipAdapterError::Kv)?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        // `value` is stored as a hex TEXT column (D1's HTTP API has no BLOB
        // type); decode back to bytes. Index payloads are UTF-8 JSON, but we
        // round-trip via hex to stay binary-safe + column-stable. `hex` is
        // already a container dep; this keeps the path lint-clean (no manual
        // indexing) vs a hand-rolled base64.
        let value_hex = row.get("value").and_then(|v| v.as_str()).ok_or_else(|| {
            PipAdapterError::Kv("D1 adapter_pip_index: missing `value`".to_owned())
        })?;
        let value = hex::decode(value_hex)
            .map_err(|e| PipAdapterError::Kv(format!("D1 adapter_pip_index: value decode: {e}")))?;
        let inserted_ms = row
            .get("inserted_ms")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                PipAdapterError::Kv("D1 adapter_pip_index: missing `inserted_ms`".to_owned())
            })?;
        Ok(Some((value, inserted_ms)))
    }

    async fn put(
        &self,
        tenant: &TenantId,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), PipAdapterError> {
        let ns = tenant.to_string();
        let value_hex = hex::encode(&value);
        self.d1
            .query(
                "INSERT INTO adapter_pip_index (namespace, kv_key, value, inserted_ms) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(namespace, kv_key) DO UPDATE SET \
                   value = excluded.value, inserted_ms = excluded.inserted_ms",
                &[
                    serde_json::Value::String(ns),
                    serde_json::Value::String(key.to_owned()),
                    serde_json::Value::String(value_hex),
                    serde_json::Value::from(inserted_at_unix_ms),
                ],
            )
            .await
            .map_err(PipAdapterError::Kv)?;
        Ok(())
    }
}

/// Thin shell wrapping the shared [`PatVerifier`] as pip's
/// `TenantResolver`. Maps the verifier's tenant-uuid TEXT onto the
/// adapter's [`TenantId`] (the adapter's port returns a typed id, unlike
/// brew's string port).
#[derive(Debug)]
struct PipPatResolver(Arc<PatVerifier>);

#[async_trait]
impl TenantResolver for PipPatResolver {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, PipAdapterError> {
        let tenant_text = self.0.verify(pat_plaintext).await.map_err(|e| match e {
            VerifyError::InvalidPat => PipAdapterError::Auth("invalid PAT".to_owned()),
            VerifyError::Backend(m) => PipAdapterError::Cas(format!("verifier backend: {m}")),
        })?;
        let uuid = uuid::Uuid::parse_str(&tenant_text).map_err(|e| {
            // D1 stores canonical uuid text; a non-uuid here is a backend
            // invariant break, not a client-auth failure.
            PipAdapterError::Cas(format!(
                "verifier returned non-uuid tenant `{tenant_text}`: {e}"
            ))
        })?;
        Ok(TenantId::from_uuid(uuid))
    }
}

/// Build the `/pip/*` sub-router from shared CAS handlers + the
/// url→hash map (reused for the wheel moat) + the PAT verifier.
///
/// The `cas_read`/`cas_write` are the SAME trait objects the
/// cas/ac/bazel/turbo/brew surfaces use; `map` is the D1-backed
/// url→content-hash store (also the D1 client reused for the index KV);
/// `verifier` is shared across cache adapters. On a construction error
/// the route is simply NOT mounted (empty sub-router + logged) so the
/// container still boots.
///
/// # SSRF / upstream hosts
///
/// `config.upstream_pypi` pins the INDEX host (`pypi.org`). The adapter's
/// wheel fetch follows the absolute `url` from the parsed index, which on
/// real PyPI is `files.pythonhosted.org` — a DIFFERENT host. The pip
/// adapter's `UpstreamClient::fetch_wheel` does NOT apply brew's
/// single-origin SSRF guard (it fetches the index-supplied absolute URL
/// as-is). See `prefix_fix` + `open_decisions` for the required SSRF
/// allowlist (both PyPI hosts) inside the adapter's upstream module.
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    d1: Arc<D1HttpClient>,
    verifier: Arc<PatVerifier>,
) -> Router {
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        PIP_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn CasStore> = Arc::new(PipMoatStore { moat });
    let metadata_kv: Arc<dyn KvStore> = Arc::new(PipIndexKvStore { d1 });
    let resolver: Arc<dyn TenantResolver> = Arc::new(PipPatResolver(verifier));
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());

    let upstream_pypi = match Url::parse(PIP_UPSTREAM_DEFAULT) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "/pip upstream URL invalid; pip NOT mounted");
            return Router::new();
        }
    };

    let upstream = match UpstreamClient::new(upstream_pypi.clone()) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::error!(error = ?e, "/pip upstream client build failed; pip NOT mounted");
            return Router::new();
        }
    };

    let config = PipAdapterConfig::new(
        // bind_addr is unused by `build_router` (only `run_pip_adapter` binds).
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        upstream_pypi,
        DEFAULT_INDEX_TTL_SECONDS,
        DEFAULT_WHEEL_SIZE_LIMIT_BYTES,
        true, // prefer PEP 691 JSON index
        cas,
        metadata_kv,
        resolver,
        auditor,
    );

    let state = AdapterState {
        config: Arc::new(config),
        upstream,
    };

    // `build_router` is infallible for pip (returns Router, not Result). The
    // gate is an OUTER layer wrapping the WHOLE `/pip` mount (NOT an inner layer
    // on the adapter): pip's adapter routes are parameterized
    // (`/simple/:project/`, `/pkg/:sha256/:filename`), and axum 0.7's
    // `nest_service` matches the post-strip remainder against those params at
    // routing time — before an inner layer could rewrite it. An inner gate that
    // drops the tenant segment would run too late (the `/<tenant>/<rest>`
    // remainder never matches the params ⇒ 404). Wrapping the mount lets the
    // gate rewrite `/pip/<tenant>/<rest>` → `/pip/<rest>` BEFORE `nest_service`
    // routes. See [`pip_gate`].
    let adapter = build_router(state);
    Router::new()
        .nest_service("/pip", adapter)
        .layer(middleware::from_fn(pip_gate))
}

/// Gate layer: per-operation cache-scope enforcement + tenant-segment
/// strip.
///
/// Runs as an OUTER layer (BEFORE `nest_service` strips `/pip`), so
/// `req.uri().path()` is the FULL `/pip/<tenant>/<rest>`. (1) enforces the
/// per-op scope from the server-trusted `x-corelink-scope` header (GET ⇒
/// read); (2) rewrites the path to `/pip/<rest>` — dropping the `<tenant>`
/// segment while KEEPING the static `/pip` prefix so `nest_service` then
/// strips `/pip` and hands `/<rest>` to the adapter's typed routes (and the
/// upstream fetch sees the real PEP path, never the tenant segment). Running
/// outer is required because axum 0.7 matches the nested param routes at
/// routing time, before an inner layer could rewrite the path (see [`router`]).
async fn pip_gate(mut req: Request, next: Next) -> Response {
    let scope = req
        .headers()
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let scope_ok = match *req.method() {
        // pip is a read-through cache: clients only GET (simple index +
        // wheels + healthz). The transparent cache-fill (a CAS/KV write by
        // the service) is not a client write.
        Method::GET | Method::HEAD => requires_cache_read(scope),
        // pip exposes no client write surface; gate defensively anyway.
        Method::PUT => requires_cache_write(scope),
        _ => true,
    };
    if !scope_ok {
        return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
    }

    // Drop the `<tenant>` segment but KEEP the `/pip` mount prefix:
    // `/pip/<tenant>/<rest>` → `/pip/<rest>` (so `nest_service` strips `/pip`
    // and the adapter sees `/<rest>`). A path not under `/pip/` is left
    // untouched (it cannot reach this mount anyway).
    if let Some(rest_with_tenant) = req.uri().path().strip_prefix("/pip/") {
        let rewritten = match rest_with_tenant.split_once('/') {
            Some((_tenant, rest)) => format!("/pip/{rest}"),
            // Just `/pip/<tenant>` with no sub-path → `/pip/` (adapter 404s).
            None => "/pip/".to_owned(),
        };
        let new_path_and_query = match req.uri().query() {
            Some(q) => format!("{rewritten}?{q}"),
            None => rewritten,
        };
        match Uri::builder().path_and_query(new_path_and_query).build() {
            Ok(u) => *req.uri_mut() = u,
            Err(e) => {
                tracing::warn!(error = %e, "pip_gate path rewrite failed");
                return (StatusCode::BAD_REQUEST, "bad path").into_response();
            }
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
    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId as PatTenantId,
        SCOPE_CACHE_RW,
    };
    use tower::ServiceExt; // for `.oneshot`
    use uuid::Uuid;

    use corelink_adapter_host::pip::wheel::sha256_hex;

    use crate::adapter_cache::canonical_hash_hex;
    use crate::adapter_pat::{PatRow, PatRowLookup};

    use super::*;

    const SCOPE_RW: &str = "cas:rw";

    /// `PatRowLookup` that knows ONE token_id → row; everything else unknown.
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

    /// `PatRowLookup` that knows nothing (rejects every token).
    struct EmptyLookup;
    #[async_trait]
    impl PatRowLookup for EmptyLookup {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            Ok(None)
        }
    }

    /// In-memory url→content-hash map (level-2 of the moat).
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

    /// Non-verifying CAS stub (accepts any claimed_hash) — `pip::router`
    /// wires the production `canonical_hash_hex`, which a verifying
    /// in-memory handler would reject. Keyed by `(namespace, hash)`.
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

    // `(tenant_uuid, kv_key) → (value, inserted_ms)` rows for the fake index KV.
    type FakeKvRows = HashMap<(String, String), (Vec<u8>, u64)>;

    /// In-memory pip index KV (mirrors `PipIndexKvStore` semantics) — keyed
    /// by `(tenant_uuid, kv_key)` so the wheel route can find a seeded
    /// index. The D1-backed prod impl is exercised via D1 integration; this
    /// fake keeps the route test hermetic. We swap it in via the test-only
    /// `router_with` builder (the prod `router` always uses the D1 store).
    #[derive(Debug, Default)]
    struct FakeKv(Mutex<FakeKvRows>);
    #[async_trait]
    impl KvStore for FakeKv {
        async fn get(
            &self,
            tenant: &TenantId,
            key: &str,
        ) -> Result<Option<(Vec<u8>, u64)>, PipAdapterError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(tenant.to_string(), key.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            tenant: &TenantId,
            key: &str,
            value: Vec<u8>,
            inserted_at_unix_ms: u64,
        ) -> Result<(), PipAdapterError> {
            self.0.lock().unwrap().insert(
                (tenant.to_string(), key.to_owned()),
                (value, inserted_at_unix_ms),
            );
            Ok(())
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Build the pip sub-router from explicit shells (test-only) so we can
    /// inject the in-memory `FakeKv` instead of the D1-backed store. Mirrors
    /// `router` exactly otherwise (moat under PUBLIC, gate layer, nest).
    fn router_with(
        cas_read: Arc<dyn CasReadHandler>,
        cas_write: Arc<dyn CasWriteHandler>,
        map: Arc<dyn UrlMapStore>,
        kv: Arc<dyn KvStore>,
        verifier: Arc<PatVerifier>,
    ) -> Router {
        let moat = Arc::new(MoatCache::production(
            cas_read,
            cas_write,
            map,
            PIP_SERVICE_PRINCIPAL,
        ));
        let cas: Arc<dyn CasStore> = Arc::new(PipMoatStore { moat });
        let resolver: Arc<dyn TenantResolver> = Arc::new(PipPatResolver(verifier));
        let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());
        let upstream_pypi = Url::parse(PIP_UPSTREAM_DEFAULT).unwrap();
        let upstream = Arc::new(UpstreamClient::new(upstream_pypi.clone()).unwrap());
        let config = PipAdapterConfig::new(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            upstream_pypi,
            DEFAULT_INDEX_TTL_SECONDS,
            DEFAULT_WHEEL_SIZE_LIMIT_BYTES,
            true,
            cas,
            kv,
            resolver,
            auditor,
        );
        let state = AdapterState {
            config: Arc::new(config),
            upstream,
        };
        // Mirror prod `router`: the gate is an OUTER layer wrapping the mount.
        let adapter = build_router(state);
        Router::new()
            .nest_service("/pip", adapter)
            .layer(middleware::from_fn(pip_gate))
    }

    /// Router whose verifier rejects ALL PATs (empty lookup); stores unused.
    fn router_rejecting() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        router_with(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(FakeKv::default()),
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
            .oneshot(get(
                "/pip/t/simple/requests/",
                Some("corelink_whatever"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn missing_pat_is_401_reaches_adapter() {
        // scope present → gate passes → adapter authenticate fails → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/pip/t/simple/requests/", None, Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_prefix_pat_is_401() {
        // After the prefix-fix the adapter rejects a non-`corelink_` token at
        // the auth shim. (Before the fix, pip deferred the prefix check to the
        // resolver, which the EmptyLookup still rejects → 401 either way.)
        let app = router_rejecting();
        let resp = app
            .oneshot(get(
                "/pip/t/simple/requests/",
                Some("ghp_github"),
                Some(SCOPE_RW),
            ))
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
                "/pip/t/simple/requests/",
                Some("corelink_not-a-real-token"),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cache_hit_round_trip_with_tenant_stripped() {
        // Mint a real PAT; seed (a) the 2-level moat so the wheel BYTES are a
        // cache HIT (no upstream), and (b) the per-tenant index KV so the
        // wheel route can resolve the project entry. Proves end-to-end:
        // nest_service mount + tenant-strip + scope + Option-B resolve +
        // index KV read + moat get + sha256→blake3 digest fork.
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xBEEF);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
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

        // The wheel bytes + their PyPI sha256 (the adapter's CAS Digest key).
        let wheel_bytes = b"PK\x03\x04fake-wheel-zip".to_vec();
        let sha256 = sha256_hex(&wheel_bytes); // 64 hex chars
        let filename = "requests-2.31.0-py3-none-any.whl";

        // Seed the moat: map (PUBLIC, sha256) → blake3(bytes); CAS holds bytes
        // under (PUBLIC, blake3). This is the sha256→blake3 fork in action.
        let blake3 = canonical_hash_hex(&wheel_bytes);
        let cas = Arc::new(StubCas::default());
        cas.0.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), blake3.clone()),
            wheel_bytes.clone(),
        );
        let map = Arc::new(FakeMap::default());
        map.0
            .lock()
            .unwrap()
            .insert((PUBLIC_NAMESPACE.to_owned(), sha256.clone()), blake3);

        // Seed the per-tenant index KV with a PEP 691 payload that names the
        // wheel + its sha256 (the wheel route reads this to find the file).
        let index_json = serde_json::json!({
            "name": "requests",
            "files": [{
                "filename": filename,
                "url": format!("https://files.pythonhosted.org/packages/xx/{filename}#sha256={sha256}"),
                "hashes": { "sha256": sha256 },
                "requires-python": ">=3.7",
                "yanked": false,
            }],
        });
        let kv = Arc::new(FakeKv::default());
        let kv_key = format!("pip:idx:{}", "requests");
        kv.0.lock().unwrap().insert(
            (tenant_uuid.to_string(), kv_key),
            (serde_json::to_vec(&index_json).unwrap(), now_ms()),
        );

        let app = router_with(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            Arc::clone(&kv) as Arc<dyn KvStore>,
            verifier,
        );

        // path-tenant "ignored" ≠ the PAT tenant → proves the path tenant is
        // stripped + untrusted (the wheel bytes are the shared PUBLIC moat).
        let resp = app
            .oneshot(get(
                &format!("/pip/ignored/pkg/{sha256}/{filename}"),
                Some(&pt),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), wheel_bytes.as_slice());
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}
