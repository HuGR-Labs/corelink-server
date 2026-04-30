//! libFuzzer harness for `ClientVerifier::verify`. Random
//! `(body, claimed_digest)` pairs; asserts the verify outcome
//! matches the `Digest::compute(body) == claimed` truth and never
//! panics.

#![no_main]

use corelink_client_verify::{ClientVerifier, Digest, VerifyError, DIGEST_LEN};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < DIGEST_LEN {
        return;
    }
    let mut claimed_bytes = [0u8; DIGEST_LEN];
    claimed_bytes.copy_from_slice(&data[..DIGEST_LEN]);
    let body = &data[DIGEST_LEN..];

    let claimed_hex: String = claimed_bytes.iter().map(|b| format!("{b:02x}")).collect();
    let Ok(claimed) = Digest::from_hex(&claimed_hex) else {
        return;
    };

    let v = ClientVerifier::default_on();
    let computed = Digest::compute(body);
    let result = v.verify(body, &claimed);

    if computed.verify_constant_time(&claimed) {
        // Truth: matches → must Ok.
        assert!(result.is_ok(), "matched body must verify");
    } else {
        match result {
            Err(VerifyError::DigestMismatch { expected, computed: cs }) => {
                assert_eq!(expected, claimed.to_hex());
                assert_eq!(cs, computed.to_hex());
            }
            Err(VerifyError::VerifyDisabled) => {
                panic!("default verifier must not be disabled");
            }
            Err(_) => panic!("unexpected non-exhaustive variant"),
            Ok(()) => panic!("mismatched body must error"),
        }
    }
});
