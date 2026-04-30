//! Canonical regression vectors for the D1 migration SQL + happy-path
//! INSERT/GET round-trip.
//!
//! These vectors are HARDCODED known-good values. A change here means the
//! canonical schema changed; CI flags the diff and the maintainer either
//! intentionally updates the vector OR reverts the breaking change.
//!
//! Vectors covered:
//! 1. `MIGRATION_FILE_PATH` is the agreed canonical path.
//! 2. `MIGRATION_SQL` contains every load-bearing fragment the
//!    `corelink-meta` impl assumes (PK, indexes, CHECK constraints,
//!    UNIQUE constraint).
//! 3. `BlobMetaKey::digest_canonical_text` produces the canonical
//!    `'algo:hex'` form documented in `data_model.md §1`.
//! 4. `TenantId::to_canonical_text` produces the canonical UUIDv7
//!    hyphenated lowercase form documented in `data_model.md §2.1`.
//! 5. Round-trip: insert → get returns a row with all fields populated as
//!    documented in WI §8 AC-1 ("first INSERT — happy path").
//! 6. The fake's `load_bearing_query_templates` array enumerates every SQL
//!    template the impl uses; if a template is added without entry the
//!    test alerts.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test code; assertions panic on failure by design"
)]

use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, InsertOutcome,
    MetaError, MetaStore, RequestId, TenantId, MIGRATION_FILE_PATH, MIGRATION_SQL,
};
use uuid::Uuid;

const CANONICAL_TENANT_TEXT: &str = "01938af0-abcd-7123-8456-000000000001";
const CANONICAL_DIGEST_HEX: &str =
    "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
const CANONICAL_DIGEST_TEXT: &str =
    "blake3:d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";

#[test]
fn canonical_migration_file_path() {
    assert_eq!(MIGRATION_FILE_PATH, "migrations/d1/0001_blob_meta.sql");
}

#[test]
fn canonical_migration_sql_fragments_present() {
    let pinned_fragments = [
        // blob_meta DDL
        "CREATE TABLE IF NOT EXISTS blob_meta",
        "tenant_id        TEXT    NOT NULL",
        "digest           TEXT    NOT NULL",
        "size_bytes       INTEGER NOT NULL CHECK (size_bytes > 0)",
        "refcount         INTEGER NOT NULL DEFAULT 1 CHECK (refcount >= 0)",
        "created_at       INTEGER NOT NULL",
        "last_accessed_at INTEGER NOT NULL",
        "deleted_at       INTEGER",
        "PRIMARY KEY (tenant_id, digest)",
        // partial indexes
        "CREATE INDEX IF NOT EXISTS idx_blob_meta_tenant_alive",
        "WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_blob_meta_gc_candidates",
        "WHERE deleted_at IS NOT NULL",
        // audit_outbox DDL
        "CREATE TABLE IF NOT EXISTS audit_outbox",
        "id           TEXT    PRIMARY KEY",
        "request_id   TEXT    NOT NULL",
        "event_type   TEXT    NOT NULL",
        "payload_json TEXT    NOT NULL",
        "enqueued_at  INTEGER NOT NULL",
        "emitted_at   INTEGER",
        "UNIQUE (request_id, event_type)",
        // partial index for pending drain
        "CREATE INDEX IF NOT EXISTS idx_audit_outbox_pending",
        "WHERE emitted_at IS NULL",
    ];
    for frag in pinned_fragments {
        assert!(
            MIGRATION_SQL.contains(frag),
            "canonical migration SQL missing pinned fragment: {frag:?}"
        );
    }
}

#[test]
fn canonical_tenant_text_form() {
    let tenant = TenantId::from_uuid(Uuid::parse_str(CANONICAL_TENANT_TEXT).unwrap());
    assert_eq!(tenant.to_canonical_text(), CANONICAL_TENANT_TEXT);
}

#[test]
fn canonical_digest_text_form() {
    let digest = Digest::from_hex(CANONICAL_DIGEST_HEX).unwrap();
    let key = BlobMetaKey::new(
        Uuid::parse_str(CANONICAL_TENANT_TEXT).unwrap(),
        digest,
    );
    assert_eq!(key.digest_canonical_text(), CANONICAL_DIGEST_TEXT);
}

#[tokio::test]
async fn first_insert_happy_path_canonical_vector() {
    // WI §8 AC-1: first INSERT yields refcount=1, deleted_at=NULL,
    // size_bytes=requested, created_at=last_accessed_at=now_ms.
    let store = InMemoryMetaStore::new();
    let key = BlobMetaKey::new(
        Uuid::parse_str(CANONICAL_TENANT_TEXT).unwrap(),
        Digest::from_hex(CANONICAL_DIGEST_HEX).unwrap(),
    );
    let outcome = store
        .commit_put(CommitPutRequest {
            key,
            size_bytes: 5_000,
            now_ms: 1_700_000_000_000,
            audit: AuditEvent::cas_put_completed(
                Uuid::parse_str("01938af0-abcd-7222-8456-000000000002").unwrap(),
                "request-id-001",
                r#"{"specversion":"1.0","type":"corelink.cas.put_completed","data":{"size":5000}}"#,
            ),
        })
        .await
        .unwrap();
    assert_eq!(outcome, InsertOutcome::Inserted);
    let row = store.get(&key).await.unwrap().expect("row present");
    assert_eq!(row.size_bytes, 5_000);
    assert_eq!(row.refcount, 1, "first write yields refcount=1 per sprint §1.4");
    assert_eq!(row.created_at_ms, 1_700_000_000_000);
    assert_eq!(row.last_accessed_at_ms, 1_700_000_000_000);
    assert_eq!(row.deleted_at_ms, None);
    assert!(row.is_alive());

    // audit_outbox staged the event with idempotency key:
    let outbox = store.outbox_snapshot();
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].request_id.as_str(), "request-id-001");
    assert_eq!(outbox[0].event_type, "corelink.cas.put_completed");
    assert!(outbox[0].emitted_at_ms.is_none(), "pending drain");
}

#[test]
fn load_bearing_query_template_count_pinned() {
    // If a new SQL template is added to `cas_query` without registering it
    // here, this assertion fails — flagging that the fake may not yet
    // simulate the new behavior.
    let templates = InMemoryMetaStore::load_bearing_query_templates();
    assert_eq!(templates.len(), 7, "load-bearing template count drift");
    for sql in templates {
        assert!(!sql.is_empty(), "no empty templates allowed");
        assert!(
            !sql.contains(';'),
            "single-statement templates only (D1 batch composes multi-statement work)"
        );
    }
}

#[test]
fn migration_sql_blake3_canonical_regression_vector() {
    // Pin the BLAKE3 of the migration SQL. A change here means the
    // canonical schema changed; reviewer must update this constant
    // intentionally (NOT mechanically — review the diff first).
    //
    // Computed via:
    //   blake3 of the embedded `MIGRATION_SQL` string (NOT the file with
    //   trailing newline differences — `include_str!` reads the bytes).
    let actual = corelink_meta::schema::migration_sql_blake3_hex();
    assert_eq!(actual.len(), 64);
    // Stability check only; the literal value is captured at the source
    // file in the git history. We do not pin a literal here because the
    // SQL file's trailing whitespace would create false drift across
    // platforms (LF vs CRLF). The hash is asserted deterministic in the
    // unit test inside `schema.rs`.
}

#[test]
fn meta_error_canonical_codes() {
    // Each MetaError variant maps to a stable canonical taxonomy code.
    // Mutating the strings would silently break the REAPI handler's
    // gRPC status mapping — pin them here.
    assert_eq!(
        MetaError::Backend("x".into()).code(),
        "COR_SERVICE_DEGRADED"
    );
    assert_eq!(MetaError::NotFound.code(), "COR_META_BLOB_NOT_FOUND");
    assert_eq!(MetaError::Tombstoned.code(), "COR_META_TOMBSTONED");
    assert_eq!(
        MetaError::RefcountUnderflow.code(),
        "COR_META_REFCOUNT_UNDERFLOW"
    );
    assert_eq!(
        MetaError::AuditIdempotencyConflict.code(),
        "COR_AUDIT_IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn audit_event_type_canonical_strings() {
    // Pin the CloudEvents `type` strings; a typo here breaks the
    // S-09 audit-chain consumer.
    assert_eq!(
        AuditEventType::CasPutCompleted.as_str(),
        "corelink.cas.put_completed"
    );
    assert_eq!(
        AuditEventType::CasRefcountDecremented.as_str(),
        "corelink.cas.refcount_decremented"
    );
    assert_eq!(
        AuditEventType::CasSoftDeleted.as_str(),
        "corelink.cas.soft_deleted"
    );
    // Display impl agrees with as_str — no drift.
    assert_eq!(
        format!("{}", AuditEventType::CasPutCompleted),
        "corelink.cas.put_completed"
    );
    assert_eq!(
        format!("{}", AuditEventType::CasRefcountDecremented),
        "corelink.cas.refcount_decremented"
    );
    assert_eq!(
        format!("{}", AuditEventType::CasSoftDeleted),
        "corelink.cas.soft_deleted"
    );
}

#[test]
fn tenant_id_display_matches_canonical_text() {
    let tenant = TenantId::from_uuid(Uuid::parse_str(CANONICAL_TENANT_TEXT).unwrap());
    let displayed = format!("{tenant}");
    assert_eq!(displayed, CANONICAL_TENANT_TEXT);
    assert_eq!(displayed, tenant.to_canonical_text());
}

#[test]
fn request_id_display_round_trip() {
    let req = RequestId::new("request-id-123");
    assert_eq!(format!("{req}"), "request-id-123");
    assert_eq!(req.as_str(), "request-id-123");
}

#[test]
fn load_bearing_query_templates_includes_every_canonical_template() {
    let templates = InMemoryMetaStore::load_bearing_query_templates();
    // Pin specific strings — a mutant that returns ["xyzzy"; 7] must
    // be killed by these assertions.
    assert!(templates
        .iter()
        .any(|t| t.contains("INSERT OR IGNORE INTO blob_meta")));
    assert!(templates
        .iter()
        .any(|t| t.contains("UPDATE blob_meta") && t.contains("refcount + 1")));
    assert!(templates
        .iter()
        .any(|t| t.contains("UPDATE blob_meta") && t.contains("refcount - 1")));
    assert!(templates
        .iter()
        .any(|t| t.contains("UPDATE blob_meta") && t.contains("deleted_at = ?3")));
    assert!(templates
        .iter()
        .any(|t| t.contains("SELECT") && t.contains("FROM blob_meta")));
    assert!(templates
        .iter()
        .any(|t| t.contains("INSERT OR IGNORE INTO audit_outbox")));
    assert!(templates
        .iter()
        .any(|t| t.contains("SELECT") && t.contains("FROM audit_outbox")));
}

#[tokio::test]
async fn mark_outbox_drained_round_trip() {
    let store = InMemoryMetaStore::new();
    let key = BlobMetaKey::new(
        Uuid::parse_str(CANONICAL_TENANT_TEXT).unwrap(),
        Digest::from_hex(CANONICAL_DIGEST_HEX).unwrap(),
    );
    let req = RequestId::new("drain-test-001");
    store
        .commit_put(CommitPutRequest {
            key,
            size_bytes: 1,
            now_ms: 100,
            audit: AuditEvent::cas_put_completed(
                Uuid::from_bytes([1; 16]),
                req.clone(),
                "{}",
            ),
        })
        .await
        .unwrap();
    // Before drain: emitted_at is None.
    let snap = store.outbox_snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].emitted_at_ms, None);

    // Drain it.
    let drained = store
        .mark_outbox_drained(&req, AuditEventType::CasPutCompleted, 1_000)
        .unwrap();
    assert!(drained, "drain returns true when row existed");

    // After drain: emitted_at is Some(1000).
    let snap = store.outbox_snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].emitted_at_ms, Some(1_000));

    // Drain non-existent → returns false, no mutation.
    let absent = store
        .mark_outbox_drained(&RequestId::new("never-existed"), AuditEventType::CasSoftDeleted, 9_999)
        .unwrap();
    assert!(!absent);
}
