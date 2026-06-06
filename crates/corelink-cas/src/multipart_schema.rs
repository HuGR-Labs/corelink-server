//! `corelink-multipart-schema` — D1 (Cloudflare SQLite) multipart + chunking
//! + Merkle storage schema (WI-S05-004).
//!
//! # What this crate ships
//!
//! 1. The canonical SQL artifact `migrations/d1/0003_multipart_chunks_manifest.sql`,
//!    embedded via [`MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST`] so production
//!    code can pass the DDL to `wrangler d1 migrations apply` (or any
//!    host-side runner) without re-reading the file from disk.
//! 2. A **host-side in-memory schema simulator** ([`MultipartSchema`]) that
//!    enforces every load-bearing invariant the migration relies on:
//!    - `chunks` PRIMARY KEY uniqueness on `(tenant_id, chunk_digest)` +
//!      `INSERT … ON CONFLICT DO UPDATE SET refcount = refcount + 1`
//!      (`INV-MULTIPART-IDEMPOTENT` HIGH).
//!    - `manifest_chunks` PRIMARY KEY uniqueness on
//!      `(tenant_id, blob_digest, chunk_index)` + ordered scan invariant.
//!    - `multipart_sessions` PARTIAL UNIQUE on
//!      `(tenant_id, blob_digest_expected) WHERE state = 'in_progress'`
//!      (Lote 10.5bis P0 fix; the SQL artifact carries the partial UNIQUE
//!      INDEX and the simulator enforces the same semantic — a single
//!      in-progress session per blob, multiple completed/aborted rows
//!      legitimately coexist as audit trail).
//!    - State graph `in_progress → completed | aborted` is monotone
//!      (`INV-MULTIPART-STATE-MONOTONIC` HIGH; reverse transitions
//!      rejected).
//!    - Every inline `CHECK` constraint mnemonic listed in the migration
//!      header (`chk_chunks_*`, `chk_manifest_*`, `chk_multipart_*`).
//!    - Tenant-isolation envelope: every read is keyed by `tenant_id`
//!      first, so a Tenant B query for a Tenant A row returns
//!      `Ok(None)` (Layer 4 of 5-Layer Defense).
//!    - `tenant_prefix` BLOB(16) materialised at INSERT time per
//!      ADR-0035 H-3 / `INV-MULTIPART-PATH-KEY-MATERIALIZED` HIGH.
//! 3. The [`region`] module documenting the canonical 5-region list
//!    (`sam`, `iad`, `lhr`, `nrt`, `syd`) that the schema CHECK
//!    constraints enforce + the canonical `corelink-{chunk,manifest}-<region>`
//!    bucket name + wrangler binding for each region.
//!
//! # Why simulator instead of live D1 tests
//!
//! S-05 lands without Cloudflare D1 credentials wired into CI (no
//! remote + Cloudflare Workers + D1 staging are HARD inflection points
//! per `corelink_autonomous_execution_charter.md`). The simulator
//! covers the algorithmic invariants that a schema bug would expose:
//! cross-tenant key collisions, CHECK-constraint regressions,
//! PK-direction confusion at storage layer, idempotent upsert
//! semantics, partial-UNIQUE state-conditioned constraints,
//! state-monotonic transitions. The live-D1 roundtrip tests run in
//! WI-S05-006 once miniflare/wrangler-dev integration tests land
//! alongside the sweeper conformance suite.
//!
//! # Wiring into the multipart handler
//!
//! WI-S05-001's `SessionStore` + `ChunkStore` traits
//! (`corelink-worker::reapi::cas`) are the integration seam the
//! handler depends on. The simulator in this crate ([`MultipartSchema`])
//! mirrors the **same** algorithmic semantics the in-memory fakes ship
//! in WI-S05-001 + WI-S05-003 — both enforce the load-bearing
//! invariants the production D1 binding (deferred to WI-S05-006) will
//! expose. WI-S05-001 keeps its in-tree fakes to avoid a worker →
//! multipart-schema dependency cycle (the worker crate sits
//! topologically above multipart-schema in the DAG); this crate's
//! simulator is the canonical SQL-shape mirror that property tests
//! drive.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - The simulator is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the three tables and reports structured
//!   [`SimError`] errors when an invariant fires.

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST: &str =
    include_str!("../../../migrations/d1/0003_multipart_chunks_manifest.sql");

pub mod region;
pub mod sim;

pub use region::{MultipartRegion, REGION_LIST};
pub use sim::{
    ChunkUpsertOutcome, ChunkUpsertRequest, ChunksRow, ManifestChunkInsertOutcome,
    ManifestChunkInsertRequest, ManifestChunkRow, MultipartFinalizeOutcome,
    MultipartFinalizeRequest, MultipartInitiateOutcome, MultipartInitiateRequest, MultipartSchema,
    MultipartSession, MultipartSessionState, SessionId, SimError, BLOB_DIGEST_HEX_LEN,
    CHUNK_DIGEST_HEX_LEN, CHUNK_INDEX_MAX, CHUNK_SIZE_BYTES_MAX, DEFAULT_SESSION_TTL_MS,
    MAX_CHUNKS_PER_BLOB, TENANT_PREFIX_LEN,
};

/// Returns the canonical schema version recorded by migration 0003 in
/// the D1 `multipart` domain.
///
/// Version is sequential within the D1 domain; D1 (`blob_meta` =
/// schema 1, `ac_meta` = schema 2, `multipart_chunks_manifest` =
/// schema 3) and Postgres (`auth` schema) versioning are independent
/// — a schema bump here does NOT increment the Postgres
/// `schema_version` table.
#[must_use]
pub const fn multipart_schema_version() -> u32 {
    3
}
