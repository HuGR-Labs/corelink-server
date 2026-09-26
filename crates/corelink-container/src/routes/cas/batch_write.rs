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
    (auth, scope, headers): CasRequestAuth,
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
    let staging_context = match staging_cas_write_context(&headers).await {
        Ok(context) => context,
        Err(()) => return (StatusCode::FORBIDDEN, "invalid staging admission").into_response(),
    };
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
        match state
            .write
            .write_with_effect_and_context(req, staging_context.clone())
        {
            Ok(resp) => {
                let status = if resp.durable { "created" } else { "exists" };
                results.push(serde_json::json!({
                    "hash": entry.hash, "status": status, "error": serde_json::Value::Null,
                }));
            }
            Err(failure) => {
                let msg = batch_object_error_message(&failure.cause);
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
