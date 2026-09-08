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
