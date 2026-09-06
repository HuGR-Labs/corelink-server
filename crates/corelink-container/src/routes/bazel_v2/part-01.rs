/// `PUT /bazel/v2/:instance/blobs/ac/:hash/:size` — AC write.
async fn handle_ac_write(
    State(state): State<BazelRouteState>,
    Path((instance, hash, size)): Path<(String, String, String)>,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (see handle_cas_write).
    _concurrency: BazelPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): AC update is a cache WRITE; require `cas:rw`
    // (or `admin`) BEFORE any storage access. A read-only token is rejected.
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // cf-multitenant WP5b (fail-CLOSED): mirror the native `ac.rs` AC-write gate on
    // the Bazel REAPI v2 AC surface. A narrowed runner-job PAT with a pinned AC key
    // may write ONLY that exact key — otherwise a per-job credential could escape its
    // narrowing by routing an arbitrary AC write through `/bazel/v2/.../blobs/ac/:hash`
    // instead of `/v1/ac/...`. A `"*"` pin (the launch default) or no pin ⇒ no key
    // restriction (NO-OP for current traffic). CAS is content-addressed so it needs no
    // such gate; only the AC (action-cache) surface is key-poisonable.
    if !runner_job.ac_key_allowed(&hash) {
        return (
            StatusCode::FORBIDDEN,
            "ac write outside the job's allowed key",
        )
            .into_response();
    }
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any storage access.
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession gate (finding #4) — AFTER scope+tenant, BEFORE storage.
    if let Some(resp) = pat_gate_reject_write(&state, &tenant, &headers).await {
        return resp;
    }
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match parse_digest(&hash, &size) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state.adapter.ac_put(
        &instance,
        &digest,
        body.to_vec(),
        corelink_bazel_bridge::adapter::WriteCtx {
            principal: &p,
            caller_tenant: &tenant,
            at_unix_ms: now_ms(),
            storage_quota_bytes: crate::byte_accounting::storage_quota_from_headers(&headers),
        },
    ) {
        Ok(()) => {
            // usage-metering-roi: AC write (fire-and-forget, no await/I/O).
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::Write);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_bridge_err(e),
    }
}

/// `POST /bazel/v2/:instance/findMissingBlobs` — batch find-missing.
///
/// Accepts JSON body `{"blobDigests":[{"hash":"…","sizeBytes":N},…]}`.
/// Returns `{"missingBlobDigests":[…]}`.
async fn handle_find_missing(
    State(state): State<BazelRouteState>,
    Path(instance): Path<String>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Scope gate (fail-CLOSED): findMissingBlobs is an existence probe over the
    // CAS — require the find-missing capability (ADR-0071) BEFORE any storage
    // access. `can_find_missing()` is satisfied by an EXPLICIT find-missing grant
    // (the true least-privilege `find-missing` scope) OR by any read grant (read ⊇
    // find-missing), so pre-ADR-0071 `cas:r`/`cas:rw`/`admin` PATs are unchanged
    // while a find-only PAT can now probe here (and ONLY here). The body extractor
    // (`Bytes`) stays LAST per axum 0.7 ordering; `CacheScope` is `FromRequestParts`
    // so it precedes the body.
    if !scope.can_find_missing() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // F-defense (fail-CLOSED): reject a missing/sentinel tenant with 401
    // BEFORE any body parse or storage access.
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // Native PAT possession gate (finding #4) — AFTER scope+tenant, BEFORE any
    // body parse or storage access.
    if let Some(resp) = pat_gate_reject(&state, &tenant, &headers).await {
        return resp;
    }
    let p = principal(&headers);

    // Parse + validate the JSON body BEFORE the quota gate so we know the
    // digest count (F12: charge proportional to batch fan-out).
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "request body is not valid UTF-8").into_response()
        }
    };
    let digests = match parse_find_missing_request(body_str) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };

    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1).
    // F12 + CAA-360 #14/#18: charge one op-cost unit per digest (proportional
    // to the full batch fan-out, uncapped) in a single atomic check-and-accrue
    // so the ceiling trips at the right dollar amount with no per-digest
    // round-trips and no iteration cap to dilute large batches.
    if let Some(resp) = quota_reject_batch(&state, &tenant, digests.len()).await {
        return resp;
    }

    // Delegate to the find-missing handler.
    let missing = match state
        .find_missing
        .find_missing(&instance, &p, &tenant, now_ms(), &digests)
    {
        Ok(m) => m,
        Err(e) => return map_bridge_err(e),
    };

    // Serialise the response.
    match build_find_missing_response(missing) {
        Ok(json) => (StatusCode::OK, [("content-type", "application/json")], json).into_response(),
        Err(e) => map_bridge_err(e),
    }
}

// ─── Stock-Bazel HTTP cache alias handlers ─────────────────────────────────────
//
// These four handlers implement the plain HTTP-cache scheme stock `bazel
// --remote_cache=https://host/bazel/cache` actually speaks: `GET`/`PUT` on
// `/cas/<hash>` and `/ac/<hash>` — no `:instance` segment, no `:size`. Buck2 is
// NOT in scope here: it speaks REAPI over gRPC only. They map onto the SAME
// `BazelAdapter`/store as the REST handlers above and run the IDENTICAL gate
// sequence (scope → tenant → PAT → quota; AC-write also the runner-job key
// pin), preserving every security invariant. The one difference vs the REST
// handlers is where the REAPI `instance`/tenant
// comes from: NOT a path segment (there is none) but the
// Worker-injected `x-corelink-tenant-id` header, extracted fail-CLOSED by
// `caller_tenant`. Because `instance == caller_tenant` by construction, the
// adapter's cross-tenant guard is a tautology here and isolation rests entirely
// on the per-tenant namespace: a cross-tenant hash is a uniform 404 (never
// another tenant's bytes), and a missing/sentinel tenant is 401.

/// `GET /bazel/cache/cas/{hash}` — stock-Bazel HTTP-cache CAS read.
///
/// Stock alias for [`handle_cas_read`]. No `:size` in the URL; the read path
/// addresses purely by hash, so a `0` placeholder size is used to build the
/// [`Digest`] (which still enforces the 64-lowercase-hex hash rule).
async fn handle_http_cas_read(
    State(state): State<BazelRouteState>,
    Path(hash): Path<String>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &tenant, &headers).await {
        return resp;
    }
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match Digest::new(hash, 0) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .cas_get(&tenant, &digest, &p, &tenant, now_ms())
    {
        Ok(bytes) => {
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::ReadHit);
            (
                StatusCode::OK,
                [("content-type", "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        Err(e) => {
            if matches!(e, BazelBridgeError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_bridge_err(e)
        }
    }
}

/// `PUT /bazel/cache/cas/{hash}` — stock-Bazel HTTP-cache CAS write.
///
/// Stock alias for [`handle_cas_write`]. The stock client sends no declared
/// size, so the [`Digest`] size is the actual body length (making the adapter's
/// size-match check a tautology); the SHA-256 content-addressing boundary check
/// below is the real integrity gate — poisoned bytes never enter the
/// `bazel/sha256/` keyspace.
async fn handle_http_cas_write(
    State(state): State<BazelRouteState>,
    Path(hash): Path<String>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (declared AHEAD of
    // `body: Bytes` so axum runs it BEFORE the body is buffered).
    _concurrency: BazelPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject_write(&state, &tenant, &headers).await {
        return resp;
    }
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match Digest::new(hash, body.len() as u64) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    if let Err(e) = corelink_bazel_bridge::digest::verify_sha256(&digest.hash, &body) {
        return map_bridge_err(e);
    }
    match state.adapter.cas_put(
        &tenant,
        &digest,
        body.to_vec(),
        corelink_bazel_bridge::adapter::WriteCtx {
            principal: &p,
            caller_tenant: &tenant,
            at_unix_ms: now_ms(),
            storage_quota_bytes: crate::byte_accounting::storage_quota_from_headers(&headers),
        },
    ) {
        Ok(()) => {
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::Write);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_bridge_err(e),
    }
}

/// `GET /bazel/cache/ac/{hash}` — stock-Bazel HTTP-cache AC read.
///
/// Stock alias for [`handle_ac_read`]. `0` placeholder size (AC read addresses
/// by the action-digest hash).
async fn handle_http_ac_read(
    State(state): State<BazelRouteState>,
    Path(hash): Path<String>,
    scope: crate::scope::CacheScope,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &tenant, &headers).await {
        return resp;
    }
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match Digest::new(hash, 0) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state
        .adapter
        .ac_get(&tenant, &digest, &p, &tenant, now_ms())
    {
        Ok(bytes) => {
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::ReadHit);
            (
                StatusCode::OK,
                [("content-type", "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        Err(e) => {
            if matches!(e, BazelBridgeError::NotFound { .. }) {
                state
                    .usage_meter
                    .record(&tenant, crate::usage_meter::UsageEvent::ReadMiss);
            }
            map_bridge_err(e)
        }
    }
}

/// `PUT /bazel/cache/ac/{hash}` — stock-Bazel HTTP-cache AC write.
///
/// Stock alias for [`handle_ac_write`]. Mirrors the REST AC-write gate exactly,
/// INCLUDING the WP5b runner-job AC-key pin (`ac_key_allowed`) so a narrowed
/// per-job PAT cannot escape its key narrowing through this alias. The AC payload
/// is the ActionResult (addressed by the action digest, NOT its own content
/// hash), so — like the REST AC write — no `verify_sha256` is applied; the size
/// is the actual body length (AC has no size-match check).
async fn handle_http_ac_write(
    State(state): State<BazelRouteState>,
    Path(hash): Path<String>,
    scope: crate::scope::CacheScope,
    runner_job: crate::scope::RunnerJob,
    headers: HeaderMap,
    // cluster F: pre-body per-tenant concurrency reservation (see handle_cas_write).
    _concurrency: BazelPutGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // WP5b parity with `handle_ac_write`: a narrowed runner-job PAT with a pinned
    // AC key may write ONLY that key; `"*"`/no pin ⇒ NO-OP.
    if !runner_job.ac_key_allowed(&hash) {
        return (
            StatusCode::FORBIDDEN,
            "ac write outside the job's allowed key",
        )
            .into_response();
    }
    let tenant = match caller_tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject_write(&state, &tenant, &headers).await {
        return resp;
    }
    if let Some(resp) = quota_reject(&state, &tenant).await {
        return resp;
    }
    let p = principal(&headers);
    let digest = match Digest::new(hash, body.len() as u64) {
        Ok(d) => d,
        Err(e) => return map_bridge_err(e),
    };
    match state.adapter.ac_put(
        &tenant,
        &digest,
        body.to_vec(),
        corelink_bazel_bridge::adapter::WriteCtx {
            principal: &p,
            caller_tenant: &tenant,
            at_unix_ms: now_ms(),
            storage_quota_bytes: crate::byte_accounting::storage_quota_from_headers(&headers),
        },
    ) {
        Ok(()) => {
            state
                .usage_meter
                .record(&tenant, crate::usage_meter::UsageEvent::Write);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_bridge_err(e),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("tests-00-00.rs");
    include!("tests-00-01.rs");
}
