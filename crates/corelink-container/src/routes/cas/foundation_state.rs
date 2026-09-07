/// Shared route state — distinct trait objects for read and write.
#[derive(Clone)]
#[non_exhaustive]
pub struct CasRouteState {
    /// Production wiring builds these `Arc<dyn ...Handler>` values
    /// from the appropriate native or wasm32 impl; see [`build_handlers`].
    pub read: Arc<dyn CasReadHandler>,
    /// Write handler (separate trait object — see crate-level docs).
    pub write: Arc<dyn CasWriteHandler>,
    /// Delete handler (D-8) — write-capable. In production this points at
    /// the same shared handler instance as `read`/`write`.
    pub delete: Arc<dyn CasDeleteHandler>,
    /// List handler (D-8) — read-capable paginated blob enumeration.
    pub list: Arc<dyn CasListHandler>,
    /// Optional 410-Gone tombstone store (hugit-P2 seam B, WP-B). When present,
    /// `GET` consults it FIRST: an erased `(tenant, hash)` short-circuits to
    /// HTTP 410 Gone (never 404, never 200) before the R2 lookup. `None` (the
    /// default — in-memory / test builds, and prod until PR #254's R2 erase
    /// adapter lands) ⇒ no tombstone gate, classic 200/404 behaviour. Backed by
    /// [`crate::routes::cas_erase::D1TombstoneStore`] in prod.
    pub tombstones: Option<Arc<dyn crate::routes::cas_erase::TombstoneStore>>,
    /// Optional per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    /// `Some` in production (D1-backed) — checked at the TOP of each billable
    /// handler, AFTER the scope gate, BEFORE storage. `None` in dev/CI (no D1)
    /// — the ceiling is simply not enforced. See [`crate::tenant_quota`].
    pub quota: Option<crate::routes::QuotaGate>,
    /// Optional native PAT possession gate (red-team finding #4 — defense-in-
    /// depth). `Some` in production (PAT_SIGNING_KEY + D1) — re-runs the full
    /// Argon2id Option-B verify at the TOP of each billable handler, AFTER the
    /// scope+tenant gate, BEFORE storage, so a leaked `PAT_SIGNING_KEY` cannot
    /// serve a forged tenant's PAT. `None` in dev/CI (skipped). See
    /// [`crate::native_pat_gate`].
    pub pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    /// Per-tenant in-flight CAS write concurrency counter (cluster F — the native
    /// CAS write path buffers the full body before any gate and had NO per-tenant
    /// concurrency cap, so a single authenticated tenant could open N concurrent
    /// PUTs and consume N × body-limit of heap). Mirrors the Bazel `put_inflight`
    /// guard: a `FromRequestParts` extractor ([`CasPutGuard`]) declared AHEAD of
    /// `body: Bytes` increments this BEFORE the body is buffered and rejects the
    /// over-cap PUT 429; the RAII [`CasPutSlot`] releases on return.
    pub(crate) put_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-tenant in-flight CAS bulk-READ concurrency counter (brutal-fleet M2 —
    /// MED DoS). The `batch-read` path accumulates up to [`BATCH_MAX_BYTES`] of
    /// object bytes into a per-request payload buffer, and `batch-exists` fans out
    /// up to [`BATCH_MAX_OBJECTS`] storage existence probes — neither held a
    /// per-tenant concurrency cap, so one authenticated tenant could open N
    /// concurrent bulk reads and consume N × payload heap (≈ N × 8 MiB) on the
    /// shared container. A [`CasReadConcurrencyGuard`] `FromRequestParts` extractor
    /// declared AHEAD of `body: Bytes` increments this BEFORE the body is buffered
    /// and rejects the over-cap read 429; the RAII [`CasReadSlot`] releases on
    /// return. This is a SEPARATE pool from [`Self::put_inflight`] — reads and
    /// writes do not contend on one counter (sharing would over-throttle a tenant
    /// that legitimately reads and writes concurrently).
    pub(crate) read_inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Display usage aggregator (usage-metering-roi). The read-HIT / read-MISS /
    /// write decision points `record` fire-and-forget into this meter — a cheap
    /// lock+increment, NEVER a DB call / `await` on the hot path. Inert (`record`
    /// is a no-op) in dev/CI; `build_with_factory` sets the ONE process-wide
    /// meter shared across the instrumented cache surfaces. See
    /// [`crate::usage_meter`].
    pub usage_meter: Arc<crate::usage_meter::UsageMeter>,
    // Storage byte accounting (red-team finding #1 / cluster B+C) is NOT a route
    // field: it is enforced INSIDE the `write`/`delete` trait objects above by
    // the [`crate::byte_accounting::AccountingCasHandler`] decorator (wired in
    // `routes::build_with_factory`), so EVERY surface that shares these handlers
    // — native CAS, Bazel, OCI, cargo/brew/npm/pip — gets identical, atomic,
    // reserve→commit→release accounting with no per-route plumbing.
}

impl CasRouteState {
    /// Construct a [`CasRouteState`] from its public collaborators, initializing
    /// the crate-private per-tenant in-flight write counter ([`Self::put_inflight`])
    /// to empty. This is the supported constructor for callers OUTSIDE the crate
    /// (e.g. integration smoke tests) that cannot name the `pub(crate)` field.
    #[must_use]
    pub fn new(
        read: Arc<dyn CasReadHandler>,
        write: Arc<dyn CasWriteHandler>,
        delete: Arc<dyn CasDeleteHandler>,
        list: Arc<dyn CasListHandler>,
        tombstones: Option<Arc<dyn crate::routes::cas_erase::TombstoneStore>>,
        quota: Option<crate::routes::QuotaGate>,
        pat_gate: Option<std::sync::Arc<crate::native_pat_gate::NativePatGate>>,
    ) -> Self {
        Self {
            read,
            write,
            delete,
            list,
            tombstones,
            quota,
            pat_gate,
            put_inflight: Arc::new(Mutex::new(HashMap::new())),
            read_inflight: Arc::new(Mutex::new(HashMap::new())),
            // Inert placeholder for external callers; the production wiring in
            // `build_with_factory` sets the shared, D1-backed meter.
            usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
        }
    }
}

impl core::fmt::Debug for CasRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasRouteState").finish_non_exhaustive()
    }
}

/// Maximum concurrent in-flight native CAS writes for a single tenant. Excess
/// writes are rejected 429 BEFORE the body is buffered. Bounds peak per-tenant
/// write heap; mirrors [`super::bazel_v2::BAZEL_WRITE_CONCURRENCY_LIMIT`].
pub const CAS_WRITE_CONCURRENCY_LIMIT: usize = 8;

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors `auth_tenant::AuthTenant`'s sentinel set so the pre-body write guard
/// fails CLOSED on the same non-authenticated values.
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

/// Canonical fail-CLOSED 401 for a missing/sentinel authenticated tenant in the
/// pre-body write guard. Does not leak which condition tripped.
fn unauthenticated_tenant() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
}

/// RAII release of one per-tenant in-flight CAS write slot (decrements on every
/// return path — success, error, panic). See [`CasPutGuard`].
pub(crate) struct CasPutSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for CasPutSlot {
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

/// `FromRequestParts` extractor reserving a per-tenant in-flight CAS write slot
/// BEFORE the body is buffered (cluster F — pre-buffer OOM guard). Declared ahead
/// of `body: Bytes` in `handle_write` so axum 0.7 runs it first: a tenant already
/// at [`CAS_WRITE_CONCURRENCY_LIMIT`] is rejected 429 with no body read. Mirrors
/// `bazel_v2::BazelPutGuard` exactly.
pub(crate) struct CasPutGuard {
    _slot: CasPutSlot,
}

impl FromRequestParts<CasRouteState> for CasPutGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Reserve per AUTHENTICATED tenant (fail-CLOSED on missing/sentinel —
        // mirrors `AuthTenant`): an unauthenticated request never reserves a slot
        // and never buffers a body.
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty()
            || raw.len() > crate::auth_tenant::MAX_TENANT_ID_BYTES
            || TENANT_SENTINELS.contains(&raw)
        {
            return Err(unauthenticated_tenant());
        }
        let tenant_key = raw.to_owned();
        {
            let mut inflight = match state.put_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(tenant_id = %tenant_key, error = %e, "cas write concurrency tracker poisoned; failing closed");
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= CAS_WRITE_CONCURRENCY_LIMIT {
                tracing::warn!(tenant_id = %tenant_key, in_flight = *count, limit = CAS_WRITE_CONCURRENCY_LIMIT, "cas write concurrency limit reached; 429 BEFORE body buffering");
                return Err(
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent uploads").into_response(),
                );
            }
            *count += 1;
        }
        Ok(Self {
            _slot: CasPutSlot {
                inflight: Arc::clone(&state.put_inflight),
                tenant_key,
            },
        })
    }
}

/// Maximum concurrent in-flight native CAS READs for a single tenant — both the
/// bulk routes (`batch-read` / `batch-exists`) and, since B-052, the single-object
/// `GET /v1/cas/:tenant/:hash`. Excess reads are rejected 429 before any object is
/// buffered. Bounds peak per-tenant read-payload heap (`batch-read` accumulates up
/// to [`BATCH_MAX_BYTES`] per request; a single GET buffers one whole object).
///
/// Single and bulk reads deliberately share ONE pool. Giving the single GET its
/// own counter would let a tenant hold `2 x 8` concurrent reads and defeat the
/// ceiling this constant exists to impose — the pool is the bound, not the route.
/// Still a SEPARATE axis from the write cap [`CAS_WRITE_CONCURRENCY_LIMIT`] so
/// reads and writes do not over-throttle each other; same per-tenant ceiling (8).
pub const CAS_READ_CONCURRENCY_LIMIT: usize = 8;

/// RAII release of one per-tenant in-flight CAS bulk-READ slot (decrements on
/// every return path — success, error, panic). Mirrors [`CasPutSlot`] against the
/// SEPARATE `read_inflight` pool. See [`CasReadConcurrencyGuard`].
pub(crate) struct CasReadSlot {
    inflight: Arc<Mutex<HashMap<String, usize>>>,
    tenant_key: String,
}

impl Drop for CasReadSlot {
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

/// `FromRequestParts` extractor reserving a per-tenant in-flight CAS bulk-READ
/// slot BEFORE the body is buffered (brutal-fleet M2 — pre-buffer DoS guard).
/// Declared ahead of `body: Bytes` in `handle_batch_read` / `handle_batch_exists`
/// so axum 0.7 runs it first: a tenant already at [`CAS_READ_CONCURRENCY_LIMIT`]
/// concurrent bulk reads is rejected 429 with no body read. Mirrors [`CasPutGuard`]
/// exactly but against the SEPARATE [`CasRouteState::read_inflight`] pool — bulk
/// reads and writes are bounded on independent counters (sharing one pool would
/// over-throttle a tenant that legitimately reads and writes at the same time).
pub(crate) struct CasReadConcurrencyGuard {
    _slot: CasReadSlot,
}

impl FromRequestParts<CasRouteState> for CasReadConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Reserve per AUTHENTICATED tenant (fail-CLOSED on missing/sentinel —
        // mirrors `AuthTenant` / `CasPutGuard`): an unauthenticated request never
        // reserves a slot and never buffers a body.
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty()
            || raw.len() > crate::auth_tenant::MAX_TENANT_ID_BYTES
            || TENANT_SENTINELS.contains(&raw)
        {
            return Err(unauthenticated_tenant());
        }
        let tenant_key = raw.to_owned();
        {
            let mut inflight = match state.read_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(tenant_id = %tenant_key, error = %e, "cas read concurrency tracker poisoned; failing closed");
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= CAS_READ_CONCURRENCY_LIMIT {
                tracing::warn!(tenant_id = %tenant_key, in_flight = *count, limit = CAS_READ_CONCURRENCY_LIMIT, "cas bulk-read concurrency limit reached; 429 BEFORE body buffering");
                return Err(
                    (StatusCode::TOO_MANY_REQUESTS, "too many concurrent reads").into_response()
                );
            }
            *count += 1;
        }
        Ok(Self {
            _slot: CasReadSlot {
                inflight: Arc::clone(&state.read_inflight),
                tenant_key,
            },
        })
    }
}

/// Process-wide weighted reservation for one CAS object read. The permit is
/// held until the handler returns, covering the full response-buffer lifetime.
pub(crate) struct GlobalCasReadBudgetGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasReadBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_read_budget(CAS_READ_SINGLE_PERMITS, "single").await?,
        })
    }
}

/// Process-wide reservation for a `batch-read` response. It is acquired before
/// the request body extractor, so a saturated container rejects before parsing
/// or materialising a batch.
pub(crate) struct GlobalCasBatchReadBudgetGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
    _admission: tokio::sync::OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasBatchReadBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        // The batch envelope is intentionally admitted through a second,
        // weighted gate. The handler later acquires the full single-object
        // reservation while this 22 MiB envelope remains held; the admission
        // cap prevents two envelopes plus one object from exceeding the
        // process-wide 220 MiB slice.
        let admission = acquire_global_cas_batch_read_admission().await?;
        Ok(Self {
            _permit: acquire_global_cas_read_budget(CAS_READ_BATCH_PERMITS, "batch-read").await?,
            _admission: admission,
        })
    }
}

/// Process-wide reservation for a single CAS write, acquired before the body
/// extractor buffers request bytes.
pub(crate) struct GlobalCasWriteBudgetGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasWriteBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_write_budget(CAS_WRITE_SINGLE_PERMITS, "single").await?,
        })
    }
}

/// Process-wide reservation for a length-framed CAS batch write.
pub(crate) struct GlobalCasBatchWriteBudgetGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl FromRequestParts<CasRouteState> for GlobalCasBatchWriteBudgetGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &CasRouteState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            _permit: acquire_global_cas_write_budget(CAS_WRITE_BATCH_PERMITS, "batch").await?,
        })
    }
}

/// Sentinel carried in [`CasHandlerError::Internal`] by
/// [`UnavailableCasHandler`] so [`map_err`] can map the storage-unavailable
/// condition to **503** (rather than the generic 500 the `Internal` wildcard
/// arm yields). The handler-error enum lives in a sibling crate and is
/// `#[non_exhaustive]` without a dedicated `Unavailable` variant, so we thread
/// the distinction through `Internal` with this exact marker prefix.
const STORAGE_UNAVAILABLE_SENTINEL: &str = "storage-unavailable: ";

/// Fail-CLOSED stand-in handler mounted in place of `InMemoryCasHandler` when
/// storage credentials ARE present but the R2 handler refused to build —
/// today exactly the "R2_TDK_HEX required" case (F1, CAA-360).
///
/// A silent `InMemory` fallback there would serve a NON-durable cache with no
/// alarm (fail-OPEN-ish). Instead every read/write through this handler fails
/// CLOSED + LOUD: it returns [`CasHandlerError::Internal`] carrying the
/// [`STORAGE_UNAVAILABLE_SENTINEL`], which [`map_err`] maps to HTTP 503 — the
/// route is unavailable until `R2_TDK_HEX` is set. Distinct from the
/// creds-ABSENT path (`None` ⇒ dev/CI `InMemory`, which is fine).
#[derive(Debug)]
struct UnavailableCasHandler;

impl CasReadHandler for UnavailableCasHandler {
    fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasWriteHandler for UnavailableCasHandler {
    fn write(&self, _req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasDeleteHandler for UnavailableCasHandler {
    fn delete(&self, _req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}

impl CasListHandler for UnavailableCasHandler {
    fn list(&self, _req: CasListRequest) -> Result<CasListResponse, CasHandlerError> {
        Err(CasHandlerError::Internal(format!(
            "{STORAGE_UNAVAILABLE_SENTINEL}R2 CAS handler refused to build (R2_TDK_HEX unset/invalid)"
        )))
    }
}
