//! [`InMemoryMultipartAdapter`] — host-side fake honouring every
//! load-bearing semantic the production R2 adapter inherits.
//!
//! Per F-001 closure (2026-05-01) every adapter instance owns its
//! state — no globals. The fake is `Send + Sync` (the inner state
//! lives behind a single `Mutex<HashMap<…>>`); the per-tenant
//! semaphore lives in [`crate::concurrency::PerTenantSemaphore`]
//! which is also instance-scoped.

use core::time::Duration;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::SystemTime;

use bytes::Bytes;
use sha2::Digest as ShaDigest;
use sha2::Sha256;
use uuid::Uuid;

use crate::adapter::{InitiateRequest, MultipartAdapter};
use crate::bounds;
use crate::concurrency::PerTenantSemaphore;
use crate::error::MultipartError;
use crate::object_key;
use crate::types::{
    Bucket, CompletedObject, MultipartUpload, OrphanedUpload, PartETag, PartNumber, SessionState,
};

/// Internal session row tracked by the fake.
#[derive(Clone, Debug)]
struct Session {
    upload_id: String,
    tenant_id: Uuid,
    bucket: Bucket,
    object_key: String,
    initiated_at: SystemTime,
    state: SessionState,
    /// Parts indexed by part_number → (etag, bytes). BTreeMap so
    /// `complete` can walk part numbers in canonical ascending order.
    parts: BTreeMap<u32, RecordedPart>,
    /// Cached completion result; populated on first successful
    /// `complete` call so re-invocation is a pure idempotent read.
    completed: Option<CompletedObject>,
}

#[derive(Clone, Debug)]
struct RecordedPart {
    etag: PartETag,
    size_bytes: u64,
}

/// In-memory R2 multipart fake. Per-instance state — no globals.
pub struct InMemoryMultipartAdapter {
    sessions: Mutex<HashMap<String, Session>>,
    semaphore: PerTenantSemaphore,
    /// Optional override for the upload-id mint — wired by tests
    /// that need deterministic upload ids. None ⇒ UUIDv4.
    id_seed: Mutex<Option<u64>>,
}

impl Default for InMemoryMultipartAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for InMemoryMultipartAdapter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryMultipartAdapter")
            .field("limit", &self.semaphore.limit())
            .finish_non_exhaustive()
    }
}

impl InMemoryMultipartAdapter {
    /// Construct a fresh in-memory adapter with the canonical
    /// per-tenant concurrency budget.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            semaphore: PerTenantSemaphore::default(),
            id_seed: Mutex::new(None),
        }
    }

    /// Construct a fake with a custom per-tenant concurrency budget.
    #[must_use]
    pub fn with_concurrency(limit: usize) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            semaphore: PerTenantSemaphore::new(limit),
            id_seed: Mutex::new(None),
        }
    }

    /// Pin a deterministic upload-id seed. Subsequent `initiate`
    /// calls mint upload-ids of the form `mp-fake-<incrementing-counter>`
    /// so test assertions can be byte-stable across runs.
    pub fn set_id_seed(&self, seed: u64) {
        if let Ok(mut g) = self.id_seed.lock() {
            *g = Some(seed);
        }
    }

    /// Borrow the per-tenant concurrency manager (test-only
    /// helper for property tests + chaos suite that need to query
    /// the limit or trip the `try_acquire` path directly).
    #[must_use]
    pub fn semaphore(&self) -> &PerTenantSemaphore {
        &self.semaphore
    }

    /// Number of sessions currently tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock_sessions().map(|g| g.len()).unwrap_or(0)
    }

    /// Whether the fake is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot of every session in the fake — test-only helper.
    #[must_use]
    pub fn sessions_snapshot(&self) -> Vec<MultipartUpload> {
        let g = match self.lock_sessions() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        g.values()
            .map(|s| {
                MultipartUpload::new(
                    s.upload_id.clone(),
                    s.tenant_id,
                    s.bucket,
                    s.object_key.clone(),
                    s.initiated_at,
                )
            })
            .collect()
    }

    /// Snapshot the `(part_number, etag)` pairs recorded for an
    /// upload — test-only helper used by integration tests to
    /// assert ETag tracking ordering.
    #[must_use]
    pub fn parts_snapshot(&self, upload_id: &str) -> Vec<(u32, PartETag)> {
        let g = match self.lock_sessions() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        g.get(upload_id)
            .map(|s| {
                s.parts
                    .iter()
                    .map(|(n, p)| (*n, p.etag.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Inspect the session state for a given upload — test-only
    /// helper consumed by chaos tests that need to assert state
    /// transitions.
    #[must_use]
    pub fn session_state(&self, upload_id: &str) -> Option<SessionState> {
        let g = self.lock_sessions().ok()?;
        g.get(upload_id).map(|s| s.state)
    }

    fn lock_sessions(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, Session>>, MultipartError> {
        self.sessions
            .lock()
            .map_err(|_| MultipartError::Backend("session map mutex poisoned".to_string()))
    }

    fn mint_upload_id(&self) -> String {
        if let Ok(mut g) = self.id_seed.lock() {
            if let Some(seed) = g.as_mut() {
                let n = *seed;
                *seed = seed.wrapping_add(1);
                return format!("mp-fake-{n:016x}");
            }
        }
        // Production-shaped opaque id — UUIDv4 hex.
        let id = Uuid::new_v4();
        format!("mp-{}", id.simple())
    }

    fn check_session_tenant(
        session: &Session,
        tenant_id: Uuid,
        upload_id: &str,
    ) -> Result<(), MultipartError> {
        if session.tenant_id != tenant_id {
            return Err(MultipartError::CrossTenantUpload {
                upload_id: upload_id.to_string(),
            });
        }
        Ok(())
    }
}

impl MultipartAdapter for InMemoryMultipartAdapter {
    async fn initiate(
        &self,
        request: InitiateRequest<'_>,
    ) -> Result<MultipartUpload, MultipartError> {
        let object_key = object_key::compose(
            request.bucket,
            request.region,
            request.tenant_prefix,
            request.digest_hex,
            request.suffix,
        )?;
        let mut g = self.lock_sessions()?;

        // Idempotent re-initiate: scan for an InProgress session
        // bound to (tenant_id, object_key). Returns the existing
        // handle so the client can resume.
        if let Some(existing) = g.values().find(|s| {
            s.tenant_id == request.tenant_id
                && s.object_key == object_key
                && s.state == SessionState::InProgress
        }) {
            return Ok(MultipartUpload::new(
                existing.upload_id.clone(),
                existing.tenant_id,
                existing.bucket,
                existing.object_key.clone(),
                existing.initiated_at,
            ));
        }

        let upload_id = self.mint_upload_id();
        let initiated_at = SystemTime::now();
        let session = Session {
            upload_id: upload_id.clone(),
            tenant_id: request.tenant_id,
            bucket: request.bucket,
            object_key: object_key.clone(),
            initiated_at,
            state: SessionState::InProgress,
            parts: BTreeMap::new(),
            completed: None,
        };
        g.insert(upload_id.clone(), session);
        Ok(MultipartUpload::new(
            upload_id,
            request.tenant_id,
            request.bucket,
            object_key,
            initiated_at,
        ))
    }

    async fn upload_part(
        &self,
        tenant_id: Uuid,
        upload: &MultipartUpload,
        part_number: PartNumber,
        bytes: Bytes,
    ) -> Result<PartETag, MultipartError> {
        // Acquire the per-tenant permit; released on guard drop at
        // end of scope.
        let _permit = self.semaphore.acquire(tenant_id).await?;

        let size = u64::try_from(bytes.len()).map_err(|_| MultipartError::PartTooLarge {
            part_number: part_number.get(),
            size_bytes: u64::MAX,
            limit_bytes: bounds::R2_MAX_PART_SIZE_BYTES,
        })?;
        if size > bounds::R2_MAX_PART_SIZE_BYTES {
            return Err(MultipartError::PartTooLarge {
                part_number: part_number.get(),
                size_bytes: size,
                limit_bytes: bounds::R2_MAX_PART_SIZE_BYTES,
            });
        }

        // Compute the ETag — production R2 returns an MD5 hex of
        // the part body; we use SHA-256-prefix-32-hex here so the
        // fake stays free of the `md-5` crate dependency. The
        // canonical handler treats the ETag as opaque, so the
        // hash function doesn't matter for the contract — what
        // matters is determinism.
        let etag = part_etag(&bytes);

        let mut g = self.lock_sessions()?;
        let session = g
            .get_mut(&upload.upload_id)
            .ok_or_else(|| MultipartError::UploadIdNotFound {
                upload_id: upload.upload_id.clone(),
            })?;
        Self::check_session_tenant(session, tenant_id, &upload.upload_id)?;
        if session.state != SessionState::InProgress {
            return Err(MultipartError::UploadIdNotFound {
                upload_id: upload.upload_id.clone(),
            });
        }
        session.parts.insert(
            part_number.get(),
            RecordedPart {
                etag: PartETag::new(etag.clone()),
                size_bytes: size,
            },
        );
        Ok(PartETag::new(etag))
    }

    async fn complete(
        &self,
        tenant_id: Uuid,
        upload: &MultipartUpload,
        parts: Vec<(PartNumber, PartETag)>,
    ) -> Result<CompletedObject, MultipartError> {
        // Pre-flight checks BEFORE locking the map so we don't
        // hold the lock across error returns.
        if parts.is_empty() {
            return Err(MultipartError::EmptyPartList {
                upload_id: upload.upload_id.clone(),
            });
        }
        // Detect duplicates structurally — two parts with the same
        // number is a wire-shape bug (silent shadowing).
        let mut seen = HashSet::new();
        for (pn, _) in &parts {
            if !seen.insert(pn.get()) {
                return Err(MultipartError::DuplicatePartNumber {
                    part_number: pn.get(),
                });
            }
        }

        let mut g = self.lock_sessions()?;
        let session = g
            .get_mut(&upload.upload_id)
            .ok_or_else(|| MultipartError::UploadIdNotFound {
                upload_id: upload.upload_id.clone(),
            })?;
        Self::check_session_tenant(session, tenant_id, &upload.upload_id)?;

        // Idempotent re-complete: return the cached result.
        if session.state == SessionState::Completed {
            if let Some(cached) = &session.completed {
                return Ok(cached.clone());
            }
            // Should never happen — Completed implies cached. Fall
            // through to backend error.
            return Err(MultipartError::Backend(
                "completed session has no cached object".to_string(),
            ));
        }

        if session.state == SessionState::Aborted {
            return Err(MultipartError::UploadIdNotFound {
                upload_id: upload.upload_id.clone(),
            });
        }

        // Validate every submitted part exists with matching ETag.
        let mut total_bytes: u64 = 0;
        for (pn, supplied) in &parts {
            let recorded = session.parts.get(&pn.get()).ok_or_else(|| {
                MultipartError::PartMissing {
                    part_number: pn.get(),
                    expected: "<not uploaded>".to_string(),
                    actual: supplied.as_str().to_string(),
                }
            })?;
            if recorded.etag != *supplied {
                return Err(MultipartError::PartMissing {
                    part_number: pn.get(),
                    expected: recorded.etag.as_str().to_string(),
                    actual: supplied.as_str().to_string(),
                });
            }
            total_bytes = total_bytes.saturating_add(recorded.size_bytes);
        }

        // Compose the canonical S3 multipart ETag — concat ETags
        // → SHA-256 → first-16-bytes hex → "-<part_count>". We use
        // SHA-256 here for the same dependency-minimisation reason
        // as `part_etag`.
        let mut hasher = Sha256::new();
        for (pn, _) in &parts {
            if let Some(recorded) = session.parts.get(&pn.get()) {
                hasher.update(recorded.etag.as_str().as_bytes());
            }
        }
        let digest = hasher.finalize();
        let prefix_slice = digest.get(..16).unwrap_or(&[]);
        let composite = format!("{:032x}-{}", as_u128_be(prefix_slice), parts.len());

        let object = CompletedObject {
            bucket: session.bucket,
            object_key: session.object_key.clone(),
            etag: composite,
            size_bytes: total_bytes,
        };

        session.state = SessionState::Completed;
        session.completed = Some(object.clone());
        Ok(object)
    }

    async fn abort(
        &self,
        tenant_id: Uuid,
        upload: &MultipartUpload,
    ) -> Result<(), MultipartError> {
        let mut g = self.lock_sessions()?;
        let session = g
            .get_mut(&upload.upload_id)
            .ok_or_else(|| MultipartError::UploadIdNotFound {
                upload_id: upload.upload_id.clone(),
            })?;
        Self::check_session_tenant(session, tenant_id, &upload.upload_id)?;
        match session.state {
            SessionState::InProgress | SessionState::Aborted => {
                session.state = SessionState::Aborted;
                session.parts.clear();
                Ok(())
            }
            SessionState::Completed => {
                // INV-MULTIPART-FINALIZE-IRREVOCABLE: completion is
                // sealed; abort is rejected as session-not-found.
                Err(MultipartError::UploadIdNotFound {
                    upload_id: upload.upload_id.clone(),
                })
            }
        }
    }

    async fn list_orphans(
        &self,
        bucket: Bucket,
        now: SystemTime,
        max_age: Duration,
    ) -> Result<Vec<OrphanedUpload>, MultipartError> {
        let g = self.lock_sessions()?;
        let mut out = Vec::new();
        for session in g.values() {
            if session.bucket != bucket {
                continue;
            }
            if session.state != SessionState::InProgress {
                continue;
            }
            let age = match now.duration_since(session.initiated_at) {
                Ok(d) => d,
                // initiated_at is in the future relative to `now`
                // (clock skew); treat as zero age — not orphaned.
                Err(_) => continue,
            };
            if age > max_age {
                out.push(OrphanedUpload {
                    upload_id: session.upload_id.clone(),
                    tenant_id: session.tenant_id,
                    object_key: session.object_key.clone(),
                    initiated_at: session.initiated_at,
                    age,
                });
            }
        }
        // Stable order: by initiated_at ascending — the oldest
        // sessions surface first so the sweeper aborts them in
        // canonical order.
        out.sort_by_key(|o| o.initiated_at);
        Ok(out)
    }
}

/// Compute the per-part ETag — opaque hex string. Production
/// R2 returns an MD5 hex; this fake uses SHA-256 prefix-32-hex to
/// avoid pulling `md-5` into the workspace. The canonical handler
/// is opaque to the algorithm.
fn part_etag(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex::encode(digest.get(..16).unwrap_or(&[]))
}

/// Read up to a 16-byte slice as a u128 big-endian. Used by the
/// composite ETag formatter to render a 32-char hex.
fn as_u128_be(bytes: &[u8]) -> u128 {
    let mut buf = [0u8; 16];
    let n = bytes.len().min(16);
    if n > 0 {
        let src = match bytes.get(..n) {
            Some(s) => s,
            None => return 0,
        };
        let dst = match buf.get_mut(..n) {
            Some(d) => d,
            None => return 0,
        };
        dst.copy_from_slice(src);
    }
    u128::from_be_bytes(buf)
}

// ----------------------------------------------------------------------
// Always-failing fake — used by chaos tests to exercise the
// `MultipartError::Backend` propagation path.
// ----------------------------------------------------------------------

/// Adapter that fails every call with [`MultipartError::Backend`].
/// Lives in src/ (not just tests/) so multiple test files can
/// reuse it.
#[derive(Debug)]
pub struct AlwaysFailingMultipartAdapter {
    /// Diagnostic string surfaced verbatim in the error.
    pub diagnostic: String,
}

impl AlwaysFailingMultipartAdapter {
    /// Construct a failing adapter with a stable diagnostic.
    #[must_use]
    pub fn new(diagnostic: impl Into<String>) -> Self {
        Self {
            diagnostic: diagnostic.into(),
        }
    }
}

impl MultipartAdapter for AlwaysFailingMultipartAdapter {
    async fn initiate(
        &self,
        _request: InitiateRequest<'_>,
    ) -> Result<MultipartUpload, MultipartError> {
        Err(MultipartError::Backend(self.diagnostic.clone()))
    }

    async fn upload_part(
        &self,
        _tenant_id: Uuid,
        _upload: &MultipartUpload,
        _part_number: PartNumber,
        _bytes: Bytes,
    ) -> Result<PartETag, MultipartError> {
        Err(MultipartError::Backend(self.diagnostic.clone()))
    }

    async fn complete(
        &self,
        _tenant_id: Uuid,
        _upload: &MultipartUpload,
        _parts: Vec<(PartNumber, PartETag)>,
    ) -> Result<CompletedObject, MultipartError> {
        Err(MultipartError::Backend(self.diagnostic.clone()))
    }

    async fn abort(
        &self,
        _tenant_id: Uuid,
        _upload: &MultipartUpload,
    ) -> Result<(), MultipartError> {
        Err(MultipartError::Backend(self.diagnostic.clone()))
    }

    async fn list_orphans(
        &self,
        _bucket: Bucket,
        _now: SystemTime,
        _max_age: Duration,
    ) -> Result<Vec<OrphanedUpload>, MultipartError> {
        Err(MultipartError::Backend(self.diagnostic.clone()))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;
    use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
    use zeroize::Zeroizing;

    fn fixed_prefix(tenant: Uuid) -> corelink_tenant_path::TenantPrefix {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
        derive_prefix(&tdk, tenant)
    }

    fn dummy_digest() -> String {
        "0123456789abcdef".repeat(4)
    }

    fn initiate_req<'a>(
        tenant: Uuid,
        prefix: &'a corelink_tenant_path::TenantPrefix,
        digest_hex: &'a str,
    ) -> InitiateRequest<'a> {
        InitiateRequest::new(tenant, prefix, Bucket::Chunk, "sam", digest_hex)
    }

    #[tokio::test]
    async fn initiate_happy_path() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        assert_eq!(u.tenant_id, t);
        assert_eq!(u.bucket, Bucket::Chunk);
        assert!(u.object_key.starts_with("chunk-sam/"));
        assert!(!u.upload_id.is_empty());
    }

    #[tokio::test]
    async fn initiate_idempotent_returns_same_upload_id() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u1 = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let u2 = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        assert_eq!(u1.upload_id, u2.upload_id);
    }

    #[tokio::test]
    async fn initiate_distinct_tenants_distinct_sessions() {
        let a = InMemoryMultipartAdapter::new();
        let t_a = Uuid::from_u128(1);
        let t_b = Uuid::from_u128(2);
        let p_a = fixed_prefix(t_a);
        let p_b = fixed_prefix(t_b);
        let dh = dummy_digest();
        let u_a = a.initiate(initiate_req(t_a, &p_a, &dh)).await.unwrap();
        let u_b = a.initiate(initiate_req(t_b, &p_b, &dh)).await.unwrap();
        assert_ne!(u_a.upload_id, u_b.upload_id);
        assert_ne!(u_a.object_key, u_b.object_key);
    }

    #[tokio::test]
    async fn upload_part_records_etag() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let body = Bytes::from_static(b"part-1-bytes");
        let etag = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), body)
            .await
            .unwrap();
        let parts = a.parts_snapshot(&u.upload_id);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].0, 1);
        assert_eq!(parts[0].1, etag);
    }

    #[tokio::test]
    async fn upload_part_idempotent_on_same_bytes() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let body = Bytes::from_static(b"hello");
        let e1 = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), body.clone())
            .await
            .unwrap();
        let e2 = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), body)
            .await
            .unwrap();
        assert_eq!(e1, e2);
    }

    #[tokio::test]
    async fn upload_part_cross_tenant_rejected() {
        let a = InMemoryMultipartAdapter::new();
        let t_a = Uuid::from_u128(1);
        let t_b = Uuid::from_u128(2);
        let p_a = fixed_prefix(t_a);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t_a, &p_a, &dh)).await.unwrap();
        let r = a
            .upload_part(t_b, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"x"))
            .await;
        assert!(matches!(r, Err(MultipartError::CrossTenantUpload { .. })));
    }

    #[tokio::test]
    async fn complete_happy_path() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let e1 = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"a"))
            .await
            .unwrap();
        let e2 = a
            .upload_part(t, &u, PartNumber::new(2).unwrap(), Bytes::from_static(b"b"))
            .await
            .unwrap();
        let parts = vec![
            (PartNumber::new(1).unwrap(), e1),
            (PartNumber::new(2).unwrap(), e2),
        ];
        let obj = a.complete(t, &u, parts).await.unwrap();
        assert_eq!(obj.bucket, Bucket::Chunk);
        assert!(obj.etag.contains('-'));
    }

    #[tokio::test]
    async fn complete_idempotent_returns_cached() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let e1 = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"a"))
            .await
            .unwrap();
        let parts = vec![(PartNumber::new(1).unwrap(), e1)];
        let o1 = a.complete(t, &u, parts.clone()).await.unwrap();
        let o2 = a.complete(t, &u, parts).await.unwrap();
        assert_eq!(o1, o2);
    }

    #[tokio::test]
    async fn complete_rejects_empty_parts() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let r = a.complete(t, &u, Vec::new()).await;
        assert!(matches!(r, Err(MultipartError::EmptyPartList { .. })));
    }

    #[tokio::test]
    async fn complete_rejects_duplicate_part_number() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let e1 = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"a"))
            .await
            .unwrap();
        let parts = vec![
            (PartNumber::new(1).unwrap(), e1.clone()),
            (PartNumber::new(1).unwrap(), e1),
        ];
        let r = a.complete(t, &u, parts).await;
        assert!(matches!(
            r,
            Err(MultipartError::DuplicatePartNumber { part_number: 1 })
        ));
    }

    #[tokio::test]
    async fn complete_rejects_missing_part() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        // Don't upload part 1; submit it anyway.
        let parts = vec![(PartNumber::new(1).unwrap(), PartETag::new("fake"))];
        let r = a.complete(t, &u, parts).await;
        assert!(matches!(r, Err(MultipartError::PartMissing { part_number: 1, .. })));
    }

    #[tokio::test]
    async fn complete_rejects_etag_mismatch() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let _e = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"a"))
            .await
            .unwrap();
        let parts = vec![(PartNumber::new(1).unwrap(), PartETag::new("wrong"))];
        let r = a.complete(t, &u, parts).await;
        assert!(matches!(r, Err(MultipartError::PartMissing { part_number: 1, .. })));
    }

    #[tokio::test]
    async fn abort_happy_path_idempotent() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        a.abort(t, &u).await.unwrap();
        // 2nd abort is a no-op.
        a.abort(t, &u).await.unwrap();
        // upload_part on aborted session yields UploadIdNotFound.
        let r = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"a"))
            .await;
        assert!(matches!(r, Err(MultipartError::UploadIdNotFound { .. })));
    }

    #[tokio::test]
    async fn abort_rejects_after_complete() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        let e1 = a
            .upload_part(t, &u, PartNumber::new(1).unwrap(), Bytes::from_static(b"a"))
            .await
            .unwrap();
        let _ = a
            .complete(t, &u, vec![(PartNumber::new(1).unwrap(), e1)])
            .await
            .unwrap();
        let r = a.abort(t, &u).await;
        assert!(matches!(r, Err(MultipartError::UploadIdNotFound { .. })));
    }

    #[tokio::test]
    async fn list_orphans_returns_only_in_progress_older_than_max_age() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        // initiated_at is `now-ish`; advance the sweeper's `now`
        // by 8 days so the session is older than 7d.
        let future_now = u.initiated_at + Duration::from_secs(8 * 86_400);
        let orphans = a
            .list_orphans(Bucket::Chunk, future_now, Duration::from_secs(7 * 86_400))
            .await
            .unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].upload_id, u.upload_id);

        // Aborted sessions don't surface.
        a.abort(t, &u).await.unwrap();
        let orphans = a
            .list_orphans(Bucket::Chunk, future_now, Duration::from_secs(7 * 86_400))
            .await
            .unwrap();
        assert!(orphans.is_empty());
    }

    #[tokio::test]
    async fn list_orphans_filters_by_bucket() {
        let a = InMemoryMultipartAdapter::new();
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let r_chunk = InitiateRequest::new(t, &p, Bucket::Chunk, "sam", &dh);
        let r_manifest = InitiateRequest::new(t, &p, Bucket::Manifest, "sam", &dh);
        let u_c = a.initiate(r_chunk).await.unwrap();
        let _u_m = a.initiate(r_manifest).await.unwrap();
        let future_now = u_c.initiated_at + Duration::from_secs(10 * 86_400);
        let orph_c = a
            .list_orphans(Bucket::Chunk, future_now, Duration::from_secs(7 * 86_400))
            .await
            .unwrap();
        let orph_m = a
            .list_orphans(Bucket::Manifest, future_now, Duration::from_secs(7 * 86_400))
            .await
            .unwrap();
        assert_eq!(orph_c.len(), 1);
        assert_eq!(orph_m.len(), 1);
        assert_ne!(orph_c[0].upload_id, orph_m[0].upload_id);
    }

    #[tokio::test]
    async fn always_failing_adapter_propagates_backend_error() {
        let a = AlwaysFailingMultipartAdapter::new("R2 5xx");
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let r = a.initiate(initiate_req(t, &p, &dh)).await;
        assert!(matches!(r, Err(MultipartError::Backend(s)) if s == "R2 5xx"));
    }

    #[tokio::test]
    async fn deterministic_id_seed_yields_stable_ids() {
        let a = InMemoryMultipartAdapter::new();
        a.set_id_seed(0x42);
        let t = Uuid::nil();
        let p = fixed_prefix(t);
        let dh = dummy_digest();
        let u1 = a.initiate(initiate_req(t, &p, &dh)).await.unwrap();
        // Re-initiate is idempotent; we need a fresh fingerprint
        // to mint a NEW id — switch the digest hex.
        let dh2 = "f".repeat(64);
        let u2 = a.initiate(initiate_req(t, &p, &dh2)).await.unwrap();
        assert_eq!(u1.upload_id, "mp-fake-0000000000000042");
        assert_eq!(u2.upload_id, "mp-fake-0000000000000043");
    }

    #[test]
    fn part_etag_is_deterministic() {
        let a = part_etag(b"hello");
        let b = part_etag(b"hello");
        assert_eq!(a, b);
        let c = part_etag(b"world");
        assert_ne!(a, c);
        assert_eq!(a.len(), 32);
    }
}
