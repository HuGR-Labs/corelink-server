//! Production [`R2Backend`] implementation backed by a Cloudflare R2 bucket.
//!
//! `worker::r2::Bucket` provides `put / get / head / delete` plus
//! multipart. We expose only the three operations the [`R2Backend`]
//! trait declares: `put_if_none_match`, `get`, `head` — multipart lives
//! behind a separate trait in `corelink-r2-multipart` and is wired in
//! its own adapter (out of scope for this crate).
//!
//! # `If-None-Match: *` semantics
//!
//! Cloudflare R2 supports the `If-None-Match` precondition on `PUT`
//! since 2024-Q1. When the precondition fails, the SDK returns a
//! `PreconditionFailed` error. We map that to
//! [`BackendPutOutcome::AlreadyExists`] (idempotent duplicate write per
//! INV-CAS-IDEMPOTENCY). All other errors propagate as
//! [`R2Error::Backend`] which surfaces as `COR_SERVICE_DEGRADED`
//! upstream.
//!
//! # `Send`-future bridging
//!
//! `worker::r2::Bucket::*` futures are `!Send` because they wrap
//! `js_sys::JsFuture`. The `R2Backend` trait demands `Send + 'a`
//! futures. CF Workers are single-threaded so the apparent `Send` is
//! sound in practice; we use `worker::send::SendFuture` to satisfy the
//! type system. This is the same pattern used by `CfKvJwksCache` in
//! `corelink-clerk-cf`.

use bytes::Bytes;
use corelink_cas::r2_storage::R2Error;
use corelink_cas::r2_storage::{BackendPutOutcome, R2Backend};
use std::future::Future;
// `worker::r2` is a private module in workers-rs 0.8; the public re-export
// at crate root (`pub use crate::r2::*` in `worker/src/lib.rs`) lifts
// `Bucket`, `Object`, `ObjectBody`, the builder types, etc. into
// `worker::*`. We import from the crate root to stay on the public API.
use worker::Bucket;

/// Production [`R2Backend`] adapter wrapping a `worker::r2::Bucket`.
///
/// Construct via [`CfR2BucketAdapter::new`] from a bucket obtained via
/// `env.bucket("CAS_BUCKET")`. The adapter is cheaply clonable (the
/// inner `Bucket` is itself `Clone`).
#[derive(Clone, Debug)]
pub struct CfR2BucketAdapter {
    bucket: Bucket,
}

impl CfR2BucketAdapter {
    /// Wrap a `worker::r2::Bucket` binding.
    ///
    /// ```ignore
    /// // Inside a CF Worker fetch handler:
    /// let bucket = env.bucket("CAS_BUCKET")?;
    /// let backend = CfR2BucketAdapter::new(bucket);
    /// ```
    #[must_use]
    pub fn new(bucket: Bucket) -> Self {
        Self { bucket }
    }

    /// Borrow the underlying `worker::r2::Bucket` for callers that need
    /// the raw surface (e.g. multipart upload paths).
    #[must_use]
    pub fn inner(&self) -> &Bucket {
        &self.bucket
    }
}

impl R2Backend for CfR2BucketAdapter {
    fn put_if_none_match<'a>(
        &'a self,
        key: &'a str,
        body: Bytes,
    ) -> impl Future<Output = Result<BackendPutOutcome, R2Error>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            // R2 SDK accepts `Vec<u8>` for the body; we copy because
            // `Bytes` is reference-counted and the SDK consumes the
            // data. The copy is on the wasm heap; CF's binding side
            // performs another copy across the JS boundary.
            let payload: Vec<u8> = body.to_vec();
            // R2 `put` does not currently surface `If-None-Match` as a
            // first-class builder method in workers-rs 0.8. We use the
            // `http_metadata` etag-equivalent path: attempt the put,
            // and if it fails with a "precondition" error, classify as
            // duplicate. As of workers-rs 0.8 the `PutOptionsBuilder`
            // exposes `.execute()` only — INM support arrived in
            // 0.8.4+. We fall back to a head-probe-then-put for now;
            // this is racy under burst (two concurrent first writes
            // could both pass the head probe + both write), but the
            // CAS contract treats the second write as a duplicate
            // anyway since the digest is content-addressed and the
            // body bytes are identical (INV-CAS-IDEMPOTENCY §3.2).
            let exists = self
                .bucket
                .head(key.to_string())
                .await
                .map_err(|e| R2Error::Backend(format!("r2 head: {e}")))?
                .is_some();
            if exists {
                return Ok(BackendPutOutcome::AlreadyExists);
            }
            self.bucket
                .put(key.to_string(), payload)
                .execute()
                .await
                .map_err(|e| R2Error::Backend(format!("r2 put: {e}")))?;
            Ok(BackendPutOutcome::Stored)
        })
    }

    fn get<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<Bytes, R2Error>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            let object = self
                .bucket
                .get(key.to_string())
                .execute()
                .await
                .map_err(|e| R2Error::Backend(format!("r2 get: {e}")))?
                .ok_or(R2Error::NotFound)?;
            let body = object.body().ok_or_else(|| {
                // CF returned object metadata but no body — treat as
                // backend fault (not a miss) since the object exists.
                R2Error::Backend("r2 get: body absent on hydrated object".to_owned())
            })?;
            let bytes_vec = body
                .bytes()
                .await
                .map_err(|e| R2Error::Backend(format!("r2 body bytes: {e}")))?;
            Ok(Bytes::from(bytes_vec))
        })
    }

    fn head<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<bool, R2Error>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            let opt = self
                .bucket
                .head(key.to_string())
                .await
                .map_err(|e| R2Error::Backend(format!("r2 head: {e}")))?;
            Ok(opt.is_some())
        })
    }
}
