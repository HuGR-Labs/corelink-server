//! Fuzz the prost decode of `BatchUpdateBlobsRequest` against arbitrary
//! byte sequences. Goal: catch panics / OOMs in proto deserialization
//! that would otherwise surface as a server-side crash on adversarial
//! input.
//!
//! WI-S01-005 §14.5.5 SAST + cargo-fuzz contract.

#![no_main]

use libfuzzer_sys::fuzz_target;
use prost::Message;

fuzz_target!(|data: &[u8]| {
    // Decoder is allowed to fail; what we forbid is panicking. The
    // generated `BatchUpdateBlobsRequest` carries `bytes`, repeated
    // sub-messages, and varint-encoded enum tags — exercise each path.
    let _ = corelink_reapi::proto::reapi::BatchUpdateBlobsRequest::decode(data);
    let _ = corelink_reapi::proto::reapi::GetCapabilitiesRequest::decode(data);
});
