//! Fuzz the `ByteStream::Read` resource_name parser against arbitrary
//! UTF-8 strings. Goal: catch panics in the read-side resource_name
//! parser landed in WI-S02-001.
//!
//! WI-S02-001 §10.1.5 + §14.1.5 cargo-fuzz smoke contract.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // The parser is total over str; what we forbid is panicking.
        // We don't care about the Result distinction — just exercise
        // every code path.
        let _ = corelink_reapi::handler::parse_read_resource_name(s);
    }
});
