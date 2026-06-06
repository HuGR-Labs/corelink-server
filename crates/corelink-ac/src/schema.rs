//! `corelink-ac-schema` — D1 (Cloudflare SQLite) Action Cache schema (WI-S04-002).
//!
//! # What this crate ships
//!
//! 1. The canonical SQL artifact `migrations/d1/0002_ac_meta.sql`,
//!    embedded via [`crate::schema::MIGRATION_0002_AC_META`] so production code can
//!    pass the DDL to `wrangler d1 migrations apply` (or any host-side
//!    runner) without re-reading the file from disk.
//! 2. A **host-side in-memory schema simulator** ([`crate::schema::AcSchema`]) that
//!    enforces every load-bearing invariant the migration relies on:
//!    PRIMARY KEY uniqueness on `(tenant_id, action_digest)`, every
//!    inline CHECK constraint, tenant-isolation envelope (the
//!    `(tenant_id, action_digest)` PK shape makes a Tenant B query
//!    of a Tenant A row return `Ok(None)`), and the idempotent
//!    `INSERT … ON CONFLICT (tenant_id, action_digest) DO UPDATE`
//!    semantic that pins `INV-AC-IDEMPOTENT` +
//!    `INV-AC-RESULT-HASH-IMMUTABLE`. The simulator is purposefully
//!    ORM-free — its sole consumers are property tests in this crate.
//! 3. The [`crate::schema::region`] module documenting the canonical 5-region list
//!    (`sam`, `iad`, `lhr`, `nrt`, `syd`) that the schema CHECK
//!    constraint enforces. Adding a new region requires a new ADR +
//!    new migration per ADR-0036 Rule 3.
//!
//! # Why simulator instead of live D1 tests
//!
//! S-04 lands without Cloudflare D1 credentials wired into CI (no
//! remote + Cloudflare Workers + D1 staging are HARD inflection
//! points per `corelink_autonomous_execution_charter.md`). The
//! simulator covers the algorithmic invariants that a schema bug
//! would expose (cross-tenant key collisions, CHECK-constraint
//! regressions, PK-direction confusion at storage layer, idempotent
//! upsert semantics); the live-D1 roundtrip tests run in WI-S04-006
//! once miniflare/wrangler-dev integration tests land alongside the
//! REAPI conformance suite.
//!
//! # Wiring into the AC handler
//!
//! WI-S04-001's `AcMetaStore` trait surface
//! (`corelink-worker::reapi::ac::meta`) is the integration seam the
//! handler depends on. The simulator in this crate ([`crate::schema::AcSchema`])
//! mirrors the **same** algorithmic semantics the in-memory fake
//! `InMemoryAcMetaStore` ships in WI-S04-001 — both enforce the
//! load-bearing invariants the production D1 binding (deferred to
//! WI-S04-006) will expose. WI-S04-001 keeps its in-tree fake to
//! avoid a worker → ac-schema dependency cycle (the worker crate
//! sits topologically above ac-schema in the DAG); this crate's
//! simulator is the canonical SQL-shape mirror that property tests
//! drive.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - The simulator is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the `ac_meta` table and reports
//!   structured [`crate::schema::SimError`] errors when an invariant fires.

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0002_AC_META: &str = include_str!("../../../migrations/d1/0002_ac_meta.sql");

pub mod region;
pub mod sim;

pub use region::{AcRegion, REGION_LIST};
pub use sim::{
    AcMetaRow, AcSchema, AcUpsertOutcome, AcUpsertRequest, SigAlg, SimError, BLOB_REFS_COUNT_MAX,
    BLOB_REFS_SIZE_MAX, DIGEST_HEX_LEN, RESULT_SIZE_BYTES_MAX, TENANT_PREFIX_LEN,
};

/// Returns the canonical schema version recorded by migration 0002 in
/// the D1 `ac_meta` domain.
///
/// Version is sequential within the D1 domain; D1 (`ac_meta`) and
/// Postgres (`auth` schema) versioning are independent — a schema
/// bump here does NOT increment the Postgres `schema_version` table.
#[must_use]
pub const fn ac_schema_version() -> u32 {
    2
}
