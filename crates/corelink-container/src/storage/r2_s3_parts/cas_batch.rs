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
        // This explicit batch-exists exception owns the joined audit/probe
        // timing window; single-object reads and lists remain serial.
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
