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
        let (tenant_id, can_write) =
            self.0.verify_capability(pat_plaintext).await.map_err(|e| match e {
                VerifyError::InvalidPat => TenantResolveError::InvalidPat,
                VerifyError::Backend(m) => TenantResolveError::Backend(m),
            })?;
        Ok(ResolvedTenant { tenant_id, can_write })
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
    let cas: Arc<dyn CasStore> = Arc::new(CargoMoatStore { moat, cap_resolver });
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
    let gate_state = CargoGateState { quota, resolver };
    let adapter = server::build_router(config)
        .layer(middleware::from_fn_with_state(gate_state, cargo_gate));
    Router::new().nest_service("/cargo", adapter)
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
                        req.extensions_mut().insert(
                            server::GateResolvedTenant(resolved.tenant_id),
                        );
                    }
                    Ok(_no_write) => {
                        tracing::warn!(
                            "cargo: PUT denied — PAT scope lacks write capability (F27)"
                        );
                        return (
                            StatusCode::FORBIDDEN,
                            "PAT does not grant write capability",
                        )
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
            self.0
                .lock()
                .unwrap()
                .insert((ns.to_owned(), url_hash.to_owned()), content_hash.to_owned());
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
        assert_eq!(recorded, None, "no resolver ⇒ None cap (fail-closed posture)");
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
}
