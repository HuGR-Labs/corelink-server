/// Metadata and GC tables deliberately store the plain canonical hex digest.
/// The algorithm is already selected by the external route/parser and must not
/// be re-encoded into a second, ambiguous identity at the D1 boundary.
fn canonical_meta_digest(hash: &str) -> String {
    hash.to_owned()
}

impl CasReadHandler for R2CasHandler {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        use corelink_handler_cas::observer::Sli;

        #[cfg(test)]
        if self.test_post_header_body_timeout {
            return match R2S3Client::test_post_header_body_timeout() {
                Err(error) => Err(CasHandlerError::Internal(error)),
                Ok(outcome) => Err(CasHandlerError::Internal(format!(
                    "test body collector unexpectedly returned {outcome:?}"
                ))),
            };
        }

        // Batch-read supplies its 8 MiB object ceiling through the request so
        // the storage adapter can reject oversized objects from metadata before
        // collecting a body. Ordinary single reads retain the historical
        // 64 MiB ceiling.
        let max_bytes = req
            .max_bytes
            .unwrap_or(crate::routes::cas::CAS_READ_MAX_OBJECT_BYTES);

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

        // Durable `ReadAttempted` MUST commit before resolving a key or
        // dispatching any R2 request. This is intentionally serial even when
        // the production sink is the concrete D1 sink: an audit failure must
        // result in zero storage calls, not merely zero returned bytes.
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::ReadAttempted,
                req.tenant.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(|e| {
                emit(true);
                CasHandlerError::AuditFailed(e)
            })?;

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
        let stored_opt = {
            let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Store);
            tokio::task::block_in_place(|| handle.block_on(self.get_capped_for_read(&key, max_bytes)))
        };
        let stored_opt = match stored_opt {
            Ok(ok) => ok,
            Err(e) => {
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

        // ReadAttempted audit MUST commit before resolving a key or
        // dispatching the R2 HEAD. A HEAD reveals only presence (not bytes),
        // but it is still a storage read covered by the durable audit.
        let handle = tokio::runtime::Handle::current();
        self.audit
            .emit(AuditEvent::new(
                AuditEventKind::ReadAttempted,
                req.tenant.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            ))
            .map_err(|e| {
                emit(true);
                CasHandlerError::AuditFailed(e)
            })?;

        let result = tokio::task::block_in_place(|| {
            handle.block_on(self.probe_existence_unaudited(&req))
        });
        match result {
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
    /// Returns `None` when the async batch-audit seam is not wired
    /// (`audit_async == None`: test handlers and non-D1 `AuditSink`s), so
    /// those callers keep looping [`Self::exists`]. When wired, the batched
    /// durable audit completes before any bounded-concurrent probe starts.
    ///
    /// # Fail-CLOSED
    ///
    /// The audit batch commits before the probes are dispatched. An audit
    /// error returns `AuditFailed` with zero storage calls. Cross-tenant
    /// denial happens strictly before anything is dispatched, and still emits
    /// its own `ReadDenied` row.
    fn exists_batch(&self, reqs: &[CasReadRequest]) -> Option<Result<Vec<bool>, CasHandlerError>> {
        // No async-capable durable sink ⇒ no batch capability ⇒ the caller
        // uses the unchanged per-digest `exists()` loop.
        let audit_async = Arc::clone(self.audit_async.as_ref()?);
        Some(self.exists_batch_inner(&audit_async, reqs))
    }
}
