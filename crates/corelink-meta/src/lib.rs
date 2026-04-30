//! CoreLink CAS metadata layer (S-01 / WI-S01-004).
//!
//! Implements the **D1 (Cloudflare SQLite) `blob_meta` + `audit_outbox`**
//! transactional store: idempotent INSERT, atomic refcount increment/decrement,
//! soft-delete tombstones, and the outbox-pattern audit emission that pairs
//! atomically with every metadata mutation (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//!
//! ## Architectural seam
//!
//! The crate sits between the R2 single-blob adapter
//! ([`corelink-worker`](https://docs.rs/corelink-worker)) and the REAPI handler
//! that lands in WI-S01-005. The handler wires:
//!
//! ```text
//!     VerifiedBody (corelink-hash)
//!            │
//!            ▼
//!     R2Writer.put             ← INV-CAS-INTEGRITY (write-time hash verify),
//!  (corelink-worker, S-01-003)    INV-CAS-IMMUTABILITY (`If-None-Match: *`)
//!            │
//!            ▼
//!     MetaStore.commit_put     ← THIS CRATE (S-01-004)
//!  (atomic D1 batch:             — `INSERT OR IGNORE blob_meta`
//!   blob_meta + audit_outbox)    — INSERT audit_outbox `corelink.cas.put_completed`
//! ```
//!
//! ## Why a trait abstraction
//!
//! The real Cloudflare D1 binding (`worker::D1Database`) only exists inside
//! the Workers runtime; it cannot be exercised under host-side `cargo test`.
//! [`MetaStore`] is the minimal trait that the REAPI handler depends on; the
//! crate ships an [`InMemoryMetaStore`] fake that preserves every documented
//! semantic (PRIMARY KEY enforcement, `INSERT OR IGNORE` idempotency,
//! `(request_id, event_type)` UNIQUE dedupe, atomic batch). The real CF D1
//! shim lands in WI-S01-005 alongside miniflare-backed integration tests.
//!
//! ## Safety properties
//!
//! - `#![forbid(unsafe_code)]` (literal + Cargo `[lints]`).
//! - All operations parameterized — never string-format user input into SQL.
//!   The `cas_query` helpers expose a stable, prepared-statement-style API
//!   so the production CF binding adapter can `.bind(...)` directly.
//! - `(tenant_id, digest)` is the canonical PK; cross-tenant reads of a
//!   different tenant's digest cannot succeed (rows are keyed under different
//!   PKs).
//! - `audit_outbox` UNIQUE `(request_id, event_type)` makes retried inserts
//!   safe-by-construction: a second commit with the same request_id is a
//!   no-op (the outer batch is rolled back as a unit, so the blob_meta
//!   counterpart is also re-applied via `INSERT OR IGNORE`).
//! - SQLite **does not support** `SELECT ... FOR UPDATE` (PostgreSQL-only).
//!   Refcount race prevention rides on D1's batch-as-transaction semantics
//!   plus the single-statement atomic `UPDATE ... SET refcount = refcount + 1
//!   ... RETURNING refcount` pattern (WI §2 mitigation 2). The
//!   [`InMemoryMetaStore`] fake faithfully simulates this with a `Mutex`.
//!
//! ## Quickstart
//!
//! ```rust
//! use corelink_hash::Digest;
//! use corelink_meta::{
//!     AuditEvent, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, InsertOutcome,
//!     MetaStore,
//! };
//! use uuid::Uuid;
//!
//! # async fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let store = InMemoryMetaStore::new();
//! let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001")?;
//! let digest = Digest::from_hex(
//!     "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
//! )?;
//! let key = BlobMetaKey::new(tenant, digest);
//!
//! let outcome = store
//!     .commit_put(CommitPutRequest {
//!         key,
//!         size_bytes: 5_000,
//!         now_ms: 1_700_000_000_000,
//!         audit: AuditEvent::cas_put_completed(
//!             Uuid::parse_str("01938af0-abcd-7222-8456-000000000002")?,
//!             "request-id-001",
//!             r#"{"specversion":"1.0","type":"corelink.cas.put_completed"}"#,
//!         ),
//!     })
//!     .await?;
//! assert_eq!(outcome, InsertOutcome::Inserted);
//!
//! let row = store.get(&key).await?.expect("just inserted");
//! assert_eq!(row.refcount, 1);
//! assert_eq!(row.size_bytes, 5_000);
//! # Ok(()) }
//! ```
//!
//! ## Anti-patterns (do NOT)
//!
//! - **Do not** add a `force_insert` API that bypasses `INSERT OR IGNORE`.
//!   The idempotent semantics are the load-bearing seam (INV-CAS-IDEMPOTENCY).
//! - **Do not** mutate `digest` or `size_bytes` post-INSERT. The columns are
//!   write-once-then-read-only by convention; mutation is INV-CAS-IMMUTABILITY
//!   violation. (Refcount and `last_accessed_at` are mutable; `deleted_at`
//!   is set at most once via `soft_delete`.)
//! - **Do not** emit audit events outside [`MetaStore::commit_put`] /
//!   [`MetaStore::commit_decrement`] / [`MetaStore::commit_soft_delete`].
//!   Any mutation to `blob_meta` must be paired with an audit_outbox row in
//!   the same batch (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//! - **Do not** assume `SELECT ... FOR UPDATE` will work on D1. It will not
//!   (SQLite has no row-locking SELECT). Use the trait methods, which encode
//!   the correct atomic-update pattern.

#![forbid(unsafe_code)]

pub mod cas_query;
pub mod error;
pub mod fake;
pub mod schema;
pub mod store;
pub mod types;

pub use error::MetaError;
pub use fake::InMemoryMetaStore;
pub use schema::{MIGRATION_FILE_PATH, MIGRATION_SQL};
pub use store::{
    CommitDecrementRequest, CommitPutRequest, CommitSoftDeleteRequest, MetaStore, OutboxRow,
};
pub use types::{
    AuditEvent, AuditEventType, BlobMetaKey, BlobMetaRow, DecrementOutcome, InsertOutcome,
    RequestId, TenantId,
};
