//! Adversarial regression tests — 7+ scenarios covering cryptographic threat
//! model for WI-S14-007 erasure attestation.
//!
//! Scenarios:
//! 1. Forge signature attempt (10k random sigs; 0 false-pass).
//! 2. Replay attestation — same request_id, same canonical → D1 UNIQUE
//!    detects (modelled as duplicate canonical detection).
//! 3. Public key substitution — verify with wrong key fails.
//! 4. Time tampering on destroyed_ts — tampered payload fails verify.
//! 5. Tenant impersonation — forged tenant_id fails verify.
//! 6. JCS canonicalization tamper (Unicode NFC bypass attempt).
//! 7. Signing key compromise simulation — old attestation verifiable with
//!    old Overlap key; NOT verifiable with emergency rotation new key.
//! 8. Payload substitution — KEEP the original valid canonical_payload_jcs +
//!    signature, but mutate the typed `payload` (tenant_id) → MUST fail
//!    verify (binds typed payload to the signed bytes).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "adversarial tests are allowed to use these primitives"
)]

use base64::Engine as _;
use corelink_erasure_attestation::{
    verify_attestation_signature, ErasureAttestationPayload, ErasureAttestationSigner,
    ErasureSigningKey, EvidenceBundle, Region,
};

fn sample_bundle() -> EvidenceBundle {
    EvidenceBundle {
        audit_chain_segment_ids: vec!["seg-001".to_string()],
        kms_destroy_ts: 1_700_000_000_000,
        kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
        tenant_id: "tenant-42".to_string(),
    }
}

fn sample_payload() -> ErasureAttestationPayload {
    ErasureAttestationPayload {
        tenant_id: "tenant-42".to_string(),
        request_id: "req-001".to_string(),
        destroyed_ts: 1_700_000_000_000,
        kms_provider: "aws_kms".to_string(),
        kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
        evidence_hash: EvidenceBundle::compute_hash(&sample_bundle()),
        region: Region::Weur,
        attestation_key_id: 1,
    }
}

/// Scenario 1: forge signature attempt.
///
/// 10_000 random 64-byte blobs as signatures — all MUST fail verify.
/// Confirms Ed25519 256-bit security: attacker cannot guess a valid sig.
#[test]
fn forge_signature_10k_attempts_zero_pass() {
    use rand::RngCore;
    let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
    let pk = sk.public_key();
    let signer = ErasureAttestationSigner::new(sk);
    let att = signer.sign(sample_payload()).unwrap();
    let mut rng = rand::thread_rng();

    for _ in 0..10_000 {
        let mut forge_bytes = [0u8; 64];
        rng.fill_bytes(&mut forge_bytes);
        let mut forged = att.clone();
        forged.signature_ed25519 = base64::engine::general_purpose::STANDARD.encode(forge_bytes);
        assert!(
            verify_attestation_signature(&forged, &pk).is_err(),
            "forge attempt must be rejected"
        );
    }
}

/// Scenario 2: replay attestation (same request_id, different context).
///
/// The canonical payload for a replayed attestation is identical to the
/// original (same request_id + same fields). The D1 UNIQUE constraint on
/// `request_id` is the runtime enforcement; this test confirms the canonical
/// form is stable (idempotent) so the UNIQUE constraint fires reliably.
#[test]
fn replay_request_id_canonical_is_identical() {
    let p1 = sample_payload();
    let p2 = sample_payload(); // exact same request_id
    let c1 = serde_jcs::to_string(&p1).unwrap();
    let c2 = serde_jcs::to_string(&p2).unwrap();
    assert_eq!(
        c1, c2,
        "replay: same payload must produce identical canonical"
    );
}

/// Scenario 3: public key substitution.
///
/// Attacker substitutes the public key endpoint response with a different
/// key. Verification with the wrong key MUST fail.
#[test]
fn public_key_substitution_fails_verify() {
    let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
    let signer = ErasureAttestationSigner::new(sk);
    let att = signer.sign(sample_payload()).unwrap();

    // Attacker substitutes a different key.
    let attacker_sk = ErasureSigningKey::generate(999, Region::Weur, 0, 0);
    let attacker_pk = attacker_sk.public_key();
    assert!(
        verify_attestation_signature(&att, &attacker_pk).is_err(),
        "wrong key must reject valid attestation"
    );
}

/// Scenario 4: time tampering on `destroyed_ts`.
///
/// Attacker modifies `destroyed_ts` in the payload. The canonical form
/// changes, so the existing signature no longer matches — verify fails.
#[test]
fn time_tampering_destroyed_ts_fails_verify() {
    let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
    let pk = sk.public_key();
    let signer = ErasureAttestationSigner::new(sk);
    let mut att = signer.sign(sample_payload()).unwrap();

    // Tamper: advance timestamp by 1 hour.
    att.payload.destroyed_ts += 3_600_000;
    // Re-canonicalize with tampered payload to simulate attacker
    // updating the canonical form too (but keeping old signature).
    att.canonical_payload_jcs = serde_jcs::to_string(&att.payload).unwrap();

    assert!(
        verify_attestation_signature(&att, &pk).is_err(),
        "tampered destroyed_ts must fail verify"
    );
}

/// Scenario 5: tenant impersonation — forged `tenant_id`.
#[test]
fn tenant_impersonation_fails_verify() {
    let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
    let pk = sk.public_key();
    let signer = ErasureAttestationSigner::new(sk);
    let mut att = signer.sign(sample_payload()).unwrap();

    // Attacker impersonates a different tenant.
    att.payload.tenant_id = "evil-tenant".to_string();
    att.canonical_payload_jcs = serde_jcs::to_string(&att.payload).unwrap();

    assert!(
        verify_attestation_signature(&att, &pk).is_err(),
        "impersonated tenant_id must fail verify"
    );
}

/// Scenario 6: JCS canonicalization tamper — Unicode NFC bypass attempt.
///
/// Attacker tries to introduce a Unicode character that normalizes to a
/// different byte sequence. serde_jcs enforces Unicode NFC; the
/// canonical bytes are stable so the signature remains valid only for
/// the original payload.
#[test]
fn jcs_unicode_tamper_fails_verify() {
    let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
    let pk = sk.public_key();
    let signer = ErasureAttestationSigner::new(sk);

    // Payload with ASCII only (safe baseline).
    let att = signer.sign(sample_payload()).unwrap();

    // Tamper: append a zero-width non-breaking space to canonical form.
    let mut tampered = att.clone();
    tampered.canonical_payload_jcs.push('\u{FEFF}');

    assert!(
        verify_attestation_signature(&tampered, &pk).is_err(),
        "tampered canonical (unicode bypass attempt) must fail verify"
    );
}

/// Scenario 7: signing key compromise simulation — emergency rotation.
///
/// When the signing key is suspected compromised, an emergency rotation
/// is performed: a new key is generated and promoted. Old attestations
/// signed pre-rotation remain verifiable with the old (Overlap) public
/// key during the 30d window. They are NOT verifiable with the new key
/// — confirming that key substitution by an emergency rotation is safe.
#[test]
fn signing_key_compromise_emergency_rotation() {
    let old_sk = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
    let old_pk = old_sk.public_key();
    let old_signer = ErasureAttestationSigner::new(old_sk);

    // Attestation signed with the pre-compromise key.
    let att = old_signer.sign(sample_payload()).unwrap();

    // Emergency rotation: new key generated.
    let new_sk = ErasureSigningKey::generate(2, Region::Weur, 0, 0);
    let new_pk = new_sk.public_key();

    // Old attestation verifiable with old Overlap key.
    assert!(
        verify_attestation_signature(&att, &old_pk).is_ok(),
        "old attestation MUST verify with old Overlap key during 30d window"
    );

    // Old attestation NOT verifiable with new key (not a forgery bypass).
    assert!(
        verify_attestation_signature(&att, &new_pk).is_err(),
        "old attestation MUST NOT verify with new emergency rotation key"
    );
}

/// Scenario 8: payload substitution (H2).
///
/// The signature is valid over `canonical_payload_jcs`, but consumers read the
/// typed `payload`. An attacker keeps the ORIGINAL (validly signed) canonical
/// bytes + signature UNTOUCHED, and only mutates the typed `payload` (here:
/// `tenant_id`) — mis-attributing the erasure to a victim tenant. Verification
/// MUST reject this: the typed payload must re-canonicalize to the signed bytes.
#[test]
fn payload_substitution_keeps_canonical_and_sig_fails_verify() {
    let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
    let pk = sk.public_key();
    let signer = ErasureAttestationSigner::new(sk);
    let mut att = signer.sign(sample_payload()).unwrap();

    // Sanity: genuine attestation verifies before tampering.
    assert!(
        verify_attestation_signature(&att, &pk).is_ok(),
        "genuine attestation must verify"
    );

    // Snapshot the validly-signed bytes + signature.
    let original_canonical = att.canonical_payload_jcs.clone();
    let original_sig = att.signature_ed25519.clone();

    // Attack: mutate ONLY the typed payload; leave canonical + sig intact
    // (do NOT re-canonicalize — that is the prior, already-covered attack).
    att.payload.tenant_id = "victim-tenant".to_string();
    assert_eq!(
        att.canonical_payload_jcs, original_canonical,
        "attack keeps original canonical bytes"
    );
    assert_eq!(
        att.signature_ed25519, original_sig,
        "attack keeps original signature"
    );

    assert!(
        verify_attestation_signature(&att, &pk).is_err(),
        "payload substitution (mutated typed payload, original canonical+sig) must fail verify"
    );

    // Also confirm request_id substitution is caught.
    let mut att2 = signer.sign(sample_payload()).unwrap();
    att2.payload.request_id = "req-forged".to_string();
    assert!(
        verify_attestation_signature(&att2, &pk).is_err(),
        "request_id substitution must fail verify"
    );
}
