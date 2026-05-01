//! Idempotency + UNIQUE-constraint canonical regression tests.
//!
//! Mirrors the WI §8 Gherkin scenarios that pin the `INSERT … ON
//! CONFLICT` semantic at storage layer. The simulator preserves
//! every immutable column under the idempotent path; only
//! `last_hit_at` (and optionally `expires_at`) mutate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_ac_schema::{
    AcRegion, AcSchema, AcUpsertOutcome, AcUpsertRequest, SigAlg,
};
use uuid::Uuid;

fn tenant_a() -> Uuid {
    Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
}

fn hex64(seed: u8) -> String {
    let bytes = [seed; 32];
    hex::encode(bytes)
}

fn req(tid: Uuid, action_seed: u8, result_seed: u8, now_ms: i64) -> AcUpsertRequest {
    AcUpsertRequest {
        tenant_id: tid,
        action_digest: hex64(action_seed),
        tenant_prefix: [0xab; 16],
        path_key_id: 1,
        result_hash: hex64(result_seed),
        blob_refs: r#"[]"#.to_string(),
        blob_refs_count: 0,
        result_size_bytes: 64,
        sig_key_id: 1,
        sig_alg: SigAlg::HkdfSha256,
        region: AcRegion::Sam,
        now_ms,
        ttl_ms: Some(60_000),
        created_by_pat_id: Some("pat-original".to_string()),
        created_by_request_id: Some("req-original".to_string()),
    }
}

#[test]
fn first_insert_then_idempotent_refresh_emits_correct_outcomes() {
    let mut s = AcSchema::new();
    let r1 = req(tenant_a(), 1, 0xaa, 1_000);
    assert_eq!(s.upsert(r1.clone()).unwrap(), AcUpsertOutcome::Inserted);
    let mut r2 = r1.clone();
    r2.now_ms = 5_000;
    assert_eq!(
        s.upsert(r2).unwrap(),
        AcUpsertOutcome::IdempotentRefresh
    );
}

#[test]
fn idempotent_refresh_preserves_created_by_columns() {
    let mut s = AcSchema::new();
    let r1 = req(tenant_a(), 1, 0xaa, 1_000);
    s.upsert(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.now_ms = 5_000;
    r2.created_by_pat_id = Some("pat-different".into());
    r2.created_by_request_id = Some("req-different".into());
    s.upsert(r2).unwrap();
    let row = s.get(&tenant_a(), &r1.action_digest).unwrap();
    assert_eq!(row.created_by_pat_id.as_deref(), Some("pat-original"));
    assert_eq!(row.created_by_request_id.as_deref(), Some("req-original"));
}

#[test]
fn idempotent_refresh_preserves_immutable_blob_refs() {
    let mut s = AcSchema::new();
    let mut r1 = req(tenant_a(), 1, 0xaa, 1_000);
    r1.blob_refs = r#"["digestA","digestB"]"#.to_string();
    r1.blob_refs_count = 2;
    s.upsert(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.now_ms = 5_000;
    r2.blob_refs = r#"["digestC"]"#.to_string(); // mutation attempted
    r2.blob_refs_count = 1;
    s.upsert(r2).unwrap();
    let row = s.get(&tenant_a(), &r1.action_digest).unwrap();
    // Blob refs immutable across idempotent refresh.
    assert_eq!(row.blob_refs, r#"["digestA","digestB"]"#);
    assert_eq!(row.blob_refs_count, 2);
}

#[test]
fn ttl_extends_on_idempotent_refresh_when_provided() {
    let mut s = AcSchema::new();
    let r1 = req(tenant_a(), 1, 0xaa, 1_000);
    s.upsert(r1.clone()).unwrap();
    let row1 = s.get(&tenant_a(), &r1.action_digest).unwrap().clone();
    assert_eq!(row1.expires_at, Some(61_000));

    let mut r2 = r1.clone();
    r2.now_ms = 10_000;
    r2.ttl_ms = Some(120_000);
    s.upsert(r2).unwrap();
    let row2 = s.get(&tenant_a(), &r1.action_digest).unwrap();
    assert_eq!(row2.expires_at, Some(130_000)); // now_ms + new delta
    assert_eq!(row2.last_hit_at, 10_000);
}

#[test]
fn ttl_left_unchanged_when_idempotent_refresh_omits_ttl() {
    let mut s = AcSchema::new();
    let r1 = req(tenant_a(), 1, 0xaa, 1_000);
    s.upsert(r1.clone()).unwrap();
    let mut r2 = r1.clone();
    r2.now_ms = 10_000;
    r2.ttl_ms = None;
    s.upsert(r2).unwrap();
    let row2 = s.get(&tenant_a(), &r1.action_digest).unwrap();
    assert_eq!(row2.expires_at, Some(61_000)); // preserved from first insert
    assert_eq!(row2.last_hit_at, 10_000);
}

#[test]
fn result_hash_immutable_on_mismatched_upsert() {
    let mut s = AcSchema::new();
    let r1 = req(tenant_a(), 1, 0xaa, 1_000);
    s.upsert(r1.clone()).unwrap();
    let r2 = req(tenant_a(), 1, 0xbb, 5_000);
    let outcome = s.upsert(r2.clone()).unwrap();
    assert_eq!(outcome, AcUpsertOutcome::ResultHashMismatch);
    let row = s.get(&tenant_a(), &r1.action_digest).unwrap();
    // Existing result_hash preserved.
    assert_eq!(row.result_hash, r1.result_hash);
    // last_hit_at NOT refreshed (mismatch is a no-op write).
    assert_eq!(row.last_hit_at, 1_000);
    // Row count unchanged.
    assert_eq!(s.row_count(), 1);
}

#[test]
fn migration_apply_then_apply_again_is_no_op() {
    let mut s = AcSchema::new();
    s.upsert(req(tenant_a(), 1, 0xaa, 1_000)).unwrap();
    s.upsert(req(tenant_a(), 2, 0xbb, 2_000)).unwrap();
    let before = s.row_count();
    s.reapply_migration().unwrap();
    s.reapply_migration().unwrap();
    s.reapply_migration().unwrap();
    assert_eq!(s.row_count(), before);
}

#[test]
fn pk_uniqueness_per_tenant_action_pair() {
    // Same action_digest, different tenants => two distinct rows.
    let mut s = AcSchema::new();
    let ten_a = tenant_a();
    let ten_b = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
    s.upsert(req(ten_a, 7, 0xaa, 1_000)).unwrap();
    let mut r_b = req(ten_b, 7, 0xbb, 1_000);
    r_b.tenant_id = ten_b;
    s.upsert(r_b.clone()).unwrap();
    assert_eq!(s.row_count(), 2);
    assert!(s.get(&ten_a, &r_b.action_digest).is_some());
    assert!(s.get(&ten_b, &r_b.action_digest).is_some());
}
