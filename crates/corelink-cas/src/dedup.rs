//! `corelink-dedup` — Intra-tenant chunk deduplication index trait +
//! InMemory fake + REAPI v2 `FindMissingBlobs` orchestrator + dedup-on-
//! write counters (WI-S07-001).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the dedup plane: trait surfaces every production
//! Cloudflare D1 binding will satisfy, plus in-memory fakes that
//! exercise every load-bearing invariant the production wiring relies
//! on. Property tests pinned at 10 k iter against the fakes cover
//! tenant isolation (CTRL-ISO-005 existence-oracle gate), idempotent
//! set-difference semantic, monotone dedup-on-write counter accounting,
//! and audit fail-closed envelope without spinning up miniflare.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`index`] module ships [`DedupIndex`] (the canonical batch
//!    `find_missing_blobs` trait every D1 backend will satisfy) +
//!    [`InMemoryDedupIndex`] whose semantics mirror the SQL
//!    `SELECT chunk_digest FROM chunks WHERE tenant_id = ? AND
//!    chunk_digest IN (?, ?, ...) AND deleted_at IS NULL` set-difference
//!    semantic byte-for-byte with the S-05 WI-S05-004 `chunks` table.
//! 2. The [`audit`] module ships [`DedupEventType`] +
//!    [`DedupAuditRecord`] + [`DedupAuditSink`] + the
//!    [`InMemoryDedupAuditSink`] capture sink (fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The canonical 1-event
//!    taxonomy is `corelink.dedup.find_missing_blobs_executed`; the
//!    enum is `#[non_exhaustive]` so follow-on WIs can grow the
//!    taxonomy additively.
//! 3. The [`metrics`] module ships [`DedupMetricsObserver`] +
//!    [`InMemoryDedupMetrics`] capture sink (3 canonical metrics:
//!    `corelink.dedup.chunks_inserted_total`,
//!    `corelink.dedup.chunks_reused_total`,
//!    `corelink.dedup.find_missing_blobs_total`).
//! 4. The [`error`] module ships the canonical [`DedupError`] taxonomy
//!    every fallible API surfaces (transport / programmer / auth
//!    failures only — "digest is missing" is a happy-path outcome, not
//!    an error).
//! 5. The [`write`] module ships [`record_chunk_write`] — the helper
//!    the production `BatchUpdateBlobs` / `WriteBlob` / chunk-upsert
//!    handler invokes after each `chunks` INSERT/ON CONFLICT to
//!    increment the dedup-on-write counters per WI §6.1.7 (R-S07-2).
//!
//! # Why dedup is `trait + fake` here, real D1 in WI-S07-005
//!
//! S-07 lands without Cloudflare D1 credentials wired into CI (no
//! remote + Cloudflare Workers + D1 staging are HARD inflection points
//! per `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a backend binding bug would expose:
//! cross-tenant existence-oracle leak (CTRL-ISO-005), set-difference
//! recombination order, idempotent re-query, monotone counter
//! accounting, audit fail-closed atomicity. The live-D1 conformance
//! tests run alongside WI-S07-005 (DASH-DEDUP + alerts) once
//! miniflare/wrangler-dev integration tests land.
//!
//! # Cripto-driven invariants enforced
//!
//! - **Tenant-scoped strict** (CTRL-ISO-005): every `find_missing_blobs`
//!   call MUST receive a `tenant_id` and the result is computed
//!   against a `(tenant_id, chunk_digest)` PK lookup keyed leftmost on
//!   tenant. A Tenant B query for a digest that exists ONLY under
//!   Tenant A surfaces the digest in the missing set — the response is
//!   structurally indistinguishable from a never-existed digest. Cross-
//!   tenant dedup queries are rejected with [`DedupError::CrossTenantBlocked`]
//!   per `dedup.cross_tenant.enabled = false` (default).
//! - **Idempotent set-difference**: repeated calls with the same
//!   `(tenant_id, queried_digests)` return the same `missing` vec
//!   (modulo input order — preserved by construction).
//! - **Cripto-grade chunk_digest**: `chunk_digest` is BLAKE3-256 of
//!   chunk bytes (S-05 inheritance; deterministic +
//!   collision-resistant ≥ 128-bit). Index integrity follows from
//!   BLAKE3 collision-resistance.
//! - **Soft-delete grace respect**: a chunk row whose
//!   `deleted_at IS NOT NULL` (S-06 grace window 72 h) is treated as
//!   missing, forcing the client to re-upload (re-INSERT increments
//!   refcount + resets `deleted_at`). The fake mirrors this semantic.
//! - **Monotone dedup-on-write counters**: every chunk INSERT bumps
//!   `chunks_inserted_total` by 1; every ON CONFLICT (refcount
//!   increment) bumps `chunks_reused_total` by 1. The two counters are
//!   never decremented; the dedup ratio is `reused / (reused +
//!   inserted)` (target ≥ 3× per spec_contract §SOTA framing).
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - The fake is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the `chunks` table's dedup-relevant
//!   columns and reports structured [`DedupError`] errors when an
//!   invariant fires.
//!
//! # Wiring into the REAPI handler (forward; WI-S07-005 ship gate)
//!
//! The REAPI v2 `FindMissingBlobs` handler in `corelink-reapi` already
//! exists at the BLOB-digest level (`corelink-reapi::find_missing` from
//! WI-S02-002). This crate's [`DedupIndex::find_missing_blobs`] surfaces
//! the **CHUNK-digest** layer for the chunked-CAS path (S-05 multipart
//! / FastCDC content-defined chunking): clients with chunked content
//! call this trait to skip-upload chunks already present, and the
//! gRPC handler composes it on top of the existing BLOB-level RPC. The
//! production wiring lands in WI-S07-005 conformance suite alongside
//! the DASH-DEDUP dashboard.

#![forbid(unsafe_code)]

pub mod audit;
pub mod error;
pub mod index;
pub mod metrics;
pub mod write;

pub use audit::{
    canonical_audit_event_strings, DedupAuditRecord, DedupAuditSink, DedupAuditSinkError,
    DedupEventType, InMemoryDedupAuditSink,
};
pub use error::DedupError;
pub use index::{
    BlobDigest, ChunksRowSummary, DedupConfig, DedupIndex, FindMissingBlobsOutcome,
    InMemoryDedupIndex, MAX_FIND_MISSING_BATCH_SIZE,
};
pub use metrics::{
    canonical_metric_names, DedupMetricKind, DedupMetricsObserver, DedupMetricsObserverError,
    InMemoryDedupMetrics,
};
pub use write::{record_chunk_write, ChunkWriteOutcome};
