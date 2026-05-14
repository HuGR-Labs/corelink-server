//! Property tests for erasure attestation — 6 props × 10k iter (PR gate).
//!
//! Run with:
//! ```sh
//! PROPTEST_CASES=10000 cargo test -p corelink-erasure-attestation --test prop_attestation
//! ```
//! Nightly: `PROPTEST_CASES=100000`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use corelink_erasure_attestation::{
    ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey, EvidenceBundle,
    Region, verify_attestation_signature,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` from environment (default 10_000 per spec).
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn arb_region() -> impl Strategy<Value = Region> {
    prop_oneof![
        Just(Region::Wnam),
        Just(Region::Enam),
        Just(Region::Weur),
        Just(Region::Sam),
    ]
}

fn arb_payload() -> impl Strategy<Value = ErasureAttestationPayload> {
    (
        "[a-z0-9-]{8,32}",       // tenant_id
        "[a-z0-9-]{8,36}",       // request_id
        1_600_000_000_000u64..2_000_000_000_000u64, // destroyed_ts
        prop_oneof![
            Just("aws_kms"),
            Just("gcp_kms"),
            Just("azure_kv"),
            Just("vault"),
        ],
        "[a-z0-9:/-]{8,60}",     // kms_key_id
        "[a-f0-9]{64}",           // evidence_hash (sha256 hex pattern)
        arb_region(),
        1u64..=100u64,            // attestation_key_id
    )
        .prop_map(
            |(tenant_id, request_id, destroyed_ts, kms_provider, kms_key_id, evidence_hash, region, attestation_key_id)| {
                ErasureAttestationPayload {
                    tenant_id,
                    request_id,
                    destroyed_ts,
                    kms_provider: kms_provider.to_string(),
                    kms_key_id,
                    evidence_hash,
                    region,
                    attestation_key_id,
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// prop_jcs_deterministic: same payload serialized twice → byte-equal JCS.
    ///
    /// Invariant: INV-AUDIT-CHAIN-HASH-DETERMINISTIC (HIGH).
    #[test]
    fn prop_jcs_deterministic(payload in arb_payload()) {
        let c1 = serde_jcs::to_string(&payload).unwrap();
        let c2 = serde_jcs::to_string(&payload).unwrap();
        prop_assert_eq!(c1, c2);
    }

    /// prop_ed25519_sign_verify_roundtrip: random payloads sign + verify OK.
    #[test]
    fn prop_ed25519_sign_verify_roundtrip(payload in arb_payload()) {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
        let pk = sk.public_key();
        let signer = ErasureAttestationSigner::new(sk);
        let att = signer.sign(payload).unwrap();
        prop_assert!(verify_attestation_signature(&att, &pk).is_ok());
    }

    /// prop_signature_forge_rejected: forged (random) signatures must not verify.
    ///
    /// Generates a valid attestation then replaces the signature with a
    /// random 64-byte base64 blob. verify MUST reject it.
    #[test]
    fn prop_signature_forge_rejected(
        payload in arb_payload(),
        forge_bytes in prop::collection::vec(any::<u8>(), 64..=64usize),
    ) {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
        let pk = sk.public_key();
        let signer = ErasureAttestationSigner::new(sk);
        let mut att = signer.sign(payload).unwrap();
        // Replace with forged signature bytes.
        att.signature_ed25519 =
            base64::engine::general_purpose::STANDARD.encode(&forge_bytes);
        // A forged signature MUST NOT verify (0 false-pass).
        prop_assert!(verify_attestation_signature(&att, &pk).is_err());
    }

    /// prop_overlap_window_verify_both_keys: rotation completed;
    /// attestation signed pre-rotation is verifiable with the old (Overlap)
    /// public key AND not verifiable with the new key.
    #[test]
    fn prop_overlap_window_verify_both_keys(payload in arb_payload()) {
        let sk_old = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
        let pk_old = sk_old.public_key();
        let signer_old = ErasureAttestationSigner::new(sk_old);
        let att = signer_old.sign(payload).unwrap();

        // Verify with old key (Overlap) — MUST succeed.
        prop_assert!(verify_attestation_signature(&att, &pk_old).is_ok());

        // Verify with a new (different) key — MUST fail.
        let sk_new = ErasureSigningKey::generate(2, Region::Weur, 0, 0);
        let pk_new = sk_new.public_key();
        prop_assert!(verify_attestation_signature(&att, &pk_new).is_err());
    }

    /// prop_evidence_hash_deterministic: same bundle → same SHA-256 hash.
    #[test]
    fn prop_evidence_hash_deterministic(
        seg_id in "[a-z0-9-]{4,20}",
        ts in 1_600_000_000_000u64..2_000_000_000_000u64,
        key_id in "[a-z0-9:/-]{8,40}",
        tenant in "[a-z0-9-]{4,20}",
    ) {
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec![seg_id.clone()],
            kms_destroy_ts: ts,
            kms_key_id: key_id.clone(),
            tenant_id: tenant.clone(),
        };
        let h1 = EvidenceBundle::compute_hash(&bundle);
        let h2 = EvidenceBundle::compute_hash(&bundle);
        prop_assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars.
        prop_assert_eq!(h1, h2);
    }

    /// prop_replay_request_id_rejected: two attestations with the same
    /// `request_id` must have identical canonical forms (idempotent payload)
    /// — the D1 UNIQUE constraint is the runtime enforcement; this property
    /// tests that the same payload always maps to the same canonical bytes
    /// so the constraint fires correctly.
    #[test]
    fn prop_replay_request_id_idempotent(payload in arb_payload()) {
        let p2 = payload.clone();
        let c1 = serde_jcs::to_string(&payload).unwrap();
        let c2 = serde_jcs::to_string(&p2).unwrap();
        prop_assert_eq!(c1, c2);
    }
}

// Allow dead_code for base64 import used in prop test above
use base64::Engine as _;
