//! `/npm/<tenant>/<rest…>` — npm registry cache surface.
//!
//! Mounts the `corelink_adapter_host::npm` adapter (a read-through
//! `registry.npmjs.org` mirror: `GET /<pkg>` package metadata JSON +
//! `GET /<pkg>/-/<tarball>.tgz` tarball bytes, plus `/-/ping`) into the
//! container router. TARBALL bytes are content-deduped through the shared
//! 2-level [`MoatCache`]; package METADATA (mutable JSON with a TTL) is
//! cached in a small D1-backed KV the moat does not provide
//! ([`crate::adapter_cache`] is content-addressed and immutable, so it
//! cannot hold mutable TTL'd metadata).
//!
//! # Path shape
//!
//! The Worker forwards the FULL path `/npm/<tenant>/<rest>` (the
//! `<tenant>` segment is the tenant namespace, used only for DO routing).
//! `nest_service("/npm", …)` strips the static `/npm` prefix and hands
//! `/<tenant>/<rest>` to [`npm_gate`], which:
//!
//! 1. enforces the per-operation cache scope (GET/HEAD ⇒ read) from the
//!    Worker-set, server-trusted `x-corelink-scope` header; and
//! 2. rewrites `/<tenant>/<rest>` → `/<rest>` so the adapter's
//!    `/:pkg` and `/:pkg/-/:tarball` routes match AND the upstream fetch
//!    targets the real registry path (never `registry.npmjs.org/<tenant>/…`).
//!
//! `nest_service` (not `nest`) is required so the adapter's parameterized
//! routes are preserved rather than flattened under the mount prefix.
//!
//! # Trust + storage model
//!
//! Tenant identity comes from the bearer PAT, re-verified in the container
//! ([`crate::adapter_pat::PatVerifier`], Option B). The path `<tenant>` is
//! NEVER trusted. The two storage surfaces namespace DIFFERENTLY because they
//! see different amounts of the request:
//!
//! - **Package METADATA** (the D1 KV, [`NpmMetaKv`]) embeds the package name
//!   in its key, so it can split PUBLIC vs PRIVATE: UNSCOPED metadata lands in
//!   the shared [`crate::adapter_cache::PUBLIC_NAMESPACE`] (cross-tenant share
//!   — public registry data) and SCOPED `@org/...` metadata lands in the
//!   per-tenant namespace (isolated).
//! - **Tarball BYTES** (the moat, [`NpmMoatStore`]) only see `(tenant,
//!   SHA256(url))` — the package name (hence the public-vs-scoped signal) is
//!   NOT visible at that port. To avoid leaking a private scoped tarball's
//!   bytes cross-tenant, ALL tarball bytes are namespaced PER-TENANT (still
//!   intra-tenant content-deduped by the moat). Cross-tenant PUBLIC tarball
//!   dedup (the network-effect moat) is a tracked enhancement — see the
//!   [`NpmMoatStore`] type doc.
//!
//! In all cases the PAT gates ACCESS; only public metadata content is shared.
//!
//! The mandatory tarball integrity check (downloaded bytes' SHA1 vs the
//! metadata-published `dist.shasum`) runs INSIDE the adapter
//! (`corelink_adapter_host::npm::tarball`) BEFORE the [`CasStore::put`] in
//! the moat — this wiring does not relax it.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::Request;
use axum::http::{Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use url::Url;

use corelink_adapter_host::npm::config::{
    NpmAdapterConfig, DEFAULT_METADATA_TTL_SECONDS, DEFAULT_TARBALL_SIZE_LIMIT_BYTES,
    DEFAULT_UPSTREAM_REGISTRY,
};
use corelink_adapter_host::npm::error::NpmAdapterError;
use corelink_adapter_host::npm::ports::{
    CasStore, KvStore, ResolvedTenant, TenantResolver, TenantResolverHandle,
};
use corelink_adapter_host::npm::server::{build_router, AdapterState};
use corelink_adapter_host::npm::upstream::UpstreamClient;
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore, PUBLIC_NAMESPACE};
use crate::adapter_kv::{NpmKvError, NpmKvStore};
use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::scope::{requires_cache_read, requires_cache_write, SCOPE_HEADER};

/// Service principal stamped on the adapter's CAS operations. Identifies the
/// adapter-host service, NOT the end-user PAT.
const NPM_SERVICE_PRINCIPAL: &str = "npm-adapter-host";

/// KV-key prefix the adapter uses for scoped (`@org/…`) package metadata.
/// `corelink_adapter_host::npm::metadata::kv_key_for_pkg` builds
/// `npm:meta:<normalized-pkg>`; a scoped package normalizes to
/// `npm:meta:@org/name`, so a `@` after the `npm:meta:` prefix is the
/// PUBLIC-vs-PRIVATE discriminator visible at the KV port.
const NPM_META_KEY_PREFIX: &str = "npm:meta:";

/// Classify a metadata KV key as PUBLIC (unscoped) or PRIVATE (scoped
/// `@org/…`). Returns the moat/KV namespace to use: the shared
/// [`PUBLIC_NAMESPACE`] for unscoped packages (cross-tenant dedup — the
/// moat) or the per-tenant id for scoped packages (isolated).
fn namespace_for_meta_key<'a>(key: &str, tenant_ns: &'a str) -> &'a str {
    match key.strip_prefix(NPM_META_KEY_PREFIX) {
        Some(pkg) if pkg.starts_with('@') => tenant_ns,
        _ => PUBLIC_NAMESPACE,
    }
}

/// npm's `CasStore` port → the 2-level [`MoatCache`].
///
/// npm keys tarball blobs by an opaque `Digest` (the adapter's
/// `SHA256(tarball-URL)` — npm tarballs are immutable, so the URL is the
/// canonical identity). That digest becomes the moat's `url_hash`; the
/// bytes are stored blake3-content-addressed INSIDE the moat (so identical
/// tarballs dedup to ONE CAS blob WITHIN a namespace).
///
/// # Namespace = per-tenant (tarball-byte isolation)
///
/// SECURITY: tarball bytes are stored under the PER-TENANT namespace
/// (`tenant.to_string()`), NOT the shared [`PUBLIC_NAMESPACE`]. The tarball
/// URL — and therefore whether the package is scoped (`@org/…`, private) —
/// is NOT visible at this port (the adapter passes only `(tenant, digest)`),
/// so we cannot tell a public tarball from a private scoped one here. Storing
/// every tarball under the shared PUBLIC namespace would LEAK a private
/// scoped tarball's bytes cross-tenant (any tenant that learned the
/// `SHA256(url)` could read another tenant's private tarball). Keying by the
/// PAT-derived tenant closes that hole: bytes remain deduped INTRA-tenant
/// (the moat still content-addresses by blake3 within the namespace) and are
/// fully isolated across tenants.
///
/// Tracked enhancement: cross-tenant dedup of PUBLIC (unscoped) tarball bytes
/// — the network-effect moat for the dominant public-dep case — is forfeited
/// by per-tenant namespacing. Restoring it safely needs the package name (the
/// public-vs-scoped discriminator, exactly as [`NpmMetaKv`] already has for
/// metadata) threaded through the `CasStore` port so unscoped tarballs can
/// route to [`PUBLIC_NAMESPACE`] while scoped ones stay per-tenant. Until the
/// port carries that signal, fail-CLOSED (isolate) is the correct default.
#[derive(Debug)]
struct NpmMoatStore {
    moat: Arc<MoatCache>,
}

#[async_trait]
impl CasStore for NpmMoatStore {
    async fn get(
        &self,
        tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, NpmAdapterError> {
        // SECURITY: per-tenant namespace (not PUBLIC) — see the type doc. The
        // tarball URL/scope is invisible here, so isolate by tenant to avoid
        // leaking private scoped tarball bytes cross-tenant. `digest` is the
        // adapter's SHA256(tarball-URL) ⇒ the moat map key (`url_hash`).
        self.moat
            .get(&tenant.to_string(), &digest.to_hex())
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => NpmAdapterError::Cas(m),
            })
    }

    async fn put(
        &self,
        tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), NpmAdapterError> {
        // SECURITY: per-tenant namespace (not PUBLIC) — see the type doc.
        self.moat
            // `None`: npm tarballs accrue against the tenant's EXISTING
            // `tenant_storage_state` row's stored cap (the OCI surface — WP #10 —
            // is the one that threads a resolved cap; npm keeps the prior posture).
            .put(&tenant.to_string(), &digest.to_hex(), bytes, None)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => NpmAdapterError::Cas(m),
            })
    }
}

/// npm's `KvStore` port → the D1-backed [`NpmKvStore`] for mutable,
/// TTL'd package metadata JSON.
///
/// The PUBLIC/PRIVATE split IS enforceable here: the KV key embeds the
/// normalized package name (`npm:meta:<pkg>`), so unscoped metadata lands
/// in [`PUBLIC_NAMESPACE`] (cross-tenant share — public registry data) and
/// scoped `@org/…` metadata lands in the per-tenant namespace (isolated).
/// The freshness/TTL decision stays pure-logic in the adapter, which gets
/// `inserted_at_unix_ms` back from [`KvStore::get`].
#[derive(Debug)]
struct NpmMetaKv {
    kv: Arc<NpmKvStore>,
}

#[async_trait]
impl KvStore for NpmMetaKv {
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError> {
        let tenant_ns = tenant.to_string();
        let ns = namespace_for_meta_key(key, &tenant_ns);
        self.kv.get(ns, key).await.map_err(|e| match e {
            NpmKvError::Backend(m) => NpmAdapterError::Kv(m),
        })
    }

    async fn put(
        &self,
        tenant: &TenantId,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), NpmAdapterError> {
        let tenant_ns = tenant.to_string();
        let ns = namespace_for_meta_key(key, &tenant_ns);
        self.kv
            .put(ns, key, value, inserted_at_unix_ms)
            .await
            .map_err(|e| match e {
                NpmKvError::Backend(m) => NpmAdapterError::Kv(m),
            })
    }
}

/// Thin shell wrapping the shared [`PatVerifier`] as npm's `TenantResolver`.
///
/// The shared verifier returns the tenant id as a UUID text string (the D1
/// `pat.tenant_id` column); npm's port returns a typed
/// [`corelink_core::types::tenant::TenantId`], so this parses the text into
/// the newtype. A non-UUID tenant id is a backend invariant break (D1
/// always stores canonical UUID text); it keeps its pre-existing `Auth`
/// mapping here (the arm is unreachable without a corrupt D1 row and is NOT a
/// load shed, so it is out of scope for the shed split below — noted rather
/// than silently changed).
///
/// `VerifyError::Backend` — a D1 fault OR an Argon2id permit-pool load shed —
/// maps to `NpmAdapterError::VerifierOverloaded` (503 + `Retry-After`), NOT to
/// `Auth` (401): the verifier never reached a verdict on the credential, so a
/// 401 both lied to a client holding a valid PAT and broke the container's
/// symmetric shed (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`) end-to-end.
#[derive(Debug)]
struct NpmPatResolver(Arc<PatVerifier>);

#[async_trait]
impl TenantResolver for NpmPatResolver {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, NpmAdapterError> {
        let tenant_text = self.0.verify(pat_plaintext).await.map_err(|e| match e {
            VerifyError::InvalidPat => NpmAdapterError::Auth("invalid PAT".to_owned()),
            VerifyError::Backend(m) => NpmAdapterError::VerifierOverloaded(m),
        })?;
        let uuid = uuid::Uuid::parse_str(&tenant_text)
            .map_err(|e| NpmAdapterError::Auth(format!("backend: tenant id not a UUID: {e}")))?;
        Ok(TenantId::from_uuid(uuid))
    }

    /// Override: call `verify_capability` so the write-gate can use the
    /// D1-verified `can_write` bit instead of trusting only the header (F27).
    async fn resolve_with_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<ResolvedTenant, NpmAdapterError> {
        let (tenant_text, can_write) =
            self.0
                .verify_capability(pat_plaintext)
                .await
                .map_err(|e| match e {
                    VerifyError::InvalidPat => NpmAdapterError::Auth("invalid PAT".to_owned()),
                    VerifyError::Backend(m) => NpmAdapterError::VerifierOverloaded(m),
                })?;
        let uuid = uuid::Uuid::parse_str(&tenant_text)
            .map_err(|e| NpmAdapterError::Auth(format!("backend: tenant id not a UUID: {e}")))?;
        Ok(ResolvedTenant {
            tenant_id: TenantId::from_uuid(uuid),
            can_write,
        })
    }
}

/// State for [`npm_gate`]: the shared tenant resolver (F27 two-layer write
/// enforcement) + the optional per-tenant monthly `$`-ceiling [`QuotaGate`]
/// (rt-nuclear #22 — the npm/pip/brew adapters had NO container-side $-ceiling).
#[derive(Clone)]
struct NpmGateState {
    resolver: TenantResolverHandle,
    quota: Option<crate::routes::QuotaGate>,
}

/// Build the `/npm/*` sub-router from shared CAS handlers + the url→hash
/// map + the npm metadata KV + the PAT verifier.
///
/// The `cas_read`/`cas_write` are the SAME trait objects the
/// cas/ac/bazel/turbo/brew surfaces use; `map` is the D1-backed
/// url→content-hash store (tarball dedup); `meta_kv` is the D1-backed
/// metadata KV; `verifier` is shared across cache adapters. On a
/// construction error the route is simply NOT mounted (empty sub-router +
/// logged) so the container still boots.
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    meta_kv: Arc<NpmKvStore>,
    verifier: Arc<PatVerifier>,
    quota: Option<crate::routes::QuotaGate>,
) -> Router {
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        NPM_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn CasStore> = Arc::new(NpmMoatStore { moat });
    let kv: Arc<dyn KvStore> = Arc::new(NpmMetaKv { kv: meta_kv });
    let resolver: TenantResolverHandle = Arc::new(NpmPatResolver(verifier));
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = match Url::parse(DEFAULT_UPSTREAM_REGISTRY) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "/npm upstream URL invalid; npm NOT mounted");
            return Router::new();
        }
    };

    let upstream = match UpstreamClient::new(upstream_url.clone()) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::error!(error = ?e, "/npm upstream client build failed; npm NOT mounted");
            return Router::new();
        }
    };

    let config = NpmAdapterConfig::new(
        // bind_addr is unused by `build_router` (only `run_npm_adapter` binds).
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        upstream_url,
        DEFAULT_METADATA_TTL_SECONDS,
        DEFAULT_TARBALL_SIZE_LIMIT_BYTES,
        cas,
        kv,
        resolver.clone(),
        auditor,
    );

    let adapter = build_router(AdapterState::new(Arc::new(config), upstream));
    // The gate is an OUTER layer (wrapping the WHOLE `/npm` mount), NOT an
    // inner layer on the adapter. The npm adapter's routes are parameterized
    // (`/:pkg`, `/:pkg/-/:tarball`), and axum 0.7's `nest_service` matches the
    // post-strip remainder against those params at routing time — BEFORE an
    // inner layer could rewrite it. So an inner gate that drops the tenant
    // segment runs too late: `/<tenant>/<pkg>` (two segments) never matches the
    // single-segment `/:pkg`, yielding a 404. Wrapping the mount lets the gate
    // rewrite `/npm/<tenant>/<rest>` → `/npm/<rest>` BEFORE `nest_service`
    // routes, so the inner param routes see the real registry path. (brew can
    // use an inner gate only because its inner route is a catch-all `/{*path}`.)
    // The SAME resolver is threaded into the gate for two-layer write enforcement
    // (F27) — one PAT verification, not two.
    let gate_state = NpmGateState { resolver, quota };
    Router::new()
        .nest_service("/npm", adapter)
        .layer(middleware::from_fn_with_state(gate_state, npm_gate))
}

/// Gate layer: per-operation cache-scope enforcement + two-layer write
/// enforcement (F27) + tenant-segment strip.
///
/// Runs as an OUTER layer (BEFORE `nest_service` strips `/npm`), so
/// `req.uri().path()` is the FULL `/npm/<tenant>/<rest>`. (1) enforces the
/// per-op scope from the server-trusted `x-corelink-scope` header (GET/HEAD ⇒
/// read); (2) for PUT/POST (defensive write surface) also requires the PAT's
/// `can_write` bit via the resolver's single `resolve_with_capability`
/// verification (F27 — two-layer write enforcement mirroring OCI, no redundant
/// second PAT verify); (3) rewrites the path to `/npm/<rest>` — dropping the
/// `<tenant>` segment while KEEPING the static `/npm` prefix so `nest_service`
/// then strips `/npm` and hands `/<rest>` to the adapter's `/:pkg` +
/// `/:pkg/-/:tarball` routes. Running outer is required because axum 0.7 matches
/// the nested param routes at routing time, before an inner layer could rewrite
/// the path (see [`router`]).
async fn npm_gate(
    axum::extract::State(state): axum::extract::State<NpmGateState>,
    mut req: Request,
    next: Next,
) -> Response {
    let resolver = &state.resolver;
    let scope = req
        .headers()
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_write = matches!(*req.method(), Method::PUT | Method::POST);
    let scope_ok = match *req.method() {
        // npm is a read-through mirror: clients only GET (metadata, tarball,
        // ping). The transparent cache-fill (a CAS/KV write by the service)
        // is not a client write.
        Method::GET | Method::HEAD => requires_cache_read(scope),
        // npm exposes no client write surface (no `npm publish`); gate
        // defensively anyway.
        Method::PUT | Method::POST => requires_cache_write(scope),
        // Fail-CLOSED: the adapter only routes GET (axum's `get` also serves
        // HEAD), so anything else would 405 downstream today — but the gate
        // must not assume that. An unmapped method is denied here so a future
        // adapter route can never ship without an explicit scope decision. No
        // browser/CORS clients exist on this surface (npm CLI only), so
        // OPTIONS is not legitimate traffic.
        _ => false,
    };
    if !scope_ok {
        return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
    }

    // F27 — two-layer write enforcement: for PUT/POST requests, require the PAT's
    // own `can_write` bit from the resolver's SINGLE PAT verification
    // (`resolve_with_capability` — HMAC + Argon2id against D1). This ensures a
    // Worker-side scope-header mistake cannot grant a write that the PAT's D1
    // record does not authorise — without a redundant second verify. The npm
    // resolver collapses every verification failure into `NpmAdapterError::Auth`,
    // so any resolver `Err` is treated as a fail-CLOSED write denial (401).
    //
    // REV-S3 (mirror cargo_gate): for a write, the PAT-derived tenant id from
    // the F27 verify is the AUTHORITATIVE cost-attribution key (it cannot be
    // spoofed by a Worker-set header). Capture it here and prefer it over
    // `x-corelink-tenant-id` for the $-ceiling gate below.
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
                tracing::warn!("npm: write with no bearer token — rejecting (F27)");
                return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
            }
            Some(pat_plaintext) => {
                match resolver.resolve_with_capability(&pat_plaintext).await {
                    Ok(resolved) if resolved.can_write => {
                        // PAT grants write — both layers pass; continue.
                        // REV-S3: capture the crypto-verified tenant id from THIS
                        // single PAT verification as the authoritative quota key.
                        resolved_tenant_for_quota = Some(resolved.tenant_id.to_string());
                    }
                    Ok(_no_write) => {
                        tracing::warn!(
                            "npm: write denied — PAT scope lacks write capability (F27)"
                        );
                        return (StatusCode::FORBIDDEN, "PAT does not grant write capability")
                            .into_response();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "npm: write denied — PAT re-verify failed (F27)");
                        return (StatusCode::UNAUTHORIZED, "invalid PAT").into_response();
                    }
                }
            }
        }
    }

    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1; rt-nuclear
    // #22): charge the flat per-op cost AFTER the scope + F27 checks, BEFORE the
    // adapter runs. 402 over-ceiling / 503 fail-CLOSED. Mirrors `cargo_gate`.
    //
    // REV-S3 (quota fail-OPEN closed): the cost-attribution tenant is sourced,
    // in priority order:
    //   1. the PAT-resolved tenant id from the F27 verify above (writes only) —
    //      AUTHORITATIVE, cannot be spoofed by a Worker-set header; then
    //   2. the Worker-set, server-trusted `x-corelink-tenant-id` header (the
    //      only source for reads, where no PAT verify runs in this gate). A
    //      forged header can over-charge only ITS OWN tenant.
    // If the gate is configured but NO tenant id is available, we now fail
    // CLOSED (503) instead of silently skipping the charge: a missing label is a
    // Worker header-injection regression (or direct container access) and must
    // surface immediately rather than let a tenant exceed its $-ceiling unmetered.
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
                "npm: quota gate active but no tenant id for cost attribution \
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

    // Drop the `<tenant>` segment but KEEP the `/npm` mount prefix:
    // `/npm/<tenant>/<rest>` → `/npm/<rest>` (so `nest_service` strips `/npm`
    // and the adapter sees `/<rest>`). A path that is not under `/npm/` is left
    // untouched (it cannot reach this mount anyway).
    if let Some(rest_with_tenant) = req.uri().path().strip_prefix("/npm/") {
        let rewritten = match rest_with_tenant.split_once('/') {
            Some((_tenant, rest)) => format!("/npm/{rest}"),
            // Just `/npm/<tenant>` with no rest → `/npm/` (adapter has no `/`
            // route ⇒ 404, the correct "tenant but no package" outcome).
            None => "/npm/".to_owned(),
        };
        let new_path_and_query = match req.uri().query() {
            Some(q) => format!("{rewritten}?{q}"),
            None => rewritten,
        };
        match Uri::builder().path_and_query(new_path_and_query).build() {
            Ok(u) => *req.uri_mut() = u,
            Err(e) => {
                tracing::warn!(error = %e, "npm_gate path rewrite failed");
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
    use corelink_adapter_host::npm::metadata::kv_key_for_pkg;
    use corelink_adapter_host::npm::tarball::tarball_url_digest;
    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId as PatTenantId,
        SCOPE_CACHE_RW,
    };
    use tower::ServiceExt; // for `.oneshot`
    use uuid::Uuid;

    use crate::adapter_cache::canonical_hash_hex;
    use crate::adapter_kv::NpmKvBackend;
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

    /// In-memory url→content-hash map (the tarball dedup level-2).
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

    // `(ns, key) → (value, inserted_ms)` rows for the in-memory KV fake.
    type FakeKvRows = HashMap<(String, String), (Vec<u8>, u64)>;

    /// In-memory npm metadata KV backend: `(ns,key) → (value, inserted_ms)`.
    #[derive(Default, Debug)]
    struct FakeKv(Mutex<FakeKvRows>);
    #[async_trait]
    impl NpmKvBackend for FakeKv {
        async fn get(&self, ns: &str, key: &str) -> Result<Option<(Vec<u8>, u64)>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), key.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            ns: &str,
            key: &str,
            value: Vec<u8>,
            inserted_at_unix_ms: u64,
        ) -> Result<(), String> {
            self.0.lock().unwrap().insert(
                (ns.to_owned(), key.to_owned()),
                (value, inserted_at_unix_ms),
            );
            Ok(())
        }
    }

    /// Non-verifying CAS stub (accepts any claimed_hash) — `npm::router` wires
    /// the production `canonical_hash_hex`, which a verifying in-memory handler
    /// would reject. Keyed by `(namespace, hash)`.
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

    fn npm_kv(backend: Arc<dyn NpmKvBackend>) -> Arc<NpmKvStore> {
        Arc::new(NpmKvStore::new(backend))
    }

    /// Router whose verifier rejects ALL PATs (empty lookup); cas/map/kv unused.
    fn router_rejecting() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            npm_kv(Arc::new(FakeKv::default())),
            verifier,
            None,
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
            .oneshot(get("/npm/t/lodash", Some("corelink_whatever"), None))
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
                .uri("/npm/t/lodash")
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
        // scope present → gate passes → adapter authenticate() fails → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/npm/t/lodash", None, Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_prefix_pat_is_401() {
        // After the prefix-fix, npm's extract_pat enforces the `corelink_`
        // prefix, so a `ghp_`-style token is rejected at the adapter → 401.
        let app = router_rejecting();
        let resp = app
            .oneshot(get("/npm/t/lodash", Some("ghp_github"), Some(SCOPE_RW)))
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
                "/npm/t/lodash",
                Some("corelink_not-a-real-token"),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Router whose verifier rejects ALL PATs but whose per-tenant `$`-ceiling
    /// quota gate is ACTIVE (hermetic in-memory store + fake clock). Used to
    /// prove the gate's cost-attribution does NOT fall open when no tenant id
    /// is available (REV-S3).
    fn router_with_quota() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        let store = Arc::new(crate::tenant_quota::InMemoryQuotaStore::new());
        let clock = Arc::new(crate::wall_clock::InMemoryFakeWallClock::at_unix_ms(
            1_700_000_000_000,
        ));
        let guard = Arc::new(crate::tenant_quota::QuotaGuard::new(store, clock));
        // $1/op flat cost — a fresh tenant (under the $5 tripwire) would be
        // ADMITTED, so a 503 here is unambiguously the no-tenant fail-CLOSED
        // path, not an over-ceiling 402.
        let gate = crate::routes::QuotaGate::new_for_test(guard, 1_000_000);
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            npm_kv(Arc::new(FakeKv::default())),
            verifier,
            Some(gate),
        )
    }

    #[tokio::test]
    async fn quota_gate_without_tenant_header_fails_closed_not_skipped() {
        // REV-S3 regression: a billable op (scope-valid GET) that reaches an
        // ACTIVE quota gate with NO `x-corelink-tenant-id` and no PAT-resolved
        // tenant must FAIL CLOSED (503) — the prior code silently skipped the
        // charge (fail-OPEN), an unmetered $-ceiling bypass.
        let app = router_with_quota();
        let resp = app
            .oneshot(get(
                "/npm/t/lodash",
                Some("corelink_whatever"),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "no-tenant billable op must fail closed (503), not skip the charge"
        );
    }

    #[tokio::test]
    async fn metadata_cache_hit_round_trip_with_tenant_stripped() {
        // Mint a real PAT; seed the metadata KV (PUBLIC namespace, unscoped
        // package) so a GET /<pkg> is a cache HIT (no upstream). Proves
        // end-to-end: nest_service mount + tenant-strip + scope + Option-B
        // resolve + KV get + JSON passthrough.
        let key = test_key();
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(Uuid::from_u128(0xBEEF)),
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
                find_only: false,
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

        // Seed the metadata KV under PUBLIC (unscoped `lodash`). The bytes must
        // be VALID JSON: the cache-hit path re-validates the cached payload
        // (`serve_metadata` → `validate_metadata_json`) and fail-CLOSEs (502) on
        // malformed JSON.
        let meta_json = serde_json::to_vec(&serde_json::json!({
            "name": "lodash",
            "versions": {},
        }))
        .unwrap();
        let kv_backend = Arc::new(FakeKv::default());
        kv_backend.0.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), kv_key_for_pkg("lodash")),
            (meta_json.clone(), now_ms()),
        );

        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            npm_kv(kv_backend),
            verifier,
            None,
        );

        // path-tenant \"ignored\" ≠ the PAT tenant → proves the path tenant is
        // stripped + untrusted (unscoped metadata uses the shared PUBLIC ns).
        let resp = app
            .oneshot(get("/npm/ignored/lodash", Some(&pt), Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), meta_json.as_slice());
    }

    #[tokio::test]
    async fn tarball_cache_hit_round_trip_through_moat() {
        // Seed BOTH the metadata KV (needed for dist.shasum lookup) and the
        // 2-level moat (tarball bytes) so a GET /<pkg>/-/<file> is a full
        // cache HIT served from the moat with NO upstream. Proves the
        // CasStore→MoatCache wiring end-to-end.
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xCAFE);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0xD00D)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        // The tarball-byte namespace is the PAT's tenant (the security-fix
        // isolation), NOT PUBLIC. `NpmMoatStore` derives it from the typed
        // `TenantId`, which is the parsed UUID text of `pat.tenant_id`.
        let tenant_ns = TenantId::from_uuid(tenant_uuid).to_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
                find_only: false,
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

        let tarball_bytes = b"\x1f\x8bfake-tarball".to_vec();
        // The adapter verifies SHA1(bytes) == dist.shasum before serving even
        // on a CAS hit? No — on a CAS hit it serves directly. But it STILL
        // re-parses metadata to derive the version + dist.shasum, so the KV
        // must contain a matching version entry. Compute the real sha1.
        let shasum = corelink_adapter_host::npm::tarball::sha1_hex(&tarball_bytes);
        let meta_json = serde_json::to_vec(&serde_json::json!({
            "name": "lodash",
            "versions": {
                "4.17.21": { "dist": { "shasum": shasum } }
            }
        }))
        .unwrap();

        let kv_backend = Arc::new(FakeKv::default());
        kv_backend.0.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), kv_key_for_pkg("lodash")),
            (meta_json, now_ms()),
        );

        // The tarball CAS key is SHA256(tarball-URL); the adapter builds the
        // URL as `<registry>/<pkg>/-/<file>`.
        let tarball_url = format!("{}/lodash/-/lodash-4.17.21.tgz", DEFAULT_UPSTREAM_REGISTRY);
        let digest = tarball_url_digest(&tarball_url).unwrap();
        let url_hash = digest.to_hex();
        let content_hash = canonical_hash_hex(&tarball_bytes);

        // Seed both moat levels under the PER-TENANT namespace — tarball bytes
        // are tenant-isolated by the security fix (NOT PUBLIC). This also
        // proves the moat get is keyed by the PAT tenant, not the path tenant.
        let cas = Arc::new(StubCas::default());
        cas.0.lock().unwrap().insert(
            (tenant_ns.clone(), content_hash.clone()),
            tarball_bytes.clone(),
        );
        let map = Arc::new(FakeMap::default());
        map.0
            .lock()
            .unwrap()
            .insert((tenant_ns.clone(), url_hash), content_hash);

        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            npm_kv(kv_backend),
            verifier,
            None,
        );

        let resp = app
            .oneshot(get(
                "/npm/ignored/lodash/-/lodash-4.17.21.tgz",
                Some(&pt),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), tarball_bytes.as_slice());
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
