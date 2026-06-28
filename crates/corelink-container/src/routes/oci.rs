//! `/v2/*` + `/token` — OCI Distribution Spec v1.1 registry surface.
//!
//! Mounts the `corelink_adapter_host::oci` adapter (a full OCI
//! Distribution Spec v1.1 registry: `docker` / `podman` / `buildah` /
//! `containerd` / `crane` / Kubernetes image-pull / BuildKit cache /
//! Helm OCI all speak to it) into the container router. Blob bytes go
//! through the shared 2-level content-dedup [`MoatCache`]; manifests +
//! tag lists go through an in-process KV shell (see the KV seam below).
//!
//! # Path shape — why `.merge`, NOT `nest_service`
//!
//! The Worker forwards the request path UNCHANGED (`pathSuffix: path`,
//! `worker/src/index.ts` `matchRoute` `oci_v2` arm — it slices `/v2`
//! only to derive a routing key, it does NOT rewrite the forwarded
//! path). The adapter's router already registers the exact public OCI
//! paths (`/v2/`, `/v2`, `/v2/_catalog`, `/v2/*rest`, `/token`), so we
//! mount it with [`Router::merge`] and add only a scope-gate layer.
//!
//! This is the load-bearing deviation from `routes/brew.rs`:
//!
//! * brew uses `nest_service("/brew", …)` + strips a leading
//!   `<tenant>` segment, because brew's wire path is
//!   `/brew/<tenant>/<bottle-path>` and the bottle path must be
//!   tenant-free before the upstream fetch.
//! * OCI has NO tenant path segment. The first segment after `/v2/`
//!   is the OCI *repository name* (`/v2/alpine/blobs/…`). Stripping it
//!   would corrupt the repo. The tenant is carried inside the
//!   adapter's HMAC bearer token (minted at `/token`), never in the
//!   path. So the OCI gate does pure per-op scope enforcement and NO
//!   path surgery.
//!
//! # Trust + storage model
//!
//! OCI uses a two-leg auth flow (OCI Distribution Spec v1.1 §auth):
//!
//! 1. The client `GET /token` with `Authorization: Basic
//!    base64(user:<pat>)`. The adapter resolves the PAT via its
//!    [`TenantResolver`] port — here [`OciPatResolver`], a thin shell
//!    over the shared [`crate::adapter_pat::PatVerifier`] (Option B:
//!    full re-verify incl. Argon2id, mapping [`VerifyError`] →
//!    [`crate::oci::ports`]'s `String` `PortResult` error). On success
//!    the adapter mints an HMAC bearer token carrying `(tenant, scope,
//!    expiry)` signed with its OWN `token_signing_key`.
//! 2. The client retries `/v2/*` ops with `Authorization: Bearer
//!    <hmac-token>`. The adapter verifies the HMAC locally and reads
//!    the tenant out of the token — the PAT is NOT re-presented and
//!    the shared verifier is NOT hit on the data plane.
//!
//! Consequently the Option-B PAT re-verify runs once per token
//! exchange, exactly at the [`OciPatResolver`] seam. The per-op
//! `x-corelink-scope` gate ([`oci_gate`]) is defence-in-depth on top of
//! the adapter's own bearer-scope checks.
//!
//! Blob bytes are stored via the shared [`MoatCache`] keyed by the OCI
//! digest string (`sha256:<hex>`) as the moat `url_hash`; the bytes are
//! content-addressed by blake3 INSIDE the moat (the OCI sha256 digest
//! is NOT the CoreLink content hash — the moat map provides exactly the
//! `OCI-digest → blake3-content-hash` indirection). Images are stored
//! under the per-tenant namespace (isolated) — see the public-dedup
//! seam in the module-level OPEN DECISIONS.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::Router;
use bytes::Bytes;
use uuid::Uuid;

use corelink_adapter_host::oci::config::defaults;
use corelink_adapter_host::oci::digest::OciDigest;
use corelink_adapter_host::oci::ports::{
    BlobStore, ManifestKvStore, PortResult, ResolvedPat, TenantResolver,
};
use corelink_adapter_host::oci::{router as oci_router, AppState, OciAdapterConfig};
use corelink_audit::ports::{AuditEmitter, InMemoryAuditEmitter};
use corelink_core::{SecretWrap, TenantId};
use corelink_handler_cas::{CasReadHandler, CasWriteHandler};

use crate::adapter_cache::{MoatCache, MoatError, UrlMapStore};
use crate::adapter_pat::{PatVerifier, VerifyError};

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

/// Per-tenant slice of the global in-flight byte ceiling (rt-nuclear #3/#12 —
/// noisy-neighbour starvation). The global [`OCI_MAX_INFLIGHT_BYTES`] alone lets
/// a SINGLE tenant fill the entire 512 MiB ceiling (its session cap × max layer
/// size easily exceeds it), starving every other tenant's pushes (a `429` DoS
/// against innocent tenants). We additionally bound each tenant to `1/8` of the
/// global ceiling (64 MiB) so no one tenant can monopolise more than its slice;
/// a `PATCH` that would push a tenant's own in-flight bytes over this slice is
/// rejected with the same `429`-mapped port error, and the bytes are released on
/// finalize / cancel / failed-append exactly like the global counter.
const OCI_MAX_INFLIGHT_BYTES_PER_TENANT: u64 = OCI_MAX_INFLIGHT_BYTES / 8; // 64 MiB

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
/// canonical text) — isolated by default. Cross-tenant public-image
/// dedup (storing public base images under
/// [`crate::adapter_cache::PUBLIC_NAMESPACE`]) is an explicit follow-up;
/// see the module OPEN DECISIONS.
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
}

impl std::fmt::Debug for OciMoatStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OciMoatStore").finish_non_exhaustive()
    }
}

impl OciMoatStore {
    fn new(moat: Arc<MoatCache>) -> Self {
        Self {
            moat,
            uploads: Mutex::new(HashMap::new()),
            inflight_bytes: AtomicU64::new(0),
            tenant_inflight: Mutex::new(HashMap::new()),
        }
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

        let open_count = g
            .keys()
            .filter(|k| k.starts_with(&session_prefix))
            .count();
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
        let chunk_len = u64::try_from(chunk.len())
            .map_err(|e| format!("oci chunk len overflow: {e}"))?;
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
        let session = match g.get_mut(upload_uuid) {
            Some(s) => s,
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
            .map_err(|e| format!("oci finalize: content does not match declared digest {blob_key}: {e:?}"))?;
        // Persist content-addressed under the per-tenant namespace,
        // mapping the OCI digest (`blob_key`) → blake3 content hash. Thread the
        // RESOLVED per-tier storage cap (from the verified bearer) so the
        // byte-accounting reservation reserves against it — a DOWNGRADED tenant
        // pushing exclusively over OCI is rejected once over the resolved cap,
        // and an indeterminate cap (`None`) fails CLOSED on an unseeded tenant
        // (mirrors native).
        self.moat
            .put(
                &tenant.to_canonical_text(),
                blob_key,
                session.buf,
                storage_cap_bytes,
            )
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
        let (tenant_text, can_write) =
            self.verifier
                .verify_capability(pat.expose())
                .await
                .map_err(|e| match e {
                    VerifyError::InvalidPat => "invalid PAT".to_owned(),
                    VerifyError::Backend(m) => format!("backend: {m}"),
                })?;
        let uuid = Uuid::parse_str(&tenant_text)
            .map_err(|e| format!("backend: malformed tenant uuid: {e}"))?;
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

/// Build the OCI `/v2/*` + `/token` sub-router from the shared CAS
/// handlers + the url→hash map + the durable manifest KV + the PAT
/// verifier + the OCI session HMAC key.
///
/// `cas_read`/`cas_write` are the SAME trait objects cas/ac/bazel/turbo/
/// brew use; `map` is the D1-backed url→content-hash store; `manifest_kv`
/// is the durable mutable manifest/tag store
/// ([`crate::adapter_oci_kv::OciKvStore`] in prod); `verifier` is the
/// shared Option-B PAT verifier; `token_signing_key` is the raw
/// (≥32-byte) HMAC key for the adapter's realm bearer tokens (from
/// [`OCI_TOKEN_KEY_ENV`]). On any construction error the route is simply
/// NOT mounted (empty sub-router + logged) so the container still boots.
///
/// No `x-corelink-scope` gate is layered here: the Worker forwards OCI
/// RAW (pass-through — it cannot resolve a PAT scope for the two-leg flow),
/// so per-op authorization is the adapter's OWN bearer-scope enforcement
/// (`scope.allows(repo, action)` on every `/v2` op) plus the `/token`
/// downscope to the PAT's capability. A header gate would 403 every
/// request under pass-through.
// Wiring/DI constructor: each argument is a distinct production collaborator
// (CAS read/write handlers, URL-map + manifest KV stores, PAT verifier, realm
// signing key, and the two optional quota gates). Bundling them into a params
// struct adds indirection without removing any real coupling, so the
// too-many-arguments lint is suppressed here by intent.
#[allow(clippy::too_many_arguments)]
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    manifest_kv: Arc<dyn ManifestKvStore>,
    verifier: Arc<PatVerifier>,
    token_signing_key: SecretWrap,
    quota: Option<crate::routes::QuotaGate>,
    request_count: Option<crate::request_count::RequestCountGate>,
    cap_resolver: Option<Arc<dyn crate::oci_cap::TenantCapResolver>>,
) -> Router {
    // rt-nuclear #2/#8/#9: the OCI $-ceiling gate must resolve the cost-attribution
    // tenant from the VERIFIED HMAC bearer (the Worker strips `x-corelink-tenant-id`
    // on the OCI pass-through, and an unverified header/claim would let a tenant
    // charge a victim). Capture a copy of the realm signing key for the gate BEFORE
    // `token_signing_key` is moved into the adapter config below. `SecretWrap` is not
    // `Clone` (secret-copy discipline); reconstruct one explicit copy via `new`.
    let gate_realm_key = SecretWrap::new(token_signing_key.expose().to_owned());

    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        OCI_SERVICE_PRINCIPAL,
    ));
    let cas: Arc<dyn BlobStore> = Arc::new(OciMoatStore::new(moat));
    let resolver: Arc<dyn TenantResolver> = Arc::new(OciPatResolver {
        verifier,
        cap_resolver,
    });
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());

    let config = OciAdapterConfig::new(
        // bind_addr is unused by `router`/`build_router` (only
        // `run_oci_adapter` binds).
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        OCI_BEARER_REALM.to_owned(),
        defaults::BLOB_SIZE_LIMIT_BYTES,
        defaults::MULTIPART_CHUNK_SIZE_BYTES,
        // _catalog stays OFF (cross-tenant repo-name leak); the adapter
        // also structurally rejects `true` in `sanity_check`.
        defaults::ENABLE_CATALOG,
        defaults::TOKEN_TTL_SECS,
        token_signing_key,
        cas,
        manifest_kv,
        resolver,
        auditor,
    );

    // Fail-CLOSED on a bad config (e.g. token key <32 bytes): do NOT
    // mount rather than serve an adapter with a weak session-HMAC key.
    if let Err(e) = config.sanity_check() {
        tracing::error!(error = %e, "/v2 OCI config sanity_check failed; OCI NOT mounted");
        return Router::new();
    }

    let state = AppState::new(
        Arc::new(config),
        corelink_adapter_host::oci::wallclock_unix_ms,
    );
    // `.merge` (NOT `nest_service`): the adapter owns `/v2/*` + `/token`
    // verbatim and the Worker forwards the path unchanged.
    let mut router = Router::new().merge(oci_router(state));
    // Per-tenant monthly $-ceiling gate (ADR-0068): OCI previously bypassed the
    // Worker `$`-ceiling/quota path entirely (cluster B). Charge the flat per-op
    // cost on EVERY method (reads AND writes — `docker pull` GET/HEAD of
    // manifests/blobs has real R2 Class-B/egress COGS, exactly like the native
    // CAS/AC read path) so OCI is subject to the SAME ceiling as
    // CAS/AC/Bazel/Turbo. The $-ceiling charge is fail-CLOSED (402 over). Blob
    // BYTE accrual is already enforced by the `AccountingCasHandler` decorator
    // wrapping the shared `cas_write`.
    //
    // rt-nuclear #2/#8/#9: the cost-attribution tenant is resolved from the
    // VERIFIED HMAC bearer (`gate_realm_key`), NOT a request header. The Worker
    // strips `x-corelink-tenant-id` on the OCI pass-through (so the old header
    // read was always empty ⇒ the charge was always skipped — a total $-ceiling
    // bypass), and trusting an unverified header/token claim would let a tenant
    // charge a victim. A write with no VALID bearer is left uncharged because the
    // adapter's data plane 401s it (no billable work succeeds). 402 over-ceiling.
    //
    // rt-nuclear #8 (request-count half): the SAME middleware also meters OCI
    // requests on EVERY method (reads AND writes) against the tenant's monthly
    // REQUEST-count cap (`monthly_request_counts`, migration 0071) — keyed on
    // the SAME verified bearer. The Worker forwards OCI RAW and returns before
    // its `checkRequestQuota` block, so OCI requests bypassed the request cap
    // exactly as they bypassed the $-ceiling; this closes the sibling gap
    // container-side. 429 over the cap (SLO-style, fail-OPEN — unlike the
    // fail-CLOSED $-ceiling).
    //
    // Wire the layer when EITHER gate is present (they are independent axes; in
    // dev/CI without a D1 storage env BOTH are `None` and no layer mounts).
    if quota.is_some() || request_count.is_some() {
        let gate_state = OciCostGate {
            gate: quota,
            request_count,
            realm_key: Arc::new(gate_realm_key),
        };
        router = router.layer(axum::middleware::from_fn_with_state(gate_state, oci_quota_gate));
    }
    router
}

/// State for [`oci_quota_gate`]: the per-tenant `$`-ceiling
/// [`QuotaGate`](crate::routes::QuotaGate) and the monthly request-count
/// [`RequestCountGate`](crate::request_count::RequestCountGate) (each optional /
/// independent), plus a copy of the OCI realm HMAC signing key used to VERIFY
/// the bearer token and recover its tenant (the attribution key) — see the
/// `router` doc.
#[derive(Clone)]
struct OciCostGate {
    gate: Option<crate::routes::QuotaGate>,
    request_count: Option<crate::request_count::RequestCountGate>,
    realm_key: Arc<SecretWrap>,
}

/// Recover the tenant from a `/v2/*` request's `Authorization: Bearer <token>`
/// header by VERIFYING the adapter's HMAC realm token (`corelink_adapter_host::
/// oci::auth::verify`). Returns `None` when there is no bearer or it fails
/// verification (expired / forged / malformed) — the data plane 401s those, so
/// the gate leaves them uncharged. Verifying (not just parsing) is load-bearing:
/// the tenant segment is attacker-controlled, so an unverified read would let one
/// tenant bill another.
// SECURITY-REVIEW (OCI trust path, audit #7 — INFO, pending review): OCI resolves
// the attribution tenant from the HMAC-verified realm bearer here — a SEPARATE
// trust path from the header-based planes (native CAS/AC trust a Worker-injected
// `x-corelink-tenant-id`). Accepted pending review: anon reads with no resolvable
// bearer go UNCOUNTED. Note: for a RESOLVED bearer the $-ceiling is now charged
// fail-CLOSED on reads too (402 over); only the request-count axis is fail-OPEN
// (429). See the findings doc (#7) — the actual trust-path review is a separate task.
fn oci_bearer_tenant(realm_key: &SecretWrap, headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ").or_else(|| raw.strip_prefix("bearer "))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    corelink_adapter_host::oci::auth::verify(realm_key, token, now)
        .ok()
        .map(|vt| vt.tenant.to_canonical_text())
}

/// Per-tenant quota charge for OCI requests. BOTH axes are metered on EVERY
/// method (reads AND writes) — `docker pull` (GET/HEAD of manifests/blobs) does
/// real, billable work on the shared multi-tenant cache (R2 Class-B GETs +
/// egress), so leaving it uncharged let a free tenant loop pulls to evade the
/// monthly request cap and burn unmetered egress (rt-nuclear r34 #1/#11). The
/// `$`-ceiling axis is fail-CLOSED (402 over, charged on reads too — matching
/// the native CAS/AC read path; a prior write-only carve-out, PR #318, let an
/// authenticated tenant pull unlimited blobs without ever hitting their ceiling);
/// the request-count axis is fail-OPEN (429 over). See the `router` doc.
async fn oci_quota_gate(
    axum::extract::State(st): axum::extract::State<OciCostGate>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Attribution tenant comes from the VERIFIED HMAC bearer, never a request
    // header: the Worker strips `x-corelink-tenant-id` on the OCI pass-through
    // (the old header read was always empty ⇒ charge always skipped — a total
    // bypass, rt-nuclear #2/#8/#9). A request with no valid bearer is 401'd by
    // the adapter data plane (writes) / authorized by bearer-scope (reads), so it
    // is left unmetered exactly as before — we only ADD metering when a tenant is
    // resolvable, preserving the current auth semantics for reads.
    if let Some(tenant) = oci_bearer_tenant(&st.realm_key, req.headers()) {
        // $-ceiling (fail-CLOSED, 402 over): charged on EVERY method INCLUDING
        // reads (rt-nuclear cycle-2 #3). R2 Class-B GETs have real COGS, and the
        // native CAS/AC plane (cas.rs / ac.rs) charges reads against the same
        // per-tenant monthly $-ceiling. The previous write-only gate (PR #318)
        // let an authenticated tenant pull unlimited OCI blobs/manifests without
        // ever hitting their ceiling — unmetered egress/R2-GET cost-amplification
        // and a read-path carve-out the native plane does not have. Unauthenticated
        // reads (no resolvable bearer tenant) remain unmetered, exactly as before.
        if let Some(gate) = st.gate.as_ref() {
            if let Some(resp) = gate.check(&tenant).await {
                return resp;
            }
        }
        // Monthly request-count cap (fail-OPEN, 429 over — rt-nuclear #8/#11):
        // charged on EVERY method (reads included). Run AFTER the $-ceiling so an
        // over-budget write rejects 402 before it consumes a request-count slot
        // (the two checks are independent axes; ordering only matters for which
        // response wins on a write that trips both, and the $-cap is the harder
        // business guarantee). It is fail-OPEN per request, so charging GET/HEAD
        // never produces a false 402.
        if let Some(rc) = st.request_count.as_ref() {
            if let Some(resp) = rc.check_and_increment(&tenant).await {
                return resp;
            }
        }
    }
    next.run(req).await
}

// No `oci_gate`: per-op authorization is the adapter's bearer-scope
// enforcement (`scope.allows(repo, action)` on every `/v2` op) plus the
// `/token` downscope to the PAT's capability (see `router` doc). The Worker
// forwards OCI raw, so there is no server-set `x-corelink-scope` to gate on —
// a header gate would 403 every request under pass-through.
//
// F26 — security note: OCI `x-corelink-scope` gate exception (two-leg pass-through)
//
// The Worker intentionally does NOT inject `x-corelink-scope` on any OCI request
// (both `/token` and `/v2/*`). The OCI Distribution Spec v1.1 §auth two-leg flow
// requires that the client presents `Authorization: Basic <pat>` to `/token` and
// receives an HMAC bearer. Because the Worker cannot resolve the PAT scope at
// forward time (it does not re-verify the PAT), it cannot set the header; a gate
// here would `403` every OCI request, including the initial token exchange.
//
// Compensating controls that make this safe:
//   1. `/token` runs the full Option-B PAT re-verify (HMAC + D1 lookup + Argon2id
//      via `OciPatResolver` → `PatVerifier::verify_capability`). An invalid or
//      revoked PAT returns 401 before a bearer is minted.
//   2. The minted bearer carries the PAT's REAL capability (read vs read/write) via
//      the `can_write` downscope path (`OciScope::restricted_to_read` on `cas:r`
//      PATs), so a read-only PAT cannot obtain a `push` bearer.
//   3. Every `/v2/*` data-plane op verifies the HMAC bearer locally
//      (`crate::oci::auth::verify`) and enforces `scope.allows(repo, action)`
//      before touching any port. There is no unauthenticated code path.
//   4. The per-tenant upload-session cap (F25, `OCI_MAX_OPEN_SESSIONS_PER_TENANT`)
//      limits in-memory abuse from a valid but malicious authenticated tenant.
//
// Net: the absence of `x-corelink-scope` is a necessary protocol accommodation,
// not a gap. The PAT re-verify + bearer scope-downscope + per-op scope enforcement
// provide equivalent or stronger defence than the header gate would on the other
// adapters (which trust the Worker-injected header rather than re-verifying).
//
// If the Worker is extended to resolve OCI PAT scopes at forward time, the gate
// SHOULD be added for defence-in-depth — but doing so requires the Worker to
// perform Argon2id-equivalent work on every OCI call, which is out of scope.

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;

    use axum::body::Body;
    use axum::http::{Method, Request as HttpRequest, StatusCode};
    use base64::Engine as _;
    use corelink_handler_cas::{
        CasHandlerError, CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
    };
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId as PatTenantId,
        SCOPE_CACHE_R, SCOPE_CACHE_RW,
    };
    use tower::ServiceExt; // for `.oneshot`
    use uuid::Uuid;

    use crate::adapter_cache::canonical_hash_hex;
    use crate::adapter_pat::{PatRow, PatRowLookup};
    use crate::scope::SCOPE_HEADER;

    use super::*;

    const SCOPE_RW: &str = "cas:rw";
    /// 32-byte raw HMAC key for the adapter's session tokens (config
    /// `sanity_check` requires ≥32 bytes).
    const OCI_KEY: &str = "oci-session-key-0123456789abcdef"; // 32 bytes

    /// `PatRowLookup` that knows ONE token_id → row; everything else
    /// unknown.
    struct OneTokenLookup {
        token_id: String,
        row: PatRow,
    }
    #[async_trait]
    impl PatRowLookup for OneTokenLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            Ok((token_id == self.token_id).then(|| self.row.clone()))
        }
    }

    /// `PatRowLookup` that knows nothing (rejects every token).
    struct EmptyLookup;
    #[async_trait]
    impl PatRowLookup for EmptyLookup {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            Ok(None)
        }
    }

    /// In-memory url→content-hash map (mirrors brew's `FakeMap`).
    #[derive(Default)]
    struct FakeMap(Mutex<HashMap<(String, String), String>>);
    #[async_trait]
    impl UrlMapStore for FakeMap {
        async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), url_hash.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            ns: &str,
            url_hash: &str,
            content_hash: &str,
            _len: u64,
        ) -> Result<(), String> {
            self.0.lock().unwrap().insert(
                (ns.to_owned(), url_hash.to_owned()),
                content_hash.to_owned(),
            );
            Ok(())
        }
    }

    /// In-memory `ManifestKvStore` fake for the route tests (prod wires the
    /// durable `crate::adapter_oci_kv::OciKvStore`).
    #[derive(Default, Debug)]
    struct OciKvFake(Mutex<HashMap<(String, String), Bytes>>);
    #[async_trait]
    impl ManifestKvStore for OciKvFake {
        async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(tenant.to_canonical_text(), key.to_owned()))
                .cloned())
        }
        async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()> {
            self.0
                .lock()
                .unwrap()
                .insert((tenant.to_canonical_text(), key.to_owned()), value);
            Ok(())
        }
        async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>> {
            let t = tenant.to_canonical_text();
            Ok(self
                .0
                .lock()
                .unwrap()
                .keys()
                .filter(|(kt, ks)| *kt == t && ks.starts_with(prefix))
                .map(|(_, ks)| ks.clone())
                .collect())
        }
    }

    /// Non-verifying CAS stub (accepts any claimed_hash) — `oci::router`
    /// wires production `canonical_hash_hex`, which a verifying in-memory
    /// handler would reject. Keyed by `(tenant, hash)`.
    #[derive(Debug, Default)]
    struct StubCas(Mutex<HashMap<(String, String), Vec<u8>>>);
    impl CasReadHandler for StubCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            match self
                .0
                .lock()
                .unwrap()
                .get(&(req.tenant.clone(), req.hash.clone()))
            {
                Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
                None => Err(CasHandlerError::Internal("stub: absent".into())),
            }
        }
    }
    impl CasWriteHandler for StubCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            self.0
                .lock()
                .unwrap()
                .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }
    impl corelink_handler_cas::CasDeleteHandler for StubCas {
        fn delete(
            &self,
            req: corelink_handler_cas::CasDeleteRequest,
        ) -> Result<corelink_handler_cas::CasDeleteResponse, CasHandlerError> {
            let removed = self
                .0
                .lock()
                .unwrap()
                .remove(&(req.tenant.clone(), req.hash.clone()));
            let reclaimed = removed.as_ref().map(|b| b.len() as u64).unwrap_or(0);
            Ok(corelink_handler_cas::CasDeleteResponse::with_reclaimed(
                removed.is_some(),
                reclaimed,
            ))
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Router whose verifier rejects ALL PATs (empty lookup); cas/map
    /// unused. Session HMAC key is valid so the route MOUNTS.
    fn router_rejecting() -> Router {
        let cas: Arc<StubCas> = Arc::new(StubCas::default());
        let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
        router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,
            None,
            None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        )
    }

    fn req(
        method: Method,
        uri: &str,
        authorization: Option<&str>,
        scope: Option<&str>,
    ) -> HttpRequest<Body> {
        let mut b = HttpRequest::builder().method(method).uri(uri);
        if let Some(a) = authorization {
            b = b.header("authorization", a);
        }
        if let Some(s) = scope {
            b = b.header(SCOPE_HEADER, s);
        }
        b.body(Body::empty()).unwrap()
    }

    /// `Authorization: Basic base64("oci:<pat>")` — the OCI Basic leg.
    fn basic(pat: &str) -> String {
        let enc = base64::engine::general_purpose::STANDARD.encode(format!("oci:{pat}"));
        format!("Basic {enc}")
    }

    #[tokio::test]
    async fn read_only_pat_token_is_downscoped_to_pull_only() {
        // SECURITY (scope-escalation fix): a read-only (`cas:r`) PAT
        // exchanging at `/token` for `push,pull` is granted PULL-ONLY. The
        // minted bearer still PULLS (GET an absent blob → 404, i.e. the pull
        // scope passed) but CANNOT PUSH (manifest PUT → denied).
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xCAFE);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0x1234)),
            PatScopes::from_u64(SCOPE_CACHE_R), // READ-ONLY
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: "cas:r".to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));
        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,
            None,
            None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        );

        // Exchange the read-only PAT (requesting push,pull) for a bearer.
        let resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:push,pull",
                Some(&basic(&pt)),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = json["token"].as_str().expect("token field").to_owned();

        // PULL is granted: GET an absent blob → 404 (pull scope passed; a
        // denied scope would be 401/403).
        let get = req(
            Method::GET,
            "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
            Some(&format!("Bearer {bearer}")),
            None,
        );
        let resp = app.clone().oneshot(get).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "pull-only bearer must still pull (absent blob ⇒ 404)"
        );

        // PUSH is denied: a manifest PUT with the pull-only bearer is rejected.
        let put = HttpRequest::builder()
            .method(Method::PUT)
            .uri("/v2/alpine/manifests/latest")
            .header("authorization", format!("Bearer {bearer}"))
            .header("content-type", "application/vnd.oci.image.manifest.v1+json")
            .body(Body::from(r#"{"schemaVersion":2}"#))
            .unwrap();
        let resp = app.oneshot(put).await.unwrap();
        assert!(
            !resp.status().is_success(),
            "read-only PAT must not push (pull-only bearer); got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn missing_bearer_on_data_plane_is_401_reaches_adapter() {
        // scope present → gate passes → adapter `/v2/*` dispatch finds no
        // bearer → 401 + Www-Authenticate. Proves `.merge` routed to the
        // adapter.
        let app = router_rejecting();
        let resp = app
            .oneshot(req(
                Method::GET,
                "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
                None,
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().get("Www-Authenticate").is_some());
    }

    #[tokio::test]
    async fn forged_bearer_on_data_plane_is_401() {
        // A `corelink-oci.`-shaped but HMAC-invalid bearer → adapter
        // verify fails → 401. (The data plane never touches the PAT
        // resolver — it verifies the session HMAC.)
        let forged = "corelink-oci.00000000-0000-0000-0000-000000000000.cmVwb3NpdG9yeTphbHBpbmU6cHVsbA.9999999999.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let app = router_rejecting();
        let resp = app
            .oneshot(req(
                Method::GET,
                "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
                Some(&format!("Bearer {forged}")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_pat_at_token_is_401_resolver_runs() {
        // `/token` with a `corelink_`-prefixed but HMAC-invalid PAT in
        // Basic auth → gate passes (GET ⇒ read) → adapter `/token` →
        // OciPatResolver → shared verifier → InvalidPat → 401. Proves
        // `.merge` routed `/token` to the adapter AND the Option-B
        // resolver ran.
        let app = router_rejecting();
        let resp = app
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:pull",
                Some(&basic("corelink_not-a-real-token")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn token_exchange_then_blob_pull_round_trip() {
        // End-to-end: mint a real PAT; exchange it at `/token` for an
        // HMAC bearer; seed the moat with a blob under the PAT's tenant
        // namespace; pull it back → 200 + bytes. Proves: `.merge` mount,
        // Option-B resolve at `/token`, HMAC mint/verify, scope gate,
        // and the moat blob-key (OCI digest → blake3) round-trip.
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xBEEF);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0xF00D)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

        // Seed the moat: a blob whose OCI digest is the moat url_hash,
        // bytes stored content-addressed by blake3, under the PAT
        // tenant's namespace (canonical UUID text).
        let bytes = b"oci-layer-bytes".to_vec();
        let oci_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        let content_hash = canonical_hash_hex(&bytes);
        let tenant_ns = tenant_uuid.to_string();
        let cas = Arc::new(StubCas::default());
        cas.0
            .lock()
            .unwrap()
            .insert((tenant_ns.clone(), content_hash.clone()), bytes.clone());
        let map = Arc::new(FakeMap::default());
        map.0
            .lock()
            .unwrap()
            .insert((tenant_ns, oci_digest.to_owned()), content_hash);

        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,
            None,
            None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        );

        // Leg 1: exchange the PAT (Basic) for an HMAC bearer at /token.
        let resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:pull",
                Some(&basic(&pt)),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = json["token"].as_str().expect("token field").to_owned();

        // Leg 2: pull the seeded blob with the minted bearer.
        let resp = app
            .oneshot(req(
                Method::GET,
                &format!("/v2/alpine/blobs/{oci_digest}"),
                Some(&format!("Bearer {bearer}")),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let pulled = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(pulled.as_ref(), bytes.as_slice());
    }

    #[tokio::test]
    async fn finalize_rejects_digest_lie_and_persists_nothing() {
        // rt-nuclear cycle-2 #2: a finalize that declares a digest NOT matching
        // the uploaded bytes MUST be rejected BEFORE the bytes are persisted, so
        // no digest-lie ever lands in the (tenant, blob_key) slot — content-
        // addressing is enforced by the store itself, not just the caller.
        let cas = Arc::new(StubCas::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-test",
        ));
        let store = OciMoatStore::new(moat);
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xC));
        let uuid = store.open_upload(&tenant).await.unwrap();
        store
            .append_chunk(&tenant, &uuid, Bytes::from_static(b"real-content"))
            .await
            .unwrap();
        // A lying digest (64 hex zeros) — NOT sha256("real-content").
        let lie = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        assert!(
            store.finalize_upload(&tenant, &uuid, lie, None).await.is_err(),
            "a digest-lie finalize must be rejected"
        );
        // And NOTHING was persisted under the lying key (no poisoned slot).
        assert!(
            store.get_blob(&tenant, lie).await.unwrap().is_none(),
            "a rejected digest-lie must not leave a persisted slot"
        );
    }

    #[tokio::test]
    async fn upload_session_is_tenant_scoped() {
        // Confused-deputy guard: the shared `_oci` DO buffers ALL tenants'
        // uploads in one process map, so a tenant may only append/cancel/
        // finalize a session it opened. A cross-tenant uuid must look like an
        // absent session (no existence oracle) and must NOT mutate it.
        let cas = Arc::new(StubCas::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-test",
        ));
        let store = OciMoatStore::new(moat);
        let tenant_a = TenantId::from_uuid(Uuid::from_u128(0xA));
        let tenant_b = TenantId::from_uuid(Uuid::from_u128(0xB));

        let uuid = store.open_upload(&tenant_a).await.unwrap();

        // Tenant B cannot touch tenant A's session (each → not-found error).
        assert!(store
            .append_chunk(&tenant_b, &uuid, Bytes::from_static(b"x"))
            .await
            .is_err());
        assert!(store.cancel_upload(&tenant_b, &uuid).await.is_err());
        assert!(store
            .finalize_upload(&tenant_b, &uuid, "sha256:00", None)
            .await
            .is_err());

        // A's session is intact (B's attempts were rejected before any mutation),
        // so tenant A can still append.
        assert!(store
            .append_chunk(&tenant_a, &uuid, Bytes::from_static(b"x"))
            .await
            .is_ok());
    }

    /// Build an `OciMoatStore` whose moat write handler is the REAL
    /// `AccountingCasHandler` (byte-accounting) over an in-memory `ByteStore`,
    /// so a `finalize_upload` reserves against the threaded cap exactly as
    /// production does. Returns the store + the byte store (to assert the
    /// counter) + the byte region.
    fn accounting_oci_store() -> (
        OciMoatStore,
        Arc<crate::byte_accounting::testing::InMemoryByteStore>,
        String,
    ) {
        use crate::byte_accounting::{
            testing::InMemoryByteStore, AccountingCasHandler, ByteAccountant,
        };
        // `StubCas` is non-verifying (accepts any claimed_hash) — the moat's
        // `production` ctor wires the real `canonical_hash_hex`, which a verifying
        // in-memory handler would reject. The byte-accounting decorator wraps it
        // exactly as production wraps the R2 handler.
        let inner = Arc::new(StubCas::default());
        let byte_store = Arc::new(InMemoryByteStore::new());
        let region = "iad".to_owned();
        let accountant = Arc::new(ByteAccountant::new(byte_store.clone(), region.clone()));
        let acct = Arc::new(AccountingCasHandler::new(
            Arc::clone(&inner) as Arc<dyn CasWriteHandler>,
            Arc::clone(&inner) as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
            accountant,
        ));
        let moat = Arc::new(MoatCache::production(
            inner as Arc<dyn CasReadHandler>,
            acct as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-cap-test",
        ));
        (OciMoatStore::new(moat), byte_store, region)
    }

    /// Push a blob of `len` bytes through open→append→finalize under `cap`.
    async fn push_blob(store: &OciMoatStore, tenant: &TenantId, len: usize, cap: Option<i64>)
        -> Result<(), String> {
        let bytes = vec![0xABu8; len];
        let digest = corelink_adapter_host::oci::digest::OciDigest::compute(
            corelink_adapter_host::oci::digest::OciDigestAlgo::Sha256,
            &bytes,
        )
        .map_err(|e| format!("{e:?}"))?;
        let uuid = store.open_upload(tenant).await?;
        store
            .append_chunk(tenant, &uuid, Bytes::from(bytes))
            .await?;
        store
            .finalize_upload(tenant, &uuid, &digest.to_wire(), cap)
            .await
            .map(|_| ())
    }

    #[tokio::test]
    async fn downgraded_tenant_oci_write_over_resolved_cap_is_rejected() {
        // WP #10 regression. A DOWNGRADED tenant (resolved cap = 1000 bytes)
        // pushing exclusively over OCI:
        //   * an UNDER-cap push succeeds and accrues bytes_used;
        //   * an OVER-cap push is REJECTED (the resolved cap threaded from the
        //     bearer reserves against `tenant_storage_state`), so OCI can no
        //     longer over-store past the (possibly stale) cap.
        let (store, byte_store, region) = accounting_oci_store();
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xD0_0D));
        let t_text = tenant.to_canonical_text();
        let cap = Some(1_000i64);

        // 600 < 1000 → accrues, seeds the row with the REAL cap.
        push_blob(&store, &tenant, 600, cap)
            .await
            .expect("under-cap OCI push must succeed");
        assert_eq!(byte_store.used(&t_text, &region), 600);

        // 500 more would total 1100 > 1000 → REJECTED (over the resolved cap).
        let err = push_blob(&store, &tenant, 500, cap)
            .await
            .expect_err("over-cap OCI push must be rejected");
        assert!(
            err.contains(crate::byte_accounting::OVER_CAP_SENTINEL),
            "over-cap rejection must carry the 402 sentinel; got: {err}"
        );
        // The rejected push did NOT move the counter.
        assert_eq!(byte_store.used(&t_text, &region), 600);
    }

    #[tokio::test]
    async fn oci_write_with_indeterminate_cap_on_fresh_tenant_fails_closed() {
        // WP #10 fail-closed mirror of native: an unresolvable cap (`None`) on a
        // tenant with NO `tenant_storage_state` row must be REFUSED (never seed
        // an uncapped row from absence) — absence is never treated as unlimited.
        let (store, byte_store, region) = accounting_oci_store();
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xFEED));
        let err = push_blob(&store, &tenant, 100, None)
            .await
            .expect_err("indeterminate cap on a fresh tenant must fail closed");
        assert!(
            err.contains(crate::byte_accounting::ACCT_UNAVAILABLE_SENTINEL),
            "fresh-tenant indeterminate cap must fail closed (503 sentinel); got: {err}"
        );
        assert_eq!(
            byte_store.used(&tenant.to_canonical_text(), &region),
            0,
            "a fail-closed push must not seed or move the counter"
        );
    }

    #[tokio::test]
    async fn open_upload_enforces_per_tenant_session_cap() {
        // F25 — per-tenant open-session cap.
        //
        // Open OCI_MAX_OPEN_SESSIONS_PER_TENANT sessions for tenant A; the next
        // open must fail. Tenant B is unaffected (independent counter).
        let cas = Arc::new(StubCas::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-test-cap",
        ));
        let store = OciMoatStore::new(moat);
        let tenant_a = TenantId::from_uuid(Uuid::from_u128(0xAA));
        let tenant_b = TenantId::from_uuid(Uuid::from_u128(0xBB));

        // Fill tenant A's quota.
        let mut sessions = Vec::new();
        for _ in 0..OCI_MAX_OPEN_SESSIONS_PER_TENANT {
            let uuid = store.open_upload(&tenant_a).await.unwrap();
            sessions.push(uuid);
        }
        assert_eq!(sessions.len(), OCI_MAX_OPEN_SESSIONS_PER_TENANT);

        // One more for tenant A must be rejected (cap reached).
        let err = store.open_upload(&tenant_a).await.unwrap_err();
        assert!(
            err.contains("too many open upload sessions"),
            "expected cap error, got: {err}"
        );

        // Tenant B is not affected by tenant A's sessions.
        let b_session = store.open_upload(&tenant_b).await;
        assert!(
            b_session.is_ok(),
            "tenant B must not be blocked by tenant A's sessions"
        );

        // After cancelling one of A's sessions the cap is relaxed.
        store
            .cancel_upload(&tenant_a, &sessions[0])
            .await
            .unwrap();
        let new_session = store.open_upload(&tenant_a).await;
        assert!(
            new_session.is_ok(),
            "tenant A must be able to open a new session after cancelling one"
        );
    }

    /// F25 route-level test: `POST /v2/<repo>/blobs/uploads/` returns
    /// `429 Too Many Requests` + `Retry-After` when the per-tenant session
    /// cap is exhausted.
    ///
    /// This exercises the full HTTP path (adapter router → `open()` handler
    /// → `OciMoatStore::open_upload` → error-mapping → `err_response`) so
    /// we verify both the status code and the presence of the `Retry-After`
    /// header.
    #[tokio::test]
    async fn upload_session_cap_returns_429_with_retry_after() {
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xCAFF00);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0xDEAD)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));
        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,
            None,
            None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        );

        // Obtain a push+pull bearer for the test tenant.
        let token_resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:myrepo:push,pull",
                Some(&basic(&pt)),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(token_resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(token_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = format!("Bearer {}", json["token"].as_str().expect("token field"));

        // Open OCI_MAX_OPEN_SESSIONS_PER_TENANT sessions — each must return 202.
        for _ in 0..OCI_MAX_OPEN_SESSIONS_PER_TENANT {
            let resp = app
                .clone()
                .oneshot(
                    HttpRequest::builder()
                        .method(Method::POST)
                        .uri("/v2/myrepo/blobs/uploads/")
                        .header("authorization", &bearer)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::ACCEPTED,
                "expected 202 while filling quota"
            );
        }

        // The next POST must hit the cap → 429 + Retry-After.
        let resp = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method(Method::POST)
                    .uri("/v2/myrepo/blobs/uploads/")
                    .header("authorization", &bearer)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "expected 429 when session cap is reached"
        );
        assert!(
            resp.headers().contains_key("retry-after"),
            "429 response must carry a Retry-After header"
        );
    }

    /// Audit #6 / WP-OCI-DOS — global in-flight byte ceiling.
    ///
    /// Sending a chunk that would push the global inflight byte counter over
    /// [`OCI_MAX_INFLIGHT_BYTES`] must be rejected with the same
    /// "too many open upload sessions" port error (the adapter maps this to
    /// 429). This verifies that a single tenant cannot exhaust the shared
    /// heap by sending one very large chunk even if it stays below the blob
    /// size limit.
    ///
    /// We bypass the ceiling constant by directly manipulating the atomic
    /// counter on the `OciMoatStore` — we don't need to allocate GiBs of
    /// RAM to test the guard.
    #[tokio::test]
    async fn append_chunk_respects_global_inflight_ceiling() {
        let cas = Arc::new(StubCas::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-test-ceiling",
        ));
        let store = OciMoatStore::new(moat);
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xCE117));

        let uuid = store.open_upload(&tenant).await.unwrap();

        // Pre-load the global counter to just below the ceiling so a
        // 1-byte chunk tip it over.
        store
            .inflight_bytes
            .store(OCI_MAX_INFLIGHT_BYTES, Ordering::Relaxed);

        let err = store
            .append_chunk(&tenant, &uuid, Bytes::from_static(b"x"))
            .await
            .unwrap_err();
        assert!(
            err.contains("too many open upload sessions"),
            "expected ceiling error, got: {err}"
        );

        // After cancel the counter is released and a fresh session can accept
        // chunks (reset counter so the test is self-contained).
        store.cancel_upload(&tenant, &uuid).await.unwrap();
        store.inflight_bytes.store(0, Ordering::Relaxed);

        let uuid2 = store.open_upload(&tenant).await.unwrap();
        let ok = store
            .append_chunk(&tenant, &uuid2, Bytes::from_static(b"hello"))
            .await;
        assert!(ok.is_ok(), "chunk must succeed when counter is reset");
    }

    /// rt-nuclear #3/#12 — per-tenant in-flight byte budget (noisy-neighbour
    /// starvation). With ONE tenant at its per-tenant slice (well below the
    /// GLOBAL ceiling), that tenant's next chunk is rejected 429-mapped, while a
    /// DIFFERENT tenant can still push — proving the per-tenant cap isolates
    /// tenants. The counter is released on cancel so the tenant recovers.
    #[tokio::test]
    async fn append_chunk_respects_per_tenant_inflight_budget() {
        let cas = Arc::new(StubCas::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-test-per-tenant",
        ));
        let store = OciMoatStore::new(moat);
        let hog = TenantId::from_uuid(Uuid::from_u128(0x803));
        let victim = TenantId::from_uuid(Uuid::from_u128(0x71C7100));

        // The global counter is far below the global ceiling — only the
        // per-tenant slice should trip here.
        let hog_uuid = store.open_upload(&hog).await.unwrap();
        store
            .tenant_inflight
            .lock()
            .unwrap()
            .insert(hog.to_canonical_text(), OCI_MAX_INFLIGHT_BYTES_PER_TENANT);

        let err = store
            .append_chunk(&hog, &hog_uuid, Bytes::from_static(b"x"))
            .await
            .unwrap_err();
        assert!(
            err.contains("too many open upload sessions"),
            "expected per-tenant budget 429-mapped error, got: {err}"
        );
        // The global counter must NOT have been left credited by the rejected
        // append (the global reservation was rolled back).
        assert_eq!(
            store.inflight_bytes.load(Ordering::Relaxed),
            0,
            "a per-tenant-budget rejection must roll back the global reservation"
        );

        // A DIFFERENT tenant is unaffected — no cross-tenant starvation.
        let victim_uuid = store.open_upload(&victim).await.unwrap();
        let ok = store
            .append_chunk(&victim, &victim_uuid, Bytes::from_static(b"hello"))
            .await;
        assert!(
            ok.is_ok(),
            "a second tenant must still push while the first is at its per-tenant budget"
        );

        // After the hog cancels, its per-tenant counter is released and it can
        // push again.
        store.cancel_upload(&hog, &hog_uuid).await.unwrap();
        store
            .tenant_inflight
            .lock()
            .unwrap()
            .remove(&hog.to_canonical_text());
        let hog_uuid2 = store.open_upload(&hog).await.unwrap();
        let recovered = store
            .append_chunk(&hog, &hog_uuid2, Bytes::from_static(b"again"))
            .await;
        assert!(recovered.is_ok(), "tenant must recover after its budget is released");
    }

    /// Audit #6 / WP-OCI-DOS — lazy abandoned-session reaper.
    ///
    /// Sessions that have been idle for longer than [`OCI_SESSION_IDLE_TIMEOUT_MS`]
    /// are reaped on the next `open_upload`. This prevents a crashed client from
    /// permanently locking its tenant out of the per-tenant session cap (F25).
    ///
    /// We inject a stale session directly into the store's upload map to avoid
    /// needing to sleep for the full 15-minute timeout.
    #[tokio::test]
    async fn open_upload_reaps_abandoned_sessions() {
        let cas = Arc::new(StubCas::default());
        let moat = Arc::new(MoatCache::production(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            "oci-test-reaper",
        ));
        let store = OciMoatStore::new(moat);
        let tenant = TenantId::from_uuid(Uuid::from_u128(0xABAD1DEA));
        let tenant_text = tenant.to_canonical_text();

        // Fill the per-tenant cap with synthetic stale sessions (last_active
        // set to epoch 0, guaranteed older than any timeout).
        {
            let mut g = store.uploads.lock().unwrap();
            for i in 0..OCI_MAX_OPEN_SESSIONS_PER_TENANT {
                let stale_uuid = format!("{tenant_text}:stale{i:04}");
                g.insert(
                    stale_uuid,
                    UploadSession {
                        buf: vec![0u8; 1024],
                        last_active_ms: 0, // epoch → always stale
                    },
                );
            }
            // Reflect the fake bytes in the global counter.
            store
                .inflight_bytes
                .store((OCI_MAX_OPEN_SESSIONS_PER_TENANT as u64) * 1024, Ordering::Relaxed);
        }

        // The cap is now full (OCI_MAX_OPEN_SESSIONS_PER_TENANT stale
        // sessions). Without the reaper, open_upload would return an error.
        // With the reaper, all stale sessions are evicted BEFORE the cap check
        // and a new session is opened successfully.
        let result = store.open_upload(&tenant).await;
        assert!(
            result.is_ok(),
            "open_upload must reap stale sessions and succeed; got: {:?}",
            result.err()
        );

        // Stale sessions freed their bytes from the global counter.
        // The new session added 0 bytes (empty buffer), so inflight_bytes
        // should be 0 after reap.
        assert_eq!(
            store.inflight_bytes.load(Ordering::Relaxed),
            0,
            "inflight_bytes must be 0 after stale sessions are reaped"
        );
    }

    /// Audit #5 / WP-OCI-DOS — manifest PUT body cap.
    ///
    /// A `PUT /v2/<repo>/manifests/<ref>` body larger than
    /// `MAX_MANIFEST_BYTES` (4 MiB) must be rejected with `413 Payload Too
    /// Large` before any heap allocation for schema parsing. This exercises
    /// the full HTTP path through the router so we see the correct status code.
    #[tokio::test]
    async fn manifest_put_oversized_body_returns_413() {
        use corelink_adapter_host::oci::server::handlers::MAX_MANIFEST_BYTES;

        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0x0DEBAD);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0xF00D)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));
        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,
            None,
            None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        );

        // Get a push bearer.
        let token_resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:myimg:push,pull",
                Some(&basic(&pt)),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(token_resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(token_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let bearer = format!("Bearer {}", json["token"].as_str().expect("token field"));

        // Send a body that is 1 byte over the cap — must be 413.
        let oversized = vec![b'x'; MAX_MANIFEST_BYTES + 1];
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::PUT)
                    .uri("/v2/myimg/manifests/latest")
                    .header("authorization", bearer)
                    .header("content-type", "application/vnd.oci.image.manifest.v1+json")
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "oversized manifest body must return 413"
        );
    }

    // ── Cluster E / A24: UNAUTH /token backend fault is OPAQUE ─────────────────

    /// `PatRowLookup` that always fails with a RAW backend string mimicking the
    /// CF D1 HTTP API error (status + body, possibly SQL). The verifier maps
    /// this to `VerifyError::Backend`, which the OCI resolver surfaces as the
    /// adapter `Auth` error — the A24 leak path.
    struct BackendErrLookup;
    #[async_trait]
    impl PatRowLookup for BackendErrLookup {
        async fn lookup(&self, _token_id: &str) -> Result<Option<PatRow>, String> {
            Err(
                "D1 HTTP 500 Internal Server Error: {\"errors\":[{\"code\":7500,\
                 \"message\":\"no such table: pat in SELECT tenant_id, pat_hash, scope FROM pat WHERE token_id = ?1\"}]}"
                    .to_owned(),
            )
        }
    }

    #[tokio::test]
    async fn token_backend_fault_is_opaque_to_unauth_caller() {
        // A24 (UNAUTH-reachable, highest priority): the OCI `/token` leg runs
        // the PAT-verify backend. When that backend faults, the public response
        // must NOT echo the raw CF D1 API error (status / body / SQL). An
        // UNauthenticated caller (it presents only a syntactically-valid PAT in
        // Basic, never a verified credential) must learn nothing about the
        // internal store. The OCI error envelope SHAPE is preserved; only the
        // `message` content is scrubbed to an opaque, ref-tagged string.
        let key = test_key();
        // A real (HMAC-valid) PAT so verification proceeds PAST the cheap
        // fast-reject and actually hits the (faulting) D1 lookup.
        let tenant_uuid = Uuid::from_u128(0xA24);
        let (plaintext, _pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0xBEEF)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let verifier = Arc::new(PatVerifier::new(Arc::new(BackendErrLookup), key));
        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,
            None,
            None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        );

        let resp = app
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:pull",
                Some(&basic(&pt)),
                None,
            ))
            .await
            .unwrap();
        // The exchange fails (backend fault) — but the wire body must be clean.
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&body_bytes);

        // Envelope SHAPE preserved (clients depend on it): { "errors": [...] }.
        let json: serde_json::Value =
            serde_json::from_slice(&body_bytes).expect("OCI error envelope must remain valid JSON");
        assert!(
            json.get("errors").and_then(|e| e.as_array()).is_some(),
            "OCI error envelope shape must be preserved"
        );

        // NO internal detail crosses the wire (the heart of A24).
        for needle in [
            "D1",
            "HTTP 500",
            "no such table",
            "SELECT",
            "FROM pat",
            "token_id",
            "errors\":[{\"code\":7500", // raw CF error object
            "backend",
        ] {
            assert!(
                !body.contains(needle),
                "A24: scrubbed /token body leaked internal detail {needle:?}; got: {body}"
            );
        }
        // It IS the opaque, code-keyed message + a correlation ref.
        assert!(
            body.contains("authentication failed") && body.contains("ref:"),
            "A24: expected opaque ref-tagged message, got: {body}"
        );
        // The status is the auth-failure shape (401), unchanged.
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn oci_bearer_tenant_resolves_only_a_verified_bearer() {
        // rt-nuclear #2/#8/#9: the $-ceiling cost-attribution tenant is recovered
        // by VERIFYING the HMAC bearer (not a request header the Worker strips,
        // and not an unverified token claim that would let one tenant bill
        // another).
        use corelink_adapter_host::oci::auth::{mint, OciScope};
        let key = SecretWrap::new("0123456789abcdef0123456789abcdef".to_owned());
        let tenant = TenantId::from_uuid(
            Uuid::parse_str("00000000-0000-4000-8000-0000000abcde").expect("uuid"),
        );
        let scope = OciScope::new("t/img", vec!["push".to_owned(), "pull".to_owned()]);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_secs();
        let token = mint(&key, &tenant, &scope, Some(0), now, 300).expect("mint");

        let mut h = axum::http::HeaderMap::new();
        h.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().expect("hv"),
        );
        // valid bearer → its tenant
        assert_eq!(
            oci_bearer_tenant(&key, &h).as_deref(),
            Some(tenant.to_canonical_text().as_str()),
            "a verified bearer must resolve to its tenant",
        );
        // forged bearer (wrong realm key) → None (cannot bill a victim)
        let attacker_key = SecretWrap::new("fedcba9876543210fedcba9876543210".to_owned());
        assert!(
            oci_bearer_tenant(&attacker_key, &h).is_none(),
            "a bearer that fails HMAC verify must NOT resolve a tenant",
        );
        // absent bearer → None (the data plane 401s the write; left uncharged)
        assert!(
            oci_bearer_tenant(&key, &axum::http::HeaderMap::new()).is_none(),
            "no Authorization header ⇒ no tenant",
        );
    }

    // ── rt-nuclear #8: OCI write metered against the monthly request-count cap ──

    /// In-memory [`RequestCountStore`](crate::request_count::RequestCountStore)
    /// for the OCI gate test; pre-seedable to drive the over-cap (429) path.
    #[derive(Debug, Default)]
    struct GateCounter(Mutex<HashMap<(String, String), i64>>);
    #[async_trait]
    impl crate::request_count::RequestCountStore for GateCounter {
        async fn increment(
            &self,
            tenant_id: &str,
            year_month: &str,
            _now_ms: i64,
        ) -> Result<i64, String> {
            let mut m = self.0.lock().unwrap();
            let c = m
                .entry((tenant_id.to_owned(), year_month.to_owned()))
                .or_insert(0);
            *c += 1;
            Ok(*c)
        }
    }

    /// Fixed-tier resolver for the OCI gate test.
    #[derive(Debug)]
    struct GateTier(&'static str);
    #[async_trait]
    impl crate::request_count::TierResolver for GateTier {
        async fn tier(&self, _tenant_id: &str) -> Result<String, String> {
            Ok(self.0.to_owned())
        }
    }

    #[tokio::test]
    async fn oci_write_over_request_count_cap_is_429_keyed_on_bearer_tenant() {
        // rt-nuclear #8 (request-count half): an OCI WRITE method is metered
        // against the tenant's monthly request-count cap (`monthly_request_counts`),
        // keyed on the SAME verified-HMAC-bearer tenant the $-ceiling gate uses
        // (#318). The Worker forwards OCI RAW and never counts these, so the
        // container must. Pre-seed the counter to the free cap so the write is
        // the (cap+1)-th request → the gate rejects it 429 BEFORE the adapter.
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xD00D);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0x5151)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

        // Pre-seed the request-count store at the free cap, keyed on the bearer
        // tenant's canonical UUID text + the CURRENT UTC month (the gate derives
        // the same bucket from the system clock).
        let store = Arc::new(GateCounter::default());
        let now_ms = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let tenant_text = TenantId::from_uuid(tenant_uuid).to_canonical_text();
        let year_month = crate::request_count::year_month_utc(now_ms);
        store.0.lock().unwrap().insert(
            (tenant_text, year_month),
            crate::request_count::CAP_FREE, // next op is the (cap+1)-th
        );

        let rc_gate = crate::request_count::RequestCountGate::new(
            store,
            Arc::new(GateTier("free")),
            Arc::new(crate::wall_clock::SystemWallClock::new()),
        );

        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,          // no $-ceiling gate in this test
            Some(rc_gate), // request-count gate under test
            None,          // cap resolver inert (StubCas)
        );

        // Exchange the PAT for a push,pull bearer.
        let resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:push,pull",
                Some(&basic(&pt)),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = json["token"].as_str().expect("token field").to_owned();

        // A WRITE (manifest PUT) with the valid bearer → over the cap → 429.
        // The gate runs as the outer layer, so it rejects BEFORE the adapter.
        let put = HttpRequest::builder()
            .method(Method::PUT)
            .uri("/v2/alpine/manifests/latest")
            .header("authorization", format!("Bearer {bearer}"))
            .header("content-type", "application/vnd.oci.image.manifest.v1+json")
            .body(Body::from(r#"{"schemaVersion":2}"#))
            .unwrap();
        let resp = app.clone().oneshot(put).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an OCI write over the monthly request-count cap must be 429"
        );
        assert!(
            resp.headers().contains_key(axum::http::header::RETRY_AFTER),
            "a 429 must carry Retry-After (next-month-start)"
        );

        // A READ (GET) over the cap is ALSO 429 now (rt-nuclear r34 #1/#11):
        // `docker pull` is billable work, so it is metered on the request-count
        // axis exactly like a write. (Pre-seed left the counter AT the cap; the
        // earlier write was rejected 402-style/429 before incrementing, so the
        // GET is still the (cap+1)-th countable op → over the cap → 429.)
        let get = req(
            Method::GET,
            "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
            Some(&format!("Bearer {bearer}")),
            None,
        );
        let resp = app.oneshot(get).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "an OCI read over the monthly request-count cap must be 429 (reads are now metered)"
        );
    }

    #[tokio::test]
    async fn oci_read_increments_request_count_and_write_still_counts() {
        // rt-nuclear r34 #1/#11 regression: an OCI READ (GET/HEAD) with a valid
        // bearer increments the monthly request-count gate (closing the
        // `docker pull` → unmetered exploit), and a WRITE still charges the
        // request-count axis, both keyed on the verified-HMAC-bearer tenant. We
        // observe the in-memory store directly to prove each increment. (The
        // write-only $-ceiling axis is covered by
        // `oci_write_over_request_count_cap_is_429_keyed_on_bearer_tenant` and the
        // $-ceiling 402 tests; this edit leaves the write $-path untouched.)
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xBEEF);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0x7171)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        let lookup = OneTokenLookup {
            token_id: pat.token_id.as_str().to_owned(),
            row: PatRow {
                tenant_id: pat.tenant_id.0.to_string(),
                pat_hash: pat.hash.as_str().to_owned(),
                scope: SCOPE_RW.to_owned(),
            },
        };
        let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

        // Fresh store (counter starts at 0, well under the cap).
        let store = Arc::new(GateCounter::default());
        let now_ms = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let tenant_text = TenantId::from_uuid(tenant_uuid).to_canonical_text();
        let year_month = crate::request_count::year_month_utc(now_ms);
        let bucket = (tenant_text.clone(), year_month.clone());

        let rc_gate = crate::request_count::RequestCountGate::new(
            Arc::clone(&store) as Arc<dyn crate::request_count::RequestCountStore>,
            Arc::new(GateTier("free")),
            Arc::new(crate::wall_clock::SystemWallClock::new()),
        );

        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            Arc::new(OciKvFake::default()),
            verifier,
            SecretWrap::new(OCI_KEY.to_owned()),
            None,          // no $-ceiling gate in this test (write $-path covered elsewhere)
            Some(rc_gate), // request-count gate under test (now charged on reads too)
            None,          // cap resolver inert (StubCas)
        );

        // Exchange the PAT for a push,pull bearer.
        let resp = app
            .clone()
            .oneshot(req(
                Method::GET,
                "/token?scope=repository:alpine:push,pull",
                Some(&basic(&pt)),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bearer = json["token"].as_str().expect("token field").to_owned();

        let count = |b: &(String, String)| -> i64 {
            store.0.lock().unwrap().get(b).copied().unwrap_or(0)
        };
        assert_eq!(count(&bucket), 0, "no countable op yet");

        // A READ (GET blob) with the valid bearer → reaches the adapter (404 for
        // the absent blob) AND increments the request-count gate.
        let get = req(
            Method::GET,
            "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
            Some(&format!("Bearer {bearer}")),
            None,
        );
        let resp = app.clone().oneshot(get).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "the read is authorized + reaches the adapter (absent blob ⇒ 404)"
        );
        assert_eq!(
            count(&bucket),
            1,
            "an OCI read with a valid bearer MUST increment the request-count gate"
        );

        // A WRITE (manifest PUT) → charges the request-count axis again (now 2),
        // confirming writes are still metered after the read-metering change.
        let put = HttpRequest::builder()
            .method(Method::PUT)
            .uri("/v2/alpine/manifests/latest")
            .header("authorization", format!("Bearer {bearer}"))
            .header("content-type", "application/vnd.oci.image.manifest.v1+json")
            .body(Body::from(r#"{"schemaVersion":2}"#))
            .unwrap();
        let resp = app.oneshot(put).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "under the cap, the write is not 429'd"
        );
        assert_eq!(
            count(&bucket),
            2,
            "an OCI write MUST still increment the request-count gate"
        );
    }
}
