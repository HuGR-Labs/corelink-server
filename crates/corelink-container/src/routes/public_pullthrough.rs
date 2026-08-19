//! WP-G — per-tenant OCI manifest + blob **upstream-on-miss** resolution
//! (M1 of the manifest-resolution keystone).
//!
//! # What this is
//!
//! [`UpstreamManifestResolver`] implements the adapter-host
//! [`ManifestResolver`] port. The manifest `GET`/`HEAD` handlers consult the
//! per-tenant [`ManifestKvStore`] first; only on a MISS do they call
//! [`ManifestResolver::resolve_on_miss`]. This resolver then fetches the
//! manifest for `(tenant, repo, reference)` from the FIXED upstream
//! (`registry-1.docker.io`), verifies it, persists it into the tenant's OWN KV
//! namespace, and — for an image manifest — eagerly fetches + digest-verifies +
//! persists its config + layer blobs into the tenant's OWN moat namespace, so
//! the subsequent by-digest blob `GET`s (which carry no repo of their own) hit
//! the per-tenant moat. It returns the bytes to serve.
//!
//! # Why per-tenant, no `_public` (M1 scope)
//!
//! A **manifest is a pointer** — it names blob digests. Content-addressing
//! protects blob bytes, not the pointer. So a per-tenant on-miss needs NO
//! allowlist: the bytes land in the tenant's own namespace, only that tenant
//! reads them, and each is digest-verified on fetch. A tenant fetching a
//! poisoned digest only poisons ITSELF. The gate here is therefore **abuse**
//! (rate-limit + size-cap + the tenant's own resolved quota), not trust. This
//! introduces NO new `_public` writer — cross-tenant `_public` image resolution
//! is M2 (allowlist-gated by owner-pinned image-manifest digest).
//!
//! # Flag-gated OFF, INERT
//!
//! Wired only when [`crate::public_flags::oci_upstream_on_miss`]
//! (`OCI_UPSTREAM_ON_MISS`) is ON at boot AND the upstream client builds; else
//! the OCI router leaves the resolver `None` and a KV miss 404s exactly as
//! today. Flag OFF ⇒ byte-identical to today. This is M1 of the
//! manifest-resolution keystone: flag-gated OFF, INERT until a repin turns it
//! on; no new prod behavior lands with this change.
//!
//! # Fail-open at every step
//!
//! Any upstream / verify / store failure ⇒ `Ok(None)` (the handler 404s and
//! buildkit fails open to docker.io — today's behavior). The SSRF-safe client +
//! anon-token dance + `validate_repository` guard are REUSED from
//! [`crate::routes::public_mirror`] (one audited SSRF/token implementation), and
//! the resolver never serves unverified bytes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Mutex as AsyncMutex;

use corelink_adapter_host::oci::digest::{OciDigest, OciDigestAlgo};
use corelink_adapter_host::oci::ports::{
    ManifestKvStore, ManifestResolver, PortResult, ResolvedManifest,
};
use corelink_adapter_host::oci::pull::manifest::{manifest_ct_key, manifest_key};
use corelink_core::TenantId;
use corelink_ratelimit::{
    BucketKey, InMemoryTokenBucketRateLimiter, NoOpRateLimitAuditSink, NoOpRateLimitMetrics,
    RateLimitConfig, RateLimiter,
};

use crate::adapter_cache::{MoatCache, PUBLIC_NAMESPACE};
use crate::oci_cap::TenantCapResolver;
use crate::public_base_allowlist::PublicBaseAllowlist;
use crate::routes::public_mirror::{
    validate_repository, DockerHubBlobFetcher, DockerHubManifestFetcher, UpstreamBlobFetcher,
    UpstreamManifestFetcher, UpstreamRegistryClient,
};

/// Default media type served when neither the upstream `Content-Type` nor the
/// manifest body's `mediaType` names one (defensive — real registries always
/// send one).
const DEFAULT_MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";

/// Conservative per-tenant sustained upstream-fetch rate (tokens/second). An
/// on-miss upstream fetch is EXPENSIVE (a remote round-trip + a moat write per
/// blob), so the per-tenant budget is deliberately small — a legitimate
/// `docker build` resolves a handful of distinct references, while a tenant
/// looping distinct misses to hammer the upstream is shed to `Ok(None)`
/// (fail-open, its own build then falls back to docker.io). Documented const so
/// the cap is auditable in one place.
const UPSTREAM_FETCH_RPS: u32 = 5;

/// Per-tenant burst capacity (tokens) for upstream fetches — absorbs the
/// distinct-reference fan-out of one image resolve (index → per-arch manifest →
/// a few by-digest manifest misses) before shedding.
const UPSTREAM_FETCH_BURST: u32 = 20;

/// Hard cap on a fetched manifest (bytes). Mirrors the adapter's
/// `MAX_MANIFEST_BYTES` DoS guard; the fetcher enforces it while streaming, and
/// the resolver re-checks the returned length defensively. Over-size ⇒
/// `Ok(None)`.
const MAX_PULLTHROUGH_MANIFEST_BYTES: usize = 4 * 1024 * 1024;

/// Hard cap on a single eagerly-fetched config/layer blob (bytes). The fetcher
/// already bounds a blob at its own `MIRROR_MAX_BLOB_BYTES` (1 GiB) while
/// streaming; the resolver re-checks the returned length against this same
/// ceiling defensively. Over-size ⇒ the blob (and thus the whole image resolve)
/// fails open to `Ok(None)`.
const MAX_PULLTHROUGH_BLOB_BYTES: usize = 1024 * 1024 * 1024;

/// Storage cap threaded into every `_public` moat write (M2 closure promote).
/// `Some(0)` is the genuine-unlimited shared-meter SEED — a `_public` write is
/// never charged to any tenant's per-tier quota (matches `byte_accounting.rs`'s
/// `unowned_shared_namespace` override and `public_mirror.rs`'s admin promote).
/// The GROWTH of `_public` is bounded by [`PUBLIC_GROWTH_CEILING_BYTES`], NOT by
/// this per-write cap.
const PUBLIC_CAP: Option<i64> = Some(0);

/// Anti-bloat ceiling (B4) on how many bytes THIS resolver instance may promote
/// into the shared `_public` namespace over its lifetime. A `_public` closure
/// promote is REFUSED (fail-open to the per-tenant path) once the running total
/// reaches this bound, so a hostile allowlist-bloat / mirror-amplification cannot
/// grow `_public` without limit (cross-tenant cost-contagion).
///
/// 64 GiB is generous headroom for the owner-curated base-image set (a base
/// image's full closure is hundreds of MiB) while still a hard in-process cap.
///
/// **SCOPE (durable follow-up needed):** this is an IN-PROCESS counter — it
/// bounds a single container's contribution to `_public` per boot, NOT the
/// durable cross-region `_public` row's `bytes_used`. A container reaps to zero
/// and the counter resets; N regions × M containers each get their own budget.
/// A durable cross-region ceiling would read the `_public` per-region
/// `tenant_storage_state.bytes_used` (accrued by `byte_accounting.rs`) and gate
/// on it — that read path does not exist today (the `TenantCapResolver` reads
/// tier tables, not `bytes_used`), so wiring it is NEW D1 plumbing tracked as a
/// follow-up. Until then the WRITE allowlist (owner-pinned root digests only) is
/// the primary admission bound and this in-process ceiling is the backstop.
const PUBLIC_GROWTH_CEILING_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// Per-tenant manifest + blob upstream-on-miss resolver (M1). See the module
/// doc. Everything is fail-open; the flag gates whether it is wired at all.
pub(crate) struct UpstreamManifestResolver {
    /// SSRF-safe upstream manifest fetcher (shares the ONE registry client).
    manifest_fetcher: Arc<dyn UpstreamManifestFetcher>,
    /// SSRF-safe upstream blob fetcher (shares the ONE registry client).
    blob_fetcher: Arc<dyn UpstreamBlobFetcher>,
    /// The per-tenant manifest KV to persist resolved manifests into.
    kv: Arc<dyn ManifestKvStore>,
    /// The 2-level moat to persist an image's config + layer blobs into (the
    /// tenant's OWN namespace — never `_public`).
    moat: Arc<MoatCache>,
    /// Resolves the tenant's RESOLVED per-tier storage cap for the moat write.
    /// `None` (dev/CI or an indeterminate cap) ⇒ the moat write fails CLOSED on
    /// an unseeded tenant, mirroring `finalize_upload`.
    cap_resolver: Option<Arc<dyn TenantCapResolver>>,
    /// Per-tenant token bucket bounding the upstream-fetch rate (abuse gate).
    limiter: InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>,
    /// Per-`(tenant, repo, reference)` single-flight locks so concurrent misses
    /// of the SAME reference coalesce into ONE upstream fetch (the waiters read
    /// the now-warm KV instead). Same per-key async-mutex pattern as
    /// [`crate::request_count::CachedTierResolver`].
    inflight: AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    /// M2 cross-tenant `_public` routing. When `dedup` is on
    /// ([`crate::public_flags::oci_public_dedup_enabled`], boot-read) a DIGEST
    /// reference resolves from / promotes into the shared `_public` namespace
    /// (see [`Self::resolve_on_miss`]); when off the resolver is byte-identical to
    /// M1 (per-tenant only). Shared with `OciMoatStore` at the router.
    dedup: bool,
    /// The owner-curated, digest-pinned allowlist. A DIGEST reference on this
    /// allowlist is the ROOT of a cross-tenant `_public` closure promote; the
    /// transitively-referenced children/blobs are content-addressed from that
    /// pinned root, so they need NOT be individually allowlisted.
    allowlist: PublicBaseAllowlist,
    /// Running total of bytes THIS resolver instance has promoted into `_public`
    /// (anti-bloat ceiling B4). Gated against [`Self::public_ceiling_bytes`]
    /// BEFORE each promote. See [`PUBLIC_GROWTH_CEILING_BYTES`] for the
    /// in-process-scope caveat + the durable follow-up.
    public_bytes_promoted: AtomicU64,
    /// The `_public` growth ceiling this instance enforces (default
    /// [`PUBLIC_GROWTH_CEILING_BYTES`]; tests override it to exercise the gate).
    public_ceiling_bytes: u64,
}

impl std::fmt::Debug for UpstreamManifestResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamManifestResolver")
            .field("cap_resolver", &self.cap_resolver.is_some())
            .finish_non_exhaustive()
    }
}

impl UpstreamManifestResolver {
    /// Production constructor. Builds ONE shared [`UpstreamRegistryClient`] (the
    /// single audited SSRF/token implementation) and derives both fetchers from
    /// it, plus the conservative per-tenant rate limiter. Returns `None` when
    /// the upstream client cannot be built (TLS init failure / unparseable fixed
    /// URL) — the OCI router then leaves the resolver unwired (clean 404 no-op).
    #[must_use]
    pub(crate) fn new(
        kv: Arc<dyn ManifestKvStore>,
        moat: Arc<MoatCache>,
        cap_resolver: Option<Arc<dyn TenantCapResolver>>,
        allowlist: PublicBaseAllowlist,
    ) -> Option<Self> {
        let client = Arc::new(UpstreamRegistryClient::new().ok()?);
        let manifest_fetcher: Arc<dyn UpstreamManifestFetcher> =
            Arc::new(DockerHubManifestFetcher::from_client(Arc::clone(&client)));
        let blob_fetcher: Arc<dyn UpstreamBlobFetcher> =
            Arc::new(DockerHubBlobFetcher::from_client(client));
        // Read the SAME boot flag `OciMoatStore` reads — cross-tenant `_public`
        // routing is on only when the F3.2 dedup flag is `"1"` at boot.
        Some(Self::with_parts(
            manifest_fetcher,
            blob_fetcher,
            kv,
            moat,
            cap_resolver,
            Self::default_limiter(),
            allowlist,
            crate::public_flags::oci_public_dedup_enabled(),
        ))
    }

    /// Construct from explicit parts (the wiring seam tests inject fakes at). The
    /// `allowlist` + `dedup` are passed explicitly here so a test can exercise the
    /// `_public` closure promote without the shipped (alpine-only) baked manifest.
    // Each argument is a distinct injected collaborator (two fetchers, the KV, the
    // moat, the cap resolver, the limiter) plus the two M2 `_public` knobs
    // (allowlist + dedup) — bundling them into a params struct adds indirection
    // without removing real coupling, exactly as `routes::oci::router` documents.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_parts(
        manifest_fetcher: Arc<dyn UpstreamManifestFetcher>,
        blob_fetcher: Arc<dyn UpstreamBlobFetcher>,
        kv: Arc<dyn ManifestKvStore>,
        moat: Arc<MoatCache>,
        cap_resolver: Option<Arc<dyn TenantCapResolver>>,
        limiter: InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>,
        allowlist: PublicBaseAllowlist,
        dedup: bool,
    ) -> Self {
        Self {
            manifest_fetcher,
            blob_fetcher,
            kv,
            moat,
            cap_resolver,
            limiter,
            inflight: AsyncMutex::new(HashMap::new()),
            dedup,
            allowlist,
            public_bytes_promoted: AtomicU64::new(0),
            public_ceiling_bytes: PUBLIC_GROWTH_CEILING_BYTES,
        }
    }

    /// The conservative production per-tenant upstream-fetch limiter
    /// ([`UPSTREAM_FETCH_RPS`] / [`UPSTREAM_FETCH_BURST`]), with the bounded
    /// NoOp sinks (O(1) memory) exactly like [`crate::routes::ratelimit_layer`].
    #[must_use]
    fn default_limiter(
    ) -> InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics> {
        let config = RateLimitConfig::with_overrides(
            UPSTREAM_FETCH_RPS,
            UPSTREAM_FETCH_BURST,
            corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS,
            corelink_ratelimit::RETRY_AFTER_HARD_CEILING_SECS,
            corelink_ratelimit::RETRY_AFTER_CANCELED_TENANT_SECS,
        )
        .unwrap_or_else(RateLimitConfig::canonical);
        InMemoryTokenBucketRateLimiter::new(
            Arc::new(NoOpRateLimitAuditSink::new()),
            Arc::new(NoOpRateLimitMetrics::new()),
            config,
        )
    }

    /// Charge one token against `tenant`'s upstream-fetch bucket. `true` ⇒
    /// proceed; `false` ⇒ over-limit (the caller returns `Ok(None)`, fail-open).
    /// The limiter's own internal fault fails OPEN (proceeds) — availability of
    /// the fetch path over a perfectly-enforced cap, matching the data plane's
    /// rate-limit posture.
    fn rate_limit_ok(&self, tenant: &TenantId) -> bool {
        let now_ms = now_unix_ms();
        let uuid = *tenant.as_uuid();
        match self
            .limiter
            .try_acquire(uuid, BucketKey::per_tenant(uuid), 1, now_ms)
        {
            Ok(outcome) => outcome.decision.is_allow(),
            Err(_) => true,
        }
    }

    /// The per-`(tenant, repo, reference)` single-flight lock, created on demand.
    async fn inflight_lock(&self, key: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.inflight.lock().await;
        // Drop only UNCONTENDED locks so the map does not grow without bound
        // under reference churn (the map's own Arc is the sole strong ref).
        if map.len() >= INFLIGHT_MAP_CAP && !map.contains_key(key) {
            map.retain(|_, l| Arc::strong_count(l) > 1);
        }
        Arc::clone(
            map.entry(key.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    /// Serve a manifest already present in the per-tenant KV (the single-flight
    /// coalesced path: a peer flight populated it while we waited). `None` ⇒ not
    /// in KV. Reconstructs the `Docker-Content-Digest` by recomputing sha256 and
    /// the `Content-Type` from the companion ct slot.
    async fn serve_from_kv(
        &self,
        tenant: &TenantId,
        repo: &str,
        reference: &str,
    ) -> Option<ResolvedManifest> {
        let body = self
            .kv
            .get(tenant, &manifest_key(repo, reference))
            .await
            .ok()??;
        let digest = OciDigest::compute(OciDigestAlgo::Sha256, &body)
            .ok()?
            .to_wire();
        let content_type = self
            .kv
            .get(tenant, &manifest_ct_key(repo, reference))
            .await
            .ok()
            .flatten()
            .and_then(|b| String::from_utf8(b.to_vec()).ok())
            .unwrap_or_else(|| DEFAULT_MANIFEST_MEDIA_TYPE.to_owned());
        Some(ResolvedManifest {
            bytes: body,
            content_type,
            digest,
        })
    }

    /// Resolve the tenant's RESOLVED per-tier storage cap for a moat write.
    /// `None` (no resolver, or an indeterminate cap) ⇒ the moat write fails
    /// CLOSED on an unseeded tenant, mirroring `finalize_upload`.
    async fn resolve_cap(&self, tenant_text: &str) -> Option<i64> {
        match self.cap_resolver.as_ref() {
            Some(r) => r.resolve_storage_cap(tenant_text).await,
            None => None,
        }
    }

    /// Fetch + digest-verify + persist one blob (config or layer) by its
    /// `sha256:` digest into `namespace` (the tenant's OWN namespace for the M1
    /// per-tenant path; [`PUBLIC_NAMESPACE`] for the M2 closure promote — the
    /// SAME fetch→verify→put either way). Fail-open: any fetch/verify/store
    /// failure ⇒ `false` (the image resolve then aborts to `Ok(None)`). NO
    /// allowlist gate — the bytes are content-addressed, and for `_public` the
    /// admission control is the allowlisted ROOT the closure descends from.
    async fn persist_blob(
        &self,
        namespace: &str,
        repo: &str,
        digest_wire: &str,
        cap: Option<i64>,
    ) -> bool {
        // Parse the declared digest first (rejects a malformed descriptor before
        // any fetch).
        let Ok(parsed) = OciDigest::parse(digest_wire) else {
            return false;
        };
        let canonical = parsed.to_wire();
        let Ok(bytes) = self.blob_fetcher.fetch_blob(repo, &canonical).await else {
            return false;
        };
        if bytes.len() > MAX_PULLTHROUGH_BLOB_BYTES {
            return false;
        }
        // MANDATORY write-time verify (fail-CLOSED): a digest-lie never reaches
        // the moat.
        if parsed.verify_against_bytes(&bytes).is_err() {
            return false;
        }
        let len = bytes.len();
        let ok = self
            .moat
            .put(namespace, &canonical, bytes, cap)
            .await
            .is_ok();
        // Account bytes that actually landed in `_public` toward the anti-bloat
        // ceiling (B4). Blobs dominate a closure's size, so counting them here
        // (not just the KB-sized manifests) is what makes the ceiling meaningful.
        if ok && namespace == PUBLIC_NAMESPACE {
            self.note_public_bytes(len);
        }
        ok
    }

    /// Add `n` bytes to the running `_public`-promoted total (anti-bloat
    /// ceiling B4), saturating.
    fn note_public_bytes(&self, n: usize) {
        self.public_bytes_promoted
            .fetch_add(n as u64, Ordering::Relaxed);
    }

    /// Is this instance still under its `_public` growth ceiling (B4)? Checked
    /// BEFORE a closure promote; `false` ⇒ refuse the promote (fail-open to the
    /// per-tenant path). READS from `_public` are never ceiling-gated.
    fn public_under_ceiling(&self) -> bool {
        self.public_bytes_promoted.load(Ordering::Relaxed) < self.public_ceiling_bytes
    }

    /// Serve a manifest already present in the shared `_public` namespace (the
    /// cross-tenant existence HIT — no upstream call). `None` ⇒ not in `_public`
    /// (or revoked: [`MoatCache::get`] applies the `public_blocklist` filter for
    /// `_public`, so a revoked digest reads as a MISS and falls through).
    /// Content-type is inferred from the manifest body's `mediaType`; the digest
    /// is the (canonical) reference itself, since a `_public` manifest is only
    /// ever stored under a by-digest key.
    async fn serve_from_public(&self, digest_wire: &str) -> Option<ResolvedManifest> {
        let body = self.moat.get(PUBLIC_NAMESPACE, digest_wire).await.ok()??;
        let body = Bytes::from(body);
        let json = serde_json::from_slice::<serde_json::Value>(&body).ok()?;
        let content_type = manifest_media_type(None, &json);
        Some(ResolvedManifest {
            bytes: body,
            content_type,
            digest: digest_wire.to_owned(),
        })
    }

    /// Promote the FULL TRANSITIVE CLOSURE of an allowlisted-root DIGEST into the
    /// shared `_public` namespace, then serve the just-promoted manifest. Returns
    /// `None` on ANY failure (fail-open: the caller falls through to the M1
    /// per-tenant path) — and, critically, the root index/manifest is stored LAST
    /// (only after every child + blob verified + stored), so a mid-closure
    /// failure never leaves a `_public` root that a later existence read would
    /// serve as a COMPLETE closure. Orphaned children/blobs left behind are
    /// individually digest-verified + content-addressed (harmless; a re-promote
    /// is idempotent).
    ///
    /// Every descriptor is digest-verified against fetched bytes BEFORE its
    /// `_public` write (fail-CLOSED). `repo` is gated through the SAME
    /// origin-escape guard the mirror uses before any upstream fetch.
    async fn promote_public_closure(
        &self,
        repo: &str,
        root_digest: &str,
    ) -> Option<ResolvedManifest> {
        // Anti-bloat ceiling (B4): refuse once this instance has promoted enough
        // into `_public`. Fail-open — the tenant still gets a per-tenant resolve.
        if !self.public_under_ceiling() {
            return None;
        }
        if validate_repository(repo).is_err() {
            return None;
        }
        // Fetch + verify the ROOT by its immutable digest.
        let parsed_root = OciDigest::parse(root_digest).ok()?;
        let fetched = self
            .manifest_fetcher
            .fetch_manifest(repo, root_digest)
            .await
            .ok()?;
        if fetched.bytes.len() > MAX_PULLTHROUGH_MANIFEST_BYTES {
            return None;
        }
        // Fail-CLOSED: the fetched bytes MUST hash to the allowlisted digest.
        parsed_root.verify_against_bytes(&fetched.bytes).ok()?;
        let json = serde_json::from_slice::<serde_json::Value>(&fetched.bytes).ok()?;
        let content_type = manifest_media_type(fetched.content_type.as_deref(), &json);

        if let Some(children) = index_child_digests(&json) {
            // INDEX: promote each per-arch child (its config + layers + the child
            // manifest) FIRST; only if ALL succeed store the index bytes LAST.
            for child in &children {
                if !self.promote_public_child_manifest(repo, child).await {
                    return None;
                }
            }
            if self
                .moat
                .put(
                    PUBLIC_NAMESPACE,
                    root_digest,
                    fetched.bytes.clone(),
                    PUBLIC_CAP,
                )
                .await
                .is_err()
            {
                return None;
            }
            self.note_public_bytes(fetched.bytes.len());
        } else if let Some(blob_digests) = image_blob_digests(&json) {
            // IMAGE manifest: promote config + layers FIRST, store the manifest
            // LAST (so a partial blob failure never leaves a served-as-complete
            // manifest in `_public`).
            for d in &blob_digests {
                if !self
                    .persist_blob(PUBLIC_NAMESPACE, repo, d, PUBLIC_CAP)
                    .await
                {
                    return None;
                }
            }
            if self
                .moat
                .put(
                    PUBLIC_NAMESPACE,
                    root_digest,
                    fetched.bytes.clone(),
                    PUBLIC_CAP,
                )
                .await
                .is_err()
            {
                return None;
            }
            self.note_public_bytes(fetched.bytes.len());
        } else {
            // Neither an index nor an image manifest ⇒ nothing to promote.
            return None;
        }

        Some(ResolvedManifest {
            bytes: Bytes::from(fetched.bytes),
            content_type,
            digest: root_digest.to_owned(),
        })
    }

    /// Promote ONE per-arch child (an image manifest) of an index into `_public`:
    /// fetch it BY digest, digest-verify (fail-CLOSED), promote its config +
    /// layer blobs, then store the child manifest bytes LAST. `false` on any
    /// failure (the caller aborts the whole closure promote, fail-open).
    async fn promote_public_child_manifest(&self, repo: &str, child_digest: &str) -> bool {
        let Ok(parsed) = OciDigest::parse(child_digest) else {
            return false;
        };
        let canonical = parsed.to_wire();
        let Ok(fetched) = self.manifest_fetcher.fetch_manifest(repo, &canonical).await else {
            return false;
        };
        if fetched.bytes.len() > MAX_PULLTHROUGH_MANIFEST_BYTES {
            return false;
        }
        if parsed.verify_against_bytes(&fetched.bytes).is_err() {
            return false;
        }
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&fetched.bytes) else {
            return false;
        };
        // A child of an index is an IMAGE manifest (config + layers).
        let Some(blob_digests) = image_blob_digests(&json) else {
            return false;
        };
        for d in &blob_digests {
            if !self
                .persist_blob(PUBLIC_NAMESPACE, repo, d, PUBLIC_CAP)
                .await
            {
                return false;
            }
        }
        let len = fetched.bytes.len();
        let ok = self
            .moat
            .put(PUBLIC_NAMESPACE, &canonical, fetched.bytes, PUBLIC_CAP)
            .await
            .is_ok();
        if ok {
            self.note_public_bytes(len);
        }
        ok
    }
}

/// Upper bound on distinct single-flight keys held at once (bounded memory under
/// reference churn; ~tens of bytes per entry).
const INFLIGHT_MAP_CAP: usize = 50_000;

/// Return the current wall-clock time in ms since the Unix epoch (saturating).
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// Choose the media type to serve: prefer the upstream `Content-Type`, else the
/// manifest body's `mediaType`, else the canonical OCI v1 default.
fn manifest_media_type(fetched_ct: Option<&str>, body: &serde_json::Value) -> String {
    if let Some(ct) = fetched_ct {
        let trimmed = ct.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    if let Some(mt) = body.get("mediaType").and_then(serde_json::Value::as_str) {
        if !mt.is_empty() {
            return mt.to_owned();
        }
    }
    DEFAULT_MANIFEST_MEDIA_TYPE.to_owned()
}

/// Collect a JSON image manifest's `config.digest` + every `layers[].digest`,
/// or `None` when the doc is NOT an image manifest (an index/list carries
/// `manifests`, not `config`+`layers`). A returned `Some(vec)` means "this is an
/// image; eagerly resolve these blobs".
fn image_blob_digests(body: &serde_json::Value) -> Option<Vec<String>> {
    let config = body.get("config")?;
    let layers = body.get("layers")?.as_array()?;
    let mut digests = Vec::with_capacity(layers.len() + 1);
    let config_digest = config.get("digest").and_then(serde_json::Value::as_str)?;
    digests.push(config_digest.to_owned());
    for layer in layers {
        let d = layer.get("digest").and_then(serde_json::Value::as_str)?;
        digests.push(d.to_owned());
    }
    Some(digests)
}

/// Collect an OCI index / manifest-list's per-arch child manifest digests,
/// keeping ONLY `linux` children (`platform.os == "linux"`), or `None` when the
/// doc is NOT an index (an image manifest carries `config`+`layers`, not
/// `manifests`). A returned `Some(vec)` means "this is an index; recurse-promote
/// these children".
///
/// The filter is `os == "linux"`, NOT merely "not `unknown`": besides the
/// `unknown/unknown` attestation entries (buildkit SBOM / provenance), a real
/// base image's index also carries non-linux runnable entries (e.g. `golang`'s
/// `windows/amd64` child). CoreLink runners are linux, so promoting a windows
/// image manifest + its layers into `_public` is fetch/storage we would never
/// serve — skip everything but linux. A child kept this way must still carry a
/// `digest`; a linux child missing one is a malformed index ⇒ `None`
/// (fail-closed, nothing promoted).
fn index_child_digests(body: &serde_json::Value) -> Option<Vec<String>> {
    let manifests = body.get("manifests")?.as_array()?;
    let mut out = Vec::with_capacity(manifests.len());
    for m in manifests {
        let os = m
            .get("platform")
            .and_then(|p| p.get("os"))
            .and_then(serde_json::Value::as_str);
        if os != Some("linux") {
            // Skip windows / unknown-attestation / any non-linux child — never
            // served on a linux runner, so never worth promoting to `_public`.
            continue;
        }
        let d = m.get("digest").and_then(serde_json::Value::as_str)?;
        out.push(d.to_owned());
    }
    Some(out)
}

#[async_trait]
impl ManifestResolver for UpstreamManifestResolver {
    async fn resolve_on_miss(
        &self,
        tenant: &TenantId,
        repo: &str,
        reference: &str,
    ) -> PortResult<Option<ResolvedManifest>> {
        // 1. Rate-limit the per-tenant upstream-fetch rate. Over-limit ⇒
        //    fail-open (Ok(None)); the tenant's build falls back to docker.io.
        if !self.rate_limit_ok(tenant) {
            return Ok(None);
        }

        // 2. Single-flight on (tenant, repo, reference) so concurrent misses of
        //    the SAME reference coalesce into ONE upstream fetch.
        let sf_key = format!("{}|{repo}|{reference}", tenant.to_canonical_text());
        let lock = self.inflight_lock(&sf_key).await;
        let _guard = lock.lock().await;
        // A peer flight may have populated KV while we waited — serve that
        // instead of fetching again.
        if let Some(rm) = self.serve_from_kv(tenant, repo, reference).await {
            return Ok(Some(rm));
        }

        // 2.5 CROSS-TENANT `_public` decision (M2), BEFORE the per-tenant path.
        //     Only a DIGEST reference is `_public`-eligible — a TAG is mutable
        //     (first-writer poisoning) so it ALWAYS takes the M1 per-tenant path
        //     and never reads or writes `_public`.
        if self.dedup {
            if let Ok(parsed_ref) = OciDigest::parse(reference) {
                let canonical_ref = parsed_ref.to_wire();
                // `_public` READ (existence): a cross-tenant HIT needs NO upstream
                // call — serve the shared copy (byte-identical, content-addressed;
                // revocation still filters inside `MoatCache::get`). This serves a
                // transitively-promoted child that is NOT individually allowlisted.
                if let Some(rm) = self.serve_from_public(&canonical_ref).await {
                    return Ok(Some(rm));
                }
                // `_public` WRITE: only an allowlisted ROOT digest promotes its
                // full transitive closure into `_public`. Fail-open — a failed
                // promote falls through to the M1 per-tenant path below.
                if self.allowlist.is_allowlisted(&canonical_ref) {
                    if let Some(rm) = self.promote_public_closure(repo, &canonical_ref).await {
                        return Ok(Some(rm));
                    }
                }
            }
        }

        // 3. Gate the repo through the SAME origin-escape guard the mirror uses
        //    (before any upstream fetch).
        if validate_repository(repo).is_err() {
            return Ok(None);
        }

        // 4. Fetch the manifest from the FIXED upstream (multi-Accept). The
        //    fetcher caps the body; over-size ⇒ Err ⇒ fail-open.
        let fetched = match self.manifest_fetcher.fetch_manifest(repo, reference).await {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };
        if fetched.bytes.len() > MAX_PULLTHROUGH_MANIFEST_BYTES {
            return Ok(None);
        }

        // 5. Verify / compute the digest. A `sha256:`/`sha512:` reference MUST
        //    hash to the fetched bytes (never serve unverified); a tag reference
        //    computes the digest.
        let digest_wire = match OciDigest::parse(reference) {
            Ok(declared) => {
                if declared.verify_against_bytes(&fetched.bytes).is_err() {
                    // Digest-lie (or an unsupported-algo reference) ⇒ fail-open,
                    // nothing stored.
                    return Ok(None);
                }
                declared.to_wire()
            }
            Err(_) => match OciDigest::compute(OciDigestAlgo::Sha256, &fetched.bytes) {
                Ok(d) => d.to_wire(),
                Err(_) => return Ok(None),
            },
        };

        // Defensive cross-check: the resolver always TRUSTS its own recomputed
        // digest (never the upstream's claimed one), but a disagreement with the
        // upstream `Docker-Content-Digest` is worth an observability breadcrumb.
        if let Some(upstream_digest) = fetched.docker_content_digest.as_deref() {
            if upstream_digest != digest_wire {
                tracing::debug!(
                    repo,
                    reference,
                    upstream = upstream_digest,
                    computed = %digest_wire,
                    "oci upstream-on-miss: Docker-Content-Digest disagrees with computed digest (trusting computed)"
                );
            }
        }

        // 6. Parse the doc + decide image vs index.
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&fetched.bytes) else {
            return Ok(None);
        };
        let content_type = manifest_media_type(fetched.content_type.as_deref(), &json);
        let tenant_ns = tenant.to_canonical_text();

        // For an IMAGE manifest, eagerly fetch + verify + persist its config +
        // layer blobs into the tenant's OWN moat FIRST — so a partial failure
        // never leaves a served manifest whose by-digest blob GETs (which carry
        // no repo) would 404. (Ordering note: the design lists manifest-then-
        // blobs; persisting blobs first strictly strengthens the fail-open
        // guarantee without changing observable success behavior.) An INDEX is
        // stored as-is with NO recursion — buildkit GETs the per-arch manifest
        // by digest as its own on-miss (M1 does not walk the index).
        if let Some(digests) = image_blob_digests(&json) {
            let cap = self.resolve_cap(&tenant_ns).await;
            for d in &digests {
                if !self.persist_blob(&tenant_ns, repo, d, cap).await {
                    return Ok(None);
                }
            }
        }

        // 7. Persist the manifest into the per-tenant KV. For a TAG, also write
        //    the by-digest slot so a later by-digest GET hits; write the
        //    companion content-type slot for each stored reference.
        let body = Bytes::from(fetched.bytes);
        if !self
            .persist_manifest(tenant, repo, reference, &digest_wire, &content_type, &body)
            .await
        {
            return Ok(None);
        }

        // 8. Serve.
        Ok(Some(ResolvedManifest {
            bytes: body,
            content_type,
            digest: digest_wire,
        }))
    }
}

impl UpstreamManifestResolver {
    /// Persist the manifest bytes + companion content-type slot under the
    /// request `reference` and, when `reference` is a tag (differs from the
    /// canonical digest), ALSO under the by-digest slot. Fail-open: any KV put
    /// failure ⇒ `false`. Kept out of the trait method so the persistence policy
    /// reads in one place.
    async fn persist_manifest(
        &self,
        tenant: &TenantId,
        repo: &str,
        reference: &str,
        digest_wire: &str,
        content_type: &str,
        body: &Bytes,
    ) -> bool {
        // The requested reference (tag or digest).
        if self
            .kv
            .put(tenant, &manifest_key(repo, reference), body.clone())
            .await
            .is_err()
        {
            return false;
        }
        if self
            .kv
            .put(
                tenant,
                &manifest_ct_key(repo, reference),
                Bytes::from(content_type.to_owned()),
            )
            .await
            .is_err()
        {
            return false;
        }
        // For a tag, also index by the resolved digest so a later
        // `GET manifests/<digest>` hits the per-tenant KV directly.
        if reference != digest_wire {
            if self
                .kv
                .put(tenant, &manifest_key(repo, digest_wire), body.clone())
                .await
                .is_err()
            {
                return false;
            }
            if self
                .kv
                .put(
                    tenant,
                    &manifest_ct_key(repo, digest_wire),
                    Bytes::from(content_type.to_owned()),
                )
                .await
                .is_err()
            {
                return false;
            }
        }
        true
    }
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use corelink_handler_cas::{
        CasHandlerError, CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler,
        CasWriteRequest, CasWriteResponse,
    };

    use crate::adapter_cache::{UrlMapStore, PUBLIC_NAMESPACE};
    use crate::routes::public_mirror::FetchedManifest;

    use super::*;

    const SERVICE_PRINCIPAL: &str = "oci-pullthrough-test";

    /// Canonical `sha256:<hex>` of `bytes`.
    fn sha256_wire(bytes: &[u8]) -> String {
        OciDigest::compute(OciDigestAlgo::Sha256, bytes)
            .expect("sha256 compute")
            .to_wire()
    }

    // ── Fakes ────────────────────────────────────────────────────────────────

    /// Fake manifest fetcher: returns canned `FetchedManifest`s keyed by
    /// `(repo, reference)`, counting every call.
    #[derive(Debug, Default)]
    struct FakeManifestFetcher {
        by_ref: Mutex<HashMap<(String, String), FetchedManifest>>,
        calls: AtomicUsize,
    }
    impl FakeManifestFetcher {
        fn insert(&self, repo: &str, reference: &str, m: FetchedManifest) {
            self.by_ref
                .lock()
                .unwrap()
                .insert((repo.to_owned(), reference.to_owned()), m);
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl UpstreamManifestFetcher for FakeManifestFetcher {
        async fn fetch_manifest(
            &self,
            repository: &str,
            reference: &str,
        ) -> Result<FetchedManifest, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.by_ref
                .lock()
                .unwrap()
                .get(&(repository.to_owned(), reference.to_owned()))
                .cloned()
                .ok_or_else(|| "not found".to_owned())
        }
    }

    /// Fake blob fetcher: returns canned bytes keyed by `(repo, digest)`,
    /// counting every call.
    #[derive(Debug, Default)]
    struct FakeBlobFetcher {
        by_digest: Mutex<HashMap<(String, String), Vec<u8>>>,
        calls: AtomicUsize,
    }
    impl FakeBlobFetcher {
        fn insert(&self, repo: &str, digest: &str, bytes: Vec<u8>) {
            self.by_digest
                .lock()
                .unwrap()
                .insert((repo.to_owned(), digest.to_owned()), bytes);
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl UpstreamBlobFetcher for FakeBlobFetcher {
        async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.by_digest
                .lock()
                .unwrap()
                .get(&(repository.to_owned(), digest.to_owned()))
                .cloned()
                .ok_or_else(|| "blob not found".to_owned())
        }
    }

    /// In-memory `ManifestKvStore` keyed by `(tenant-text, key)`.
    #[derive(Debug, Default)]
    struct FakeKv(Mutex<HashMap<(String, String), Bytes>>);
    #[async_trait]
    impl ManifestKvStore for FakeKv {
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

    /// In-memory url→content-hash map recording `(namespace, url_hash)`.
    #[derive(Debug, Default)]
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

    /// Non-verifying in-memory CAS keyed by `(namespace, claimed_hash)`.
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

    /// Deterministic non-crypto content hasher for the moat (orthogonal to the
    /// OCI sha256 verify).
    fn fake_hash(bytes: &[u8]) -> String {
        format!("h{:08x}", bytes.iter().map(|b| u32::from(*b)).sum::<u32>())
    }

    fn moat(cas: Arc<RecordingCas>, map: Arc<FakeMap>) -> Arc<MoatCache> {
        Arc::new(MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            SERVICE_PRINCIPAL,
        ))
    }

    /// A generous limiter (burst 1000) so rate-limiting never interferes with a
    /// functional test (the conservative production limiter is exercised by the
    /// flag/const wiring, not these behavioral tests).
    fn generous_limiter(
    ) -> InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics> {
        let config = RateLimitConfig::with_overrides(
            1000,
            1000,
            corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS,
            corelink_ratelimit::RETRY_AFTER_HARD_CEILING_SECS,
            corelink_ratelimit::RETRY_AFTER_CANCELED_TENANT_SECS,
        )
        .unwrap();
        InMemoryTokenBucketRateLimiter::new(
            Arc::new(NoOpRateLimitAuditSink::new()),
            Arc::new(NoOpRateLimitMetrics::new()),
            config,
        )
    }

    struct Rig {
        resolver: UpstreamManifestResolver,
        manifest_fetcher: Arc<FakeManifestFetcher>,
        blob_fetcher: Arc<FakeBlobFetcher>,
        kv: Arc<FakeKv>,
        cas: Arc<RecordingCas>,
        map: Arc<FakeMap>,
    }

    /// Default rig: dedup OFF, empty (deny-all) allowlist → the M1 per-tenant-only
    /// behavior. Every M1 test uses this, so they double as the "dedup OFF ⇒
    /// byte-identical to M1, no `_public` writes/reads" proof.
    fn rig() -> Rig {
        rig_full(generous_limiter(), PublicBaseAllowlist::default(), false)
    }

    /// Build an allowlist from a set of already-canonical `sha256:` digests.
    fn allowlist_of(digests: &[&str]) -> PublicBaseAllowlist {
        let manifest = digests.iter().map(|d| format!("{d}\n")).collect::<String>();
        PublicBaseAllowlist::parse(&manifest).expect("hermetic allowlist parses")
    }

    fn rig_full(
        limiter: InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>,
        allowlist: PublicBaseAllowlist,
        dedup: bool,
    ) -> Rig {
        let manifest_fetcher = Arc::new(FakeManifestFetcher::default());
        let blob_fetcher = Arc::new(FakeBlobFetcher::default());
        let kv = Arc::new(FakeKv::default());
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let resolver = UpstreamManifestResolver::with_parts(
            Arc::clone(&manifest_fetcher) as Arc<dyn UpstreamManifestFetcher>,
            Arc::clone(&blob_fetcher) as Arc<dyn UpstreamBlobFetcher>,
            Arc::clone(&kv) as Arc<dyn ManifestKvStore>,
            moat(Arc::clone(&cas), Arc::clone(&map)),
            None, // cap resolver: None → moat.put gets None; RecordingCas ignores it.
            limiter,
            allowlist,
            dedup,
        );
        Rig {
            resolver,
            manifest_fetcher,
            blob_fetcher,
            kv,
            cas,
            map,
        }
    }

    fn tenant(n: u128) -> TenantId {
        TenantId::from_uuid(uuid::Uuid::from_u128(n))
    }

    /// Build a minimal image manifest JSON with the given config + layer blob
    /// digests, returning the serialized bytes.
    fn image_manifest_bytes(config_digest: &str, layer_digests: &[&str]) -> Vec<u8> {
        let layers: Vec<serde_json::Value> = layer_digests
            .iter()
            .map(|d| {
                serde_json::json!({
                    "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                    "digest": d,
                    "size": 1
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": config_digest,
                "size": 1
            },
            "layers": layers
        }))
        .unwrap()
    }

    fn index_bytes(child_digest: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [{
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": child_digest,
                "size": 1,
                "platform": { "os": "linux", "architecture": "amd64" }
            }]
        }))
        .unwrap()
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn image_manifest_miss_caches_manifest_and_blobs() {
        let r = rig();
        let t = tenant(1);
        let repo = "library/alpine";
        let config_bytes = b"config-object-bytes".to_vec();
        let layer_bytes = b"layer-tar-gzip-bytes".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);
        r.manifest_fetcher.insert(
            repo,
            "3.20",
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: Some("application/vnd.oci.image.manifest.v1+json".to_owned()),
                docker_content_digest: Some(manifest_digest.clone()),
            },
        );
        r.blob_fetcher
            .insert(repo, &config_digest, config_bytes.clone());
        r.blob_fetcher
            .insert(repo, &layer_digest, layer_bytes.clone());

        let out = r
            .resolver
            .resolve_on_miss(&t, repo, "3.20")
            .await
            .expect("no internal error")
            .expect("resolves the image manifest");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(out.bytes.as_ref(), manifest.as_slice());

        // Manifest stored in per-tenant KV under BOTH the tag + the digest slot.
        let kv = r.kv.0.lock().unwrap();
        assert!(kv.contains_key(&(t.to_canonical_text(), manifest_key(repo, "3.20"))));
        assert!(kv.contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
        drop(kv);
        // Config + layer blobs stored in the PER-TENANT moat namespace.
        let cas = r.cas.0.lock().unwrap();
        assert_eq!(
            cas.get(&(t.to_canonical_text(), fake_hash(&config_bytes))),
            Some(&config_bytes)
        );
        assert_eq!(
            cas.get(&(t.to_canonical_text(), fake_hash(&layer_bytes))),
            Some(&layer_bytes)
        );
        // Nothing landed in `_public`.
        assert!(!cas.keys().any(|(ns, _)| ns == PUBLIC_NAMESPACE));
    }

    #[tokio::test]
    async fn second_get_hits_kv_no_second_upstream_fetch() {
        let r = rig();
        let t = tenant(2);
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let _ = r
            .resolver
            .resolve_on_miss(&t, repo, "latest")
            .await
            .unwrap();
        assert_eq!(r.manifest_fetcher.call_count(), 1);
        // A 2nd resolve (a peer/racing miss) coalesces via the KV re-check — no
        // 2nd upstream fetch.
        let again = r
            .resolver
            .resolve_on_miss(&t, repo, "latest")
            .await
            .unwrap()
            .expect("served from KV");
        assert_eq!(again.bytes.as_ref(), manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "a warm KV re-check must not re-fetch upstream"
        );
    }

    #[tokio::test]
    async fn digest_reference_mismatch_stores_nothing() {
        let r = rig();
        let t = tenant(3);
        let repo = "library/alpine";
        // The reference is a digest of SOME bytes, but the fetcher returns
        // DIFFERENT bytes → verify fails → Ok(None), nothing stored.
        let honest = b"the-real-manifest".to_vec();
        let ref_digest = sha256_wire(&honest);
        r.manifest_fetcher.insert(
            repo,
            &ref_digest,
            FetchedManifest {
                bytes: b"tampered-manifest".to_vec(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &ref_digest)
            .await
            .unwrap();
        assert!(out.is_none(), "a digest-lie must not resolve");
        assert!(
            r.kv.0.lock().unwrap().is_empty(),
            "nothing stored on mismatch"
        );
        assert!(r.cas.0.lock().unwrap().is_empty());
        assert_eq!(
            r.blob_fetcher.call_count(),
            0,
            "no blob fetch on a bad manifest"
        );
    }

    #[tokio::test]
    async fn index_miss_stores_index_without_blob_fetches() {
        let r = rig();
        let t = tenant(4);
        let repo = "library/debian";
        let child = sha256_wire(b"per-arch-manifest");
        let index = index_bytes(&child);
        let index_digest = sha256_wire(&index);
        r.manifest_fetcher.insert(
            repo,
            "12",
            FetchedManifest {
                bytes: index.clone(),
                content_type: Some("application/vnd.oci.image.index.v1+json".to_owned()),
                docker_content_digest: Some(index_digest.clone()),
            },
        );
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, "12")
            .await
            .unwrap()
            .expect("index resolves");
        assert_eq!(out.digest, index_digest);
        assert_eq!(out.content_type, "application/vnd.oci.image.index.v1+json");
        // Index stored; NO blob fetches, NO recursion into the per-arch child.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, "12"))));
        assert_eq!(
            r.blob_fetcher.call_count(),
            0,
            "an index must not fetch blobs"
        );
        assert!(r.cas.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn tenant_a_resolution_is_not_visible_to_tenant_b() {
        let r = rig();
        let ta = tenant(10);
        let tb = tenant(11);
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let _ = r
            .resolver
            .resolve_on_miss(&ta, repo, "latest")
            .await
            .unwrap();
        // Tenant A's manifest is in A's KV, NOT B's.
        let kv = r.kv.0.lock().unwrap();
        assert!(kv.contains_key(&(ta.to_canonical_text(), manifest_key(repo, "latest"))));
        assert!(!kv.contains_key(&(tb.to_canonical_text(), manifest_key(repo, "latest"))));
    }

    #[tokio::test]
    async fn oversize_manifest_fails_open() {
        let r = rig();
        let t = tenant(5);
        let repo = "library/alpine";
        // A manifest body just over the cap → Ok(None), nothing stored.
        let big = vec![b'x'; MAX_PULLTHROUGH_MANIFEST_BYTES + 1];
        r.manifest_fetcher.insert(
            repo,
            "huge",
            FetchedManifest {
                bytes: big,
                content_type: None,
                docker_content_digest: None,
            },
        );
        let out = r.resolver.resolve_on_miss(&t, repo, "huge").await.unwrap();
        assert!(out.is_none(), "over-size manifest must fail open");
        assert!(r.kv.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn oversize_blob_fails_open() {
        let r = rig();
        let t = tenant(6);
        let repo = "library/alpine";
        let big_layer = vec![b'y'; MAX_PULLTHROUGH_BLOB_BYTES + 1];
        let config_bytes = b"cfg".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&big_layer);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, big_layer);

        let out = r
            .resolver
            .resolve_on_miss(&t, repo, "latest")
            .await
            .unwrap();
        assert!(
            out.is_none(),
            "an over-size layer blob must fail the resolve open"
        );
        // The manifest was NOT persisted (blobs are persisted first; a failure
        // aborts before the manifest write).
        assert!(
            r.kv.0.lock().unwrap().is_empty(),
            "no manifest stored when a blob is over-size"
        );
    }

    #[tokio::test]
    async fn concurrent_identical_misses_fetch_upstream_once() {
        let r = Arc::new(rig());
        let t = tenant(7);
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..24 {
            let rr = Arc::clone(&r);
            set.spawn(async move { rr.resolver.resolve_on_miss(&t, repo, "latest").await });
        }
        while let Some(res) = set.join_next().await {
            assert!(
                res.unwrap().unwrap().is_some(),
                "every coalesced miss resolves"
            );
        }
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "concurrent identical misses must coalesce into ONE upstream fetch"
        );
    }

    // ── M2 `_public` cross-tenant tests ──────────────────────────────────────

    /// True iff `(PUBLIC_NAMESPACE, url_hash)` has a map row (the digest was
    /// promoted into `_public`).
    fn in_public(rig: &Rig, url_hash: &str) -> bool {
        rig.map
            .0
            .lock()
            .unwrap()
            .contains_key(&(PUBLIC_NAMESPACE.to_owned(), url_hash.to_owned()))
    }

    /// Count of distinct `_public` map rows (promoted manifests + blobs).
    fn public_row_count(rig: &Rig) -> usize {
        rig.map
            .0
            .lock()
            .unwrap()
            .keys()
            .filter(|(ns, _)| ns == PUBLIC_NAMESPACE)
            .count()
    }

    /// Allowlisted IMAGE digest, dedup ON: the config + layer + the manifest are
    /// promoted to `_public`, served — and a SECOND tenant that never pushed it,
    /// resolving the SAME digest, is served from `_public` with ZERO extra
    /// upstream fetch. This is the cross-tenant proof.
    #[tokio::test]
    async fn allowlisted_image_promotes_closure_and_serves_cross_tenant() {
        let repo = "library/alpine";
        let config_bytes = b"cfg-object".to_vec();
        let layer_bytes = b"layer-bytes".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        let r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), true);
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: Some("application/vnd.oci.image.manifest.v1+json".to_owned()),
                docker_content_digest: Some(manifest_digest.clone()),
            },
        );
        r.blob_fetcher
            .insert(repo, &config_digest, config_bytes.clone());
        r.blob_fetcher
            .insert(repo, &layer_digest, layer_bytes.clone());

        // Tenant A (by-digest FROM) → promote closure to `_public`, serve it.
        let ta = tenant(100);
        let out = r
            .resolver
            .resolve_on_miss(&ta, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("allowlisted image resolves from the promote");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(out.bytes.as_ref(), manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "one manifest fetch for A"
        );

        // Full closure lives in `_public`: manifest + config + layer.
        assert!(in_public(&r, &manifest_digest), "manifest in `_public`");
        assert!(in_public(&r, &config_digest), "config in `_public`");
        assert!(in_public(&r, &layer_digest), "layer in `_public`");
        // Nothing landed in tenant A's OWN namespace (cross-tenant, not per-tenant).
        assert!(
            r.kv.0.lock().unwrap().is_empty(),
            "an allowlisted `_public` promote does NOT write per-tenant KV"
        );
        assert!(
            !r.cas
                .0
                .lock()
                .unwrap()
                .keys()
                .any(|(ns, _)| *ns == ta.to_canonical_text()),
            "no per-tenant CAS write on the `_public` promote"
        );

        // Tenant B (never pushed alpine) resolves the SAME digest → served from
        // `_public`, ZERO extra upstream fetch (the cross-tenant proof).
        let tb = tenant(101);
        let out_b = r
            .resolver
            .resolve_on_miss(&tb, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("tenant B served from `_public`");
        assert_eq!(out_b.bytes.as_ref(), manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "tenant B must be served from `_public` with NO extra upstream fetch"
        );
    }

    /// Allowlisted INDEX digest, dedup ON: the per-arch child (skip
    /// `unknown/unknown`) + its blobs + the index are all promoted to `_public`;
    /// a by-digest GET of the child — which is NOT individually allowlisted —
    /// serves from `_public` (the existence read).
    #[tokio::test]
    async fn allowlisted_index_promotes_children_and_child_serves_from_public() {
        let repo = "library/debian";
        let config_bytes = b"debian-cfg".to_vec();
        let layer_bytes = b"debian-layer".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let child_manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let child_digest = sha256_wire(&child_manifest);
        // Index with one real linux/amd64 child + a `windows/amd64` child + an
        // `unknown/unknown` attestation entry. ONLY the linux child is promoted;
        // the windows + attestation entries MUST be skipped (never fetched — their
        // digests are intentionally NOT registered with the fetcher, so any fetch
        // of them would error and abort the promote).
        let index = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": child_digest,
                    "size": 1,
                    "platform": { "os": "linux", "architecture": "amd64" }
                },
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": sha256_wire(b"windows-image-manifest"),
                    "size": 1,
                    "platform": { "os": "windows", "architecture": "amd64" }
                },
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": sha256_wire(b"attestation-blob"),
                    "size": 1,
                    "platform": { "os": "unknown", "architecture": "unknown" }
                }
            ]
        }))
        .unwrap();
        let index_digest = sha256_wire(&index);

        let r = rig_full(generous_limiter(), allowlist_of(&[&index_digest]), true);
        r.manifest_fetcher.insert(
            repo,
            &index_digest,
            FetchedManifest {
                bytes: index.clone(),
                content_type: Some("application/vnd.oci.image.index.v1+json".to_owned()),
                docker_content_digest: Some(index_digest.clone()),
            },
        );
        r.manifest_fetcher.insert(
            repo,
            &child_digest,
            FetchedManifest {
                bytes: child_manifest.clone(),
                content_type: Some("application/vnd.oci.image.manifest.v1+json".to_owned()),
                docker_content_digest: Some(child_digest.clone()),
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let ta = tenant(110);
        let out = r
            .resolver
            .resolve_on_miss(&ta, repo, &index_digest)
            .await
            .unwrap()
            .expect("index resolves via promote");
        assert_eq!(out.digest, index_digest);
        // index + the ONE linux child fetched (2); the windows + attestation
        // entries are skipped (never fetched).
        assert_eq!(
            r.manifest_fetcher.call_count(),
            2,
            "only the index + its one LINUX child are fetched (windows + attestation skipped)"
        );
        // Closure fully in `_public`: index, child manifest, config, layer.
        assert!(in_public(&r, &index_digest));
        assert!(in_public(&r, &child_digest));
        assert!(in_public(&r, &config_digest));
        assert!(in_public(&r, &layer_digest));

        // A by-digest GET of the child (NOT individually allowlisted) serves from
        // `_public` via the existence read — for a tenant that never pushed it.
        let tb = tenant(111);
        let child_out = r
            .resolver
            .resolve_on_miss(&tb, repo, &child_digest)
            .await
            .unwrap()
            .expect("child manifest served from `_public` by existence");
        assert_eq!(child_out.bytes.as_ref(), child_manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            2,
            "the child existence read must NOT trigger any upstream fetch"
        );
    }

    /// UN-allowlisted digest, dedup ON: nothing enters `_public`; the M1
    /// per-tenant path is taken.
    #[tokio::test]
    async fn unallowlisted_digest_dedup_on_stays_per_tenant() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        // dedup ON but the allowlist does NOT contain this digest.
        let r = rig_full(
            generous_limiter(),
            allowlist_of(&[&sha256_wire(b"some-other-image")]),
            true,
        );
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(120);
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("un-allowlisted digest resolves per-tenant (M1)");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(public_row_count(&r), 0, "nothing in `_public`");
        // Per-tenant path: manifest in the tenant's OWN KV.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
    }

    /// TAG reference, dedup ON: never touches `_public` (per-tenant only) even
    /// when a homograph digest is allowlisted — a tag is mutable.
    #[tokio::test]
    async fn tag_reference_dedup_on_never_public() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        // The manifest's digest IS allowlisted, but the request is by TAG.
        let r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), true);
        r.manifest_fetcher.insert(
            repo,
            "3.20",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: Some(manifest_digest.clone()),
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(130);
        let _ = r
            .resolver
            .resolve_on_miss(&t, repo, "3.20")
            .await
            .unwrap()
            .expect("tag resolves per-tenant");
        assert_eq!(public_row_count(&r), 0, "a TAG never writes `_public`");
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, "3.20"))));
    }

    /// dedup OFF, even an allowlisted digest: no `_public` reads/writes — the
    /// resolver is byte-identical to M1 (this is the explicit dedup-OFF proof;
    /// the whole M1 suite runs on `rig()` which is dedup OFF too).
    #[tokio::test]
    async fn dedup_off_allowlisted_digest_no_public() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        // Allowlisted, but dedup is OFF → per-tenant.
        let r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), false);
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(140);
        let _ = r
            .resolver
            .resolve_on_miss(&t, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("resolves per-tenant with dedup OFF");
        assert_eq!(public_row_count(&r), 0, "dedup OFF ⇒ no `_public` writes");
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
    }

    /// Over the `_public` growth ceiling: no promote (fail-open to per-tenant).
    #[tokio::test]
    async fn over_ceiling_no_public_promote_fails_open() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        let mut r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), true);
        // Force the ceiling to zero: `0 >= 0` ⇒ every promote is refused.
        r.resolver.public_ceiling_bytes = 0;
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(150);
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("over-ceiling still resolves per-tenant (fail-open)");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(
            public_row_count(&r),
            0,
            "over-ceiling ⇒ no `_public` promote"
        );
        // Fell through to the per-tenant M1 path.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
    }

    /// A broken child fetch mid-closure ⇒ nothing partial served, and the index
    /// ROOT is NOT left in `_public` (so a later existence read cannot serve an
    /// incomplete closure as complete). The resolve fails open to per-tenant.
    #[tokio::test]
    async fn broken_child_mid_closure_leaves_no_public_root() {
        let repo = "library/debian";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let child_manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let child_digest = sha256_wire(&child_manifest);
        let index = index_bytes(&child_digest);
        let index_digest = sha256_wire(&index);

        let r = rig_full(generous_limiter(), allowlist_of(&[&index_digest]), true);
        // The index is fetchable, but the CHILD manifest is NOT (upstream fetch
        // fails) → the closure promote aborts.
        r.manifest_fetcher.insert(
            repo,
            &index_digest,
            FetchedManifest {
                bytes: index,
                content_type: Some("application/vnd.oci.image.index.v1+json".to_owned()),
                docker_content_digest: Some(index_digest.clone()),
            },
        );
        // (child_digest intentionally NOT inserted → fetch_manifest errs.)
        // Also make the index itself resolvable per-tenant on the fail-open path.

        let t = tenant(160);
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &index_digest)
            .await
            .unwrap();
        // Fell open to per-tenant (the index IS fetchable there → resolves), but
        // CRUCIALLY the index root is NOT in `_public`.
        assert!(
            !in_public(&r, &index_digest),
            "a mid-closure failure must NOT leave the index root in `_public`"
        );
        // Whatever the per-tenant outcome, no `_public` root exists for a later
        // existence read to serve as a complete closure.
        assert!(
            out.is_some(),
            "index still resolves per-tenant on the fail-open path"
        );
        // The index was stored per-tenant, never as a `_public` root.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &index_digest))));
    }
}
