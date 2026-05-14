//! [`ErasureAttestationPayload`], [`ErasureAttestation`], and
//! [`ErasureAttestationSigner`] — core signing flow.

use crate::error::AttestationError;
use crate::key::ErasureSigningKey;
use crate::region::Region;
use base64::Engine as _;
use ed25519_dalek::Signer as _;
use serde::{Deserialize, Serialize};

/// Payload fields that are JCS-canonicalized and signed.
///
/// All fields are included in the canonical JSON; ordering is
/// deterministic via RFC 8785 (keys sorted lexicographically by
/// `serde_jcs`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErasureAttestationPayload {
    /// BYOK tenant identifier.
    pub tenant_id: String,

    /// DSR erasure request identifier (UUID v4; UNIQUE constraint in D1).
    pub request_id: String,

    /// Millisecond timestamp when CMK access was destroyed (server-side).
    pub destroyed_ts: u64,

    /// KMS provider string (e.g. `"aws_kms"`, `"gcp_kms"`, `"azure_kv"`,
    /// `"vault"`).
    pub kms_provider: String,

    /// KMS key ID (ARN / resource name) that was destroyed.
    pub kms_key_id: String,

    /// SHA-256 hex digest of the [`EvidenceBundle`] binding audit chain
    /// segment IDs + KMS destroy timestamp + KMS key_id + tenant_id.
    /// (NIST SP 800-88 Rev.1 §2.4 evidence binding.)
    pub evidence_hash: String,

    /// Region where the erasure occurred.
    pub region: Region,

    /// Key ID of the Ed25519 signing key used (maps to
    /// `erasure_public_keys.key_id` in D1 for verify endpoint lookup).
    pub attestation_key_id: u64,
}

/// A signed erasure attestation — payload + Ed25519 signature + JCS
/// canonical form.
///
/// Persisted to R2 audit bucket with 7-year retention and indexed in D1.
/// Delivered to customer + auditor for offline verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureAttestation {
    /// The attestation payload (all user-visible fields).
    pub payload: ErasureAttestationPayload,

    /// Base64-encoded Ed25519 signature over `canonical_payload_jcs`.
    pub signature_ed25519: String,

    /// RFC 8785 JCS-canonicalized payload JSON that was signed.
    /// A verifier MUST verify the signature against this exact byte string.
    pub canonical_payload_jcs: String,
}

/// Signs erasure attestation payloads using an in-memory
/// [`ErasureSigningKey`].
///
/// In production the signing key is loaded from a Cloudflare Workers
/// Secret by the erasure worker; this struct provides the pure-logic
/// signing surface for CI and unit tests.
#[derive(Debug)]
pub struct ErasureAttestationSigner {
    signing_key: ErasureSigningKey,
}

impl ErasureAttestationSigner {
    /// Construct a signer from a signing key.
    #[must_use]
    pub fn new(signing_key: ErasureSigningKey) -> Self {
        Self { signing_key }
    }

    /// Sign an erasure attestation payload.
    ///
    /// Steps:
    /// 1. Canonicalize payload via RFC 8785 JCS (`serde_jcs`).
    /// 2. Sign the canonical bytes with Ed25519.
    /// 3. Base64-encode the 64-byte signature.
    ///
    /// # Errors
    ///
    /// - [`AttestationError::Canonicalization`] if `serde_jcs` fails.
    /// - [`AttestationError::Sign`] if ed25519-dalek signing fails.
    pub fn sign(
        &self,
        payload: ErasureAttestationPayload,
    ) -> Result<ErasureAttestation, AttestationError> {
        // 1. JCS canonicalize — RFC 8785 deterministic JSON.
        let canonical = serde_jcs::to_string(&payload)
            .map_err(|e| AttestationError::Canonicalization(e.to_string()))?;

        // 2. Sign canonical bytes.
        let sig = self
            .signing_key
            .signing_key
            .sign(canonical.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

        tracing::debug!(
            region = %payload.region,
            request_id = %payload.request_id,
            attestation_key_id = %payload.attestation_key_id,
            "erasure.sign: signed attestation",
        );

        Ok(ErasureAttestation {
            payload,
            signature_ed25519: sig_b64,
            canonical_payload_jcs: canonical,
        })
    }

    /// Return the key ID of the signing key.
    #[must_use]
    pub fn key_id(&self) -> u64 {
        self.signing_key.key_id
    }

    /// Return the region this signer is bound to.
    #[must_use]
    pub fn region(&self) -> Region {
        self.signing_key.region
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests may use unwrap/expect"
)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceBundle;
    use crate::key::ErasureSigningKey;

    fn sample_payload() -> ErasureAttestationPayload {
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec!["seg-001".to_string()],
            kms_destroy_ts: 1_700_000_000_000,
            kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
            tenant_id: "tenant-42".to_string(),
        };
        ErasureAttestationPayload {
            tenant_id: "tenant-42".to_string(),
            request_id: "req-001".to_string(),
            destroyed_ts: 1_700_000_000_000,
            kms_provider: "aws_kms".to_string(),
            kms_key_id: "arn:aws:kms:eu-west-1:123:key/abc".to_string(),
            evidence_hash: EvidenceBundle::compute_hash(&bundle),
            region: Region::Weur,
            attestation_key_id: 1,
        }
    }

    #[test]
    fn sign_produces_64_byte_base64_sig() {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 30 * 24 * 3_600 * 1_000);
        let signer = ErasureAttestationSigner::new(sk);
        let att = signer.sign(sample_payload()).expect("sign");
        // Ed25519 signature = 64 bytes → base64 = 88 chars
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(&att.signature_ed25519)
            .expect("base64 decode");
        assert_eq!(sig_bytes.len(), 64);
    }

    #[test]
    fn canonical_is_deterministic() {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
        let signer = ErasureAttestationSigner::new(sk);
        let p1 = sample_payload();
        let p2 = sample_payload();
        let a1 = signer.sign(p1).unwrap();
        let a2 = signer.sign(p2).unwrap();
        assert_eq!(a1.canonical_payload_jcs, a2.canonical_payload_jcs);
    }

    #[test]
    fn different_payloads_different_canonical() {
        let sk = ErasureSigningKey::generate(1, Region::Weur, 0, 0);
        let signer = ErasureAttestationSigner::new(sk);
        let mut p2 = sample_payload();
        p2.request_id = "req-002".to_string();
        let a1 = signer.sign(sample_payload()).unwrap();
        let a2 = signer.sign(p2).unwrap();
        assert_ne!(a1.canonical_payload_jcs, a2.canonical_payload_jcs);
        assert_ne!(a1.signature_ed25519, a2.signature_ed25519);
    }
}
