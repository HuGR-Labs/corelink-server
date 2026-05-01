//! [`MultipartAdapter`] trait — the integration boundary between the
//! R2 multipart pipeline and the rest of the worker.
//!
//! ## Trait-abstraction-defer pattern (charter §autonomous decision tree)
//!
//! The crate ships:
//! 1. **This trait surface** — the API every consumer codes against.
//! 2. **`InMemoryMultipartAdapter`** — host-side fake honouring every
//!    documented semantic; what the property tests exercise.
//!
//! The production `aws-sdk-s3` Cloudflare R2 binding shim lands in
//! WI-S05-006 (alongside the conformance suite); until then the
//! handler depends on this trait surface only and the in-memory
//! fake is the canonical reference impl.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore canonical pattern"
)]

use core::future::Future;
use core::time::Duration;
use std::time::SystemTime;

use bytes::Bytes;
use corelink_tenant_path::TenantPrefix;
use uuid::Uuid;

use crate::error::MultipartError;
use crate::types::{
    Bucket, CompletedObject, MultipartUpload, OrphanedUpload, PartETag, PartNumber,
};

/// Initiate-multipart-upload request shape — packed so additional
/// invariants (e.g. content-type, custom metadata) can be added
/// post-v1 without breaking call sites.
///
/// `#[non_exhaustive]` for the same reason as its sibling structs.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct InitiateRequest<'a> {
    /// Tenant the upload is bound to.
    pub tenant_id: Uuid,
    /// Tenant prefix (used by the canonical object-key composer).
    pub tenant_prefix: &'a TenantPrefix,
    /// Bucket family.
    pub bucket: Bucket,
    /// Lower-case ASCII region literal (e.g. `"sam"`, `"iad"`).
    pub region: &'a str,
    /// Content digest in canonical 64-char lower hex.
    pub digest_hex: &'a str,
    /// Optional suffix (e.g. `Some(".json")` for manifests).
    pub suffix: Option<&'a str>,
}

impl<'a> InitiateRequest<'a> {
    /// Convenience builder.
    #[must_use]
    pub fn new(
        tenant_id: Uuid,
        tenant_prefix: &'a TenantPrefix,
        bucket: Bucket,
        region: &'a str,
        digest_hex: &'a str,
    ) -> Self {
        Self {
            tenant_id,
            tenant_prefix,
            bucket,
            region,
            digest_hex,
            suffix: None,
        }
    }

    /// Override the optional suffix (e.g. `.json` for manifests).
    #[must_use]
    pub fn with_suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = Some(suffix);
        self
    }
}

/// Trait surface for the R2 multipart adapter.
///
/// Every method takes `tenant_id` explicitly so the cross-tenant
/// guard can fire without trusting any client-supplied path
/// component. The `MultipartUpload` handle threaded through
/// `upload_part` / `complete` / `abort` carries its own bound
/// `tenant_id`; the adapter rejects calls whose explicit `tenant_id`
/// disagrees with the handle's binding via
/// [`MultipartError::CrossTenantUpload`].
pub trait MultipartAdapter: Send + Sync {
    /// Initiate a multipart upload session.
    ///
    /// Idempotent on `(tenant_id, object_key)` while a session is
    /// `InProgress`: a re-call returns the existing
    /// [`MultipartUpload`] handle (same `upload_id`) so the client
    /// can pick up where it left off without orphaning the
    /// previous attempt.
    ///
    /// # Errors
    ///
    /// - [`MultipartError::InvalidObjectKey`] if the request shape
    ///   is malformed.
    /// - [`MultipartError::Backend`] for transport failures.
    fn initiate<'a>(
        &'a self,
        request: InitiateRequest<'a>,
    ) -> impl Future<Output = Result<MultipartUpload, MultipartError>> + Send + 'a;

    /// Upload a single part. Returns the part ETag the adapter
    /// recorded — the canonical handler stores the
    /// `(part_number, etag)` pair and submits them ordered to
    /// `complete`.
    ///
    /// `part_number` is bounded to `1..=10_000`
    /// ([`PartNumber::new`]). The adapter awaits a per-tenant
    /// concurrency permit before issuing the underlying R2 PUT.
    ///
    /// **Idempotency**: re-issuing the same `(upload_id,
    /// part_number, bytes)` triple is a no-op on the part list
    /// (the existing ETag is returned). Re-issuing the same
    /// `(upload_id, part_number)` with **different** bytes
    /// overwrites the previously-recorded part — the canonical R2
    /// semantic, mirrored here.
    ///
    /// # Errors
    ///
    /// - [`MultipartError::UploadIdNotFound`] if the session is
    ///   missing or expired.
    /// - [`MultipartError::CrossTenantUpload`] on tenant mismatch.
    /// - [`MultipartError::PartTooLarge`] if `bytes.len()` exceeds
    ///   the R2 hard cap.
    /// - [`MultipartError::Backend`] for transport failures.
    fn upload_part<'a>(
        &'a self,
        tenant_id: Uuid,
        upload: &'a MultipartUpload,
        part_number: PartNumber,
        bytes: Bytes,
    ) -> impl Future<Output = Result<PartETag, MultipartError>> + Send + 'a;

    /// Complete a multipart upload by submitting an ordered list
    /// of `(part_number, etag)` pairs. Atomic — all parts are
    /// committed or none.
    ///
    /// Idempotent on the same `upload_id`: a re-call after the
    /// session is already `Completed` returns the cached
    /// [`CompletedObject`] without re-issuing the R2 commit.
    ///
    /// # Errors
    ///
    /// - [`MultipartError::UploadIdNotFound`] / `CrossTenantUpload`.
    /// - [`MultipartError::EmptyPartList`] if `parts` is empty.
    /// - [`MultipartError::DuplicatePartNumber`] if `parts`
    ///   contains the same number twice.
    /// - [`MultipartError::PartMissing`] if a referenced part has
    ///   not been uploaded or its ETag mismatches.
    fn complete<'a>(
        &'a self,
        tenant_id: Uuid,
        upload: &'a MultipartUpload,
        parts: Vec<(PartNumber, PartETag)>,
    ) -> impl Future<Output = Result<CompletedObject, MultipartError>> + Send + 'a;

    /// Abort a multipart upload. Cleans up uploaded parts.
    ///
    /// Idempotent on the same `upload_id`: a re-call after the
    /// session is already `Aborted` returns `Ok(())`. Calling
    /// abort on a `Completed` session is **rejected** —
    /// completion is irrevocable per
    /// `INV-MULTIPART-FINALIZE-IRREVOCABLE`.
    ///
    /// # Errors
    ///
    /// - [`MultipartError::UploadIdNotFound`] / `CrossTenantUpload`.
    /// - [`MultipartError::Backend`] if the abort cannot be
    ///   committed.
    fn abort<'a>(
        &'a self,
        tenant_id: Uuid,
        upload: &'a MultipartUpload,
    ) -> impl Future<Output = Result<(), MultipartError>> + Send + 'a;

    /// Enumerate orphaned multipart sessions older than `max_age`.
    /// Consumed by the WI-S05-006 sweeper to abort sessions with
    /// no activity inside the canonical 7-day window.
    ///
    /// `now` is supplied explicitly by the sweeper so the call is
    /// deterministic under test (no implicit `SystemTime::now()`
    /// dependency).
    ///
    /// # Errors
    ///
    /// - [`MultipartError::Backend`] for transport failures.
    fn list_orphans<'a>(
        &'a self,
        bucket: Bucket,
        now: SystemTime,
        max_age: Duration,
    ) -> impl Future<Output = Result<Vec<OrphanedUpload>, MultipartError>> + Send + 'a;
}
