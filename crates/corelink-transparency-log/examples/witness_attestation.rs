//! Witness a CoreLink-signed entry on the (in-memory fake) public Rekor log.
//!
//! Mirrors the post-hoc seam a consumer (the erasure worker / container) runs
//! OFF the write path: build the canonical `hashedrekord`, submit it, and
//! persist the returned `{log_index, inclusion_proof}` — or degrade to the
//! out-of-band retry queue on a Rekor outage (fail-OPEN, ADR-0066).
//!
//! Run with: `cargo run -p corelink-transparency-log --example witness_attestation`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "example binary prints to stdout"
)]

use corelink_transparency_log::{witness_or_degrade, InMemoryRekor, SignedEntry, WitnessOutcome};

#[tokio::main]
async fn main() {
    // A CoreLink-signed entry — e.g. an Ed25519 erasure attestation: `payload`
    // is the RFC 8785 JCS canonical bytes, `signature` is the detached Ed25519
    // signature, `public_key_pem` is the published per-region verifying key.
    let entry = SignedEntry::new(
        br#"{"request_id":"req-uuid-001","tenant_id":"tenant-42"}"#.to_vec(),
        "ed25519",
        b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA...\n-----END PUBLIC KEY-----\n".to_vec(),
        vec![0xDE, 0xAD, 0xBE, 0xEF],
    );

    println!("content digest (sha256): {}", entry.content_sha256_hex());
    println!(
        "rekor wire body: {}",
        entry.to_rekor_hashedrekord().to_wire_json().unwrap()
    );

    let rekor = InMemoryRekor::new();
    match witness_or_degrade(&rekor, &entry).await {
        WitnessOutcome::Witnessed(record) => {
            println!(
                "WITNESSED — log_index={} uuid={} root_hash={}",
                record.log_index, record.entry_uuid, record.inclusion_proof.root_hash
            );
            println!("persist this RekorWitnessRecord alongside the entry.");
        }
        WitnessOutcome::Degraded(reason) => {
            println!("DEGRADED (fail-OPEN) — queue for retry: {reason}");
            println!("the entry stays durable; witnessing is best-effort.");
        }
    }
}
