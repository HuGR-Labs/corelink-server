//! Example: lookup attestation by request_id (stub for D1 index query).
//!
//! In production `GET /v1/public/attestation/{request_id}` returns the
//! JSON-serialized [`ErasureAttestation`] from the D1 index +
//! R2 audit bucket lookup. This example demonstrates deserialization.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "examples use println"
)]

use corelink_erasure_attestation::{
    ErasureAttestation, ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey,
    EvidenceBundle, Region,
};

fn main() {
    // Simulate creating and serializing an attestation.
    let bundle = EvidenceBundle {
        audit_chain_segment_ids: vec!["seg-001".to_string()],
        kms_destroy_ts: 1_700_000_000_000,
        kms_key_id: "vault/transit/keys/tenant-99".to_string(),
        tenant_id: "tenant-99".to_string(),
    };
    let sk = ErasureSigningKey::generate(5, Region::Sam, 0, 0);
    let signer = ErasureAttestationSigner::new(sk);
    let payload = ErasureAttestationPayload {
        tenant_id: "tenant-99".to_string(),
        request_id: "req-lookup-001".to_string(),
        destroyed_ts: 1_700_000_000_000,
        kms_provider: "vault".to_string(),
        kms_key_id: "vault/transit/keys/tenant-99".to_string(),
        evidence_hash: EvidenceBundle::compute_hash(&bundle),
        region: Region::Sam,
        attestation_key_id: 5,
    };
    let attestation = signer.sign(payload).expect("sign");

    // Serialize to JSON (simulates R2 storage + HTTP response).
    let json = serde_json::to_string_pretty(&attestation).expect("serialize");
    println!("Serialized attestation ({} bytes):", json.len());
    println!("{json}");

    // Deserialize (simulates client parsing endpoint response).
    let parsed: ErasureAttestation = serde_json::from_str(&json).expect("deserialize");
    println!("\nDeserialized request_id: {}", parsed.payload.request_id);
    println!("region: {}", parsed.payload.region);
}
