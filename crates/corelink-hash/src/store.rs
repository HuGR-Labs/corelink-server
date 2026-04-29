//! [`BlobStoreWrite`] trait — the storage-adapter contract.
//!
//! This trait is the architectural seam that turns "type-driven CAS
//! integrity" from a library claim into a repo-enforced invariant. Any
//! storage adapter (R2, KV, in-memory test fake) plugged into the CoreLink
//! write path **must** implement this trait, and the signature requires a
//! `&VerifiedBody` argument — a value that, by construction, can only have
//! been produced by a successful BLAKE3 verification (see
//! [`VerifiedBody::new`](crate::VerifiedBody::new)).
//!
//! The S-01 R2 adapter (WI-S01-003) implements this trait; once it lands,
//! any code path that wants to write to R2 *physically* cannot do so
//! without first going through `VerifiedBody::new`. Forgetting the
//! verification is a compile error, not a code-review oversight.

use core::error::Error as StdError;
use core::future::Future;

use crate::VerifiedBody;

/// Storage-adapter write contract.
///
/// `put_verified` accepts only [`&VerifiedBody`](crate::VerifiedBody),
/// enforcing INV-CAS-INTEGRITY at the type system level. Async, fallible
/// per-implementation.
pub trait BlobStoreWrite: Send + Sync {
    /// Implementation-specific error type (R2 IO, fake-store closed, …).
    type Error: StdError + Send + Sync + 'static;

    /// Persist `vb` to the underlying store.
    ///
    /// Implementations are free to require additional context (region,
    /// tenant prefix, key prefix); those are concerns of the concrete
    /// adapter and are wired into its `&self`.
    fn put_verified<'a>(
        &'a self,
        vb: &'a VerifiedBody,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}
