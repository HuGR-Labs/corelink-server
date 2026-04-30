//! Fuzz harness for the canonical R2 key construction path.
//!
//! Random `(tdk_bytes, tenant_id_bytes, region_byte, body_bytes)` quadruples
//! drive the writer's full key-derivation pipeline (HMAC → b64 → key
//! construction → in-memory PUT). The harness asserts:
//! - the writer never panics on any adversarial input,
//! - the resulting key is exactly the canonical grammar,
//! - the prefix segment is exactly 16 ASCII chars from the URL-safe base64
//!   alphabet,
//! - the digest segments are lowercase hex and have the canonical lengths.

#![no_main]
// Fuzz harness: panics ARE the contract (libFuzzer treats panic as a finding),
// so we keep allow on panic / expect / indexing here while the lib crate
// stays strict.
#![allow(clippy::panic, clippy::expect_used, clippy::indexing_slicing)]

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::r2::{InMemoryR2, R2Writer};
use corelink_worker::{Region, TenantCtx};
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;
use uuid::Uuid;
use zeroize::Zeroizing;

fn region_from_byte(b: u8) -> Region {
    match b % 3 {
        0 => Region::Wnam,
        1 => Region::Weur,
        _ => Region::Sam,
    }
}

fn is_url_safe_b64(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 49 {
        return;
    }
    let mut tdk_bytes = [0u8; 32];
    tdk_bytes.copy_from_slice(&data[..32]);
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&data[32..48]);
    let region_byte = data[48];
    let body_bytes = &data[49..];

    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(tdk_bytes));
    let uuid = Uuid::from_bytes(uuid_bytes);
    let region = region_from_byte(region_byte);
    let ctx = TenantCtx::new(&tdk, uuid, region);

    let body = Bytes::copy_from_slice(body_bytes);
    let claimed = Digest::compute(&body);
    let Ok(vb) = VerifiedBody::new(body, claimed) else {
        return;
    };

    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(region, Arc::clone(&backend));

    let _ = futures_executor::block_on(writer.put(&ctx, &vb));

    let keys = backend.keys_snapshot();
    let Some(key) = keys.first() else { return };

    let parts: Vec<&str> = key.split('/').collect();
    assert_eq!(parts.len(), 6, "key must have 6 path segments: {key}");
    assert!(
        parts.first().copied() == Some(region.cas_bucket_name()),
        "bucket segment drift: {key}",
    );
    let prefix_seg = parts.get(1).copied().unwrap_or("");
    assert_eq!(prefix_seg.len(), 16, "prefix must be 16 chars: {key}");
    for &b in prefix_seg.as_bytes() {
        assert!(is_url_safe_b64(b), "non-URL-safe-b64 byte in prefix: {key}");
    }
    assert_eq!(parts.get(2).copied(), Some("blake3"), "digest fn drift: {key}");
    let s0 = parts.get(3).copied().unwrap_or("");
    let s1 = parts.get(4).copied().unwrap_or("");
    let hex = parts.get(5).copied().unwrap_or("");
    assert_eq!(s0.len(), 2);
    assert_eq!(s1.len(), 2);
    assert_eq!(hex.len(), 64);
    for seg in [s0, s1, hex] {
        for c in seg.chars() {
            assert!(
                c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
                "non-lowercase-hex char {c:?} in segment {seg:?}: {key}"
            );
        }
    }
});
