// `/cargo/<tenant>/<key>` — sccache HTTP build-cache surface.
//
// Mounts the `corelink_adapter_host::cargo` adapter (the sccache HTTP
// storage backend: `GET`/`PUT`/`HEAD /<key>`) into the container router.
// This closes FINDING-sccache-adapter-gaps §Gap 1: the Worker already
// forwards `/cargo/*` to the container, but the adapter was never mounted,
// so every forwarded request 404'd.
//
// # Path shape
//
// The Worker forwards the FULL path `/cargo/<tenant>/<key>` (the
// `<tenant>` segment is used only for DO routing). The adapter's own
// router is `/:key` (a single segment) because the adapter derives the
// tenant from the PAT, never from the path. We bridge the two shapes with
// `nest_service("/cargo", …)`, which strips the static `/cargo` prefix and
// hands `/<tenant>/<key>` to a thin gate layer that:
//
// 1. enforces the per-operation cache scope (GET/HEAD ⇒ read, PUT ⇒ write)
//    from the Worker-set, server-trusted `x-corelink-scope` header; and
// 2. rewrites the request path `/<tenant>/<key>` → `/<key>` so the
//    adapter's `/:key` route matches.
//
// `nest_service` (not `nest`) is required: `nest` would flatten the
// adapter's `/:key` into a 2-segment matcher (`/cargo/:key`) and reject
// the 3-segment `/cargo/<tenant>/<key>` before the gate ever runs.
//
// # Tenant + scope trust model
//
// Tenant identity comes from the PAT, re-verified in the container against
// the D1 `pat` store ([`crate::adapter_pat::PatVerifier`] — the ONE verifier
// shared by cargo/brew/npm/pip, wrapped here via [`resolver_from_verifier`];
// Option B — HMAC + Argon2id possession check). The path `<tenant>` is
// NEVER trusted for storage. The scope gate here is the per-operation
// layer (read vs write) on top of the resolver's "has cache capability"
// check, mirroring the H1 scope spine used by cas/ac/turbo/bazel.
//
// ## Two-layer write enforcement (F27)
//
// For PUT requests the gate enforces BOTH:
//
// 1. `x-corelink-scope` header must carry write capability (Worker-set,
//    server-trusted from D1); AND
// 2. The bearer PAT's `can_write` bit, obtained from the SAME single PAT
//    verification the resolver already runs to derive the tenant
//    (`TenantResolver::resolve_with_capability` — HMAC + Argon2id against D1).
//
// This eliminates the single-header-trust gap (F27): even if the Worker ever
// injected a wrong scope header, the resolver's PAT-derived `can_write` bit
// would block the write. Mirrors the OCI adapter's two-layer model — and there
// is exactly ONE PAT verification per request (via the resolver port), not a
// redundant second one.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;

use async_trait::async_trait;
use corelink_adapter_host::cargo::config::DEFAULT_BODY_SIZE_LIMIT_BYTES;
use corelink_adapter_host::cargo::ports::{
    CasError, CasStore, ResolvedTenant, SharedTenantResolver, TenantResolveError, TenantResolver,
};
use corelink_adapter_host::cargo::translate::key_from_path;
use corelink_adapter_host::cargo::{server, CargoAdapterConfig};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_handler_cas::{CasReadHandler, CasWriteHandler, CasWriteOperationContext};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore};
use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::scope::{requires_cache_read, requires_cache_write, SCOPE_HEADER};
use crate::storage::staging_load_test_admission::StagingLoadTestAdmissionGate;

/// Service principal recorded on adapter CAS operations. Identifies the
/// adapter-host service, NOT the end-user PAT (which the resolver verified).
const CARGO_SERVICE_PRINCIPAL: &str = "cargo-adapter-host";

/// cargo's `CasStore` port → the 2-level [`MoatCache`], namespaced PER-TENANT.
///
/// sccache is a key→value cache: the key is `blake3(rustc-cmdline + input
/// fingerprints)` — a hash of the compile INPUTS, NOT of the cached OUTPUT bytes.
/// The previous `CargoCasBridge` passed that key straight through as the CAS
/// `digest_hex`, but the CAS write VERIFIES `claimed == blake3(content)` (see
/// `handler.rs::HashMismatch`), so every sccache PUT failed integrity and 502'd.
///
/// Routing through `MoatCache` fixes it: `put` computes `content_hash =
/// blake3(bytes)`, stores the blob content-addressed (the verify now passes —
/// claimed == actual), and records `(namespace, key) → content_hash` in the
/// url-map; `get` resolves the map then fetches the blob. Identical to
/// [`super::brew::BrewMoatStore`] EXCEPT the namespace is the **tenant id** — the
/// cargo cache is PRIVATE per-tenant (no cross-tenant dedup; never `_public`).
///
/// ## Fresh-tenant cap seeding (`cap_resolver`)
///
/// A tenant that has NEVER done a native CAS write has no `tenant_storage_state`
/// row. The byte-accounting reservation FAILS CLOSED (502) when asked to seed
/// such a row with an indeterminate cap (`None`) — so a brand-new sccache user's
/// FIRST `PUT /cargo/<tenant>/<key>` 502'd. We close that exactly as OCI does
/// (WP #10): resolve the tenant's RESOLVED per-tier storage cap container-side
/// via the shared [`crate::oci_cap::TenantCapResolver`] (keyed by the tenant id
/// the cargo adapter already derived from the PAT) and thread it into
/// [`MoatCache::put`] so the row auto-seeds with the REAL cap on the first write.
///
/// Routed requests use the Worker-set cap from [`CARGO_STORAGE_QUOTA_CAP`], so
/// they do not repeat the D1 tier lookup. `cap_resolver` remains an `Option` as
/// a direct-call fallback for dev/CI and callers that bypass the gate; an
/// indeterminate cap from that resolver (D1 error) stays `None` — absence is
/// NEVER treated as unlimited.
#[derive(Debug)]
struct CargoMoatStore {
    moat: Arc<MoatCache>,
    cap_resolver: Option<Arc<dyn crate::oci_cap::TenantCapResolver>>,
}

tokio::task_local! {
    /// The Worker-authenticated storage cap for the current cargo request.
    ///
    /// The cargo gate scopes this value around the downstream adapter call.
    /// `Some` is a valid cap; `None` deliberately preserves the byte-accounting
    /// fail-closed behavior when the trusted header is absent or invalid.
    static CARGO_STORAGE_QUOTA_CAP: Option<i64>;
}

#[async_trait]
impl CasStore for CargoMoatStore {
    async fn get(&self, tenant_id: &str, key: &str) -> Result<Option<Vec<u8>>, CasError> {
        // PRIVATE per-tenant namespace = the tenant id (NOT brew's `_public`).
        // `key` is the sccache key; the moat maps it to the content hash.
        self.moat.get(tenant_id, key).await.map_err(|e| match e {
            MoatError::Backend(m) => CasError::Backend(m),
        })
    }

    async fn put(&self, tenant_id: &str, key: &str, bytes: Vec<u8>) -> Result<(), CasError> {
        self.put_with_context(tenant_id, key, bytes, None).await
    }

    async fn put_with_context(
        &self,
        tenant_id: &str,
        key: &str,
        bytes: Vec<u8>,
        context: Option<Arc<dyn corelink_handler_cas::CasWriteOperationContext>>,
    ) -> Result<(), CasError> {
        // Routed requests already carry the Worker-authenticated cap. Reusing
        // it avoids a fresh D1 tier lookup on every PUT. The scoped `None`
        // value is intentional: absent or invalid trusted headers remain
        // fail-closed and must not fall back to an untrusted client value.
        // Direct adapter calls retain the resolver fallback for dev/CI and
        // callers that do not pass through `cargo_gate`.
        let storage_cap_bytes = match CARGO_STORAGE_QUOTA_CAP.try_with(|cap| *cap) {
            Ok(cap) => cap,
            Err(_) => match self.cap_resolver.as_ref() {
                Some(r) => r.resolve_storage_cap(tenant_id).await,
                None => None,
            },
        };
        self.moat
            .put_with_context(tenant_id, key, bytes, storage_cap_bytes, context)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => CasError::Backend(m),
            })
    }
}

/// Thin shell adapting the shared [`PatVerifier`] (Option B) to cargo's
/// `TenantResolver` port. Identical pattern to brew/npm/pip: the container
/// holds ONE `PatVerifier` and every adapter wraps it, so the
/// HMAC → D1 → Argon2id → fail-CLOSED-scope pipeline lives once in
/// [`crate::adapter_pat`].
#[derive(Debug)]
struct CargoPatResolver(Arc<PatVerifier>);

#[async_trait]
impl TenantResolver for CargoPatResolver {
    async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
        self.0.verify(pat_plaintext).await.map_err(|e| match e {
            VerifyError::InvalidPat => TenantResolveError::InvalidPat,
            VerifyError::Backend(m) => TenantResolveError::Backend(m),
        })
    }

    /// Override: call `verify_capability` so the write-gate can use the
    /// D1-verified `can_write` bit instead of trusting only the header.
    async fn resolve_with_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<ResolvedTenant, TenantResolveError> {
        let (tenant_id, can_write, runner_job) = self
            .0
            .verify_capability_full(pat_plaintext)
            .await
            .map_err(|e| match e {
                VerifyError::InvalidPat => TenantResolveError::InvalidPat,
                VerifyError::Backend(m) => TenantResolveError::Backend(m),
            })?;
        Ok(ResolvedTenant {
            tenant_id,
            can_write,
            runner_job,
        })
    }
}

/// Wrap the shared [`PatVerifier`] as cargo's injectable [`SharedTenantResolver`].
/// The container build path calls this so cargo shares the one verifier with
/// brew/npm/pip rather than constructing a second D1-backed resolver.
#[must_use]
pub fn resolver_from_verifier(verifier: Arc<PatVerifier>) -> SharedTenantResolver {
    Arc::new(CargoPatResolver(verifier))
}

/// Gate state bundling the optional $-ceiling gate and the tenant resolver
/// needed for two-layer write enforcement (F27). The resolver's
/// `resolve_with_capability` yields the PAT-derived `can_write` bit from the
/// SAME single verification the adapter uses to resolve the tenant — so the gate
/// does NOT run a redundant second PAT verify. `Clone`-cheap (all fields
/// arc-shaped).
#[derive(Clone)]
struct CargoGateState {
    quota: Option<crate::routes::QuotaGate>,
    resolver: SharedTenantResolver,
    staging_admission: Option<Arc<StagingLoadTestAdmissionGate>>,
    /// The SAME per-tenant 2-level moat the adapter's [`CargoMoatStore`] wraps —
    /// held here so the gate can serve the WebDAV `PROPFIND` (stat) and `DELETE`
    /// (write-check cleanup) that opendal issues but axum's `MethodRouter` cannot
    /// route (a non-standard / unregistered method). Reused, not a second store.
    moat: Arc<MoatCache>,
}

/// Opaque admission carried across Cargo's async and blocking CAS bridges.
#[derive(Debug)]
struct StagingCargoWriteContext(
    Arc<crate::storage::staging_load_test_admission::StagingLoadTestAdmissionContext>,
);

impl CasWriteOperationContext for StagingCargoWriteContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
/// Build the `/cargo/*` sub-router from shared CAS handlers + a PAT→tenant
/// resolver. The SAME resolver backs both the adapter (tenant resolution) and
/// the gate's two-layer write enforcement (F27) — one PAT verification, not two.
///
/// The `resolver` is injected so production wires the shared [`PatVerifier`]
/// (via [`resolver_from_verifier`]) while tests pass a hermetic stub. The CAS
/// handlers are the SAME `Arc<dyn …>` trait objects the cas/bazel/turbo
/// surfaces use (no new R2 connection).
///
/// `cap_resolver` resolves the tenant's RESOLVED per-tier storage cap so a fresh
/// tenant's FIRST cargo write auto-seeds its `tenant_storage_state` row instead
/// of failing closed (502). Production wires the shared
/// [`crate::oci_cap::D1TenantCapResolver`] (the SAME resolver OCI uses); tests /
/// dev-CI pass `None` (keeping the previous fail-closed-on-fresh-row posture).
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    resolver: SharedTenantResolver,
    quota: Option<crate::routes::QuotaGate>,
    cap_resolver: Option<Arc<dyn crate::oci_cap::TenantCapResolver>>,
) -> Router {
    // 2-level moat (key→content_hash→blob), namespaced per-tenant — see
    // [`CargoMoatStore`] for why the old direct-digest bridge 502'd.
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        CARGO_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn CasStore> = Arc::new(CargoMoatStore {
        moat: Arc::clone(&moat),
        cap_resolver,
    });
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());
    // bind_addr is unused by `build_router` (only `run_cargo_adapter` binds);
    // pass an ephemeral placeholder.
    let config = CargoAdapterConfig::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        DEFAULT_BODY_SIZE_LIMIT_BYTES,
        cas,
        resolver.clone(),
        auditor,
    );

    // The gate layer carries both the optional $-ceiling gate and the tenant
    // resolver so PUT requests are subject to two-layer write enforcement (F27):
    // scope header check AND the PAT-derived `can_write` bit from the resolver's
    // single verification (no redundant second PAT verify).
    let gate_state = CargoGateState {
        quota,
        resolver,
        staging_admission: StagingLoadTestAdmissionGate::from_env().ok().map(Arc::new),
        moat,
    };
    // The co-read hint layer is added LAST ⇒ it is the OUTERMOST layer, so the
    // cell it scopes covers `cargo_gate` too — which matters because the gate
    // serves PROPFIND itself (resolver + moat) rather than delegating to the
    // adapter.
    let adapter = server::build_router(config)
        .layer(middleware::from_fn_with_state(gate_state, cargo_gate))
        .layer(middleware::from_fn(cargo_coread_hint));
    Router::new().nest_service("/cargo", adapter)
}

/// The server-trusted tenant the Worker resolved from the PAT and stamped on
/// the forward. A *hint* for the co-read only — the moat is keyed by the tenant
/// the container derives from the PAT itself, never by this.
const TENANT_HINT_HEADER: &str = "x-corelink-tenant-id";

/// Publish the co-read hint for a cargo READ so the container's per-request D1
/// `pat` read can carry the url-map lookup in the same round trip.
///
/// # Why only the read verbs
///
/// `GET` / `HEAD` / `PROPFIND` are exactly the requests whose storage step is a
/// url-map **read** keyed by data already known before the PAT resolves (the
/// key is the last path segment). `PUT` writes the map (an upsert, not a read)
/// and `DELETE` removes it; `MKCOL` touches no storage. Hinting those would add
/// a wasted index probe to the co-read and buy nothing, so they keep the plain
/// `pat` statement.
///
/// # Why this is not a trust change
///
/// The hint's namespace is the Worker-set, client-unsettable
/// `x-corelink-tenant-id` (the same header the $-ceiling gate already attributes
/// cost by, and which the Worker cross-checks against the URL tenant). It
/// decides NOTHING: it only says which row to fetch alongside the `pat` row. The
/// fetched row is served only if it matches the namespace the moat is actually
/// called with — the PAT-derived tenant — so a wrong or stale hint costs one
/// wasted index probe and the storage read then happens for real. See
/// [`crate::d1_coread`].
async fn cargo_coread_hint(req: Request, next: Next) -> Response {
    let is_read =
        matches!(*req.method(), Method::GET | Method::HEAD) || req.method().as_str() == "PROPFIND";
    if !is_read {
        return next.run(req).await;
    }
    // Both inputs must be present and well-formed, else there is nothing to
    // co-read and we leave the request on the unchanged serial path.
    let namespace = req
        .headers()
        .get(TENANT_HINT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    // The SAME normalization the adapter's GET/HEAD and the gate's PROPFIND use
    // (`key_from_path`), so the hinted key is byte-identical to the one the moat
    // will ask for — a mismatch here would silently cost a wasted probe.
    let key = key_from_path(&webdav_path(&req));
    match (namespace, key) {
        (Some(ns), Some(k)) => crate::d1_coread::scope(ns, k, next.run(req)).await,
        _ => next.run(req).await,
    }
}
