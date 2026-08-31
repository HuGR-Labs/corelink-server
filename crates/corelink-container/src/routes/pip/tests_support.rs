//! Shared fixtures for the `routes::pip` test modules.
//!
//! Every hermetic double the pip route tests need lives here — the three
//! `PatRowLookup` shells (known token / absent token / backend fault), the
//! in-memory moat halves (`StubCas` + `FakeMap`), the in-memory index KV, and
//! five router builders that mirror the production [`super::router`] wiring
//! while swapping the D1-backed index store for `FakeKv`.
//!
//! It is a fixture file, not a property file: nothing here asserts. It sits
//! in its own module rather than inside one of the test files because all
//! five property files draw on it, and hosting it in a sibling would make that
//! sibling look load-bearing for the others when it is only a neighbour.
//!
//! The builders are the load-bearing part: `router_with`,
//! `router_with_put_probe`, `router_with_quota`,
//! `router_with_quota_observable`, and `router_rejecting` must keep mirroring
//! `super::router`'s layering — the gate as an OUTER layer wrapping the `/pip`
//! mount, and ONE resolver shared by the adapter and the gate. A test that
//! builds the layers differently from production stops testing production.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode};
use corelink_handler_cas::{
    CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
};
use corelink_pat::PatSigningKey;

use crate::adapter_pat::{PatRow, PatRowLookup};

use super::*;

/// The full read+write cache scope, as the Worker stamps it on
/// `x-corelink-scope` and as the seeded `PatRow` records it.
pub(super) const SCOPE_RW: &str = "cas:rw";

/// `PatRowLookup` that knows ONE token_id → row; everything else unknown.
pub(super) struct OneTokenLookup {
    pub(super) token_id: String,
    pub(super) row: PatRow,
}
#[async_trait]
impl PatRowLookup for OneTokenLookup {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        Ok((token_id == self.token_id).then(|| self.row.clone()))
    }
}

/// `PatRowLookup` that knows nothing (rejects every token).
pub(super) struct EmptyLookup;
#[async_trait]
impl PatRowLookup for EmptyLookup {
    async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
        Ok(None)
    }
}

/// `PatRowLookup` whose D1-equivalent read failed. Kept distinct from
/// [`EmptyLookup`]: an absent row is an invalid credential, while this error
/// must prove the F27 resolver-error arm fails closed before the request can
/// reach the adapter.
#[derive(Default)]
pub(super) struct FailingLookup {
    calls: AtomicUsize,
}
impl FailingLookup {
    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
#[async_trait]
impl PatRowLookup for FailingLookup {
    async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err("test D1 lookup unavailable".to_owned())
    }
}

/// In-memory url→content-hash map (level-2 of the moat).
#[derive(Default)]
pub(super) struct FakeMap(pub(super) Mutex<HashMap<(String, String), String>>);
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
pub(super) struct StubCas(pub(super) Mutex<HashMap<(String, String), Vec<u8>>>);
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
pub(super) type FakeKvRows = HashMap<(String, String), (Vec<u8>, u64)>;

/// In-memory pip index KV (mirrors `PipIndexKvStore` semantics) — keyed
/// by `(tenant_uuid, kv_key)` so the wheel route can find a seeded
/// index. The D1-backed prod impl is exercised via D1 integration; this
/// fake keeps the route test hermetic. We swap it in via the test-only
/// `router_with` builder (the prod `router` always uses the D1 store).
#[derive(Debug, Default)]
pub(super) struct FakeKv(pub(super) Mutex<FakeKvRows>);
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

pub(super) fn test_key() -> Arc<PatSigningKey> {
    Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
}

/// Build the pip sub-router from explicit shells (test-only) so we can
/// inject the in-memory `FakeKv` instead of the D1-backed store. Mirrors
/// `router` exactly otherwise (moat under PUBLIC, gate layer, nest).
pub(super) fn router_with(
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
    // The SAME resolver backs the adapter (tenant resolution) AND the gate's
    // two-layer write check — one PAT verification, not two.
    let resolver: TenantResolverHandle = Arc::new(PipPatResolver(verifier));
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
        resolver.clone(),
        auditor,
    );
    let state = AdapterState {
        config: Arc::new(config),
        upstream,
    };
    // Mirror prod `router`: the gate is an OUTER layer wrapping the mount.
    // The SAME resolver is threaded in for two-layer write enforcement (F27).
    let adapter = build_router(state);
    let gate_state = PipGateState {
        resolver,
        quota: None,
    };
    Router::new()
        .nest_service("/pip", adapter)
        .layer(middleware::from_fn_with_state(gate_state, pip_gate))
}

/// Like [`router_with`], except the otherwise-unmapped test PUT has a tiny
/// downstream handler. This makes the valid-write control prove the gate
/// called `next.run`: a gate-local 405 cannot impersonate this 204 response.
pub(super) fn router_with_put_probe(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    kv: Arc<dyn KvStore>,
    verifier: Arc<PatVerifier>,
) -> (Router, Arc<AtomicUsize>) {
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        PIP_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn CasStore> = Arc::new(PipMoatStore { moat });
    let resolver: TenantResolverHandle = Arc::new(PipPatResolver(verifier));
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
        resolver.clone(),
        auditor,
    );
    let state = AdapterState {
        config: Arc::new(config),
        upstream,
    };
    let probe = Arc::new(AtomicUsize::new(0));
    let probe_for_handler = Arc::clone(&probe);
    let adapter = build_router(state).route(
        "/simple/requests/",
        axum::routing::put(move || {
            let probe = Arc::clone(&probe_for_handler);
            async move {
                probe.fetch_add(1, Ordering::SeqCst);
                StatusCode::NO_CONTENT
            }
        }),
    );
    let gate_state = PipGateState {
        resolver,
        quota: None,
    };
    let router = Router::new()
        .nest_service("/pip", adapter)
        .layer(middleware::from_fn_with_state(gate_state, pip_gate));
    (router, probe)
}

/// Like [`router_with`] but with an ACTIVE per-tenant `$`-ceiling quota
/// gate (hermetic in-memory store + fake clock). Used to prove the gate's
/// cost-attribution does NOT fall open when no tenant id is available
/// (REV-S3).
pub(super) fn router_with_quota(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    kv: Arc<dyn KvStore>,
    verifier: Arc<PatVerifier>,
) -> Router {
    router_with_quota_observable(cas_read, cas_write, map, kv, verifier).0
}

/// [`router_with_quota`], plus a handle on the quota store the gate charges.
///
/// Status alone cannot answer *which* tenant was billed: the fixture's $1/op
/// against a fresh tenant is admitted whichever label is used, so an
/// attribution bug is invisible from the response. Reading the store back
/// through `QuotaStore::get` is what makes the REV-S3 claim — that a write is
/// charged to the PAT-derived tenant and not to a caller-supplied header —
/// an assertion rather than an inference.
pub(super) fn router_with_quota_observable(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    kv: Arc<dyn KvStore>,
    verifier: Arc<PatVerifier>,
) -> (Router, Arc<crate::tenant_quota::InMemoryQuotaStore>) {
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        PIP_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn CasStore> = Arc::new(PipMoatStore { moat });
    let resolver: TenantResolverHandle = Arc::new(PipPatResolver(verifier));
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
        resolver.clone(),
        auditor,
    );
    let state = AdapterState {
        config: Arc::new(config),
        upstream,
    };
    let adapter = build_router(state);
    let store = Arc::new(crate::tenant_quota::InMemoryQuotaStore::new());
    let clock = Arc::new(crate::wall_clock::InMemoryFakeWallClock::at_unix_ms(
        1_700_000_000_000,
    ));
    let guard = Arc::new(crate::tenant_quota::QuotaGuard::new(
        Arc::clone(&store) as Arc<dyn crate::tenant_quota::QuotaStore>,
        clock,
    ));
    // $1/op flat cost — a fresh tenant (under the $5 tripwire) would be
    // ADMITTED, so a 503 here is unambiguously the no-tenant fail-CLOSED
    // path, not an over-ceiling 402.
    let gate = crate::routes::QuotaGate::new_for_test(guard, 1_000_000);
    let gate_state = PipGateState {
        resolver,
        quota: Some(gate),
    };
    let router = Router::new()
        .nest_service("/pip", adapter)
        .layer(middleware::from_fn_with_state(gate_state, pip_gate));
    (router, store)
}

/// Router whose verifier rejects ALL PATs (empty lookup); stores unused.
pub(super) fn router_rejecting() -> Router {
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

pub(super) fn get(uri: &str, pat: Option<&str>, scope: Option<&str>) -> HttpRequest<Body> {
    let mut b = HttpRequest::builder().method(Method::GET).uri(uri);
    if let Some(p) = pat {
        b = b.header("authorization", format!("Bearer {p}"));
    }
    if let Some(s) = scope {
        b = b.header(SCOPE_HEADER, s);
    }
    b.body(Body::empty()).unwrap()
}

/// A PUT, the only method that reaches the F27 two-layer write arm.
///
/// Carries the tenant header separately from the PAT on purpose: the whole
/// point of F27/REV-S3 is that these two can DISAGREE, and every interesting
/// case is one where they do. `tenant_header` is `Option` so a test can omit
/// it entirely and prove the write still has an attribution key — which only
/// holds if that key came from the PAT.
pub(super) fn put(
    uri: &str,
    pat: Option<&str>,
    scope: Option<&str>,
    tenant_header: Option<&str>,
) -> HttpRequest<Body> {
    let mut b = HttpRequest::builder().method(Method::PUT).uri(uri);
    if let Some(p) = pat {
        b = b.header("authorization", format!("Bearer {p}"));
    }
    if let Some(s) = scope {
        b = b.header(SCOPE_HEADER, s);
    }
    if let Some(t) = tenant_header {
        b = b.header("x-corelink-tenant-id", t);
    }
    b.body(Body::empty()).unwrap()
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
