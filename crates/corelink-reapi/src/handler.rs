//! Tonic-based gRPC handlers for `BatchUpdateBlobs`, `Capabilities`, and
//! `ByteStream::Write` (host-server feature).
//!
//! ## Surface
//!
//! - [`CasWriteService`] — implements
//!   `build.bazel.remote.execution.v2.ContentAddressableStorage` (write half).
//! - [`CapabilitiesService`] — implements `Capabilities.GetCapabilities`.
//! - [`ByteStreamService`] — implements `google.bytestream.ByteStream`.
//!
//! All three services share a single `Arc<HandlerCore<…>>` so the auth
//! validator + storage + meta + orphan reconciler can be wired once at
//! deploy time and the gRPC services can be instantiated cheaply per RPC.
//!
//! ## Auth seam
//!
//! Authentication runs as the first step of each RPC. We extract the
//! `authorization: Bearer <token>` header from the gRPC `Metadata`, run
//! the injected [`PatValidator`], and bind the resulting
//! [`crate::pat::TenantContext`]. From there a
//! [`corelink_worker::TenantCtx`] is derived (TDK lookup + prefix
//! derivation runs inline at the start of each authenticated RPC).
//!
//! ## TDK plumbing
//!
//! The auth-stub layer (S-01) does NOT carry the TDK; the handler holds an
//! `Arc<TenantDerivationKey>` (production: read once at deploy from
//! Cloudflare Secrets) and uses it together with the validated `tenant_id`
//! to construct the storage-side `TenantCtx`. This matches `tenant.rs`'s
//! "TDK lives in the auth/dispatcher layer; per-request ctx never carries
//! it" rule.
//!
//! ## File layout (Wave 33 Stream A2.1c)
//!
//! The handler module is structured into per-service + per-concern files
//! under `handler/` so each file stays ≤ 500 LOC per the L2.10
//! file-size discipline:
//!
//! - `handler.rs` (parent) — preamble, `Clock`/`SystemClock`/
//!   `HandlerCore`, public re-exports, submodule glue. Per the
//!   workspace `clippy::mod_module_files = "deny"` lint, the parent
//!   module file lives at `handler.rs` rather than `handler/mod.rs`.
//! - `cas.rs` — `CasWriteService` + `ContentAddressableStorage` impl
//!   (`batch_update_blobs` / `batch_read_blobs` / `find_missing_blobs`).
//! - `batch_read.rs` — phase helpers for the two-pass `batch_read_blobs`
//!   pipeline (validate / pre-validate / pass1 / decide / pass2).
//! - `batch_read_compose.rs` — Step 4e response composition.
//! - `capabilities.rs` — `CapabilitiesService` + `Capabilities` impl.
//! - `bytestream.rs` — `ByteStreamService` + `ByteStream` impl (read /
//!   write / query).
//! - `bytestream_stream.rs` — `chunked_read_stream_with_audit` +
//!   chunked-frame helpers + `ReadCompleteAuditGuard` Drop-time fallback.
//! - `helpers.rs` — gRPC plumbing + per-blob orchestration helpers +
//!   resource-name parsers.
//! - `audit_emit.rs` — single-shot audit emitters
//!   (`emit_read_completed_audit_post_stream` etc.) + deterministic
//!   audit-id derivation + public `_pub` re-exports for the HTTP read
//!   handler.
//! - `audit_emit_batch.rs` — `*_at_slot` audit-emit variants used by the
//!   batch handlers + `emit_find_missing_batch_audit`.
//! - `tests.rs` — unit tests pinned at parity with the pre-split
//!   `#[cfg(test)] mod tests` block.

#![allow(
    clippy::result_large_err,
    reason = "tonic::Status is canonical wire-error type sized by tonic; \
              boxing here would break tonic-server trait bounds"
)]

use std::sync::Arc;

use corelink_meta::MetaStore;
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::r2::{R2Backend, R2Reader, R2Writer};
use corelink_worker::TenantCtx as StorageTenantCtx;
use tonic::{Request, Status};

use crate::orchestrator::OrphanReconciler;
use crate::pat::PatValidator;

mod audit_emit;
mod audit_emit_batch;
mod batch_read;
mod batch_read_compose;
mod bytestream;
mod bytestream_stream;
mod capabilities;
mod cas;
mod helpers;
mod per_blob;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Public surface re-exports (preserved at parity with the pre-split file's
// `pub` symbols — every external `use corelink_reapi::handler::*` path
// continues to resolve).
// ---------------------------------------------------------------------------

pub use audit_emit::{emit_read_completed_audit_pub, emit_read_miss_audit_pub};
pub use bytestream::ByteStreamService;
pub use capabilities::CapabilitiesService;
pub use cas::CasWriteService;
pub use helpers::{parse_read_resource_name, ParsedReadResource};

/// Canonical chunk size for `ByteStream::Read` streaming responses.
///
/// 1 MiB matches the canonical REAPI v2 conformance recommendation per
/// `remote_cache_product_profile.md §7.2.1` and stays well below the
/// gRPC default `max_decoding_message_size` (4 MiB) so existing clients
/// (Bazel/Buck2) decode without bumping limits. Worker peak memory per
/// concurrent request is therefore bounded by `chunk + framing overhead`
/// — ≪ the 50 MiB hard ceiling per WI-S02-001 §10.1.3.
///
/// ## Streaming surface scope (WI-S02-001 v1.1.0 §6.1.5 clarification)
///
/// The current [`crate::read::CasReadOrchestrator`] returns the full
/// blob body via [`corelink_worker::storage::r2::R2Reader::get`] which
/// itself returns `bytes::Bytes` (full materialization at the storage
/// adapter seam). Combined with the S-01 single-blob 5 MiB cap
/// (`SINGLE_BLOB_LIMIT_BYTES`), Worker peak memory per concurrent read
/// is bounded by `5 MiB body + 1 MiB chunk overhead = ~6 MiB` — well
/// below the 50 MiB hard ceiling for any S-02 supported blob size.
///
/// True end-to-end streaming (R2 SDK streamed response forwarded
/// byte-for-byte without an intermediate materialization) requires
/// extending `R2Backend::get` → `R2Backend::get_stream`; that change
/// lands in **WI-S05-005** (multipart read) alongside
/// `R2Backend::put_multipart`. Until then the 1 GiB AC scenario in
/// §10.1.3 is unreachable by construction (writes are capped at 5 MiB
/// in S-01).
pub const READ_CHUNK_SIZE_BYTES: usize = 1024 * 1024;

/// Clock seam — production wires `std::time::SystemTime`; tests pin a fixed
/// instant via the `Clock` trait. Pure trait so the handler can compile to
/// wasm32 without pulling chrono into the surface; we only need Unix ms
/// (the audit envelope deliberately omits CE `time` — see
/// [`crate::audit`] module rustdoc).
pub trait Clock: Send + Sync {
    /// Wall-clock instant in Unix milliseconds.
    fn now_ms(&self) -> u64;
}

/// Wall-clock impl backed by `std::time::SystemTime`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

/// Convenience type holding the cross-cutting handler dependencies. All
/// three gRPC services borrow this `Arc` — instantiating a service is
/// cheap (no per-RPC allocation in the hot path).
///
/// ## Read seam (WI-S02-001)
///
/// Adds an [`R2Reader`] alongside the existing `R2Writer`. Both are
/// pinned to the same `Region` (the auth dispatcher must hand the
/// handler a region-pinned core) and share the same backend `Arc<B>`.
/// The reader is constructed once at deploy time and cloned cheaply
/// per-request through the per-service `Arc<HandlerCore>`.
pub struct HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    pat: V,
    tdk: Arc<TenantDerivationKey>,
    writer: Arc<R2Writer<B>>,
    reader: Arc<R2Reader<B>>,
    meta: Arc<M>,
    reconciler: Arc<R>,
    clock: C,
}

impl<V, B, M, R, C> std::fmt::Debug for HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerCore").finish_non_exhaustive()
    }
}

impl<V, B, M, R, C> HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    /// Construct a fresh handler core. The six dependencies (auth, TDK,
    /// writer, reader, meta, reconciler) are sharable across service
    /// clones. `writer.region()` and `reader.region()` MUST agree —
    /// the constructor asserts via `debug_assert_eq!` so any wiring bug
    /// surfaces in CI rather than at first cross-tenant request.
    #[must_use]
    pub fn new(
        pat: V,
        tdk: Arc<TenantDerivationKey>,
        writer: Arc<R2Writer<B>>,
        reader: Arc<R2Reader<B>>,
        meta: Arc<M>,
        reconciler: Arc<R>,
        clock: C,
    ) -> Self {
        debug_assert_eq!(
            writer.region(),
            reader.region(),
            "HandlerCore: writer.region() and reader.region() must agree (caller wiring bug)"
        );
        Self {
            pat,
            tdk,
            writer,
            reader,
            meta,
            reconciler,
            clock,
        }
    }

    /// Borrow the metadata store. Used by the HTTP read handler to
    /// invoke the [`crate::read::CasReadOrchestrator`] without taking
    /// ownership of the core.
    #[must_use]
    pub fn meta(&self) -> &M {
        self.meta.as_ref()
    }

    /// Borrow the R2 reader.
    #[must_use]
    pub fn reader(&self) -> &R2Reader<B> {
        self.reader.as_ref()
    }

    /// Borrow the PAT validator.
    #[must_use]
    pub const fn pat(&self) -> &V {
        &self.pat
    }

    /// Borrow the TDK so HTTP handlers can derive a [`StorageTenantCtx`].
    #[must_use]
    pub fn tdk(&self) -> &TenantDerivationKey {
        self.tdk.as_ref()
    }

    /// Borrow the clock.
    #[must_use]
    pub const fn clock(&self) -> &C {
        &self.clock
    }

    /// Authenticate a tonic `Request`, returning the parsed
    /// `TenantContext` plus the storage-side `TenantCtx` (which carries
    /// the HMAC-derived prefix).
    pub(super) fn authenticate<T>(
        &self,
        req: &Request<T>,
    ) -> Result<(crate::pat::TenantContext, StorageTenantCtx), Status> {
        let token =
            helpers::extract_bearer(req).map_err(|e| helpers::pat_error_to_status(&e))?;
        let request_id = helpers::extract_request_id(req);
        let pat_ctx = self
            .pat
            .authenticate(&token, &request_id)
            .map_err(|e| helpers::pat_error_to_status(&e))?;
        let storage_ctx = StorageTenantCtx::new(&self.tdk, pat_ctx.tenant_id(), pat_ctx.region());
        Ok((pat_ctx, storage_ctx))
    }
}

// Crate-internal accessors so the per-service submodules can reach the
// `Arc<…>` payloads without the per-service files re-implementing the
// HandlerCore struct surface. These are NOT `pub` and never leak past the
// `handler::` module boundary.
impl<V, B, M, R, C> HandlerCore<V, B, M, R, C>
where
    V: PatValidator,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    pub(super) fn writer_arc(&self) -> &Arc<R2Writer<B>> {
        &self.writer
    }
    pub(super) fn reader_arc(&self) -> &Arc<R2Reader<B>> {
        &self.reader
    }
    pub(super) fn meta_arc(&self) -> &Arc<M> {
        &self.meta
    }
    pub(super) fn reconciler_arc(&self) -> &Arc<R> {
        &self.reconciler
    }
    pub(super) fn clock_ref(&self) -> &C {
        &self.clock
    }
}
