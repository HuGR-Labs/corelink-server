impl axum::extract::FromRequestParts<TurboRouteState> for EventsConcurrencyGuard {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &TurboRouteState,
    ) -> Result<Self, Self::Rejection> {
        // Per AUTHENTICATED tenant — same fail-CLOSED tenant resolution as the
        // PUT guard (`PutConcurrencyGuard`) and `AuthTenant`: a missing/empty or
        // sentinel tenant 401s and never reserves a slot.
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
            let mut inflight = match state.events_inflight.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_key,
                        error = %e,
                        "turbo /events concurrency tracker mutex poisoned; failing closed"
                    );
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrency tracker unavailable",
                    )
                        .into_response());
                }
            };
            let count = inflight.entry(tenant_key.clone()).or_insert(0);
            if *count >= EVENTS_CONCURRENCY_LIMIT {
                tracing::warn!(
                    tenant_id = %tenant_key,
                    in_flight = *count,
                    limit = EVENTS_CONCURRENCY_LIMIT,
                    "turbo /events per-tenant concurrency limit reached; returning 429"
                );
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many concurrent telemetry posts for this tenant",
                )
                    .into_response());
            }
            *count += 1;
        }

        Ok(Self {
            _slot: EventsSlot {
                inflight: Arc::clone(&state.events_inflight),
                tenant_key,
            },
        })
    }
}

// ── Handler construction ──────────────────────────────────────────────────────

/// Build the [`TurboRouteState`] for the current build target and runtime.
///
/// # Runtime backing-store selection (v2)
///
/// When storage credentials are configured (`StorageEnv::from_env()`), the
/// handler is backed by the **durable** [`R2KvStore`](crate::storage::r2_kv::R2KvStore)
/// (closes the former `TODO(v2)`: artifacts now persist across container
/// restarts). Otherwise — dev / CI without secrets — it falls back to the
/// in-RAM [`InMemoryKvStore`]. Mirrors the `cas`/`ac` `build_handlers`
/// fallback convention; the route handlers and audit/validation logic are
/// identical for both backings (opaque-key semantics, no hash verify).
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn build_handlers() -> TurboRouteState {
    let audit = Arc::new(InMemoryTurboAuditSink::new());

    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{r2_kv, StorageEnv};
        if StorageEnv::from_env().is_some() {
            // `block_in_place` rationale: build_handlers runs inside the
            // `#[tokio::main]` multi-thread runtime; a bare `block_on`
            // from a running future hangs. Mirrors `cas`/`ac` handlers.
            let handle = tokio::runtime::Handle::current();
            let built =
                tokio::task::block_in_place(|| handle.block_on(r2_kv::build_r2_kv_from_env()));
            match built {
                Some(Ok(store)) => {
                    let store = Arc::new(store);
                    let read: Arc<dyn corelink_turbo_bridge::adapter::CasReadStore> = store.clone();
                    let write: Arc<dyn corelink_turbo_bridge::adapter::CasWriteStore> = store;
                    tracing::info!("Turbo handler: R2KvStore (durable storage)");
                    let handler: Arc<dyn TurboArtifactHandler> =
                        Arc::new(CasAdapterTurboHandler::new(read, write, audit));
                    return TurboRouteState {
                        handler,
                        quota: None,
                        pat_gate: None,
                        bytes: None,
                        put_inflight: Arc::new(Mutex::new(HashMap::new())),
                        get_inflight: Arc::new(Mutex::new(HashMap::new())),
                        events_inflight: Arc::new(Mutex::new(HashMap::new())),
                        write_locks: new_write_locks(),
                        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
                    };
                }
                Some(Err(e)) => {
                    // CAA-360 #7: storage creds ARE present but R2KvStore refused
                    // to build → do NOT silently fall back to the non-durable
                    // InMemory store (fail-OPEN → silent data loss). Mount the
                    // fail-CLOSED UnavailableTurboHandler so every verb 503s LOUDLY
                    // until storage is fixed (mirrors cas.rs UnavailableCasHandler).
                    tracing::error!(error = %e, "Turbo R2KvStore build failed; mounting fail-CLOSED 503 handler (audit #7)");
                    return TurboRouteState {
                        handler: Arc::new(UnavailableTurboHandler),
                        quota: None,
                        pat_gate: None,
                        bytes: None,
                        put_inflight: Arc::new(Mutex::new(HashMap::new())),
                        get_inflight: Arc::new(Mutex::new(HashMap::new())),
                        events_inflight: Arc::new(Mutex::new(HashMap::new())),
                        write_locks: new_write_locks(),
                        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
                    };
                }
                None => {}
            }
        }
    }

    tracing::info!("Turbo handler: InMemoryKvStore (no storage credentials configured)");
    let store = Arc::new(InMemoryKvStore::new());
    let handler: Arc<dyn TurboArtifactHandler> =
        Arc::new(CasAdapterTurboHandler::new(store.clone(), store, audit));
    TurboRouteState {
        handler,
        quota: None,
        pat_gate: None,
        bytes: None,
        put_inflight: Arc::new(Mutex::new(HashMap::new())),
        get_inflight: Arc::new(Mutex::new(HashMap::new())),
        events_inflight: Arc::new(Mutex::new(HashMap::new())),
        write_locks: new_write_locks(),
        // Inert placeholder; `build_with_factory` sets the shared, D1-backed meter.
        usage_meter: Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

/// Build the axum `Router` exposing all four `/v8/artifacts/*` routes.
///
/// Routes registered:
/// - `GET  /v8/artifacts/:hash`    → [`handle_get`]
/// - `PUT  /v8/artifacts/:hash`    → [`handle_put`]
/// - `POST /v8/artifacts/events`   → [`handle_events`]
/// - `POST /v8/artifacts/status`   → [`handle_status`]
///
/// The `events` and `status` routes must be registered BEFORE the `:hash`
/// wildcard route so matchit resolves the literals first.
pub fn router(state: TurboRouteState) -> Router {
    Router::new()
        // Fixed-path routes registered BEFORE the wildcard `:hash` routes so
        // matchit prefers the literal segments `events` / `status` over the
        // capture.
        //
        // C4: the events route is "accept-and-drop" telemetry with NO storage
        // and NO concurrency guard, so it must NOT inherit the 100 MiB artifact
        // body limit below. We layer its OWN tiny `EVENTS_BODY_LIMIT_BYTES`
        // (64 KiB) directly on the route handler — axum honours the INNERMOST
        // `DefaultBodyLimit`, so this per-route layer overrides the outer 100
        // MiB default for `/events` only, and axum rejects oversized telemetry
        // (413) BEFORE buffering it. The artifact GET/PUT and the static
        // `status` route keep the 100 MiB limit.
        .route(
            TURBO_EVENTS_ROUTE,
            post(handle_events).layer(axum::extract::DefaultBodyLimit::max(
                EVENTS_BODY_LIMIT_BYTES,
            )),
        )
        .route(TURBO_STATUS_ROUTE, post(handle_status))
        .route(TURBO_GET_ROUTE, get(handle_get).put(handle_put))
        // Per-route body cap: Turbo build artifacts are legitimately larger than
        // the 10 MiB global limit set in `main.rs`. This inner `DefaultBodyLimit`
        // layer overrides the outer global default for the `/v8/artifacts/*`
        // routes only (axum honours the innermost limit) while still bounding the
        // body at 100 MiB so a PAT cannot OOM the shared container. The `events`
        // route above carries its own (smaller, innermost) limit so this does
        // NOT widen its cap back to 100 MiB.
        .layer(axum::extract::DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES))
        .with_state(state)
}

// ── Route handlers ────────────────────────────────────────────────────────────

/// `GET /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Returns 200 + raw artifact bytes on hit, 404 on miss, 400 on bad params,
/// 403 on cross-tenant, 503 on audit-closed.
#[allow(
    clippy::too_many_arguments,
    reason = "axum handler — every parameter is a request extractor (State / Path / Query / headers + the GET concurrency guards); not a refactorable argument list. Mirrors handle_put."
)]
async fn handle_get(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // H1: the per-tenant read concurrency reservation is a `FromRequestParts`
    // extractor (lock+count+increment, 429 on over-cap). axum runs every
    // `FromRequestParts` extractor during extraction — BEFORE the handler body
    // runs and buffers the artifact — so an over-cap GET is rejected 429 BEFORE
    // the (up to 100 MiB) artifact is read into the heap. Holding `_concurrency`
    // for the whole handler keeps the slot reserved until return; its `GetSlot`
    // RAII-releases on drop. Mirrors `handle_put`'s `PutConcurrencyGuard`.
    _concurrency: GetConcurrencyGuard,
    // H1: the process-wide GET budget — a SECOND `FromRequestParts` extractor. It
    // reserves one of `GLOBAL_TURBO_GET_PERMITS` global permits (503 on global
    // saturation) BEFORE the artifact is buffered, bounding aggregate
    // cross-tenant read heap on a pool SEPARATE from writes. The held permit
    // RAII-releases when the handler returns. Mirrors `GlobalPutBudgetGuard`.
    _global_budget: GlobalGetBudgetGuard,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): Turbo GET is a cache READ — require
    // `cas:rw` or `cas:r`. BEFORE any audit or storage. NO-OP for `cas:rw`.
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Isolation tenant is the DO-injected, PAT-resolved authenticated tenant
    // (`auth.0`) — NOT the client `teamId`.  `teamId` is a Turborepo team label
    // demoted to a logical sub-namespace inside the authenticated tenant.
    let caller_tenant = auth.0;
    // DoS / key-aliasing guard: reject empty / overlong / `/`-bearing `teamId`
    // (it is interpolated into the storage key) BEFORE any audit or storage.
    // Mirrors the MAX_HASH_LEN guard; maps to 400 via `map_err`.
    if let Err(e) = validate_team_id(&params.team_id) {
        return map_err(e);
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(resp) = gate.verify(&caller_tenant, bearer).await {
            return resp;
        }
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost, AFTER the scope gate, BEFORE storage. 402 over-ceiling /
    // 503 fail-CLOSED. `None` in dev/CI ⇒ not enforced.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&caller_tenant).await {
            return resp;
        }
    }
    let now_ms = SystemWallClock.now_ms();
    // usage-metering-roi: keep the tenant for the fire-and-forget meter record
    // below — `caller_tenant` is moved into the request constructor.
    let meter_tenant = caller_tenant.clone();
    let req = TurboGetRequest::new(
        hash,
        params.team_id,
        params.slug,
        // Principal is filled in by the auth middleware in production;
        // Phase 0 uses a placeholder.
        format!("anon@{caller_tenant}"),
        caller_tenant,
        now_ms,
    );
    match state.handler.get(req) {
        Ok(resp) => {
            // usage-metering-roi: artifact GET HIT (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&meter_tenant, crate::usage_meter::UsageEvent::ReadHit);
            // F-009 / WP-9a: echo the stored Turborepo signature so a client
            // with `TURBO_REMOTE_CACHE_SIGNATURE_KEY` can verify the artifact.
            // `None` ⇒ the artifact was stored unsigned: omit the header
            // entirely and serve the bytes exactly as before. Never invent one.
            //
            // The value was validated as printable ASCII both on the way in and
            // on the way back out of storage, so `from_str` cannot realistically
            // fail here; if it somehow does, fail CLOSED (500) rather than serve
            // a hit whose signature we silently dropped.
            let tag_header = match resp.artifact_tag.as_deref() {
                None => None,
                Some(tag) => match axum::http::HeaderValue::from_str(tag) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "turbo: stored artifact tag is not a legal header value; failing closed"
                        );
                        return (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response();
                    }
                },
            };
            let mut response = (StatusCode::OK, resp.bytes).into_response();
            if let Some(v) = tag_header {
                response.headers_mut().insert(ARTIFACT_TAG_HEADER, v);
            }
            response
        }
        Err(e) => {
            // usage-metering-roi: a genuine NotFound is a GET MISS; other errors
            // are faults, not classified ops.
            if matches!(e, corelink_turbo_bridge::TurboBridgeError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&meter_tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_err(e)
        }
    }
}
