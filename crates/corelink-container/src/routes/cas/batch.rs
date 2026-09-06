/// `POST /v1/cas/:tenant/batch` — bulk write (length-framed upload).
///
/// # Wire format (FROZEN)
///
/// The body is, in order: (a) a MANIFEST of newline-delimited JSON, one line per
/// object `{"hash":"<blake3-64hex>","len":<u64>}` in upload order; (b) a single
/// blank line `\n` terminating the manifest; (c) the concatenated raw object
/// bytes in manifest order, each exactly `len` bytes — there is NO per-object
/// delimiter, the manifest's `len` fields frame the payload.
///
/// # Gate sequence (mirrors `handle_write`)
///
/// Cross-tenant 403 → 415 wrong content-type → write-scope 403 → pat-gate →
/// then **ONE** [`QuotaGate::check_batch`] charge of `n` (the object count) for
/// the whole request. We charge ONCE per batch, NEVER per object: a per-object
/// charge would re-introduce the #318 quota-bypass-by-batching regression in
/// reverse — it would N×-bill a single auth'd request — and the cost model is
/// per-op, so `check_batch(tenant, n)` is the proportional, single-round-trip
/// charge (same shape as `bazel_v2::quota_reject_batch`). No tombstone gate
/// (writes are not tombstone-gated — mirrors `handle_write`).
///
/// # Per-object isolation
///
/// Each object is committed independently via `state.write.write(...)`, which is
/// the SAME content-verify the single PUT relies on (the write handler hashes
/// the bytes and returns `HashMismatch` on a forged claim) — the batch route
/// does not re-implement the hash check, it delegates to the one chokepoint. A
/// per-object failure (hash mismatch, storage fault) yields that object's
/// `status:"error"` with a message; the REST of the batch still commits. We do
/// NOT fail the whole batch on one bad object: bulk git ingest must make
/// forward progress and report the bad object, not lose the good ones.
///
/// # Caps (FROZEN)
///
/// ≤ [`BATCH_MAX_OBJECTS`] objects AND ≤ [`BATCH_MAX_BYTES`] of object bytes,
/// whichever is hit first ⇒ otherwise 413 `batch_too_large`. The byte cap is
/// checked against the manifest's declared `sum(len)` so an over-cap request is
/// rejected before any storage write.
///
/// # Response 200
///
/// JSON array `[{"hash":"…","status":"created|exists|error","error":<msg|null>}]`
/// in manifest order (`created` = fresh write, `exists` = idempotent
/// already-present, `error` = per-object failure).
async fn handle_batch_write(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // finding #2 (HIGH DoS): pre-body per-tenant concurrency reservation (declared
    // AHEAD of `body: Bytes`, so axum runs it BEFORE the up-to-BATCH_MAX_BYTES body
    // is buffered). 429 on over-cap; the RAII slot releases on return. SHARES the
    // same per-tenant CAS_WRITE_CONCURRENCY_LIMIT pool as `handle_write` (keyed on
    // the authenticated tenant) — a tenant's single+batch uploads count together.
    _concurrency: CasPutGuard,
    // B-077: reserve the complete batch body before body buffering.
    _global_write_budget: GlobalCasBatchWriteBudgetGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Cross-tenant: path echo must equal the authenticated tenant (mirrors
    // handle_write) — 403 BEFORE any parse or storage access.
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    // FROZEN: the length-framed upload is ONLY the batch content-type. A wrong
    // or absent type ⇒ 415 before the body is parsed.
    if !content_type_is(&headers, &[BATCH_CONTENT_TYPE]) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    // Write-scope gate (fail-CLOSED): a batch upload is a cache WRITE — require
    // `cas:rw` (mirrors handle_write).
    if !scope.can_write() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT write-capability gate (deep-audit money/auth F-1): re-derive
    // the PAT's D1 `can_write` at the container — AFTER scope, BEFORE storage.
    // A read-only PAT on a write path ⇒ 403 even if the scope header claimed write.
    if let Some(resp) = pat_gate_reject_write(&state, &auth.0, &headers).await {
        return resp;
    }
    if body.len() > BATCH_REQUEST_BODY_LIMIT_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
    }

    // ── Parse + frame-validate the whole request (any framing error ⇒ 400) ──
    let Some((manifest_bytes, payload)) = split_manifest(&body) else {
        return (StatusCode::BAD_REQUEST, "missing manifest terminator").into_response();
    };
    let manifest_text = match std::str::from_utf8(manifest_bytes) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "manifest not utf-8").into_response(),
    };
    let mut entries: Vec<BatchUploadManifestEntry> = Vec::new();
    for line in manifest_text.lines() {
        if line.is_empty() {
            continue;
        }
        if line.len() > BATCH_MAX_LINE_BYTES {
            return batch_too_large();
        }
        if entries.len() >= BATCH_MAX_OBJECTS {
            return batch_too_large();
        }
        match serde_json::from_str::<BatchUploadManifestEntry>(line) {
            Ok(e) if e.hash.len() <= BATCH_MAX_HASH_BYTES => entries.push(e),
            Ok(_) => return batch_too_large(),
            Err(_) => return (StatusCode::BAD_REQUEST, "malformed manifest").into_response(),
        }
    }

    // ── Caps (FROZEN): ≤ N objects AND ≤ B bytes, whichever first ⇒ 413 ──
    let Some(total_len) = entries
        .iter()
        .try_fold(0u64, |total, entry| total.checked_add(entry.len))
    else {
        return batch_too_large();
    };
    if total_len > BATCH_MAX_BYTES as u64 {
        return batch_too_large();
    }
    // Framing: the declared lengths must exactly account for the payload bytes
    // (no slack, no truncation) ⇒ otherwise 400 for the WHOLE request.
    if total_len != payload.len() as u64 {
        return (StatusCode::BAD_REQUEST, "payload length mismatch").into_response();
    }

    // ── ONE quota charge for the whole batch (n = object count) ──
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check_batch(&auth.0, entries.len()).await {
            return resp;
        }
    }

    // ── Commit each object independently; per-object errors do not abort ──
    let now_ms = SystemWallClock.now_ms();
    let quota_cap = crate::byte_accounting::storage_quota_from_headers(&headers);
    let mut results = Vec::with_capacity(entries.len());
    let mut offset: usize = 0;
    for entry in &entries {
        let entry_len = match usize::try_from(entry.len) {
            Ok(len) => len,
            Err(_) => return batch_too_large(),
        };
        let end = match offset.checked_add(entry_len) {
            Some(end) => end,
            None => return batch_too_large(),
        };
        // `sum(entry.len) == payload.len()` was asserted above (framing gate),
        // so `offset..end` is always in bounds; `.get()` keeps it panic-free
        // (clippy `indexing_slicing`) and the explicit branch stays fail-closed.
        let Some(slice) = payload.get(offset..end) else {
            return (StatusCode::BAD_REQUEST, "payload length mismatch").into_response();
        };
        offset = end;
        // Canonical-hash gate (defense-in-depth, mirrors the single PUT). A
        // non-canonical claim is a per-object error, not a batch abort.
        if !super::ac::is_canonical_digest(&entry.hash) {
            results.push(serde_json::json!({
                "hash": entry.hash, "status": "error", "error": "malformed hash",
            }));
            continue;
        }
        let req = CasWriteRequest::new(
            auth.0.clone(),
            entry.hash.clone(),
            slice.to_vec(),
            format!("anon@{}", auth.0),
            auth.0.clone(),
            now_ms,
        )
        .with_storage_quota_bytes(quota_cap);
        // Content-verify + storage commit happen INSIDE state.write (the same
        // chokepoint the single PUT uses): the handler hashes the bytes and
        // returns HashMismatch on a forged claim; the accounting decorator
        // reserves/commits the bytes. durable=true ⇒ fresh, false ⇒ idempotent.
        match state.write.write(req) {
            Ok(resp) => {
                let status = if resp.durable { "created" } else { "exists" };
                results.push(serde_json::json!({
                    "hash": entry.hash, "status": status, "error": serde_json::Value::Null,
                }));
            }
            Err(e) => {
                let msg = batch_object_error_message(&e);
                results.push(serde_json::json!({
                    "hash": entry.hash, "status": "error", "error": msg,
                }));
            }
        }
    }
    (StatusCode::OK, Json(serde_json::Value::Array(results))).into_response()
}

/// Compact per-object error message for a `/batch` upload failure. Mirrors the
/// `map_err` taxonomy (NotFound/HashMismatch/CrossTenant/Audit/Internal) but
/// keeps a single object's failure OUT of the batch's HTTP status — the request
/// is 200 and the failure is reported in that object's `error` field. We never
/// leak internal storage detail (same discipline as `map_err`).
fn batch_object_error_message(e: &CasHandlerError) -> String {
    match e {
        CasHandlerError::HashMismatch { .. } => "content hash mismatch".to_owned(),
        CasHandlerError::NotFound { .. } => "not found".to_owned(),
        CasHandlerError::CrossTenantDenied { .. } => "cross-tenant".to_owned(),
        CasHandlerError::AuditFailed(_) => "audit closed".to_owned(),
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::byte_accounting::OVER_CAP_SENTINEL) =>
        {
            "storage quota exceeded".to_owned()
        }
        // F-004 — a batch re-PUT of an erased hash is refused by the shared
        // tombstone gate; report it per-object (the rest of the batch proceeds).
        CasHandlerError::Internal(ref msg)
            if msg.starts_with(crate::routes::cas_erase::TOMBSTONE_GONE_SENTINEL) =>
        {
            "erased".to_owned()
        }
        _ => "internal error".to_owned(),
    }
}

/// `POST /v1/cas/:tenant/batch-read` — bulk read (length-framed download).
///
/// # Request
///
/// NDJSON `{"hash":"<blake3>"}` lines. Accepts [`BATCH_CONTENT_TYPE`] or
/// [`NDJSON_CONTENT_TYPE`] (the read-side body is a plain hash list).
///
/// # Gate sequence
///
/// Cross-tenant 403 → 415 → read-scope 403 → pat-gate → ONE
/// [`QuotaGate::check_batch`] charge of `n` (the hash count). Per hash the
/// tombstone gate is applied FIRST (mirrors `handle_read`: an erased
/// `(tenant, hash)` ⇒ `gone`), then the R2 read.
///
/// # Response 200
///
/// A MANIFEST of `{"hash":"…","len":<u64>,"status":"ok|absent|gone"}` NDJSON
/// lines + a single blank line `\n` + the concatenated raw bytes of the `ok`
/// objects in manifest order. `absent` (404-class) and `gone` (410-class
/// tombstoned) contribute zero bytes.
async fn handle_batch_read(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // brutal-fleet M2 (MED DoS): pre-body per-tenant bulk-read concurrency
    // reservation (declared AHEAD of `body: Bytes`, so axum runs it BEFORE the
    // body is buffered AND before the up-to-BATCH_MAX_BYTES payload accumulator is
    // built). 429 on over-cap; the RAII slot releases on return. SEPARATE pool
    // from the write guard (`CAS_READ_CONCURRENCY_LIMIT`, keyed on the
    // authenticated tenant) — bulk reads don't contend with the tenant's writes.
    _read_concurrency: CasReadConcurrencyGuard,
    // B-077: reserve the bounded batch working set before body buffering.
    _global_read_budget: GlobalCasBatchReadBudgetGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    if !content_type_is(&headers, &[BATCH_CONTENT_TYPE, NDJSON_CONTENT_TYPE]) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    if body.len() > BATCH_REQUEST_BODY_LIMIT_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
    }

    let hashes = match parse_ndjson_hashes(&body) {
        Ok(h) => h,
        Err(rejection) => return rejection.into_response(),
    };
    // FROZEN cap: object count only on the request side (the response byte
    // volume is bounded by what is actually stored, but the request fan-out is
    // capped here so one request can't drive N unbounded reads behind one
    // charge).
    if hashes.len() > BATCH_MAX_OBJECTS {
        return batch_too_large();
    }

    // ONE quota charge for the whole batch.
    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check_batch(&auth.0, hashes.len()).await {
            return resp;
        }
    }

    let now_ms = SystemWallClock.now_ms();
    // Build the manifest and the payload in lock-step (manifest order). We cap
    // the accumulated payload at BATCH_MAX_BYTES: a tenant could request 2000
    // large blobs whose stored bytes exceed 8 MiB, which would blow the global
    // 10 MiB response budget — so an `ok` object that would push the payload
    // over the cap aborts the WHOLE request 413 (FROZEN: ≤ 8 MiB per batch).
    let mut manifest = String::new();
    let mut payload: Vec<u8> = Vec::new();

    // Per-hash outcome produced by a spawned read task. Carried back across the
    // spawn boundary (all variants are `Send`) and reassembled IN hash order so
    // the manifest/payload framing is byte-identical to the old serial loop.
    enum PerHash {
        /// Non-canonical hash, or a `NotFound` read ⇒ 404-class `absent`.
        Absent,
        /// Tombstoned `(tenant, hash)` ⇒ 410-class `gone`.
        Gone,
        /// Present blob bytes held with its process-wide reservation.
        Ok {
            bytes: Vec<u8>,
            _global_permit: OwnedSemaphorePermit,
        },
        /// Process-wide budget unavailable; fail the whole batch closed.
        BudgetFault,
        /// Tombstone gate lookup faulted ⇒ fail CLOSED (503) for the whole batch.
        TombstoneFault,
        /// Read faulted (non-NotFound) ⇒ propagate via `map_err`.
        ReadErr(CasHandlerError),
    }

    // Fan out, in hash order, with bounded (BATCH_READ_FANOUT-way) concurrency.
    // Each task holds an owned semaphore permit for its whole lifetime, so at
    // most BATCH_READ_FANOUT reads are in flight at once. We MUST use
    // `tokio::spawn` (not `spawn_blocking`): the sync `read()` internally does
    // `tokio::task::block_in_place(..)`, which is only valid on a multi-thread
    // runtime WORKER thread — spawn_blocking threads are not workers and would
    // panic.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(BATCH_READ_FANOUT));
    let mut handles: Vec<tokio::task::JoinHandle<PerHash>> = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        // Acquire the permit BEFORE spawning so the in-flight count is bounded
        // to BATCH_READ_FANOUT (permit is moved into the task and held for its
        // lifetime). The semaphore is never closed here, so `Err` (closed) is
        // unreachable — but we fail CLOSED (500) rather than `.expect()` it
        // (clippy::expect-used is denied repo-wide).
        let permit = match Arc::clone(&semaphore).acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                return map_err(CasHandlerError::Internal(
                    "batch-read semaphore closed".into(),
                ))
            }
        };
        let read = state.read.clone();
        let tombstones = state.tombstones.clone();
        let tenant = auth.0.clone();
        let hash = hash.clone();
        handles.push(tokio::spawn(async move {
            // Hold the permit for the whole task lifetime.
            let _permit = permit;
            // Non-canonical hash ⇒ it cannot name a stored blob; report absent
            // (it is not a framing error and must not abort the batch).
            if !super::ac::is_canonical_digest(&hash) {
                return PerHash::Absent;
            }
            let global_permit = match acquire_global_cas_read_budget(
                CAS_READ_SINGLE_PERMITS,
                "batch-read-object",
            )
            .await
            {
                Ok(permit) => permit,
                Err(_) => return PerHash::BudgetFault,
            };
            // Tombstone gate FIRST (mirrors handle_read): an erased blob is
            // `gone`, never resurrected, never reported absent. A lookup fault
            // fails CLOSED for the whole batch (the erase write-side is the
            // source of truth).
            if let Some(tombstones) = tombstones.as_ref() {
                match tombstones.is_tombstoned(&tenant, &hash).await {
                    Ok(true) => return PerHash::Gone,
                    Ok(false) => {}
                    Err(e) => {
                        // PEN-2/REV-S1: fail CLOSED, identical to the single-read
                        // gate (handle_read ~678). A tombstone-lookup transport
                        // error must NOT serve a possibly-erased (GDPR) object —
                        // fail the whole batch rather than risk resurrecting
                        // erased bytes during a D1 blip.
                        tracing::warn!(error = %e, "cas batch-read: tombstone gate lookup failed; failing CLOSED (503)");
                        return PerHash::TombstoneFault;
                    }
                }
            }
            let req = CasReadRequest::new(
                tenant.clone(),
                hash.clone(),
                format!("anon@{tenant}"),
                tenant.clone(),
                now_ms,
            );
            // Sync `read()` — `block_in_place` inside it is valid because this is
            // a multi-thread-runtime worker thread (`tokio::spawn`).
            match read.read(req) {
                Ok(resp) => PerHash::Ok {
                    bytes: resp.bytes,
                    _global_permit: global_permit,
                },
                Err(CasHandlerError::NotFound { .. }) => PerHash::Absent,
                Err(e) => PerHash::ReadErr(e),
            }
        }));
    }

    // Reassemble IN push order (== hash order). This ordering is LOAD-BEARING:
    // the client slices the concatenated payload by the manifest `len`s, so the
    // manifest lines and the payload segments must both follow request order.
    for (hash, handle) in hashes.iter().zip(handles) {
        let outcome = match handle.await {
            Ok(o) => o,
            // A spawned task panicked (or was cancelled) ⇒ internal read
            // failure. Map to the same 500 surface as a generic read error.
            Err(_join_err) => {
                return map_err(CasHandlerError::Internal("batch read task failed".into()));
            }
        };
        match outcome {
            PerHash::Absent => {
                manifest.push_str(&format!(
                    "{}\n",
                    serde_json::json!({"hash": hash, "len": 0, "status": "absent"})
                ));
            }
            PerHash::Gone => {
                manifest.push_str(&format!(
                    "{}\n",
                    serde_json::json!({"hash": hash, "len": 0, "status": "gone"})
                ));
            }
            PerHash::Ok {
                bytes,
                _global_permit: _,
            } => {
                if payload.len() + bytes.len() > BATCH_MAX_BYTES {
                    return batch_too_large();
                }
                manifest.push_str(&format!(
                    "{}\n",
                    serde_json::json!({"hash": hash, "len": bytes.len(), "status": "ok"})
                ));
                payload.extend_from_slice(&bytes);
            }
            PerHash::BudgetFault => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "global read budget unavailable",
                )
                    .into_response();
            }
            PerHash::TombstoneFault => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "tombstone gate unavailable",
                )
                    .into_response();
            }
            PerHash::ReadErr(e) => return map_err(e),
        }
    }
    // Stream the manifest and payload as separate frames. Keeping both guards
    // in this stream prevents the response payload or tenant slot from
    // escaping their reservation when the handler returns; concatenating into
    // `out` would transiently duplicate the payload and releasing the slot
    // here would let a slow client multiply the per-tenant response heap.
    let mut manifest = manifest.into_bytes();
    manifest.push(b'\n');
    let body_stream = async_stream::stream! {
        let _read_slot = _read_concurrency;
        let _permit = _global_read_budget;
        yield Ok::<Frame<axum::body::Bytes>, std::convert::Infallible>(
            Frame::data(axum::body::Bytes::from(manifest)),
        );
        yield Ok::<Frame<axum::body::Bytes>, std::convert::Infallible>(
            Frame::data(axum::body::Bytes::from(payload)),
        );
    };
    (StatusCode::OK, Body::new(StreamBody::new(body_stream))).into_response()
}

/// `POST /v1/cas/:tenant/batch-exists` — bulk HEAD-class existence probe.
///
/// # Request
///
/// NDJSON `{"hash":"<blake3>"}` lines (≤ [`BATCH_MAX_OBJECTS`] hashes). Accepts
/// [`BATCH_CONTENT_TYPE`] or [`NDJSON_CONTENT_TYPE`].
///
/// # Probe method
///
/// HEAD-class: this MUST NOT read object bytes. The `CasReadHandler` trait has
/// no dedicated `exists`/HEAD method, so the cheapest correct probe available is
/// a `state.read.read(...)` whose `Ok`/`NotFound` outcome is mapped to
/// `present` — the bytes are discarded and never enter the response. (When a
/// true HEAD probe is added to the handler trait this should switch to it; until
/// then a read-and-discard is the only correct existence signal.) ONE
/// [`QuotaGate::check_batch`] charge of `n`.
///
/// # Response 200
///
/// JSON array `[{"hash":"…","present":<bool>}]` in request order.
async fn handle_batch_exists(
    State(state): State<CasRouteState>,
    Path(tenant): Path<String>,
    auth: crate::auth_tenant::AuthTenant,
    scope: crate::scope::CacheScope,
    headers: axum::http::HeaderMap,
    // brutal-fleet M2 (MED DoS): pre-body per-tenant bulk-read concurrency
    // reservation (declared AHEAD of `body: Bytes`). `batch-exists` doesn't
    // accumulate object bytes, but it fans out up to BATCH_MAX_OBJECTS storage
    // existence probes per request, so it shares the same bulk-read concurrency
    // axis as `batch-read`. 429 on over-cap; RAII slot releases on return.
    _read_concurrency: CasReadConcurrencyGuard,
    // B-077: reserve the process-wide batch working set before body buffering,
    // independently of the per-tenant concurrency slot above.
    _global_read_budget: GlobalCasBatchReadBudgetGuard,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if tenant != auth.0 {
        return (StatusCode::FORBIDDEN, "cross-tenant").into_response();
    }
    if !content_type_is(&headers, &[BATCH_CONTENT_TYPE, NDJSON_CONTENT_TYPE]) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    if !scope.can_read() {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    if let Some(resp) = pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }

    let hashes = match parse_ndjson_hashes(&body) {
        Ok(h) => h,
        Err(rejection) => return rejection.into_response(),
    };
    if hashes.len() > BATCH_MAX_OBJECTS {
        return batch_too_large();
    }

    if let Some(gate) = state.quota.as_ref() {
        if let Some(resp) = gate.check_batch(&auth.0, hashes.len()).await {
            return resp;
        }
    }

    let now_ms = SystemWallClock.now_ms();
    let mut results = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        // A non-canonical hash cannot name a stored blob ⇒ present=false (it is
        // not a framing error).
        let present = if !super::ac::is_canonical_digest(hash) {
            false
        } else {
            let req = CasReadRequest::new(
                auth.0.clone(),
                hash.clone(),
                format!("anon@{}", auth.0),
                auth.0.clone(),
                now_ms,
            );
            // HEAD-class existence probe: `CasReadHandler::exists` is overridden
            // by the R2 handler as a single S3 `HeadObject` with NO body fetch
            // (r2_s3.rs) — using `read()` here would fetch every present object's
            // full bytes (tens of GiB of R2 GET egress over a closure-sized probe
            // — the exact anti-pattern the trait's `exists` default warns about),
            // defeating the whole point of a cheap dedup probe. `Ok(true)` ⇒
            // present; `Ok(false)`/`Err` ⇒ false (conservative — never claim a
            // blob is present on a storage fault; the caller re-uploads, which is
            // idempotent).
            matches!(state.read.exists(req), Ok(true))
        };
        results.push(serde_json::json!({ "hash": hash, "present": present }));
    }
    (StatusCode::OK, Json(serde_json::Value::Array(results))).into_response()
}
