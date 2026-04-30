//! Host-side in-memory fake of [`crate::MetaStore`].
//!
//! ## Why this is more than a HashMap-with-extras
//!
//! The fake is the **canonical executable spec** of the trait's semantic
//! requirements:
//!
//! 1. **PRIMARY KEY enforcement** on `(tenant_id, digest)`: a second
//!    `commit_put` for the same key is a silent no-op (`InsertOutcome::AlreadyExists`).
//! 2. **CHECK constraints** (`refcount >= 0`, `size_bytes > 0`) enforced
//!    in code with explicit error variants.
//! 3. **Atomic batch semantics** simulated by acquiring the inner `Mutex`
//!    once per commit and dropping it only after BOTH writes succeed. A
//!    failure mid-batch (audit idempotency conflict) restores the prior
//!    state.
//! 4. **Audit-outbox UNIQUE `(request_id, event_type)`** enforced; retried
//!    commits with same `(request_id, event_type)` and identical payload
//!    are no-ops (idempotency); same key with **different** payload is
//!    [`crate::MetaError::AuditIdempotencyConflict`].
//! 5. **Tombstone semantics**: a tombstoned row's `deleted_at` is sticky;
//!    a second `commit_soft_delete` no-ops at the SQL layer (the WHERE
//!    clause filters tombstoned rows). Refcount mutations on a tombstoned
//!    row return [`crate::MetaError::Tombstoned`].
//!
//! Two concurrency-test helpers are exposed as test-only inspection
//! accessors ([`InMemoryMetaStore::row_count`], [`InMemoryMetaStore::outbox_snapshot`]).
//! They are not part of the [`crate::MetaStore`] trait surface — they exist
//! purely to let property tests assert system-wide invariants without
//! racing against the store.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::cas_query;
use crate::error::MetaError;
use crate::store::{
    CommitDecrementRequest, CommitPutRequest, CommitSoftDeleteRequest, MetaStore, OutboxRow,
};
use crate::types::{
    AuditEvent, AuditEventType, BlobMetaKey, BlobMetaRow, DecrementOutcome, InsertOutcome,
    RequestId, TenantId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredBlobRow {
    size_bytes: u64,
    refcount: u64,
    created_at_ms: u64,
    last_accessed_at_ms: u64,
    deleted_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredOutboxRow {
    id: Uuid,
    tenant: TenantId,
    digest: Option<String>,
    request_id: RequestId,
    event_type: AuditEventType,
    payload_json: String,
    enqueued_at_ms: u64,
    emitted_at_ms: Option<u64>,
}

#[derive(Debug, Default)]
struct Inner {
    /// `BTreeMap` keyed on canonical text forms — matches the D1 storage
    /// representation byte-for-byte. Using canonical text keys here means
    /// "two `BlobMetaKey` values with the same canonical text form" cannot
    /// produce two rows even if (somehow) the `Uuid`/`Digest` types had a
    /// hash collision against `Eq`. The fake's keying strategy mirrors
    /// what D1 uses on disk (TEXT PK).
    rows: BTreeMap<(String, String), StoredBlobRow>,
    /// `audit_outbox` rows. Keyed on `(request_id, event_type)` to enforce
    /// the UNIQUE constraint in O(log n).
    outbox: BTreeMap<(String, AuditEventType), StoredOutboxRow>,
}

/// In-memory fake [`MetaStore`] backed by a `BTreeMap<(text, text), Row>`
/// guarded by a `Mutex` for atomic batch semantics.
#[derive(Debug)]
pub struct InMemoryMetaStore {
    inner: Mutex<Inner>,
}

impl Default for InMemoryMetaStore {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }
}

impl InMemoryMetaStore {
    /// Construct a fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total `blob_meta` row count (alive + tombstoned).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.with_lock(|inner| inner.rows.len())
    }

    /// Snapshot every audit_outbox row, ordered by enqueue time. The
    /// canonical drain worker (S-09) consumes them in this order.
    #[must_use]
    pub fn outbox_snapshot(&self) -> Vec<OutboxRow> {
        self.with_lock(|inner| {
            let mut rows: Vec<_> = inner
                .outbox
                .values()
                .map(|s| OutboxRow {
                    id: s.id,
                    tenant_id: *s.tenant.as_uuid(),
                    digest: s.digest.clone(),
                    request_id: s.request_id.clone(),
                    event_type: s.event_type.as_str().to_owned(),
                    payload_json: s.payload_json.clone(),
                    enqueued_at_ms: s.enqueued_at_ms,
                    emitted_at_ms: s.emitted_at_ms,
                })
                .collect();
            rows.sort_by_key(|r| (r.enqueued_at_ms, r.id));
            rows
        })
    }

    /// Test-only helper: forcibly set `refcount` on an existing row.
    ///
    /// Exists so property tests in [`crate`] can simulate "row is at
    /// refcount=K" without going through the canonical increment path
    /// (which lives upstream — refcount increment via `commit_put` is
    /// first-write-only by spec; subsequent increments come from the
    /// CAS-write-with-existing-blob handler in WI-S01-005). Returns
    /// `true` iff a row at `key` existed and was updated.
    ///
    /// Not part of the public [`MetaStore`] trait. Hidden from rustdoc to
    /// keep the production API surface clean. Production code MUST NOT
    /// reach for this — it bypasses the audit-emission invariant.
    #[doc(hidden)]
    pub fn set_refcount_for_tests(&self, key: BlobMetaKey, refcount: u64) -> bool {
        let pk = key_pair(&key);
        self.with_lock(|inner| {
            if let Some(row) = inner.rows.get_mut(&pk) {
                row.refcount = refcount;
                true
            } else {
                false
            }
        })
    }

    /// Test-only helper: mark an outbox row as drained. Mirrors what the
    /// S-09 drain worker will do via a separate UPDATE.
    pub fn mark_outbox_drained(
        &self,
        request_id: &RequestId,
        event_type: AuditEventType,
        emitted_at_ms: u64,
    ) -> Result<bool, MetaError> {
        self.with_lock(|inner| {
            let key = (request_id.as_str().to_owned(), event_type);
            if let Some(row) = inner.outbox.get_mut(&key) {
                row.emitted_at_ms = Some(emitted_at_ms);
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    /// Read every load-bearing SQL template once, just to assert the impl
    /// is wired against the canonical templates. Used by the
    /// `schema_canonical` integration test as a smoke that the fake stays
    /// in lock-step with [`crate::cas_query`].
    #[must_use]
    pub fn load_bearing_query_templates() -> [&'static str; 7] {
        [
            cas_query::INSERT_BLOB_META,
            cas_query::UPDATE_BLOB_META_INCREMENT_REFCOUNT,
            cas_query::UPDATE_BLOB_META_DECREMENT_REFCOUNT,
            cas_query::UPDATE_BLOB_META_SOFT_DELETE,
            cas_query::SELECT_BLOB_META,
            cas_query::INSERT_AUDIT_OUTBOX,
            cas_query::SELECT_AUDIT_OUTBOX_BY_REQUEST,
        ]
    }

    fn with_lock<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        // Mutex poisoning indicates a panic inside another `with_lock`
        // closure; the fake is test infrastructure so we surface it as a
        // recoverable backend error in the trait-level methods. For the
        // public read-only helpers (row_count / outbox_snapshot) we
        // collapse it to the default — those methods do not return a
        // Result and a poisoned lock under test code is itself a test
        // failure.
        match self.inner.lock() {
            Ok(mut g) => f(&mut g),
            Err(p) => f(&mut p.into_inner()),
        }
    }

    fn with_lock_result<R>(
        &self,
        f: impl FnOnce(&mut Inner) -> Result<R, MetaError>,
    ) -> Result<R, MetaError> {
        match self.inner.lock() {
            Ok(mut g) => f(&mut g),
            Err(_) => Err(MetaError::Backend(
                "InMemoryMetaStore mutex poisoned".to_owned(),
            )),
        }
    }
}

fn key_pair(key: &BlobMetaKey) -> (String, String) {
    (
        key.tenant().to_canonical_text(),
        key.digest_canonical_text(),
    )
}

/// Plan-only check for the audit-outbox staging step. Returns:
/// - `Ok(StagePlan::FreshInsert(row))` — a new row to insert if the
///   blob_meta-side mutation succeeds.
/// - `Ok(StagePlan::IdempotentNoOp)` — an existing row already present
///   with identical payload; the audit side is a no-op for this batch.
/// - `Err(AuditIdempotencyConflict)` — request_id reused with a different
///   payload; abort the entire batch (including any blob_meta mutation
///   that was about to land).
///
/// Plan-only means: this function MUST NOT mutate `inner.outbox`. The
/// caller commits the row only if the blob_meta-side mutation succeeded.
/// Splitting plan from commit is the load-bearing detail that gives the
/// fake true atomic-batch semantics: a NotFound/Tombstoned/Underflow on
/// the blob_meta side never leaves an orphan audit row.
fn plan_outbox_stage(inner: &Inner, audit: &AuditEvent) -> Result<StagePlan, MetaError> {
    let key = (audit.request_id.as_str().to_owned(), audit.event_type);
    if let Some(existing) = inner.outbox.get(&key) {
        if existing.payload_json == audit.payload_json {
            return Ok(StagePlan::IdempotentNoOp);
        }
        return Err(MetaError::AuditIdempotencyConflict);
    }
    Ok(StagePlan::FreshInsert(key))
}

enum StagePlan {
    FreshInsert((String, AuditEventType)),
    IdempotentNoOp,
}

fn commit_outbox_stage(
    inner: &mut Inner,
    plan: StagePlan,
    audit: &AuditEvent,
    tenant: TenantId,
    digest: Option<String>,
    enqueued_at_ms: u64,
) {
    if let StagePlan::FreshInsert(key) = plan {
        inner.outbox.insert(
            key,
            StoredOutboxRow {
                id: audit.id,
                tenant,
                digest,
                request_id: audit.request_id.clone(),
                event_type: audit.event_type,
                payload_json: audit.payload_json.clone(),
                enqueued_at_ms,
                emitted_at_ms: None,
            },
        );
    }
}

impl MetaStore for InMemoryMetaStore {
    async fn commit_put(&self, request: CommitPutRequest) -> Result<InsertOutcome, MetaError> {
        if request.size_bytes == 0 {
            // The schema's CHECK (size_bytes > 0) would surface this as a
            // SQL constraint violation; the fake mirrors it as a Backend
            // error so behavior is identical between fake and D1.
            return Err(MetaError::Backend(
                "size_bytes must be > 0 (schema CHECK)".to_owned(),
            ));
        }

        let pk = key_pair(&request.key);
        let tenant = *request.key.tenant();
        let digest_text = request.key.digest_canonical_text();

        // The `with_lock_result` closure simulates the D1 `db.batch([...])`
        // atomic envelope. The batch is "plan, then commit" so the
        // blob_meta and audit_outbox sides land or roll back together —
        // even when the failure originates on the blob_meta side.
        self.with_lock_result(|inner| {
            // Step 1 — plan-only check on the audit row. Surfaces
            // AuditIdempotencyConflict immediately; never mutates yet.
            let plan = plan_outbox_stage(inner, &request.audit)?;

            // Step 2 — apply blob_meta INSERT OR IGNORE. Cannot fail
            // post-plan (size_bytes > 0 already checked above; INSERT OR
            // IGNORE is total).
            let outcome = match inner.rows.entry(pk) {
                std::collections::btree_map::Entry::Occupied(_) => InsertOutcome::AlreadyExists,
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(StoredBlobRow {
                        size_bytes: request.size_bytes,
                        refcount: 1,
                        created_at_ms: request.now_ms,
                        last_accessed_at_ms: request.now_ms,
                        deleted_at_ms: None,
                    });
                    InsertOutcome::Inserted
                }
            };

            // Step 3 — commit the audit row (no-op if idempotent retry).
            commit_outbox_stage(
                inner,
                plan,
                &request.audit,
                tenant,
                Some(digest_text.clone()),
                request.now_ms,
            );
            Ok(outcome)
        })
    }

    async fn commit_decrement(
        &self,
        request: CommitDecrementRequest,
    ) -> Result<DecrementOutcome, MetaError> {
        let pk = key_pair(&request.key);
        let tenant = *request.key.tenant();
        let digest_text = request.key.digest_canonical_text();

        self.with_lock_result(|inner| {
            // Step 1 — plan-only check on the audit row.
            let plan = plan_outbox_stage(inner, &request.audit)?;

            // Step 2 — attempt the UPDATE. Failures here roll back BOTH
            // blob_meta (no-op; we never wrote yet) and outbox (we never
            // committed the planned row).
            let row = inner.rows.get_mut(&pk).ok_or(MetaError::NotFound)?;
            if row.deleted_at_ms.is_some() {
                return Err(MetaError::Tombstoned);
            }
            if row.refcount == 0 {
                return Err(MetaError::RefcountUnderflow);
            }
            row.refcount -= 1;
            row.last_accessed_at_ms = request.now_ms;
            let outcome = if row.refcount == 0 {
                DecrementOutcome::ReachedZero
            } else {
                DecrementOutcome::Decremented {
                    new_refcount: row.refcount,
                }
            };

            // Step 3 — commit outbox.
            commit_outbox_stage(
                inner,
                plan,
                &request.audit,
                tenant,
                Some(digest_text.clone()),
                request.now_ms,
            );
            Ok(outcome)
        })
    }

    async fn commit_soft_delete(&self, request: CommitSoftDeleteRequest) -> Result<(), MetaError> {
        let pk = key_pair(&request.key);
        let tenant = *request.key.tenant();
        let digest_text = request.key.digest_canonical_text();

        self.with_lock_result(|inner| {
            let plan = plan_outbox_stage(inner, &request.audit)?;

            // Idempotent soft-delete: WHERE deleted_at IS NULL clause means
            // a re-tombstone is a no-op at the SQL layer; we surface that
            // as Ok(()) (the user's intent — "this row is dead" — is
            // already satisfied).
            if let Some(row) = inner.rows.get_mut(&pk) {
                if row.deleted_at_ms.is_none() {
                    row.deleted_at_ms = Some(request.now_ms);
                }
                commit_outbox_stage(
                    inner,
                    plan,
                    &request.audit,
                    tenant,
                    Some(digest_text.clone()),
                    request.now_ms,
                );
                Ok(())
            } else {
                Err(MetaError::NotFound)
            }
        })
    }

    async fn get(&self, key: &BlobMetaKey) -> Result<Option<BlobMetaRow>, MetaError> {
        let pk = key_pair(key);
        self.with_lock_result(|inner| {
            Ok(inner.rows.get(&pk).map(|s| BlobMetaRow {
                key: *key,
                size_bytes: s.size_bytes,
                refcount: s.refcount,
                created_at_ms: s.created_at_ms,
                last_accessed_at_ms: s.last_accessed_at_ms,
                deleted_at_ms: s.deleted_at_ms,
            }))
        })
    }
}
