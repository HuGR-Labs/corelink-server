//! REAPI v2 SplitBlob/SpliceBlob handler module (WI-S05-001).
//!
//! Re-exports the canonical handler trait + impl + per-WI trait
//! abstractions for the upstream integration tier (gRPC tonic + REST
//! axum surfaces in `corelink-reapi` `host-server` feature; deferred to
//! WI-S05-006 conformance suite per charter trait-abstraction-defer
//! pattern).
//!
//! ## Public surface
//!
//! - [`SplitSpliceHandler`] / [`SplitSpliceHandlerImpl`] — the
//!   canonical handler trait + the canonical impl with the
//!   `init/append/finalize/abort` SplitBlob flow + the
//!   `manifest-lookup → per-chunk verified streaming` SpliceBlob flow.
//! - [`BlobDigest`] / [`ChunkDigest`] / [`ManifestDigest`] /
//!   [`SessionId`] / [`ChunkIndex`] — typed shapes that travel across
//!   the trait boundary.
//! - [`SplitError`] / [`SpliceError`] — `#[non_exhaustive]` error
//!   taxonomies mapped 1:1 to `COR_MULTIPART_*` codes per WI §23.
//! - [`BlobEventType`] — 5-variant audit taxonomy + the
//!   `#[non_exhaustive]` `BlobAuditRecord` shape.
//! - [`session::SessionStore`] — multipart session row store
//!   (`(tenant_id, blob_digest)` PK; in-memory fake here; D1 binding
//!   in WI-S05-004 schema + WI-S05-006 binding).
//! - [`chunk_store::ChunkStore`] — chunk content-addressable store
//!   (`(tenant_id, chunk_digest)` PK; in-memory fake here; R2 binding
//!   in WI-S05-003).
//! - [`assembler::BlobAssembler`] — manifest builder + streaming
//!   verifier (delegate WI-S05-005 in production; in-memory fake
//!   ships here so the handler has end-to-end coverage at
//!   pure-logic level).
//! - [`audit::AuditSink`] — the audit emit seam used by the handler.
//!
//! ## What is NOT here
//!
//! - Tonic gRPC + axum REST wrappers: deferred to WI-S05-006
//!   alongside the conformance suite. The handler trait is the
//!   integration seam.
//! - Real R2 multipart binding: WI-S05-003.
//! - Real D1 schema for chunks / manifest_chunks / multipart_sessions:
//!   WI-S05-004.
//! - Real Merkle manifest builder/verifier: WI-S05-005.
//! - Sweeper cron DO + RB-FM-060: lands in [`sweeper`] at WI-S05-006
//!   alongside the REAPI conformance suite + 100k cross-tenant property
//!   test + DASH-MULTIPART dashboard + PRR ship gate.

pub mod assembler;
pub mod audit;
pub mod chunk_store;
pub mod handler;
pub mod session;
pub mod split_splice;
pub mod sweeper;
pub mod types;

pub use assembler::{
    AssemblerError, BlobAssembler, ChunkSink, CollectingSink, InMemoryBlobAssembler, ManifestKey,
    ManifestRecord,
};
pub use audit::{AuditSink, AuditSinkError, BlobAuditRecord, BlobEventType, InMemoryAuditSink};
pub use chunk_store::{ChunkKey, ChunkRecord, ChunkStore, ChunkStoreError, InMemoryChunkStore};
pub use session::{
    BoundChunk, InMemorySessionStore, OrphanCandidate, SessionFinalize, SessionInit, SessionKey,
    SessionSnapshot, SessionState, SessionStore, SessionStoreError,
};
pub use split_splice::{
    Clock, FakeClock, FinalizeSplitOutcome, InitSplitOutcome, SpliceError, SpliceOutcome,
    SplitError, SplitSpliceHandler, SplitSpliceHandlerBuilder, SplitSpliceHandlerImpl, SystemClock,
    MAX_CHUNK_BYTES,
};
pub use sweeper::{
    canonical_metric_pairs, InMemoryOrphanSweeper, OrphanSweeper, SweepBatchOutcome,
    SweepRowOutcome, SweeperError, SweeperTickOutcome, MAX_BATCH_SIZE as SWEEPER_MAX_BATCH_SIZE,
    ORPHAN_AGE_MS, ORPHAN_SWEPT_REASON, TICK_INTERVAL_MS,
};
pub use types::{
    BlobDigest, ChunkDigest, ChunkIndex, ManifestDigest, SessionId, MAX_CHUNKS_PER_BLOB,
};
