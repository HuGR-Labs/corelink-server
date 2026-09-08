/// Service principal stamped on the adapter's CAS operations. Identifies
/// the adapter-host service, NOT the end-user PAT.
const OCI_SERVICE_PRINCIPAL: &str = "oci-adapter-host";

/// Maximum number of concurrent open upload sessions per tenant (F25 guard).
///
/// Each open session holds an in-memory `Vec<u8>` buffer that accumulates blob
/// chunks via `PATCH`. Without a cap, an authenticated tenant can open N sessions
/// and feed each a large body, growing heap to N × chunk-size up to N × 5 GiB.
/// Capping at 4 matches the Turborepo PUT concurrency limit and covers realistic
/// multi-layer parallel push patterns (most OCI clients push 1–3 layers at once).
/// Excess `POST /v2/<repo>/blobs/uploads/` are rejected with an error mapped to
/// 429 by the adapter error layer.
const OCI_MAX_OPEN_SESSIONS_PER_TENANT: usize = 4;

/// Global ceiling on in-flight upload bytes across ALL tenants (audit #6 /
/// WP-OCI-DOS).
///
/// The shared `_oci` Durable Object process holds all tenants' upload sessions
/// in a single heap. Without a cross-tenant ceiling, N tenants each with 4
/// open sessions could together accumulate N × 4 × 5 GiB of heap — unbounded
/// memory exhaustion. This constant caps the TOTAL buffered bytes at any
/// instant across all open sessions.
///
/// 512 MiB is sized so that the maximum realistic multi-tenant burst (e.g.
/// 10 concurrent tenants each pushing a 50 MiB layer) fits comfortably, while
/// a single tenant cannot fill more than a bounded fraction of the ceiling with
/// their per-tenant session cap. When the ceiling is reached, new `PATCH`
/// chunks are rejected with a port error that surfaces as `429 + Retry-After`.
const OCI_MAX_INFLIGHT_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB

/// Per-tenant in-flight byte ceiling (rt-nuclear #3/#12). This is a bounded
/// 128 MiB slice, strictly below the 512 MiB process-wide ceiling; the
/// separate open-session cap provides additional concurrency fairness. It is
/// intentionally independent from the session's configured blob limit.
const OCI_MAX_INFLIGHT_BYTES_PER_TENANT: u64 = 128 * 1024 * 1024;

/// Idle timeout after which an upload session is considered abandoned and is
/// reaped on the next `open_upload` call (lazy reap — audit #6 / WP-OCI-DOS).
///
/// OCI clients that crash or disconnect mid-push leave open sessions that hold
/// memory indefinitely. Without a reaper the per-tenant cap (F25) would
/// permanently lock a tenant out of pushing if their client crashed between
/// `POST` (open) and `PUT` (finalize). 15 minutes covers realistic slow
/// uploads on poor network links while recycling abandoned sessions promptly.
///
/// The reaper is lazy (runs inside `open_upload` under the same mutex lock),
/// so there is no background thread or tokio task — correct for the
/// single-threaded Durable Object runtime.
const OCI_SESSION_IDLE_TIMEOUT_MS: u64 = 15 * 60 * 1000; // 15 min

/// Bearer-realm URL the adapter advertises in `Www-Authenticate` on a
/// `/v2/` 401, pointing OCI clients at the `/token` exchange. The dedicated flat
/// prod host `corelink-oci.humangr.com` — NOW PROVISIONED (CNAME → workers.dev
/// proxied + the `corelink-oci.humangr.com/*` Worker route, 2026-06-21) so a real
/// `docker login/push` reaches the host-agnostic `/v2/` + `/token` OCI paths. The
/// adapter only emits this string; it never fetches it.
const OCI_BEARER_REALM: &str = "https://corelink-oci.humangr.com/token";

/// Env var holding the raw (≥32-byte) HMAC key the adapter uses to sign
/// its realm bearer tokens. Distinct from `PAT_SIGNING_KEY` (which keys
/// the PAT HMAC) — this keys the OCI *session* token only. Read by the
/// container mount ([`crate::routes::build_with_factory`]).
pub const OCI_TOKEN_KEY_ENV: &str = "CORELINK_OCI_TOKEN_KEY";

/// Legacy env var name for the OCI session HMAC key (CAA-360 #8). Prod was
/// deployed with `HUGR_OCI_TOKEN_KEY` while the code canonicalized to
/// `CORELINK_OCI_TOKEN_KEY` — the drift would fail the OCI route CLOSED in prod.
/// The mount reads the canonical name first and falls back to this legacy name
/// so the route works regardless; rename the prod secret to the canonical name
/// to retire this fallback.
pub const OCI_TOKEN_KEY_ENV_LEGACY: &str = "HUGR_OCI_TOKEN_KEY";

// ── BlobStore port → the shared MoatCache ───────────────────────────────────

/// Per-session state for an in-flight OCI blob upload.
///
/// Keyed by `<tenant-text>:<uuid>` in [`OciMoatStore::uploads`].
struct UploadSession {
    /// Accumulated chunk bytes (grows with each `PATCH`).
    buf: Vec<u8>,
    /// Wall-clock ms of the last successful `append_chunk` (or session
    /// open if no chunks have arrived yet). Used by the lazy reaper
    /// ([`OCI_SESSION_IDLE_TIMEOUT_MS`]) to evict abandoned sessions.
    last_active_ms: u64,
}

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
/// canonical text) — isolated by default. **F3.2 increment 6** adds
/// flag-gated cross-tenant dedup: when `dedup` is on AND an owner-pinned
/// digest is [`is_allowlisted`](PublicBaseAllowlist::is_allowlisted), that
/// blob upload/finalize put and blob get are routed to the shared
/// [`crate::adapter_cache::PUBLIC_NAMESPACE`] (uncapped) via the SAME
/// predicate on both paths ([`Self::routes_to_public`]). The predicate has a
/// digest but no media type, so the five baked index digests are eligible too;
/// the upload bytes must still verify against the declared digest before write.
/// This predicate does not govern `PUT /v2/<repo>/manifests/<reference>`:
/// manifest/index JSON remains in the tenant-scoped `ManifestKvStore`.
/// The flag defaults OFF for safe fallback behavior; production enables it via
/// the boot flag after the owner-reviewed six-pin manifest is baked in.
///
/// Upload sessions are buffered in-process in `uploads` keyed by the
/// server-allocated UUID; `finalize_upload` flushes the assembled bytes
/// to the moat AND returns them so the adapter can run its digest
/// verification. The buffer is NOT durable (process restart drops
/// in-flight uploads) — acceptable for the single-container deployment;
/// see OPEN DECISIONS.
///
/// DoS mitigations (audit #6 / WP-OCI-DOS):
///
/// * `inflight_bytes` — atomic counter of bytes currently held across ALL
///   open sessions. Capped at [`OCI_MAX_INFLIGHT_BYTES`]; `PATCH` that
///   would push the counter over the ceiling is rejected with a port error
///   mapped to `429 + Retry-After` by the adapter layer.
/// * Lazy session reaper — `open_upload` evicts sessions idle for more
///   than [`OCI_SESSION_IDLE_TIMEOUT_MS`] ms before checking the per-tenant
///   cap, so a crashed client cannot permanently lock its tenant out of
///   pushing. No background thread is required (DO runtime is
///   single-threaded).
struct OciMoatStore {
    moat: Arc<MoatCache>,
    /// `<tenant-text>:<uuid>` → session state for in-flight uploads.
    uploads: Mutex<HashMap<String, UploadSession>>,
    /// Bytes currently buffered across ALL open upload sessions.
    /// Updated atomically; never allowed to exceed [`OCI_MAX_INFLIGHT_BYTES`].
    inflight_bytes: AtomicU64,
    /// Per-tenant in-flight upload bytes (rt-nuclear #3/#12 — noisy-neighbour
    /// starvation). Keyed by tenant canonical text; each entry is bounded by
    /// [`OCI_MAX_INFLIGHT_BYTES_PER_TENANT`]. Mutated under this lock INSIDE the
    /// `append_chunk` reservation so the per-tenant budget is enforced atomically
    /// with the buffer extend; pruned to zero on release.
    tenant_inflight: Mutex<HashMap<String, u64>>,
    /// F3.2 inc6: is flag-gated cross-tenant `_public` routing of allowlisted
    /// OCI digests enabled? PROD injects
    /// [`crate::public_flags::oci_public_dedup_enabled`] (boot-read, default
    /// OFF); tests inject a literal. When `false` this store is byte-identical
    /// to the per-tenant-only pre-inc6 behavior (the whole WP is a no-op).
    dedup: bool,
    /// The owner-curated, digest-pinned public-base allowlist consulted ONLY
    /// when `dedup` is on. Fail-CLOSED: a malformed baked manifest degrades to
    /// deny-all (empty), never a partially-trusted set. Contains only baked
    /// owner-pinned digests. The predicate deliberately receives no OCI media type:
    /// any owner-pinned digest can route to `_public`, but unpinned digests do
    /// not.
    allowlist: PublicBaseAllowlist,
    /// Adapter-configured maximum blob size. This is the session budget and is
    /// checked before every buffer append; the global and tenant ceilings above
    /// remain the independent process-memory bounds.
    max_blob_size_bytes: u64,
}

impl std::fmt::Debug for OciMoatStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciMoatStore").finish_non_exhaustive()
    }
}

impl OciMoatStore {
    /// Test constructor. `dedup` is the boot flag
    /// ([`crate::public_flags::oci_public_dedup_enabled`]); the allowlist is the
    /// baked owner manifest, loaded FAIL-CLOSED — a malformed manifest degrades
    /// to deny-all (empty) rather than a partially-trusted set, so a broken
    /// manifest can never widen the shared namespace. Production wires
    /// [`Self::with_allowlist`] directly so the allowlist instance is SHARED with
    /// the manifest resolver (loaded once at the router).
    #[cfg(test)]
    fn new(moat: Arc<MoatCache>, dedup: bool) -> Self {
        Self::with_allowlist(
            moat,
            dedup,
            PublicBaseAllowlist::from_baked_manifest().unwrap_or_default(),
        )
    }

    /// Constructor with an explicit allowlist. The production [`Self::new`]
    /// delegates here with the baked manifest; tests inject a hermetic allowlist
    /// so `_public` routing can be exercised with a hermetic allowlist instead of
    /// depending on the production manifest population.
    #[cfg(test)]
    fn with_allowlist(moat: Arc<MoatCache>, dedup: bool, allowlist: PublicBaseAllowlist) -> Self {
        Self::with_allowlist_and_blob_limit(moat, dedup, allowlist, defaults::BLOB_SIZE_LIMIT_BYTES)
    }

    /// Construct the store with the same blob limit used by the adapter. The
    /// explicit argument keeps the in-memory session budget config-derived
    /// rather than duplicating a smaller, hidden constant in this layer.
    fn with_allowlist_and_blob_limit(
        moat: Arc<MoatCache>,
        dedup: bool,
        allowlist: PublicBaseAllowlist,
        max_blob_size_bytes: u64,
    ) -> Self {
        Self {
            moat,
            uploads: Mutex::new(HashMap::new()),
            inflight_bytes: AtomicU64::new(0),
            tenant_inflight: Mutex::new(HashMap::new()),
            dedup,
            allowlist,
            max_blob_size_bytes,
        }
    }

    /// The SINGLE flag+allowlist predicate deciding whether a client-finalized,
    /// owner-pinned digest routes to the shared `_public` namespace. The write
    /// path ([`BlobStore::finalize_upload`]) uses this predicate; the read path
    /// ([`BlobStore::get_blob`]) is deliberately the existence-based superset and
    /// consults `_public` whenever `dedup` is on. This predicate has no media-type
    /// input: it admits exactly the baked digest set, including pinned indexes.
    /// It is used only for blob upload/finalize storage; manifest PUT uses the
    /// tenant-scoped `ManifestKvStore` instead.
    fn routes_to_public(&self, blob_key: &str) -> bool {
        self.dedup && self.allowlist.is_allowlisted(blob_key)
    }

    /// Release `bytes` from a tenant's per-tenant in-flight counter (rt-nuclear
    /// #3/#12), saturating at zero and pruning the entry when it reaches 0 so the
    /// map does not grow without bound. Mirror of the global `fetch_sub` release.
    fn release_tenant_bytes(&self, tenant_text: &str, bytes: u64) {
        if bytes == 0 {
            return;
        }
        if let Ok(mut t) = self.tenant_inflight.lock() {
            if let Some(c) = t.get_mut(tenant_text) {
                *c = c.saturating_sub(bytes);
                if *c == 0 {
                    t.remove(tenant_text);
                }
            }
        }
    }
}
