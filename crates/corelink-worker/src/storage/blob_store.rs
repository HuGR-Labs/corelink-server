//! [`BlobStore`] — the **read + write** trait abstraction promised in
//! WI-S01-003 §6.1.3.
//!
//! `corelink-hash` already exposes [`BlobStoreWrite`](corelink_hash::BlobStoreWrite),
//! the *write-only* type-driven seam (only `&VerifiedBody` may pass). The WI
//! also calls for a unified read/write trait so that future swaps (Backblaze
//! B2, AWS S3, Tigris, ...) can be wired in without touching the REAPI
//! handler. This module is that contract.
//!
//! The trait is generic over the per-tenant context type so that callers
//! (S-01-005 REAPI handler; S-02 read path; S-06 GC sweeper) can reuse it
//! without taking an unnecessary dependency on the concrete `TenantCtx`
//! when a future region/tenant ABI evolves. In practice, the only impl
//! today is for `(R2Writer, R2Reader)` over `TenantCtx`, but the trait is
//! the documented swap point.

use core::error::Error as StdError;
use core::future::Future;

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};

/// Combined blob-store contract: `put_verified` (write, type-driven via
/// `&VerifiedBody`) + `get` (read).
///
/// Implementations in this crate: [`R2BlobStore`].
///
/// `Error` mirrors the `corelink-hash` style — implementation-specific
/// `Send + Sync + StdError`. The REAPI handler / read path map this back to
/// `error_taxonomy.md` codes via `R2Error::taxonomy_code()`.
pub trait BlobStore: Send + Sync {
    /// Per-request tenant/region context the impl needs. For the R2 impl
    /// this is [`crate::TenantCtx`].
    type Ctx;
    /// Implementation-specific error type.
    type Error: StdError + Send + Sync + 'static;

    /// Persist `vb` under the impl's canonical key for `ctx`. Idempotent on
    /// duplicate (returns `Ok(())` either way; concrete writers expose a
    /// metric-distinguishable variant separately).
    fn put_verified<'a>(
        &'a self,
        ctx: &'a Self::Ctx,
        vb: &'a VerifiedBody,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    /// Fetch the blob at `(ctx, digest)`. Returns the impl's `NotFound`
    /// variant on miss; cross-tenant reads always see `NotFound` by
    /// construction (REG-NAMESPACE-001 + ADR-0028 enumeration-oracle
    /// closure).
    fn get<'a>(
        &'a self,
        ctx: &'a Self::Ctx,
        digest: &'a Digest,
    ) -> impl Future<Output = Result<Bytes, Self::Error>> + Send + 'a;
}

use crate::storage::error::R2Error;
use crate::storage::r2::{R2Backend, R2Reader, R2Writer};
use crate::TenantCtx;

/// Concrete [`BlobStore`] impl pairing an [`R2Writer`] and an [`R2Reader`].
///
/// Returned by [`R2BlobStore::new`]. Writer + reader **must** be pinned to
/// the same [`Region`](crate::Region); the constructor enforces this so a
/// caller cannot accidentally wire a write-WNAM / read-WEUR store and
/// silently land blobs in one bucket while reading from another. The
/// underlying [`R2Backend`] is left to the caller (in production, both
/// halves share the same `Arc<B>`; the in-memory test fake similarly).
#[derive(Debug)]
pub struct R2BlobStore<B: R2Backend> {
    writer: R2Writer<B>,
    reader: R2Reader<B>,
}

/// Error returned by [`R2BlobStore::new`] when writer + reader regions
/// disagree. A mis-paired store would silently land writes in one bucket
/// and reads from another; the constructor refuses this configuration.
#[derive(Debug, thiserror::Error)]
#[error("R2BlobStore region mismatch: writer={writer}, reader={reader}")]
pub struct R2BlobStoreRegionMismatch {
    /// Region the supplied writer was pinned to.
    pub writer: crate::Region,
    /// Region the supplied reader was pinned to.
    pub reader: crate::Region,
}

impl<B: R2Backend> R2BlobStore<B> {
    /// Pair a writer + reader into a single [`BlobStore`] handle.
    ///
    /// Returns [`R2BlobStoreRegionMismatch`] if the two halves disagree on
    /// region.
    pub fn new(
        writer: R2Writer<B>,
        reader: R2Reader<B>,
    ) -> Result<Self, R2BlobStoreRegionMismatch> {
        if writer.region() != reader.region() {
            return Err(R2BlobStoreRegionMismatch {
                writer: writer.region(),
                reader: reader.region(),
            });
        }
        Ok(Self { writer, reader })
    }

    /// Borrow the underlying writer (escape hatch for callers that need
    /// the metric-distinguishable [`crate::storage::r2::PutOutcome`] return).
    #[must_use]
    pub fn writer(&self) -> &R2Writer<B> {
        &self.writer
    }

    /// Borrow the underlying reader.
    #[must_use]
    pub fn reader(&self) -> &R2Reader<B> {
        &self.reader
    }
}

impl<B: R2Backend> BlobStore for R2BlobStore<B> {
    type Ctx = TenantCtx;
    type Error = R2Error;

    async fn put_verified(&self, ctx: &TenantCtx, vb: &VerifiedBody) -> Result<(), R2Error> {
        self.writer.put(ctx, vb).await.map(|_| ())
    }

    async fn get(&self, ctx: &TenantCtx, digest: &Digest) -> Result<Bytes, R2Error> {
        self.reader.get(ctx, digest).await
    }
}
