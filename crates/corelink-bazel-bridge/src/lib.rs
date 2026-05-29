//! `corelink-bazel-bridge` — REAPI v2 REST translation layer for Bazel
//! `--remote_cache`.
//!
//! # Purpose
//!
//! This crate maps the subset of REAPI v2 cache REST endpoints that Bazel
//! speaks onto the existing [`corelink_handler_cas`] and
//! [`corelink_handler_ac`] trait surfaces. Every Bazel user can point
//! `--remote_cache=https://corelink-api.humangr.com` and get distributed
//! cache for free; the bridge is the translation layer that makes it work.
//!
//! # Protocol pinned
//!
//! REAPI v2 version [`REAPI_VERSION`] (Remote Execution API, REAPI v2 spec
//! maintained by the Remote Execution Working Group). The REST shape is
//! used throughout; gRPC/tonic is explicitly **not** a dependency of this
//! crate.
//!
//! # Endpoint mapping
//!
//! | REAPI v2 REST | CoreLink internal |
//! |---|---|
//! | `GET /<instance>/blobs/<hash>/<size>` | CAS read |
//! | `POST /<instance>/uploads/<uuid>/blobs/<hash>/<size>` | CAS write |
//! | `GET /<instance>/blobs/ac/<hash>/<size>` | AC lookup |
//! | `PUT /<instance>/blobs/ac/<hash>/<size>` | AC update |
//! | `POST /<instance>/findMissingBlobs` | batch find-missing |
//!
//! # Modules
//!
//! - [`digest`] — REAPI `Digest` type: `{ hash, size_bytes }` + parse +
//!   validation.
//! - [`uri`] — REAPI URI pattern parser: tenant/instance extraction +
//!   operation kind.
//! - [`find_missing`] — `FindMissingHandler` trait + `InMemoryFindMissing`
//!   delegating to the CAS read handler via batch loop.
//! - [`adapter`] — REAPI-shaped adapter wrapping `Arc<dyn CasReadHandler>`
//!   / `CasWriteHandler` / `AcLookupHandler` / `AcUpdateHandler`.
//! - [`error`] — `BazelBridgeError` taxonomy.
//!
//! # Hard invariants
//!
//! - **INV-BAZEL-DIGEST-VALIDATE** — REAPI Digest hash MUST be exactly 64
//!   lowercase hex characters; size_bytes MUST be ≤ 4 GiB on writes; the
//!   bytes length MUST match size_bytes on PUT, else [`error::BazelBridgeError::SizeMismatch`].
//! - **INV-BAZEL-FIND-MISSING-CAP** — `findMissingBlobs` rejects batches
//!   larger than 4096 digests with a 413-equivalent error
//!   ([`error::BazelBridgeError::BatchTooLarge`]).
//! - **INV-BAZEL-NO-GROPC** — this crate MUST NOT depend on tonic, prost,
//!   or any gRPC runtime. REST only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod adapter;
pub mod digest;
pub mod error;
pub mod find_missing;
pub mod uri;

/// REAPI protocol version this crate targets.
///
/// Pinned per the implementation contract; the REST cache surface covered
/// is the v2.3.0 subset used by Bazel `--remote_cache`.
pub const REAPI_VERSION: &str = "2.3.0";

/// Maximum number of digests accepted in a single `findMissingBlobs`
/// request. REAPI v2.3.0 §5.3: servers MAY enforce this cap.
pub const FIND_MISSING_BLOB_CAP: usize = 4096;

/// Maximum allowed `size_bytes` for a single blob upload. REAPI v2.3.0
/// §5.2: blobs larger than 4 GiB MUST use the ByteStream API; since this
/// bridge is REST-only we reject oversized blobs early.
pub const MAX_BLOB_SIZE_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
