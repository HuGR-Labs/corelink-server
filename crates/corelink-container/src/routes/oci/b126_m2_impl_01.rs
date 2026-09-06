/// Service principal stamped on the adapter's CAS operations. Identifies
/// the adapter-host service, NOT the end-user PAT.
const OCI_SERVICE_PRINCIPAL: &str = "oci-adapter-host";

/// Maximum number of concurrent open upload sessions per tenant (F25 guard).
///
/// Each open session holds an in-memory `Vec<u8>` buffer that accumulates blob
/// chunks via `PATCH`. Without a cap, an authenticated tenant can open N sessions
/// and feed each a large body, growing heap to N × chunk-size up to N × 5 GiB.
/// Capping at 4 matches the Turborepo PUT concurrency limit and covers realistic
/// multi-layer parallel push patterns (most OCI clients push 1–3 layers at once).
/// Excess `POST /v2/<repo>/blobs/uploads/` are rejected with an error mapped to
/// 429 by the adapter error layer.
const OCI_MAX_OPEN_SESSIONS_PER_TENANT: usize = 4;

/// Global ceiling on in-flight upload bytes across ALL tenants (audit #6 /
/// WP-OCI-DOS).
///
/// The shared `_oci` Durable Object process holds all tenants' upload sessions
/// in a single heap. Without a cross-tenant ceiling, N tenants each with 4
/// open sessions could together accumulate N × 4 × 5 GiB of heap — unbounded
/// memory exhaustion. This constant caps the TOTAL buffered bytes at any
/// instant across all open sessions.
///
/// 512 MiB is sized so that the maximum realistic multi-tenant burst (e.g.
/// 10 concurrent tenants each pushing a 50 MiB layer) fits comfortably, while
/// a single tenant cannot fill more than a bounded fraction of the ceiling with
/// their per-tenant session cap. When the ceiling is reached, new `PATCH`
/// chunks are rejected with a port error that surfaces as `429 + Retry-After`.
const OCI_MAX_INFLIGHT_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB

/// Per-tenant in-flight byte ceiling (rt-nuclear #3/#12). This is a bounded
/// 128 MiB slice, strictly below the 512 MiB process-wide ceiling; the
/// separate open-session cap provides additional concurrency fairness. It is
/// intentionally independent from the session's configured blob limit.
const OCI_MAX_INFLIGHT_BYTES_PER_TENANT: u64 = 128 * 1024 * 1024;

/// Idle timeout after which an upload session is considered abandoned and is
/// reaped on the next `open_upload` call (lazy reap — audit #6 / WP-OCI-DOS).
///
/// OCI clients that crash or disconnect mid-push leave open sessions that hold
/// memory indefinitely. Without a reaper the per-tenant cap (F25) would
/// permanently lock a tenant out of pushing if their client crashed between
/// `POST` (open) and `PUT` (finalize). 15 minutes covers realistic slow
/// uploads on poor network links while recycling abandoned sessions promptly.
///
/// The reaper is lazy (runs inside `open_upload` under the same mutex lock),
/// so there is no background thread or tokio task — correct for the
/// single-threaded Durable Object runtime.
const OCI_SESSION_IDLE_TIMEOUT_MS: u64 = 15 * 60 * 1000; // 15 min

/// Bearer-realm URL the adapter advertises in `Www-Authenticate` on a
/// `/v2/` 401, pointing OCI clients at the `/token` exchange. The dedicated flat
/// prod host `corelink-oci.humangr.com` — NOW PROVISIONED (CNAME → workers.dev
/// proxied + the `corelink-oci.humangr.com/*` Worker route, 2026-06-21) so a real
/// `docker login/push` reaches the host-agnostic `/v2/` + `/token` OCI paths. The
/// adapter only emits this string; it never fetches it.
const OCI_BEARER_REALM: &str = "https://corelink-oci.humangr.com/token";

/// Env var holding the raw (≥32-byte) HMAC key the adapter uses to sign
/// its realm bearer tokens. Distinct from `PAT_SIGNING_KEY` (which keys
/// the PAT HMAC) — this keys the OCI *session* token only. Read by the
/// container mount ([`crate::routes::build_with_factory`]).
pub const OCI_TOKEN_KEY_ENV: &str = "CORELINK_OCI_TOKEN_KEY";

/// Legacy env var name for the OCI session HMAC key (CAA-360 #8). Prod was
/// deployed with `HUGR_OCI_TOKEN_KEY` while the code canonicalized to
/// `CORELINK_OCI_TOKEN_KEY` — the drift would fail the OCI route CLOSED in prod.
/// The mount reads the canonical name first and falls back to this legacy name
/// so the route works regardless; rename the prod secret to the canonical name
/// to retire this fallback.
pub const OCI_TOKEN_KEY_ENV_LEGACY: &str = "HUGR_OCI_TOKEN_KEY";

// ── BlobStore port → the shared MoatCache ───────────────────────────────────

/// Per-session state for an in-flight OCI blob upload.
///
/// Keyed by `<tenant-text>:<uuid>` in [`OciMoatStore::uploads`].
struct UploadSession {
    /// Accumulated chunk bytes (grows with each `PATCH`).
    buf: Vec<u8>,
    /// Wall-clock ms of the last successful `append_chunk` (or session
    /// open if no chunks have arrived yet). Used by the lazy reaper
    /// ([`OCI_SESSION_IDLE_TIMEOUT_MS`]) to evict abandoned sessions.
    last_active_ms: u64,
}

/// OCI `BlobStore` port → the 2-level [`MoatCache`].
///
/// `blob_key` is the OCI digest wire string (`sha256:<hex>`); we use it
/// directly as the moat `url_hash`, and the moat content-addresses the
/// bytes by blake3 internally (the OCI sha256 digest is NOT a CoreLink
/// content hash). The adapter verifies the declared digest against the
/// uploaded bytes itself (`oci::push::upload::put` →
/// `OciDigest::verify_against_bytes`) BEFORE this store is asked to
/// persist, so this shell never re-hashes for verification.
///
/// Images are stored under the PER-TENANT namespace (the `tenant` arg's
/// canonical text) — isolated by default. **F3.2 increment 6** adds
/// flag-gated cross-tenant dedup: when `dedup` is on AND an owner-pinned
/// digest is [`is_allowlisted`](PublicBaseAllowlist::is_allowlisted), that
/// blob upload/finalize put and blob get are routed to the shared
/// [`crate::adapter_cache::PUBLIC_NAMESPACE`] (uncapped) via the SAME
/// predicate on both paths ([`Self::routes_to_public`]). The predicate has a
/// digest but no media type, so the five baked index digests are eligible too;
/// the upload bytes must still verify against the declared digest before write.
/// This predicate does not govern `PUT /v2/<repo>/manifests/<reference>`:
/// manifest/index JSON remains in the tenant-scoped `ManifestKvStore`.
/// The flag defaults OFF for safe fallback behavior; production enables it via
/// the boot flag after the owner-reviewed six-pin manifest is baked in.
///
/// Upload sessions are buffered in-process in `uploads` keyed by the
/// server-allocated UUID; `finalize_upload` flushes the assembled bytes
/// to the moat AND returns them so the adapter can run its digest
/// verification. The buffer is NOT durable (process restart drops
/// in-flight uploads) — acceptable for the single-container deployment;
/// see OPEN DECISIONS.
///
/// DoS mitigations (audit #6 / WP-OCI-DOS):
///
/// * `inflight_bytes` — atomic counter of bytes currently held across ALL
///   open sessions. Capped at [`OCI_MAX_INFLIGHT_BYTES`]; `PATCH` that
///   would push the counter over the ceiling is rejected with a port error
///   mapped to `429 + Retry-After` by the adapter layer.
/// * Lazy session reaper — `open_upload` evicts sessions idle for more
///   than [`OCI_SESSION_IDLE_TIMEOUT_MS`] ms before checking the per-tenant
///   cap, so a crashed client cannot permanently lock its tenant out of
///   pushing. No background thread is required (DO runtime is
///   single-threaded).
struct OciMoatStore {
    moat: Arc<MoatCache>,
    /// `<tenant-text>:<uuid>` → session state for in-flight uploads.
    uploads: Mutex<HashMap<String, UploadSession>>,
    /// Bytes currently buffered across ALL open upload sessions.
    /// Updated atomically; never allowed to exceed [`OCI_MAX_INFLIGHT_BYTES`].
    inflight_bytes: AtomicU64,
    /// Per-tenant in-flight upload bytes (rt-nuclear #3/#12 — noisy-neighbour
    /// starvation). Keyed by tenant canonical text; each entry is bounded by
    /// [`OCI_MAX_INFLIGHT_BYTES_PER_TENANT`]. Mutated under this lock INSIDE the
    /// `append_chunk` reservation so the per-tenant budget is enforced atomically
    /// with the buffer extend; pruned to zero on release.
    tenant_inflight: Mutex<HashMap<String, u64>>,
    /// F3.2 inc6: is flag-gated cross-tenant `_public` routing of allowlisted
    /// OCI digests enabled? PROD injects
    /// [`crate::public_flags::oci_public_dedup_enabled`] (boot-read, default
    /// OFF); tests inject a literal. When `false` this store is byte-identical
    /// to the per-tenant-only pre-inc6 behavior (the whole WP is a no-op).
    dedup: bool,
    /// The owner-curated, digest-pinned public-base allowlist consulted ONLY
    /// when `dedup` is on. Fail-CLOSED: a malformed baked manifest degrades to
    /// deny-all (empty), never a partially-trusted set. Contains only baked
    /// owner-pinned digests. The predicate deliberately receives no OCI media type:
    /// any owner-pinned digest can route to `_public`, but unpinned digests do
    /// not.
    allowlist: PublicBaseAllowlist,
    /// Adapter-configured maximum blob size. This is the session budget and is
    /// checked before every buffer append; the global and tenant ceilings above
    /// remain the independent process-memory bounds.
    max_blob_size_bytes: u64,
}

impl std::fmt::Debug for OciMoatStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciMoatStore").finish_non_exhaustive()
    }
}

impl OciMoatStore {
    /// Test constructor. `dedup` is the boot flag
    /// ([`crate::public_flags::oci_public_dedup_enabled`]); the allowlist is the
    /// baked owner manifest, loaded FAIL-CLOSED — a malformed manifest degrades
    /// to deny-all (empty) rather than a partially-trusted set, so a broken
    /// manifest can never widen the shared namespace. Production wires
    /// [`Self::with_allowlist`] directly so the allowlist instance is SHARED with
    /// the manifest resolver (loaded once at the router).
    #[cfg(test)]
    fn new(moat: Arc<MoatCache>, dedup: bool) -> Self {
        Self::with_allowlist(
            moat,
            dedup,
            PublicBaseAllowlist::from_baked_manifest().unwrap_or_default(),
        )
    }

    /// Constructor with an explicit allowlist. The production [`Self::new`]
    /// delegates here with the baked manifest; tests inject a hermetic allowlist
    /// so `_public` routing can be exercised with a hermetic allowlist instead of
    /// depending on the production manifest population.
    #[cfg(test)]
    fn with_allowlist(moat: Arc<MoatCache>, dedup: bool, allowlist: PublicBaseAllowlist) -> Self {
        Self::with_allowlist_and_blob_limit(moat, dedup, allowlist, defaults::BLOB_SIZE_LIMIT_BYTES)
    }

    /// Construct the store with the same blob limit used by the adapter. The
    /// explicit argument keeps the in-memory session budget config-derived
    /// rather than duplicating a smaller, hidden constant in this layer.
    fn with_allowlist_and_blob_limit(
        moat: Arc<MoatCache>,
        dedup: bool,
        allowlist: PublicBaseAllowlist,
        max_blob_size_bytes: u64,
    ) -> Self {
        Self {
            moat,
            uploads: Mutex::new(HashMap::new()),
            inflight_bytes: AtomicU64::new(0),
            tenant_inflight: Mutex::new(HashMap::new()),
            dedup,
            allowlist,
            max_blob_size_bytes,
        }
    }

    /// The SINGLE flag+allowlist predicate deciding whether a client-finalized,
    /// owner-pinned digest routes to the shared `_public` namespace. The write
    /// path ([`BlobStore::finalize_upload`]) uses this predicate; the read path
    /// ([`BlobStore::get_blob`]) is deliberately the existence-based superset and
    /// consults `_public` whenever `dedup` is on. This predicate has no media-type
    /// input: it admits exactly the baked digest set, including pinned indexes.
    /// It is used only for blob upload/finalize storage; manifest PUT uses the
    /// tenant-scoped `ManifestKvStore` instead.
    fn routes_to_public(&self, blob_key: &str) -> bool {
        self.dedup && self.allowlist.is_allowlisted(blob_key)
    }

    /// Release `bytes` from a tenant's per-tenant in-flight counter (rt-nuclear
    /// #3/#12), saturating at zero and pruning the entry when it reaches 0 so the
    /// map does not grow without bound. Mirror of the global `fetch_sub` release.
    fn release_tenant_bytes(&self, tenant_text: &str, bytes: u64) {
        if bytes == 0 {
            return;
        }
        if let Ok(mut t) = self.tenant_inflight.lock() {
            if let Some(c) = t.get_mut(tenant_text) {
                *c = c.saturating_sub(bytes);
                if *c == 0 {
                    t.remove(tenant_text);
                }
            }
        }
    }
}

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
