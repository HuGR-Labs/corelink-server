//! AC envelope sign / verify trait surface (WI-S04-001 §6.1.5–§6.1.6).
//!
//! Per the charter trait-abstraction-defer pattern, the WI-S04-001
//! handler depends on the [`Signer`] trait surface; the real HKDF
//! tenant-key impl ships in WI-S04-004 (`corelink-ac::sig`). This
//! module defines the trait, the canonical [`AcEnvelope`] payload that
//! crosses the trait boundary, and an [`InMemoryFakeSigner`] that
//! exercises every code path for the handler property tests.
//!
//! ## Canonical envelope shape
//!
//! [`AcEnvelope`] holds the **canonical 121-byte preimage** the
//! signature is computed over per ADR-0021 + Lote 10.4-tris P0-R5-003:
//! `version(1B) || sig_key_id(4B) || tenant_id(16B) || action_digest_hash(32B) ||
//! action_digest_size(8B) || result_hash(32B) || sig_key_id_again(4B) || pad(24B)`.
//! The fake signer simply HMAC-SHAs the canonical_bytes with a fixed
//! per-tenant key derived deterministically (per
//! [`InMemoryFakeSigner::derive_key`]); the real impl in WI-S04-004
//! uses HKDF-SHA256 with `info = b"ac-sig"` per ADR-0021.
//!
//! The fake's signature is **distinct** between any two tenants AND
//! between any two `(action_digest, result_hash)` pairs — sufficient
//! for the property-test boundary (cross-tenant + tampering detection
//! both surface as `SigError::Mismatch`).

use core::fmt;

use thiserror::Error;
use uuid::Uuid;

use super::types::{ActionDigest, ResultHash};

/// Length of the canonical AC envelope preimage (bytes).
///
/// Layout per ADR-0021 v1.0.0 §3 + Lote 10.4-tris P0-R5-003 fix:
/// fixed 121-byte big-endian shape so the chain hash + sig verifier
/// have an unambiguous canonical encoding regardless of host endian /
/// proto evolution. The fake honors the same shape so any drift
/// between fake and prod is caught at compile-time (`const`-asserted).
pub const AC_ENVELOPE_PREIMAGE_LEN: usize = 121;

/// Length of the canonical AC envelope signature output (bytes).
///
/// HMAC-SHA256 / HKDF-SHA256 both emit a 32-byte tag; the fake honors
/// the same shape.
pub const AC_ENVELOPE_SIG_LEN: usize = 32;

/// Reserved sig key id sentinel (`0`) — every real sig_key_id is
/// `>= 1` per ADR-0021 §P0-R5-001 (key_id=0 is sentinel for "no key
/// available"; signing or verifying with key_id=0 is a programmer
/// error).
pub const RESERVED_SIG_KEY_ID: u32 = 0;

/// A signed AC envelope ready for persistence into R2 (the production
/// path) or the in-memory fake (the test path).
///
/// The `canonical_bytes` field is the 121-byte preimage; the
/// `signature` field is the 32-byte HMAC-SHA256 / HKDF-SHA256 output;
/// the `sig_key_id` field tracks the rotation version used for
/// signing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcEnvelope {
    /// Canonical 121-byte preimage. Layout per ADR-0021.
    pub canonical_bytes: [u8; AC_ENVELOPE_PREIMAGE_LEN],
    /// 32-byte signature over `canonical_bytes`.
    pub signature: [u8; AC_ENVELOPE_SIG_LEN],
    /// Sig key id used for signing — must be `>= 1`
    /// ([`RESERVED_SIG_KEY_ID`] is forbidden).
    pub sig_key_id: u32,
}

impl AcEnvelope {
    /// Compose the canonical 121-byte preimage from its components.
    ///
    /// Layout offsets are fixed `const` and asserted at compile time
    /// against [`AC_ENVELOPE_PREIMAGE_LEN`] so the slice-indexing
    /// arithmetic is provably in-bounds; the
    /// `clippy::indexing_slicing` allow on the function body is
    /// scoped to this canonical-shape constant write only.
    ///
    /// # Errors
    ///
    /// Returns [`SigError::KeyIdReserved`] when `sig_key_id == 0`
    /// (per ADR-0021 §P0-R5-001).
    #[allow(
        clippy::indexing_slicing,
        reason = "all slice indices are compile-time constants asserted against AC_ENVELOPE_PREIMAGE_LEN; bounds are provably in-range"
    )]
    pub fn canonicalize(
        version: u8,
        sig_key_id: u32,
        tenant_id: Uuid,
        action_digest: &ActionDigest,
        result_hash: &ResultHash,
    ) -> Result<[u8; AC_ENVELOPE_PREIMAGE_LEN], SigError> {
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        // Layout offsets — `const` + compile-time assert keeps the
        // 121-byte canonical shape pinned across the crate.
        const VERSION_OFFSET: usize = 0;
        const SIG_KEY_ID_OFFSET: usize = 1;
        const TENANT_ID_OFFSET: usize = 5;
        const ACTION_HASH_OFFSET: usize = 21;
        const ACTION_SIZE_OFFSET: usize = 53;
        const RESULT_HASH_OFFSET: usize = 61;
        const SIG_KEY_ID_REPEAT_OFFSET: usize = 93;
        const PAD_OFFSET: usize = 97;
        const _: () = assert!(AC_ENVELOPE_PREIMAGE_LEN == 1 + 4 + 16 + 32 + 8 + 32 + 4 + 24);
        const _: () = assert!(PAD_OFFSET + 24 == AC_ENVELOPE_PREIMAGE_LEN);

        let mut buf = [0u8; AC_ENVELOPE_PREIMAGE_LEN];
        buf[VERSION_OFFSET] = version;
        buf[SIG_KEY_ID_OFFSET..SIG_KEY_ID_OFFSET + 4].copy_from_slice(&sig_key_id.to_be_bytes());
        buf[TENANT_ID_OFFSET..TENANT_ID_OFFSET + 16].copy_from_slice(tenant_id.as_bytes());
        buf[ACTION_HASH_OFFSET..ACTION_HASH_OFFSET + 32]
            .copy_from_slice(action_digest.hash.as_bytes());
        buf[ACTION_SIZE_OFFSET..ACTION_SIZE_OFFSET + 8]
            .copy_from_slice(&action_digest.size_bytes.to_be_bytes());
        buf[RESULT_HASH_OFFSET..RESULT_HASH_OFFSET + 32]
            .copy_from_slice(result_hash.as_digest().as_bytes());
        buf[SIG_KEY_ID_REPEAT_OFFSET..SIG_KEY_ID_REPEAT_OFFSET + 4]
            .copy_from_slice(&sig_key_id.to_be_bytes());
        // The `PAD_OFFSET..AC_ENVELOPE_PREIMAGE_LEN` window is already
        // zero from the `[0u8; …]` initializer.
        Ok(buf)
    }
}

/// Errors surfaced by [`Signer::sign`] / [`Signer::verify`].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SigError {
    /// The signature did not match the recomputed value. Maps to 422
    /// `COR_AC_SIG_INVALID` + audit `ac.get.sig_invalid` (CRITICAL).
    #[error("ac envelope signature mismatch")]
    Mismatch,
    /// Programmer error: `sig_key_id == 0` is the reserved sentinel.
    #[error("ac envelope sig_key_id is reserved (must be >= 1)")]
    KeyIdReserved,
    /// Backend crypto fault — only meaningful in the production
    /// HKDF impl (e.g., key material missing under rotation race).
    /// Maps to 503 `COR_AC_BACKEND_UNAVAILABLE`.
    #[error("ac signer backend error: {0}")]
    Backend(String),
}

impl SigError {
    /// Whether this variant maps to the reserved-key sentinel — used
    /// by [`Signer::sign`] callers to short-circuit before issuing a
    /// failing audit emit (this is a programmer error, not a security
    /// signal).
    #[must_use]
    pub const fn is_key_id_reserved(&self) -> bool {
        matches!(self, Self::KeyIdReserved)
    }
}

/// AC envelope signer + verifier trait. Real impl ships in
/// WI-S04-004 (`corelink-ac::sig` HKDF-SHA256).
pub trait Signer: Send + Sync {
    /// Sign the canonical preimage bytes with the per-tenant key
    /// derived from `tenant_id` + the configured `sig_key_id`.
    ///
    /// Returns the signature bytes (`AC_ENVELOPE_SIG_LEN` long). The
    /// caller composes the [`AcEnvelope`] explicitly from
    /// `(canonical_bytes, signature, sig_key_id)` so the producer + the
    /// chain consumer share an identical envelope shape.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when `sig_key_id == 0`.
    /// - [`SigError::Backend`] on production backend fault.
    fn sign(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
    ) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError>;

    /// Verify `signature` against the recomputed canonical bytes.
    ///
    /// Comparison MUST be constant-time on the production HKDF impl
    /// (`subtle::ConstantTimeEq`) — the fake here does the same.
    ///
    /// # Errors
    ///
    /// - [`SigError::Mismatch`] when the signature does not match the
    ///   expected value computed from `tenant_id` / `sig_key_id` /
    ///   `canonical_bytes`.
    /// - [`SigError::KeyIdReserved`] when `sig_key_id == 0`.
    /// - [`SigError::Backend`] on production backend fault.
    fn verify(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
        signature: &[u8; AC_ENVELOPE_SIG_LEN],
    ) -> Result<(), SigError>;
}

/// Test-only signer that derives a stable per-tenant key and HMAC-SHAs
/// the canonical bytes. Uses [`subtle::ConstantTimeEq`] for the
/// verification comparison so the property tests exercise the same
/// timing seam the production HKDF impl will exhibit.
#[derive(Clone, Copy, Default)]
pub struct InMemoryFakeSigner;

impl fmt::Debug for InMemoryFakeSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryFakeSigner").finish()
    }
}

impl InMemoryFakeSigner {
    /// Construct a fresh fake signer. The fake holds no state — every
    /// instance derives the same per-tenant key for the same
    /// `(tenant_id, sig_key_id)` pair.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Deterministic per-tenant key derivation. `key = SHA256(tenant_id ||
    /// sig_key_id)`. NOT cryptographic; satisfies the property-test
    /// requirements (every distinct `(tenant_id, sig_key_id)` pair
    /// yields a distinct key, and the same pair yields the same key
    /// across instances).
    #[must_use]
    fn derive_key(tenant_id: Uuid, sig_key_id: u32) -> [u8; 32] {
        use sha2::{Digest as _, Sha256};
        let mut h = Sha256::new();
        h.update(b"corelink-ac-fake-signer-v1");
        h.update(tenant_id.as_bytes());
        h.update(sig_key_id.to_be_bytes());
        let out = h.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&out);
        buf
    }

    fn sign_inner(
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
    ) -> [u8; AC_ENVELOPE_SIG_LEN] {
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<sha2::Sha256>;
        let key = Self::derive_key(tenant_id, sig_key_id);
        let mut mac = match HmacSha256::new_from_slice(&key) {
            Ok(m) => m,
            // HMAC-SHA256 accepts any key length; this branch is
            // unreachable. Surface as zero-vector (treated as
            // mismatch by every legitimate verify).
            Err(_) => return [0u8; AC_ENVELOPE_SIG_LEN],
        };
        mac.update(canonical_bytes);
        let tag = mac.finalize().into_bytes();
        let mut out = [0u8; AC_ENVELOPE_SIG_LEN];
        out.copy_from_slice(&tag);
        out
    }
}

impl Signer for InMemoryFakeSigner {
    fn sign(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
    ) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError> {
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        Ok(Self::sign_inner(tenant_id, sig_key_id, canonical_bytes))
    }

    fn verify(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
        signature: &[u8; AC_ENVELOPE_SIG_LEN],
    ) -> Result<(), SigError> {
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        let expected = Self::sign_inner(tenant_id, sig_key_id, canonical_bytes);
        // Constant-time compare via subtle so timing-based bypass is
        // unreachable. (subtle is already a workspace dep.)
        use subtle::ConstantTimeEq;
        if expected.ct_eq(signature).into() {
            Ok(())
        } else {
            Err(SigError::Mismatch)
        }
    }
}

/// Helper: BLAKE3 wrapper for sig-key derivation rotation
/// equivalence check used by the property tests.
#[doc(hidden)]
#[must_use]
pub fn debug_fake_key(tenant_id: Uuid, sig_key_id: u32) -> [u8; 32] {
    InMemoryFakeSigner::derive_key(tenant_id, sig_key_id)
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
    use corelink_hash::Digest;

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    fn fixed_action_digest() -> ActionDigest {
        ActionDigest::new(Digest::compute(b"action"), 1234)
    }

    fn fixed_result_hash() -> ResultHash {
        ResultHash::compute(&super::super::types::ActionResult::new(
            Vec::new(),
            Vec::new(),
            0,
            b"result".to_vec(),
        ))
    }

    #[test]
    fn canonical_preimage_is_121_bytes() {
        let bytes = AcEnvelope::canonicalize(
            1,
            42,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap();
        assert_eq!(bytes.len(), AC_ENVELOPE_PREIMAGE_LEN);
        // Version byte
        assert_eq!(bytes[0], 1);
        // sig_key_id big-endian
        assert_eq!(&bytes[1..5], &42_u32.to_be_bytes());
    }

    #[test]
    fn canonical_preimage_rejects_reserved_key_id() {
        let err = AcEnvelope::canonicalize(
            1,
            RESERVED_SIG_KEY_ID,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap_err();
        assert!(err.is_key_id_reserved());
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let signer = InMemoryFakeSigner::new();
        let bytes = AcEnvelope::canonicalize(
            1,
            7,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap();
        let sig = signer.sign(fixed_tenant(), 7, &bytes).unwrap();
        signer.verify(fixed_tenant(), 7, &bytes, &sig).unwrap();
    }

    #[test]
    fn cross_tenant_signature_does_not_verify() {
        let signer = InMemoryFakeSigner::new();
        let bytes = AcEnvelope::canonicalize(
            1,
            7,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap();
        let sig_a = signer.sign(fixed_tenant(), 7, &bytes).unwrap();
        let other = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
        let err = signer.verify(other, 7, &bytes, &sig_a).unwrap_err();
        assert_eq!(err, SigError::Mismatch);
    }

    #[test]
    fn byte_flip_breaks_verify() {
        let signer = InMemoryFakeSigner::new();
        let bytes = AcEnvelope::canonicalize(
            1,
            7,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap();
        let sig = signer.sign(fixed_tenant(), 7, &bytes).unwrap();
        let mut tampered = bytes;
        tampered[10] ^= 0x01;
        let err = signer
            .verify(fixed_tenant(), 7, &tampered, &sig)
            .unwrap_err();
        assert_eq!(err, SigError::Mismatch);
    }

    #[test]
    fn distinct_key_ids_yield_distinct_sigs() {
        let signer = InMemoryFakeSigner::new();
        let bytes1 = AcEnvelope::canonicalize(
            1,
            7,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap();
        let bytes2 = AcEnvelope::canonicalize(
            1,
            8,
            fixed_tenant(),
            &fixed_action_digest(),
            &fixed_result_hash(),
        )
        .unwrap();
        let s1 = signer.sign(fixed_tenant(), 7, &bytes1).unwrap();
        let s2 = signer.sign(fixed_tenant(), 8, &bytes2).unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn reserved_key_id_rejected_on_sign_and_verify() {
        let signer = InMemoryFakeSigner::new();
        let bytes = [0u8; AC_ENVELOPE_PREIMAGE_LEN];
        let err = signer.sign(fixed_tenant(), 0, &bytes).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
        let err = signer
            .verify(fixed_tenant(), 0, &bytes, &[0u8; AC_ENVELOPE_SIG_LEN])
            .unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn debug_fake_key_distinct_per_tenant() {
        let a = debug_fake_key(fixed_tenant(), 1);
        let b = debug_fake_key(
            Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap(),
            1,
        );
        assert_ne!(a, b);
    }
}
