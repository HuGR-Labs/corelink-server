//! `/cargo/<tenant>/<key>` — sccache HTTP build-cache surface.
//!
//! Mounts the `corelink_adapter_host::cargo` adapter (the sccache HTTP
//! storage backend: `GET`/`PUT`/`HEAD /<key>`) into the container router.
//! This closes FINDING-sccache-adapter-gaps §Gap 1: the Worker already
//! forwards `/cargo/*` to the container, but the adapter was never mounted,
//! so every forwarded request 404'd.
//!
//! # Path shape
//!
//! The Worker forwards the FULL path `/cargo/<tenant>/<key>` (the
//! `<tenant>` segment is used only for DO routing). The adapter's own
//! router is `/:key` (a single segment) because the adapter derives the
//! tenant from the PAT, never from the path. We bridge the two shapes with
//! `nest_service("/cargo", …)`, which strips the static `/cargo` prefix and
//! hands `/<tenant>/<key>` to a thin gate layer that:
//!
//! 1. enforces the per-operation cache scope (GET/HEAD ⇒ read, PUT ⇒ write)
//!    from the Worker-set, server-trusted `x-corelink-scope` header; and
//! 2. rewrites the request path `/<tenant>/<key>` → `/<key>` so the
//!    adapter's `/:key` route matches.
//!
//! `nest_service` (not `nest`) is required: `nest` would flatten the
//! adapter's `/:key` into a 2-segment matcher (`/cargo/:key`) and reject
//! the 3-segment `/cargo/<tenant>/<key>` before the gate ever runs.
//!
//! # Tenant + scope trust model
//!
//! Tenant identity comes from the PAT, re-verified in the container against
//! the D1 `pat` store ([`crate::adapter_pat::PatVerifier`] — the ONE verifier
//! shared by cargo/brew/npm/pip, wrapped here via [`resolver_from_verifier`];
//! Option B — HMAC + Argon2id possession check). The path `<tenant>` is
//! NEVER trusted for storage. The scope gate here is the per-operation
//! layer (read vs write) on top of the resolver's "has cache capability"
//! check, mirroring the H1 scope spine used by cas/ac/turbo/bazel.
//!
//! ## Two-layer write enforcement (F27)
//!
//! For PUT requests the gate enforces BOTH:
//!
//! 1. `x-corelink-scope` header must carry write capability (Worker-set,
//!    server-trusted from D1); AND
//! 2. The bearer PAT's `can_write` bit, obtained from the SAME single PAT
//!    verification the resolver already runs to derive the tenant
//!    (`TenantResolver::resolve_with_capability` — HMAC + Argon2id against D1).
//!
//! This eliminates the single-header-trust gap (F27): even if the Worker ever
//! injected a wrong scope header, the resolver's PAT-derived `can_write` bit
//! would block the write. Mirrors the OCI adapter's two-layer model — and there
//! is exactly ONE PAT verification per request (via the resolver port), not a
//! redundant second one.

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
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore};
use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::scope::{requires_cache_read, requires_cache_write, SCOPE_HEADER};

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
/// `cap_resolver` is `Option` so dev/CI (no D1 storage env) keeps the previous
/// `None`/fail-closed posture; an indeterminate cap from the resolver (D1 error)
/// also stays `None` — absence is NEVER treated as unlimited.
#[derive(Debug)]
struct CargoMoatStore {
    moat: Arc<MoatCache>,
    cap_resolver: Option<Arc<dyn crate::oci_cap::TenantCapResolver>>,
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
        // Resolve the tenant's RESOLVED per-tier storage cap so a FRESH tenant
        // (no `tenant_storage_state` row — e.g. a new sccache user whose first
        // request is a cargo PUT) auto-seeds the row with the REAL cap instead
        // of hitting the `None`-cap fail-closed 502. Mirrors OCI (WP #10), keyed
        // by the tenant id the adapter already derived from the PAT. No resolver
        // (dev/CI) OR an indeterminate cap (D1 error) ⇒ `None` ⇒ the previous
        // fail-closed posture — absence is never treated as unlimited.
        let storage_cap_bytes = match self.cap_resolver.as_ref() {
            Some(r) => r.resolve_storage_cap(tenant_id).await,
            None => None,
        };
        self.moat
            .put(tenant_id, key, bytes, storage_cap_bytes)
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
    /// The SAME per-tenant 2-level moat the adapter's [`CargoMoatStore`] wraps —
    /// held here so the gate can serve the WebDAV `PROPFIND` (stat) and `DELETE`
    /// (write-check cleanup) that opendal issues but axum's `MethodRouter` cannot
    /// route (a non-standard / unregistered method). Reused, not a second store.
    moat: Arc<MoatCache>,
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

/// Gate layer for the cargo surface: per-operation cache-scope enforcement +
/// two-layer write enforcement (F27).
///
/// Runs as a `layer` on the adapter router (which is a catch-all `/*path`, so
/// it matches the nested `/cargo/<tenant>/<key>` shape directly — no path
/// rewrite needed). The gate enforces:
///
/// 1. **Scope header check** — the Worker-set, server-trusted
///    `x-corelink-scope` header must carry the required capability (read or
///    write). An insufficient scope → 403.
/// 2. **PAT-derived `can_write` for writes (F27)** — for PUT requests the gate
///    ALSO extracts the bearer PAT and calls
///    `TenantResolver::resolve_with_capability()` to obtain the D1-verified
///    `can_write` bit from the SAME single PAT verification the adapter uses to
///    resolve the tenant (no redundant second verify). A PAT whose D1 record
///    lacks write capability is rejected (403) even if the header says `cas:rw`.
///    This makes cargo two-layer for writes, matching the OCI model.
/// 3. **$-ceiling gate** — per-tenant monthly cost check (ADR-0068).
///
/// Read requests (GET/HEAD) are NOT subject to the extra `can_write` check; the
/// header check alone is sufficient for reads (the PAT resolver in the adapter
/// still runs and rejects any invalid PAT → 401).
async fn cargo_gate(
    axum::extract::State(state): axum::extract::State<CargoGateState>,
    mut req: Request,
    next: Next,
) -> Response {
    let scope = req
        .headers()
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // WebDAV MKCOL: opendal (sccache's WebDAV backend) creates parent "directories"
    // before PUTting a sharded key (e.g. `6/b/4/<hash>`). The cargo store is a FLAT
    // content-addressed KV — directories are implicit — so MKCOL is a success no-op.
    // Gate it on cache-WRITE scope (it is part of a write flow) so the surface stays
    // fail-closed. Without this, opendal's dir-creation 403s and the real `sccache`
    // binary can never write (a real-client gap invisible to curl, which PUTs the
    // sharded key directly and never issues MKCOL). Root-caused live 2026-07-19.
    if req.method().as_str() == "MKCOL" {
        return if requires_cache_write(scope) {
            (StatusCode::CREATED, "").into_response()
        } else {
            (StatusCode::FORBIDDEN, "insufficient cache scope").into_response()
        };
    }

    // WebDAV PROPFIND (opendal "stat"): sccache's opendal WebDAV backend PROPFINDs
    // a key (Depth: 0) to check existence + size around the `.sccache_check`
    // write-probe and directory handling. The flat content-addressed KV has no
    // native stat verb, so we synthesize a WebDAV `207 Multi-Status` from the
    // per-tenant moat lookup (present) or `404` (absent — opendal then proceeds to
    // write). It is a READ, gated on cache-read scope. Short-circuited HERE (like
    // MKCOL) because PROPFIND is a NON-STANDARD method that axum's `MethodRouter`
    // cannot route — falling through to the adapter would 405 and the real
    // `sccache` binary could never stat. Root-caused live 2026-07-19.
    if req.method().as_str() == "PROPFIND" {
        if !requires_cache_read(scope) {
            return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
        }
        // Extract owned inputs BEFORE awaiting so no borrow of `req` (whose body
        // is not `Sync`) crosses an `.await` — that would make the gate future
        // non-`Send` and `from_fn` would reject it.
        let path = webdav_path(&req);
        let bearer = bearer_owned(&req);
        return handle_propfind(
            Arc::clone(&state.moat),
            state.resolver.clone(),
            path,
            bearer,
        )
        .await;
    }

    // WebDAV DELETE (opendal write-check cleanup): sccache PUTs `.sccache_check`,
    // reads it back, then DELETEs it. Remove the per-tenant key→content-hash map
    // row so a later GET/PROPFIND misses (the CAS blob is left for GC). It is a
    // WRITE — gated on cache-write scope AND the PAT's D1 `can_write` bit (F27,
    // mirroring PUT), keyed by the PAT-resolved tenant (NEVER the path tenant).
    // Idempotent: 204 even if the key was absent. Short-circuited here (not routed
    // to the adapter, whose `MethodRouter` has no DELETE route → 405) so ALL of
    // the sccache WebDAV surface lives on ONE path.
    if req.method() == Method::DELETE {
        if !requires_cache_write(scope) {
            return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
        }
        let path = webdav_path(&req);
        let bearer = bearer_owned(&req);
        return handle_delete(
            Arc::clone(&state.moat),
            state.resolver.clone(),
            path,
            bearer,
        )
        .await;
    }

    let is_write = req.method() == Method::PUT;
    let scope_ok = match *req.method() {
        Method::PUT => requires_cache_write(scope),
        Method::GET | Method::HEAD => requires_cache_read(scope),
        // Fail-CLOSED: the adapter only routes GET/HEAD/PUT (so anything else
        // would 405 downstream today), but the gate must not assume that —
        // an unmapped method is denied here so a future adapter route can
        // never ship without an explicit scope decision. No browser/CORS
        // clients exist on this surface, so OPTIONS is not legitimate traffic.
        _ => false,
    };
    if !scope_ok {
        return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
    }

    // F27 — two-layer write enforcement: for PUT requests, require the PAT's own
    // `can_write` bit from the resolver's SINGLE PAT verification
    // (`resolve_with_capability` — HMAC + Argon2id against D1). This ensures a
    // Worker-side scope-header mistake cannot grant a write that the PAT's D1
    // record does not authorise — without a redundant second verify (the adapter
    // already verifies the same PAT to resolve the tenant). Read requests skip
    // this (the adapter's resolver still verifies the PAT for auth; only the
    // write-capability cross-check is gated here).
    // REV-S3: for PUT, the PAT-derived tenant id from the F27 verify below is the
    // AUTHORITATIVE cost-attribution key (it cannot be spoofed by a Worker-set
    // header). We capture it here and prefer it over `x-corelink-tenant-id` for
    // the $-ceiling gate.
    let mut resolved_tenant_for_quota: Option<String> = None;
    if is_write {
        let pat_token = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(str::to_owned);
        match pat_token {
            None => {
                tracing::warn!("cargo: PUT with no bearer token — rejecting (F27)");
                return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
            }
            Some(pat_plaintext) => {
                match state.resolver.resolve_with_capability(&pat_plaintext).await {
                    Ok(resolved) if resolved.can_write => {
                        // PAT grants write — both layers pass; continue.
                        // REV-S3: thread the authoritative, crypto-verified
                        // tenant id from THIS single PAT verification into a
                        // typed request extension so the downstream adapter's
                        // `handle_put` reuses it instead of running Argon2id a
                        // second time. Server-internal typed storage — not
                        // client-settable — so this is not a trust downgrade.
                        resolved_tenant_for_quota = Some(resolved.tenant_id.clone());
                        req.extensions_mut()
                            .insert(server::GateResolvedTenant(resolved.tenant_id));
                    }
                    Ok(_no_write) => {
                        tracing::warn!(
                            "cargo: PUT denied — PAT scope lacks write capability (F27)"
                        );
                        return (StatusCode::FORBIDDEN, "PAT does not grant write capability")
                            .into_response();
                    }
                    Err(TenantResolveError::Backend(m)) => {
                        tracing::error!(error = %m, "cargo: resolver backend error (F27)");
                        return (
                            StatusCode::SERVICE_UNAVAILABLE,
                            "PAT verifier backend error",
                        )
                            .into_response();
                    }
                    // `InvalidPat` + any future non-exhaustive variant: fail-CLOSED
                    // (the PAT did not resolve, so the write is denied → 401).
                    Err(e) => {
                        tracing::warn!(error = %e, "cargo: PUT denied — PAT re-verify failed (F27)");
                        return (StatusCode::UNAUTHORIZED, "invalid PAT").into_response();
                    }
                }
            }
        }
    }

    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost AFTER the scope gate, BEFORE the adapter runs PAT auth +
    // CAS. 402 over-ceiling / 503 fail-CLOSED.
    //
    // REV-S3 (quota fail-OPEN closed): the cost-attribution tenant is sourced,
    // in priority order:
    //   1. the PAT-resolved tenant id from the F27 verify above (PUT only) —
    //      AUTHORITATIVE, cannot be spoofed by a Worker-set header; then
    //   2. the Worker-set, server-trusted `x-corelink-tenant-id` header
    //      (the only source available for reads, where no PAT verify runs in
    //      this gate). A forged header can over-charge only ITS OWN tenant.
    // If the gate is configured but NO tenant id is available, we now fail
    // CLOSED (503) instead of silently skipping the charge: a missing label is
    // a Worker header-injection regression (or direct container access) and
    // must surface immediately rather than letting a tenant exceed its
    // $-ceiling unmetered.
    if let Some(gate) = state.quota.as_ref() {
        let tenant = match resolved_tenant_for_quota {
            Some(ref t) if !t.is_empty() => t.as_str(),
            _ => req
                .headers()
                .get("x-corelink-tenant-id")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .unwrap_or(""),
        };
        if tenant.is_empty() {
            tracing::error!(
                "cargo: quota gate active but no tenant id for cost attribution \
                 (missing x-corelink-tenant-id and no PAT-resolved tenant) — \
                 failing CLOSED (REV-S3)"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "cost-attribution tenant unavailable",
            )
                .into_response();
        }
        if let Some(resp) = gate.check(tenant).await {
            return resp;
        }
    }
    next.run(req).await
}

/// The WebDAV request path used for the `<D:href>` and key extraction.
///
/// Behind `nest_service("/cargo", …)` the middleware may observe the
/// prefix-stripped path (`/<tenant>/<key>`); the outer router preserves the full
/// wire path (`/cargo/<tenant>/<key>`) in the [`axum::extract::OriginalUri`]
/// extension. We prefer the original so the `href` echoes exactly what opendal
/// sent. The key is the LAST path segment either way, so key extraction is
/// unaffected by which one we use. Returns an OWNED `String` so no borrow of the
/// request (whose body is not `Sync`) is held across an `.await`.
fn webdav_path(req: &Request) -> String {
    req.extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|o| o.0.path().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned())
}

/// Extract the `Bearer` PAT as an OWNED `String` (so it is not borrowed from the
/// non-`Sync` request body across an `.await`, which would make the gate future
/// non-`Send` and break `from_fn`).
fn bearer_owned(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_owned)
}

/// `PROPFIND /cargo/<tenant>/<key>` — synthesize a WebDAV stat from the moat.
///
/// Scope (cache-read) is enforced by the caller. Resolves the tenant from the
/// PAT (never the path), mirroring the adapter's GET/HEAD auth, then serves a
/// `207 Multi-Status` (present, with the real byte length) or `404` (absent).
///
/// Takes fully OWNED inputs (the caller extracts them from the request before
/// the first `.await`) so the gate future stays `Send`.
async fn handle_propfind(
    moat: Arc<MoatCache>,
    resolver: SharedTenantResolver,
    path: String,
    bearer: Option<String>,
) -> Response {
    // A collection stat (trailing slash ⇒ empty last segment): opendal stats the
    // tenant/dir root. Answer a minimal `207` collection — it discloses nothing
    // beyond "this is a directory" and needs no moat/auth lookup.
    if path.ends_with('/') {
        return propfind_collection_response(&path);
    }

    // Use the SAME normalization GET/PUT use so the stat resolves the identical
    // moat key. A key those paths would reject cannot exist ⇒ not-found.
    let key = match key_from_path(&path) {
        Some(k) => k,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let tenant_id = match resolve_read_tenant(&resolver, bearer).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    match moat.get(&tenant_id, &key).await {
        Ok(Some(bytes)) => propfind_file_response(&path, bytes.len() as u64),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(MoatError::Backend(m)) => {
            tracing::error!(error = %m, "cargo PROPFIND: moat backend error");
            (StatusCode::BAD_GATEWAY, "cache backend error").into_response()
        }
    }
}

/// `DELETE /cargo/<tenant>/<key>` — remove the per-tenant key→hash map row.
///
/// Scope (cache-write) is enforced by the caller. Applies the F27 second layer
/// (the PAT's D1-verified `can_write` bit) and keys the delete by the
/// PAT-resolved tenant. Idempotent: `204` even if the key was absent. All `req`
/// borrows are dropped before the first `.await` (gate future must be `Send`).
/// Is `key` an sccache build ARTIFACT (a 64-hex object digest), as opposed to
/// one of sccache's non-hex control keys (notably `.sccache_check`)?
///
/// `normalize_key` deliberately admits BOTH shapes — rejecting the control keys
/// is what made the real client deem the backend unusable once — so the two are
/// distinguished here rather than at the door. Only artifacts are worth
/// protecting from eviction; a control key is written and deleted by the client
/// within one health probe and holds nothing.
fn is_object_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

async fn handle_delete(
    moat: Arc<MoatCache>,
    resolver: SharedTenantResolver,
    path: String,
    bearer: Option<String>,
) -> Response {
    // F27 two-layer write: require a bearer PAT whose D1 record grants write.
    let pat = match bearer {
        Some(p) => p,
        None => return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response(),
    };
    let (tenant_id, runner_job) = match resolver.resolve_with_capability(&pat).await {
        Ok(resolved) if resolved.can_write => (resolved.tenant_id, resolved.runner_job),
        Ok(_no_write) => {
            return (StatusCode::FORBIDDEN, "PAT does not grant write capability").into_response();
        }
        Err(TenantResolveError::Backend(m)) => {
            tracing::error!(error = %m, "cargo DELETE: resolver backend error");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "PAT verifier backend error",
            )
                .into_response();
        }
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid PAT").into_response(),
    };

    let key = match key_from_path(&path) {
        Some(k) => k,
        // A key GET/PUT would reject cannot exist ⇒ idempotent no-op success.
        None => return StatusCode::NO_CONTENT.into_response(),
    };

    // 0086 runner-job containment (AUDIT-2026-08-23-CACHE-INTEGRITY-COVERAGE
    // F-1). The native plane refuses CAS DELETE outright for a runner-job
    // credential — "a stolen per-job credential must not be able to EVICT the
    // tenant's cache" — but this plane could not see the marker at all until
    // `PatRow::runner_job` existed, the same structural blindness ADR-0071 closed
    // for `find_only`.
    //
    // ⚠️ The refusal is NARROW ON PURPOSE, and a blanket one would take down the
    // runner fleet. sccache's startup write-check does PUT `.sccache_check` →
    // GET → DELETE, and the runner fabric's own dogfood IS sccache over CoreLink
    // with a runner-minted PAT. A 400 on that control key already cost us a
    // "real client never worked" outage once (see `translate::normalize_key`), so
    // the write-check MUST keep round-tripping. Only a real build ARTIFACT — a
    // 64-hex object key — is protected here; every control key still deletes.
    if runner_job && is_object_key(&key) {
        return (
            StatusCode::FORBIDDEN,
            "artifact delete not permitted for a runner-job credential",
        )
            .into_response();
    }

    match moat.delete(&tenant_id, &key).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(MoatError::Backend(m)) => {
            tracing::error!(error = %m, "cargo DELETE: moat backend error");
            (StatusCode::BAD_GATEWAY, "cache backend error").into_response()
        }
    }
}

/// Resolve the tenant from an OWNED bearer PAT for a READ (never the path
/// tenant). `Err(response)` carries the 401/503 to return on failure.
async fn resolve_read_tenant(
    resolver: &SharedTenantResolver,
    bearer: Option<String>,
) -> Result<String, Response> {
    let pat = match bearer {
        Some(p) => p,
        None => {
            return Err((StatusCode::UNAUTHORIZED, "missing bearer token").into_response());
        }
    };
    match resolver.resolve(&pat).await {
        Ok(t) => Ok(t),
        Err(TenantResolveError::Backend(m)) => {
            tracing::error!(error = %m, "cargo PROPFIND: resolver backend error");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "PAT verifier backend error",
            )
                .into_response())
        }
        Err(_) => Err((StatusCode::UNAUTHORIZED, "invalid PAT").into_response()),
    }
}

/// Fixed `getlastmodified` for every PROPFIND `207`. opendal's WebDAV stat
/// deserializer treats `<D:getlastmodified>` as a REQUIRED field: a 207 without
/// it fails with `missing field getlastmodified`, so the real `sccache` binary
/// flags the whole storage ReadOnly and never writes (invisible to a 207-status
/// check; caught only by a cold-store to warm-HIT round-trip). CAS objects are
/// immutable + content-addressed, so a stable epoch httpdate is correct and keeps
/// the response deterministic. RFC 1123 format.
const PROPFIND_LAST_MODIFIED: &str = "Thu, 01 Jan 1970 00:00:00 GMT";

/// `207 Multi-Status` for an EXISTING key: a `200 OK` propstat carrying
/// `getcontentlength` and `getlastmodified` — the exact shape opendal's WebDAV
/// stat parser accepts (empirically proven against the real `sccache` 0.15 binary).
fn propfind_file_response(href: &str, size_bytes: u64) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>{href}</D:href>\
         <D:propstat><D:prop><D:resourcetype/>\
         <D:getcontentlength>{size_bytes}</D:getcontentlength>\
         <D:getlastmodified>{PROPFIND_LAST_MODIFIED}</D:getlastmodified></D:prop>\
         <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>",
        href = xml_escape(href),
    );
    multistatus_response(body)
}

/// `207 Multi-Status` for a collection (directory) stat — `resourcetype` carries
/// `<D:collection/>`. opendal stats the tenant/dir root; this satisfies it.
fn propfind_collection_response(href: &str) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>{href}</D:href>\
         <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype>\
         <D:getlastmodified>{PROPFIND_LAST_MODIFIED}</D:getlastmodified></D:prop>\
         <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>",
        href = xml_escape(href),
    );
    multistatus_response(body)
}

/// Build a `207 Multi-Status` response with `Content-Type: application/xml`.
fn multistatus_response(body: String) -> Response {
    let mut resp = Response::new(axum::body::Body::from(body));
    *resp.status_mut() = StatusCode::MULTI_STATUS;
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/xml"),
    );
    resp
}

/// Minimal XML text escaping for the `<D:href>` value. Cargo keys are already a
/// constrained safe alphabet (see `key_from_path`), so this is defense-in-depth.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };

    use crate::oci_cap::TenantCapResolver;

    use super::*;

    /// Hermetic in-memory url→content-hash map (the `MoatCache` map port).
    #[derive(Default)]
    struct FakeUrlMap(Mutex<HashMap<(String, String), String>>);
    #[async_trait]
    impl UrlMapStore for FakeUrlMap {
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
        async fn delete(&self, ns: &str, url_hash: &str) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .remove(&(ns.to_owned(), url_hash.to_owned()));
            Ok(())
        }
    }

    /// `CasWriteHandler` that RECORDS the `storage_quota_bytes` of the last
    /// write request (proving the resolved cap is threaded all the way into
    /// `CasWriteRequest`), and accepts any claimed hash.
    #[derive(Debug, Default)]
    struct RecordingCasWrite {
        last_cap: Mutex<Option<Option<i64>>>,
    }
    impl CasWriteHandler for RecordingCasWrite {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            *self.last_cap.lock().unwrap() = Some(req.storage_quota_bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }
    impl CasReadHandler for RecordingCasWrite {
        fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            Err(CasHandlerError::Internal("not used".into()))
        }
    }

    /// `TenantCapResolver` that returns a FIXED cap for the configured tenant
    /// and `None` (indeterminate) for everyone else.
    #[derive(Debug)]
    struct StubCapResolver {
        tenant: String,
        cap: Option<i64>,
    }
    #[async_trait]
    impl TenantCapResolver for StubCapResolver {
        async fn resolve_storage_cap(&self, tenant_id: &str) -> Option<i64> {
            if tenant_id == self.tenant {
                self.cap
            } else {
                None
            }
        }
    }

    fn store_with_resolver(
        cap_resolver: Option<Arc<dyn TenantCapResolver>>,
    ) -> (CargoMoatStore, Arc<RecordingCasWrite>) {
        let rec = Arc::new(RecordingCasWrite::default());
        let read: Arc<dyn CasReadHandler> = rec.clone();
        let write: Arc<dyn CasWriteHandler> = rec.clone();
        let map: Arc<dyn UrlMapStore> = Arc::new(FakeUrlMap::default());
        let moat = Arc::new(MoatCache::production(
            read,
            write,
            map,
            CARGO_SERVICE_PRINCIPAL,
        ));
        (CargoMoatStore { moat, cap_resolver }, rec)
    }

    /// THE fix: a fresh tenant's cargo write carries the RESOLVED per-tier cap
    /// (so the byte-accounting reservation seeds the `tenant_storage_state` row
    /// instead of failing closed 502 on the indeterminate `None`).
    #[tokio::test]
    async fn put_threads_resolved_cap_into_cas_write() {
        let tenant = "fresh-tenant-xyz";
        let resolver: Arc<dyn TenantCapResolver> = Arc::new(StubCapResolver {
            tenant: tenant.to_owned(),
            cap: Some(50 * 1_073_741_824), // solo tier cap
        });
        let (store, rec) = store_with_resolver(Some(resolver));

        store
            .put(tenant, "url-key-1", b"sccache-artifact".to_vec())
            .await
            .expect("put must succeed");

        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(
            recorded,
            Some(50 * 1_073_741_824),
            "the resolved per-tier cap must reach CasWriteRequest::storage_quota_bytes \
             so a fresh tenant_storage_state row seeds (no 502)"
        );
    }

    /// With NO resolver wired (dev/CI), the cap stays `None` — the previous
    /// fail-closed-on-fresh-row posture is preserved.
    #[tokio::test]
    async fn put_with_no_resolver_keeps_none_cap() {
        let (store, rec) = store_with_resolver(None);
        store
            .put("any-tenant", "url-key-2", b"x".to_vec())
            .await
            .expect("put must succeed");
        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(
            recorded, None,
            "no resolver ⇒ None cap (fail-closed posture)"
        );
    }

    /// An INDETERMINATE cap from the resolver (D1 error → `None`) is threaded as
    /// `None` — absence is never upgraded to unlimited.
    #[tokio::test]
    async fn put_with_indeterminate_cap_threads_none() {
        // The resolver only knows "known-tenant"; everyone else ⇒ None.
        let resolver: Arc<dyn TenantCapResolver> = Arc::new(StubCapResolver {
            tenant: "known-tenant".to_owned(),
            cap: Some(10 * 1_073_741_824),
        });
        let (store, rec) = store_with_resolver(Some(resolver));
        store
            .put("unknown-tenant", "url-key-3", b"y".to_vec())
            .await
            .expect("put must succeed");
        let recorded = rec.last_cap.lock().unwrap().expect("a write happened");
        assert_eq!(
            recorded, None,
            "indeterminate resolver cap must stay None (never unlimited)"
        );
    }

    // ---- cargo_gate: WebDAV MKCOL no-op (sccache real-client fix) --------------

    /// A `TenantResolver` that MUST NOT be called: MKCOL short-circuits in the
    /// gate before any PAT verification, so reaching the resolver is a bug.
    #[derive(Debug)]
    struct UnusedResolver;
    #[async_trait]
    impl TenantResolver for UnusedResolver {
        async fn resolve(&self, _pat_plaintext: &str) -> Result<String, TenantResolveError> {
            panic!("MKCOL must short-circuit before the resolver is ever called");
        }
    }

    /// Router carrying ONLY the `cargo_gate` layer over a fallback that 200s if
    /// reached. MKCOL requests short-circuit in the gate and never hit the
    /// fallback, so this exercises `cargo_gate` in isolation (no adapter/CAS).
    fn gate_only_router() -> Router {
        use axum::routing::any;
        let resolver: SharedTenantResolver = Arc::new(UnusedResolver);
        let state = CargoGateState {
            quota: None,
            resolver,
            // MKCOL short-circuits before any moat access; a real (unused) store
            // keeps the state well-formed.
            moat: in_memory_moat(),
        };
        Router::new()
            .fallback(any(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(state, cargo_gate))
    }

    /// A hermetic [`MoatCache`] over an in-memory CAS + fake map, wired with
    /// `fake_hash` (matching `InMemoryCasHandler`'s verification). Seed it via
    /// `moat.put(tenant, key, bytes, None)` and read it via `moat.get`.
    fn in_memory_moat() -> Arc<MoatCache> {
        use corelink_handler_cas::handler::fake_hash;
        use corelink_handler_cas::{InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver};
        let cas = Arc::new(InMemoryCasHandler::new(
            Arc::new(InMemoryAuditSink::new()),
            Arc::new(InMemorySliObserver::new()),
        ));
        let map: Arc<dyn UrlMapStore> = Arc::new(FakeUrlMap::default());
        Arc::new(MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            CARGO_SERVICE_PRINCIPAL,
        ))
    }

    fn mkcol_request(scope: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method(Method::from_bytes(b"MKCOL").unwrap())
            .uri("/cargo/tenant-abc/6/b/4")
            .header(SCOPE_HEADER, scope)
            .body(axum::body::Body::empty())
            .unwrap()
    }

    /// MKCOL + a cache-WRITE scope → 201 CREATED (success no-op). This is the
    /// exact path opendal issues before PUTting a sharded key; without it the
    /// real `sccache` binary can never write.
    #[tokio::test]
    async fn mkcol_with_write_scope_is_201_noop() {
        use tower::ServiceExt;
        let resp = gate_only_router()
            .oneshot(mkcol_request("cas:rw"))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "MKCOL with cache-write scope must succeed as a directory-creation no-op"
        );
    }

    /// MKCOL + a read-only scope → 403 FORBIDDEN. MKCOL is part of a write flow,
    /// so the surface stays fail-closed on a credential that lacks write.
    #[tokio::test]
    async fn mkcol_with_readonly_scope_is_403() {
        use tower::ServiceExt;
        let resp = gate_only_router()
            .oneshot(mkcol_request("cas:r"))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "MKCOL with a read-only scope must be denied (still gated on cache-write)"
        );
    }

    // ---- cargo_gate: WebDAV PROPFIND + DELETE (real-sccache/opendal fix) --------

    /// A resolver that resolves ANY PAT to a fixed tenant with a fixed
    /// write-capability (so the PROPFIND read path + DELETE F27 path reach the
    /// moat). Distinct from `UnusedResolver`, which panics.
    #[derive(Debug)]
    struct FixedTenantResolver {
        tenant: String,
        can_write: bool,
        /// The 0086 narrowing marker. `false` for every case that predates the
        /// runner-job containment; the DELETE cells set it explicitly.
        runner_job: bool,
    }
    #[async_trait]
    impl TenantResolver for FixedTenantResolver {
        async fn resolve(&self, _pat: &str) -> Result<String, TenantResolveError> {
            Ok(self.tenant.clone())
        }
        async fn resolve_with_capability(
            &self,
            _pat: &str,
        ) -> Result<ResolvedTenant, TenantResolveError> {
            Ok(ResolvedTenant {
                tenant_id: self.tenant.clone(),
                can_write: self.can_write,
                runner_job: self.runner_job,
            })
        }
    }

    /// Router carrying the `cargo_gate` over the given moat + resolver. GET/PUT/
    /// HEAD would hit the 200 fallback; PROPFIND/DELETE short-circuit in the gate.
    fn webdav_router(moat: Arc<MoatCache>, tenant: &str, can_write: bool) -> Router {
        webdav_router_marked(moat, tenant, can_write, false)
    }

    /// [`webdav_router`] with the 0086 runner-job marker set explicitly, for the
    /// containment cells.
    fn webdav_router_marked(
        moat: Arc<MoatCache>,
        tenant: &str,
        can_write: bool,
        runner_job: bool,
    ) -> Router {
        use axum::routing::any;
        let resolver: SharedTenantResolver = Arc::new(FixedTenantResolver {
            tenant: tenant.to_owned(),
            can_write,
            runner_job,
        });
        let state = CargoGateState {
            quota: None,
            resolver,
            moat,
        };
        Router::new()
            .fallback(any(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(state, cargo_gate))
    }

    fn webdav_request(
        method: &[u8],
        uri: &str,
        scope: &str,
        bearer: Option<&str>,
    ) -> axum::http::Request<axum::body::Body> {
        let mut b = axum::http::Request::builder()
            .method(Method::from_bytes(method).unwrap())
            .uri(uri)
            .header(SCOPE_HEADER, scope);
        if let Some(t) = bearer {
            b = b.header(axum::http::header::AUTHORIZATION, format!("Bearer {t}"));
        }
        b.body(axum::body::Body::empty()).unwrap()
    }

    async fn body_string(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// PROPFIND on an EXISTING key → `207` with the right `getcontentlength` and
    /// a `200 OK` propstat (the shape opendal's stat parser accepts).
    #[tokio::test]
    async fn propfind_existing_key_is_207_with_size() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        let bytes = b"sccache-artifact".to_vec(); // 16 bytes
        moat.put(tenant, "abc123object", bytes.clone(), None)
            .await
            .unwrap();
        let app = webdav_router(Arc::clone(&moat), tenant, true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/abc123object",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::MULTI_STATUS);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/xml")
        );
        let body = body_string(resp).await;
        assert!(
            body.contains("<D:getcontentlength>16</D:getcontentlength>"),
            "must report the stored blob's real byte length; got: {body}"
        );
        assert!(
            body.contains("HTTP/1.1 200 OK"),
            "must carry a 200 OK propstat; got: {body}"
        );
        assert!(
            body.contains("/cargo/tenant-abc/abc123object"),
            "href must echo the request path; got: {body}"
        );
        // REGRESSION LOCK: opendal's WebDAV stat deserializer treats
        // <D:getlastmodified> as REQUIRED — a 207 without it fails "missing field
        // getlastmodified", so the real sccache binary flags storage ReadOnly and
        // never writes (invisible to a 207-status check). Empirically proven.
        assert!(
            body.contains("<D:getlastmodified>"),
            "207 MUST carry <D:getlastmodified> or opendal/sccache treats storage as ReadOnly; got: {body}"
        );
    }

    /// PROPFIND on an ABSENT key → `404` (opendal treats it as not-found and
    /// proceeds to write).
    #[tokio::test]
    async fn propfind_absent_key_is_404() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/never-written",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// PROPFIND with a read-ONLY scope is ALLOWED (it is a read) — reaches the
    /// moat and returns `207` for an existing key.
    #[tokio::test]
    async fn propfind_readonly_scope_is_allowed() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        moat.put(tenant, "roobject", b"z".to_vec(), None)
            .await
            .unwrap();
        let app = webdav_router(Arc::clone(&moat), tenant, false);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/roobject",
                "cas:r",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::MULTI_STATUS,
            "PROPFIND is a read — a read-only scope must be sufficient"
        );
    }

    /// PROPFIND with NO cache scope → `403` (fail-closed, before any lookup).
    #[tokio::test]
    async fn propfind_without_scope_is_403() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/whatever",
                "",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// PROPFIND on the collection root (trailing slash) → `207` collection.
    #[tokio::test]
    async fn propfind_collection_root_is_207_collection() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"PROPFIND",
                "/cargo/tenant-abc/",
                "cas:r",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::MULTI_STATUS);
        let body = body_string(resp).await;
        assert!(
            body.contains("<D:collection/>"),
            "a collection stat must carry <D:collection/>; got: {body}"
        );
        // REGRESSION LOCK: the collection 207 is what opendal PROPFINDs on the
        // tenant/dir root during its write-check — it MUST also carry
        // <D:getlastmodified> or the deserialize fails and storage goes ReadOnly.
        assert!(
            body.contains("<D:getlastmodified>"),
            "collection 207 MUST carry <D:getlastmodified> (opendal stat parser); got: {body}"
        );
    }

    /// DELETE an EXISTING key → `204` and the key is GONE from the moat.
    #[tokio::test]
    async fn delete_existing_key_is_204_and_removes_it() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        moat.put(tenant, "delobject", b"bytes".to_vec(), None)
            .await
            .unwrap();
        // Sanity: present before.
        assert!(moat.get(tenant, "delobject").await.unwrap().is_some());
        let check = Arc::clone(&moat);
        let app = webdav_router(moat, tenant, true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(
            check.get(tenant, "delobject").await.unwrap().is_none(),
            "the key must be gone from the moat after DELETE"
        );
    }

    /// 0086 CONTAINMENT (audit F-1): a runner-job credential may NOT delete a
    /// build artifact, even holding a write scope AND the D1 write bit — the
    /// two layers this surface used to stop at. Mirrors the native plane's
    /// `runner_job_cas_delete_returns_403`.
    #[tokio::test]
    async fn runner_job_cannot_delete_an_artifact() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let key = "a".repeat(64); // a 64-hex object key = a real artifact
        let moat = in_memory_moat();
        moat.put(tenant, &key, b"bytes".to_vec(), None)
            .await
            .unwrap();
        let check = Arc::clone(&moat);
        let app = webdav_router_marked(moat, tenant, true, true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                &format!("/cargo/tenant-abc/{key}"),
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "a runner-job credential must not evict the tenant's cache"
        );
        assert!(
            check.get(tenant, &key).await.unwrap().is_some(),
            "the artifact must still be there after the refused DELETE"
        );
    }

    /// The other half of the containment, and the one that keeps the runner
    /// fleet alive: sccache's `.sccache_check` write-check MUST still round-trip
    /// under a runner-job credential. A blanket DELETE refusal would fail every
    /// runner box at startup — the same class of breakage as the 400 that once
    /// made the real client disable the backend entirely.
    #[tokio::test]
    async fn runner_job_can_still_delete_the_sccache_write_check() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let moat = in_memory_moat();
        moat.put(tenant, ".sccache_check", b"probe".to_vec(), None)
            .await
            .unwrap();
        let check = Arc::clone(&moat);
        let app = webdav_router_marked(moat, tenant, true, true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/.sccache_check",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "the write-check control key must stay deletable"
        );
        assert!(
            check.get(tenant, ".sccache_check").await.unwrap().is_none(),
            "the control key must actually be removed"
        );
    }

    /// A NORMAL (non-runner) credential deletes artifacts exactly as before —
    /// the containment must not widen into ordinary cache management.
    #[tokio::test]
    async fn normal_pat_can_still_delete_an_artifact() {
        use tower::ServiceExt;
        let tenant = "tenant-abc";
        let key = "b".repeat(64);
        let moat = in_memory_moat();
        moat.put(tenant, &key, b"bytes".to_vec(), None)
            .await
            .unwrap();
        let check = Arc::clone(&moat);
        let app = webdav_router_marked(moat, tenant, true, false);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                &format!("/cargo/tenant-abc/{key}"),
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(check.get(tenant, &key).await.unwrap().is_none());
    }

    /// `is_object_key` decides the containment, so pin its edges: only exactly
    /// 64 hex chars is an artifact. Anything else is a control key and stays
    /// deletable — erring toward the runner fleet keeping working.
    #[test]
    fn is_object_key_accepts_only_64_hex() {
        assert!(is_object_key(&"a".repeat(64)));
        assert!(is_object_key(&"0123456789abcdef".repeat(4)));
        assert!(is_object_key(&"A".repeat(64)), "uppercase hex is still hex");
        assert!(!is_object_key(&"a".repeat(63)), "too short");
        assert!(!is_object_key(&"a".repeat(65)), "too long");
        assert!(!is_object_key(".sccache_check"), "the control key");
        assert!(!is_object_key(&"g".repeat(64)), "non-hex");
        assert!(!is_object_key(""), "empty");
    }

    /// DELETE with a read-ONLY scope → `403` (it is write-gated).
    #[tokio::test]
    async fn delete_readonly_scope_is_403() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:r",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// DELETE with a write scope but a PAT whose D1 record lacks write (F27) →
    /// `403` — the second layer blocks it even though the header said `cas:rw`.
    #[tokio::test]
    async fn delete_pat_without_write_is_403_f27() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", false); // PAT can_write = false
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// DELETE with no bearer token → `401` (F27 requires a PAT).
    #[tokio::test]
    async fn delete_without_bearer_is_401() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/delobject",
                "cas:rw",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// DELETE of an ABSENT key is idempotent → still `204`.
    #[tokio::test]
    async fn delete_absent_key_is_204_idempotent() {
        use tower::ServiceExt;
        let moat = in_memory_moat();
        let app = webdav_router(moat, "tenant-abc", true);
        let resp = app
            .oneshot(webdav_request(
                b"DELETE",
                "/cargo/tenant-abc/nothing-here",
                "cas:rw",
                Some("pat"),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // ---- The co-read: ONE D1 round trip for the pat row AND the url-map row ---
    //
    // Prod (`1ee76248-r1`, n=65, authed `/cargo` 404 miss): `opat` 62/72/81 and
    // `ostore` 66/75/84 ms — 147 ms, 55 % of a 267 ms `origin` — were two
    // container→D1-primary round trips issued back to back. These tests pin that
    // they are now ONE, and that collapsing them changed nothing about auth.
    //
    // The counter is on the two ports the request crosses D1 through
    // (`PatRowLookup` and `UrlMapStore`), so it counts round trips the way the
    // measurement does, not statements.

    use std::sync::atomic::{AtomicUsize, Ordering};

    use corelink_pat::{mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId};
    use uuid::Uuid;

    use crate::adapter_pat::{PatRow, PatRowLookup};

    /// The D1 seam of the cargo read path, counting round trips.
    ///
    /// `lookup` emulates the production [`crate::storage::d1_http::D1HttpClient`]
    /// contract exactly: when a co-read hint is in scope it satisfies the
    /// url-map lookup in the SAME call and publishes it. `honours_coread =
    /// false` reproduces the serial path (a backend that does not co-read), so
    /// the fallback is exercised by the same fixture.
    struct CountingD1 {
        /// Every D1 round trip the request made, over BOTH ports.
        round_trips: AtomicUsize,
        /// The subset that were SEPARATE url-map reads — the second hop this
        /// change exists to remove.
        map_reads: AtomicUsize,
        /// The live `pat` row, or `None` = absent / expired / REVOKED (the D1
        /// statement filters those out, so the verifier sees no row).
        row: Mutex<Option<PatRow>>,
        /// `(namespace, url_hash) → content_hash`.
        map: Mutex<HashMap<(String, String), String>>,
        /// Forced backend fault on the `pat` read (D1 unreachable).
        pat_err: Option<String>,
        /// Forced backend fault on the separate url-map read.
        map_err: Option<String>,
        honours_coread: bool,
    }

    impl CountingD1 {
        fn new(row: Option<PatRow>) -> Self {
            Self {
                round_trips: AtomicUsize::new(0),
                map_reads: AtomicUsize::new(0),
                row: Mutex::new(row),
                map: Mutex::new(HashMap::new()),
                pat_err: None,
                map_err: None,
                honours_coread: true,
            }
        }
        fn serial(mut self) -> Self {
            self.honours_coread = false;
            self
        }
        fn with_mapping(self, ns: &str, key: &str, content_hash: &str) -> Self {
            self.map
                .lock()
                .unwrap()
                .insert((ns.to_owned(), key.to_owned()), content_hash.to_owned());
            self
        }
        fn trips(&self) -> usize {
            self.round_trips.load(Ordering::SeqCst)
        }
        fn map_reads(&self) -> usize {
            self.map_reads.load(Ordering::SeqCst)
        }
        fn lookup_map(&self, ns: &str, key: &str) -> Option<String> {
            self.map
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), key.to_owned()))
                .cloned()
        }
    }

    impl std::fmt::Debug for CountingD1 {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("CountingD1").finish_non_exhaustive()
        }
    }

    #[async_trait]
    impl PatRowLookup for CountingD1 {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.pat_err {
                return Err(e.clone());
            }
            if self.honours_coread {
                if let Some((cell, ns, key)) = crate::d1_coread::hint() {
                    cell.publish(self.lookup_map(&ns, &key));
                }
            }
            Ok(self.row.lock().unwrap().clone())
        }
    }

    #[async_trait]
    impl UrlMapStore for CountingD1 {
        async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            self.map_reads.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = &self.map_err {
                return Err(e.clone());
            }
            Ok(self.lookup_map(ns, url_hash))
        }
        async fn put(
            &self,
            ns: &str,
            url_hash: &str,
            content_hash: &str,
            _len: u64,
        ) -> Result<(), String> {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            self.map.lock().unwrap().insert(
                (ns.to_owned(), url_hash.to_owned()),
                content_hash.to_owned(),
            );
            Ok(())
        }
    }

    /// A CAS that hands back FIXED bytes for any hash — the moat's own
    /// content-hash re-check decides whether they are served, so a test seeds
    /// the map with `canonical_hash_hex(bytes)` to model a real hit.
    #[derive(Debug)]
    struct FixedBytesCas(Vec<u8>);
    impl CasReadHandler for FixedBytesCas {
        fn read(
            &self,
            req: corelink_handler_cas::CasReadRequest,
        ) -> Result<corelink_handler_cas::CasReadResponse, CasHandlerError> {
            Ok(corelink_handler_cas::CasReadResponse::new(
                self.0.clone(),
                req.hash,
            ))
        }
    }
    impl CasWriteHandler for FixedBytesCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    fn coread_test_key() -> PatSigningKey {
        PatSigningKey::from_bytes(vec![0x42_u8; 32]).unwrap()
    }

    /// Mint a real PAT; returns `(plaintext, pat_hash, tenant_id)`.
    fn mint_cargo_pat(key: &PatSigningKey) -> (String, String, String) {
        let tenant = TenantId(Uuid::from_u128(0x5cca_c4e0_0000_0001));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant,
            PrincipalId(Uuid::from_u128(0x5cca_c4e0_0000_0002)),
            PatScopes::from_u64(corelink_pat::SCOPE_CACHE_RW),
            None,
            key,
            1,
        )
        .unwrap();
        (
            plaintext.into_string(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    /// The REAL cargo router (adapter + gate + co-read hint layer) over the
    /// counting D1 seam and the REAL `PatVerifier` — so the PAT is verified with
    /// real crypto and the only fakes are the two D1 ports.
    fn coread_router(d1: Arc<CountingD1>, key: &PatSigningKey, blob: Vec<u8>) -> Router {
        let verifier = crate::adapter_pat::PatVerifier::with_key_set(
            Arc::clone(&d1) as Arc<dyn PatRowLookup>,
            vec![key.clone()],
        );
        let cas = Arc::new(FixedBytesCas(blob));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::clone(&d1) as Arc<dyn UrlMapStore>,
            resolver_from_verifier(Arc::new(verifier)),
            None,
            None,
        )
    }

    /// A cargo GET exactly as the Worker forwards it: server-trusted scope +
    /// tenant headers, bearer PAT, `/cargo/<tenant>/<key>`.
    fn cargo_get(tenant: &str, key: &str, pat: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/{tenant}/{key}"))
            .header(SCOPE_HEADER, "cas:rw")
            .header(TENANT_HINT_HEADER, tenant)
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pat}"))
            .body(axum::body::Body::empty())
            .unwrap()
    }

    fn live_row(pat_hash: &str, tenant: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: pat_hash.to_owned(),
            scope: "cas:rw".to_owned(),
            find_only: false,
            runner_job: false,
        }
    }

    /// ⭐ THE property. A cargo GET that MISSES — the exact request the prod
    /// probe measured at `opat` 72 + `ostore` 75 ms — must cross to D1 exactly
    /// ONCE, with the `pat` row and the url-map row in the same round trip.
    ///
    /// On the serial path this reads 2 (and `map_reads` 1): that is the
    /// negative-control signature.
    #[tokio::test]
    async fn cargo_get_miss_costs_exactly_one_d1_round_trip() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let d1 = Arc::new(CountingD1::new(Some(live_row(&hash, &tenant))));
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let resp = app
            .oneshot(cargo_get(&tenant, &"a".repeat(64), &pat))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "cache miss ⇒ 404");
        assert_eq!(
            d1.trips(),
            1,
            "the cargo GET issued {} D1 round trips; expected exactly 1 carrying \
             BOTH the pat row and the url-map row (prod: opat 72ms + ostore 75ms \
             = 2 RTTs = 55% of origin)",
            d1.trips()
        );
        assert_eq!(
            d1.map_reads(),
            0,
            "the url-map must NOT be read separately — that second read IS the \
             round trip this change removes"
        );
    }

    /// The co-read is not just short-circuiting misses: a HIT serves the bytes
    /// the co-read row pointed at, still in one round trip.
    #[tokio::test]
    async fn cargo_get_hit_serves_the_coread_row_in_one_round_trip() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let blob = b"sccache-artifact-bytes".to_vec();
        let cache_key = "b".repeat(64);
        let d1 = Arc::new(
            CountingD1::new(Some(live_row(&hash, &tenant))).with_mapping(
                &tenant,
                &cache_key,
                &crate::adapter_cache::canonical_hash_hex(&blob),
            ),
        );
        let app = coread_router(Arc::clone(&d1), &key, blob.clone());

        let resp = app
            .oneshot(cargo_get(&tenant, &cache_key, &pat))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), blob.as_slice());
        assert_eq!(d1.trips(), 1, "a HIT is one round trip too");
        assert_eq!(d1.map_reads(), 0);
    }

    /// ⛔ IMMEDIATE REVOCATION SURVIVES. #1022 kept the per-request `pat` read
    /// for exactly this, and folding the url-map read into it must not turn it
    /// into a cache: revoke the row in D1 and the VERY NEXT request 401s.
    ///
    /// The round-trip count is asserted too — the second request must have made
    /// its own D1 read (2 total). A change that served the second request from
    /// anything remembered would show 1 here.
    #[tokio::test]
    async fn revoking_the_pat_row_401s_the_very_next_request() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let d1 = Arc::new(CountingD1::new(Some(live_row(&hash, &tenant))));
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let first = app
            .clone()
            .oneshot(cargo_get(&tenant, &"c".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::NOT_FOUND, "live PAT ⇒ served");

        // Revocation in D1: `revoked_at_ms IS NULL` stops matching, so the
        // statement returns no row — indistinguishable from absent, by design.
        *d1.row.lock().unwrap() = None;

        let second = app
            .oneshot(cargo_get(&tenant, &"c".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(
            second.status(),
            StatusCode::UNAUTHORIZED,
            "a PAT revoked in D1 must 401 on the very next request — the \
             co-read must not have cached the row"
        );
        assert_eq!(
            d1.trips(),
            2,
            "each request must make its OWN D1 read (no cross-request reuse)"
        );
    }

    /// ⚠️ AUTH BEFORE ACT. The co-read is keyed by the Worker's tenant HINT; the
    /// moat is keyed by the tenant the container derived from the PAT. When they
    /// disagree, the prefetched row MUST be discarded unread and the storage
    /// lookup re-issued under the PAT-derived tenant — never served across.
    ///
    /// Here the hint names a victim tenant that HAS the key; the PAT resolves to
    /// a different tenant that does NOT. The answer must be the PAT tenant's
    /// (404), and the map must have been read for real under it.
    #[tokio::test]
    async fn a_row_prefetched_under_a_spoofed_tenant_hint_is_never_served() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let cache_key = "d".repeat(64);
        let blob = b"victim-tenant-secret".to_vec();
        let d1 = Arc::new(
            CountingD1::new(Some(live_row(&hash, &tenant))).with_mapping(
                "victim-tenant",
                &cache_key,
                &crate::adapter_cache::canonical_hash_hex(&blob),
            ),
        );
        let app = coread_router(Arc::clone(&d1), &key, blob);

        // The URL + hint header both claim the victim tenant; the PAT does not.
        let req = axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/victim-tenant/{cache_key}"))
            .header(SCOPE_HEADER, "cas:rw")
            .header(TENANT_HINT_HEADER, "victim-tenant")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {pat}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "the victim tenant's cached object must NOT be served to a PAT that \
             resolves to a different tenant"
        );
        assert_eq!(
            d1.map_reads(),
            1,
            "the mismatched prefetch must be discarded and the url-map re-read \
             under the PAT-derived tenant"
        );
    }

    /// D1-failure semantics on the `pat` read are unchanged: a backend fault is
    /// a SHED (503 + `Retry-After`), never a 401 — `VerifierOverloaded`, not
    /// `Auth` (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`).
    #[tokio::test]
    async fn a_d1_fault_on_the_coread_still_503s_never_401s() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let mut d1 = CountingD1::new(Some(live_row(&hash, &tenant)));
        d1.pat_err = Some("D1 HTTP 500: upstream".to_owned());
        let d1 = Arc::new(d1);
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let resp = app
            .oneshot(cargo_get(&tenant, &"e".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "a D1 fault must stay a 503 shed — a valid PAT is never told it is \
             invalid because the verifier could not reach D1"
        );
    }

    /// A backend that does NOT co-read (and the fallback a failed co-read takes)
    /// keeps the exact serial behaviour: two round trips, and a url-map fault
    /// still surfaces as the CAS-backend 502 — NOT the verifier's 503.
    #[tokio::test]
    async fn the_serial_fallback_keeps_both_reads_and_the_502_map_fault() {
        use tower::ServiceExt;
        let key = coread_test_key();
        let (pat, hash, tenant) = mint_cargo_pat(&key);
        let mut d1 = CountingD1::new(Some(live_row(&hash, &tenant))).serial();
        d1.map_err = Some("D1 HTTP 500: adapter_cache_map".to_owned());
        let d1 = Arc::new(d1);
        let app = coread_router(Arc::clone(&d1), &key, b"unused".to_vec());

        let resp = app
            .oneshot(cargo_get(&tenant, &"f".repeat(64), &pat))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_GATEWAY,
            "a url-map backend fault on the serial path is a CAS-dependency 502, \
             exactly as before the co-read existed"
        );
        assert_eq!(
            d1.trips(),
            2,
            "no co-read ⇒ the two reads stay serial (this is the pre-change \
             baseline the fallback preserves)"
        );
        assert_eq!(d1.map_reads(), 1);
    }

    /// What the handler saw: the `(namespace, url_hash)` hint in scope, or
    /// `None` when the layer published none.
    type ObservedHint = Arc<Mutex<Option<Option<(String, String)>>>>;

    /// Drive `req` through the co-read hint layer alone and report the hint the
    /// downstream handler observed.
    async fn observe_hint(req: axum::http::Request<axum::body::Body>) -> Option<(String, String)> {
        use tower::ServiceExt;
        let observed: ObservedHint = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&observed);
        let app = Router::new()
            .fallback(axum::routing::any(move || {
                let sink = Arc::clone(&sink);
                async move {
                    *sink.lock().unwrap() =
                        Some(crate::d1_coread::hint().map(|(_cell, ns, key)| (ns, key)));
                    StatusCode::OK
                }
            }))
            .layer(middleware::from_fn(cargo_coread_hint));
        let _ = app.oneshot(req).await.unwrap();
        let seen = observed.lock().unwrap().clone();
        seen.expect("the handler must have run")
    }

    /// A PUT publishes NO hint: its storage step is a map WRITE, so co-reading a
    /// map row would be a wasted index probe on every sccache upload.
    #[tokio::test]
    async fn a_write_verb_publishes_no_coread_hint() {
        let key = "1".repeat(64);
        let req = axum::http::Request::builder()
            .method(Method::PUT)
            .uri(format!("/cargo/tenant-abc/{key}"))
            .header(TENANT_HINT_HEADER, "tenant-abc")
            .body(axum::body::Body::from("bytes"))
            .unwrap();
        assert_eq!(
            observe_hint(req).await,
            None,
            "PUT must reach the handler with NO co-read hint in scope"
        );
    }

    /// A GET with no server-trusted tenant header (direct container access, or a
    /// Worker regression) publishes no hint and stays on the unchanged serial
    /// path — absence of the hint is never guessed at.
    #[tokio::test]
    async fn a_get_without_the_tenant_header_publishes_no_hint() {
        let key = "2".repeat(64);
        let req = axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/tenant-abc/{key}"))
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(observe_hint(req).await, None);
    }

    /// A GET DOES publish the hint, keyed by the server-trusted tenant header
    /// and the SAME `key_from_path` normalization the moat will ask with (a
    /// divergence here would silently cost a wasted probe on every request).
    #[tokio::test]
    async fn a_get_publishes_the_hint_the_moat_will_ask_with() {
        // UPPERCASE hex on the wire — `key_from_path` lowercases 64-hex object
        // keys, so the hint must carry the lowercased form.
        let req = axum::http::Request::builder()
            .method(Method::GET)
            .uri(format!("/cargo/tenant-abc/{}", "AB".repeat(32)))
            .header(TENANT_HINT_HEADER, "  tenant-abc  ")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(
            observe_hint(req).await,
            Some(("tenant-abc".to_owned(), "ab".repeat(32))),
            "the hint must be the trimmed tenant + the normalized key"
        );
    }
}
