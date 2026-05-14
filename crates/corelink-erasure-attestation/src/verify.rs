//! Signature verification logic for [`ErasureAttestation`].

use crate::attestation::ErasureAttestation;
use crate::error::AttestationError;
use crate::key::ErasurePublicKey;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier};

/// Verify the Ed25519 signature of an [`ErasureAttestation`] against the
/// supplied public key.
///
/// The verifier checks:
/// 1. The `signature_ed25519` field decodes from base64 to 64 bytes.
/// 2. The decoded signature verifies against `canonical_payload_jcs`
///    using the provided `public_key.verifying_key`.
///
/// The `public_key.key_id` SHOULD match `attestation.payload.attestation_key_id`;
/// callers MUST select the correct public key from the list returned by
/// `GET /v1/public/keys/erasure/{region}.pub` based on `attestation_key_id`.
///
/// # Errors
///
/// - [`AttestationError::Verify`] if the signature is invalid, forged,
///   or cannot be decoded from base64.
///
/// # Example
///
/// ```rust,no_run
/// use corelink_erasure_attestation::{
///     ErasureAttestation, ErasurePublicKey, verify_attestation_signature,
/// };
///
/// # fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let attestation: ErasureAttestation = serde_json::from_str("{}")?;
/// let public_key: ErasurePublicKey = serde_json::from_str("{}")?;
/// verify_attestation_signature(&attestation, &public_key)?;
/// println!("verified");
/// # Ok(())
/// # }
/// ```
pub fn verify_attestation_signature(
    attestation: &ErasureAttestation,
    public_key: &ErasurePublicKey,
) -> Result<(), AttestationError> {
    // 1. Decode base64 signature.
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&attestation.signature_ed25519)
        .map_err(|e| AttestationError::Verify(format!("base64 decode failed: {e}")))?;

    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| {
        AttestationError::Verify("signature must be exactly 64 bytes".to_string())
    })?;

    let signature = Signature::from_bytes(&sig_arr);

    // 2. Verify against canonical payload bytes.
    public_key
        .verifying_key
        .verify(
            attestation.canonical_payload_jcs.as_bytes(),
            &signature,
        )
        .map_err(|e| AttestationError::Verify(format!("ed25519 verify failed: {e}")))?;

    tracing::debug!(
        request_id = %attestation.payload.request_id,
        key_id = %public_key.key_id,
        "erasure.verify: signature valid",
    );

    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests may use unwrap/expect"
)]
mod tests {
    use super::*;
    use crate::attestation::ErasureAttestationSigner;
    use crate::evidence::EvidenceBundle;
    use crate::key::ErasureSigningKey;
    use crate::region::Region;
    use crate::{ErasureAttestationPayload, ErasurePublicKey};

    fn make_attestation_and_key() -> (ErasureAttestation, ErasurePublicKey) {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
        let pk = sk.public_key();
        let signer = ErasureAttestationSigner::new(sk);
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec!["seg-001".to_string()],
            kms_destroy_ts: 1_700_000_000_000,
            kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
            tenant_id: "tenant-42".to_string(),
        };
        let payload = ErasureAttestationPayload {
            tenant_id: "tenant-42".to_string(),
            request_id: "req-001".to_string(),
            destroyed_ts: 1_700_000_000_000,
            kms_provider: "aws_kms".to_string(),
            kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
            evidence_hash: EvidenceBundle::compute_hash(&bundle),
            region: Region::Weur,
            attestation_key_id: 1,
        };
        let att = signer.sign(payload).unwrap();
        (att, pk)
    }

    #[test]
    fn valid_signature_verifies() {
        let (att, pk) = make_attestation_and_key();
        verify_attestation_signature(&att, &pk).expect("should verify");
    }

    #[test]
    fn tampered_canonical_fails_verify() {
        let (mut att, pk) = make_attestation_and_key();
        att.canonical_payload_jcs.push('!');
        let err = verify_attestation_signature(&att, &pk).unwrap_err();
        assert!(matches!(err, AttestationError::Verify(_)));
    }

    #[test]
    fn wrong_key_fails_verify() {
        let (att, _pk) = make_attestation_and_key();
        // Generate a completely different key.
        let other_sk = ErasureSigningKey::generate(99, Region::Weur, 0, 0);
        let other_pk = other_sk.public_key();
        let err = verify_attestation_signature(&att, &other_pk).unwrap_err();
        assert!(matches!(err, AttestationError::Verify(_)));
    }

    #[test]
    fn invalid_base64_sig_fails() {
        let (mut att, pk) = make_attestation_and_key();
        att.signature_ed25519 = "not-valid-base64!!!".to_string();
        let err = verify_attestation_signature(&att, &pk).unwrap_err();
        assert!(matches!(err, AttestationError::Verify(_)));
    }

    #[test]
    fn truncated_sig_fails() {
        let (mut att, pk) = make_attestation_and_key();
        // Encode only 32 bytes instead of 64.
        att.signature_ed25519 =
            base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        let err = verify_attestation_signature(&att, &pk).unwrap_err();
        assert!(matches!(err, AttestationError::Verify(_)));
    }
}
