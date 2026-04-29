//! libFuzzer harness for `Digest::compute` + `VerifiedBody::new`. Random
//! `(body, claimed_digest)` pairs; asserts no panic + invariants on output.

#![no_main]

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody, DIGEST_LEN};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < DIGEST_LEN {
        return;
    }
    let mut claimed_bytes = [0u8; DIGEST_LEN];
    claimed_bytes.copy_from_slice(&data[..DIGEST_LEN]);
    let body_bytes = &data[DIGEST_LEN..];

    // Parse the claimed digest by hex-encoding the random 32 bytes (always valid).
    let claimed_hex: String = claimed_bytes.iter().map(|b| format!("{b:02x}")).collect();
    let Ok(claimed) = Digest::from_hex(&claimed_hex) else {
        // Should never happen — 32 bytes round-trip to valid hex.
        return;
    };

    // Compute matches itself (determinism on this body).
    let computed = Digest::compute(body_bytes);
    let ct_eq = computed.verify_constant_time(&computed);
    assert!(ct_eq);

    // VerifiedBody::new behavior:
    //   - if claimed == computed: must Ok
    //   - else: must Err(HashMismatch)
    let result = VerifiedBody::new(Bytes::copy_from_slice(body_bytes), claimed);
    if computed == claimed {
        let vb = result.expect("matched digest must verify");
        assert_eq!(vb.body().len(), body_bytes.len());
    } else {
        assert!(result.is_err(), "mismatched digest must reject");
    }
});
