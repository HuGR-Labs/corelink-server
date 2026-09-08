/// A sync `CasReadHandler` + `CasWriteHandler` backed by [`R2S3Client`].
///
/// Wraps the async S3 operations with
/// `tokio::runtime::Handle::current().block_on(...)` so the sync
/// handler traits can drive async I/O from within a tokio runtime.
#[non_exhaustive]
pub struct R2CasHandler {
    client: R2S3Client,
    /// The R2 region string (e.g. `"iad"`) used as key prefix.
    cas_region: String,
    /// Tenant derivation key for `derive_prefix`. Wrapped in
    /// `Option<...>` because tests construct without a TDK.
    tdk: Option<TenantDerivationKey>,
    audit: Arc<dyn AuditSink>,
    /// Narrow async-capable seam onto the SAME sink as `audit` (perf,
    /// concurrent-list PR): `Some` only when `audit` is backed by the
    /// durable `D1AuditOutboxSink` (the production builder always wires
    /// this — see [`build_r2_cas_handler_from_env`] /
    /// [`Self::with_async_audit`]). `list()` uses this to `tokio::join!`
    /// the mandatory `ListAttempted` audit write with the R2 enumeration
    /// instead of running them serially; `None` (tests, any other
    /// `AuditSink` impl) keeps the fully serial fallback, byte-identical to
    /// before this seam existed. This does NOT weaken fail-closed: the
    /// response is still gated on the audit result, checked FIRST, exactly
    /// as the serial path checks it — see `list()`'s doc.
    audit_async: Option<Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>>,
    /// D1 lease that fences the live CAS write path against GC purge epochs.
    /// `None` is retained only for in-memory/unit-test constructors; the
    /// production builder refuses to mount R2 without this dependency.
    cas_write_fence: Option<Arc<dyn crate::storage::cas_write_fence::CasWriteFence>>,
    sli: Arc<dyn SliObserver>,
    /// BYOK Wave 3a (GATED-INERT): per-tenant BYOK config cache. `None` on the
    /// non-BYOK build / tests → the plaintext path runs unchanged. When `Some`
    /// AND a tenant is `active`, the CAS write/read path encrypts at rest.
    byok_config_cache: Option<Arc<ByokConfigCache>>,
    /// BYOK Wave 3a: the Tcs resolver (CMK-unwrap → convergence secret). `None`
    /// → plaintext path. Both this and `byok_config_cache` must be `Some` for
    /// encryption to engage (frozen policy §3).
    tcs_resolver: Option<Arc<TcsResolver>>,
    /// BYOK Wave 3c: the Mode-B (random-DEK) encryptor + `byok_envelope` store.
    /// `None` → a tenant configured for `crypto_mode='random'` fails CLOSED on
    /// the data plane (never plaintext); Mode A (convergent) is unaffected.
    byok_mode_b: Option<Arc<ModeBEncryptor>>,
    #[cfg(test)]
    test_post_header_body_timeout: bool,
}

impl core::fmt::Debug for R2CasHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("R2CasHandler")
            .field("cas_region", &self.cas_region)
            .finish_non_exhaustive()
    }
}

impl R2CasHandler {
    /// Construct an `R2CasHandler`.
    ///
    /// `tdk_bytes` is a 32-byte secret key loaded from env/KMS. Pass
    /// `None` only in tests that use a dev/zero TDK.
    #[must_use]
    pub fn new(
        client: R2S3Client,
        cas_region: impl Into<String>,
        tdk_bytes: Option<Zeroizing<[u8; 32]>>,
        audit: Arc<dyn AuditSink>,
        sli: Arc<dyn SliObserver>,
    ) -> Self {
        let tdk = tdk_bytes.map(TenantDerivationKey::from_bytes);
        Self {
            client,
            cas_region: cas_region.into(),
            tdk,
            audit,
            audit_async: None,
            cas_write_fence: None,
            sli,
            byok_config_cache: None,
            tcs_resolver: None,
            byok_mode_b: None,
            #[cfg(test)]
            test_post_header_body_timeout: false,
        }
    }

    /// Test-only seam that sends the real R2 body collector timeout through
    /// this production `CasReadHandler` implementation.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_test_post_header_body_timeout(mut self) -> Self {
        self.test_post_header_body_timeout = true;
        self
    }

    /// Attach the concurrent-list async audit seam: `audit_async` MUST be
    /// the SAME sink as the `audit` passed to [`Self::new`] (the production
    /// builder enforces this by cloning one `Arc<D1AuditOutboxSink>` into
    /// both places — see [`build_r2_cas_handler_from_env`]). Passing a
    /// DIFFERENT sink here would let `list()` write its audit row to one
    /// sink while every other call writes to another — never do that.
    ///
    /// Optional: a handler with `audit_async` left `None` keeps `list()`
    /// fully serial (identical to the pre-existing behavior) — this is the
    /// state every test handler in this module is in today.
    #[must_use]
    pub fn with_async_audit(
        mut self,
        audit_async: Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>,
    ) -> Self {
        self.audit_async = Some(audit_async);
        self
    }

    /// Attach the D1-backed CAS write fence.
    ///
    /// The production builder wires this to the same D1 database used by GC.
    /// A handler with storage credentials but without this dependency is not
    /// constructed; test-only handlers may omit it to keep their hermetic
    /// in-memory behavior.
    #[must_use]
    pub fn with_cas_write_fence(
        mut self,
        fence: Arc<dyn crate::storage::cas_write_fence::CasWriteFence>,
    ) -> Self {
        self.cas_write_fence = Some(fence);
        self
    }

    /// Attach the BYOK Wave-3a collaborators (config cache + Tcs resolver),
    /// enabling convergent encryption-at-rest for `active` tenants.
    ///
    /// GATED-INERT: encryption engages ONLY for a tenant whose
    /// `tenant_byok_config.state == 'active'`; every other tenant (and the
    /// `_public` namespace) keeps the exact plaintext path. The production
    /// builder ([`build_r2_cas_handler_from_env`]) does NOT call this yet —
    /// onboarding (the sole writer of the `active` state) is a later wave, and
    /// the default build links no real `KmsProvider` — so production stays on
    /// the unchanged plaintext path until that wave wires a provider here.
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

    /// Attach the BYOK Wave-3c Mode-B (random-DEK) encryptor. GATED-INERT: only
    /// engaged for an `active` tenant whose `crypto_mode='random'`. Without it,
    /// such a tenant fails CLOSED on the data plane (never plaintext); Mode A is
    /// unaffected. Chains after [`Self::with_byok`].
    #[must_use]
    pub fn with_byok_random(mut self, mode_b: Arc<ModeBEncryptor>) -> Self {
        self.byok_mode_b = Some(mode_b);
        self
    }

    /// Resolve the BYOK plan for a (tenant, digest, algo): the **physical R2 key
    /// digest** (§4-hardened for an active tenant — audit H-4) plus the body
    /// crypto plan. Single source of truth for the read/write/exists/delete
    /// paths.
    ///
    /// - `Plaintext` — BYOK not wired / not configured / inactive / `_public`:
    ///   the physical digest is the RAW digest (byte-identical to today).
    /// - `Convergent` / `Random` — active: the physical digest is
    ///   `harden_digest(tcs, digest)`; the body plan carries the real-digest
    ///   [`CryptoContext`].
    /// - `Err(..)` — active-but-unresolvable (KMS/Tcs down, Mode-B unwired,
    ///   partial): FAIL CLOSED (5xx); NEVER plaintext.
    async fn resolve_byok(
        &self,
        tenant: &str,
        digest: &str,
        algo: DigestAlgo,
    ) -> Result<ByokResolved, CasHandlerError> {
        let plaintext = || ByokResolved {
            physical_digest: digest.to_owned(),
            plan: ByokBodyPlan::Plaintext,
        };
        // Both Wave-3a collaborators must be present (frozen policy §3); else the
        // existing plaintext path runs unchanged — no D1 hop, no behaviour change.
        let (Some(cache), Some(resolver)) =
            (self.byok_config_cache.as_ref(), self.tcs_resolver.as_ref())
        else {
            return Ok(plaintext());
        };
        // `_public` is deterministic public content with no secret — it MUST stay
        // plaintext (raw key) so cross-tenant dedup is preserved (plan §3).
        if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            return Ok(plaintext());
        }
        // ONE D1 read on a cache miss; cached (incl. the not-configured answer).
        // A config error fails closed — never a silent plaintext downgrade.
        let Some(cfg) = cache
            .get(tenant)
            .await
            .map_err(|e| CasHandlerError::Internal(format!("byok config read: {e}")))?
        else {
            return Ok(plaintext());
        };
        match engagement_for(&cfg) {
            ByokEngagement::Plaintext => Ok(plaintext()),
            ByokEngagement::FailClosed(why) => Err(CasHandlerError::Internal(format!(
                "byok active but {why}; refusing to fall back to plaintext (fail-closed)"
            ))),
            ByokEngagement::Encrypt(mode) => {
                let key_id = cfg.cmk_key_id.clone().unwrap_or_default();
                // The Tcs is resolved for BOTH modes — Mode A uses it for the
                // convergent DEK, and BOTH modes use it to §4-harden the physical
                // R2 key (audit H-4: the on-disk key reveals nothing without it).
                let tcs = resolver
                    .resolve(&cfg)
                    .await
                    .map_err(|e| CasHandlerError::Internal(format!("byok tcs resolve: {e}")))?;
                let physical_digest = harden_digest(&tcs, digest);
                let plan = match mode {
                    ByokCryptoMode::Convergent => ByokBodyPlan::Convergent {
                        tcs,
                        ctx: cas_crypto_context(tenant, digest, algo, &key_id),
                    },
                    ByokCryptoMode::Random => {
                        if self.byok_mode_b.is_none() {
                            return Err(CasHandlerError::Internal(
                                "byok active Mode B (random) but the random-mode encryptor is \
                                 not wired; refusing to fall back to plaintext (fail-closed)"
                                    .to_owned(),
                            ));
                        }
                        ByokBodyPlan::Random {
                            ctx: cas_crypto_context_for(
                                tenant,
                                digest,
                                algo,
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

    /// Encrypt the body for a resolved plan. `Ok(None)` ⇒ store the plaintext
    /// unchanged; `Ok(Some(ct))` ⇒ store the ciphertext blob; `Err` ⇒ fail closed
    /// (the caller returns before any PUT — plaintext is NEVER stored).
    async fn encrypt_body(
        &self,
        plan: &ByokBodyPlan,
        plaintext: &[u8],
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        match plan {
            ByokBodyPlan::Plaintext => Ok(None),
            ByokBodyPlan::Convergent { tcs, ctx } => {
                let stored = encrypt_cas_blob(plaintext, tcs, ctx)
                    .map_err(|e| CasHandlerError::Internal(format!("byok encrypt: {e}")))?;
                Ok(Some(stored))
            }
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    CasHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                let stored = mode_b
                    .encrypt(plaintext, ctx)
                    .await
                    .map_err(|e| CasHandlerError::Internal(format!("byok mode-b encrypt: {e}")))?;
                Ok(Some(stored))
            }
        }
    }

    /// Decrypt the stored body for a resolved plan. `Plaintext` ⇒ return the
    /// stored bytes unchanged; otherwise decrypt (fail closed on any failure —
    /// raw stored bytes are NEVER served). The post-decrypt content-hash
    /// re-verify (audit C1) runs on the returned PLAINTEXT, in the caller.
    async fn decrypt_body(
        &self,
        plan: &ByokBodyPlan,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, CasHandlerError> {
        match plan {
            ByokBodyPlan::Plaintext => Ok(stored),
            ByokBodyPlan::Convergent { tcs, ctx } => decrypt_cas_blob(&stored, tcs, ctx)
                .map_err(|e| CasHandlerError::Internal(format!("byok decrypt: {e}"))),
            ByokBodyPlan::Random { ctx } => {
                let mode_b = self.byok_mode_b.as_ref().ok_or_else(|| {
                    CasHandlerError::Internal("byok mode-b encryptor missing".to_owned())
                })?;
                mode_b
                    .decrypt(&stored, ctx)
                    .await
                    .map_err(|e| CasHandlerError::Internal(format!("byok mode-b decrypt: {e}")))
            }
        }
    }

    /// BYOK Wave 4a — reclaim the Mode-B `byok_envelope` row for a resolved plan
    /// AFTER the R2 object has been deleted. ONLY Mode B (`Random`) writes an
    /// envelope row, so `Plaintext` / `Convergent` (Mode A) are no-ops.
    ///
    /// Ordering + fail-safety (frozen policy §3): the caller deletes the R2
    /// object FIRST, then calls this. A failed reclaim leaves an orphan
    /// wrapped-DEK row that now wraps NOTHING (the blob is already gone) — the
    /// SAFE-fail direction — so the caller WARNS and continues rather than fail
    /// the whole delete (which could leave a readable blob whose key was
    /// destroyed). `Ok(())` ⇒ nothing to reclaim or reclaim succeeded.
    async fn reclaim_byok_envelope(&self, plan: &ByokBodyPlan) -> Result<(), String> {
        if let ByokBodyPlan::Random { ctx } = plan {
            if let Some(mode_b) = self.byok_mode_b.as_ref() {
                return mode_b.reclaim(ctx).await;
            }
        }
        Ok(())
    }

    /// BYOK write hook (test-facing): resolve + encrypt the body. The production
    /// `write` path resolves ONCE and calls [`Self::encrypt_body`] directly.
    #[cfg(test)]
    async fn byok_encrypt_for_write(
        &self,
        req: &CasWriteRequest,
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        let resolved = self
            .resolve_byok(&req.tenant, &req.claimed_hash, req.algo)
            .await?;
        self.encrypt_body(&resolved.plan, &req.bytes).await
    }

    /// BYOK read hook (test-facing): resolve + decrypt the stored body. The
    /// production `read` path resolves ONCE and calls [`Self::decrypt_body`].
    #[cfg(test)]
    async fn byok_decrypt_for_read(
        &self,
        req: &CasReadRequest,
        stored: Vec<u8>,
    ) -> Result<Vec<u8>, CasHandlerError> {
        let resolved = self.resolve_byok(&req.tenant, &req.hash, req.algo).await?;
        self.decrypt_body(&resolved.plan, stored).await
    }

    /// BYOK delete-reclaim hook (test-facing): resolve + reclaim the Mode-B
    /// `byok_envelope` row, mirroring the post-R2-delete step in the production
    /// `delete` path (which WARNS on the returned `Err` and never fails the
    /// delete). Returns the reclaim `Result` so a test can assert the safe-fail
    /// direction.
    #[cfg(test)]
    async fn byok_reclaim_for_delete(
        &self,
        tenant: &str,
        digest: &str,
        algo: DigestAlgo,
    ) -> Result<(), String> {
        let resolved = self
            .resolve_byok(tenant, digest, algo)
            .await
            .map_err(|e| format!("resolve: {e}"))?;
        self.reclaim_byok_envelope(&resolved.plan).await
    }

    /// Derive the R2 key for a (tenant, digest) pair.
    ///
    /// The tenant prefix is ALWAYS `derive_prefix(tdk, tenant_uuid)` —
    /// an unpredictable, secret-keyed HMAC namespace (layer 5 of
    /// `INV-TENANT-ISOLATION`). The handler cannot be constructed
    /// without a TDK on the production path (see
    /// [`build_r2_cas_handler_from_env`], which fails closed when
    /// `R2_TDK_HEX` is unset) — so the raw-padded public-prefix
    /// fallback used by simple non-UUID test fixtures is gated behind
    /// `#[cfg(test)]` and is unreachable in production (F1/F2).
    fn r2_key(&self, tenant: &str, digest: &str, algo: DigestAlgo) -> Result<String, String> {
        let prefix = tenant_prefix(self.tdk.as_ref(), tenant)?;
        Ok(R2S3Client::blob_key(
            &self.cas_region,
            &prefix,
            digest,
            algo,
        ))
    }

    /// Emit both SLI observations (availability + latency).
    /// Emit the availability + latency pair for one handler entry.
    ///
    /// `latency_us` is the wall-clock the handler entry took, which is
    /// what [`SliObservation`]'s field has always been documented as
    /// carrying. Every CAS/AC call site used to pass a literal `0`
    /// here, which made `LatencyCasGetP99` / `LatencyCasPutP99` /
    /// `LatencyAcHitP99` samples of nothing — a latency SLI whose every
    /// sample is zero is not a loose measurement (B-057). The callers
    /// now start an `Instant` at handler entry and pass the elapsed
    /// microseconds, so the latency SLIs observe the same window the
    /// availability SLIs count.
    fn emit_sli(
        &self,
        avail: corelink_handler_cas::observer::Sli,
        lat: corelink_handler_cas::observer::Sli,
        is_error: bool,
        latency_us: u64,
    ) {
        self.sli
            .observe(SliObservation::new(avail, is_error, latency_us));
        self.sli
            .observe(SliObservation::new(lat, is_error, latency_us));
    }

    /// Read one CAS object through the B-051 pre-materialisation ceiling.
    /// Both the production concurrent-audit path and the serial fallback MUST
    /// use this helper; keeping the capped storage read in one place prevents
    /// a path-specific `get()` from bypassing the B-077 process-wide envelope.
    /// `max_bytes` is supplied by the request so batch-read can use its tighter
    /// 8 MiB object ceiling while single reads retain the ordinary ceiling.
    async fn get_capped_for_read(
        &self,
        key: &str,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>, CasHandlerError> {
        match self.client.get_capped(key, max_bytes).await {
            Ok(CappedGet::Found(bytes)) => Ok(Some(bytes)),
            Ok(CappedGet::Missing) => Ok(None),
            Ok(CappedGet::TooLarge { actual_bytes }) => {
                warn!(
                    key,
                    actual_bytes = ?actual_bytes,
                    limit_bytes = max_bytes,
                    "R2CasHandler::read refused an over-size object"
                );
                Err(CasHandlerError::ObjectTooLarge {
                    actual_bytes: actual_bytes.unwrap_or(0),
                    limit_bytes: max_bytes,
                })
            }
            Err(e) => {
                warn!(error = %e, key, "R2CasHandler::read error");
                Err(CasHandlerError::Internal(e))
            }
        }
    }
}
