//! `corelink-r2-multipart` — R2 multipart adapter (WI-S05-003).
//!
//! Ships the canonical [`MultipartAdapter`] trait + a host-side
//! [`InMemoryMultipartAdapter`] fake that honours every load-bearing
//! semantic the production R2 binding will inherit:
//!
//! 1. **Tenant-prefix scoped object keys** (5-Layer Defense Layer 4
//!    per `INV-MULTIPART-PATH-TENANT-SCOPED`). Every `object_key`
//!    flows through [`object_key::compose`] which structurally
//!    embeds the tenant prefix between the bucket family and the
//!    content digest; client-supplied path components are
//!    structurally unreachable.
//! 2. **Idempotent lifecycle** (init / upload-part / complete /
//!    abort) per `INV-MULTIPART-IDEMPOTENT`. Re-initiating with the
//!    same `(tenant_id, object_key)` while a session is `InProgress`
//!    returns the existing upload id; re-uploading the same part
//!    bytes is a no-op on the part list (the adapter records the
//!    same ETag); re-completing returns the cached
//!    [`CompletedObject`]; re-aborting is a no-op.
//! 3. **Cross-tenant upload-id rejection**. The adapter binds the
//!    `upload_id ↔ tenant_id` couple at `initiate` time and refuses
//!    every subsequent call (`upload_part` / `complete` / `abort`)
//!    whose `tenant_id` argument disagrees with the stored binding
//!    — the fake returns [`MultipartError::CrossTenantUpload`] and
//!    leaves the session untouched.
//! 4. **Bounded per-tenant concurrency** via [`tokio::sync::Semaphore`].
//!    A tenant holds at most
//!    [`bounds::DEFAULT_PER_TENANT_PARALLEL_PARTS`] concurrent
//!    `upload_part` calls; the immediate-poll
//!    [`concurrency::PerTenantSemaphore::try_acquire`] callsite
//!    trips [`MultipartError::ConcurrencyLimitReached`] (the
//!    `try_*` API surface; `upload_part` itself awaits a permit so
//!    the back-pressure path is non-blocking + observable).
//! 5. **Bounded parser**. Part numbers are `1..=10_000` per the R2
//!    hard limit; [`PartNumber::new`] rejects out-of-range numbers
//!    with [`MultipartError::MaxPartsExceeded`].
//! 6. **Orphan enumeration** via [`MultipartAdapter::list_orphans`]
//!    — the only legal way for the WI-S05-006 sweeper to discover
//!    sessions older than `max_age` and abort them.
//!
//! ## Anti-scope (per WI §6.2)
//!
//! - Resumable upload mid-stream restart.
//! - Multi-region replication of multipart sessions.
//! - HEAD-before-initiate pre-flight validation.
//! - Customer-tunable part size at GA.
//!
//! ## Integration seam (deferred per charter trait-abstraction-defer)
//!
//! - Real `aws-sdk-s3` Cloudflare R2 shim — lands in WI-S05-006.
//!   The trait surface in this crate is the integration boundary;
//!   the in-memory fake here is the canonical reference impl that
//!   the production shim must observably match (round-tripping the
//!   property tests in `tests/prop_r2_multipart.rs`).
//!
//! - D1 `multipart_sessions` schema mounting — lands in WI-S05-004.
//!   Until the schema is live, the fake holds the partial-UNIQUE
//!   `(tenant_id, object_key) WHERE state='in_progress'` semantic
//!   in-memory; once the schema lands, the production adapter MUST
//!   honour the same partial-UNIQUE shape.
//!
//! ## API stability
//!
//! [`Bucket`], [`MultipartUpload`], [`PartETag`], [`CompletedObject`],
//! [`OrphanedUpload`], [`MultipartError`] all carry
//! `#[non_exhaustive]` so additive variants / fields don't break
//! downstream callers across post-v1 releases. Bounds in
//! [`bounds`] are pinned for the life of the crate; drift is an
//! `INV-MULTIPART-BOUNDED-PARSER` regression and is gated on CI by
//! [`tests/prop_r2_multipart.rs`](../tests/prop_r2_multipart.rs).

#![forbid(unsafe_code)]

pub mod adapter;
pub mod bounds;
pub mod concurrency;
pub mod error;
pub mod in_memory;
pub mod object_key;
pub mod types;

pub use adapter::{InitiateRequest, MultipartAdapter};
pub use error::MultipartError;
pub use in_memory::{AlwaysFailingMultipartAdapter, InMemoryMultipartAdapter};
pub use types::{
    Bucket, CompletedObject, MultipartUpload, OrphanedUpload, PartETag, PartNumber, SessionState,
};

/// Crate version sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod lib_tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "test code; panic on assertion failure is the contract"
    )]

    use super::*;

    #[test]
    fn version_matches_cargo_pkg_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
