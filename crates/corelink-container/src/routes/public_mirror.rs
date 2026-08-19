//! `POST /_internal/admin/public-mirror/*` — the increment-4b **server-only**
//! public-base MIRROR endpoint (F3.2 cross-tenant public OCI base-layer cache).
//!
//! # Security model
//!
//! - **Server-only, never tenant-reachable.** The path lives under
//!   `/_internal/admin/*`, which the edge Worker maps to the admin-consumer key;
//!   a tenant request is rejected at the edge (403) and never reaches this
//!   handler. This is the write side of Control 2 (only the operator, never a
//!   client, can promote bytes into `_public`).
//! - **Admin-gated (fail-CLOSED).** The route lives at
//!   `/_internal/admin/public-mirror/promote`, and the edge Worker maps every
//!   `/_internal/admin/*` path to the **admin consumer**, forwarding the caller's
//!   `x-corelink-internal-auth` header verbatim to the container. So — exactly
//!   like its sibling `/_internal/admin/*` routes — the container validates the
//!   **admin** key: `CORELINK_ADMIN_AUTH_KEY`, with the shared
//!   `CORELINK_INTERNAL_AUTH_KEY` as fallback (the per-consumer split in
//!   [`crate::routes::admin::resolve_internal_auth_key`]). A dedicated
//!   mirror-specific key would be WRONG here: it could never match what the edge
//!   forwards for an admin path, so the route would 401 dead. This does NOT
//!   weaken finding H4 — that is about the erase/REVOKE path
//!   ([`crate::routes::public_revoke`]) using the dedicated erase key with NO
//!   fallback; the mirror only PROMOTES with a verify-before-write, so admin-level
//!   auth is the correct gate. If no key resolves the route is NOT mounted
//!   ([`build_state_from_env`] returns `None`).
//!
//! # What the promote endpoint does (inc4b / WP-A — LIVE)
//!
//! `POST /_internal/admin/public-mirror/promote` carries a JSON body naming an
//! upstream OCI layer-blob (`{ "digest": "sha256:<hex>", "repository": "<repo>" }`).
//! The flow is fail-CLOSED at every step ([`fetch_verify_promote`]):
//!
//! 1. **Allowlist gate.** The digest MUST be
//!    [`is_allowlisted`](crate::public_base_allowlist::PublicBaseAllowlist::is_allowlisted)
//!    — the owner-pinned, digest-only layer-blob trust root
//!    ([`crate::public_base_allowlist`]). As of WP-E Roll-1 the shipped manifest
//!    allowlists exactly the alpine pin, so only that digest promotes; any other
//!    digest still rejects here — an arbitrary digest can never enter `_public`.
//! 2. **SSRF-safe upstream fetch.** The blob bytes are fetched from the FIXED
//!    upstream registry ([`FIXED_UPSTREAM_REGISTRY`], never a caller-supplied
//!    URL) through a reqwest client wired with the SINGLE audited SSRF guard
//!    ([`corelink_adapter_host::upstream_ssrf`]) — the redirect policy refuses
//!    any 307→CDN hop pointing at an internal IP literal, and the anonymous
//!    token realm is guarded by the SAME `host_is_internal_ip` classifier.
//!    Re-implementing SSRF here is FORBIDDEN; this reuses the guard brew/pip/npm
//!    share.
//! 3. **Write-time digest verify (MANDATORY, fail-CLOSED).**
//!    [`OciDigest::verify_against_bytes`] re-hashes the fetched bytes against the
//!    declared digest; a digest-LIE never reaches the shared slot.
//! 4. **Idempotent promote into `_public`.** [`MoatCache::put`] into
//!    [`crate::adapter_cache::PUBLIC_NAMESPACE`] with the genuine-unlimited
//!    shared-meter seed (`Some(0)`). Content-addressed + map-upserted, so a
//!    re-promote of the same bytes succeeds with no duplicate.
//!
//! # Writer-set invariant
//!
//! This mirror is the **second and last** legitimate `_public` writer — the
//! exhaustive set is `{ tenant finalize_upload, THIS inc4b admin mirror }`
//! (F3.2 binding invariant). The read pull-through (WP-G) reuses
//! [`fetch_verify_promote`] but promotes **per-tenant only**, never `_public`,
//! by passing a tenant namespace — so it adds no third `_public` writer.
//!
//! # Roll-1 posture
//!
//! The route is admin-gated (unmounted unless the admin key is bound). As of
//! WP-E Roll-1 the shipped allowlist activates exactly the alpine pin, so the
//! server mirror can promote that one digest into `_public`; every other digest
//! still rejects at the allowlist gate. Client-side dedup stays OFF
//! (`OCI_PUBLIC_DEDUP_ENABLED`), so no CLIENT can populate `_public` — only this
//! server mirror can (server-populates-first, design GAP-E). INERT until a
//! deploy + flip: this PR changes no live behaviour on its own.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use corelink_adapter_host::oci::digest::OciDigest;
use corelink_adapter_host::upstream_ssrf::{host_is_internal_ip, ssrf_safe_redirect_policy};

use crate::adapter_cache::{MoatCache, MoatError, PUBLIC_NAMESPACE};
use crate::public_base_allowlist::PublicBaseAllowlist;
use crate::routes::admin::resolve_internal_auth_key;
use crate::routes::internal_pat::internal_auth_ok;

/// The **admin** consumer secret gating the public-mirror surface. The route
/// sits under `/_internal/admin/*`, which the edge maps to the admin consumer
/// and forwards verbatim — so the container must validate this exact key (a
/// dedicated mirror key could never match the forwarded header). Resolved with
/// the shared `CORELINK_INTERNAL_AUTH_KEY` fallback in
/// [`resolve_internal_auth_key`].
const MIRROR_AUTH_KEY_ENV: &str = "CORELINK_ADMIN_AUTH_KEY";

/// Service principal stamped on the mirror's CAS writes (identifies the mirror
/// job, not any tenant PAT).
const MIRROR_SERVICE_PRINCIPAL: &str = "public-mirror-inc4b";

/// The FIXED upstream registry the mirror pulls public base layers from. This
/// is NOT caller-supplied: the promote body carries only a digest + repository,
/// and the origin (scheme + host) is pinned here so a caller can never redirect
/// the fetch at an arbitrary host. Docker Hub's registry endpoint.
const FIXED_UPSTREAM_REGISTRY: &str = "https://registry-1.docker.io";

/// Total per-request timeout for an upstream blob fetch — a slow/adversarial
/// upstream (or its blob CDN trickling bytes) must not pin a task indefinitely.
const MIRROR_UPSTREAM_TIMEOUT_SECS: u64 = 120;

/// TCP connect-phase timeout, bounded separately so a black-holed upstream fails
/// fast at connect time.
const MIRROR_UPSTREAM_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Max redirects the SSRF-safe policy follows. Docker Hub's blob endpoint
/// 307-redirects the actual bytes to its download CDN, so redirects must be
/// followed — but only to non-internal hosts (see [`ssrf_safe_redirect_policy`]).
const MIRROR_MAX_REDIRECTS: usize = 5;

/// Hard cap on a single mirrored blob (bytes buffered in heap before the moat
/// write). Base rootfs layers are tens of MiB; 1 GiB is a generous ceiling that
/// still bounds memory against a misbehaving/oversized upstream response.
const MIRROR_MAX_BLOB_BYTES: u64 = 1024 * 1024 * 1024;

/// Route state: the admin secret gating every mirror call, plus the collaborators
/// the promote body needs — the 2-level moat (`_public` write target), the
/// owner-pinned allowlist trust root, and the SSRF-safe upstream fetcher.
#[derive(Clone)]
pub struct PublicMirrorRouteState {
    /// Admin secret (consumer-specific, shared fallback) for the constant-time
    /// `x-corelink-internal-auth` gate.
    auth_key: Arc<str>,
    /// The shared 2-level content-dedup cache the promote writes into
    /// ([`PUBLIC_NAMESPACE`]).
    moat: Arc<MoatCache>,
    /// Owner-curated, digest-pinned allowlist — the ONLY digests eligible for
    /// `_public`. Deny-all until WP-E.
    allowlist: Arc<PublicBaseAllowlist>,
    /// SSRF-safe fetcher for the FIXED upstream registry. A trait object so
    /// tests inject a hermetic fake (no network) while prod wires the reqwest
    /// Docker Hub client.
    fetcher: Arc<dyn UpstreamBlobFetcher>,
}

impl std::fmt::Debug for PublicMirrorRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never leak the admin secret via Debug (mirrors PublicRevokeRouteState).
        f.debug_struct("PublicMirrorRouteState")
            .field("auth_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Build the route state from env, or `None` (route NOT mounted) when the
/// prerequisites are absent — the admin `CORELINK_ADMIN_AUTH_KEY` (or the shared
/// `CORELINK_INTERNAL_AUTH_KEY` fallback, each ≥ 32 chars) AND the D1/R2 storage
/// env the moat needs.
/// Fail-CLOSED: absent config yields no route, exactly like the sibling admin
/// surfaces.
#[must_use]
pub fn build_state_from_env() -> Option<PublicMirrorRouteState> {
    let auth_key = resolve_internal_auth_key(MIRROR_AUTH_KEY_ENV)?;
    // The url→content-hash map (D1) is required to write `_public`; its absence
    // means no storage env, so the route stays unmounted (fail-CLOSED) rather
    // than mounting a promote that cannot persist.
    let map = crate::adapter_cache::d1_map_from_env()?;
    let (cas_read, cas_write, _delete, _list) = crate::routes::cas::build_handlers();
    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        MIRROR_SERVICE_PRINCIPAL,
    ));
    // Baked owner manifest, FAIL-CLOSED: a malformed manifest degrades to
    // deny-all (empty), never a partially-trusted set — a broken manifest can
    // never widen `_public`.
    let allowlist = Arc::new(PublicBaseAllowlist::from_baked_manifest().unwrap_or_default());
    let fetcher: Arc<dyn UpstreamBlobFetcher> = match DockerHubBlobFetcher::new() {
        Ok(f) => Arc::new(f),
        Err(e) => {
            tracing::error!(error = %e, "public-mirror: upstream fetcher init failed; route NOT mounted");
            return None;
        }
    };
    Some(PublicMirrorRouteState {
        auth_key,
        moat,
        allowlist,
        fetcher,
    })
}

/// Mount the mirror router at its top-level `/_internal/admin/*` path.
pub fn router(state: PublicMirrorRouteState) -> Router {
    Router::new()
        .route(
            "/_internal/admin/public-mirror/promote",
            post(handle_promote),
        )
        .with_state(state)
}

/// The promote request body: an owner-pinned upstream layer-blob to mirror into
/// `_public`. The `digest` is the trust anchor (allowlist-gated AND
/// verify-against-bytes'd); `repository` is only a locator for the FIXED
/// upstream's blob path + anonymous-token scope — it carries no trust (a wrong
/// repo either fails the fetch or yields bytes that fail the digest verify).
#[derive(Debug, Deserialize)]
struct PromoteRequest {
    /// Upstream OCI content digest, `sha256:<64 lowercase hex>`.
    digest: String,
    /// Upstream repository the blob lives under (e.g. `library/alpine`), used to
    /// build the FIXED-upstream blob path + token scope.
    repository: String,
}

/// `POST /_internal/admin/public-mirror/promote` — the inc4b promote handler.
///
/// The auth gate runs FIRST (401 before any work). An authenticated call runs
/// the fail-CLOSED [`fetch_verify_promote`] flow into [`PUBLIC_NAMESPACE`] with
/// the shared-meter seed (`Some(0)`). Errors map to a JSON body + status
/// ([`PromoteError`]); success is `200 { "status": "promoted", … }`.
async fn handle_promote(
    State(state): State<PublicMirrorRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !internal_auth_ok(state.auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    let req: PromoteRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "bad_request",
                    "detail": format!("invalid promote body: {e}"),
                })),
            )
                .into_response();
        }
    };
    match fetch_verify_promote(
        state.fetcher.as_ref(),
        &state.allowlist,
        &state.moat,
        &req.repository,
        &req.digest,
        // The mirror is the SECOND `_public` writer: it promotes into the shared
        // namespace with the genuine-unlimited shared-meter seed.
        PUBLIC_NAMESPACE,
        Some(0),
    )
    .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "promoted",
                "digest": req.digest,
                "namespace": PUBLIC_NAMESPACE,
            })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

/// Fail-CLOSED outcomes of a promote. Each maps to a status + JSON body; a
/// verify mismatch or a non-allowlisted digest NEVER results in a write.
#[derive(Debug)]
pub(crate) enum PromoteError {
    /// The digest is not a canonical `sha256:`/`sha512:` wire digest (400).
    BadDigest(String),
    /// The `repository` locator is not a well-formed OCI repo name (400) — a
    /// path-traversal / origin-escape guard on the FIXED-upstream blob path.
    BadRepository(String),
    /// The digest is not on the owner-pinned allowlist (422) — the deny-all
    /// INERT rejection in prod; nothing is fetched or written.
    NotAllowlisted(String),
    /// The SSRF-safe upstream fetch failed (502) — connect/redirect-guard/status
    /// error. Nothing is written.
    UpstreamFetch(String),
    /// The fetched bytes do not hash to the declared digest (422). FAIL-CLOSED:
    /// a digest-lie never reaches the shared slot.
    DigestMismatch(String),
    /// The moat write failed after verification (500).
    Store(String),
}

impl IntoResponse for PromoteError {
    fn into_response(self) -> Response {
        let (status, error, detail) = match self {
            Self::BadDigest(d) => (StatusCode::BAD_REQUEST, "bad_digest", d),
            Self::BadRepository(d) => (StatusCode::BAD_REQUEST, "bad_repository", d),
            Self::NotAllowlisted(d) => (StatusCode::UNPROCESSABLE_ENTITY, "not_allowlisted", d),
            Self::UpstreamFetch(d) => (StatusCode::BAD_GATEWAY, "upstream_fetch", d),
            Self::DigestMismatch(d) => (StatusCode::UNPROCESSABLE_ENTITY, "digest_mismatch", d),
            Self::Store(d) => (StatusCode::INTERNAL_SERVER_ERROR, "store", d),
        };
        (
            status,
            Json(serde_json::json!({ "error": error, "detail": detail })),
        )
            .into_response()
    }
}

/// The shared inc4b promote flow, parameterized by target namespace so WP-G's
/// read pull-through can reuse the IDENTICAL fetch→verify pipeline while
/// promoting **per-tenant** (never `_public`). Fail-CLOSED at every step:
///
/// 1. parse the digest (canonical wire form),
/// 2. `is_allowlisted` gate — only an owner-pinned digest proceeds,
/// 3. validate the repository locator (origin-escape guard),
/// 4. SSRF-safe fetch from the FIXED upstream,
/// 5. `verify_against_bytes` (MANDATORY) — a mismatch aborts BEFORE any write,
/// 6. idempotent [`MoatCache::put`] into `namespace` with `storage_cap_bytes`.
///
/// The mirror admin path passes (`PUBLIC_NAMESPACE`, `Some(0)`); WP-G will pass
/// a tenant namespace + the tenant's resolved cap — so this helper introduces no
/// new `_public` writer on its own, the CALL SITE chooses the namespace.
///
/// # Errors
/// Returns the first failing step as a [`PromoteError`]; on any error nothing is
/// written to `namespace`.
pub(crate) async fn fetch_verify_promote(
    fetcher: &dyn UpstreamBlobFetcher,
    allowlist: &PublicBaseAllowlist,
    moat: &MoatCache,
    repository: &str,
    digest: &str,
    namespace: &str,
    storage_cap_bytes: Option<i64>,
) -> Result<(), PromoteError> {
    // 1. Parse + canonicalize the digest. `OciDigest::parse` normalizes hex to
    //    lowercase, so the allowlist check + storage key use the canonical wire
    //    form the manifest stores (entries are lowercase-only by construction).
    let parsed = OciDigest::parse(digest)
        .map_err(|e| PromoteError::BadDigest(format!("{digest}: {e:?}")))?;
    let canonical = parsed.to_wire();
    // 2. Allowlist gate (fail-CLOSED). Deny-all in prod → every real call stops
    //    here; nothing is fetched or written.
    if !allowlist.is_allowlisted(&canonical) {
        return Err(PromoteError::NotAllowlisted(canonical));
    }
    // 3. Repository locator validation (origin-escape / path-traversal guard on
    //    the FIXED-upstream blob path).
    validate_repository(repository).map_err(PromoteError::BadRepository)?;
    // 4. SSRF-safe fetch from the FIXED upstream (never a caller URL).
    let bytes = fetcher
        .fetch_blob(repository, &canonical)
        .await
        .map_err(PromoteError::UpstreamFetch)?;
    // 5. Write-time digest verify (MANDATORY, fail-CLOSED) — a digest-lie never
    //    reaches `namespace`.
    parsed
        .verify_against_bytes(&bytes)
        .map_err(|e| PromoteError::DigestMismatch(format!("{canonical}: {e:?}")))?;
    // 6. Idempotent promote. Content-addressed CAS write + map upsert, keyed by
    //    the canonical digest — a re-promote of the same bytes succeeds with no
    //    duplicate.
    moat.put(namespace, &canonical, bytes, storage_cap_bytes)
        .await
        .map_err(|e| match e {
            MoatError::Backend(m) => PromoteError::Store(m),
        })?;
    Ok(())
}

/// Validate a `repository` is a well-formed, lowercase OCI repository name — the
/// origin-escape guard for the FIXED-upstream blob path. Rejects the empty
/// string, any character outside `[a-z0-9._/-]`, a leading/trailing `/`, an
/// empty path component (`//`), and any `..` component (path traversal). A valid
/// name can only extend the FIXED upstream's `/v2/<repo>/blobs/…` path, never
/// swap its host.
///
/// `pub(crate)` so the WP-G read pull-through
/// ([`crate::routes::public_pullthrough`]) gates the repo through the SAME guard
/// before any upstream manifest/blob fetch.
pub(crate) fn validate_repository(repository: &str) -> Result<(), String> {
    if repository.is_empty() {
        return Err("repository is empty".to_owned());
    }
    if repository.starts_with('/') || repository.ends_with('/') {
        return Err(format!(
            "repository '{repository}' has a leading/trailing '/'"
        ));
    }
    for component in repository.split('/') {
        if component.is_empty() {
            return Err(format!(
                "repository '{repository}' has an empty path component"
            ));
        }
        if component == ".." || component == "." {
            return Err(format!(
                "repository '{repository}' has a '.'/'..' path component (traversal)"
            ));
        }
    }
    if let Some(bad) = repository.bytes().find(|b| {
        !(b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-' | b'/'))
    }) {
        return Err(format!(
            "repository '{repository}' has a disallowed character '{}'",
            char::from(bad)
        ));
    }
    Ok(())
}

/// The multi-media-type `Accept` a manifest fetch negotiates: an OCI image
/// index, a Docker manifest list, an OCI image manifest, and a Docker v2 image
/// manifest — the four shapes buildkit resolves through. Docker Hub
/// content-negotiates on this header, so omitting it yields the legacy v1
/// manifest.
const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.index.v1+json,\
application/vnd.docker.distribution.manifest.list.v2+json,\
application/vnd.oci.image.manifest.v1+json,\
application/vnd.docker.distribution.manifest.v2+json";

/// Hard cap on a single fetched manifest (bytes buffered before verify). OCI
/// manifests + indexes are KiB-scale; 4 MiB mirrors the adapter's
/// `MAX_MANIFEST_BYTES` DoS guard and bounds a hostile upstream.
const MIRROR_MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

// ── Shared SSRF-safe upstream registry client ────────────────────────────────

/// The ONE audited SSRF-safe upstream registry client: a reqwest client wired
/// with the SINGLE [`ssrf_safe_redirect_policy`] guard, the FIXED
/// `registry-1.docker.io` origin, and the anonymous-token dance. BOTH the blob
/// fetcher ([`DockerHubBlobFetcher`]) and the manifest fetcher
/// ([`DockerHubManifestFetcher`]) share ONE instance, so there is exactly one
/// SSRF/token implementation (the F3.2 "one SSRF guard" audit invariant) — a
/// second copy of the redirect policy or the token dance is FORBIDDEN.
#[derive(Debug)]
pub(crate) struct UpstreamRegistryClient {
    client: reqwest::Client,
    upstream: url::Url,
}

impl UpstreamRegistryClient {
    /// Build the client against [`FIXED_UPSTREAM_REGISTRY`].
    ///
    /// # Errors
    /// Returns an error string if the fixed URL is unparseable or the reqwest
    /// client cannot be built (e.g. TLS init failure).
    pub(crate) fn new() -> Result<Self, String> {
        let upstream = url::Url::parse(FIXED_UPSTREAM_REGISTRY)
            .map_err(|e| format!("fixed upstream parse: {e}"))?;
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "corelink-public-mirror/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(MIRROR_UPSTREAM_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(MIRROR_UPSTREAM_CONNECT_TIMEOUT_SECS))
            .redirect(ssrf_safe_redirect_policy(MIRROR_MAX_REDIRECTS))
            .build()
            .map_err(|e| format!("client build: {e}"))?;
        Ok(Self { client, upstream })
    }

    /// Resolve an anonymous OCI bearer token from a 401's
    /// `WWW-Authenticate: Bearer realm=…,service=…,scope=…` challenge, scoped to
    /// `repository:<repo>:pull`. SSRF guard: the realm must be `https` and its
    /// host must NOT be an internal IP literal (reusing the shared
    /// [`host_is_internal_ip`] classifier — no second SSRF copy). Returns
    /// `Ok(None)` when the response carries no parseable Bearer challenge.
    async fn anon_token_for(
        &self,
        challenged: &reqwest::Response,
        repository: &str,
    ) -> Result<Option<String>, String> {
        let header = match challenged.headers().get(reqwest::header::WWW_AUTHENTICATE) {
            Some(h) => h
                .to_str()
                .map_err(|_| "non-ascii WWW-Authenticate".to_owned())?,
            None => return Ok(None),
        };
        let Some(realm) = parse_bearer_realm(header) else {
            return Ok(None);
        };
        let realm_url = url::Url::parse(&realm).map_err(|e| format!("token realm parse: {e}"))?;
        // SSRF guard on the token realm host: https + not an internal IP literal.
        // (`host_is_internal_ip` is the SAME audited classifier the redirect
        // policy uses — the container reuses it rather than re-implementing SSRF.)
        if realm_url.scheme() != "https" {
            return Err("SSRF guard: token realm is not https".to_owned());
        }
        match realm_url.host_str() {
            Some(h) if !host_is_internal_ip(h) => {}
            Some(h) => return Err(format!("SSRF guard: token realm host {h} is internal")),
            None => return Err("SSRF guard: token realm has no host".to_owned()),
        }
        // Echo the `service` value FROM the challenge, not the upstream HOST.
        // Docker Hub's blob host is `registry-1.docker.io` but its token service
        // identifier is `registry.docker.io` (no `-1`); `auth.docker.io` binds
        // the issued token to the requested `service`, and the registry rejects
        // a token whose `service` does not match its own identifier with a 401.
        // Deriving `service` from `upstream.host_str()` therefore mints a token
        // the registry refuses. Use the challenge's `service`, falling back to
        // the upstream host only when the challenge omits it.
        let service = parse_bearer_param(header, "service").unwrap_or_else(|| {
            self.upstream
                .host_str()
                .unwrap_or("registry-1.docker.io")
                .to_owned()
        });
        let mut token_url = realm_url;
        token_url
            .query_pairs_mut()
            .append_pair("service", &service)
            .append_pair("scope", &format!("repository:{repository}:pull"));
        let resp = self
            .client
            .get(token_url)
            .send()
            .await
            .map_err(|e| format!("token fetch: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("token endpoint status: {}", resp.status()));
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| format!("token json: {e}"))?;
        Ok(body
            .get("token")
            .or_else(|| body.get("access_token"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned))
    }

    /// GET `path` on the FIXED upstream origin with the given `accept` header,
    /// retrying ONCE with an anonymous bearer on a 401. Returns the successful
    /// response (headers intact, body unread) or an error string. `path` is
    /// built on the fixed origin and re-asserted to stay there (defence in
    /// depth — a scheme-bearing repo would host-swap via `Url::join`).
    async fn get_with_anon_retry(
        &self,
        path: &str,
        repository: &str,
        accept: &str,
    ) -> Result<reqwest::Response, String> {
        let url = self
            .upstream
            .join(path)
            .map_err(|e| format!("url join: {e}"))?;
        if url.scheme() != self.upstream.scheme() || url.host_str() != self.upstream.host_str() {
            return Err("SSRF guard: URL escaped the fixed upstream origin".to_owned());
        }
        let mut response = self
            .client
            .get(url.clone())
            .header(reqwest::header::ACCEPT, accept)
            .send()
            .await
            .map_err(|e| format!("send: {e}"))?;
        // Docker Hub 401s an anonymous pull with a Bearer challenge; resolve the
        // anonymous token and retry once.
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(token) = self.anon_token_for(&response, repository).await? {
                response = self
                    .client
                    .get(url)
                    .bearer_auth(token)
                    .header(reqwest::header::ACCEPT, accept)
                    .send()
                    .await
                    .map_err(|e| format!("send (authed): {e}"))?;
            }
        }
        let status = response.status();
        if !status.is_success() {
            return Err(format!("upstream status: {status}"));
        }
        Ok(response)
    }

    /// Stream `response`'s body into a heap buffer, enforcing `max_bytes` both
    /// against the declared `Content-Length` (fail-fast) and the streamed total.
    async fn read_capped(response: reqwest::Response, max_bytes: u64) -> Result<Vec<u8>, String> {
        if let Some(declared) = response.content_length() {
            if declared > max_bytes {
                return Err(format!("upstream body oversized: {declared} > {max_bytes}"));
            }
        }
        let capacity = response
            .content_length()
            .and_then(|l| usize::try_from(l).ok())
            .unwrap_or(0);
        let mut buf: Vec<u8> = Vec::with_capacity(capacity);
        let mut stream = response;
        while let Some(chunk) = stream
            .chunk()
            .await
            .map_err(|e| format!("read chunk: {e}"))?
        {
            let new_total =
                u64::try_from(buf.len().saturating_add(chunk.len())).unwrap_or(u64::MAX);
            if new_total > max_bytes {
                return Err(format!(
                    "upstream body oversized while streaming: {new_total} > {max_bytes}"
                ));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }

    /// Fetch `GET /v2/<repo>/blobs/<digest>` (unverified bytes).
    async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String> {
        let path = format!("/v2/{repository}/blobs/{digest}");
        let response = self.get_with_anon_retry(&path, repository, "*/*").await?;
        Self::read_capped(response, MIRROR_MAX_BLOB_BYTES).await
    }

    /// Fetch `GET /v2/<repo>/manifests/<reference>` (tag OR `sha256:` digest)
    /// with the multi-media-type [`MANIFEST_ACCEPT`], returning the raw bytes,
    /// the served `Content-Type`, and the upstream `Docker-Content-Digest` (when
    /// present — the caller still recomputes for a tag). Unverified.
    async fn fetch_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<FetchedManifest, String> {
        let path = format!("/v2/{repository}/manifests/{reference}");
        let response = self
            .get_with_anon_retry(&path, repository, MANIFEST_ACCEPT)
            .await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let docker_content_digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = Self::read_capped(response, MIRROR_MAX_MANIFEST_BYTES).await?;
        Ok(FetchedManifest {
            bytes,
            content_type,
            docker_content_digest,
        })
    }
}

// ── Upstream blob fetcher ────────────────────────────────────────────────────

/// SSRF-safe fetcher of an upstream OCI layer blob. A trait so the promote flow
/// is testable hermetically (a fake returns canned bytes) while prod wires the
/// reqwest client against the FIXED registry.
#[async_trait::async_trait]
pub(crate) trait UpstreamBlobFetcher: Send + Sync + std::fmt::Debug {
    /// Fetch the blob identified by `digest` under `repository` from the FIXED
    /// upstream. Returns the raw bytes (unverified — the caller runs
    /// `verify_against_bytes`), or an error string on any failure.
    async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String>;
}

/// Production blob fetcher: a thin wrapper over the shared
/// [`UpstreamRegistryClient`] (`GET {FIXED_UPSTREAM_REGISTRY}/v2/<repo>/blobs/<digest>`).
#[derive(Debug)]
pub(crate) struct DockerHubBlobFetcher {
    client: Arc<UpstreamRegistryClient>,
}

impl DockerHubBlobFetcher {
    /// Build a blob fetcher over a fresh [`UpstreamRegistryClient`].
    ///
    /// # Errors
    /// Propagates [`UpstreamRegistryClient::new`]'s error.
    fn new() -> Result<Self, String> {
        Ok(Self {
            client: Arc::new(UpstreamRegistryClient::new()?),
        })
    }

    /// Build a blob fetcher SHARING an existing [`UpstreamRegistryClient`] (so
    /// the blob + manifest fetchers reuse ONE SSRF/token implementation).
    pub(crate) fn from_client(client: Arc<UpstreamRegistryClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl UpstreamBlobFetcher for DockerHubBlobFetcher {
    async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String> {
        self.client.fetch_blob(repository, digest).await
    }
}

// ── Upstream manifest fetcher ────────────────────────────────────────────────

/// A manifest fetched from the FIXED upstream (unverified — the caller
/// digest-verifies for a `sha256:` reference or recomputes for a tag).
#[derive(Debug, Clone)]
pub(crate) struct FetchedManifest {
    /// Raw manifest bytes.
    pub bytes: Vec<u8>,
    /// The upstream-served `Content-Type` (media type), if any.
    pub content_type: Option<String>,
    /// The upstream `Docker-Content-Digest`, if any.
    pub docker_content_digest: Option<String>,
}

/// SSRF-safe fetcher of an upstream OCI manifest (tag OR digest). A trait so the
/// WP-G resolver is testable hermetically while prod wires the shared reqwest
/// client against the FIXED registry.
#[async_trait::async_trait]
pub(crate) trait UpstreamManifestFetcher: Send + Sync + std::fmt::Debug {
    /// Fetch the manifest for `reference` under `repository` from the FIXED
    /// upstream with the multi-media-type `Accept`. Returns the unverified
    /// [`FetchedManifest`], or an error string on any failure.
    async fn fetch_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<FetchedManifest, String>;
}

/// Production manifest fetcher: a thin wrapper over the shared
/// [`UpstreamRegistryClient`].
#[derive(Debug)]
pub(crate) struct DockerHubManifestFetcher {
    client: Arc<UpstreamRegistryClient>,
}

impl DockerHubManifestFetcher {
    /// Build a manifest fetcher SHARING an existing [`UpstreamRegistryClient`].
    pub(crate) fn from_client(client: Arc<UpstreamRegistryClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl UpstreamManifestFetcher for DockerHubManifestFetcher {
    async fn fetch_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<FetchedManifest, String> {
        self.client.fetch_manifest(repository, reference).await
    }
}

/// Extract the `realm="…"` value from a `WWW-Authenticate: Bearer …` header, or
/// `None` if it is not a Bearer challenge or carries no realm. Docker Hub's
/// challenge values contain no commas, so a comma split of the param list is
/// sufficient.
fn parse_bearer_realm(header: &str) -> Option<String> {
    parse_bearer_param(header, "realm")
}

/// Extract a single `key="value"` parameter from a `Bearer …` challenge header
/// (`realm`, `service`, `scope`). Case-insensitive on the `Bearer` prefix; the
/// value's surrounding quotes are stripped. Returns `None` when the prefix or
/// the key is absent.
fn parse_bearer_param(header: &str, key: &str) -> Option<String> {
    let rest = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))?;
    for part in rest.split(',') {
        if let Some((k, v)) = part.trim().split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').to_owned());
            }
        }
    }
    None
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

    use async_trait::async_trait;
    use axum::body::Body;
    use corelink_adapter_host::oci::digest::OciDigestAlgo;
    use corelink_handler_cas::{
        CasHandlerError, CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler,
        CasWriteRequest, CasWriteResponse,
    };
    use http::Request;
    use tower::ServiceExt;

    use crate::adapter_cache::UrlMapStore;

    use super::*;

    #[test]
    fn bearer_param_extracts_dockerhub_service_not_blob_host() {
        // The real Docker Hub blob-401 challenge: the blob HOST is
        // `registry-1.docker.io` but the token `service` is `registry.docker.io`
        // (no `-1`). The fetcher MUST echo this `service`, not re-derive it from
        // the upstream host, or auth.docker.io mints a token the registry 401s.
        let header = r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#;
        assert_eq!(
            parse_bearer_realm(header).as_deref(),
            Some("https://auth.docker.io/token"),
        );
        assert_eq!(
            parse_bearer_param(header, "service").as_deref(),
            Some("registry.docker.io"),
        );
        assert_eq!(
            parse_bearer_param(header, "scope").as_deref(),
            Some("repository:library/alpine:pull"),
        );
        assert_eq!(parse_bearer_param(header, "absent"), None);
        assert_eq!(parse_bearer_param("Basic realm=\"x\"", "service"), None);
    }

    const TEST_KEY: &str = "test-admin-secret-key-at-least-32-chars!!";
    /// A real alpine amd64 rootfs layer digest (the commented example pin in the
    /// shipped manifest). Used ONLY as an allowlist key in-test — never fetched
    /// (the fake fetcher returns canned bytes).
    const ALPINE_DIGEST: &str =
        "sha256:25f1d6b1951ac8eb3740558fe94cb83d377bdadf95fd9f98b50d2e1b96130471";

    /// Canonical `sha256:<hex>` wire digest of `bytes` (via the same
    /// `OciDigest` compute the verify path uses).
    fn sha256_wire(bytes: &[u8]) -> String {
        OciDigest::compute(OciDigestAlgo::Sha256, bytes)
            .expect("sha256 compute")
            .to_wire()
    }

    /// Fake fetcher: hands back canned bytes for any (repo, digest), or a fixed
    /// error. Records the last (repo, digest) it was asked for.
    #[derive(Debug)]
    struct FakeFetcher {
        result: Result<Vec<u8>, String>,
        calls: Mutex<Vec<(String, String)>>,
    }
    impl FakeFetcher {
        fn returning(bytes: Vec<u8>) -> Self {
            Self {
                result: Ok(bytes),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn failing(msg: &str) -> Self {
            Self {
                result: Err(msg.to_owned()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl UpstreamBlobFetcher for FakeFetcher {
        async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String> {
            self.calls
                .lock()
                .unwrap()
                .push((repository.to_owned(), digest.to_owned()));
            self.result.clone()
        }
    }

    /// In-memory url→content-hash map recording (namespace, url_hash).
    #[derive(Default, Debug)]
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

    /// Non-verifying in-memory CAS keyed by (namespace, claimed_hash). Records
    /// every stored blob so a test can assert the `_public` write happened.
    #[derive(Debug, Default)]
    struct RecordingCas(Mutex<HashMap<(String, String), Vec<u8>>>);
    impl CasReadHandler for RecordingCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            match self
                .0
                .lock()
                .unwrap()
                .get(&(req.tenant.clone(), req.hash.clone()))
            {
                Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
                None => Err(CasHandlerError::Internal("absent".into())),
            }
        }
    }
    impl CasWriteHandler for RecordingCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            self.0
                .lock()
                .unwrap()
                .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    /// Deterministic non-crypto hasher for the moat's content-address (the moat
    /// hasher is orthogonal to the OCI digest verify — that uses real sha256).
    fn fake_hash(bytes: &[u8]) -> String {
        format!("h{:08x}", bytes.iter().map(|b| u32::from(*b)).sum::<u32>())
    }

    /// Build a moat over the recording CAS + fake map.
    fn moat(cas: Arc<RecordingCas>, map: Arc<FakeMap>) -> Arc<MoatCache> {
        Arc::new(MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            MIRROR_SERVICE_PRINCIPAL,
        ))
    }

    /// Allowlist containing exactly `digests` (hermetic — does NOT depend on the
    /// shipped deny-all manifest, mirroring WP-B's `with_allowlist` pattern).
    fn allowlist_with(digests: &[&str]) -> Arc<PublicBaseAllowlist> {
        let manifest = digests.join("\n");
        Arc::new(PublicBaseAllowlist::parse(&manifest).expect("test allowlist parses"))
    }

    fn state(
        moat: Arc<MoatCache>,
        allowlist: Arc<PublicBaseAllowlist>,
        fetcher: Arc<dyn UpstreamBlobFetcher>,
    ) -> PublicMirrorRouteState {
        PublicMirrorRouteState {
            auth_key: Arc::from(TEST_KEY),
            moat,
            allowlist,
            fetcher,
        }
    }

    fn promote_req(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(http::Method::POST)
            .uri("/_internal/admin/public-mirror/promote")
            .header("content-type", "application/json");
        if let Some(a) = auth {
            b = b.header("x-corelink-internal-auth", a);
        }
        b.body(Body::from(body.to_string())).unwrap()
    }

    // ── S0 auth-gate tests (kept green) ──────────────────────────────────────

    #[tokio::test]
    async fn unauthenticated_is_401() {
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let st = state(
            moat(Arc::clone(&cas), Arc::clone(&map)),
            allowlist_with(&[]),
            Arc::new(FakeFetcher::returning(vec![])),
        );
        let resp = router(st)
            .oneshot(promote_req(
                None,
                serde_json::json!({"digest": ALPINE_DIGEST, "repository": "library/alpine"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        // No write occurred.
        assert!(cas.0.lock().unwrap().is_empty());
        assert!(map.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn wrong_secret_is_401() {
        let st = state(
            moat(
                Arc::new(RecordingCas::default()),
                Arc::new(FakeMap::default()),
            ),
            allowlist_with(&[]),
            Arc::new(FakeFetcher::returning(vec![])),
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some("wrong-but-also-32-chars-long-secret!!!"),
                serde_json::json!({"digest": ALPINE_DIGEST, "repository": "library/alpine"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── inc4b promote tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn allowlisted_digest_promotes_into_public_namespace() {
        // An admin-authed hit with an allowlisted digest whose bytes match →
        // a `_public` map row + CAS blob is written (namespace == PUBLIC_NAMESPACE).
        let bytes = b"alpine-rootfs-layer-bytes".to_vec();
        let digest = sha256_wire(&bytes);
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let st = state(
            moat(Arc::clone(&cas), Arc::clone(&map)),
            allowlist_with(&[&digest]),
            Arc::new(FakeFetcher::returning(bytes.clone())),
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": digest, "repository": "library/alpine"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // The map row is under PUBLIC_NAMESPACE keyed by the canonical digest.
        let map_guard = map.0.lock().unwrap();
        let content_hash = map_guard
            .get(&(PUBLIC_NAMESPACE.to_owned(), digest.clone()))
            .expect("_public map row written");
        // The CAS blob is stored under PUBLIC_NAMESPACE + that content hash.
        let cas_guard = cas.0.lock().unwrap();
        let stored = cas_guard
            .get(&(PUBLIC_NAMESPACE.to_owned(), content_hash.clone()))
            .expect("_public CAS blob written");
        assert_eq!(stored, &bytes);
    }

    #[tokio::test]
    async fn non_allowlisted_digest_is_rejected_no_write() {
        // Arbitrary (non-allowlisted) digest → 422, NO fetch, NO write.
        let bytes = b"whatever".to_vec();
        let digest = sha256_wire(&bytes);
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let fetcher = Arc::new(FakeFetcher::returning(bytes.clone()));
        let st = state(
            moat(Arc::clone(&cas), Arc::clone(&map)),
            allowlist_with(&[]), // deny-all
            Arc::clone(&fetcher) as Arc<dyn UpstreamBlobFetcher>,
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": digest, "repository": "library/alpine"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(cas.0.lock().unwrap().is_empty(), "no CAS write");
        assert!(map.0.lock().unwrap().is_empty(), "no map write");
        assert!(
            fetcher.calls.lock().unwrap().is_empty(),
            "allowlist gate rejects BEFORE any upstream fetch"
        );
    }

    #[tokio::test]
    async fn verify_mismatch_fails_closed_no_write() {
        // Bytes that do NOT hash to the (allowlisted) declared digest → 422,
        // fail-closed, NO write. Uses the real alpine digest but the fake fetcher
        // returns different bytes.
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let st = state(
            moat(Arc::clone(&cas), Arc::clone(&map)),
            allowlist_with(&[ALPINE_DIGEST]),
            Arc::new(FakeFetcher::returning(b"not-the-alpine-bytes".to_vec())),
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": ALPINE_DIGEST, "repository": "library/alpine"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(cas.0.lock().unwrap().is_empty(), "digest-lie never written");
        assert!(map.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn idempotent_repromote_no_duplicate() {
        // Re-promoting the same bytes succeeds and leaves exactly one `_public`
        // row + one CAS blob (content-addressed, map-upserted).
        let bytes = b"debian-rootfs-layer".to_vec();
        let digest = sha256_wire(&bytes);
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let st = state(
            moat(Arc::clone(&cas), Arc::clone(&map)),
            allowlist_with(&[&digest]),
            Arc::new(FakeFetcher::returning(bytes.clone())),
        );
        let router = router(st);
        for _ in 0..2 {
            let resp = router
                .clone()
                .oneshot(promote_req(
                    Some(TEST_KEY),
                    serde_json::json!({"digest": digest, "repository": "library/debian"}),
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        assert_eq!(
            map.0.lock().unwrap().len(),
            1,
            "exactly one _public map row"
        );
        assert_eq!(cas.0.lock().unwrap().len(), 1, "exactly one CAS blob");
    }

    #[tokio::test]
    async fn upstream_fetch_error_maps_502_no_write() {
        let bytes = b"x".to_vec();
        let digest = sha256_wire(&bytes);
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let st = state(
            moat(Arc::clone(&cas), Arc::clone(&map)),
            allowlist_with(&[&digest]),
            Arc::new(FakeFetcher::failing("connect refused")),
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": digest, "repository": "library/alpine"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        assert!(cas.0.lock().unwrap().is_empty());
        assert!(map.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_repository_is_rejected_no_fetch() {
        let bytes = b"y".to_vec();
        let digest = sha256_wire(&bytes);
        let fetcher = Arc::new(FakeFetcher::returning(bytes));
        let st = state(
            moat(
                Arc::new(RecordingCas::default()),
                Arc::new(FakeMap::default()),
            ),
            allowlist_with(&[&digest]),
            Arc::clone(&fetcher) as Arc<dyn UpstreamBlobFetcher>,
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": digest, "repository": "../evil"}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(
            fetcher.calls.lock().unwrap().is_empty(),
            "repo validation rejects before any fetch"
        );
    }

    #[tokio::test]
    async fn malformed_body_is_400() {
        let st = state(
            moat(
                Arc::new(RecordingCas::default()),
                Arc::new(FakeMap::default()),
            ),
            allowlist_with(&[]),
            Arc::new(FakeFetcher::returning(vec![])),
        );
        let resp = router(st)
            .oneshot(promote_req(
                Some(TEST_KEY),
                serde_json::json!({"digest": ALPINE_DIGEST}),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_repository_accepts_and_rejects() {
        for ok in [
            "library/alpine",
            "library/debian",
            "a",
            "a/b/c",
            "foo-bar.baz_qux",
        ] {
            assert!(validate_repository(ok).is_ok(), "{ok} should be valid");
        }
        for bad in [
            "",
            "/leading",
            "trailing/",
            "a//b",
            "../evil",
            "UP",
            "a/../b",
            "has space",
        ] {
            assert!(
                validate_repository(bad).is_err(),
                "{bad} should be rejected"
            );
        }
    }
}
