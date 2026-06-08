//! `/v2/*` + `/token` — OCI Distribution Spec v1.1 registry surface.
//!
//! Mounts the `corelink_adapter_host::oci` adapter (a full OCI
//! Distribution Spec v1.1 registry: `docker` / `podman` / `buildah` /
//! `containerd` / `crane` / Kubernetes image-pull / BuildKit cache /
//! Helm OCI all speak to it) into the container router. Blob bytes go
//! through the shared 2-level content-dedup [`MoatCache`]; manifests +
//! tag lists go through an in-process KV shell (see the KV seam below).
//!
//! # Path shape — why `.merge`, NOT `nest_service`
//!
//! The Worker forwards the request path UNCHANGED (`pathSuffix: path`,
//! `worker/src/index.ts` `matchRoute` `oci_v2` arm — it slices `/v2`
//! only to derive a routing key, it does NOT rewrite the forwarded
//! path). The adapter's router already registers the exact public OCI
//! paths (`/v2/`, `/v2`, `/v2/_catalog`, `/v2/*rest`, `/token`), so we
//! mount it with [`Router::merge`] and add only a scope-gate layer.
//!
//! This is the load-bearing deviation from `routes/brew.rs`:
//!
//! * brew uses `nest_service("/brew", …)` + strips a leading
//!   `<tenant>` segment, because brew's wire path is
//!   `/brew/<tenant>/<bottle-path>` and the bottle path must be
//!   tenant-free before the upstream fetch.
//! * OCI has NO tenant path segment. The first segment after `/v2/`
//!   is the OCI *repository name* (`/v2/alpine/blobs/…`). Stripping it
//!   would corrupt the repo. The tenant is carried inside the
//!   adapter's HMAC bearer token (minted at `/token`), never in the
//!   path. So the OCI gate does pure per-op scope enforcement and NO
//!   path surgery.
//!
//! # Trust + storage model
//!
//! OCI uses a two-leg auth flow (OCI Distribution Spec v1.1 §auth):
//!
//! 1. The client `GET /token` with `Authorization: Basic
//!    base64(user:<pat>)`. The adapter resolves the PAT via its
//!    [`TenantResolver`] port — here [`OciPatResolver`], a thin shell
//!    over the shared [`crate::adapter_pat::PatVerifier`] (Option B:
//!    full re-verify incl. Argon2id, mapping [`VerifyError`] →
//!    [`crate::oci::ports`]'s `String` `PortResult` error). On success
//!    the adapter mints an HMAC bearer token carrying `(tenant, scope,
//!    expiry)` signed with its OWN `token_signing_key`.
//! 2. The client retries `/v2/*` ops with `Authorization: Bearer
//!    <hmac-token>`. The adapter verifies the HMAC locally and reads
//!    the tenant out of the token — the PAT is NOT re-presented and
//!    the shared verifier is NOT hit on the data plane.
//!
//! Consequently the Option-B PAT re-verify runs once per token
//! exchange, exactly at the [`OciPatResolver`] seam. The per-op
//! `x-corelink-scope` gate ([`oci_gate`]) is defence-in-depth on top of
//! the adapter's own bearer-scope checks.
//!
//! Blob bytes are stored via the shared [`MoatCache`] keyed by the OCI
//! digest string (`sha256:<hex>`) as the moat `url_hash`; the bytes are
//! content-addressed by blake3 INSIDE the moat (the OCI sha256 digest
//! is NOT the CoreLink content hash — the moat map provides exactly the
//! `OCI-digest → blake3-content-hash` indirection). Images are stored
//! under the per-tenant namespace (isolated) — see the public-dedup
//! seam in the module-level OPEN DECISIONS.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use bytes::Bytes;
use uuid::Uuid;

use corelink_adapter_host::oci::config::defaults;
use corelink_adapter_host::oci::ports::{
    BlobStore, ManifestKvStore, PortResult, ResolvedPat, TenantResolver,
};
use corelink_adapter_host::oci::{router as oci_router, AppState, OciAdapterConfig};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_core::{SecretWrap, TenantId};
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore};
use crate::adapter_pat::{PatVerifier, VerifyError};

/// Service principal stamped on the adapter's CAS operations. Identifies
/// the adapter-host service, NOT the end-user PAT.
const OCI_SERVICE_PRINCIPAL: &str = "oci-adapter-host";

/// Bearer-realm URL the adapter advertises in `Www-Authenticate` on a
/// `/v2/` 401, pointing OCI clients at the `/token` exchange. Flat prod
/// hostname per the deployment note (`corelink-oci.humangr.com`); the
/// adapter only ever emits this string, it does not fetch it.
const OCI_BEARER_REALM: &str = "https://corelink-oci.humangr.com/token";

/// Env var holding the raw (≥32-byte) HMAC key the adapter uses to sign
/// its realm bearer tokens. Distinct from `PAT_SIGNING_KEY` (which keys
/// the PAT HMAC) — this keys the OCI *session* token only. Read by the
/// container mount ([`crate::routes::build_with_factory`]).
pub const OCI_TOKEN_KEY_ENV: &str = "CORELINK_OCI_TOKEN_KEY";

// ── BlobStore port → the shared MoatCache ───────────────────────────────────

/// OCI `BlobStore` port → the 2-level [`MoatCache`].
///
/// `blob_key` is the OCI digest wire string (`sha256:<hex>`); we use it
/// directly as the moat `url_hash`, and the moat content-addresses the
/// bytes by blake3 internally (the OCI sha256 digest is NOT a CoreLink
/// content hash). The adapter verifies the declared digest against the
/// uploaded bytes itself (`oci::push::upload::put` →
/// `OciDigest::verify_against_bytes`) BEFORE this store is asked to
/// persist, so this shell never re-hashes for verification.
///
/// Images are stored under the PER-TENANT namespace (the `tenant` arg's
/// canonical text) — isolated by default. Cross-tenant public-image
/// dedup (storing public base images under
/// [`crate::adapter_cache::PUBLIC_NAMESPACE`]) is an explicit follow-up;
/// see the module OPEN DECISIONS.
///
/// Upload sessions are buffered in-process in `uploads` keyed by the
/// server-allocated UUID; `finalize_upload` flushes the assembled bytes
/// to the moat AND returns them so the adapter can run its digest
/// verification. The buffer is NOT durable (process restart drops
/// in-flight uploads) — acceptable for the single-container deployment;
/// see OPEN DECISIONS.
struct OciMoatStore {
    moat: Arc<MoatCache>,
    /// `uuid → accumulated chunk bytes` for in-flight upload sessions.
    uploads: Mutex<HashMap<String, Vec<u8>>>,
}

impl std::fmt::Debug for OciMoatStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciMoatStore").finish_non_exhaustive()
    }
}

impl OciMoatStore {
    fn new(moat: Arc<MoatCache>) -> Self {
        Self {
            moat,
            uploads: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl BlobStore for OciMoatStore {
    async fn open_upload(&self, tenant: &TenantId) -> PortResult<String> {
        // Server-allocated, collision-resistant session id.
        let uuid = format!("{}:{}", tenant.to_canonical_text(), Uuid::new_v4().simple());
        self.uploads
            .lock()
            .map_err(|e| format!("oci upload buf poisoned: {e}"))?
            .insert(uuid.clone(), Vec::new());
        Ok(uuid)
    }

    async fn append_chunk(
        &self,
        _tenant: &TenantId,
        upload_uuid: &str,
        chunk: Bytes,
    ) -> PortResult<u64> {
        let mut g = self
            .uploads
            .lock()
            .map_err(|e| format!("oci upload buf poisoned: {e}"))?;
        let buf = g
            .get_mut(upload_uuid)
            // EXACT shape the adapter matches on to emit a 404
            // BLOB_UPLOAD_UNKNOWN (`oci::push::upload` checks
            // `e.starts_with("upload session not found")`).
            .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?;
        buf.extend_from_slice(&chunk);
        u64::try_from(buf.len()).map_err(|e| format!("oci upload len overflow: {e}"))
    }

    async fn finalize_upload(
        &self,
        tenant: &TenantId,
        upload_uuid: &str,
        blob_key: &str,
    ) -> PortResult<Bytes> {
        let buf = {
            let mut g = self
                .uploads
                .lock()
                .map_err(|e| format!("oci upload buf poisoned: {e}"))?;
            g.remove(upload_uuid)
                .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?
        };
        let assembled = Bytes::from(buf.clone());
        // Persist content-addressed under the per-tenant namespace,
        // mapping the OCI digest (`blob_key`) → blake3 content hash.
        self.moat
            .put(&tenant.to_canonical_text(), blob_key, buf)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => m,
            })?;
        Ok(assembled)
    }

    async fn cancel_upload(&self, _tenant: &TenantId, upload_uuid: &str) -> PortResult<()> {
        self.uploads
            .lock()
            .map_err(|e| format!("oci upload buf poisoned: {e}"))?
            .remove(upload_uuid);
        Ok(())
    }

    async fn get_blob(&self, tenant: &TenantId, blob_key: &str) -> PortResult<Option<Bytes>> {
        self.moat
            .get(&tenant.to_canonical_text(), blob_key)
            .await
            .map(|opt| opt.map(Bytes::from))
            .map_err(|e| match e {
                MoatError::Backend(m) => m,
            })
    }

    async fn blob_exists(&self, tenant: &TenantId, blob_key: &str) -> PortResult<bool> {
        Ok(self.get_blob(tenant, blob_key).await?.is_some())
    }
}

// ── ManifestKvStore port → durable D1 store ─────────────────────────────────
//
// OCI manifests + tag lists are MUTABLE (a tag re-points to a new manifest on
// every push) and need prefix listing for `/tags/list`, so they cannot live in
// the content-addressed moat. The production binding is the durable D1-backed
// [`crate::adapter_oci_kv::OciKvStore`] (table `adapter_oci_kv`, migration
// 0061), injected into [`router`] below; tests inject an in-memory fake (see
// the test module).

// ── TenantResolver port → the shared PatVerifier (Option B) ─────────────────

/// Thin shell wrapping the shared [`PatVerifier`] as OCI's
/// `TenantResolver`. Hit ONLY on the `/token` leg (Basic → Bearer
/// exchange); the data plane verifies the minted HMAC token instead.
///
/// The port hands us the decoded PAT inside a [`SecretWrap`]; we expose
/// the plaintext for the shared verifier, then parse the returned tenant
/// UUID text into a [`TenantId`]. Every failure collapses to the port's
/// `String` error (the adapter maps it to `401`); a malformed tenant
/// UUID from D1 is a backend fault, also surfaced as the port error.
struct OciPatResolver(Arc<PatVerifier>);

impl std::fmt::Debug for OciPatResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciPatResolver").finish_non_exhaustive()
    }
}

#[async_trait]
impl TenantResolver for OciPatResolver {
    async fn resolve_pat(&self, pat: &SecretWrap) -> PortResult<TenantId> {
        Ok(self.resolve_pat_capability(pat).await?.tenant)
    }

    async fn resolve_pat_capability(&self, pat: &SecretWrap) -> PortResult<ResolvedPat> {
        // Option B: the container re-verifies the PAT against D1 (HMAC →
        // lookup → Argon2id → scope) and surfaces the write-capability bit
        // so the `/token` exchange downscopes the registry grant — a
        // read-only PAT cannot mint a `push` bearer.
        let (tenant_text, can_write) =
            self.0.verify_capability(pat.expose()).await.map_err(|e| match e {
                VerifyError::InvalidPat => "invalid PAT".to_owned(),
                VerifyError::Backend(m) => format!("backend: {m}"),
            })?;
        let uuid = Uuid::parse_str(&tenant_text)
            .map_err(|e| format!("backend: malformed tenant uuid: {e}"))?;
        Ok(ResolvedPat {
            tenant: TenantId::from_uuid(uuid),
            can_write,
        })
    }
}

// ── Router builder ──────────────────────────────────────────────────────────

/// Build the OCI `/v2/*` + `/token` sub-router from the shared CAS
/// handlers + the url→hash map + the durable manifest KV + the PAT
/// verifier + the OCI session HMAC key.
///
/// `cas_read`/`cas_write` are the SAME trait objects cas/ac/bazel/turbo/
/// brew use; `map` is the D1-backed url→content-hash store; `manifest_kv`
/// is the durable mutable manifest/tag store
/// ([`crate::adapter_oci_kv::OciKvStore`] in prod); `verifier` is the
/// shared Option-B PAT verifier; `token_signing_key` is the raw
/// (≥32-byte) HMAC key for the adapter's realm bearer tokens (from
/// [`OCI_TOKEN_KEY_ENV`]). On any construction error the route is simply
/// NOT mounted (empty sub-router + logged) so the container still boots.
///
/// No `x-corelink-scope` gate is layered here: the Worker forwards OCI
/// RAW (pass-through — it cannot resolve a PAT scope for the two-leg flow),
/// so per-op authorization is the adapter's OWN bearer-scope enforcement
/// (`scope.allows(repo, action)` on every `/v2` op) plus the `/token`
/// downscope to the PAT's capability. A header gate would 403 every
/// request under pass-through.
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    manifest_kv: Arc<dyn ManifestKvStore>,
    verifier: Arc<PatVerifier>,
    token_signing_key: SecretWrap,
) -> Router {
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        OCI_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn BlobStore> = Arc::new(OciMoatStore::new(moat));
    let resolver: Arc<dyn TenantResolver> = Arc::new(OciPatResolver(verifier));
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());

    let config = OciAdapterConfig::new(
        // bind_addr is unused by `router`/`build_router` (only
        // `run_oci_adapter` binds).
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        OCI_BEARER_REALM.to_owned(),
        defaults::BLOB_SIZE_LIMIT_BYTES,
        defaults::MULTIPART_CHUNK_SIZE_BYTES,
        // _catalog stays OFF (cross-tenant repo-name leak); the adapter
        // also structurally rejects `true` in `sanity_check`.
        defaults::ENABLE_CATALOG,
        defaults::TOKEN_TTL_SECS,
        token_signing_key,
        cas,
        manifest_kv,
        resolver,
        auditor,
    );

    // Fail-CLOSED on a bad config (e.g. token key <32 bytes): do NOT
    // mount rather than serve an adapter with a weak session-HMAC key.
    if let Err(e) = config.sanity_check() {
        tracing::error!(error = %e, "/v2 OCI config sanity_check failed; OCI NOT mounted");
        return Router::new();
    }

    let state = AppState::new(
        Arc::new(config),
        corelink_adapter_host::oci::wallclock_unix_ms,
    );
    // `.merge` (NOT `nest_service`): the adapter owns `/v2/*` + `/token`
    // verbatim and the Worker forwards the path unchanged.
    Router::new().merge(oci_router(state))
}

// No `oci_gate`: per-op authorization is the adapter's bearer-scope
// enforcement (`scope.allows(repo, action)` on every `/v2` op) plus the
// `/token` downscope to the PAT's capability (see `router` doc). The Worker
// forwards OCI raw, so there is no server-set `x-corelink-scope` to gate on —
// a header gate would 403 every request under pass-through.

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
    use base64::Engine as _;
    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId as PatTenantId, SCOPE_CACHE_R,
        SCOPE_CACHE_RW,
    };
    use tower::ServiceExt; // for `.oneshot`
    use uuid::Uuid;

    use crate::adapter_cache::canonical_hash_hex;
    use crate::adapter_pat::{PatRow, PatRowLookup};
    use crate::scope::SCOPE_HEADER;

    use super::*;

    const SCOPE_RW: &str = "cas:rw";
    /// 32-byte raw HMAC key for the adapter's session tokens (config
    /// `sanity_check` requires ≥32 bytes).
    const OCI_KEY: &str = "oci-session-key-0123456789abcdef"; // 32 bytes

    /// `PatRowLookup` that knows ONE token_id → row; everything else
    /// unknown.
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

    /// In-memory url→content-hash map (mirrors brew's `FakeMap`).
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
            self.0
                .lock()
                .unwrap()
                .insert((ns.to_owned(), url_hash.to_owned()), content_hash.to_owned());
            Ok(())
        }
    }

    /// In-memory `ManifestKvStore` fake for the route tests (prod wires the
    /// durable `crate::adapter_oci_kv::OciKvStore`).
    #[derive(Default, Debug)]
    struct OciKvFake(Mutex<HashMap<(String, String), Bytes>>);
    #[async_trait]
    impl ManifestKvStore for OciKvFake {
        async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(tenant.to_canonical_text(), key.to_owned()))
                .cloned())
        }
        async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()> {
            self.0
                .lock()
                .unwrap()
                .insert((tenant.to_canonical_text(), key.to_owned()), value);
            Ok(())
        }
        async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>> {
            let t = tenant.to_canonical_text();
            Ok(self
                .0
                .lock()
                .unwrap()
                .keys()
                .filter(|(kt, ks)| *kt == t && ks.starts_with(prefix))
                .map(|(_, ks)| ks.clone())
                .collect())
        }
    }

    /// Non-verifying CAS stub (accepts any claimed_hash) — `oci::router`
    /// wires production `canonical_hash_hex`, which a verifying in-memory
    /// handler would reject. Keyed by `(tenant, hash)`.
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

    /// Router whose verifier rejects ALL PATs (empty lookup); cas/map
    /// unused. Session HMAC key is valid so the route MOUNTS.
    fn router_rejecting() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
        )
    }

    fn req(
        method: Method,
        uri: &str,
        authorization: Option<&str>,
        scope: Option<&str>,
    ) -> HttpRequest<Body> {
        let mut b = HttpRequest::builder().method(method).uri(uri);
        if let Some(a) = authorization {
            b = b.header("authorization", a);
        }
        if let Some(s) = scope {
            b = b.header(SCOPE_HEADER, s);
        }
        b.body(Body::empty()).unwrap()
    }

    /// `Authorization: Basic base64("oci:<pat>")` — the OCI Basic leg.
    fn basic(pat: &str) -> String {
        let enc = base64::engine::general_purpose::STANDARD.encode(format!("oci:{pat}"));
        format!("Basic {enc}")
    }

    #[tokio::test]
    async fn read_only_pat_token_is_downscoped_to_pull_only() {
        // SECURITY (scope-escalation fix): a read-only (`cas:r`) PAT
        // exchanging at `/token` for `push,pull` is granted PULL-ONLY. The
        // minted bearer still PULLS (GET an absent blob → 404, i.e. the pull
        // scope passed) but CANNOT PUSH (manifest PUT → denied).
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xCAFE);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0x1234)),
            PatScopes::from_u64(SCOPE_CACHE_R), // READ-ONLY
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
                scope: "cas:r".to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));
        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
        );

        // Exchange the read-only PAT (requesting push,pull) for a bearer.
        let resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:push,pull",
                Some(&basic(&pt)),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = json["token"].as_str().expect("token field").to_owned();

        // PULL is granted: GET an absent blob → 404 (pull scope passed; a
        // denied scope would be 401/403).
        let get = req(
            Method::GET,
            "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
            Some(&format!("Bearer {bearer}")),
            None,
        );
        let resp = app.clone().oneshot(get).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "pull-only bearer must still pull (absent blob ⇒ 404)"
        );

        // PUSH is denied: a manifest PUT with the pull-only bearer is rejected.
        let put = HttpRequest::builder()
            .method(Method::PUT)
            .uri("/v2/alpine/manifests/latest")
            .header("authorization", format!("Bearer {bearer}"))
            .header("content-type", "application/vnd.oci.image.manifest.v1+json")
            .body(Body::from(r#"{"schemaVersion":2}"#))
            .unwrap();
        let resp = app.oneshot(put).await.unwrap();
        assert!(
            !resp.status().is_success(),
            "read-only PAT must not push (pull-only bearer); got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn missing_bearer_on_data_plane_is_401_reaches_adapter() {
        // scope present → gate passes → adapter `/v2/*` dispatch finds no
        // bearer → 401 + Www-Authenticate. Proves `.merge` routed to the
        // adapter.
        let app = router_rejecting();
        let resp = app
            .oneshot(req(
                Method::GET,
                "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
                None,
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().get("Www-Authenticate").is_some());
    }

    #[tokio::test]
    async fn forged_bearer_on_data_plane_is_401() {
        // A `corelink-oci.`-shaped but HMAC-invalid bearer → adapter
        // verify fails → 401. (The data plane never touches the PAT
        // resolver — it verifies the session HMAC.)
        let forged = "corelink-oci.00000000-0000-0000-0000-000000000000.cmVwb3NpdG9yeTphbHBpbmU6cHVsbA.9999999999.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let app = router_rejecting();
        let resp = app
            .oneshot(req(
                Method::GET,
                "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
                Some(&format!("Bearer {forged}")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_pat_at_token_is_401_resolver_runs() {
        // `/token` with a `corelink_`-prefixed but HMAC-invalid PAT in
        // Basic auth → gate passes (GET ⇒ read) → adapter `/token` →
        // OciPatResolver → shared verifier → InvalidPat → 401. Proves
        // `.merge` routed `/token` to the adapter AND the Option-B
        // resolver ran.
        let app = router_rejecting();
        let resp = app
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:pull",
                Some(&basic("corelink_not-a-real-token")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn token_exchange_then_blob_pull_round_trip() {
        // End-to-end: mint a real PAT; exchange it at `/token` for an
        // HMAC bearer; seed the moat with a blob under the PAT's tenant
        // namespace; pull it back → 200 + bytes. Proves: `.merge` mount,
        // Option-B resolve at `/token`, HMAC mint/verify, scope gate,
        // and the moat blob-key (OCI digest → blake3) round-trip.
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

        // Seed the moat: a blob whose OCI digest is the moat url_hash,
        // bytes stored content-addressed by blake3, under the PAT
        // tenant's namespace (canonical UUID text).
        let bytes = b"oci-layer-bytes".to_vec();
        let oci_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        let content_hash = canonical_hash_hex(&bytes);
        let tenant_ns = tenant_uuid.to_string();
        let cas = Arc::new(StubCas::default());
        cas.0
            .lock()
            .unwrap()
            .insert((tenant_ns.clone(), content_hash.clone()), bytes.clone());
        let map = Arc::new(FakeMap::default());
        map.0
            .lock()
            .unwrap()
            .insert((tenant_ns, oci_digest.to_owned()), content_hash);

        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
        );

        // Leg 1: exchange the PAT (Basic) for an HMAC bearer at /token.
        let resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:pull",
                Some(&basic(&pt)),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = json["token"].as_str().expect("token field").to_owned();

        // Leg 2: pull the seeded blob with the minted bearer.
        let resp = app
            .oneshot(req(
                Method::GET,
                &format!("/v2/alpine/blobs/{oci_digest}"),
                Some(&format!("Bearer {bearer}")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let pulled = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(pulled.as_ref(), bytes.as_slice());
    }
}
