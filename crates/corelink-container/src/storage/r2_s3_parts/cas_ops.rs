/// Metadata and GC tables deliberately store the plain canonical hex digest.
/// The algorithm is already selected by the external route/parser and must not
/// be re-encoded into a second, ambiguous identity at the D1 boundary.
fn canonical_meta_digest(hash: &str) -> String {
    hash.to_owned()
}

impl CasReadHandler for R2CasHandler {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

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
                    AuditEventKind::ReadDenied,
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

        // ReadAttempted audit and the (resolve_byok -> R2 GET) chain.
        //
        // CRITICAL — `block_in_place` rationale: this sync trait method is
        // invoked from inside an async axum handler that is itself being
        // polled on the tokio multi-thread runtime. A bare
        // `handle.block_on(future)` from inside a running future on the
        // SAME runtime hangs forever (observed: 60s curl timeout in prod
        // before this fix). `tokio::task::block_in_place` tells the
        // runtime to release the current worker so the inner `block_on`
        // can drive the future to completion. Valid only on the
        // multi-thread runtime — `#[tokio::main]` gives us that.
        let handle = tokio::runtime::Handle::current();

        // Unlike `list()` (a single R2 call), the read-side store work is a
        // TWO-STEP chain: BYOK Wave 3c resolution (the §4-hardened physical
        // key digest, audit H-4, plus the body crypto plan) THEN the R2 GET
        // keyed off the resolved digest. Both steps run inside `store_fut`
        // below so the WHOLE chain — not just the R2 call — overlaps with
        // the audit write; `resolved` is threaded back out because
        // `decrypt_body` needs `resolved.plan` after the join.
        type StoreResult = Result<(ByokResolved, Option<Vec<u8>>), CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH (production: durable D1 audit sink wired).
                //
                // Mirrors `R2CasHandler::list`'s seam exactly (see that
                // method's doc for the full fail-CLOSED rationale): the
                // mandatory `ReadAttempted` audit write and the
                // resolve_byok-then-R2-GET chain are DISPATCHED together
                // under one `tokio::join!`, but neither result is used —
                // and no bytes are served — until BOTH complete and are
                // checked below, in the SAME order the serial code checked
                // them: audit result first (`AuditFailed` short-circuits
                // exactly as before), store result second. A failing audit
                // no longer prevents the R2 GET from having been
                // *dispatched*; it still prevents any R2 result — and any
                // decrypted bytes — from ever *reaching the caller*.
                //
                // Cross-tenant denial (above) is UNAFFECTED — it returns
                // before this point and never joins anything.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_fut = async {
                    let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
                    let key = self
                        .r2_key(&req.tenant, &resolved.physical_digest, req.algo)
                        .map_err(CasHandlerError::Internal)?;
                    debug!(key = %key, "R2CasHandler::read");
                    let got = self.get_capped_for_read(&key).await?;
                    Ok((resolved, got))
                };
                // ONE `Phase::Store` scope wraps the ENTIRE joined window —
                // see `R2CasHandler::list` for why `oaudit` must NOT also
                // be entered here (it would double-count the overlapping
                // wall-clock window; `append_async` opens no scope of its
                // own for exactly this reason).
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
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result: StoreResult = if audit_result.is_err() {
                    // Mirrors the old code exactly: on audit failure it
                    // returned via `?` before ever touching resolve_byok or
                    // R2 — never compute or evaluate the store side here.
                    // This filler is never read (audit_result is checked
                    // first, below).
                    Ok((
                        ByokResolved {
                            physical_digest: String::new(),
                            plan: ByokBodyPlan::Plaintext,
                        },
                        None,
                    ))
                } else {
                    let resolved = match tokio::task::block_in_place(|| {
                        handle.block_on(self.resolve_byok(&req.tenant, &req.hash, req.algo))
                    }) {
                        Ok(r) => r,
                        Err(e) => {
                            emit(true);
                            return Err(e);
                        }
                    };
                    let key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
                        Ok(k) => k,
                        Err(e) => {
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    };
                    debug!(key = %key, "R2CasHandler::read");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| {
                            handle.block_on(self.get_capped_for_read(&key))
                        })
                    };
                    match result {
                        Ok(got) => Ok((resolved, got)),
                        Err(e) => {
                            emit(true);
                            return Err(e);
                        }
                    }
                };
                (audit_result, store_result)
            };

        if let Err(e) = audit_result {
            emit(true);
            return Err(CasHandlerError::AuditFailed(e));
        }

        let (resolved, stored_opt) = match store_result {
            Ok(ok) => ok,
            Err(e) => {
                // Already `warn!`-logged (with key, where available) at the
                // point the error was produced above.
                emit(true);
                return Err(e);
            }
        };

        match stored_opt {
            Some(stored) => {
                // BYOK Wave 3a/3c (GATED-INERT): for an `active` tenant, decrypt
                // the stored blob to plaintext BEFORE the content-hash re-verify
                // (audit C1: the integrity re-verify MUST run on the PLAINTEXT,
                // never on ciphertext). FAIL-CLOSED: any decrypt / unwrap failure
                // returns Err — the raw stored bytes are NEVER served for an
                // active tenant. `Plaintext`-plan tenants get their bytes back
                // unchanged (byte-identical to today).
                let bytes = match tokio::task::block_in_place(|| {
                    handle.block_on(self.decrypt_body(&resolved.plan, stored))
                }) {
                    Ok(pt) => pt,
                    Err(e) => {
                        emit(true);
                        return Err(e);
                    }
                };

                // Read-path content-addressing RE-verification: the
                // bytes R2 returned MUST still hash to the requested
                // digest. This catches R2 bitrot, storage-tier
                // tampering, or a historically mis-keyed blob BEFORE it
                // is served as trusted CAS content. A mismatch is a
                // `CorrectnessViolation`, never a hit. (See
                // `verify_content_hash`.)
                if let Err(actual) = verify_content_hash(req.algo, &req.hash, &bytes) {
                    self.audit
                        .emit(AuditEvent::new(
                            AuditEventKind::CorrectnessViolation,
                            req.tenant.clone(),
                            req.hash.clone(),
                            req.principal.clone(),
                            req.at_unix_ms,
                        ))
                        .map_err(CasHandlerError::AuditFailed)?;
                    self.sli
                        .observe(SliObservation::new(Sli::CorrectnessCas, true, 0));
                    emit(true);
                    return Err(CasHandlerError::HashMismatch {
                        claimed: req.hash,
                        actual,
                    });
                }
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::ReadServed,
                        req.tenant.clone(),
                        req.hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli
                    .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                emit(false);
                Ok(CasReadResponse::new(bytes, req.hash))
            }
            None => {
                emit(true);
                Err(CasHandlerError::NotFound {
                    tenant: req.tenant,
                    hash: req.hash,
                })
            }
        }
    }

    /// Cheap existence probe — a single S3 `HeadObject`, NO body
    /// download and NO content rehash.
    ///
    /// This is the override that removes the `findMissingBlobs`
    /// egress+rehash amplification (r34 #10): the REAPI bridge probes
    /// existence per digest, and the default trait `exists` would route
    /// through [`Self::read`] (a full GET + read-path re-verify per blob).
    /// Here we HEAD instead — `Ok(true)` when present, `Ok(false)` on
    /// absence, every other error propagated so the fail-CLOSED contract
    /// holds.
    ///
    /// Audit/SLI posture mirrors [`Self::read`]: a cross-tenant probe is
    /// denied (and audited as `ReadDenied`) before any storage touch, and
    /// a `ReadAttempted` row is emitted before the lookup. A HEAD reveals
    /// only presence (not bytes), so there is no read-path content
    /// re-verification and no `ReadServed`/`CorrectnessCas` emission — the
    /// probe never serves trusted content.
    fn exists(&self, req: CasReadRequest) -> Result<bool, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

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

        // Cross-tenant denial — audit BEFORE returning (mirrors `read`).
        if req.tenant != req.caller_tenant {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::ReadDenied,
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

        // ReadAttempted audit and the (resolve_byok -> R2 HEAD) chain —
        // mirrors `read`'s concurrent seam exactly, see there for the full
        // fail-CLOSED rationale. No `resolved`/decrypt step is needed after
        // the join here (a HEAD reveals only presence, never bytes), so
        // `store_fut` only needs to yield `Option<u64>`.
        //
        // CRITICAL — `block_in_place` rationale: see `read` above. This is
        // a HEAD (`head_size`), not a GET — no body transfer, no rehash.
        let handle = tokio::runtime::Handle::current();
        type StoreResult = Result<Option<u64>, CasHandlerError>;

        let (audit_result, store_result): (Result<(), String>, StoreResult) =
            if let Some(audit_async) = self.audit_async.as_ref() {
                // CONCURRENT PATH — see `read` for the full rationale.
                let audit_fut = audit_async.emit_cas_async(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                // The storage half is the SHARED `probe_existence_unaudited`
                // — the ONE body the batch seam (`exists_batch`) also drives,
                // so a single-digest probe and a batched one can never drift
                // on which key they HEAD or on their fail-CLOSED handling.
                let store_fut = self.probe_existence_unaudited(&req);
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| {
                    handle.block_on(async { tokio::join!(audit_fut, store_fut) })
                })
            } else {
                // SERIAL FALLBACK — byte-identical to the pre-existing
                // behavior.
                let audit_result = self.audit.emit(AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ));
                let store_result: StoreResult = if audit_result.is_err() {
                    Ok(None)
                } else {
                    let resolved = match tokio::task::block_in_place(|| {
                        handle.block_on(self.resolve_byok(&req.tenant, &req.hash, req.algo))
                    }) {
                        Ok(r) => r,
                        Err(e) => {
                            emit(true);
                            return Err(e);
                        }
                    };
                    let key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
                        Ok(k) => k,
                        Err(e) => {
                            emit(true);
                            return Err(CasHandlerError::Internal(e));
                        }
                    };
                    debug!(key = %key, "R2CasHandler::exists");
                    let result = {
                        let _scope = crate::origin_timing::PhaseScope::enter(
                            crate::origin_timing::Phase::Store,
                        );
                        tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)))
                    };
                    match result {
                        Ok(ok) => Ok(ok),
                        Err(e) => {
                            warn!(error = %e, key = %key, "R2CasHandler::exists error");
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
            Ok(Some(_)) => {
                emit(false);
                Ok(true)
            }
            Ok(None) => {
                emit(false);
                Ok(false)
            }
            Err(e) => {
                // Already `warn!`-logged (with key, where available) at the
                // point the error was produced above.
                emit(true);
                Err(e)
            }
        }
    }

    /// Batch existence probe — the `findMissingBlobs` seam. Collapses the
    /// N `ReadAttempted` audit writes into ONE D1 round trip and runs the N
    /// R2 `HeadObject` probes with bounded concurrency, instead of
    /// `2N` strictly serial public-endpoint round trips.
    ///
    /// # Measured defect this fixes
    ///
    /// `POST /bazel/v2/{instance}/findMissingBlobs` was perfectly linear at
    /// ~268 ms per digest against prod (2.10 / 5.74 / 13.48 / 26.86 s for
    /// 5 / 20 / 50 / 100 digests), i.e. ~18 minutes at the documented
    /// `FIND_MISSING_BLOB_CAP` of 4096. `Server-Timing` decomposed a 40-digest
    /// call (7961 ms total) into 40 blocking D1 audit writes (~122 ms each)
    /// plus 40 R2 HEADs (~60 ms each), all serial — the loop, not the work.
    ///
    /// # `None` ⇒ serial fallback
    ///
    /// Returns `None` when the async audit seam is not wired
    /// (`audit_async == None`: every test handler in this module, and any
    /// non-D1 `AuditSink`), so those callers keep looping [`Self::exists`],
    /// byte-identical to before this method existed. Mirrors exactly how the
    /// concurrent `list()` seam was introduced.
    ///
    /// # Fail-CLOSED
    ///
    /// The audit batch and the probes are DISPATCHED together, but the audit
    /// result is evaluated FIRST and an `Err` short-circuits to
    /// `AuditFailed` — no probe result can reach the caller without its audit
    /// rows having committed. Cross-tenant denial happens strictly before
    /// anything is dispatched, and still emits its own `ReadDenied` row. See
    /// `list()`'s doc for the full argument; this path is the same shape.
    fn exists_batch(&self, reqs: &[CasReadRequest]) -> Option<Result<Vec<bool>, CasHandlerError>> {
        // No async-capable durable sink ⇒ no batch capability ⇒ the caller
        // uses the unchanged per-digest `exists()` loop.
        let audit_async = Arc::clone(self.audit_async.as_ref()?);
        Some(self.exists_batch_inner(&audit_async, reqs))
    }
}

impl R2CasHandler {
    /// The body of [`CasReadHandler::exists_batch`], reached only once the
    /// async audit seam is known to be wired.
    fn exists_batch_inner(
        &self,
        audit_async: &crate::storage::d1_audit_sink::D1AuditOutboxSink,
        reqs: &[CasReadRequest],
    ) -> Result<Vec<bool>, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

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

        // STRICTLY FIRST, before ANY dispatch: cross-tenant denial, audited
        // as `ReadDenied` through the SYNC sink exactly as `exists()` does.
        // Scanning the whole slice before dispatching (rather than per
        // digest, interleaved) is what makes "denial precedes dispatch" true
        // for the batch as a whole — one poisoned digest must not have let
        // the others touch R2 first.
        for req in reqs {
            if req.tenant != req.caller_tenant {
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::ReadDenied,
                        req.tenant.clone(),
                        req.hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                emit(true);
                return Err(CasHandlerError::CrossTenantDenied {
                    caller: req.caller_tenant.clone(),
                    requested_tenant: req.tenant.clone(),
                });
            }
        }

        if reqs.is_empty() {
            // Nothing to audit, nothing to probe — the serial loop would
            // likewise write no row and issue no request.
            return Ok(Vec::new());
        }

        // N digests still produce N `ReadAttempted` rows — same kind, same
        // per-digest fields. The taxonomy is untouched; only the number of
        // network trips that carry the rows changes. Duplicate digests in one
        // request produce colliding `id`/`request_id` keys and are deduped by
        // `INSERT OR IGNORE`, exactly as duplicate serial emits are today.
        let events: Vec<AuditEvent> = reqs
            .iter()
            .map(|req| {
                AuditEvent::new(
                    AuditEventKind::ReadAttempted,
                    req.tenant.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                )
            })
            .collect();

        let handle = tokio::runtime::Handle::current();
        let audit_fut = audit_async.emit_cas_batch_async(&events);
        let probes_fut = self.probe_existence_concurrently(reqs);

        // ONE `Phase::Store` scope over the ENTIRE joined window — not one
        // per future, and NOT a second `Phase::Audit` scope. `Σ(phases) ≤
        // total` holds by construction rather than by `oother`'s `max(0)`
        // guard swallowing a double count. See `origin_timing.rs`'s
        // "Concurrent native-plane list seam" note, which covers this path.
        let (audit_result, probe_results) = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| {
                handle.block_on(async { tokio::join!(audit_fut, probes_fut) })
            })
        };

        // AUDIT FIRST. A failed audit returns `AuditFailed` and NO probe
        // result reaches the caller — identical to the serial path, which
        // returns from the first digest's audit `map_err(..)?` (and, like it,
        // emits no SLI observation on that path).
        if let Err(e) = audit_result {
            return Err(CasHandlerError::AuditFailed(e));
        }

        // Deterministic, request-ordered evaluation: FIRST error wins, and
        // the observations recorded before it are exactly the ones the serial
        // loop would have recorded before returning on the same digest.
        let mut flags = Vec::with_capacity(probe_results.len());
        for result in probe_results {
            match result {
                Ok(present) => {
                    emit(false);
                    flags.push(present);
                }
                Err(e) => {
                    emit(true);
                    return Err(e);
                }
            }
        }
        Ok(flags)
    }

    /// Run the per-digest HEAD probes with bounded concurrency, returning one
    /// outcome per request **in request order** (`buffered` preserves the
    /// input order regardless of completion order — the response must not
    /// depend on which R2 call finished first).
    async fn probe_existence_concurrently(
        &self,
        reqs: &[CasReadRequest],
    ) -> Vec<Result<bool, CasHandlerError>> {
        use futures::StreamExt as _;

        futures::stream::iter(reqs.iter().map(|req| async move {
            // `Some(len)` present / `None` absent — the storage-layer
            // `NotFound` absorbed into `Ok(false)` exactly as `exists` does.
            self.probe_existence_unaudited(req)
                .await
                .map(|found| found.is_some())
        }))
        .buffered(MAX_CONCURRENT_EXISTS_PROBES)
        .collect::<Vec<_>>()
        .await
    }

    /// The storage half of ONE existence probe: BYOK resolution, key
    /// derivation, one S3 `HeadObject`. No body download, no rehash — the
    /// same work [`CasReadHandler::exists`] does after its audit row.
    ///
    /// # This deliberately emits NO audit row
    ///
    /// It is `fn`-private to this module and has exactly TWO callers, both
    /// concurrent seams inside this same impl: [`CasReadHandler::exists`]'s
    /// join and [`Self::exists_batch_inner`]'s. Each has already DISPATCHED
    /// the corresponding `ReadAttempted` write and refuses to return ANY
    /// result of this probe unless that write committed. It must NOT be
    /// widened into a generally reachable way to probe the CAS without an
    /// audit row — that would be a hole straight through the fail-CLOSED
    /// contract. There is deliberately ONE body: two copies of an
    /// un-audited probe is exactly the kind of duplication where one copy
    /// later grows a caller that skips the audit.
    ///
    /// It also opens no [`crate::origin_timing::PhaseScope`] of its own: on
    /// the batch path N concurrent probes each entering `Phase::Store` would
    /// count the same wall-clock window N times over. Each caller's single
    /// scope covers its whole joined window instead.
    ///
    /// Returns the RAW `head_size` outcome (`Some(len)` present / `None`
    /// absent) rather than a bool, because `exists` needs the same shape the
    /// serial fallback produces; the batch path maps it to a bool itself.
    async fn probe_existence_unaudited(
        &self,
        req: &CasReadRequest,
    ) -> Result<Option<u64>, CasHandlerError> {
        // BYOK Wave 3c: probe the §4-hardened physical key for an active
        // tenant, so this hits the SAME key `read`/`write`/`exists` use.
        let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
        // Fail CLOSED if the tenant prefix is not derivable: never touch R2
        // under a degraded/empty (SHARED) prefix.
        let key = self
            .r2_key(&req.tenant, &resolved.physical_digest, req.algo)
            .map_err(CasHandlerError::Internal)?;
        debug!(key = %key, "R2CasHandler::exists");

        self.client.head_size(&key).await.map_err(|e| {
            warn!(error = %e, key = %key, "R2CasHandler::exists error");
            CasHandlerError::Internal(e)
        })
    }
}

impl CasWriteHandler for R2CasHandler {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

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
        if !req.is_authorized_for_caller() {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::WriteDenied,
                    req.tenant.clone(),
                    req.claimed_hash.clone(),
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

        // WriteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::WriteAttempted,
                req.tenant.clone(),
                req.claimed_hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(CasHandlerError::AuditFailed)?;

        // Content-addressing enforcement (INV-CAS-INTEGRITY):
        // the durable store MUST NOT persist bytes that do not hash to
        // the claimed digest — otherwise the CAS guarantee is a lie and
        // any client (or a buggy uploader) can poison the cache for
        // every subsequent reader of that digest. Verify BEFORE the
        // PUT; on mismatch emit `CorrectnessViolation` + a
        // `CorrectnessCas` SLI failure and reject with 422
        // `HashMismatch`. Nothing is written.
        if let Err(actual) = verify_content_hash(req.algo, &req.claimed_hash, &req.bytes) {
            self.audit
                .emit(AuditEvent::new(
                    AuditEventKind::CorrectnessViolation,
                    req.tenant.clone(),
                    req.claimed_hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                ))
                .map_err(CasHandlerError::AuditFailed)?;
            self.sli
                .observe(SliObservation::new(Sli::CorrectnessCas, true, 0));
            emit(true);
            return Err(CasHandlerError::HashMismatch {
                claimed: req.claimed_hash,
                actual,
            });
        }

        // B071: claim the D1 writer lease before resolving keys or touching
        // R2. GC acquisition excludes this lease, and the metadata commit
        // consumes it atomically. `_public` is a shared physical namespace,
        // not a tenant-owned `blob_meta` row, so assigning it to one caller
        // would leak GC ownership across tenants.
        let fence_lease = if req.tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            None
        } else if let Some(fence) = self.cas_write_fence.as_ref() {
            match fence.begin(
                &req.tenant,
                &canonical_meta_digest(&req.claimed_hash),
                req.at_unix_ms,
            ) {
                Ok(lease) => Some(lease),
                Err(e) => {
                    emit(true);
                    return Err(CasHandlerError::Internal(format!(
                        "CAS write D1 fence: {e}"
                    )));
                }
            }
        } else {
            None
        };
        let abort_fence = |lease: &Option<crate::storage::cas_write_fence::CasWriteLease>| {
            if let (Some(fence), Some(lease)) = (self.cas_write_fence.as_ref(), lease) {
                if let Err(e) = fence.abort(lease) {
                    warn!(error = %e, "CAS write D1 fence abort failed; lease expiry will recover");
                }
            }
        };

        // CRITICAL — `block_in_place` rationale: see the matching
        // comment in `<Self as CasReadHandler>::read` above.
        let handle = tokio::runtime::Handle::current();

        // BYOK Wave 3c: resolve ONCE — the §4-hardened physical key digest (audit
        // H-4) + the body crypto plan. Non-BYOK tenants resolve to the RAW digest
        // (byte-identical to today); an active-but-unresolvable tenant fails
        // CLOSED here (never PUTs plaintext).
        let resolved = match tokio::task::block_in_place(|| {
            handle.block_on(self.resolve_byok(&req.tenant, &req.claimed_hash, req.algo))
        }) {
            Ok(r) => r,
            Err(e) => {
                abort_fence(&fence_lease);
                emit(true);
                return Err(e);
            }
        };
        // Fail CLOSED if the tenant prefix is not derivable: never touch
        // R2 under a degraded/empty (SHARED) prefix.
        let key = match self.r2_key(&req.tenant, &resolved.physical_digest, req.algo) {
            Ok(k) => k,
            Err(e) => {
                abort_fence(&fence_lease);
                emit(true);
                return Err(CasHandlerError::Internal(e));
            }
        };
        debug!(key = %key, bytes = req.bytes.len(), "R2CasHandler::write");
        let request_bytes_len = req.bytes.len() as u64;

        // Idempotent-rewrite detection (rt-nuclear #13 — byte double-charge).
        // CAS is content-addressed: the key already embeds the verified content
        // hash, so an object that already exists under this key holds the SAME
        // bytes (the hash was verified above). HEAD before PUT (mirrors the AC
        // path's GET-and-compare, but for CAS a presence HEAD suffices): if the
        // blob is already present we SKIP the re-PUT and return `durable=false`,
        // so the `AccountingCasHandler` decorator does NOT charge the bytes a
        // second time on an idempotent re-write. A HEAD error fails CLOSED to the
        // PUT path (correctness over the accounting optimisation — a transient
        // HEAD blip must never drop a write); a duplicate PUT is harmless (same
        // bytes) and the worst case is the legacy double-charge, never data loss.
        //
        // BYOK Wave 3c — the dedup HEAD-skip is gated to the convergent/plaintext
        // path ONLY. Mode B (random DEK) is deliberately NOT deduped (audit C2):
        // the §4-hardened key + random DEK + the `byok_envelope` idempotency
        // (reuse-the-row, deterministic ciphertext) own the no-orphan guarantee,
        // so a Mode-B write always proceeds to the PUT below.
        let dedup_eligible = !matches!(resolved.plan, ByokBodyPlan::Random { .. });
        if dedup_eligible {
            let head_result = {
                let _scope =
                    crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
                tokio::task::block_in_place(|| handle.block_on(self.client.head_size(&key)))
            };
            match head_result {
                Ok(Some(_)) => {
                    // Already durable under this content-addressed key → idempotent
                    // no-op; do not re-PUT and do not re-charge the bytes.
                    if let (Some(fence), Some(lease)) =
                        (self.cas_write_fence.as_ref(), fence_lease.as_ref())
                    {
                        if let Err(e) = fence.commit(lease, request_bytes_len, req.at_unix_ms) {
                            abort_fence(&fence_lease);
                            emit(true);
                            return Err(CasHandlerError::Internal(format!(
                                "CAS write metadata commit: {e}"
                            )));
                        }
                        abort_fence(&fence_lease);
                    }
                    self.audit
                        .emit(AuditEvent::new(
                            AuditEventKind::WriteCommitted,
                            req.tenant.clone(),
                            req.claimed_hash.clone(),
                            req.principal.clone(),
                            req.at_unix_ms,
                        ))
                        .map_err(CasHandlerError::AuditFailed)?;
                    self.sli
                        .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                    emit(false);
                    return Ok(CasWriteResponse::new(req.claimed_hash, false));
                }
                Ok(None) => { /* absent — fall through to the PUT below */ }
                Err(e) => {
                    // Ambiguous prior state: fall through to the PUT (fail-CLOSED to
                    // a durable write) rather than risk dropping a fresh blob.
                    warn!(error = %e, key = %key, "R2CasHandler::write pre-PUT HEAD error; PUTting");
                }
            }
        }

        // BYOK Wave 3a/3c (GATED-INERT): for an `active` tenant, encrypt at rest
        // AFTER the plaintext integrity verify + the (gated) dedup HEAD check,
        // replacing the stored bytes with the ciphertext blob (convergent CLB1 or
        // Mode-B CLB2). FAIL-CLOSED: an active tenant whose encryptor/KMS/Tcs/
        // envelope-store is unavailable returns Err here — plaintext is NEVER PUT
        // for an active tenant. `None` ⇒ the plaintext path (req.bytes),
        // byte-identical to today for every non-BYOK tenant.
        let payload = match tokio::task::block_in_place(|| {
            handle.block_on(self.encrypt_body(&resolved.plan, &req.bytes))
        }) {
            Ok(Some(ciphertext)) => ciphertext,
            Ok(None) => req.bytes,
            Err(e) => {
                abort_fence(&fence_lease);
                emit(true);
                return Err(e);
            }
        };

        let result = {
            let _scope =
                crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| {
                if dedup_eligible {
                    handle
                        .block_on(self.client.put_if_absent(&key, payload))
                        .map(|fresh| (fresh, ()))
                } else {
                    handle
                        .block_on(self.client.put(&key, payload))
                        .map(|()| (true, ()))
                }
            })
        };

        match result {
            Ok((r2_fresh, ())) => {
                let metadata_result = if let (Some(fence), Some(lease)) =
                    (self.cas_write_fence.as_ref(), fence_lease.as_ref())
                {
                    Some(fence.commit(lease, request_bytes_len, req.at_unix_ms))
                } else {
                    None
                };
                if let Some(Err(e)) = metadata_result {
                    // Mode B uses a blind PUT because every write has a fresh
                    // random envelope.  If D1 metadata loses the race, the
                    // object and envelope are intentionally preserved and a
                    // durable reconciliation intent is recorded; deleting
                    // either side here would make the outcome unrecoverable.
                    if let ByokBodyPlan::Random { ctx } = &resolved.plan {
                        if let Some(mode_b) = self.byok_mode_b.as_ref() {
                            if let Err(reconcile_error) = tokio::task::block_in_place(|| {
                                handle.block_on(mode_b.record_reconciliation_intent(
                                    ctx,
                                    &key,
                                    "mode-b metadata commit failed",
                                    req.at_unix_ms,
                                ))
                            }) {
                                warn!(
                                    error = %reconcile_error,
                                    "Mode-B metadata failure could not persist reconciliation intent"
                                );
                                abort_fence(&fence_lease);
                                emit(true);
                                return Err(CasHandlerError::Internal(format!(
                                    "CAS write metadata commit: {e}; reconciliation intent: {reconcile_error}"
                                )));
                            }
                        }
                    }
                    // Only a conditional PUT that returned `true` is ours to
                    // compensate. Blind Mode-B PUTs may have replaced an
                    // existing ciphertext and are therefore never deleted.
                    if dedup_eligible && r2_fresh {
                        let rollback = tokio::task::block_in_place(|| {
                            handle.block_on(self.client.delete(&key))
                        });
                        if let Err(rollback_error) = rollback {
                            warn!(error = %rollback_error, key = %key, "CAS write R2 compensation failed after D1 metadata rejection");
                        }
                    }
                    abort_fence(&fence_lease);
                    emit(true);
                    return Err(CasHandlerError::Internal(format!(
                        "CAS write metadata commit: {e}"
                    )));
                }
                abort_fence(&fence_lease);
                self.audit
                    .emit(AuditEvent::new(
                        AuditEventKind::WriteCommitted,
                        req.tenant.clone(),
                        req.claimed_hash.clone(),
                        req.principal.clone(),
                        req.at_unix_ms,
                    ))
                    .map_err(CasHandlerError::AuditFailed)?;
                self.sli
                    .observe(SliObservation::new(Sli::CorrectnessCas, false, 0));
                emit(false);
                Ok(CasWriteResponse::new(req.claimed_hash, r2_fresh))
            }
            Err(e) => {
                warn!(error = %e, key = %key, "R2CasHandler::write error");
                abort_fence(&fence_lease);
                emit(true);
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}
