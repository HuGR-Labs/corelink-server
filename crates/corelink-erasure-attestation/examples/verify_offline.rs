//! Example: offline verification of an erasure attestation.
//!
//! In production:
//! 1. Customer fetches attestation from `GET /v1/public/attestation/{request_id}`.
//! 2. Customer fetches public keys from `GET /v1/public/keys/erasure/{region}.pub`.
//! 3. Selects the public key with `key_id == attestation.payload.attestation_key_id`.
//! 4. Calls `verify_attestation_signature`.
//!
//! This example simulates the flow locally.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples use println/eprintln for demo + error paths"
)]

use corelink_erasure_attestation::{
    verify_attestation_signature, ErasureAttestationPayload, ErasureAttestationSigner,
    ErasureSigningKey, EvidenceBundle, Region,
};

fn main() {
    // --- Simulate signing (server-side) ---
    let bundle = EvidenceBundle {
        audit_chain_segment_ids: vec!["seg-001".to_string()],
        kms_destroy_ts: 1_700_000_000_000,
        kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
        tenant_id: "tenant-42".to_string(),
    };
    let sk = ErasureSigningKey::generate(
        3,
        Region::Weur,
        1_700_000_000_000,
        1_700_000_000_000 + 30 * 24 * 3_600 * 1_000,
    );
    let pk = sk.public_key();
    let signer = ErasureAttestationSigner::new(sk);
    let payload = ErasureAttestationPayload {
        tenant_id: "tenant-42".to_string(),
        request_id: "req-offline-verify-001".to_string(),
        destroyed_ts: 1_700_000_000_000,
        kms_provider: "aws_kms".to_string(),
        kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
        evidence_hash: EvidenceBundle::compute_hash(&bundle),
        region: Region::Weur,
        attestation_key_id: 3,
    };
    let attestation = signer.sign(payload).expect("sign");

    // --- Simulate customer-side offline verification ---
    println!(
        "Verifying attestation for request_id: {}",
        attestation.payload.request_id
    );
    println!("Using public key fingerprint: {}", pk.fingerprint());

    match verify_attestation_signature(&attestation, &pk) {
        Ok(()) => println!("VERIFIED: NIST SP 800-88 Rev.1 §2.4 crypto-erase attestation valid."),
        Err(e) => {
            eprintln!("VERIFICATION FAILED: {e}");
            std::process::exit(1);
        }
    }
}
