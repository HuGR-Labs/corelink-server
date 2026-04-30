# corelink-meta

CoreLink CAS metadata layer — D1 (Cloudflare SQLite) `blob_meta` + `audit_outbox` transactional store.

This crate is the **persistence layer for CAS metadata** (size, refcount, tombstone) plus the audit-outbox staging area that pairs atomically with every metadata mutation. It implements WI-S01-004 (HIGH_RISK lane: FF-HR-002 + FF-HR-005).

## Status

- **WI-S01-004**: SEALED.
- **Lane**: HIGH_RISK (FF-HR-002 tenant_id-keyed queries; FF-HR-005 AuthZ enforcement layer).

## Public API

```rust
use corelink_meta::{
    AuditEvent, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, InsertOutcome,
    MetaStore,
};
```

The trait surface is tiny on purpose:

- `MetaStore::commit_put` — idempotent `INSERT OR IGNORE` + audit-outbox INSERT, atomic.
- `MetaStore::commit_decrement` — atomic refcount decrement + audit-outbox INSERT.
- `MetaStore::commit_soft_delete` — idempotent tombstone + audit-outbox INSERT.
- `MetaStore::get` — PK lookup; returns the row regardless of tombstone state.

Every commit method takes an `AuditEvent` so the audit-outbox row lands in the same atomic batch as the `blob_meta` mutation. This is the load-bearing INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER guarantee — no orphan blob_meta mutations, no audit gaps.

## Schema (canonical)

The migration file is at the workspace root: `migrations/d1/0001_blob_meta.sql`. The crate embeds the SQL via `include_str!` and exposes it as `corelink_meta::MIGRATION_SQL`.

Two tables:

- `blob_meta(tenant_id TEXT, digest TEXT, size_bytes INTEGER, refcount INTEGER DEFAULT 1, created_at INTEGER, last_accessed_at INTEGER, deleted_at INTEGER NULL, PRIMARY KEY(tenant_id, digest))`.
- `audit_outbox(id TEXT PK, tenant_id TEXT, digest TEXT NULL, request_id TEXT, event_type TEXT, payload_json TEXT, enqueued_at INTEGER, emitted_at INTEGER NULL, UNIQUE(request_id, event_type))`.

Indexes:

- `idx_blob_meta_tenant_alive ON blob_meta(tenant_id, deleted_at) WHERE deleted_at IS NULL` — alive-set index for AuthZ checks (S-02 read path).
- `idx_blob_meta_gc_candidates ON blob_meta(deleted_at) WHERE deleted_at IS NOT NULL` — GC sweep index (S-06).
- `idx_audit_outbox_pending ON audit_outbox(enqueued_at) WHERE emitted_at IS NULL` — drain-worker queue index (S-09).

Every `tenant_id` is stored in canonical UUIDv7 hyphenated lowercase text form. Every `digest` is stored in canonical `'algo:hex'` form (`'blake3:a1b2c3...'`).

## Why a trait abstraction over D1

The real Cloudflare D1 binding (`worker::D1Database`) only exists inside the Workers runtime. To keep the unit/integration test suite real (i.e. exercising the same code paths that run in production) we abstract the database behind the `MetaStore` trait. The crate ships an `InMemoryMetaStore` fake that preserves every documented semantic property (PRIMARY KEY enforcement, `INSERT OR IGNORE` idempotency, `(request_id, event_type)` UNIQUE dedupe, atomic batch); the real CF D1 shim lands alongside the REAPI handler in WI-S01-005.

## Atomicity model

D1 does **not** support `SELECT ... FOR UPDATE` (PostgreSQL/MySQL only). Multi-statement transactions over the Worker binding require `db.batch([stmt1, stmt2])` — D1 binds the batch to a single SQLite transaction. The trait surface mirrors this: every "commit" method takes both the blob_meta mutation **and** the audit_outbox event, and the impl lands them together (or rolls back together).

The `InMemoryMetaStore` fake simulates this with a `Mutex<Inner>` plus a "plan, then commit" pattern: the audit-outbox staging is plan-only (it can fail with `AuditIdempotencyConflict`), the blob_meta mutation runs next, and the audit-outbox row is committed only if both sides succeed. Failure on either side rolls back both.

## Quality gates

- `#![forbid(unsafe_code)]` — literal at the lib root **and** Cargo `[lints]`.
- `[lints.clippy]`: deny `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `todo`, `unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`, `mod_module_files`.
- 27 unit/integration tests (debug + release).
- 4 property tests at proptest default 10 000 iter / strategy:
  - `concurrent_commit_put_dedupes_outbox` — N concurrent first-writers; exactly one wins; outbox UNIQUE preserved.
  - `decrement_race_never_underflows` — N concurrent decrements over a refcount-K row; exactly `min(N, K)` succeed.
  - `audit_outbox_idempotency_strictness` — same `(request_id, event_type)` + same payload = no-op; same key + different payload = `AuditIdempotencyConflict`.
  - `cross_tenant_same_digest_creates_independent_rows` — per-tenant CAS per WI §9.5.
- `cargo-mutants` 100% kill rate (39/39 viable mutants caught).
- `cargo-fuzz` 60s smoke per target — 209k + 535k iterations clean, no crashes.
- `wasm32-unknown-unknown` builds clean.

## Anti-patterns (do NOT)

- **Do not** add a `force_insert` API that bypasses `INSERT OR IGNORE`. The idempotent semantics are the load-bearing seam (INV-CAS-IDEMPOTENCY).
- **Do not** mutate `digest` or `size_bytes` post-INSERT. The columns are write-once-then-read-only by convention; mutation is INV-CAS-IMMUTABILITY violation.
- **Do not** emit audit events outside the `commit_*` methods. Every `blob_meta` mutation must be paired with an `audit_outbox` row in the same batch (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
- **Do not** assume `SELECT ... FOR UPDATE` will work on D1. It will not. Use the trait methods.
