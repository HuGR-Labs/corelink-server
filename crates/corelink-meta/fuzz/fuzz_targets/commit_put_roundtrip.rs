//! Fuzz harness for the `commit_put` → `get` round-trip path.
//!
//! Random `(tenant_uuid_bytes, digest_hex_bytes, size_bytes, now_ms,
//! audit_id_bytes, request_id_str, payload_str)` tuples drive the full
//! commit_put pipeline against the in-memory fake. The harness asserts:
//!
//! - The store never panics on any adversarial input (the lib code lives
//!   under `#![forbid(unsafe_code)]` and `clippy::panic = "deny"`, so a
//!   panic would be a P0 escape).
//! - Successful insert + get round-trip preserves every field.
//! - Idempotent re-INSERT for the same `(tenant_id, digest)` pair is a
//!   no-op at the row level.
//! - Audit-outbox `(request_id, event_type)` UNIQUE is honored under
//!   adversarial retry patterns.

#![no_main]
// libFuzzer harness: panics are findings (not bugs).
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, InsertOutcome, MetaStore,
};
use libfuzzer_sys::fuzz_target;
use uuid::Uuid;

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 + 32 + 8 + 8 + 16 + 8 + 8 {
        return;
    }
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&data[..16]);
    let mut digest_bytes = [0u8; 32];
    digest_bytes.copy_from_slice(&data[16..48]);
    let size_bytes = u64::from_le_bytes([
        data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55],
    ]);
    let now_ms = u64::from_le_bytes([
        data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63],
    ]);
    let mut audit_id_bytes = [0u8; 16];
    audit_id_bytes.copy_from_slice(&data[64..80]);
    // Build request_id and payload from remaining bytes; truncate / sanitize.
    let req_seed = u64::from_le_bytes([
        data[80], data[81], data[82], data[83], data[84], data[85], data[86], data[87],
    ]);
    let pay_seed = u64::from_le_bytes([
        data[88], data[89], data[90], data[91], data[92], data[93], data[94], data[95],
    ]);

    // Sanitize size_bytes to be in [1, 5MiB] — outside this range,
    // the schema's CHECK constraint or the upstream R2 5 MiB single-blob
    // ceiling would reject. The harness focuses on the meta-store
    // invariants, not on input sanitization.
    let size_bytes = (size_bytes % (5 * 1024 * 1024)) + 1;

    let tenant = Uuid::from_bytes(uuid_bytes);
    let hex = hex_lowercase(&digest_bytes);
    let Ok(digest) = Digest::from_hex(&hex) else {
        return;
    };
    let key = BlobMetaKey::new(tenant, digest);
    let audit = AuditEvent::cas_put_completed(
        Uuid::from_bytes(audit_id_bytes),
        format!("req-{req_seed:x}"),
        format!(r#"{{"v":{pay_seed}}}"#),
    );

    let store = InMemoryMetaStore::new();
    let rt = futures_executor::block_on(store.commit_put(CommitPutRequest {
        key,
        size_bytes,
        now_ms,
        audit: audit.clone(),
    }));
    let Ok(outcome) = rt else { return };
    assert_eq!(outcome, InsertOutcome::Inserted);

    let row = futures_executor::block_on(store.get(&key))
        .unwrap()
        .expect("just inserted");
    assert_eq!(row.size_bytes, size_bytes);
    assert_eq!(row.refcount, 1);
    assert_eq!(row.created_at_ms, now_ms);
    assert_eq!(row.last_accessed_at_ms, now_ms);
    assert_eq!(row.deleted_at_ms, None);
    assert!(row.is_alive());

    // Idempotent retry — same key, same audit event, expect AlreadyExists.
    let again = futures_executor::block_on(store.commit_put(CommitPutRequest {
        key,
        size_bytes,
        now_ms: now_ms.wrapping_add(1),
        audit,
    }))
    .unwrap();
    assert_eq!(again, InsertOutcome::AlreadyExists);
    assert_eq!(store.row_count(), 1, "no duplicate rows under idempotent retry");
    assert_eq!(store.outbox_snapshot().len(), 1, "outbox UNIQUE preserved");
});

fn hex_lowercase(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(nibble(b >> 4));
        out.push(nibble(b & 0x0f));
    }
    out
}

fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => '0',
    }
}
