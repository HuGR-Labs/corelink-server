//! Fuzz harness: PUT-then-GET must round-trip the body bytes byte-for-byte
//! across any adversarial input. Also exercises the cross-tenant oracle
//! closure (a second tenant must see uniform `NotFound` on the same digest)
//! and the size-cap rejection path (oversize bodies return BlobTooLarge).
//! Panics fail the harness; mismatches assert.

#![no_main]
#![allow(clippy::panic, clippy::expect_used, clippy::indexing_slicing)]

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::error::R2Error;
use corelink_worker::storage::r2::{
    InMemoryR2, PutOutcome, R2Reader, R2Writer, SINGLE_BLOB_LIMIT_BYTES,
};
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

fuzz_target!(|data: &[u8]| {
    // Layout: TDK[32] | UUID[16] | UUID_alt[16] | region[1] | body[..]
    if data.len() < 65 {
        return;
    }
    let mut tdk_bytes = [0u8; 32];
    tdk_bytes.copy_from_slice(&data[..32]);
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&data[32..48]);
    let mut uuid_alt_bytes = [0u8; 16];
    uuid_alt_bytes.copy_from_slice(&data[48..64]);
    let region_byte = data[64];
    let body_bytes = &data[65..];

    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(tdk_bytes));
    let uuid = Uuid::from_bytes(uuid_bytes);
    let uuid_alt = Uuid::from_bytes(uuid_alt_bytes);
    let region = region_from_byte(region_byte);
    let ctx = TenantCtx::new(&tdk, uuid, region);
    let ctx_alt = TenantCtx::new(&tdk, uuid_alt, region);

    let body = Bytes::copy_from_slice(body_bytes);
    let claimed = Digest::compute(&body);
    let Ok(vb) = VerifiedBody::new(body.clone(), claimed) else {
        return;
    };

    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(region, Arc::clone(&backend));
    let reader = R2Reader::new(region, Arc::clone(&backend));

    // Size-cap path: oversize bodies must fail before touching the backend.
    if body_bytes.len() > SINGLE_BLOB_LIMIT_BYTES {
        let res = futures_executor::block_on(writer.put(&ctx, &vb));
        match res {
            Err(R2Error::BlobTooLarge { size, limit }) => {
                assert_eq!(size, body_bytes.len());
                assert_eq!(limit, SINGLE_BLOB_LIMIT_BYTES);
            }
            other => panic!("oversize must reject, got {other:?}"),
        }
        return;
    }

    let put_outcome = futures_executor::block_on(writer.put(&ctx, &vb))
        .expect("InMemoryR2 PUT must not fail");
    assert_eq!(put_outcome, PutOutcome::Fresh);

    let dup_outcome =
        futures_executor::block_on(writer.put(&ctx, &vb)).expect("InMemoryR2 PUT must not fail");
    assert_eq!(dup_outcome, PutOutcome::Duplicate);

    // Round-trip the body byte-for-byte under the same tenant.
    let got = futures_executor::block_on(reader.get(&ctx, &claimed))
        .expect("InMemoryR2 GET must not fail after a Fresh PUT");
    assert_eq!(got, body, "round-trip body mismatch");

    // Cross-tenant oracle closure: a different tenant under the same TDK
    // sees NotFound for the same digest (REG-NAMESPACE-001 + ADR-0028).
    if uuid != uuid_alt {
        match futures_executor::block_on(reader.get(&ctx_alt, &claimed)) {
            Err(R2Error::NotFound) => {}
            other => panic!("cross-tenant read must NotFound, got {other:?}"),
        }
    }
});
