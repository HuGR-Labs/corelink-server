
/// Gate layer for the cargo surface: per-operation cache-scope enforcement +
/// two-layer write enforcement (F27).
///
/// Runs as a `layer` on the adapter router (which is a catch-all `/*path`, so
/// it matches the nested `/cargo/<tenant>/<key>` shape directly — no path
/// rewrite needed). The gate enforces:
///
/// 1. **Scope header check** — the Worker-set, server-trusted
///    `x-corelink-scope` header must carry the required capability (read or
///    write). An insufficient scope → 403.
/// 2. **PAT-derived `can_write` for writes (F27)** — for PUT requests the gate
///    ALSO extracts the bearer PAT and calls
///    `TenantResolver::resolve_with_capability()` to obtain the D1-verified
///    `can_write` bit from the SAME single PAT verification the adapter uses to
///    resolve the tenant (no redundant second verify). A PAT whose D1 record
///    lacks write capability is rejected (403) even if the header says `cas:rw`.
///    This makes cargo two-layer for writes, matching the OCI model.
/// 3. **$-ceiling gate** — per-tenant monthly cost check (ADR-0068).
///
/// Read requests (GET/HEAD) are NOT subject to the extra `can_write` check; the
/// header check alone is sufficient for reads (the PAT resolver in the adapter
/// still runs and rejects any invalid PAT → 401).
async fn cargo_gate(
    axum::extract::State(state): axum::extract::State<CargoGateState>,
    mut req: Request,
    next: Next,
) -> Response {
    let scope = req
        .headers()
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // WebDAV MKCOL: opendal (sccache's WebDAV backend) creates parent "directories"
    // before PUTting a sharded key (e.g. `6/b/4/<hash>`). The cargo store is a FLAT
    // content-addressed KV — directories are implicit — so MKCOL is a success no-op.
    // Gate it on cache-WRITE scope (it is part of a write flow) so the surface stays
    // fail-closed. Without this, opendal's dir-creation 403s and the real `sccache`
    // binary can never write (a real-client gap invisible to curl, which PUTs the
    // sharded key directly and never issues MKCOL). Root-caused live 2026-07-19.
    if req.method().as_str() == "MKCOL" {
        return if requires_cache_write(scope) {
            (StatusCode::CREATED, "").into_response()
        } else {
            (StatusCode::FORBIDDEN, "insufficient cache scope").into_response()
        };
    }

    // WebDAV PROPFIND (opendal "stat"): sccache's opendal WebDAV backend PROPFINDs
    // a key (Depth: 0) to check existence + size around the `.sccache_check`
    // write-probe and directory handling. The flat content-addressed KV has no
    // native stat verb, so we synthesize a WebDAV `207 Multi-Status` from the
    // per-tenant moat lookup (present) or `404` (absent — opendal then proceeds to
    // write). It is a READ, gated on cache-read scope. Short-circuited HERE (like
    // MKCOL) because PROPFIND is a NON-STANDARD method that axum's `MethodRouter`
    // cannot route — falling through to the adapter would 405 and the real
    // `sccache` binary could never stat. Root-caused live 2026-07-19.
    if req.method().as_str() == "PROPFIND" {
        if !requires_cache_read(scope) {
            return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
        }
        // Extract owned inputs BEFORE awaiting so no borrow of `req` (whose body
        // is not `Sync`) crosses an `.await` — that would make the gate future
        // non-`Send` and `from_fn` would reject it.
        let path = webdav_path(&req);
        let bearer = bearer_owned(&req);
        return handle_propfind(
            Arc::clone(&state.moat),
            state.resolver.clone(),
            path,
            bearer,
        )
        .await;
    }

    // WebDAV DELETE (opendal write-check cleanup): sccache PUTs `.sccache_check`,
    // reads it back, then DELETEs it. Remove the per-tenant key→content-hash map
    // row so a later GET/PROPFIND misses (the CAS blob is left for GC). It is a
    // WRITE — gated on cache-write scope AND the PAT's D1 `can_write` bit (F27,
    // mirroring PUT), keyed by the PAT-resolved tenant (NEVER the path tenant).
    // Idempotent: 204 even if the key was absent. Short-circuited here (not routed
    // to the adapter, whose `MethodRouter` has no DELETE route → 405) so ALL of
    // the sccache WebDAV surface lives on ONE path.
    if req.method() == Method::DELETE {
        if !requires_cache_write(scope) {
            return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
        }
        let path = webdav_path(&req);
        let bearer = bearer_owned(&req);
        return handle_delete(
            Arc::clone(&state.moat),
            state.resolver.clone(),
            path,
            bearer,
        )
        .await;
    }

    let is_write = req.method() == Method::PUT;
    let scope_ok = match *req.method() {
        Method::PUT => requires_cache_write(scope),
        Method::GET | Method::HEAD => requires_cache_read(scope),
        // Fail-CLOSED: the adapter only routes GET/HEAD/PUT (so anything else
        // would 405 downstream today), but the gate must not assume that —
        // an unmapped method is denied here so a future adapter route can
        // never ship without an explicit scope decision. No browser/CORS
        // clients exist on this surface, so OPTIONS is not legitimate traffic.
        _ => false,
    };
    if !scope_ok {
        return (StatusCode::FORBIDDEN, "insufficient cache scope").into_response();
    }

    // F27 — two-layer write enforcement: for PUT requests, require the PAT's own
    // `can_write` bit from the resolver's SINGLE PAT verification
    // (`resolve_with_capability` — HMAC + Argon2id against D1). This ensures a
    // Worker-side scope-header mistake cannot grant a write that the PAT's D1
    // record does not authorise — without a redundant second verify (the adapter
    // already verifies the same PAT to resolve the tenant). Read requests skip
    // this (the adapter's resolver still verifies the PAT for auth; only the
    // write-capability cross-check is gated here).
    // REV-S3: for PUT, the PAT-derived tenant id from the F27 verify below is the
    // AUTHORITATIVE cost-attribution key (it cannot be spoofed by a Worker-set
    // header). We capture it here and prefer it over `x-corelink-tenant-id` for
    // the $-ceiling gate.
    let mut resolved_tenant_for_quota: Option<String> = None;
    if is_write {
        let pat_token = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(str::to_owned);
        match pat_token {
            None => {
                tracing::warn!("cargo: PUT with no bearer token — rejecting (F27)");
                return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
            }
            Some(pat_plaintext) => {
                match state.resolver.resolve_with_capability(&pat_plaintext).await {
                    Ok(resolved) if resolved.can_write => {
                        // PAT grants write — both layers pass; continue.
                        // REV-S3: thread the authoritative, crypto-verified
                        // tenant id from THIS single PAT verification into a
                        // typed request extension so the downstream adapter's
                        // `handle_put` reuses it instead of running Argon2id a
                        // second time. Server-internal typed storage — not
                        // client-settable — so this is not a trust downgrade.
                        resolved_tenant_for_quota = Some(resolved.tenant_id.clone());
                        req.extensions_mut()
                            .insert(server::GateResolvedTenant(resolved.tenant_id));
                    }
                    Ok(_no_write) => {
                        tracing::warn!(
                            "cargo: PUT denied — PAT scope lacks write capability (F27)"
                        );
                        return (StatusCode::FORBIDDEN, "PAT does not grant write capability")
                            .into_response();
                    }
                    Err(TenantResolveError::Backend(m)) => {
                        tracing::error!(error = %m, "cargo: resolver backend error (F27)");
                        return (
                            StatusCode::SERVICE_UNAVAILABLE,
                            "PAT verifier backend error",
                        )
                            .into_response();
                    }
                    // `InvalidPat` + any future non-exhaustive variant: fail-CLOSED
                    // (the PAT did not resolve, so the write is denied → 401).
                    Err(e) => {
                        tracing::warn!(error = %e, "cargo: PUT denied — PAT re-verify failed (F27)");
                        return (StatusCode::UNAUTHORIZED, "invalid PAT").into_response();
                    }
                }
            }
        }
    }

    // A supplied staging credential is never ignored. The carrier is inserted
    // only after the normal PAT gate succeeds and then follows the typed CAS
    // context path through the adapter's blocking bridge.
    if is_write {
        let admission = match crate::storage::staging_load_test_admission::admit_staging_load_test_request(
            state.staging_admission.as_deref(),
            req.headers(),
            crate::storage::staging_load_test_ownership::StagingLoadTestScenario::B103CargoWrite,
        )
        .await
        {
            Ok(context) => context,
            Err(_) => return (StatusCode::FORBIDDEN, "invalid staging admission").into_response(),
        };
        if let Some(context) = admission {
            req.extensions_mut().insert(server::GateCasWriteContext(
                Arc::new(crate::storage::cas_write_fence::StagingCasWriteContext::new(context)),
            ));
        }
    }

    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1): charge the
    // flat per-op cost AFTER the scope gate, BEFORE the adapter runs PAT auth +
    // CAS. 402 over-ceiling / 503 fail-CLOSED.
    //
    // REV-S3 (quota fail-OPEN closed): the cost-attribution tenant is sourced,
    // in priority order:
    //   1. the PAT-resolved tenant id from the F27 verify above (PUT only) —
    //      AUTHORITATIVE, cannot be spoofed by a Worker-set header; then
    //   2. the Worker-set, server-trusted `x-corelink-tenant-id` header
    //      (the only source available for reads, where no PAT verify runs in
    //      this gate). A forged header can over-charge only ITS OWN tenant.
    // If the gate is configured but NO tenant id is available, we now fail
    // CLOSED (503) instead of silently skipping the charge: a missing label is
    // a Worker header-injection regression (or direct container access) and
    // must surface immediately rather than letting a tenant exceed its
    // $-ceiling unmetered.
    if let Some(gate) = state.quota.as_ref() {
        let tenant = match resolved_tenant_for_quota {
            Some(ref t) if !t.is_empty() => t.as_str(),
            _ => req
                .headers()
                .get("x-corelink-tenant-id")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .unwrap_or(""),
        };
        if tenant.is_empty() {
            tracing::error!(
                "cargo: quota gate active but no tenant id for cost attribution \
                 (missing x-corelink-tenant-id and no PAT-resolved tenant) — \
                 failing CLOSED (REV-S3)"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "cost-attribution tenant unavailable",
            )
                .into_response();
        }
        if let Some(resp) = gate.check(tenant).await {
            return resp;
        }
    }
    // The Worker is the sole setter of this trusted header; it is stripped from
    // client input before forwarding. Scope the parsed value around the adapter
    // so cargo PUTs reuse it instead of issuing a per-write D1 tier lookup.
    let storage_cap_bytes = crate::byte_accounting::storage_quota_from_headers(req.headers());
    CARGO_STORAGE_QUOTA_CAP
        .scope(storage_cap_bytes, next.run(req))
        .await
}

/// The WebDAV request path used for the `<D:href>` and key extraction.
///
/// Behind `nest_service("/cargo", …)` the middleware may observe the
/// prefix-stripped path (`/<tenant>/<key>`); the outer router preserves the full
/// wire path (`/cargo/<tenant>/<key>`) in the [`axum::extract::OriginalUri`]
/// extension. We prefer the original so the `href` echoes exactly what opendal
/// sent. The key is the LAST path segment either way, so key extraction is
/// unaffected by which one we use. Returns an OWNED `String` so no borrow of the
/// request (whose body is not `Sync`) is held across an `.await`.
fn webdav_path(req: &Request) -> String {
    req.extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|o| o.0.path().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned())
}

/// Extract the `Bearer` PAT as an OWNED `String` (so it is not borrowed from the
/// non-`Sync` request body across an `.await`, which would make the gate future
/// non-`Send` and break `from_fn`).
fn bearer_owned(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_owned)
}

/// `PROPFIND /cargo/<tenant>/<key>` — synthesize a WebDAV stat from the moat.
///
/// Scope (cache-read) is enforced by the caller. Resolves the tenant from the
/// PAT (never the path), mirroring the adapter's GET/HEAD auth, then serves a
/// `207 Multi-Status` (present, with the real byte length) or `404` (absent).
///
/// Takes fully OWNED inputs (the caller extracts them from the request before
/// the first `.await`) so the gate future stays `Send`.
async fn handle_propfind(
    moat: Arc<MoatCache>,
    resolver: SharedTenantResolver,
    path: String,
    bearer: Option<String>,
) -> Response {
    // A collection stat (trailing slash ⇒ empty last segment): opendal stats the
    // tenant/dir root. Answer a minimal `207` collection — it discloses nothing
    // beyond "this is a directory" and needs no moat/auth lookup.
    if path.ends_with('/') {
        return propfind_collection_response(&path);
    }

    // Use the SAME normalization GET/PUT use so the stat resolves the identical
    // moat key. A key those paths would reject cannot exist ⇒ not-found.
    let key = match key_from_path(&path) {
        Some(k) => k,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let tenant_id = match resolve_read_tenant(&resolver, bearer).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    match moat.get(&tenant_id, &key).await {
        Ok(Some(bytes)) => propfind_file_response(&path, bytes.len() as u64),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(MoatError::Backend(m)) => {
            tracing::error!(error = %m, "cargo PROPFIND: moat backend error");
            (StatusCode::BAD_GATEWAY, "cache backend error").into_response()
        }
    }
}

/// `DELETE /cargo/<tenant>/<key>` — remove the per-tenant key→hash map row.
///
/// Scope (cache-write) is enforced by the caller. Applies the F27 second layer
/// (the PAT's D1-verified `can_write` bit) and keys the delete by the
/// PAT-resolved tenant. Idempotent: `204` even if the key was absent. All `req`
/// borrows are dropped before the first `.await` (gate future must be `Send`).
/// Is `key` an sccache build ARTIFACT (a 64-hex object digest), as opposed to
/// one of sccache's non-hex control keys (notably `.sccache_check`)?
///
/// `normalize_key` deliberately admits BOTH shapes — rejecting the control keys
/// is what made the real client deem the backend unusable once — so the two are
/// distinguished here rather than at the door. Only artifacts are worth
/// protecting from eviction; a control key is written and deleted by the client
/// within one health probe and holds nothing.
fn is_object_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

async fn handle_delete(
    moat: Arc<MoatCache>,
    resolver: SharedTenantResolver,
    path: String,
    bearer: Option<String>,
) -> Response {
    // F27 two-layer write: require a bearer PAT whose D1 record grants write.
    let pat = match bearer {
        Some(p) => p,
        None => return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response(),
    };
    let (tenant_id, runner_job) = match resolver.resolve_with_capability(&pat).await {
        Ok(resolved) if resolved.can_write => (resolved.tenant_id, resolved.runner_job),
        Ok(_no_write) => {
            return (StatusCode::FORBIDDEN, "PAT does not grant write capability").into_response();
        }
        Err(TenantResolveError::Backend(m)) => {
            tracing::error!(error = %m, "cargo DELETE: resolver backend error");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "PAT verifier backend error",
            )
                .into_response();
        }
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid PAT").into_response(),
    };

    let key = match key_from_path(&path) {
        Some(k) => k,
        // A key GET/PUT would reject cannot exist ⇒ idempotent no-op success.
        None => return StatusCode::NO_CONTENT.into_response(),
    };

    // 0086 runner-job containment (AUDIT-2026-08-23-CACHE-INTEGRITY-COVERAGE
    // F-1). The native plane refuses CAS DELETE outright for a runner-job
    // credential — "a stolen per-job credential must not be able to EVICT the
    // tenant's cache" — but this plane could not see the marker at all until
    // `PatRow::runner_job` existed, the same structural blindness ADR-0071 closed
    // for `find_only`.
    //
    // ⚠️ The refusal is NARROW ON PURPOSE, and a blanket one would take down the
    // runner fleet. sccache's startup write-check does PUT `.sccache_check` →
    // GET → DELETE, and the runner fabric's own dogfood IS sccache over CoreLink
    // with a runner-minted PAT. A 400 on that control key already cost us a
    // "real client never worked" outage once (see `translate::normalize_key`), so
    // the write-check MUST keep round-tripping. Only a real build ARTIFACT — a
    // 64-hex object key — is protected here; every control key still deletes.
    if runner_job && is_object_key(&key) {
        return (
            StatusCode::FORBIDDEN,
            "artifact delete not permitted for a runner-job credential",
        )
            .into_response();
    }

    match moat.delete(&tenant_id, &key).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(MoatError::Backend(m)) => {
            tracing::error!(error = %m, "cargo DELETE: moat backend error");
            (StatusCode::BAD_GATEWAY, "cache backend error").into_response()
        }
    }
}

/// Resolve the tenant from an OWNED bearer PAT for a READ (never the path
/// tenant). `Err(response)` carries the 401/503 to return on failure.
async fn resolve_read_tenant(
    resolver: &SharedTenantResolver,
    bearer: Option<String>,
) -> Result<String, Response> {
    let pat = match bearer {
        Some(p) => p,
        None => {
            return Err((StatusCode::UNAUTHORIZED, "missing bearer token").into_response());
        }
    };
    match resolver.resolve(&pat).await {
        Ok(t) => Ok(t),
        Err(TenantResolveError::Backend(m)) => {
            tracing::error!(error = %m, "cargo PROPFIND: resolver backend error");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "PAT verifier backend error",
            )
                .into_response())
        }
        Err(_) => Err((StatusCode::UNAUTHORIZED, "invalid PAT").into_response()),
    }
}

/// Fixed `getlastmodified` for every PROPFIND `207`. opendal's WebDAV stat
/// deserializer treats `<D:getlastmodified>` as a REQUIRED field: a 207 without
/// it fails with `missing field getlastmodified`, so the real `sccache` binary
/// flags the whole storage ReadOnly and never writes (invisible to a 207-status
/// check; caught only by a cold-store to warm-HIT round-trip). CAS objects are
/// immutable + content-addressed, so a stable epoch httpdate is correct and keeps
/// the response deterministic. RFC 1123 format.
const PROPFIND_LAST_MODIFIED: &str = "Thu, 01 Jan 1970 00:00:00 GMT";

/// `207 Multi-Status` for an EXISTING key: a `200 OK` propstat carrying
/// `getcontentlength` and `getlastmodified` — the exact shape opendal's WebDAV
/// stat parser accepts (empirically proven against the real `sccache` 0.15 binary).
fn propfind_file_response(href: &str, size_bytes: u64) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>{href}</D:href>\
         <D:propstat><D:prop><D:resourcetype/>\
         <D:getcontentlength>{size_bytes}</D:getcontentlength>\
         <D:getlastmodified>{PROPFIND_LAST_MODIFIED}</D:getlastmodified></D:prop>\
         <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>",
        href = xml_escape(href),
    );
    multistatus_response(body)
}

/// `207 Multi-Status` for a collection (directory) stat — `resourcetype` carries
/// `<D:collection/>`. opendal stats the tenant/dir root; this satisfies it.
fn propfind_collection_response(href: &str) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>{href}</D:href>\
         <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype>\
         <D:getlastmodified>{PROPFIND_LAST_MODIFIED}</D:getlastmodified></D:prop>\
         <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>",
        href = xml_escape(href),
    );
    multistatus_response(body)
}

/// Build a `207 Multi-Status` response with `Content-Type: application/xml`.
fn multistatus_response(body: String) -> Response {
    let mut resp = Response::new(axum::body::Body::from(body));
    *resp.status_mut() = StatusCode::MULTI_STATUS;
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/xml"),
    );
    resp
}

/// Minimal XML text escaping for the `<D:href>` value. Cargo keys are already a
/// constrained safe alphabet (see `key_from_path`), so this is defense-in-depth.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("tests-00-00.rs");
    include!("tests-00-01.rs");
}
