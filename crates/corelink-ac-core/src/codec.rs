//! Canonical [`AcEnvelope`] codec — JSON encode + decode with bounded
//! payload enforcement (WI-S04-003 §6.1.7 + §6.1.5).
//!
//! The wire format is `serde_json` over the [`AcEnvelope`] struct.
//! `serde_json` does not natively guarantee canonical key order, but
//! the struct shape ([fields in fixed order]) plus a single
//! authorized encoder (this module's [`encode`]) yields byte-stable
//! output; any consumer that wants pure JCS canonicalization can
//! re-encode via `serde_jcs` independently. The Merkle binding is
//! computed from the digest set, **not** from the JSON bytes — JSON
//! is a transport, not the cripto authority (per ADR-0037).
//!
//! Bounded-parser discipline: [`decode`] enforces
//! [`crate::bounds::MAX_PAYLOAD_BYTES`] on the input slice **before**
//! invoking `serde_json::from_slice`, so an oversized envelope is
//! rejected with `MerkleError::PayloadExceeded` without `serde_json`
//! ever allocating a `Vec<Value>` for the malformed input.

use crate::bounds::MAX_PAYLOAD_BYTES;
use crate::error::MerkleError;
use crate::types::{AcEnvelope, AC_ENVELOPE_VERSION};

/// Encode an [`AcEnvelope`] to canonical JSON bytes.
///
/// # Errors
///
/// Surfaces [`MerkleError::DecodeError`] if `serde_json` reports an
/// error (extremely rare; only triggered by `f64::NaN` etc., which
/// the envelope shape cannot produce). Surfaces
/// [`MerkleError::PayloadExceeded`] if the encoded length exceeds the
/// canonical bound.
pub fn encode(envelope: &AcEnvelope) -> Result<Vec<u8>, MerkleError> {
    if envelope.version != AC_ENVELOPE_VERSION {
        return Err(MerkleError::VersionUnsupported(envelope.version));
    }
    let bytes = serde_json::to_vec(envelope)
        .map_err(|e| MerkleError::DecodeError(e.to_string()))?;
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(MerkleError::PayloadExceeded {
            found_bytes: bytes.len(),
            bound: MAX_PAYLOAD_BYTES,
        });
    }
    Ok(bytes)
}

/// Decode an [`AcEnvelope`] from canonical JSON bytes. Bounds check
/// is applied **before** the JSON parse.
///
/// # Errors
///
/// - [`MerkleError::PayloadExceeded`] when the input slice exceeds
///   [`MAX_PAYLOAD_BYTES`].
/// - [`MerkleError::DecodeError`] when the JSON parse fails.
/// - [`MerkleError::VersionUnsupported`] when the decoded
///   `version` field is not the canonical v1.
pub fn decode(bytes: &[u8]) -> Result<AcEnvelope, MerkleError> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(MerkleError::PayloadExceeded {
            found_bytes: bytes.len(),
            bound: MAX_PAYLOAD_BYTES,
        });
    }
    let env: AcEnvelope = serde_json::from_slice(bytes)
        .map_err(|e| MerkleError::DecodeError(e.to_string()))?;
    if env.version != AC_ENVELOPE_VERSION {
        return Err(MerkleError::VersionUnsupported(env.version));
    }
    Ok(env)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use crate::types::{
        ActionDigest, ActionResult, OutputFileDigest, MERKLE_ROOT_LEN, RESULT_HASH_LEN,
    };
    use corelink_hash::Digest;

    fn fresh_envelope() -> AcEnvelope {
        let h = Digest::compute(b"action-x");
        let action_digest = ActionDigest::new(h, 100);
        let result = ActionResult::new(
            vec![OutputFileDigest::new(Digest::compute(b"out-1"), 7)],
            Vec::new(),
            0,
            b"raw".to_vec(),
        );
        AcEnvelope::new(
            "00000000-0000-7000-8000-000000000000".to_string(),
            action_digest,
            result,
            [0xAA; MERKLE_ROOT_LEN],
            [0xBB; RESULT_HASH_LEN],
            1_700_000_000_000,
            vec![1, 2, 3],
            42,
        )
    }

    #[test]
    fn round_trip_preserves_fields() {
        let env = fresh_envelope();
        let bytes = encode(&env).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn round_trip_byte_stable_three_times() {
        let env = fresh_envelope();
        let b1 = encode(&env).unwrap();
        let b2 = encode(&decode(&b1).unwrap()).unwrap();
        let b3 = encode(&decode(&b2).unwrap()).unwrap();
        assert_eq!(b1, b2);
        assert_eq!(b2, b3);
    }

    #[test]
    fn decode_rejects_oversized_payload() {
        let huge = vec![b'a'; MAX_PAYLOAD_BYTES + 1];
        let err = decode(&huge).unwrap_err();
        assert!(matches!(err, MerkleError::PayloadExceeded { .. }));
    }

    #[test]
    fn decode_rejects_garbage_json() {
        let err = decode(b"not-json{").unwrap_err();
        assert!(matches!(err, MerkleError::DecodeError(_)));
    }

    #[test]
    fn decode_rejects_version_two() {
        let mut env = fresh_envelope();
        env.version = 2;
        // We have to bypass the encode-side version check to land
        // a v2 byte payload in the test; bypass via direct
        // serialization.
        let raw = serde_json::to_vec(&env).unwrap();
        let err = decode(&raw).unwrap_err();
        assert!(matches!(err, MerkleError::VersionUnsupported(2)));
    }

    #[test]
    fn encode_rejects_version_two() {
        let mut env = fresh_envelope();
        env.version = 2;
        let err = encode(&env).unwrap_err();
        assert!(matches!(err, MerkleError::VersionUnsupported(2)));
    }
}
