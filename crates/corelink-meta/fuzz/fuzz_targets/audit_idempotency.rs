//! Fuzz harness for `audit_outbox` UNIQUE `(request_id, event_type)`
//! enforcement under adversarial retry patterns.
//!
//! The harness fires up to N commit_put attempts using the same
//! request_id but with payloads derived from random byte slices. Each
//! attempt either:
//! - succeeds if the (request_id, event_type) row matches the previous
//!   payload byte-for-byte (idempotent retry), OR
//! - fails with AuditIdempotencyConflict otherwise.
//!
//! The harness asserts that:
//! - On success, exactly one outbox row exists (no double-write).
//! - On conflict, the blob_meta side is **untouched** — atomicity holds.

#![no_main]
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use corelink_hash::Digest;
use corelink_meta::{
    AuditEvent, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, MetaError, MetaStore,
};
use libfuzzer_sys::fuzz_target;
use uuid::Uuid;

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 + 32 + 16 + 4 {
        return;
    }
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&data[..16]);
    let mut digest_bytes = [0u8; 32];
    digest_bytes.copy_from_slice(&data[16..48]);
    let mut audit_id_bytes = [0u8; 16];
    audit_id_bytes.copy_from_slice(&data[48..64]);
    let attempts = (data[64] % 8).saturating_add(2) as usize;
    let payload_seeds = &data[65..];

    let tenant = Uuid::from_bytes(uuid_bytes);
    let hex = hex_lowercase(&digest_bytes);
    let Ok(digest) = Digest::from_hex(&hex) else {
        return;
    };
    let key = BlobMetaKey::new(tenant, digest);

    let store = InMemoryMetaStore::new();
    let mut accepted_payload: Option<String> = None;
    let mut blob_committed = false;

    for i in 0..attempts {
        let pay_byte = payload_seeds.get(i).copied().unwrap_or(0);
        let payload = format!(r#"{{"v":{pay_byte}}}"#);
        let audit = AuditEvent::cas_put_completed(
            Uuid::from_bytes(audit_id_bytes),
            "stable-request",
            payload.clone(),
        );
        let result = futures_executor::block_on(store.commit_put(CommitPutRequest {
            key,
            size_bytes: 1024,
            now_ms: 1_000 + i as u64,
            audit,
        }));
        match (result, &accepted_payload) {
            (Ok(_), None) => {
                accepted_payload = Some(payload);
                blob_committed = true;
            }
            (Ok(_), Some(prev)) => {
                assert_eq!(
                    prev, &payload,
                    "idempotent path must require byte-identical payload"
                );
            }
            (Err(MetaError::AuditIdempotencyConflict), Some(_)) => {
                // Expected on payload mismatch. Atomicity invariant: blob_meta
                // unchanged (still 1 row).
                assert_eq!(store.row_count(), if blob_committed { 1 } else { 0 });
            }
            (Err(MetaError::AuditIdempotencyConflict), None) => {
                panic!("conflict before any successful commit");
            }
            (Err(e), _) => panic!("unexpected error: {e:?}"),
        }
    }
    // Final invariant: at most one outbox row total (UNIQUE constraint).
    assert!(store.outbox_snapshot().len() <= 1);
    if blob_committed {
        assert_eq!(store.row_count(), 1);
    } else {
        assert_eq!(store.row_count(), 0);
    }
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
