/// `GET /v8/artifacts/:hash` — retrieve a Turborepo build artifact.
///
/// MUST use matchit-0.7 `:name` syntax (not `{name}`).
pub const TURBO_GET_ROUTE: &str = "/v8/artifacts/{hash}";

/// `PUT /v8/artifacts/:hash` — store a Turborepo build artifact.
///
/// Same template as GET; axum disambiguates by HTTP method.
pub const TURBO_PUT_ROUTE: &str = "/v8/artifacts/{hash}";

/// `POST /v8/artifacts/events` — accept-and-drop Turborepo telemetry.
pub const TURBO_EVENTS_ROUTE: &str = "/v8/artifacts/events";

/// `POST /v8/artifacts/status` — static remote-cache enabled check.
pub const TURBO_STATUS_ROUTE: &str = "/v8/artifacts/status";

/// The Turborepo artifact-signature header (F-009 / WP-9a).
///
/// A turbo client with `TURBO_REMOTE_CACHE_SIGNATURE_KEY` set computes an
/// HMAC over the artifact hash and body and sends it as `x-artifact-tag` on
/// PUT; on GET it expects the remote cache to echo the SAME value back and
/// verifies it locally before trusting the artifact. The signing key is the
/// CUSTOMER's — CoreLink never holds it and therefore cannot compute or
/// validate the tag. The server's entire protocol role is **store it
/// alongside the artifact and hand it back verbatim**; the value is opaque.
///
/// Dropping the header (what CoreLink did before this constant existed) is
/// worse than not supporting signatures at all: the client asks for
/// verification and gets silence instead of a failure.
pub const ARTIFACT_TAG_HEADER: &str = "x-artifact-tag";

/// Per-route request-body cap for the `/v8/artifacts/*` routes (100 MiB).
///
/// Turbo build artifacts are legitimately larger than the 10 MiB global body
/// limit applied in `main.rs`, so the Turbo router layers this larger
/// `DefaultBodyLimit` (which axum honours as the innermost limit) — while still
/// bounding the body so an authenticated PAT cannot OOM the shared container.
pub const TURBO_BODY_LIMIT_BYTES: usize = 100 * 1024 * 1024;

/// Maximum concurrent in-flight `PUT /v8/artifacts/:hash` requests for a
/// single tenant. Excess PUTs are rejected with 429 (Too Many Requests).
///
/// Rationale: each Turbo PUT buffers up to `TURBO_BODY_LIMIT_BYTES` (100 MiB)
/// in memory. Without a concurrency cap, a single authenticated tenant can open
/// N concurrent PUTs and consume N × 100 MiB of heap, OOM-ing their container.
/// Capping at 4 bounds peak per-tenant working set to ~400 MiB while still
/// allowing realistic parallel builds (Turborepo's default parallelism is 2–4
/// concurrent tasks). Excess requests get 429, not 503 — the client retries.
pub const TURBO_PUT_CONCURRENCY_LIMIT: usize = 4;

/// Maximum concurrent in-flight `GET /v8/artifacts/:hash` requests for a single
/// tenant. Excess GETs are rejected with 429 (Too Many Requests).
///
/// Rationale (H1 — read-path OOM, mirrors [`TURBO_PUT_CONCURRENCY_LIMIT`]): a
/// successful GET buffers the FULL artifact (up to [`TURBO_BODY_LIMIT_BYTES`] =
/// 100 MiB) into the heap before responding. Without a concurrency cap, an
/// authenticated tenant who has uploaded a 100 MiB artifact can burst N
/// concurrent GETs of it and consume N × 100 MiB of heap, OOM-ing the shared
/// container — the read path was left asymmetrically open while the PUT path was
/// hardened. Capping at 4 bounds peak per-tenant read working set to ~400 MiB
/// while still allowing realistic parallel cache reads. Excess requests get 429,
/// not 503 — the client retries.
pub const TURBO_GET_CONCURRENCY_LIMIT: usize = 4;

/// Per-route request-body cap for `POST /v8/artifacts/events` (64 KiB).
///
/// C4: the events route is "accept-and-drop" telemetry — it has NO storage,
/// no `PutConcurrencyGuard`, and no legitimate reason to buffer a large body.
/// Without its own cap it would inherit the router-wide
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB), letting an authenticated tenant OOM
/// the shared container by POSTing 100 MiB "telemetry" bodies. This small
/// route-specific `DefaultBodyLimit` makes axum reject (413) oversized
/// telemetry BEFORE buffering. Real Turbo `/events` payloads are a few KiB of
/// JSON, so 64 KiB is generous headroom.
pub const EVENTS_BODY_LIMIT_BYTES: usize = 64 * 1024;

/// Process-wide cap on concurrent large Turbo PUTs (C5).
///
/// [`PutConcurrencyGuard`] is PER-TENANT (4 × 100 MiB each). With N distinct
/// authenticated tenants, peak transient heap is `N × 4 × 100 MiB` — N tenants
/// can together OOM the shared container even though each is within its own
/// per-tenant cap. This is a SECOND, PROCESS-WIDE budget, reserved (like the
/// per-tenant guard) in a `FromRequestParts` extractor — which axum 0.7 runs
/// BEFORE the `body: Bytes` body extractor — so a permit is taken (or the
/// request 503s) BEFORE a single body byte is buffered.
///
/// The permit count is derived from the deployed basic container's dedicated
/// Turbo PUT slice (see [`crate::container_capacity`]), not an obsolete
/// instance-size assumption. One 100 MiB artifact is admitted globally; the
/// remaining process budget covers the transient body clone and runtime.
pub const GLOBAL_TURBO_PUT_PERMITS: usize = crate::container_capacity::TURBO_PUT_GLOBAL_PERMITS;

/// Process-wide cap on concurrent large Turbo GETs (H1 — read-path twin of
/// [`GLOBAL_TURBO_PUT_PERMITS`]).
///
/// [`GetConcurrencyGuard`] is PER-TENANT (4 × 100 MiB each). With N distinct
/// authenticated tenants, peak transient read heap is `N × 4 × 100 MiB` — N
/// tenants can together OOM the shared container even though each is within its
/// own per-tenant cap. This is a SECOND, PROCESS-WIDE budget, reserved (like the
/// per-tenant guard) in a `FromRequestParts` extractor — which axum 0.7 runs
/// BEFORE the response body is buffered — so a permit is taken (or the request
/// 503s) BEFORE the artifact is read into the heap.
///
/// Sizing mirrors [`GLOBAL_TURBO_PUT_PERMITS`], using the deployed basic
/// container's dedicated Turbo GET slice. Reads get a SEPARATE pool from
/// writes: a GET flood cannot consume PUT permits (and vice versa). Beyond
/// the declared global count the extractor fails CLOSED (503) after a short
/// acquire wait rather than blocking forever.
pub const GLOBAL_TURBO_GET_PERMITS: usize = crate::container_capacity::TURBO_GET_GLOBAL_PERMITS;

/// Process-wide concurrency budget for the accept-and-drop `/v8/artifacts/events`
/// telemetry route — SEPARATE from the PUT write budget (rt-nuclear cycle-2
/// #4/#8). Events previously shared `GLOBAL_TURBO_PUT_BUDGET` (the C5 OOM guard),
/// but that let a telemetry flood / slow-body events POST hold PUT permits and
/// starve real cache writes (cross-plane DoS). A dedicated budget keeps the C5
/// OOM bound (≤ this many × `EVENTS_BODY_LIMIT_BYTES` ≈ 16 × 64 KiB = 1 MiB
/// aggregate events heap) WITHOUT letting worthless telemetry starve billable
/// writes. 16 is ample for telemetry; events are tiny + accept-and-drop.
pub const GLOBAL_TURBO_EVENTS_PERMITS: usize =
    crate::container_capacity::TURBO_EVENTS_GLOBAL_PERMITS;

/// Maximum concurrent in-flight `POST /v8/artifacts/events` requests for a
/// single tenant (rt-nuclear cycle-2 #9). Excess events POSTs are rejected with
/// 429 (Too Many Requests).
///
/// The PUT path already has a per-tenant cap ([`TURBO_PUT_CONCURRENCY_LIMIT`]),
/// but `/events` had only the PROCESS-WIDE [`GLOBAL_TURBO_EVENTS_PERMITS`]
/// budget — so one tenant could open all 16 events permits and monopolise the
/// telemetry pool, starving every OTHER tenant's `/events` (an intra-plane,
/// per-tenant fairness gap). This per-tenant cap mirrors the PUT guard so no
/// single tenant can hog more than its share of the events pool. Telemetry is
/// tiny + accept-and-drop, so 4 concurrent is ample headroom for one tenant.
pub const EVENTS_CONCURRENCY_LIMIT: usize = 4;

/// Maximum wall-time a `POST /v8/artifacts/events` body may take to stream in
/// before the handler aborts it (rt-nuclear cycle-2 #8 — slow-body slowloris).
///
/// `/events` holds one [`GLOBAL_TURBO_EVENTS_PERMITS`] permit (and now one
/// per-tenant [`EVENTS_CONCURRENCY_LIMIT`] slot) for the WHOLE request — from
/// permit-acquisition through body read. Without a server-side read deadline a
/// client can dribble its (≤ 64 KiB) body one byte at a time and pin a permit +
/// a slot indefinitely; `EVENTS_CONCURRENCY_LIMIT` such clients per tenant, or
/// `GLOBAL_TURBO_EVENTS_PERMITS` across tenants, slowloris the events pool to a
/// standstill. The handler reads the body under this timeout; a stalled body is
/// aborted with 408 (Request Timeout), which returns the handler and RAII-frees
/// the permit + slot. A real Turbo `/events` payload is a few KiB of JSON over a
/// healthy connection, so a few seconds is generous for any legitimate client.
const EVENTS_BODY_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the global-budget extractor waits for a permit before declaring
/// the container globally saturated and returning 503. Short by design (same
/// rationale as `adapter_pat::ARGON2_PERMIT_WAIT`): a caller waiting longer is
/// better served a fast fail than a stalled request holding a connection
/// hostage under a flood.
const GLOBAL_PUT_PERMIT_WAIT: Duration = Duration::from_millis(250);

/// Number of fixed per-object write locks (C1). A power of two so the shard
/// index is a cheap mask. 1024 `tokio::sync::Mutex<()>` (each is a few words)
/// is a constant, memory-BOUNDED footprint — there is no per-key map that can
/// grow or need eviction. Same-object PUTs hash to the same shard and
/// serialize; the false-sharing rate of distinct objects colliding on a shard
/// is ~1/1024, which only ever costs a little extra serialization, never
/// correctness.
const TURBO_WRITE_LOCK_SHARDS: usize = 1024;

// ── Query parameters ──────────────────────────────────────────────────────────

/// Query parameters for `GET /v8/artifacts/:hash` and
/// `PUT /v8/artifacts/:hash`.
///
/// `teamId` is **required**; missing value → 400 (axum rejects the
/// extraction before the handler is called).  `slug` is optional and
/// informational.
#[derive(Debug, Deserialize)]
pub struct ArtifactQuery {
    /// Vercel team identifier — a logical sub-namespace WITHIN the authenticated
    /// tenant, NOT the isolation tenant. Forms the storage key prefix
    /// (`"<teamId>/<hash>"`); the isolation tenant is `AuthTenant`.
    #[serde(rename = "teamId")]
    pub team_id: String,
    /// Informational slug (repo name, etc.) — logged but not partitioned.
    #[serde(default)]
    pub slug: String,
}

// ── Response DTOs ─────────────────────────────────────────────────────────────

/// Serialised body for `PUT /v8/artifacts/:hash` success response.
///
/// Vercel spec: `{"urls": ["..."]}`
#[derive(Debug, Serialize)]
pub struct PutArtifactResponse {
    /// Storage URL(s) for the stored artifact.  Turbo treats 200 + this body
    /// as "uploaded successfully"; the URL is informational.
    pub urls: Vec<String>,
}

// ── Route state ───────────────────────────────────────────────────────────────

/// Shared state for `/v8/artifacts/*` routes.
///
/// Holds a single `Arc<dyn TurboArtifactHandler>` so the backing store is
/// swappable behind the port trait without changing the route layer.  See
/// [`build_handlers`] for Phase 0 wiring using `InMemoryKvStore`.
#[derive(Clone)]
pub struct TurboRouteState {
    /// Handler implementing all four Turbo API verbs.
    pub handler: Arc<dyn TurboArtifactHandler>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed); checked at the TOP of the billable
    /// artifact GET/PUT handlers, AFTER the scope gate, BEFORE storage. `None`
    /// in dev/CI (not enforced). The telemetry `events`/static `status` verbs
    /// are NOT billable and are not gated. Set by `routes::build_with_factory`.
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production; re-runs the full Argon2id Option-B verify
    /// at the TOP of the billable artifact handlers, AFTER scope, BEFORE storage.
    /// `None` in dev/CI (skipped). See [`crate::native_pat_gate`].
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
    /// Optional per-tenant storage byte accountant (red-team finding #1).
    /// `Some` in production — Turbo writes to R2 directly (via the `R2KvStore`
    /// handler), so a successful PUT `accrue`s its body bytes (over-cap ⇒ 402 /
    /// fault ⇒ 503). `None` in dev/CI. See [`crate::byte_accounting`].
    pub bytes: Option<Arc<crate::byte_accounting::ByteAccountant>>,
    /// Per-tenant in-flight PUT concurrency counter (finding #2 — pre-buffer
    /// self-DoS guard).
    ///
    /// Maps `tenant_id → count` of PUT requests currently in-flight (body
    /// buffered in memory). The reservation is taken by the
    /// [`PutConcurrencyGuard`] `FromRequestParts` extractor — which axum runs
    /// BEFORE the `body: Bytes` extractor — so a 6th concurrent PUT is rejected
    /// 429 BEFORE its (up to 100 MiB) body is read into the heap. The RAII guard
    /// the extractor yields releases the slot when the handler returns.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-tenant in-flight GET concurrency counter (H1 — pre-buffer read-path
    /// self-DoS guard, the read twin of [`put_inflight`](Self::put_inflight)).
    ///
    /// Maps `tenant_id → count` of GET requests currently in-flight (artifact
    /// buffered in memory before the response is written). The reservation is
    /// taken by the [`GetConcurrencyGuard`] `FromRequestParts` extractor — which
    /// axum runs BEFORE the handler buffers the response body — so a 5th
    /// concurrent GET is rejected 429 BEFORE its (up to 100 MiB) artifact is read
    /// into the heap. The RAII guard the extractor yields releases the slot when
    /// the handler returns.
    pub(crate) get_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-tenant in-flight `/events` concurrency counter (rt-nuclear cycle-2
    /// #9 — events fairness).
    ///
    /// Maps `tenant_id → count` of `POST /v8/artifacts/events` requests
    /// currently in-flight. The PUT plane already has a per-tenant cap
    /// ([`put_inflight`](Self::put_inflight)); `/events` had only the
    /// process-wide [`GLOBAL_TURBO_EVENTS_PERMITS`] budget, so one tenant could
    /// take every events permit and starve all other tenants' telemetry. The
    /// reservation is taken by the [`EventsConcurrencyGuard`] `FromRequestParts`
    /// extractor (which axum runs BEFORE the body is read), capped at
    /// [`EVENTS_CONCURRENCY_LIMIT`] per tenant (429 over it). The RAII
    /// [`EventsSlot`] the extractor yields releases the slot on every return
    /// path (success, error, panic, body-read timeout).
    pub(crate) events_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Fixed array of per-object async write locks (C1 — same-key write
    /// serialization).
    ///
    /// `handle_put` accrues the full new body, then `R2KvStore::write` does a
    /// NON-serialized GET-presence probe to capture `prior_len`, then the route
    /// `release`s that `prior_len`. Two concurrent PUTs to the SAME stored
    /// object both probe the same prior size `L` and both release `L` —
    /// double-releasing `bytes_used` and underflowing the tenant's accounting
    /// while real storage is unchanged (re-grow + repeat ⇒ unbounded free
    /// storage). The fix serializes the whole probe→put→release sequence per
    /// stored object. The lock IDENTITY is `(caller_tenant, team_id, hash)` —
    /// exactly the tuple that maps to ONE R2 object (`tenant = caller_tenant`,
    /// storage key = `"<team_id>/<hash>"` per `corelink_turbo_bridge::adapter`,
    /// HMAC-prefixed per tenant in `R2KvStore::object_key`). We index a FIXED
    /// `TURBO_WRITE_LOCK_SHARDS`-wide array by a stable hash of that tuple, so
    /// the footprint is constant (no growing/evicting map) yet same-object
    /// writes always serialize. Distinct objects rarely collide on a shard
    /// (~1/1024) and a collision only adds a little serialization, never a
    /// correctness defect.
    pub(crate) write_locks: Arc<Vec<AsyncMutex<()>>>,
    /// Display usage aggregator (usage-metering-roi). The Turbo artifact GET-HIT /
    /// GET-MISS / PUT decision points `record` fire-and-forget into this meter — a
    /// cheap lock+increment, NEVER a DB call / `await` on the hot path. Inert in
    /// dev/CI; `build_with_factory` sets the ONE shared, D1-backed meter.
    pub usage_meter: Arc<crate::usage_meter::UsageMeter>,
}

/// Process-wide Turbo-PUT byte/concurrency budget (C5). A single
/// [`Semaphore`] shared by EVERY [`TurboRouteState`] (and thus every tenant)
/// in the process, sized to [`GLOBAL_TURBO_PUT_PERMITS`]. Lazily initialised
/// so it is a true singleton regardless of how many routers are built (prod
/// builds one; tests build many).
static GLOBAL_TURBO_PUT_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Accessor for the process-wide PUT budget (C5).
fn global_turbo_put_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_TURBO_PUT_BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURBO_PUT_PERMITS))),
    )
}

/// Process-wide Turbo-GET read budget (H1) — the read-path twin of
/// [`GLOBAL_TURBO_PUT_BUDGET`]. A single [`Semaphore`] shared by EVERY
/// [`TurboRouteState`] (and thus every tenant) in the process, sized to
/// [`GLOBAL_TURBO_GET_PERMITS`]. SEPARATE from the PUT budget so reads and writes
/// don't share one pool. Lazily initialised so it is a true singleton regardless
/// of how many routers are built.
static GLOBAL_TURBO_GET_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Accessor for the process-wide GET budget (H1).
fn global_turbo_get_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_TURBO_GET_BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURBO_GET_PERMITS))),
    )
}

/// Process-wide `/events` telemetry budget — SEPARATE from the PUT budget so a
/// telemetry flood cannot starve real cache writes (rt-nuclear cycle-2 #4/#8).
static GLOBAL_TURBO_EVENTS_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Accessor for the process-wide `/events` budget.
fn global_turbo_events_budget() -> Arc<Semaphore> {
    Arc::clone(
        GLOBAL_TURBO_EVENTS_BUDGET
            .get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURBO_EVENTS_PERMITS))),
    )
}

/// Build a fresh fixed-size array of per-object write locks (C1).
fn new_write_locks() -> Arc<Vec<AsyncMutex<()>>> {
    let mut v = Vec::with_capacity(TURBO_WRITE_LOCK_SHARDS);
    for _ in 0..TURBO_WRITE_LOCK_SHARDS {
        v.push(AsyncMutex::new(()));
    }
    Arc::new(v)
}

/// Stable shard index for an object's write lock (C1). Hashes the lock
/// identity tuple `(caller_tenant, team_id, hash)` — the tuple that maps to a
/// single stored R2 object — into `[0, TURBO_WRITE_LOCK_SHARDS)`.
fn write_lock_shard(tenant: &str, team_id: &str, hash: &str) -> usize {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tenant.hash(&mut h);
    0u8.hash(&mut h); // domain separator so ("a","b") ≠ ("ab","")
    team_id.hash(&mut h);
    0u8.hash(&mut h);
    hash.hash(&mut h);
    // TURBO_WRITE_LOCK_SHARDS is a power of two ⇒ the mask is exact. Mask in
    // u64 first; the result is `< TURBO_WRITE_LOCK_SHARDS` (1024) so it always
    // fits a usize on every target without a truncating cast.
    let mask = (TURBO_WRITE_LOCK_SHARDS as u64) - 1;
    usize::try_from(h.finish() & mask).unwrap_or(0)
}

impl core::fmt::Debug for TurboRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TurboRouteState")
            .field("put_inflight", &"Arc<Mutex<HashMap<..>>>")
            .finish_non_exhaustive()
    }
}

// ── Pre-buffer PUT concurrency guard (finding #2) ──────────────────────────────

/// RAII release of one per-tenant in-flight PUT slot.
///
/// Decrements the tenant's `put_inflight` count on `Drop`, so the slot is freed
/// on EVERY return path (success, handler error, panic). Carried out of the
/// [`PutConcurrencyGuard`] extractor into the handler so the slot stays held for
/// the whole request lifetime (including the body buffer + storage write).
pub(crate) struct PutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for PutSlot {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inflight.lock() {
            if let Some(c) = g.get_mut(&self.tenant_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    g.remove(&self.tenant_key);
                }
            }
        }
    }
}

/// `FromRequestParts` extractor that reserves a per-tenant in-flight PUT slot.
///
/// # Why an extractor (finding #2 — pre-buffer OOM guard)
///
/// The slot reservation USED to live inside `handle_put`, AFTER the
/// `body: axum::body::Bytes` extractor had already buffered up to
/// [`TURBO_BODY_LIMIT_BYTES`] (100 MiB) into the heap. So a burst of concurrent
/// PUTs each buffered ~100 MiB BEFORE the cap-check ran — `burst × 100 MiB` of
/// transient heap could OOM the shared container regardless of the cap.
///
/// As a [`FromRequestParts`] extractor this runs while only request **parts**
/// (headers/method/uri) are available — axum 0.7 runs every `FromRequestParts`
/// extractor BEFORE the single `FromRequest` body extractor (`Bytes`). Declaring
/// it AHEAD of `body: Bytes` in the handler signature therefore does the
/// lock+count+increment BEFORE a single body byte is read: an over-cap request
/// is rejected 429 with no buffering. The yielded [`PutSlot`] RAII-releases the
/// slot when the handler returns.
pub(crate) struct PutConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: PutSlot,
}

impl axum::extract::FromRequestParts<TurboRouteState> for PutConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // The isolation tenant is the DO-injected, PAT-resolved authenticated
        // tenant. A missing/empty header fails CLOSED (401) — the same gate the
        // `AuthTenant` extractor enforces; we mirror it here so the reservation
        // is per AUTHENTICATED tenant (an unauthenticated request never reserves
        // a slot, and never buffers a body).
        let tenant = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        // WP-I.3: reuse the canonical `auth_tenant::is_reserved_sentinel`
        // helper (single source of truth for the four sentinels: the
        // helper covers `_anonymous`, `_unknown`, `_system`, `_pending`
        // plus the two we were missing: `_oci` and `_public`). Three
        // previous local copies were drift-prone copy-paste.
        if tenant.is_empty() || crate::auth_tenant::is_reserved_sentinel(tenant) {
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        let tenant_key = tenant.to_owned();

        {
            let mut inflight = match state.put_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo PUT concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= TURBO_PUT_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = TURBO_PUT_CONCURRENCY_LIMIT,
                    "turbo PUT concurrency limit reached; returning 429 BEFORE body buffering"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent artifact uploads",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: PutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

// ── Global process-wide PUT budget guard (C5) ──────────────────────────────────

/// `FromRequestParts` extractor that reserves ONE permit from the
/// process-wide [`GLOBAL_TURBO_PUT_BUDGET`] semaphore (C5).
///
/// # Why an extractor (same reason as [`PutConcurrencyGuard`])
///
/// The per-tenant [`PutConcurrencyGuard`] bounds ONE tenant to
/// `TURBO_PUT_CONCURRENCY_LIMIT × TURBO_BODY_LIMIT_BYTES`, but with N tenants
/// the aggregate transient heap is `N × that` — N tenants can together OOM the
/// shared container. This extractor adds a SECOND, process-wide bound. As a
/// `FromRequestParts` extractor axum runs it BEFORE the `body: Bytes`
/// extractor, so the permit is reserved (or the request 503s) BEFORE any body
/// byte is buffered. The held [`OwnedSemaphorePermit`] RAII-releases the
/// permit when the handler returns (success, error, or panic).
///
/// Under global saturation we wait at most [`GLOBAL_PUT_PERMIT_WAIT`] for a
/// permit, then fail CLOSED with 503 (Service Unavailable) rather than block
/// the request forever — the client retries.
pub(crate) struct GlobalPutBudgetGuard {
    /// Held for the whole request; releases the permit on drop.
    _permit: OwnedSemaphorePermit,
}

impl axum::extract::FromRequestParts<TurboRouteState> for GlobalPutBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        let budget = global_turbo_put_budget();
        match tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, budget.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Self { _permit: permit }),
            // `acquire_owned` only errs if the semaphore is closed — we never
            // close it, so this is unreachable, but fail CLOSED if it happens.
            Ok(Err(_)) => Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global upload budget unavailable",
            )
                .into_response()),
            // Timed out waiting: the container is globally saturated.
            Err(_) => {
                tracing::warn!(
                    permits = GLOBAL_TURBO_PUT_PERMITS,
                    "turbo PUT global budget saturated; returning 503 BEFORE body buffering"
                );
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "server busy: too many concurrent uploads",
                )
                    .into_response())
            }
        }
    }
}

// ── Pre-buffer GET concurrency guard (H1 — read-path twin of PUT) ──────────────

/// RAII release of one per-tenant in-flight GET slot.
///
/// Decrements the tenant's `get_inflight` count on `Drop`, so the slot is freed
/// on EVERY return path (success, handler error, panic). Carried out of the
/// [`GetConcurrencyGuard`] extractor into the handler so the slot stays held for
/// the whole request lifetime (including the response-body buffer). Mirrors
/// [`PutSlot`].
pub(crate) struct GetSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for GetSlot {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inflight.lock() {
            if let Some(c) = g.get_mut(&self.tenant_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    g.remove(&self.tenant_key);
                }
            }
        }
    }
}

/// `FromRequestParts` extractor that reserves a per-tenant in-flight GET slot
/// (H1 — pre-buffer read-path OOM guard).
///
/// # Why an extractor (mirrors [`PutConcurrencyGuard`])
///
/// `handle_get` buffers the FULL artifact (up to [`TURBO_BODY_LIMIT_BYTES`] =
/// 100 MiB) into the heap before responding. Without a cap, a burst of
/// concurrent GETs each buffers ~100 MiB — `burst × 100 MiB` of transient heap
/// could OOM the shared container. The PUT path was hardened against exactly
/// this; the GET path was left asymmetrically open.
///
/// As a [`FromRequestParts`] extractor this runs while only request **parts**
/// (headers/method/uri) are available — BEFORE the handler ever touches storage
/// or buffers a response body. Declaring it AHEAD of any body work in the handler
/// signature therefore does the lock+count+increment BEFORE a single artifact
/// byte is read: an over-cap request is rejected 429 with no buffering. The
/// yielded [`GetSlot`] RAII-releases the slot when the handler returns. Like the
/// PUT guard it fails CLOSED (401) on a missing/sentinel tenant.
pub(crate) struct GetConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: GetSlot,
}

impl axum::extract::FromRequestParts<TurboRouteState> for GetConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // The isolation tenant is the DO-injected, PAT-resolved authenticated
        // tenant. A missing/empty/sentinel header fails CLOSED (401) — the same
        // gate `AuthTenant` and `PutConcurrencyGuard` enforce; we mirror it so the
        // reservation is per AUTHENTICATED tenant (an unauthenticated request
        // never reserves a slot, and never buffers an artifact).
        let tenant = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if tenant.is_empty() || crate::auth_tenant::is_reserved_sentinel(tenant) {
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        let tenant_key = tenant.to_owned();

        {
            let mut inflight = match state.get_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo GET concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= TURBO_GET_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = TURBO_GET_CONCURRENCY_LIMIT,
                    "turbo GET concurrency limit reached; returning 429 BEFORE body buffering"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent artifact downloads",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: GetSlot {
                inflight: Arc::clone(&state.get_inflight),
                tenant_key,
            },
        })
    }
}

// ── Global process-wide GET budget guard (H1) ──────────────────────────────────

/// `FromRequestParts` extractor that reserves ONE permit from the process-wide
/// [`GLOBAL_TURBO_GET_BUDGET`] semaphore (H1 — read-path twin of
/// [`GlobalPutBudgetGuard`]).
///
/// The per-tenant [`GetConcurrencyGuard`] bounds ONE tenant to
/// `TURBO_GET_CONCURRENCY_LIMIT × TURBO_BODY_LIMIT_BYTES`, but with N tenants the
/// aggregate transient read heap is `N × that` — N tenants can together OOM the
/// shared container. This extractor adds a SECOND, process-wide bound on a pool
/// SEPARATE from writes. As a `FromRequestParts` extractor axum runs it BEFORE
/// the handler buffers the response, so the permit is reserved (or the request
/// 503s) BEFORE any artifact byte is buffered. The held [`OwnedSemaphorePermit`]
/// RAII-releases when the handler returns.
///
/// Under global saturation we wait at most [`GLOBAL_PUT_PERMIT_WAIT`] for a
/// permit, then fail CLOSED with 503 rather than block forever — the client
/// retries.
pub(crate) struct GlobalGetBudgetGuard {
    /// Held for the whole request; releases the permit on drop.
    _permit: OwnedSemaphorePermit,
}

impl axum::extract::FromRequestParts<TurboRouteState> for GlobalGetBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        let budget = global_turbo_get_budget();
        match tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, budget.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Self { _permit: permit }),
            // `acquire_owned` only errs if the semaphore is closed — we never
            // close it, so this is unreachable, but fail CLOSED if it happens.
            Ok(Err(_)) => Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "global download budget unavailable",
            )
                .into_response()),
            // Timed out waiting: the container is globally saturated.
            Err(_) => {
                tracing::warn!(
                    permits = GLOBAL_TURBO_GET_PERMITS,
                    "turbo GET global budget saturated; returning 503 BEFORE body buffering"
                );
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "server busy: too many concurrent downloads",
                )
                    .into_response())
            }
        }
    }
}

/// Per-request guard over the SEPARATE `/events` telemetry budget
/// ([`GLOBAL_TURBO_EVENTS_PERMITS`]) — decoupled from the PUT write budget so a
/// telemetry flood / slow-body events POST cannot starve real cache writes
/// (rt-nuclear cycle-2 #4/#8). As a `FromRequestParts` extractor it acquires
/// (and 503s on saturation) BEFORE the `Bytes` body is buffered — preserving the
/// C5 OOM bound on events (≤ permits × `EVENTS_BODY_LIMIT_BYTES`) on its own pool.
pub(crate) struct EventsBudgetGuard {
    /// Held for the whole request; releases the permit on drop.
    _permit: OwnedSemaphorePermit,
}

impl axum::extract::FromRequestParts<TurboRouteState> for EventsBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        let budget = global_turbo_events_budget();
        match tokio::time::timeout(GLOBAL_PUT_PERMIT_WAIT, budget.acquire_owned()).await {
            Ok(Ok(permit)) => Ok(Self { _permit: permit }),
            // `acquire_owned` only errs if the semaphore is closed — never closed
            // here, but fail CLOSED if it ever is.
            Ok(Err(_)) => {
                Err((StatusCode::SERVICE_UNAVAILABLE, "events budget unavailable").into_response())
            }
            // Timed out: the events pool is saturated. A telemetry flood now 503s
            // ITS OWN route without touching the PUT write plane.
            Err(_) => {
                tracing::warn!(
                    permits = GLOBAL_TURBO_EVENTS_PERMITS,
                    "turbo /events budget saturated; returning 503 BEFORE body buffering"
                );
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "server busy: too many concurrent telemetry posts",
                )
                    .into_response())
            }
        }
    }
}

// ── Per-tenant /events concurrency guard (rt-nuclear cycle-2 #9) ───────────────

/// RAII release of one per-tenant in-flight `/events` slot.
///
/// Decrements the tenant's `events_inflight` count on `Drop`, so the slot is
/// freed on EVERY return path (success, handler error, panic, body-read
/// timeout). Carried out of the [`EventsConcurrencyGuard`] extractor into the
/// handler so the slot stays held for the whole request lifetime (including the
/// timed body read). Mirrors [`PutSlot`].
pub(crate) struct EventsSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for EventsSlot {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inflight.lock() {
            if let Some(c) = g.get_mut(&self.tenant_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    g.remove(&self.tenant_key);
                }
            }
        }
    }
}

/// `FromRequestParts` extractor that reserves a per-tenant in-flight `/events`
/// slot (rt-nuclear cycle-2 #9 — events fairness).
///
/// The process-wide [`EventsBudgetGuard`] bounds the events pool in aggregate,
/// but with no PER-TENANT cap one tenant could take all
/// [`GLOBAL_TURBO_EVENTS_PERMITS`] permits and starve every other tenant's
/// telemetry. This mirrors the PUT plane's [`PutConcurrencyGuard`]: as a
/// `FromRequestParts` extractor it runs BEFORE the body is read, caps the tenant
/// at [`EVENTS_CONCURRENCY_LIMIT`] in-flight (429 over it), and yields an
/// [`EventsSlot`] that RAII-releases the slot when the handler returns. Like the
/// PUT guard it fails CLOSED (401) on a missing/sentinel tenant.
pub(crate) struct EventsConcurrencyGuard {
    /// The reserved slot — released on drop. Held by the handler for the whole
    /// request (it is NOT dropped at the end of extraction).
    _slot: EventsSlot,
}

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
