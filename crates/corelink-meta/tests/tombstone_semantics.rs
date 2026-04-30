//! Tombstone semantics tests (WI-S01-004 §8 AC: soft-delete + tombstone).
//!
//! Properties verified:
//! 1. `commit_soft_delete` sets `deleted_at` exactly once. A second call
//!    is a silent no-op (the WHERE clause filters tombstoned rows). The
//!    `deleted_at` value is NOT overwritten on retries.
//! 2. After tombstoning, `commit_decrement` returns
//!    [`MetaError::Tombstoned`] — incrementing/decrementing a tombstoned
//!    blob is INV-CAS-IMMUTABILITY violation territory and rejected at
//!    the SQL level.
//! 3. Tombstoned rows are still readable via `get` (returns the row with
//!    `deleted_at_ms = Some(_)`); the `is_alive()` accessor flips to
//!    false.
//! 4. After tombstoning, a subsequent `commit_put` for the SAME
//!    `(tenant_id, digest)` is a no-op (`AlreadyExists`) — the row is
//!    still there, tombstoned. No resurrection.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]

use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, BlobMetaKey, CommitDecrementRequest, CommitPutRequest, CommitSoftDeleteRequest,
    InMemoryMetaStore, InsertOutcome, MetaError, MetaStore,
};
use uuid::Uuid;

const T_TEXT: &str = "01938af0-abcd-7123-8456-000000000007";
const D_HEX: &str = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";

fn fresh() -> (InMemoryMetaStore, BlobMetaKey) {
    let store = InMemoryMetaStore::new();
    let key = BlobMetaKey::new(
        Uuid::parse_str(T_TEXT).unwrap(),
        Digest::from_hex(D_HEX).unwrap(),
    );
    (store, key)
}

fn put(seq: u64) -> CommitPutRequest {
    let (_, key) = fresh();
    CommitPutRequest {
        key,
        size_bytes: 1024,
        now_ms: 1_000 + seq,
        audit: AuditEvent::cas_put_completed(
            Uuid::from_u128(seq as u128),
            format!("put-{seq}"),
            format!(r#"{{"seq":{seq}}}"#),
        ),
    }
}

fn dec(seq: u64) -> CommitDecrementRequest {
    let (_, key) = fresh();
    CommitDecrementRequest {
        key,
        now_ms: 2_000 + seq,
        audit: AuditEvent::cas_refcount_decremented(
            Uuid::from_u128((seq + 100) as u128),
            format!("dec-{seq}"),
            format!(r#"{{"seq":{seq}}}"#),
        ),
    }
}

fn tomb(seq: u64) -> CommitSoftDeleteRequest {
    let (_, key) = fresh();
    CommitSoftDeleteRequest {
        key,
        now_ms: 3_000 + seq,
        audit: AuditEvent::cas_soft_deleted(
            Uuid::from_u128((seq + 200) as u128),
            format!("tomb-{seq}"),
            format!(r#"{{"seq":{seq}}}"#),
        ),
    }
}

#[tokio::test]
async fn first_soft_delete_sets_deleted_at() {
    let (store, key) = fresh();
    let outcome = store.commit_put(put(1)).await.unwrap();
    assert_eq!(outcome, InsertOutcome::Inserted);
    store.commit_soft_delete(tomb(1)).await.unwrap();
    let row = store.get(&key).await.unwrap().unwrap();
    assert_eq!(row.deleted_at_ms, Some(3_001));
    assert!(!row.is_alive());
}

#[tokio::test]
async fn second_soft_delete_is_no_op_does_not_overwrite_deleted_at() {
    let (store, key) = fresh();
    store.commit_put(put(1)).await.unwrap();
    store.commit_soft_delete(tomb(1)).await.unwrap();
    let first_deleted_at = store.get(&key).await.unwrap().unwrap().deleted_at_ms;
    // Second tombstone: NOT a conflict; no-op.
    store.commit_soft_delete(tomb(2)).await.unwrap();
    let second_deleted_at = store.get(&key).await.unwrap().unwrap().deleted_at_ms;
    assert_eq!(
        first_deleted_at, second_deleted_at,
        "deleted_at is sticky; not overwritten by retry"
    );
    // Three audit emissions ARE recorded — distinct request_ids → distinct
    // outbox rows (one for the initial put + two for the soft-delete
    // retries). Each retry carries its OWN client-supplied request_id, so
    // the UNIQUE (request_id, event_type) constraint does not collapse
    // them. The drain worker (S-09) will see all three events; consumer
    // dedupe is a downstream concern.
    assert_eq!(store.outbox_snapshot().len(), 3);
}

#[tokio::test]
async fn decrement_after_tombstone_returns_tombstoned_error() {
    let (store, _key) = fresh();
    store.commit_put(put(1)).await.unwrap();
    store.commit_soft_delete(tomb(1)).await.unwrap();
    let err = store.commit_decrement(dec(1)).await.unwrap_err();
    assert!(matches!(err, MetaError::Tombstoned));
}

#[tokio::test]
async fn put_after_tombstone_is_already_exists_no_resurrection() {
    let (store, key) = fresh();
    store.commit_put(put(1)).await.unwrap();
    store.commit_soft_delete(tomb(1)).await.unwrap();
    // Another commit_put for the same (tenant, digest) post-tombstone
    // returns AlreadyExists — INSERT OR IGNORE no-ops; deleted_at stays
    // populated. No resurrection.
    let outcome = store.commit_put(put(2)).await.unwrap();
    assert_eq!(outcome, InsertOutcome::AlreadyExists);
    let row = store.get(&key).await.unwrap().unwrap();
    assert!(!row.is_alive(), "still tombstoned");
    assert_eq!(row.deleted_at_ms, Some(3_001));
}

#[tokio::test]
async fn soft_delete_missing_row_returns_not_found() {
    let store = InMemoryMetaStore::new();
    let err = store.commit_soft_delete(tomb(1)).await.unwrap_err();
    assert!(matches!(err, MetaError::NotFound));
    // No partial mutation: outbox is empty AFTER the abort. (The
    // commit_soft_delete impl stages outbox first then attempts the
    // UPDATE; if UPDATE fails we must roll back outbox too.)
    //
    // The current impl stages outbox FIRST and then surfaces NotFound,
    // which would leave an orphan audit row. The test below pins the
    // contract: NotFound aborts the entire batch including outbox.
    assert_eq!(
        store.outbox_snapshot().len(),
        0,
        "audit must NOT be staged when blob_meta side fails"
    );
}

#[tokio::test]
async fn decrement_missing_row_returns_not_found_atomic() {
    let store = InMemoryMetaStore::new();
    let err = store.commit_decrement(dec(1)).await.unwrap_err();
    assert!(matches!(err, MetaError::NotFound));
    assert_eq!(
        store.outbox_snapshot().len(),
        0,
        "audit must NOT be staged when blob_meta side fails"
    );
}
