//! Canonical value types crossing the [`crate::adapter::MultipartAdapter`]
//! trait boundary.

use core::fmt;
use core::time::Duration;
use std::time::SystemTime;

use uuid::Uuid;

use crate::bounds;

/// R2 bucket family the multipart upload targets.
///
/// `#[non_exhaustive]` — adding bucket families post-v1 (e.g.
/// `manifest_legacy`, `repack`) MUST NOT break downstream callers.
///
/// Each variant pairs with a region literal supplied by the caller
/// at object-key composition time; the [`Bucket::name`] helper
/// renders the canonical `<family>-<region>` string per WI-S05-003
/// §1 invariant 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Bucket {
    /// Chunk content bucket: `chunk-<region>`.
    Chunk,
    /// Manifest envelope bucket: `manifest-<region>`.
    Manifest,
}

impl Bucket {
    /// Canonical `<family>-<region>` rendering.
    #[must_use]
    pub fn name(self, region: &str) -> String {
        let family = match self {
            Self::Chunk => bounds::CHUNK_BUCKET_PREFIX,
            Self::Manifest => bounds::MANIFEST_BUCKET_PREFIX,
        };
        let mut out = String::with_capacity(family.len() + region.len());
        out.push_str(family);
        out.push_str(region);
        out
    }

    /// The canonical bucket family literal (e.g. `"chunk-"`).
    #[must_use]
    pub const fn family_prefix(self) -> &'static str {
        match self {
            Self::Chunk => bounds::CHUNK_BUCKET_PREFIX,
            Self::Manifest => bounds::MANIFEST_BUCKET_PREFIX,
        }
    }
}

impl fmt::Display for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Chunk => "chunk",
            Self::Manifest => "manifest",
        })
    }
}

/// Validated 1-based part number — always within `1..=10_000`.
///
/// The newtype is the only legal way to pass a part number into
/// [`crate::adapter::MultipartAdapter::upload_part`] / `complete`,
/// so out-of-range numbers are rejected at construction time
/// (`MaxPartsExceeded`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartNumber(u32);

impl PartNumber {
    /// Canonical constructor — `n` must be `1..=10_000`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::MultipartError::MaxPartsExceeded`] if `n`
    /// falls outside the hard limit.
    pub fn new(n: u32) -> Result<Self, crate::MultipartError> {
        if n == 0 || n > bounds::R2_MAX_PARTS_PER_UPLOAD {
            Err(crate::MultipartError::MaxPartsExceeded {
                part_number: n,
                max: bounds::R2_MAX_PARTS_PER_UPLOAD,
            })
        } else {
            Ok(Self(n))
        }
    }

    /// Borrow the inner `u32`.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PartNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// R2 part fingerprint — S3-compatible MD5 / SHA hex string returned
/// by `UploadPart` and consumed by `CompleteMultipartUpload` in
/// canonical part-number order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PartETag(String);

impl PartETag {
    /// Wrap a pre-computed ETag string. The ETag is opaque to the
    /// adapter — it's the R2 SDK's contract that the same bytes
    /// hash to the same ETag.
    #[must_use]
    pub fn new(etag: impl Into<String>) -> Self {
        Self(etag.into())
    }

    /// Borrow the ETag string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PartETag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lifecycle state of a multipart session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SessionState {
    /// Session is open for `upload_part` calls.
    InProgress,
    /// Session was successfully completed.
    Completed,
    /// Session was aborted (cleanup path).
    Aborted,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Aborted => "aborted",
        })
    }
}

/// Session handle returned by [`crate::adapter::MultipartAdapter::initiate`]
/// and consumed by every subsequent call.
///
/// Bound to a `tenant_id` at construction; the adapter rejects every
/// downstream call whose `tenant_id` argument disagrees with this
/// binding via [`crate::MultipartError::CrossTenantUpload`].
///
/// `#[non_exhaustive]` — adding fields (e.g. `region`,
/// `expires_at`) MUST NOT break downstream callers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MultipartUpload {
    /// R2-issued opaque upload id.
    pub upload_id: String,
    /// The tenant the upload is bound to (cross-tenant guard).
    pub tenant_id: Uuid,
    /// The bucket family the upload targets.
    pub bucket: Bucket,
    /// The canonical R2 object key (tenant-prefix scoped — see
    /// [`crate::object_key::compose`]).
    pub object_key: String,
    /// Wall-clock initiation time (used by orphan detection +
    /// audit).
    pub initiated_at: SystemTime,
}

impl MultipartUpload {
    /// Construct a new session handle. The `upload_id` is opaque —
    /// production wiring uses an R2-minted string; the in-memory
    /// fake mints a UUIDv4 hex.
    #[must_use]
    pub fn new(
        upload_id: impl Into<String>,
        tenant_id: Uuid,
        bucket: Bucket,
        object_key: impl Into<String>,
        initiated_at: SystemTime,
    ) -> Self {
        Self {
            upload_id: upload_id.into(),
            tenant_id,
            bucket,
            object_key: object_key.into(),
            initiated_at,
        }
    }
}

/// Object handle returned by
/// [`crate::adapter::MultipartAdapter::complete`].
///
/// `#[non_exhaustive]` — adding fields (e.g. `version_id`,
/// `last_modified`) MUST NOT break downstream callers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompletedObject {
    /// The bucket the object lives in.
    pub bucket: Bucket,
    /// The canonical R2 object key.
    pub object_key: String,
    /// The S3-style multipart ETag (composite MD5 of part ETags + part count).
    pub etag: String,
    /// Total durable size in bytes (sum of all part sizes).
    pub size_bytes: u64,
}

/// Snapshot of an orphaned multipart session as enumerated by
/// [`crate::adapter::MultipartAdapter::list_orphans`].
///
/// `#[non_exhaustive]` — adding fields MUST NOT break downstream
/// sweepers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OrphanedUpload {
    /// The orphaned upload id.
    pub upload_id: String,
    /// The tenant the upload was bound to.
    pub tenant_id: Uuid,
    /// The canonical R2 object key.
    pub object_key: String,
    /// Wall-clock initiation time (drives sweeper age comparison).
    pub initiated_at: SystemTime,
    /// Age of the session relative to the sweeper's reference
    /// `now`. Computed at enumeration time; mirrored here for
    /// audit-log convenience.
    pub age: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MultipartError;

    #[test]
    fn bucket_name_renders_canonical_form() {
        assert_eq!(Bucket::Chunk.name("sam"), "chunk-sam");
        assert_eq!(Bucket::Manifest.name("iad"), "manifest-iad");
    }

    #[test]
    fn part_number_rejects_zero() {
        assert!(matches!(
            PartNumber::new(0),
            Err(MultipartError::MaxPartsExceeded { part_number: 0, .. })
        ));
    }

    #[test]
    fn part_number_rejects_above_limit() {
        assert!(matches!(
            PartNumber::new(10_001),
            Err(MultipartError::MaxPartsExceeded {
                part_number: 10_001,
                max: 10_000,
            })
        ));
    }

    #[test]
    fn part_number_accepts_boundary() {
        assert_eq!(PartNumber::new(1).map(|p| p.get()).ok(), Some(1));
        assert_eq!(PartNumber::new(10_000).map(|p| p.get()).ok(), Some(10_000));
    }

    #[test]
    fn session_state_display() {
        assert_eq!(SessionState::InProgress.to_string(), "in_progress");
        assert_eq!(SessionState::Completed.to_string(), "completed");
        assert_eq!(SessionState::Aborted.to_string(), "aborted");
    }

    #[test]
    fn part_etag_round_trips() {
        let e = PartETag::new("abc123");
        assert_eq!(e.as_str(), "abc123");
        assert_eq!(e.to_string(), "abc123");
    }
}
