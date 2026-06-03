//! Pure-logic SplitBlob/SpliceBlob handler (WI-S05-001 §1, §6.1).
//!
//! [`SplitSpliceHandlerImpl`] is the canonical handler that orchestrates
//! the multipart-upload SplitBlob flow (init session → append chunks →
//! finalize manifest, OR abort) and the SpliceBlob fan-out flow
//! (manifest lookup → per-chunk verified reassembly streamed to the
//! caller). The trait surface ([`SplitSpliceHandler`]) is the
//! integration seam consumed by the gRPC tonic + axum REST wrappers
//! (deferred to the WI-S05-006 conformance suite landing — same
//! trait-abstraction-defer pattern that `reapi::ac` follows).
//!
//! ## 5-Layer Defense (auth_model.md §8.1 + ADR-0035)
//!
//! Every handler call enforces:
//!
//! 1. **Layer 1 — auth verify** is upstream (`AuthLayer`); the handler
//!    receives an `AuthCtx` reference whose construction was gated by
//!    the verifier.
//! 2. **Layer 2 — D1/session enforcement**: the
//!    `super::session::SessionStore` surface is
//!    `(tenant_id, session_id)`-keyed via `SessionKey::new`; the
//!    `super::chunk_store::ChunkStore` surface is
//!    `(tenant_id, chunk_digest)`-keyed via
//!    `super::chunk_store::ChunkKey::new`; the
//!    `super::assembler::BlobAssembler` surface is
//!    `(tenant_id, manifest_digest)`-keyed via
//!    `super::assembler::ManifestKey::new`. Cross-tenant lookups are
//!    structurally unreachable.
//! 3. **Layer 3 — scope check**: `auth_ctx.has_scope(SCOPE_CACHE_W)` for
//!    SplitBlob (init/append/finalize/abort); `SCOPE_CACHE_R` for
//!    SpliceBlob.
//! 4. **Layer 4 — HMAC tenant prefix**: every persisted artefact
//!    (chunk, manifest envelope, session row) is keyed under the
//!    materialized `TenantPrefix` read off `AuthCtx::tenant_prefix`
//!    so the storage seam never sees a raw `tenant_id` plaintext (per
//!    ADR-0035 H-3).
//! 5. **Layer 5 — audit emit**: every terminal flow path emits a typed
//!    record via `super::audit::AuditSink`.
//!
//! The 5-layer enforcement is structural — the handler refuses to
//! compile against a key built without an `AuthCtx`-derived
//! `(tenant_id, …)` pair; the session/chunk/assembler trait surfaces
//! refuse to expose any cross-tenant probing API.
//!
//! ## Idempotency contract (`INV-MULTIPART-IDEMPOTENT`)
//!
//! - Re-`init_split` for a `(tenant_id, blob_digest)` whose session is
//!   live returns the existing `SessionId` (echo).
//! - Re-`init_split` for a `(tenant_id, blob_digest)` whose finalize
//!   already produced a manifest returns
//!   `SplitOutcome::AlreadyChunked` echoing the existing
//!   `ManifestDigest`.
//! - `append_chunk` is idempotent on `(session_id, chunk_index)` — same
//!   `chunk_index` re-submitted with the **same** chunk bytes is a
//!   no-op (refcount unchanged); a re-submit with **different** bytes
//!   is rejected with [`SplitError::ChunkOrderingViolation`] preserving
//!   `INV-CAS-IMMUTABILITY`.
//! - `finalize_split` is idempotent on `session_id` — second call
//!   returns the cached `ManifestDigest` without re-building.
//! - `abort_split` on a finalized session is a `SessionAlreadyFinalized`
//!   (no destruction of finalized state — preserves
//!   `INV-MULTIPART-FINALIZE-IRREVOCABLE`).
//!
//! ## Streaming SpliceBlob fail-fast invariant
//!
//! [`SplitSpliceHandler::splice_blob`] streams chunks back to the
//! caller in canonical order via the `super::assembler::BlobAssembler`
//! trait. Per `INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST`, the assembler
//! cancels the stream on the FIRST per-chunk hash mismatch — no
//! unverified bytes ever reach the caller. The handler bubbles the
//! cancellation up as [`SpliceError::ChunkVerificationFailed`] with
//! the offending `chunk_index`.
//!
//! ## Internal layout (wave-33 stage 2.PRE-A.2)
//!
//! The original 1758-LOC monolith is decomposed into the following
//! submodules (every public name re-exported through this parent
//! so consumers continue to import via
//! `reapi::cas::split_splice::*`):
//!
//! - [`types`] — outcome enums + `MAX_CHUNK_BYTES`.
//! - [`errors`] — `SplitError` + `SpliceError` + their `From` impls.
//! - [`handler_trait`] — the `SplitSpliceHandler` trait surface.
//! - [`builder`] — `SplitSpliceHandlerBuilder` + `Clock` +
//!   `SystemClock` + `FakeClock`.
//! - [`handler`] — `SplitSpliceHandlerImpl` (struct, constructor,
//!   region/scope helpers, audit-record builder, trait-impl
//!   trampolines).
//! - [`handler_methods`] — per-method inner async bodies.

pub mod builder;
pub mod errors;
pub mod handler;
pub mod handler_methods;
pub mod handler_trait;
pub mod types;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_common;
#[cfg(test)]
mod tests_splice;

// ---------------------------------------------------------------------------
// Canonical re-exports — preserve the pre-split
// `reapi::cas::split_splice::*` public surface verbatim.
// ---------------------------------------------------------------------------

pub use builder::{Clock, FakeClock, SplitSpliceHandlerBuilder, SystemClock};
pub use errors::{SpliceError, SplitError};
pub use handler::SplitSpliceHandlerImpl;
pub use handler_trait::SplitSpliceHandler;
pub use types::{FinalizeSplitOutcome, InitSplitOutcome, SpliceOutcome, MAX_CHUNK_BYTES};
