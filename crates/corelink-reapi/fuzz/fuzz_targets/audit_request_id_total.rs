//! Fuzz the `audit_request_id_for_blob` helper. Goal: assert it is
//! TOTAL (never panics) on arbitrary string inputs — the request_id and
//! digest_canonical_text both come from untrusted gRPC metadata in
//! production.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    // Split the input into three slices: client_request_id, digest, tenant
    // bytes (16 bytes for UUID).
    let (uuid_bytes_arr, rest) = data.split_at(16);
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(uuid_bytes_arr);
    let tenant = uuid::Uuid::from_bytes(uuid_bytes);
    let mid = rest.len() / 2;
    let client_req = String::from_utf8_lossy(&rest[..mid]);
    let digest = String::from_utf8_lossy(&rest[mid..]);
    let _ = corelink_reapi::audit_request_id_for_blob(&client_req, tenant, &digest);
});
