impl R2CasHandler {
    /// The tenant-scoped LIST prefix: `<region>/<tenant_prefix>/`. Every
    /// key returned under this prefix belongs to exactly this tenant
    /// (layer 5 of `INV-TENANT-ISOLATION`); the trailing slash bounds
    /// the prefix so one tenant's prefix can never be a prefix of
    /// another's. Enumeration / delete NEVER widen beyond this.
    fn r2_list_prefix(&self, tenant: &str) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(format!("{}/{}/", self.cas_region, prefix))
    }
}

impl CasDeleteHandler for R2CasHandler {
    fn delete(
        &self,
        req: corelink_handler_cas::CasDeleteRequest,
    ) -> Result<corelink_handler_cas::CasDeleteResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;
        use corelink_handler_cas::CasDeleteResponse;

        // Delete folds availability into the PUT (mutation) SLI bucket.
        // B-057: the window the latency SLI reports. Started at handler
        // entry so it covers the same work the availability SLI counts.
        let started = Instant::now();
        let emit = |is_error: bool| {
            self.emit_sli(
                Sli::AvailCasPut,
                Sli::LatencyCasPutP99,
                is_error,
                elapsed_us(started),
            );
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::DeleteDenied,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // DeleteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::DeleteAttempted,
                req.tenant.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        // CRITICAL — `block_in_place`: see `read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: delete must target the §4-hardened physical key for an
        // active tenant (audit H-4), matching what `write`/`read` stored. Non-BYOK
        // tenants resolve to the raw digest (unchanged); an active-but-unresolvable
        // tenant fails CLOSED rather than delete the wrong (or no) key. BYOK Wave
        // 4a: the resolved plan also drives the Mode-B `byok_envelope` reclaim
        // performed AFTER the R2 object is removed (see below).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.hash, DigestAlgo::Blake3))
        }) {
            Ok(r) => r,
            Err(e) => {
                emit(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix. DELETE is native-only
        // (the Bazel REAPI bridge exposes no delete surface), so the BLAKE3
        // keyspace applies.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest, DigestAlgo::Blake3) {
            Ok(k) => k,
            Err(e) => {
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, "R2CasHandler::delete");

        // S3 DeleteObject is idempotent: deleting an absent key succeeds.
        // `existed` is best-effort (S3 does not report prior presence on a
        // plain DeleteObject); we report `true` on a clean delete so the
        // diagnostic is monotone, never a silent success on a transport
        // error. CRITICAL — `block_in_place`: see `read` above.
        //
        // Storage byte-accounting (finding #1 / cluster-C) + concurrent
        // double-DELETE over-release (rt-nuclear #6/#10/#14): the size measurement
        // and the delete are serialized per-key by `delete_if_present`, which
        // returns the reclaimed size to AT MOST ONE racer — every other racer sees
        // the object already gone and gets `None` (releases 0). This makes the
        // release reflect what THIS request actually removed, so two racing
        // deletes can never both credit the same bytes. CRITICAL — `block_in_place`:
        // see `read` above.
        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.client.delete_if_present(&key)))
        };

        match result {
            Ok(prior) => {
                let reclaimed = prior.unwrap_or(0);
                // BYOK Wave 4a: the R2 object is now gone — reclaim the Mode-B
                // `byok_envelope` row so the deleted blob's wrapped DEK does not
                // linger (orphan key-material). A reclaim failure is the SAFE-fail
                // direction (the DEK wraps nothing now), so WARN + continue — the
                // delete still SUCCEEDS and the R2 delete is NOT rolled back.
                if let Err(e) = tokio::task::block_in_place(|| {
                    handle.block_on(self.reclaim_byok_envelope(&resolved.plan))
                }) {
                    warn!(
                        error = %e, key = %key, tenant = %req.tenant,
                        "R2CasHandler::delete byok_envelope reclaim failed (orphan wrapped-DEK row; blob already deleted)"
                    );
                }
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::DeleteCommitted,
                        req.tenant.clone(),
                        req.hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                emit(false);
                Ok(CasDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
            }
            Err(e) => {
                // Fail CLOSED on a storage fault: never a silent success.
                warn!(error = %e, key = %key, "R2CasHandler::delete error");
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}

impl CasListHandler for R2CasHandler {
    fn list(
        &self,
        req: corelink_handler_cas::CasListRequest,
    ) -> Result<corelink_handler_cas::CasListResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;
        use corelink_handler_cas::{CasBlobEntry, CasListResponse};

        // List folds availability into the GET (read) SLI bucket.
        // B-057: the window the latency SLI reports. Started at handler
        // entry so it covers the same work the availability SLI counts.
        let started = Instant::now();
        let emit = |is_error: bool| {
            self.emit_sli(
                Sli::AvailCasGet,
                Sli::LatencyCasGetP99,
                is_error,
                elapsed_us(started),
            );
        };

        // Cross-tenant denial — audit BEFORE returning.
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::ListDenied,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // ListAttempted audit and the R2 enumeration.
        //
        // Enumeration is bounded to the tenant's derived prefix — cross-tenant
        // keys cannot appear in the result. Fail CLOSED if the prefix is not
        // derivable: an empty prefix would list a SHARED keyspace across every
        // non-derivable tenant. Both branches below log their own R2 error
        // (with whatever prefix they had, if any) INSIDE the branch, so the
        // shared post-join code only has to deal in `CasHandlerError`.
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<(Vec<(String, u64, String)>, Option<String>), CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                //
                // The mandatory `ListAttempted` audit write and the R2
                // `ListObjectsV2` call now run CONCURRENTLY — one
                // `tokio::join!` under a single `block_in_place` +
                // `block_on`, instead of two serial round trips (measured:
                // ~236ms total on IAD, ~30-36ms `ostore` + ~108-120ms
                // `oother` dominated by this blocking D1 INSERT — see the
                // PR).
                //
                // FAIL-CLOSED IS PRESERVED. The two calls are dispatched
                // together, but NEITHER result is used — and no bytes are
                // served — until BOTH have completed and are checked BELOW,
                // in the SAME order the old serial code checked them: the
                // audit result first (`AuditFailed` short-circuits exactly
                // as it did when the calls were serial), the store result
                // second. What changes is that a failing audit no longer
                // PREVENTS the R2 call from having been issued (it can no
                // longer prevent it — they started together); what does NOT
                // change is that a failing audit still prevents any R2
                // result from ever reaching the caller. "No bytes served
                // without the audit row" holds; "no R2 request issued
                // without the audit row" is the guarantee traded away for
                // the latency win, and it was never a stated invariant —
                // only "audit before enumeration" was, and enumeration
                // (returning rows to the caller) still cannot happen without
                // the audit row.
                //
                // Cross-tenant denial (above) is UNAFFECTED — it returns
                // before this point and never joins anything.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let prefix = self
                        .r2_list_prefix(&req.tenant)
                        .map_err(CasHandlerError::Internal)?;
                    debug!(prefix = %prefix, "R2CasHandler::list");
                    self.client
                        .list_objects_page(&prefix, req.limit, req.cursor.as_deref())
                        .await
                        .map_err(|e| {
                            warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                            CasHandlerError::Internal(e)
                        })
                };
                // ONE `Phase::Store` scope wraps the ENTIRE joined window —
                // NOT two separate scopes (one per future) — so the
                // overlapping wall time is attributed exactly once.
                // `oaudit` (`Phase::Audit`) is deliberately NOT entered
                // here: `append_async` (which this drives) skips its own
                // scope for exactly this reason. See `origin_timing.rs`'s
                // "Concurrent native-plane list seam" note — without this,
                // the two phases would double-count the same wall-clock
                // window and the `Σ(phases) ≤ total` invariant would break.
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior. Taken whenever `audit_async` is unset (every
                // test handler in this module, and any future `AuditSink`
                // impl that is not the durable D1 sink).
                let audit_result = self.audit.emit(AuditEvent::new(
                    AuditEventKind::ListAttempted,
                    req.tenant.clone(),
                    String::new(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching the prefix or
                    // R2 — never compute or evaluate the store side here.
                    Ok((Vec::new(), None))
                } else {
                    let prefix = match self.r2_list_prefix(&req.tenant) {
                        Ok(p) => p,
                        Err(e) => {
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    };
                    debug!(prefix = %prefix, "R2CasHandler::list");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| {
                            handle.block_on(self.client.list_objects_page(
                                &prefix,
                                req.limit,
                                req.cursor.as_deref(),
                            ))
                        })
                    };
                    match result {
                        Ok(ok) => Ok(ok),
                        Err(e) => {
                            warn!(error = %e, prefix = %prefix, "R2CasHandler::list error");
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        match store_result {
            Ok((rows, next_cursor)) => {
                let blobs = rows
                    .into_iter()
                    .map(|(key, size, last_modified)| {
                        // Strip `<region>/<tenant_prefix>/` to recover the
                        // bare digest; never leak the storage key layout.
                        let hash = key.rsplit('/').next().unwrap_or(&key).to_owned();
                        CasBlobEntry::new(hash, size, last_modified)
                    })
                    .collect();
                emit(false);
                Ok(CasListResponse::new(blobs, next_cursor))
            }
            Err(e) => {
                // Already `warn!`-logged (with prefix, where one was
                // available) at the point the error was produced above.
                emit(true);
                Err(e)
            }
        }
    }
}

/// Build an `R2CasHandler` from environment variables.
///
/// Returns `None` when storage credentials are not configured (dev /
/// unit-test mode). Callers fall back to `InMemoryCasHandler`.
///
/// # Errors
///
/// Returns `Err(String)` if credentials are present but the S3 client
/// cannot be constructed (e.g. endpoint URL is malformed).
pub async fn build_r2_cas_handler_from_env(
    bucket: &str,
    cas_region: &str,
) -> Option<Result<R2CasHandler, String>> {
    if let Err(e) = validate_cas_bucket_for_region(bucket, cas_region) {
        tracing::error!(bucket = %bucket, region = %cas_region, error = %e,
            "refusing CAS handler with non-residency bucket (fail-closed)");
        return Some(Err(e));
    }
    let env = super::StorageEnv::from_env()?;
    // FAIL CLOSED: storage creds are present, so this is the production
    // data plane — the secret TDK is MANDATORY (F1/F2). Without it the
    // tenant prefix would degrade to a public, predictable scheme and
    // enable same-millisecond cross-tenant blob co-residence. Refuse to
    // construct the handler (the route will not mount) and emit a loud,
    // structured error rather than serving in the silently-degraded
    // public-prefix mode.
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to mount the R2 CAS handler (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    // F1 (CAA-360) fail-CLOSED: storage creds ARE present, so this is the
    // production data plane. The audit trail MUST be DURABLE — a volatile
    // `InMemoryAuditSink` here loses every CAS audit event on restart AND makes
    // the route's `AuditFailed → 503` guard dead code (in-memory emit only
    // errors under a test-injected failure). Wire the D1 `audit_outbox` sink
    // (the same trail the S-09 drain seals); if it cannot be constructed,
    // REFUSE to build the handler — the route mounts the fail-CLOSED 503
    // handler, never a silent in-memory fallback (mirrors the `R2_TDK_HEX`
    // refusal above).
    //
    // The CONCRETE `Arc<D1AuditOutboxSink>` is kept (not just the
    // type-erased `Arc<dyn AuditSink>`) so it can ALSO be wired as the
    // handler's `audit_async` seam below — same sink instance, two views:
    // the sync `AuditSink` trait object every non-list call still uses, and
    // the concrete type `list()` uses to `tokio::join!` the audit write
    // with the R2 enumeration (perf: concurrent native-plane list).
    // B071: the live CAS writer has an explicit D1 fence dependency. Keep a
    // dedicated client for it so a future audit-sink refactor cannot silently
    // drop the writer-side lease wiring.
    let cas_write_fence = match D1HttpClient::new(&env) {
        Ok(d1) => Arc::new(crate::storage::cas_write_fence::D1CasWriteFence::new(
            Arc::new(d1),
        )),
        Err(e) => {
            tracing::error!(
                error = %e,
                "CAS write D1 fence unavailable; refusing to mount the R2 CAS handler"
            );
            return Some(Err(e));
        }
    };
    let audit_concrete = match cas_audit_sink_from_d1_concrete(D1HttpClient::new(&env)) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                error = %e,
                "durable CAS audit sink unavailable with storage creds present; \
                 refusing to mount the R2 CAS handler (fail-closed, F1)"
            );
            return Some(Err(e));
        }
    };
    let audit: Arc<dyn AuditSink> = audit_concrete.clone();
    // B-057: constant-memory aggregate, NOT the capture-everything test
    // observer. The previous `InMemorySliObserver` here retained every
    // observation in a `Vec` for the life of the container process, with
    // no production reader of `snapshot()`/`count()` anywhere.
    let sli = crate::sli_aggregate::shared();
    Some(Ok(R2CasHandler::new(
        client,
        cas_region,
        Some(tdk_bytes),
        audit,
        sli,
    )
    .with_async_audit(audit_concrete)
    .with_cas_write_fence(cas_write_fence)))
}

/// Validate the physical CAS bucket/region pairing before any handler is
/// constructed. A key prefix is not residency: the R2 bucket is the physical
/// location boundary. Unknown or legacy regional pairings fail closed instead
/// of silently writing non-IAD data into `corelink-cas-prod`.
pub(crate) fn validate_cas_bucket_for_region(bucket: &str, region: &str) -> Result<(), String> {
    let expected = match region {
        "iad" => "corelink-cas-prod",
        "lhr" => "corelink-cas-eu",
        "nrt" => "corelink-cas-apac",
        other => {
            return Err(format!(
                "no physically residency-bound CAS bucket is provisioned for region '{other}'"
            ));
        }
    };
    if bucket != expected {
        return Err(format!(
            "CAS bucket '{bucket}' is not the physical residency bucket for region '{region}' (expected '{expected}')"
        ));
    }
    Ok(())
}

/// A sync `AcLookupHandler` + `AcUpdateHandler` backed by [`R2S3Client`].
///
/// Mirrors `R2CasHandler` exactly — bridges async S3 I/O to the sync
/// AC handler trait surface via `tokio::runtime::Handle::current().block_on(...)`.
/// Wired through `routes::ac::build_handlers` when storage credentials
/// are configured; otherwise the route falls back to `InMemoryAcHandler`.
///
/// # Key scheme
///
/// AC entries reuse the canonical
/// `<region>/<tenant_prefix_16>/<action_digest>` key layout from CAS,
/// only the bucket differs (`R2_AC_BUCKET` / default `corelink-ac-iad`).
/// Per-tenant prefix isolation (layer 5 of `INV-TENANT-ISOLATION`)
/// applies identically.
pub struct R2AcHandler {
    client: R2S3Client,
    /// The R2 region string (e.g. `"iad"`) used as key prefix.
    ac_region: String,
    /// Tenant derivation key — see [`R2CasHandler`].
    tdk: Option<TenantDerivationKey>,
    audit: Arc<dyn corelink_handler_ac::AuditSink>,
    /// Narrow async-capable seam onto the SAME sink as `audit` — AC
    /// counterpart of [`R2CasHandler::audit_async`]; see there for the full
    /// rationale. `list()` uses this; `None` keeps it fully serial.
    audit_async: Option<Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>>,
    sli: Arc<dyn corelink_handler_ac::SliObserver>,
    /// BYOK Wave 3b (GATED-INERT): per-tenant BYOK config cache. `None` on the
    /// non-BYOK build / tests → the plaintext path runs unchanged. When `Some`
    /// AND a tenant is `active`, the AC update/lookup path encrypts the
    /// `result_payload` at rest under the `"ac"` surface (closes audit H1).
    /// Mirrors [`R2CasHandler::byok_config_cache`].
    byok_config_cache: Option<Arc<ByokConfigCache>>,
    /// BYOK Wave 3b: the Tcs resolver (CMK-unwrap → convergence secret). `None`
    /// → plaintext path. Both this and `byok_config_cache` must be `Some` for
    /// AC encryption to engage. Mirrors [`R2CasHandler::tcs_resolver`].
    tcs_resolver: Option<Arc<TcsResolver>>,
    /// BYOK Wave 3c: the Mode-B (random-DEK) encryptor + `byok_envelope` store
    /// for the AC surface. Mirrors [`R2CasHandler::byok_mode_b`].
    byok_mode_b: Option<Arc<ModeBEncryptor>>,
}

impl core::fmt::Debug for R2AcHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2AcHandler")
            .field("ac_region", &self.ac_region)
            .finish_non_exhaustive()
    }
}

impl R2AcHandler {
    /// Construct an `R2AcHandler`. See [`R2CasHandler::new`] for the
    /// `tdk_bytes` semantics (pass `None` in tests with a dev/zero TDK).
    #[must_use]
    pub fn new(
        client: R2S3Client,
        ac_region: impl Into<String>,
        tdk_bytes: Option<Zeroizing<[u8; 32]>>,
        audit: Arc<dyn corelink_handler_ac::AuditSink>,
        sli: Arc<dyn corelink_handler_ac::SliObserver>,
    ) -> Self {
        let tdk = tdk_bytes.map(TenantDerivationKey::from_bytes);
        Self {
            client,
            ac_region: ac_region.into(),
            tdk,
            audit,
            audit_async: None,
            sli,
            byok_config_cache: None,
            tcs_resolver: None,
            byok_mode_b: None,
        }
    }

    /// AC counterpart of [`R2CasHandler::with_async_audit`] — same
    /// same-sink invariant applies (`audit_async` MUST be the same
    /// `D1AuditOutboxSink` behind `audit`).
    #[must_use]
    pub fn with_async_audit(
        mut self,
        audit_async: Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>,
    ) -> Self {
        self.audit_async = Some(audit_async);
        self
    }

    /// Attach the BYOK Wave-3b collaborators (config cache + Tcs resolver),
    /// enabling convergent encryption-at-rest of the AC `result_payload` for
    /// `active` tenants. Mirrors [`R2CasHandler::with_byok`].
    ///
    /// GATED-INERT: encryption engages ONLY for a tenant whose
    /// `tenant_byok_config.state == 'active'`; every other tenant (and the
    /// `_public` namespace) keeps the exact plaintext path. The production
    /// builder ([`build_r2_ac_handler_from_env`]) does NOT call this yet —
    /// onboarding (the sole writer of the `active` state, and the prod
    /// `KmsProvider` wiring) is a later wave.
    #[must_use]
    pub fn with_byok(
        mut self,
        byok_config_cache: Arc<ByokConfigCache>,
        tcs_resolver: Arc<TcsResolver>,
    ) -> Self {
        self.byok_config_cache = Some(byok_config_cache);
        self.tcs_resolver = Some(tcs_resolver);
        self
    }

    /// Attach the BYOK Wave-3c Mode-B (random-DEK) encryptor for the AC surface.
    /// Mirrors [`R2CasHandler::with_byok_random`].
    #[must_use]
    pub fn with_byok_random(mut self, mode_b: Arc<ModeBEncryptor>) -> Self {
        self.byok_mode_b = Some(mode_b);
        self
    }

    /// Resolve the BYOK plan for an AC `(tenant, action_digest)`: the §4-hardened
    /// physical key digest (audit H-4) + the body crypto plan, binding the
    /// `"ac"` surface ([`ac_crypto_context`]) so an AC blob is domain-separated
    /// from CAS. Mirrors [`R2CasHandler::resolve_byok`].
    async fn resolve_byok(
        &self,
        tenant: &str,
        action_digest: &str,
    ) -> Result<ByokResolved, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        let plaintext = || ByokResolved {
            physical_digest: action_digest.to_owned(),
            plan: ByokBodyPlan::Plaintext,
        };
        let (Some(cache), Some(resolver)) =
            (self.byok_config_cache.as_ref(), self.tcs_resolver.as_ref())
        else {
            return Ok(plaintext());
        };
        // `_public` is deterministic public content — never encrypted (dedup).
        if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            return Ok(plaintext());
        }
        let Some(cfg) = cache
            .get(tenant)
            .await
            .map_err(|e| AcHandlerError::Internal(format!("byok config read: {e}")))?
        else {
            return Ok(plaintext());
        };
        match engagement_for(&cfg) {
            ByokEngagement::Plaintext => Ok(plaintext()),
            ByokEngagement::FailClosed(why) => Err(AcHandlerError::Internal(format!(
                "byok active but {why}; refusing to fall back to plaintext (fail-closed)"
            ))),
            ByokEngagement::Encrypt(mode) => {
                let key_id = cfg.cmk_key_id.clone().unwrap_or_default();
                let tcs = resolver
                    .resolve(&cfg)
                    .await
                    .map_err(|e| AcHandlerError::Internal(format!("byok tcs resolve: {e}")))?;
                let physical_digest = harden_digest(&tcs, action_digest);
                let plan = match mode {
                    ByokCryptoMode::Convergent => ByokBodyPlan::Convergent {
                        tcs,
                        ctx: ac_crypto_context(tenant, action_digest, &key_id),
                    },
                    ByokCryptoMode::Random => {
                        if self.byok_mode_b.is_none() {
                            return Err(AcHandlerError::Internal(
                                "byok active Mode B (random) but the random-mode encryptor is \
                                 not wired; refusing to fall back to plaintext (fail-closed)"
                                    .to_owned(),
                            ));
                        }
                        ByokBodyPlan::Random {
                            ctx: ac_crypto_context_for(
                                tenant,
                                action_digest,
                                &key_id,
                                CryptoMode::Random,
                            ),
                        }
                    }
                };
                Ok(ByokResolved {
                    physical_digest,
                    plan,
                })
            }
        }
    }

    /// Encrypt the AC body for a resolved plan (`None` ⇒ store plaintext). See
    /// [`R2CasHandler::encrypt_body`].
    async fn encrypt_body(
        &self,
        plan: &ByokBodyPlan,
        payload: &[u8],
    ) -> Result<Option<Vec<u8>>, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        match plan {
            ByokBodyPlan::Plaintext => Ok(None),
            ByokBodyPlan::Convergent { tcs, ctx } => {
                let stored = encrypt_cas_blob(payload, tcs, ctx)
                    .map_err(|e| AcHandlerError::Internal(format!("byok ac encrypt: {e}")))?;
                Ok(Some(stored))
            }
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    AcHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                let stored = mode_b.encrypt(payload, ctx).await.map_err(|e| {
                    AcHandlerError::Internal(format!("byok ac mode-b encrypt: {e}"))
                })?;
                Ok(Some(stored))
            }
        }
    }

    /// Decrypt the stored AC body for a resolved plan. See
    /// [`R2CasHandler::decrypt_body`].
    async fn decrypt_body(
        &self,
        plan: &ByokBodyPlan,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, corelink_handler_ac::AcHandlerError> {
        use corelink_handler_ac::AcHandlerError;
        match plan {
            ByokBodyPlan::Plaintext => Ok(stored),
            ByokBodyPlan::Convergent { tcs, ctx } => decrypt_cas_blob(&stored, tcs, ctx)
                .map_err(|e| AcHandlerError::Internal(format!("byok ac decrypt: {e}"))),
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    AcHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                mode_b
                    .decrypt(&stored, ctx)
                    .await
                    .map_err(|e| AcHandlerError::Internal(format!("byok ac mode-b decrypt: {e}")))
            }
        }
    }

    /// BYOK Wave 4a — reclaim the Mode-B `byok_envelope` row for the AC surface
    /// after the R2 object is deleted. Mirrors [`R2CasHandler::reclaim_byok_envelope`]
    /// (only `Random` writes a row; same R2-first ordering + warn-on-failure
    /// safe-fail rationale). The plan's `ctx` carries `AC_SURFACE`, so the
    /// reclaimed key is `ac:<digest>` (surface-correct).
    async fn reclaim_byok_envelope(&self, plan: &ByokBodyPlan) -> Result<(), String> {
        if let ByokBodyPlan::Random { ctx } = plan {
            if let Some(mode_b) = self.byok_mode_b.as_ref() {
                return mode_b.reclaim(ctx).await;
            }
        }
        Ok(())
    }

    /// BYOK AC write hook (test-facing): resolve + encrypt the body. The
    /// production `update` path resolves ONCE and calls [`Self::encrypt_body`].
    #[cfg(test)]
    async fn byok_encrypt_for_update(
        &self,
        req: &corelink_handler_ac::AcUpdateRequest,
    ) -> Result<Option<Vec<u8>>, corelink_handler_ac::AcHandlerError> {
        let resolved = self.resolve_byok(&req.tenant, &req.action_digest).await?;
        self.encrypt_body(&resolved.plan, &req.result_payload).await
    }

    /// BYOK AC read hook (test-facing): resolve + decrypt the stored body. The
    /// production `lookup` path resolves ONCE and calls [`Self::decrypt_body`].
    #[cfg(test)]
    async fn byok_decrypt_for_lookup(
        &self,
        tenant: &str,
        action_digest: &str,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, corelink_handler_ac::AcHandlerError> {
        let resolved = self.resolve_byok(tenant, action_digest).await?;
        self.decrypt_body(&resolved.plan, stored).await
    }

    /// BYOK AC delete-reclaim hook (test-facing): resolve + reclaim the Mode-B
    /// `byok_envelope` row (surface = `ac`), mirroring the post-R2-delete step in
    /// the production AC `delete` path.
    #[cfg(test)]
    async fn byok_reclaim_for_delete(
        &self,
        tenant: &str,
        action_digest: &str,
    ) -> Result<(), String> {
        let resolved = self
            .resolve_byok(tenant, action_digest)
            .await
            .map_err(|e| format!("resolve: {e}"))?;
        self.reclaim_byok_envelope(&resolved.plan).await
    }

    /// Derive the R2 key for a (tenant, action_digest) pair. Mirrors
    /// `R2CasHandler::r2_key`; the AC bucket uses the same layout and
    /// the same always-HMAC tenant prefix (F1/F2). The handler cannot
    /// be built without a TDK on the production path (see
    /// [`build_r2_ac_handler_from_env`]).
    fn r2_key(&self, tenant: &str, action_digest: &str) -> Result<String, String> {
        if !is_canonical_ac_digest(action_digest) {
            return Err("non-canonical AC action digest (lowercase 64-hex required)".to_owned());
        }
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        // AC keys are native (BLAKE3) keyspace — REAPI AC action digests are
        // stored under the same scheme as native CAS (no `bazel/sha256/` tag).
        Ok(R2S3Client::blob_key(
            &self.ac_region,
            &prefix,
            action_digest,
            DigestAlgo::Blake3,
        ))
    }

    /// Emit the (avail, latency) SLI pair for the lookup path. The
    /// update path folds availability into `AvailAcLookup` per the
    /// canonical-15 metric registry discipline (see
    /// `InMemoryAcHandler::update`).
    fn emit_lookup_sli(&self, is_error: bool, latency_us: u64) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli.observe(SliObservation::new(
            Sli::AvailAcLookup,
            is_error,
            latency_us,
        ));
        self.sli.observe(SliObservation::new(
            Sli::LatencyAcHitP99,
            is_error,
            latency_us,
        ));
    }

    /// LIST has a customer-visible availability/latency window, but it is not
    /// an AC lookup hit. Keep the hit-latency catalog honest by excluding list
    /// operations from `LatencyAcHitP99`.
    fn emit_list_sli(&self, is_error: bool, latency_us: u64) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli.observe(SliObservation::new(
            Sli::AvailAcLookup,
            is_error,
            latency_us,
        ));
    }

    fn emit_update_sli(&self, is_error: bool, latency_us: u64) {
        use corelink_handler_ac::{Sli, SliObservation};
        self.sli.observe(SliObservation::new(
            Sli::AvailAcLookup,
            is_error,
            latency_us,
        ));
    }
}
