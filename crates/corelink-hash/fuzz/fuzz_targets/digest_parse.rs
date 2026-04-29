//! libFuzzer harness for `Digest::from_hex`. Asserts that adversarial input
//! never panics; coverage of length / alphabet / mixed-case branches.

#![no_main]

use corelink_hash::Digest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = Digest::from_hex(s);
});
