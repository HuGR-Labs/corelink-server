//! Example: sign an erasure attestation (local key, no I/O).
//!
//! In production the signing key is loaded from a Cloudflare Workers Secret.
//! This example demonstrates the pure signing flow.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "examples use println"
)]

use corelink_erasure_attestation::{
    ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey, EvidenceBundle, Region,
};

fn main() {
    let bundle = EvidenceBundle {
        audit_chain_segment_ids: vec!["seg-001".to_string(), "seg-002".to_string()],
        kms_destroy_ts: 1_700_000_000_000,
        kms_key_id: "arn:aws:kms:eu-west-1:123456789:key/abcdef".to_string(),
        tenant_id: "tenant-enterprise-42".to_string(),
    };

    let evidence_hash = bundle.validated_hash().expect("valid bundle");

    let signing_key = ErasureSigningKey::generate(
        1,
        Region::Weur,
        1_700_000_000_000,
        1_700_000_000_000 + 30 * 24 * 3_600 * 1_000,
    );

    let signer = ErasureAttestationSigner::new(signing_key);

    let payload = ErasureAttestationPayload {
        tenant_id: "tenant-enterprise-42".to_string(),
        request_id: "dsr-req-550e8400-e29b-41d4-a716-446655440000".to_string(),
        destroyed_ts: 1_700_000_000_000,
        kms_provider: "aws_kms".to_string(),
        kms_key_id: "arn:aws:kms:eu-west-1:123456789:key/abcdef".to_string(),
        evidence_hash,
        region: Region::Weur,
        attestation_key_id: 1,
    };

    let attestation = signer.sign(payload).expect("sign");

    println!("Attestation signed successfully.");
    println!("request_id:         {}", attestation.payload.request_id);
    println!("region:             {}", attestation.payload.region);
    println!(
        "attestation_key_id: {}",
        attestation.payload.attestation_key_id
    );
    println!("evidence_hash:      {}", attestation.payload.evidence_hash);
    println!("signature_ed25519:  {}", attestation.signature_ed25519);
    println!(
        "canonical_len:      {} bytes",
        attestation.canonical_payload_jcs.len()
    );
}
