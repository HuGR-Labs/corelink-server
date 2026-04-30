//! Sync verify happy path.
//!
//! Run via `cargo run --example verify_simple`.

#![allow(clippy::expect_used, clippy::print_stdout)]

use corelink_client_verify::{ClientVerifier, Digest, VerifyConfig};

fn main() {
    let v = ClientVerifier::new(VerifyConfig::default());
    let body: &[u8] = b"hello world";
    let expected = Digest::compute(body);
    v.verify(body, &expected)
        .expect("default-on verifier proves the match");
    println!("verify ok: digest = {expected}");
}
