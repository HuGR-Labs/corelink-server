//! [`MetaStore`] trait — the canonical metadata-store contract.
//!
//! The trait is the seam between the REAPI handler (WI-S01-005) and the
//! D1 backing store. Two implementors live in this crate today:
//!
//! - [`crate::InMemoryMetaStore`] — host-side test fake; preserves every
//!   semantic property the trait advertises (atomic batch, idempotent
//!   INSERT, audit-outbox dedupe).
//! - The real Cloudflare D1 binding adapter (defer: WI-S01-005). It will
//!   live in `corelink-worker` and dispatch the [`crate::cas_query`]
//!   templates against `worker::D1Database::batch(...)`.
//!
//! ## Atomic batch semantics
//!
//! Every "commit" method on the trait MUST persist its blob_meta mutation
//! AND its audit_outbox row in a **single atomic transaction**. On D1 this
//! is `db.batch([stmt1, stmt2])` (D1 binds it to a single SQLite
//! transaction); on the in-memory fake we hold the inner `Mutex` across
//! both writes.
//!
//! Failure of the audit_outbox INSERT (e.g. payload-mismatch under retry)
//! MUST roll back the blob_meta change. This is the load-bearing
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER guarantee — **no orphan blob_meta
//! mutations**, **no audit gaps**.

use core::future::Future;

use crate::error::MetaError;
use crate::types::{
    AuditEvent, BlobMetaKey, BlobMetaRow, DecrementOutcome, InsertOutcome, RequestId,
};

/// Snapshot of a row from the `audit_outbox` table — useful for tests and
/// for the drain worker (S-09).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboxRow {
    /// `audit_outbox.id` (UUIDv7 text).
    pub id: uuid::Uuid,
    /// `audit_outbox.tenant_id` (UUIDv7 text).
    pub tenant_id: uuid::Uuid,
    /// `audit_outbox.digest` — `None` for non-blob events.
    pub digest: Option<String>,
    /// `audit_outbox.request_id`.
    pub request_id: RequestId,
    /// `audit_outbox.event_type`.
    pub event_type: String,
    /// `audit_outbox.payload_json`.
    pub payload_json: String,
    /// `audit_outbox.enqueued_at` (unix ms).
    pub enqueued_at_ms: u64,
    /// `audit_outbox.emitted_at` — `None` while the row is pending drain.
    pub emitted_at_ms: Option<u64>,
}

/// Argument bundle for [`MetaStore::commit_put`] — first-time write for a
/// `(tenant_id, digest)` pair.
#[derive(Clone, Debug)]
pub struct CommitPutRequest {
    /// Composite primary key.
    pub key: BlobMetaKey,
    /// Body size in bytes; must be `> 0` (CHECK constraint).
    pub size_bytes: u64,
    /// Unix epoch ms timestamp; written to both `created_at` and
    /// `last_accessed_at`.
    pub now_ms: u64,
    /// Audit envelope persisted atomically with the row.
    pub audit: AuditEvent,
}

/// Argument bundle for [`MetaStore::commit_decrement`].
#[derive(Clone, Debug)]
pub struct CommitDecrementRequest {
    /// Composite primary key.
    pub key: BlobMetaKey,
    /// Unix epoch ms timestamp; updates `last_accessed_at`.
    pub now_ms: u64,
    /// Audit envelope persisted atomically with the decrement.
    pub audit: AuditEvent,
}

/// Argument bundle for [`MetaStore::commit_soft_delete`].
#[derive(Clone, Debug)]
pub struct CommitSoftDeleteRequest {
    /// Composite primary key.
    pub key: BlobMetaKey,
    /// Unix epoch ms timestamp; written to `deleted_at`.
    pub now_ms: u64,
    /// Audit envelope persisted atomically with the soft-delete.
    pub audit: AuditEvent,
}

/// Canonical metadata-store contract.
///
/// Every method that mutates `blob_meta` carries an [`AuditEvent`] that is
/// persisted in the same atomic batch — INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
/// is structurally enforced at the trait level.
pub trait MetaStore: Send + Sync {
    /// Idempotent first-time INSERT plus audit-outbox staging, atomic.
    ///
    /// Returns:
    /// - [`InsertOutcome::Inserted`] on first write (refcount = 1).
    /// - [`InsertOutcome::AlreadyExists`] when the `(tenant_id, digest)` row
    ///   already exists. The audit_outbox row is still de-duplicated via
    ///   the `(request_id, event_type)` UNIQUE constraint, so retried
    ///   commits with the same idempotency key never double-emit.
    ///
    /// Errors: [`MetaError::Backend`] on transport faults;
    /// [`MetaError::AuditIdempotencyConflict`] if the same
    /// `(request_id, event_type)` already exists with a different
    /// `payload_json`.
    fn commit_put<'a>(
        &'a self,
        request: CommitPutRequest,
    ) -> impl Future<Output = Result<InsertOutcome, MetaError>> + Send + 'a;

    /// Atomic refcount decrement (with audit emission).
    ///
    /// Returns [`DecrementOutcome::Decremented { new_refcount }`] when the
    /// new value is `> 0`, [`DecrementOutcome::ReachedZero`] when it lands
    /// at exactly zero. On a tombstoned row returns
    /// [`MetaError::Tombstoned`]; on a missing row [`MetaError::NotFound`];
    /// on a row whose refcount is already zero [`MetaError::RefcountUnderflow`].
    fn commit_decrement<'a>(
        &'a self,
        request: CommitDecrementRequest,
    ) -> impl Future<Output = Result<DecrementOutcome, MetaError>> + Send + 'a;

    /// Idempotent soft-delete (tombstoning), atomic with audit emission.
    ///
    /// First call sets `deleted_at = now_ms`; subsequent calls are silent
    /// no-ops at the SQL layer (the WHERE clause filters tombstoned rows).
    /// The audit_outbox row IS staged on the no-op path too — but is
    /// de-duplicated via `(request_id, event_type)` UNIQUE so retries do
    /// not double-emit.
    fn commit_soft_delete<'a>(
        &'a self,
        request: CommitSoftDeleteRequest,
    ) -> impl Future<Output = Result<(), MetaError>> + Send + 'a;

    /// Read-only PK lookup. Returns the row regardless of `deleted_at`
    /// state; callers that want only alive rows filter
    /// [`BlobMetaRow::is_alive`] themselves.
    fn get<'a>(
        &'a self,
        key: &'a BlobMetaKey,
    ) -> impl Future<Output = Result<Option<BlobMetaRow>, MetaError>> + Send + 'a;
}
