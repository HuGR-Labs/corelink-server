/// Return the current wall-clock time in milliseconds since the Unix epoch.
/// Saturates to `u64::MAX` on overflow (impossible in practice before year
/// ~5 × 10⁸ CE, but matches the `wallclock_unix_ms` sentinel pattern).
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// True iff `upload_uuid` was minted for `tenant`. Sessions are minted as
/// `<tenant-canonical>:<random>` in [`OciMoatStore::open_upload`]; the upload
/// buffer is process-shared across tenants (all OCI traffic hits one `_oci`
/// DO), so chunk/cancel/finalize MUST reject a uuid that isn't the caller's —
/// otherwise a tenant holding another's uuid could append-to / cancel its
/// in-flight push (a confused-deputy griefing vector). A cross-tenant uuid
/// returns the SAME "not found" error as an absent one (no existence oracle).
fn upload_uuid_belongs_to(tenant: &TenantId, upload_uuid: &str) -> bool {
    let prefix = format!("{}:", tenant.to_canonical_text());
    upload_uuid.starts_with(&prefix)
}

#[async_trait]
impl BlobStore for OciMoatStore {
    async fn open_upload(&self, tenant: &TenantId) -> PortResult<String> {
        // F25 — per-tenant open-session cap.
        //
        // Count how many sessions in the map belong to this tenant by checking
        // the `<tenant-text>:` prefix (sessions are minted as
        // `<tenant-text>:<uuid>`). Reject with an error string that the adapter
        // maps to 429 when the count is at the cap. The check+insert is atomic
        // because we hold the mutex for the entire operation.
        //
        // Lazy reaper (audit #6 / WP-OCI-DOS): BEFORE counting, evict all
        // sessions whose last-active timestamp is older than
        // `OCI_SESSION_IDLE_TIMEOUT_MS`. This prevents a crashed client from
        // permanently locking its tenant out of the per-tenant cap. Reaped
        // bytes are subtracted from the global `inflight_bytes` counter.
        let tenant_text = tenant.to_canonical_text();
        let session_prefix = format!("{tenant_text}:");
        let uuid = format!("{tenant_text}:{}", Uuid::new_v4().simple());
        let now = now_unix_ms();
        let mut g = self
            .uploads
            .lock()
            .map_err(|e| format!("oci upload buf poisoned: {e}"))?;

        // Lazy reap: collect abandoned sessions (idle > timeout).
        let stale_keys: Vec<String> = g
            .iter()
            .filter(|(_, s)| now.saturating_sub(s.last_active_ms) > OCI_SESSION_IDLE_TIMEOUT_MS)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &stale_keys {
            if let Some(session) = g.remove(key) {
                let freed = u64::try_from(session.buf.len()).unwrap_or(0);
                // Saturating sub: inflight_bytes must never underflow.
                self.inflight_bytes.fetch_sub(
                    self.inflight_bytes.load(Ordering::Relaxed).min(freed),
                    Ordering::Relaxed,
                );
                // Also release the reaped session's bytes from its tenant's
                // per-tenant counter (rt-nuclear #3/#12) — the session key is
                // `<tenant-text>:<uuid>`, so the tenant text is the prefix before
                // the first `:`. (`uploads` is held; `release_tenant_bytes` takes
                // the distinct `tenant_inflight` lock — consistent uploads→tenant
                // ordering, no deadlock.)
                if let Some((reaped_tenant, _)) = key.split_once(':') {
                    self.release_tenant_bytes(reaped_tenant, freed);
                }
                tracing::info!(
                    session_uuid = %key,
                    bytes_freed = freed,
                    "oci: reaped abandoned upload session (idle > {}ms) (audit #6)",
                    OCI_SESSION_IDLE_TIMEOUT_MS,
                );
            }
        }

        let open_count = g.keys().filter(|k| k.starts_with(&session_prefix)).count();
        if open_count >= OCI_MAX_OPEN_SESSIONS_PER_TENANT {
            tracing::warn!(
                tenant_id = %tenant_text,
                open_sessions = open_count,
                limit = OCI_MAX_OPEN_SESSIONS_PER_TENANT,
                "oci: per-tenant upload-session cap reached; rejecting new session (F25)"
            );
            return Err(format!(
                "too many open upload sessions for tenant (limit {OCI_MAX_OPEN_SESSIONS_PER_TENANT})"
            ));
        }
        g.insert(
            uuid.clone(),
            UploadSession {
                buf: Vec::new(),
                last_active_ms: now,
            },
        );
        Ok(uuid)
    }

    async fn append_chunk(
        &self,
        tenant: &TenantId,
        upload_uuid: &str,
        chunk: Bytes,
    ) -> PortResult<u64> {
        if !upload_uuid_belongs_to(tenant, upload_uuid) {
            return Err(format!("upload session not found: {upload_uuid}"));
        }
        let chunk_len =
            u64::try_from(chunk.len()).map_err(|e| format!("oci chunk len overflow: {e}"))?;
        // Global in-flight byte ceiling check (audit #6 / WP-OCI-DOS).
        // Perform the check BEFORE acquiring the session mutex so a
        // ceiling violation doesn't block other tenants for the lock
        // duration. The check is not perfectly atomic with the extend
        // below (two concurrent appends could race to push inflight_bytes
        // over the limit by at most one chunk each), but the ceiling is a
        // conservative soft cap — a one-chunk race window is acceptable
        // and far smaller than the gap between the limit and OOM.
        // CAA-360 #19: ATOMICALLY reserve the chunk's bytes against the global
        // ceiling with a compare_exchange loop, instead of a load-check-then-add
        // (which let two concurrent appends both pass a stale read and overrun
        // the ceiling by up to one chunk each). The reservation IS the credit —
        // there is no separate fetch_add below; on any later failure (session
        // not found) the reserved bytes are released.
        loop {
            let current = self.inflight_bytes.load(Ordering::Relaxed);
            let next = current.saturating_add(chunk_len);
            if next > OCI_MAX_INFLIGHT_BYTES {
                tracing::warn!(
                    tenant_id = %tenant.to_canonical_text(),
                    inflight_bytes = current,
                    chunk_len,
                    limit = OCI_MAX_INFLIGHT_BYTES,
                    "oci: global in-flight byte ceiling reached; rejecting PATCH chunk (audit #6/#19)"
                );
                // Keep the "too many open upload sessions" prefix so the adapter
                // maps it to 429 (the append path matches this prefix too), but make
                // the message ACCURATE — this is the global in-flight BYTE ceiling,
                // not the per-tenant session count, so report the byte limit.
                return Err(format!(
                    "too many open upload sessions for tenant: in-flight byte ceiling \
                     reached (limit {OCI_MAX_INFLIGHT_BYTES} bytes)"
                ));
            }
            // Reserve `next` only if no concurrent writer moved the counter; on a
            // race (Err), retry with a fresh load.
            if self
                .inflight_bytes
                .compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        // PER-TENANT byte budget (rt-nuclear #3/#12 — noisy-neighbour starvation):
        // the global ceiling alone lets ONE tenant fill all 512 MiB and 429 every
        // other tenant. Reserve the chunk against the tenant's own slice
        // (`OCI_MAX_INFLIGHT_BYTES_PER_TENANT`) too; over-slice ⇒ release the
        // GLOBAL reservation we just took and reject 429. Done under the
        // `tenant_inflight` lock so the per-tenant check-and-reserve is atomic.
        let tenant_text = tenant.to_canonical_text();
        {
            let mut t = self
                .tenant_inflight
                .lock()
                .map_err(|e| format!("oci tenant inflight poisoned: {e}"))?;
            let cur = t.get(&tenant_text).copied().unwrap_or(0);
            let next = cur.saturating_add(chunk_len);
            if next > OCI_MAX_INFLIGHT_BYTES_PER_TENANT {
                drop(t);
                // Roll back the global reservation; this append does not proceed.
                self.inflight_bytes.fetch_sub(
                    self.inflight_bytes.load(Ordering::Relaxed).min(chunk_len),
                    Ordering::Relaxed,
                );
                tracing::warn!(
                    tenant_id = %tenant_text,
                    tenant_inflight_bytes = cur,
                    chunk_len,
                    limit = OCI_MAX_INFLIGHT_BYTES_PER_TENANT,
                    "oci: per-tenant in-flight byte budget reached; rejecting PATCH chunk (rt-nuclear #3/#12)"
                );
                // Keep the "too many open upload sessions" prefix so the adapter
                // maps it to 429; make the message accurate (per-tenant byte slice).
                return Err(format!(
                    "too many open upload sessions for tenant: per-tenant in-flight \
                     byte budget reached (limit {OCI_MAX_INFLIGHT_BYTES_PER_TENANT} bytes)"
                ));
            }
            t.insert(tenant_text.clone(), next);
        }
        let mut g = self
            .uploads
            .lock()
            .map_err(|e| format!("oci upload buf poisoned: {e}"))?;
        let current_len = match g.get(upload_uuid) {
            Some(s) => u64::try_from(s.buf.len())
                .map_err(|e| format!("oci upload len overflow before append: {e}"))?,
            None => {
                // The chunk will NOT be appended — release the bytes we reserved
                // above (BOTH counters) so the ceilings are not permanently
                // consumed by a failed append.
                self.inflight_bytes.fetch_sub(
                    self.inflight_bytes.load(Ordering::Relaxed).min(chunk_len),
                    Ordering::Relaxed,
                );
                drop(g);
                self.release_tenant_bytes(&tenant_text, chunk_len);
                // EXACT shape the adapter matches on to emit a 404
                // BLOB_UPLOAD_UNKNOWN (`oci::push::upload` checks
                // `e.starts_with("upload session not found")`).
                return Err(format!("upload session not found: {upload_uuid}"));
            }
        };
        let Some(next_len) = current_len.checked_add(chunk_len) else {
            drop(g);
            self.inflight_bytes.fetch_sub(chunk_len, Ordering::Relaxed);
            self.release_tenant_bytes(&tenant_text, chunk_len);
            return Err(
                "too many open upload sessions for tenant: upload session byte ceiling overflow"
                    .to_owned(),
            );
        };
        if next_len > self.max_blob_size_bytes {
            // The reservation belongs to this rejected chunk. Release BOTH
            // counters before returning so a failed pre-append check cannot
            // strand capacity or make later tenants appear over quota.
            drop(g);
            self.inflight_bytes.fetch_sub(chunk_len, Ordering::Relaxed);
            self.release_tenant_bytes(&tenant_text, chunk_len);
            tracing::warn!(
                tenant_id = %tenant_text,
                current_bytes = current_len,
                chunk_len,
                limit = self.max_blob_size_bytes,
                "oci: per-session upload byte ceiling reached; rejecting PATCH before buffer append"
            );
            return Err(format!(
                "too many open upload sessions for tenant: per-session configured blob byte ceiling reached \
                 (limit {} bytes)",
                self.max_blob_size_bytes
            ));
        }
        let session = g
            .get_mut(upload_uuid)
            .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?;
        session.buf.extend_from_slice(&chunk);
        session.last_active_ms = now_unix_ms();
        // (No fetch_add here — the bytes were atomically reserved above (#19) and
        // mirrored into the per-tenant counter.)
        u64::try_from(session.buf.len()).map_err(|e| format!("oci upload len overflow: {e}"))
    }

    async fn finalize_upload(
        &self,
        tenant: &TenantId,
        upload_uuid: &str,
        blob_key: &str,
        storage_cap_bytes: Option<i64>,
    ) -> PortResult<Bytes> {
        if !upload_uuid_belongs_to(tenant, upload_uuid) {
            return Err(format!("upload session not found: {upload_uuid}"));
        }
        let session = {
            let mut g = self
                .uploads
                .lock()
                .map_err(|e| format!("oci upload buf poisoned: {e}"))?;
            g.remove(upload_uuid)
                .ok_or_else(|| format!("upload session not found: {upload_uuid}"))?
        };
        // Release the bytes from the global AND per-tenant counters BEFORE the
        // async moat write so the ceilings open up as early as possible.
        let freed = u64::try_from(session.buf.len()).unwrap_or(0);
        self.inflight_bytes.fetch_sub(
            self.inflight_bytes.load(Ordering::Relaxed).min(freed),
            Ordering::Relaxed,
        );
        self.release_tenant_bytes(&tenant.to_canonical_text(), freed);
        let assembled = Bytes::from(session.buf.clone());
        // rt-nuclear cycle-2 #2: ENFORCE the content-addressing invariant in the
        // store, BEFORE persisting. `finalize_upload` both ASSEMBLES and PERSISTS,
        // so the caller cannot verify the declared digest before this write (it
        // has no bytes until we return them) — a digest-LIE (`?digest=` ≠ the
        // bytes) would otherwise be persisted under `blob_key` with NO rollback
        // port, breaking content-addressing within the tenant's registry. Reuse
        // the SAME `OciDigest` verify the push handler uses (so the declared
        // algorithm is honored; sha512 is rejected as unsupported, consistent
        // with the upstream verify) and reject a mismatch HERE, before `moat.put`,
        // so a lying digest NEVER reaches the persistent slot (fail-closed; no
        // rollback needed).
        OciDigest::parse(blob_key)
            .and_then(|d| d.verify_against_bytes(&session.buf))
            .map_err(|e| {
                format!("oci finalize: content does not match declared digest {blob_key}: {e:?}")
            })?;
        // Persist content-addressed, mapping the OCI digest (`blob_key`) → blake3
        // content hash. Namespace + cap depend on the flag-gated `_public`
        // routing decision. Writes use the stricter owner-pinned allowlist
        // predicate; reads may additionally use the existence-based `_public`
        // superset without admitting new public content:
        //   * allowlisted owner-pinned digest → shared `_public`, uncapped
        //     (`Some(0)`). This includes index-shaped bytes when they arrive via
        //     blob upload/finalize. The write-time `verify_against_bytes` above
        //     STILL guards this path (fail-closed; a digest-lie never reaches
        //     the shared slot).
        //   * everything else (unlisted blob/config bytes, any blob when the
        //     flag is OFF or the allowlist is deny-all) → PER-TENANT namespace
        //     with the RESOLVED per-tier storage cap threaded from the verified
        //     bearer, so a DOWNGRADED tenant is rejected once over the resolved
        //     cap and an indeterminate cap (`None`) fails CLOSED on an unseeded
        //     tenant (mirrors native). This introduces NO new `_public` writer:
        //     it only ROUTES this existing tenant write to the shared namespace.
        let tenant_ns = tenant.to_canonical_text();
        let (namespace, cap) = if self.routes_to_public(blob_key) {
            (PUBLIC_NAMESPACE, Some(0))
        } else {
            (tenant_ns.as_str(), storage_cap_bytes)
        };
        self.moat
            .put(namespace, blob_key, session.buf, cap)
            .await
            .map_err(|e| match e {
                MoatError::Backend(m) => m,
            })?;
        Ok(assembled)
    }

    async fn cancel_upload(&self, tenant: &TenantId, upload_uuid: &str) -> PortResult<()> {
        if !upload_uuid_belongs_to(tenant, upload_uuid) {
            return Err(format!("upload session not found: {upload_uuid}"));
        }
        let removed = self
            .uploads
            .lock()
            .map_err(|e| format!("oci upload buf poisoned: {e}"))?
            .remove(upload_uuid);
        if let Some(session) = removed {
            let freed = u64::try_from(session.buf.len()).unwrap_or(0);
            self.inflight_bytes.fetch_sub(
                self.inflight_bytes.load(Ordering::Relaxed).min(freed),
                Ordering::Relaxed,
            );
            self.release_tenant_bytes(&tenant.to_canonical_text(), freed);
        }
        Ok(())
    }

    async fn get_blob(&self, tenant: &TenantId, blob_key: &str) -> PortResult<Option<Bytes>> {
        // M2 `_public` READ = EXISTENCE (not allowlist). When `dedup` is on, read
        // the shared `_public` namespace FIRST for ANY blob key, then fall back to
        // the per-tenant namespace.
        //
        // Why an existence-based cross-tenant read of a content-addressed blob is
        // SAFE: an OCI blob is ALWAYS addressed by its `sha256:` digest, so
        // `_public`'s copy of a digest is BYTE-IDENTICAL to any private copy of
        // the same digest — serving the shared copy leaks nothing (there is no
        // "different tenant's bytes" for the same digest). Admission control for
        // what may be IN `_public` is the WRITE gate (client push:
        // `routes_to_public` = allowlist; resolver closure-promote: an allowlisted
        // ROOT), and revocation still filters on read (`MoatCache::get` applies the
        // `public_blocklist` for `_public`). This REPLACES the inc6 allowlist-on-read
        // so a transitively-promoted child layer (config/layer of an allowlisted
        // base, NOT individually allowlisted) serves cross-tenant. The WRITE path
        // (`finalize_upload`) still gates on `routes_to_public`, so a client push
        // to `_public` stays allowlist-gated. Under `dedup == false` (flag OFF)
        // this is byte-identical to the pre-M2 per-tenant-only read.
        if self.dedup {
            if let Some(v) =
                self.moat
                    .get(PUBLIC_NAMESPACE, blob_key)
                    .await
                    .map_err(|e| match e {
                        MoatError::Backend(m) => m,
                    })?
            {
                return Ok(Some(Bytes::from(v)));
            }
        }
        self.moat
            .get(&tenant.to_canonical_text(), blob_key)
            .await
            .map(|opt| opt.map(Bytes::from))
            .map_err(|e| match e {
                MoatError::Backend(m) => m,
            })
    }

    async fn blob_exists(&self, tenant: &TenantId, blob_key: &str) -> PortResult<bool> {
        Ok(self.get_blob(tenant, blob_key).await?.is_some())
    }
}

// ── ManifestKvStore port → durable D1 store ─────────────────────────────────
//
// OCI manifests + tag lists are MUTABLE (a tag re-points to a new manifest on
// every push) and need prefix listing for `/tags/list`, so they cannot live in
// the content-addressed moat. The production binding is the durable D1-backed
// [`crate::adapter_oci_kv::OciKvStore`] (table `adapter_oci_kv`, migration
// 0061), injected into [`router`] below; tests inject an in-memory fake (see
// the test module).

// ── TenantResolver port → the shared PatVerifier (Option B) ─────────────────

/// Thin shell wrapping the shared [`PatVerifier`] as OCI's
/// `TenantResolver`. Hit ONLY on the `/token` leg (Basic → Bearer
/// exchange); the data plane verifies the minted HMAC token instead.
///
/// The port hands us the decoded PAT inside a [`SecretWrap`]; we expose
/// the plaintext for the shared verifier, then parse the returned tenant
/// UUID text into a [`TenantId`]. Every failure collapses to the port's
/// `String` error (the adapter maps it to `401`); a malformed tenant
/// UUID from D1 is a backend fault, also surfaced as the port error.
/// OCI `TenantResolver` over the shared Option-B [`PatVerifier`], plus the
/// per-tier storage-cap resolver ([`crate::oci_cap::TenantCapResolver`]) used to
/// resolve the tenant's RESOLVED (possibly-downgraded) storage cap in the SAME
/// `/token` re-verify call. The cap is surfaced on [`ResolvedPat`] and embedded
/// in the minted bearer (WP #10) so the OCI finalize-blob write reserves against
/// it — closing the cap-on-downgrade residual where the Worker (which forwards
/// OCI RAW) cannot set the native plane's `STORAGE_QUOTA_HEADER`.
///
/// `cap_resolver` is `Option` so dev/CI (no D1 storage env) keeps minting bearers
/// (with an indeterminate cap → data-plane fail-closed on an unseeded tenant),
/// exactly mirroring the native plane when the cap header is absent.
struct OciPatResolver {
    verifier: Arc<PatVerifier>,
    cap_resolver: Option<Arc<dyn crate::oci_cap::TenantCapResolver>>,
    /// G4b tenant-suspend gate (PRIMARY enforcement point). A suspended/erased
    /// tenant's `/token` exchange returns the port error → NO bearer is minted →
    /// push AND pull are blocked at the door. `Option` so dev/CI (no D1 storage
    /// env) keeps minting bearers; when wired it is a
    /// [`crate::oci_suspend::CachedSuspendResolver`] (fail-CLOSED for a
    /// known-suspended tenant, fail-OPEN only for an unknown one).
    suspend_resolver: Option<Arc<dyn crate::oci_suspend::SuspendResolver>>,
}

impl std::fmt::Debug for OciPatResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciPatResolver").finish_non_exhaustive()
    }
}

#[async_trait]
impl TenantResolver for OciPatResolver {
    async fn resolve_pat(&self, pat: &SecretWrap) -> PortResult<TenantId> {
        Ok(self.resolve_pat_capability(pat).await?.tenant)
    }

    async fn resolve_pat_capability(&self, pat: &SecretWrap) -> PortResult<ResolvedPat> {
        // Option B: the container re-verifies the PAT against D1 (HMAC →
        // lookup → Argon2id → scope) and surfaces the write-capability bit
        // so the `/token` exchange downscopes the registry grant — a
        // read-only PAT cannot mint a `push` bearer.
        let (tenant_text, can_write) = self
            .verifier
            .verify_capability(pat.expose())
            .await
            .map_err(|e| match e {
                VerifyError::InvalidPat => "invalid PAT".to_owned(),
                VerifyError::Backend(m) => format!("backend: {m}"),
            })?;
        let uuid = Uuid::parse_str(&tenant_text)
            .map_err(|e| format!("backend: malformed tenant uuid: {e}"))?;
        // G4b PRIMARY suspend gate: a tenant whose offboarding state ∈
        // {suspended, erased} is DENIED the token exchange — return the port
        // error so the mint 401/403s and NO bearer is issued (blocks push AND
        // pull at the door). Fail-CLOSED for a KNOWN-suspended tenant even on a
        // D1 read fault (the resolver folds that into a deny); an UNKNOWN tenant
        // whose read errors (`Err`) fails OPEN — we do NOT block the whole fleet
        // during a D1 outage, and the data plane stays the deeper net. No
        // resolver wired (dev/CI) → skip.
        if let Some(resolver) = self.suspend_resolver.as_ref() {
            if matches!(resolver.suspended_state(&tenant_text).await, Ok(true)) {
                return Err("tenant suspended".to_owned());
            }
        }
        // Resolve the tenant's RESOLVED per-tier storage cap at THIS seam (the
        // only place OCI knows the tenant). `None` when no resolver is wired
        // (dev/CI) OR the cap is indeterminate (D1 error) → embedded as the
        // fail-closed sentinel; the finalize-blob reservation then refuses to
        // seed an uncapped row, mirroring native.
        let storage_cap_bytes = match self.cap_resolver.as_ref() {
            Some(r) => r.resolve_storage_cap(&tenant_text).await,
            None => None,
        };
        Ok(ResolvedPat {
            tenant: TenantId::from_uuid(uuid),
            can_write,
            storage_cap_bytes,
        })
    }
}

// ── Router builder ──────────────────────────────────────────────────────────

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
