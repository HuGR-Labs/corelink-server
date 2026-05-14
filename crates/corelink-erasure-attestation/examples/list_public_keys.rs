//! Example: list public keys for a region (Active + Overlap).
//!
//! In production `GET /v1/public/keys/erasure/{region}.pub` returns a JSON
//! array of [`ErasurePublicKey`] covering the Active key and any Overlap
//! key still within the 30d window. This example simulates the response.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "examples use println"
)]

use corelink_erasure_attestation::{ErasurePublicKey, ErasureSigningKey, Region};

fn main() {
    // Simulate rotation: old key in Overlap + new key Active.
    let old_sk = ErasureSigningKey::generate(
        2,
        Region::Weur,
        1_700_000_000_000,
        1_700_000_000_000 + 30 * 24 * 3_600 * 1_000,
    );
    let old_pk = old_sk.public_key();

    let new_sk = ErasureSigningKey::generate(
        3,
        Region::Weur,
        1_700_000_000_000 + 30 * 24 * 3_600 * 1_000,
        1_700_000_000_000 + 60 * 24 * 3_600 * 1_000,
    );
    let new_pk = new_sk.public_key();

    // Endpoint returns both keys during overlap window.
    let keys: Vec<&ErasurePublicKey> = vec![&new_pk, &old_pk]; // active first

    println!("GET /v1/public/keys/erasure/weur.pub");
    println!("Returns {} public key(s):", keys.len());
    for key in &keys {
        println!(
            "  key_id={} overlap_until_ms={} fingerprint={}",
            key.key_id,
            key.overlap_until_ms,
            key.fingerprint()
        );
        println!("  PEM:");
        for line in key.pem.lines() {
            println!("    {line}");
        }
    }

    // Serialize as JSON for endpoint response.
    let json = serde_json::to_string_pretty(&keys).expect("serialize");
    println!("\nJSON response ({} bytes):", json.len());
    println!("{json}");
}
