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
    (auth, scope, headers): CasRequestAuth,
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
        /// Present blob bytes held with the full single-object reservation.
        Ok {
            bytes: Vec<u8>,
            _global_permit: tokio::sync::OwnedSemaphorePermit,
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
    //
    // The global batch envelope is already reserved by the extractor. The
    // admission gate allows one such envelope at a time, and a live object
    // acquires the full single-read peak (196 MiB). The FIFO drain below is
    // important: a completed task keeps that reservation in its outcome until
    // its bytes are appended, so the producer must never fill a second window
    // while the first result is waiting to be consumed.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(BATCH_READ_FANOUT));
    let mut handles: Vec<tokio::task::JoinHandle<PerHash>> = Vec::with_capacity(hashes.len());
    let mut append_outcome = |hash: &str, outcome: PerHash| match outcome {
        PerHash::Absent => {
            manifest.push_str(&format!(
                "{}\n",
                serde_json::json!({"hash": hash, "len": 0, "status": "absent"})
            ));
            Ok(())
        }
        PerHash::Gone => {
            manifest.push_str(&format!(
                "{}\n",
                serde_json::json!({"hash": hash, "len": 0, "status": "gone"})
            ));
            Ok(())
        }
        PerHash::Ok {
            bytes,
            _global_permit: _,
        } => {
            if payload.len() + bytes.len() > BATCH_MAX_BYTES {
                return Err(batch_too_large());
            }
            manifest.push_str(&format!(
                "{}\n",
                serde_json::json!({"hash": hash, "len": bytes.len(), "status": "ok"})
            ));
            payload.extend_from_slice(&bytes);
            Ok(())
        }
        PerHash::BudgetFault => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "global read budget unavailable",
        )
            .into_response()),
        PerHash::TombstoneFault => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "tombstone gate unavailable",
        )
            .into_response()),
        PerHash::ReadErr(error) => Err(map_err(error)),
    };
    let mut result_hashes = hashes.iter();
    let mut terminal_response: Option<axum::response::Response> = None;
    for hash in &hashes {
        if terminal_response.is_some() {
            break;
        }
        // Acquire the permit BEFORE spawning so the in-flight count is bounded
        // to BATCH_READ_FANOUT (permit is moved into the task and held for its
        // lifetime). The semaphore is never closed here, so `Err` (closed) is
        // unreachable — but we fail CLOSED (500) rather than `.expect()` it
        // (clippy::expect-used is denied repo-wide).
        let permit = match Arc::clone(&semaphore).acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                terminal_response = Some(map_err(CasHandlerError::Internal(
                    "batch-read semaphore closed".into(),
                )));
                for pending in &handles {
                    pending.abort();
                }
                break;
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

        // Drain the oldest result as soon as the bounded window is full. A
        // completed `Ok` outcome owns its weighted permit until this point;
        // waiting to drain it while spawning the next window would strand the
        // global permit behind the producer and recreate the deadlock.
        if handles.len() == BATCH_READ_FANOUT {
            let handle = handles.remove(0);
            let outcome = match handle.await {
                Ok(o) => Some(o),
                Err(_join_err) => {
                    terminal_response = Some(map_err(CasHandlerError::Internal(
                        "batch read task failed".into(),
                    )));
                    for pending in &handles {
                        pending.abort();
                    }
                    None
                }
            };
            if let Some(outcome) = outcome {
                let result_hash = match result_hashes.next() {
                    Some(hash) => hash,
                    None => {
                        terminal_response = Some(map_err(CasHandlerError::Internal(
                            "batch read result count mismatch".into(),
                        )));
                        for pending in &handles {
                            pending.abort();
                        }
                        continue;
                    }
                };
                if let Err(response) = append_outcome(result_hash, outcome) {
                    terminal_response = Some(response);
                    for pending in &handles {
                        pending.abort();
                    }
                }
            }
        }
    }

    // Reassemble IN push order (== hash order). This ordering is LOAD-BEARING:
    // the client slices the concatenated payload by the manifest `len`s, so the
    // manifest lines and the payload segments must both follow request order.
    // On a terminal error, the producer stops before the next window, aborts
    // the at-most-(BATCH_READ_FANOUT - 1) pending tasks, and still awaits every
    // handle here. That bounded cancellation/drain keeps read and permit
    // lifetimes inside this request rather than detaching work after an error.
    while !handles.is_empty() {
        let handle = handles.remove(0);
        let outcome = match handle.await {
            Ok(o) => Some(o),
            // A spawned task panicked (or was cancelled) ⇒ internal read
            // failure. Map to the same 500 surface as a generic read error.
            Err(_join_err) => {
                if terminal_response.is_none() {
                    terminal_response = Some(map_err(CasHandlerError::Internal(
                        "batch read task failed".into(),
                    )));
                    for pending in &handles {
                        pending.abort();
                    }
                }
                None
            }
        };
        if terminal_response.is_some() {
            continue;
        }
        if let Some(outcome) = outcome {
            let result_hash = match result_hashes.next() {
                Some(hash) => hash,
                None => {
                    terminal_response = Some(map_err(CasHandlerError::Internal(
                        "batch read result count mismatch".into(),
                    )));
                    for pending in &handles {
                        pending.abort();
                    }
                    continue;
                }
            };
            if let Err(response) = append_outcome(result_hash, outcome) {
                terminal_response = Some(response);
                for pending in &handles {
                    pending.abort();
                }
            }
        }
    }
    if let Some(response) = terminal_response {
        return response;
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
    (auth, scope, headers): CasRequestAuth,
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
