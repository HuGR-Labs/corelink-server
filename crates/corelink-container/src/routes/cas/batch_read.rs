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
enum BatchReadOutcome {
    /// Non-canonical hash, or a `NotFound` read ⇒ 404-class `absent`.
    Absent,
    /// Tombstoned `(tenant, hash)` ⇒ 410-class `gone`.
    Gone,
    /// Present blob bytes held with the 24 MiB object reservation.
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

/// Shared ownership of the request-level capacity envelope.
///
/// A batch read crosses a synchronous trait boundary: a child task may be
/// inside `block_in_place` when the handler future is dropped. Keeping these
/// guards behind an `Arc` lets every live child retain the tenant slot and the
/// global envelope until that synchronous call returns. The response stream
/// owns the final reference on the success path, so slow consumers still hold
/// both reservations until their bytes are consumed or dropped.
struct BatchReadLease {
    _tenant: CasReadConcurrencyGuard,
    _global: GlobalCasBatchReadBudgetGuard,
}

/// Owns every live batch-read task. Dropping the handler future must cancel
/// queued/runnable tasks rather than detaching them; explicit terminal paths
/// call [`Self::abort_and_drain`] to await their cancellation before returning.
///
/// A synchronous `CasReadHandler::read` can be inside `block_in_place` when
/// cancellation arrives and cannot be preempted mid-call. The R2 path bounds
/// that unwinding with its request timeout and the 8 MiB object ceiling; the
/// guard's eight-task window and 24 MiB per-task permit keep the interim peak
/// within the batch reservation while those calls finish.
struct BatchReadTaskGuard {
    handles: Vec<tokio::task::JoinHandle<BatchReadOutcome>>,
}

impl BatchReadTaskGuard {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            handles: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, handle: tokio::task::JoinHandle<BatchReadOutcome>) {
        self.handles.push(handle);
    }

    fn len(&self) -> usize {
        self.handles.len()
    }

    fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// Await the oldest task without moving its `JoinHandle` out of the guard.
    /// If the handler future is cancelled while waiting, `Drop` still owns and
    /// aborts this task.
    async fn await_oldest(&mut self) -> Option<Result<BatchReadOutcome, tokio::task::JoinError>> {
        let result = {
            let oldest = self.handles.first_mut()?;
            oldest.await
        };
        // Remove the completed handle for both Ok and JoinError. A completed
        // JoinHandle must not be awaited a second time by terminal cleanup.
        self.handles.remove(0);
        Some(result)
    }

    /// Explicit terminal cleanup: abort all remaining work, then drain every
    /// JoinHandle so no child task or weighted permit outlives the response.
    async fn abort_and_drain(&mut self) {
        for pending in &self.handles {
            pending.abort();
        }
        while let Some(pending) = self.handles.pop() {
            let _ = pending.await;
        }
    }
}

impl Drop for BatchReadTaskGuard {
    fn drop(&mut self) {
        // This is the cancellation path for an externally dropped handler
        // future. There is no await available from Drop; explicit terminal
        // responses use `abort_and_drain` above, while this abort prevents any
        // queued task from starting after the request future is gone.
        for pending in &self.handles {
            pending.abort();
        }
    }
}

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
    // volume is bounded by what is actually stored, but the requested object
    // count is capped here so one request can't drive N unbounded reads behind
    // one charge).
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

    // Do not leave the extractor-owned leases as stack locals. Every spawned
    // read receives a shared reference, making request cancellation retain the
    // tenant/global envelope until active synchronous reads unwind.
    let lease = Arc::new(BatchReadLease {
        _tenant: _read_concurrency,
        _global: _global_read_budget,
    });

    // Fan out on worker tasks because the synchronous storage trait bridges its
    // async R2 client with `block_in_place`, which is valid on tokio worker
    // threads but not on `spawn_blocking` threads. The admission gate reserves
    // one envelope; this local window is the separately-derived eight-object
    // bound (`(220 - 22) / 24`).
    let semaphore = Arc::new(tokio::sync::Semaphore::new(BATCH_READ_FANOUT));
    let mut tasks = BatchReadTaskGuard::with_capacity(BATCH_READ_FANOUT);
    let mut append_outcome =
        |hash: &str, outcome: BatchReadOutcome| -> Result<(), axum::response::Response> {
            match outcome {
                BatchReadOutcome::Absent => {
                    manifest.push_str(&format!(
                        "{}\n",
                        serde_json::json!({"hash": hash, "len": 0, "status": "absent"})
                    ));
                    Ok(())
                }
                BatchReadOutcome::Gone => {
                    manifest.push_str(&format!(
                        "{}\n",
                        serde_json::json!({"hash": hash, "len": 0, "status": "gone"})
                    ));
                    Ok(())
                }
                BatchReadOutcome::Ok {
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
                BatchReadOutcome::BudgetFault => Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "global read budget unavailable",
                )
                    .into_response()),
                BatchReadOutcome::TombstoneFault => Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "tombstone gate unavailable",
                )
                    .into_response()),
                BatchReadOutcome::ReadErr(error) => Err(map_err(error)),
            }
        };
    let mut result_hashes = hashes.iter();

    for hash in &hashes {
        // Acquire before spawning so the number of live tasks never exceeds the
        // derived window. The semaphore is local and never closed, but a
        // closed result still fails closed after draining any earlier tasks.
        let permit = match Arc::clone(&semaphore).acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                tasks.abort_and_drain().await;
                return map_err(CasHandlerError::Internal(
                    "batch-read semaphore closed".into(),
                ));
            }
        };
        let read = state.read.clone();
        let tombstones = state.tombstones.clone();
        let tenant = auth.0.clone();
        let hash = hash.clone();
        let lease = Arc::clone(&lease);
        let read_budget = Arc::clone(&state.read_budget);
        tasks.push(tokio::spawn(async move {
            // The lease is intentionally held for the complete task lifetime,
            // including a synchronous `read()` bridged through block_in_place.
            let _lease = lease;
            let _window_permit = permit;
            // Non-canonical hash ⇒ it cannot name a stored blob; report absent
            // (it is not a framing error and must not abort the batch).
            if !super::ac::is_canonical_digest(&hash) {
                return BatchReadOutcome::Absent;
            }
            let global_permit = match acquire_cas_read_budget(
                read_budget,
                CAS_READ_BATCH_OBJECT_PERMITS,
                "batch-read-object",
            )
            .await
            {
                Ok(permit) => permit,
                Err(_) => return BatchReadOutcome::BudgetFault,
            };
            // Tombstone gate FIRST (mirrors handle_read): an erased blob is
            // `gone`, never resurrected, never reported absent. A lookup fault
            // fails CLOSED for the whole batch (the erase write-side is the
            // source of truth).
            if let Some(tombstones) = tombstones.as_ref() {
                match tombstones.is_tombstoned(&tenant, &hash).await {
                    Ok(true) => return BatchReadOutcome::Gone,
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "cas batch-read: tombstone gate lookup failed; failing CLOSED (503)");
                        return BatchReadOutcome::TombstoneFault;
                    }
                }
            }
            let req = CasReadRequest::new(
                tenant.clone(),
                hash.clone(),
                format!("anon@{tenant}"),
                tenant.clone(),
                now_ms,
            )
            .with_max_bytes(BATCH_MAX_BYTES as u64);
            // Sync `read()` — `block_in_place` inside it is valid because this
            // is a multi-thread-runtime worker thread (`tokio::spawn`).
            match read.read(req) {
                Ok(resp) => BatchReadOutcome::Ok {
                    bytes: resp.bytes,
                    _global_permit: global_permit,
                },
                Err(CasHandlerError::NotFound { .. }) => BatchReadOutcome::Absent,
                Err(e) => BatchReadOutcome::ReadErr(e),
            }
        }));

        // Drain the oldest result as soon as the bounded window is full. On a
        // terminal result, abort and await every pending task before returning.
        if tasks.len() == BATCH_READ_FANOUT {
            let outcome = match tasks.await_oldest().await {
                Some(Ok(outcome)) => outcome,
                Some(Err(_)) | None => {
                    tasks.abort_and_drain().await;
                    return map_err(CasHandlerError::Internal("batch read task failed".into()));
                }
            };
            let result_hash = match result_hashes.next() {
                Some(hash) => hash,
                None => {
                    tasks.abort_and_drain().await;
                    return map_err(CasHandlerError::Internal(
                        "batch read result count mismatch".into(),
                    ));
                }
            };
            if let Err(response) = append_outcome(result_hash, outcome) {
                tasks.abort_and_drain().await;
                return response;
            }
        }
    }

    // Drain the tail in request order. This second drain has the same terminal
    // cleanup rule as the full-window path; no `JoinHandle` may outlive the
    // response or an error return.
    while !tasks.is_empty() {
        let outcome = match tasks.await_oldest().await {
            Some(Ok(outcome)) => outcome,
            Some(Err(_)) | None => {
                tasks.abort_and_drain().await;
                return map_err(CasHandlerError::Internal("batch read task failed".into()));
            }
        };
        let result_hash = match result_hashes.next() {
            Some(hash) => hash,
            None => {
                tasks.abort_and_drain().await;
                return map_err(CasHandlerError::Internal(
                    "batch read result count mismatch".into(),
                ));
            }
        };
        if let Err(response) = append_outcome(result_hash, outcome) {
            tasks.abort_and_drain().await;
            return response;
        }
    }
    // Stream the manifest and payload as separate frames. Keeping both guards
    // in this stream prevents the response payload or tenant slot from
    // escaping their reservation when the handler returns; concatenating into
    // `out` would transiently duplicate the payload and releasing the slot
    // here would let a slow client multiply the per-tenant response heap.
    let mut manifest = manifest.into_bytes();
    manifest.push(b'\n');
    let response_lease = Arc::clone(&lease);
    let body_stream = async_stream::stream! {
        // Hold the request-level tenant/global reservations through response
        // consumption, not merely until this handler returns.
        let _lease = response_lease;
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
