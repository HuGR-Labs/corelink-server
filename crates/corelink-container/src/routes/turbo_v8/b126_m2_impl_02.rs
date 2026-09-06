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

/// `PUT /v8/artifacts/:hash?teamId=<team_id>&slug=<slug>`
///
/// Accepts raw `application/octet-stream` body.  Returns 200 +
/// `{"urls": [...]}` on success.
#[allow(
    clippy::too_many_arguments,
    reason = "axum handler — every parameter is a request extractor (State / Path / Query / headers / body); they are not a refactorable argument list. Mirrors tier_select.rs / audit_export/stream.rs handler allows."
)]
async fn handle_put(
    State(state): State<TurboRouteState>,
    Path(hash): Path<String>,
    Query(params): Query<ArtifactQuery>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // finding #2: the concurrency reservation is a `FromRequestParts` extractor
    // declared AHEAD of `body: Bytes`, so axum runs it (lock+count+increment,
    // 429 on over-cap) BEFORE the body is buffered into the heap. Holding
    // `_concurrency` for the whole handler keeps the slot reserved until return;
    // its `PutSlot` RAII-releases on drop.
    _concurrency: PutConcurrencyGuard,
    // C5: the process-wide PUT budget — a SECOND `FromRequestParts` extractor
    // (so it too runs BEFORE `body: Bytes`). It reserves one of
    // `GLOBAL_TURBO_PUT_PERMITS` global permits (503 on global saturation)
    // BEFORE the body is buffered, bounding aggregate cross-tenant heap. The
    // held permit RAII-releases when the handler returns.
    _global_budget: GlobalPutBudgetGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): Turbo PUT is a cache WRITE — require
    // `cas:rw`. A read-only (`cas:r`) token is rejected here. NO-OP for
    // current `cas:rw` traffic.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Isolation tenant is the authenticated `auth.0`, not the client `teamId`.
    let caller_tenant = auth.0;
    // DoS / key-aliasing guard on `teamId` (storage-key prefix) before any
    // audit or storage. Mirrors the MAX_HASH_LEN guard; maps to 400.
    if let Err(e) = validate_team_id(&params.team_id) {
        return map_err(e);
    }
    // Native PAT possession gate (finding #4) — AFTER scope, BEFORE storage.
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(resp) = gate.verify_write(&caller_tenant, bearer).await {
            return resp;
        }
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1) — see
    // `handle_get`. AFTER the scope gate, BEFORE storage.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check(&caller_tenant).await {
            return resp;
        }
    }

    // C1 — per-object write serialization. Acquire the per-(tenant, team, hash)
    // async lock BEFORE `accrue` and hold it through the `handler.put`
    // (probe→put inside `R2KvStore::write`) and the `release`/reconcile below.
    // This makes the whole reserve→probe→commit→release sequence atomic per
    // STORED object: two concurrent same-key PUTs no longer both observe the
    // same `prior_len` and double-release it (which underflowed `bytes_used`
    // while real storage was unchanged ⇒ unbounded free storage). Different
    // objects hash to different shards and stay concurrent. The lock identity
    // matches the storage object exactly: `tenant = caller_tenant`, storage key
    // = `"<team_id>/<hash>"` (see `corelink_turbo_bridge::adapter` +
    // `R2KvStore::object_key`). Held to the end of the handler via `_write_lock`.
    let shard = write_lock_shard(&caller_tenant, &params.team_id, &hash);
    // `shard` is masked into `[0, TURBO_WRITE_LOCK_SHARDS)` and `write_locks`
    // has exactly that many entries, so `get` is always `Some`; `.get()` (vs
    // indexing) keeps the workspace `indexing_slicing = deny` lint satisfied.
    // The `None` arm is structurally unreachable — fail CLOSED 503 rather than
    // proceed unserialized (which would reopen the C1 double-release window).
    let Some(lock_cell) = state.write_locks.get(shard) else {
        tracing::error!(shard, "turbo write-lock shard out of range (unreachable)");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "write serialization unavailable",
        )
            .into_response();
    };
    let _write_lock = lock_cell.lock().await;

    let now_ms = SystemWallClock.now_ms();
    let byte_len = i64::try_from(body.len()).unwrap_or(i64::MAX);
    // Storage byte accounting (finding #1 / cluster-C): Turbo writes the artifact
    // to R2 directly via its OWN `R2KvStore` (NOT the shared `CasWriteHandler`, so
    // the `AccountingCasHandler` decorator does not cover it) — so the
    // reserve→commit→release discipline is applied HERE at the route. RESERVE
    // BEFORE `handler.put` so an over-cap PUT is rejected 402 BEFORE the R2 write
    // and no over-cap artifact is committed; a reservation fault ⇒ 503
    // fail-CLOSED. (Turbo's KV is opaque-keyed with no durable/idempotent bit, so
    // every stored PUT is charged; an inner failure releases the reservation.)
    if let Some(acc) = state.bytes.as_ref() {
        // The Worker-resolved per-tier cap (server-trusted header) seeds a fresh
        // `tenant_storage_state` row; `None` ⇒ fail-CLOSED on an unseeded tenant.
        let quota_seed = crate::byte_accounting::storage_quota_from_headers(&headers);
        match acc.accrue(&caller_tenant, byte_len, quota_seed).await {
            Ok(crate::byte_accounting::AccrueOutcome::Accrued) => {}
            Ok(crate::byte_accounting::AccrueOutcome::OverCap) => {
                return (StatusCode::PAYMENT_REQUIRED, "storage quota exceeded").into_response();
            }
            Ok(crate::byte_accounting::AccrueOutcome::Indeterminate) => {
                tracing::error!(
                    tenant = %caller_tenant,
                    "turbo: storage cap indeterminate for an unseeded tenant; failing closed"
                );
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage accounting unavailable",
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!(error = %e, "turbo: byte reservation failed; failing closed");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage accounting unavailable",
                )
                    .into_response();
            }
        }
    }
    let req = TurboPutRequest::new(
        hash,
        params.team_id,
        params.slug,
        body.to_vec(),
        None, // duration_ms — Phase 0: not parsed from headers
        format!("anon@{caller_tenant}"),
        caller_tenant.clone(),
        now_ms,
    )
    // F-009 / WP-9a: carry the Turborepo signature through. A header that is
    // present but not valid UTF-8 is treated as PRESENT-and-malformed (an empty
    // `&str` fails `validate_artifact_tag`) rather than as absent, so a
    // mangled tag is refused 400 instead of being silently dropped.
    .with_artifact_tag(
        headers
            .get(ARTIFACT_TAG_HEADER)
            .map(|v| v.to_str().unwrap_or("").to_owned()),
    );
    match state.handler.put(req) {
        Ok(resp) => {
            // usage-metering-roi: artifact PUT is a WRITE op (fire-and-forget, no
            // await/I/O on the hot path).
            state
                .usage_meter
                .record(&caller_tenant, crate::usage_meter::UsageEvent::Write);
            // rt34 finding #3/#4/#5/#6: Turbo keys are OPAQUE/client-chosen (NOT
            // content-addressed), so an overwrite can change the stored SIZE. We
            // already accrued the full new body length (`byte_len`) above —
            // keeping that preserves the over-cap fail-closed check. Now reconcile
            // `bytes_used` to the TRUE on-disk delta: release the PRIOR size on an
            // overwrite (`prior_len = Some(n)`), netting `old + new - prior` (the
            // correct new total, since `old >= prior` held inductively). A fresh
            // insert (`prior_len == None`) releases nothing — the full new charge
            // stays. The OLD all-or-nothing rollback released the FULL new
            // reservation on ANY overwrite, letting a tenant store unbounded bytes
            // for free (PUT 1 byte, then PUT 100 MiB under the same key ⇒ released
            // the 100 MiB while it stayed on disk).
            if let Some(prior) = resp.prior_len {
                let prior_i64 = i64::try_from(prior).unwrap_or(i64::MAX);
                if prior_i64 > 0 {
                    if let Some(acc) = state.bytes.as_ref() {
                        if let Err(re) = acc.release(&caller_tenant, prior_i64).await {
                            tracing::warn!(error = %re, "turbo: prior-size release on overwrite failed (over-counts; conservative)");
                        }
                    }
                }
            }
            let body = PutArtifactResponse { urls: resp.urls };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => {
            // The R2 write failed AFTER we reserved — RELEASE the reservation so a
            // failed PUT does not permanently consume the tenant's headroom.
            if let Some(acc) = state.bytes.as_ref() {
                if let Err(re) = acc.release(&caller_tenant, byte_len).await {
                    tracing::warn!(error = %re, "turbo: reservation release after failed PUT failed (over-counts; conservative)");
                }
            }
            map_err(e)
        }
    }
}

/// `POST /v8/artifacts/events`
///
/// Accept-and-drop: any body accepted, always returns 200.  Turbo telemetry
/// is not CoreLink's to own — no state mutation, no audit emit.
///
/// F3 (defense-in-depth): require the `AuthTenant` extractor — fail-CLOSED 401
/// on a missing/sentinel tenant. The Worker PAT-gates this path, but the
/// container must not trust that; an unauthenticated request never reaches the
/// handler. `AuthTenant` is `FromRequestParts`, so it precedes the body
/// extractor (axum 0.7 ordering rule).
///
/// # Fairness hardening (rt-nuclear cycle-2 #8 + #9)
///
/// `/events` holds a process-wide [`EventsBudgetGuard`] permit AND a per-tenant
/// [`EventsConcurrencyGuard`] slot for the WHOLE request. Two residual fairness
/// gaps are closed here:
///
/// - **#8 (slow-body):** the body is buffered manually under an
///   [`EVENTS_BODY_READ_TIMEOUT`] deadline rather than via the unbounded
///   `Bytes` extractor, so a slowloris dribbling its body can no longer pin a
///   permit + slot indefinitely — a stalled body aborts with 408 and the
///   permit/slot RAII-release when the handler returns. The body is still read
///   under the [`EVENTS_BODY_LIMIT_BYTES`] (64 KiB) cap (413 over it), so the
///   C4 OOM bound is preserved.
/// - **#9 (per-tenant cap):** the [`EventsConcurrencyGuard`] extractor (a
///   `FromRequestParts`, so it runs before the body is read) bounds one tenant
///   to [`EVENTS_CONCURRENCY_LIMIT`] in-flight events POSTs (429 over it),
///   mirroring the PUT plane's per-tenant guard — so one tenant cannot
///   monopolise the shared events pool.
async fn handle_events(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    // C5 + rt-nuclear cycle-2 #4/#8: events holds a permit from its OWN
    // dedicated budget (`GLOBAL_TURBO_EVENTS_PERMITS`), NOT the PUT write budget.
    // Sharing the PUT budget (the old C5 design) let a telemetry flood / slow-body
    // events POST hold PUT permits and starve real cache writes (cross-plane DoS).
    // As a `FromRequestParts` extractor it runs (and 503s on saturation) BEFORE
    // the body is read; combined with the 64 KiB `EVENTS_BODY_LIMIT_BYTES` cap
    // (C4) this keeps the same OOM bound (≤ permits × 64 KiB) on the events pool
    // while isolating it from writes.
    _events_budget: EventsBudgetGuard,
    // rt-nuclear cycle-2 #9: per-tenant events concurrency cap. A
    // `FromRequestParts` extractor (runs before the body) capping one tenant to
    // `EVENTS_CONCURRENCY_LIMIT` in-flight (429 over it) so no tenant hogs the
    // pool. Held for the whole handler; its `EventsSlot` RAII-releases on return.
    _events_concurrency: EventsConcurrencyGuard,
    // #8: take the RAW body (not the unbounded `Bytes` extractor) so the body
    // read happens INSIDE the handler under an `EVENTS_BODY_READ_TIMEOUT`
    // deadline — a slow/stalled body is aborted (408) rather than pinning the
    // permit + slot it holds.
    body: axum::body::Body,
) -> impl IntoResponse {
    // #8: bound the body read by wall-time AND by the events body cap (C4). On a
    // stalled body the timeout fires (408) and the handler returns, dropping the
    // held permit (`EventsBudgetGuard`) and per-tenant slot (`EventsSlot`); on an
    // over-cap body `to_bytes` errors and we map it to 413 — preserving the
    // existing 64 KiB cap behaviour without relying on the `Bytes` extractor.
    let bytes = match tokio::time::timeout(
        EVENTS_BODY_READ_TIMEOUT,
        axum::body::to_bytes(body, EVENTS_BODY_LIMIT_BYTES),
    )
    .await
    {
        Ok(Ok(b)) => b,
        // Body exceeded the events cap (or a transport error) — reject 413,
        // matching the prior `DefaultBodyLimit`-driven behaviour. Failing CLOSED
        // (413) rather than silently accepting keeps the OOM bound intact.
        Ok(Err(_)) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "events body too large or unreadable",
            )
                .into_response();
        }
        // #8: the body did not finish streaming within the deadline — a
        // slowloris. Abort 408; the permit + per-tenant slot release on return.
        Err(_) => {
            tracing::warn!(
                tenant_id = %auth.0,
                timeout_secs = EVENTS_BODY_READ_TIMEOUT.as_secs(),
                "turbo /events body read timed out (slow body); returning 408 and \
                 releasing the held events permit + slot"
            );
            return (StatusCode::REQUEST_TIMEOUT, "events body read timed out").into_response();
        }
    };
    let principal = format!("anon@{}", auth.0);
    let req = TurboEventsRequest::new(bytes.to_vec(), principal, 0u64);
    match state.handler.events(req) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /v8/artifacts/status`
///
/// Returns static `{"status":"enabled"}`.  No request body required.
///
/// F3 (defense-in-depth): require the `AuthTenant` extractor — fail-CLOSED 401
/// on a missing/sentinel tenant. `AuthTenant` is `FromRequestParts`, so it
/// precedes the body extractor (axum 0.7 ordering rule).
async fn handle_status(
    State(state): State<TurboRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    _req: axum::extract::Request,
) -> impl IntoResponse {
    let principal = format!("anon@{}", auth.0);
    let _status_req = TurboStatusRequest::new(principal, 0u64);
    match state.handler.status() {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(e) => map_err(e),
    }
}

// ── Error mapping ─────────────────────────────────────────────────────────────

/// Sentinel prefix on a [`TurboBridgeError::Internal`] message that [`map_err`]
/// maps to HTTP **503** (storage unavailable) rather than the generic 500.
const TURBO_STORAGE_UNAVAILABLE_SENTINEL: &str = "turbo-storage-unavailable: ";

/// Fail-CLOSED stand-in mounted when storage creds ARE present but the durable
/// `R2KvStore` refused to build (CAA-360 #7). A silent `InMemoryKvStore` fallback
/// there would serve a NON-durable cache with no alarm (fail-OPEN → silent data
/// loss); every verb here instead returns a sentinel `Internal` error that
/// [`map_err`] maps to 503. Mirrors `cas.rs::UnavailableCasHandler`.
#[derive(Debug)]
struct UnavailableTurboHandler;

impl UnavailableTurboHandler {
    fn unavailable() -> TurboBridgeError {
        TurboBridgeError::Internal(format!(
            "{TURBO_STORAGE_UNAVAILABLE_SENTINEL}R2KvStore refused to build"
        ))
    }
}

impl TurboArtifactHandler for UnavailableTurboHandler {
    fn put(&self, _req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn get(&self, _req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn events(&self, _req: TurboEventsRequest) -> Result<TurboEventsResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
    fn status(&self) -> Result<TurboStatusResponse, TurboBridgeError> {
        Err(Self::unavailable())
    }
}

/// Map a [`TurboBridgeError`] to the canonical HTTP response.
///
/// All error details are logged before mapping so operators have a
/// diagnostic record without exposing internals to callers.
fn map_err(e: TurboBridgeError) -> axum::response::Response {
    tracing::warn!(error = ?e, "turbo bridge error");
    match e {
        TurboBridgeError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found").into_response(),
        TurboBridgeError::HashTooLong { .. } => {
            (StatusCode::BAD_REQUEST, "hash too long").into_response()
        }
        TurboBridgeError::TeamIdInvalid { .. } => {
            (StatusCode::BAD_REQUEST, "invalid teamId").into_response()
        }
        // F-009 / WP-9a: a PRESENT but malformed `x-artifact-tag` → 400. An
        // ABSENT tag never reaches this arm (it is not an error).
        TurboBridgeError::ArtifactTagInvalid { .. } => {
            (StatusCode::BAD_REQUEST, "invalid x-artifact-tag").into_response()
        }
        TurboBridgeError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        // Create-only / `put_if_absent` (B-024): the key already holds an
        // artifact and the store refuses to overwrite it → 409 Conflict. The
        // real turbo client treats this as a non-fatal remote-cache warning
        // (the build still succeeds), proven in
        // docs/design/2026-08-24-turborepo-create-only-evidence.md.
        TurboBridgeError::AlreadyExists { .. } => {
            (StatusCode::CONFLICT, "artifact already exists").into_response()
        }
        TurboBridgeError::AuditFailed(_) => {
            // Fail-CLOSED: audit pipeline down = 503; never serve/commit
            // without the audit row.
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // CAA-360 #7: storage-unavailable (R2KvStore refused to build) → 503,
        // distinct from a generic 500, so clients retry rather than treat it as
        // a permanent server fault.
        TurboBridgeError::Internal(ref msg)
            if msg.starts_with(TURBO_STORAGE_UNAVAILABLE_SENTINEL) =>
        {
            (StatusCode::SERVICE_UNAVAILABLE, "storage unavailable").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_1_1.rs");
    include!("b126_m2_test_1_2.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_1_1_REANCHOR, B126_M2_TEST_1_2_REANCHOR];
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
