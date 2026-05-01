//! HKDF-SHA256 signer + verifier real implementation (WI-S04-004 §1).
//!
//! Cripto algorithm summary (ADR-0021 §1):
//!
//! ```text
//! sig_key = HKDF-Expand( HKDF-Extract( salt = sig_key_id.to_le_bytes(),
//!                                       IKM  = TDK ),
//!                         info = b"ac-sig",
//!                         L    = 32 )
//! sig    = blake3::keyed_hash(&sig_key, canonical_bytes)
//! ```
//!
//! - **Salt = `sig_key_id.to_le_bytes()`** (4 bytes; binds the rotation
//!   version into the Extract step per Lote 10.4bis P0 fix).
//! - **Info = `b"ac-sig"`** (constant; CI gate enforces byte-equal).
//! - **MAC primitive** = BLAKE3 keyed-hash; 256-bit MAC; 2^128 PRF-secure
//!   forge resistance.
//! - **Constant-time verify**: [`subtle::ConstantTimeEq`] on the
//!   recomputed-vs-supplied tag.
//!
//! ## Trait surfaces
//!
//! - [`HkdfSigner`] implements [`crate::sig::SignatureSigner`] —
//!   production handler `UpdateActionResult` pre-persist hook.
//! - [`HkdfVerifier`] implements [`crate::sig::SignatureVerifier`] —
//!   production handler `GetActionResult` post-fetch hook AND the
//!   client SDK dual-side post-download verifier.
//!
//! Both surfaces hold an `Arc<dyn TdkHandle>` so they can be shared
//! freely across async tasks; `Send + Sync` is enforced by trait
//! bounds. Cheap to clone (only the Arc is bumped).

use std::sync::Arc;

use hkdf::Hkdf;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::canonical::AC_ENVELOPE_PREIMAGE_LEN;
use super::error::SigError;
use super::tdk::TdkHandle;
use super::{SignatureSigner, SignatureVerifier, RESERVED_SIG_KEY_ID};

/// HKDF info bytes — `b"ac-sig"` per ADR-0021 §1. Constant; CI gate
/// in [`crate::sig::tests::canonical_info_string`] asserts byte-equal.
pub const HKDF_INFO_AC_SIG: &[u8] = b"ac-sig";

/// Canonical signature byte length (`32`). HKDF-Expand outputs 32
/// bytes; BLAKE3 keyed-hash also outputs 32 bytes. Matches the worker
/// side-fake's `AC_ENVELOPE_SIG_LEN`.
pub const AC_ENVELOPE_SIG_LEN: usize = 32;

/// Canonical TDK byte length (`32`); re-export from [`super::tdk`] so
/// signer / verifier callers don't need a sibling-module use line.
pub use super::tdk::TDK_LEN;

/// HKDF-Expand sig_key derivation. Pulled out so the sign + verify
/// paths share one canonical implementation; eliminates the risk of a
/// refactor that drifts one side from the other.
fn derive_sig_key(tdk_bytes: &[u8], sig_key_id: u32) -> Result<Zeroizing<[u8; 32]>, SigError> {
    // Salt binds the rotation version into the Extract step — TDK
    // rotation produces correlated keying material; the salt removes
    // that correlation (industry-SOTA per TLS 1.3, Signal Protocol).
    let salt = sig_key_id.to_le_bytes();
    let hk = Hkdf::<Sha256>::new(Some(&salt), tdk_bytes);
    let mut sig_key = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO_AC_SIG, sig_key.as_mut_slice())
        .map_err(|e| SigError::TdkDerivationFailed(e.to_string()))?;
    Ok(sig_key)
}

/// Compute the BLAKE3 keyed-hash MAC tag.
fn keyed_mac(sig_key: &[u8; 32], canonical_bytes: &[u8]) -> [u8; AC_ENVELOPE_SIG_LEN] {
    blake3::keyed_hash(sig_key, canonical_bytes).into()
}

/// HKDF-SHA256 + BLAKE3-keyed signer. Production impl.
///
/// Holds an `Arc<dyn TdkHandle>` for the per-tenant TDK lookup. The
/// signer issues a single "signing" `sig_key_id` (`current_key_id`); a
/// rotation event bumps `current_key_id`, after which all newly-emitted
/// envelopes carry the new value. Existing envelopes signed with older
/// values still verify under the [`HkdfVerifier::accepted_key_ids`]
/// grace whitelist.
#[derive(Clone)]
pub struct HkdfSigner {
    tdk_handle: Arc<dyn TdkHandle>,
    current_key_id: u32,
}

impl std::fmt::Debug for HkdfSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HkdfSigner")
            .field("current_key_id", &self.current_key_id)
            .field("tdk_handle", &self.tdk_handle)
            .finish()
    }
}

impl HkdfSigner {
    /// Construct a fresh signer. Returns
    /// [`SigError::KeyIdReserved`] when `current_key_id == 0` — the
    /// sentinel value per ADR-0021 §Sentinel.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when `current_key_id == 0`.
    pub fn new(
        tdk_handle: Arc<dyn TdkHandle>,
        current_key_id: u32,
    ) -> Result<Self, SigError> {
        if current_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        Ok(Self {
            tdk_handle,
            current_key_id,
        })
    }

    /// Active sig_key_id used for new signatures.
    #[must_use]
    pub const fn current_key_id(&self) -> u32 {
        self.current_key_id
    }

    /// Bump `current_key_id` to a new rotation value (TDK rotation
    /// event). Returns [`SigError::KeyIdReserved`] when the new id is
    /// `0`.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when `new_key_id == 0`.
    pub fn rotate_to(&mut self, new_key_id: u32) -> Result<(), SigError> {
        if new_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        self.current_key_id = new_key_id;
        Ok(())
    }
}

impl SignatureSigner for HkdfSigner {
    fn sign(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
    ) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError> {
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        // The signer's "current" key_id is the one we expect callers to
        // request; if they ask for a different value the TdkHandle
        // backend may still satisfy it (rotation grace window), so we
        // don't enforce equality here. Equality enforcement is a
        // policy decision that lives at the handler layer.
        let tdk = self.tdk_handle.fetch(tenant_id, sig_key_id)?;
        let sig_key = derive_sig_key(tdk.as_bytes(), sig_key_id)?;
        Ok(keyed_mac(&sig_key, canonical_bytes))
    }
}

/// HKDF-SHA256 + BLAKE3-keyed verifier. Production impl.
///
/// Holds an `Arc<dyn TdkHandle>` and a sorted `accepted_key_ids`
/// whitelist (canonical production deployment: current + 1 previous;
/// post-incident grace ≤ 4). Verify rejects:
///
/// - `signature.len() != AC_ENVELOPE_SIG_LEN` →
///   [`SigError::LengthMismatch`] (length is public; fast-fail).
/// - `sig_key_id == 0` → [`SigError::KeyIdReserved`] (sentinel).
/// - `sig_key_id` not in whitelist → [`SigError::KeyIdUnknown`].
/// - cripto compare fails → [`SigError::Invalid`] (constant-time).
#[derive(Clone)]
pub struct HkdfVerifier {
    tdk_handle: Arc<dyn TdkHandle>,
    accepted_key_ids: Vec<u32>,
}

impl std::fmt::Debug for HkdfVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HkdfVerifier")
            .field("accepted_key_ids", &self.accepted_key_ids)
            .field("tdk_handle", &self.tdk_handle)
            .finish()
    }
}

impl HkdfVerifier {
    /// Construct a fresh verifier. Returns errors:
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] if any element of
    ///   `accepted_key_ids` is `0` — the sentinel must never be
    ///   accepted (per ADR-0021 §Sentinel).
    /// - [`SigError::TdkDerivationFailed`] (with reason `accepted_key_ids
    ///   must be non-empty`) when `accepted_key_ids` is empty —
    ///   constructing a verifier that accepts no keys is a programmer
    ///   error.
    pub fn new(
        tdk_handle: Arc<dyn TdkHandle>,
        accepted_key_ids: Vec<u32>,
    ) -> Result<Self, SigError> {
        if accepted_key_ids.is_empty() {
            return Err(SigError::TdkDerivationFailed(
                "accepted_key_ids must be non-empty".to_string(),
            ));
        }
        if accepted_key_ids.contains(&RESERVED_SIG_KEY_ID) {
            return Err(SigError::KeyIdReserved);
        }
        // Sort ascending so `oldest_active` reads the smallest entry
        // in O(1). Dedup preserves callable semantics if the caller
        // passes duplicates accidentally.
        let mut accepted_key_ids = accepted_key_ids;
        accepted_key_ids.sort_unstable();
        accepted_key_ids.dedup();
        Ok(Self {
            tdk_handle,
            accepted_key_ids,
        })
    }

    /// Currently-accepted `sig_key_id`s, sorted ascending.
    #[must_use]
    pub fn accepted_key_ids(&self) -> &[u32] {
        &self.accepted_key_ids
    }

    /// Smallest currently-accepted `sig_key_id`. Always returns a
    /// non-zero value (constructor guarantees `accepted_key_ids` is
    /// non-empty + free of the sentinel).
    #[must_use]
    pub fn oldest_active(&self) -> u32 {
        // accepted_key_ids is non-empty + sorted ascending by ctor
        // contract; the first element is the smallest.
        self.accepted_key_ids
            .first()
            .copied()
            // Lote 10.4-tris P0-R5-001: unwrap_or(1) NOT 0 (avoid
            // leaking sentinel via error response). Constructor
            // guarantees non-empty so this branch is unreachable;
            // belt-and-suspenders.
            .unwrap_or(1)
    }

    /// Add a new `sig_key_id` to the accepted whitelist (rotation
    /// grace extension).
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when `new_key_id == 0`.
    pub fn admit(&mut self, new_key_id: u32) -> Result<(), SigError> {
        if new_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        if !self.accepted_key_ids.contains(&new_key_id) {
            self.accepted_key_ids.push(new_key_id);
            self.accepted_key_ids.sort_unstable();
        }
        Ok(())
    }

    /// Remove a `sig_key_id` from the accepted whitelist (rotation
    /// grace expiry). No-op if the id is not present. Returns an error
    /// if removing the id would leave the whitelist empty (a verifier
    /// that accepts no keys is a programmer error).
    ///
    /// # Errors
    ///
    /// - [`SigError::TdkDerivationFailed`] (with reason `accepted_key_ids
    ///   must be non-empty`) when removing the id would leave the
    ///   whitelist empty.
    pub fn retire(&mut self, retired_key_id: u32) -> Result<(), SigError> {
        if !self.accepted_key_ids.contains(&retired_key_id) {
            return Ok(());
        }
        if self.accepted_key_ids.len() == 1 {
            return Err(SigError::TdkDerivationFailed(
                "accepted_key_ids must be non-empty".to_string(),
            ));
        }
        self.accepted_key_ids.retain(|&k| k != retired_key_id);
        Ok(())
    }
}

impl SignatureVerifier for HkdfVerifier {
    fn verify(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
        signature: &[u8],
    ) -> Result<(), SigError> {
        // Length check — public, fast-fail before constant-time path.
        if signature.len() != AC_ENVELOPE_SIG_LEN {
            return Err(SigError::LengthMismatch {
                expected: AC_ENVELOPE_SIG_LEN,
                got: signature.len(),
            });
        }
        // Sentinel check — public, fast-fail.
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        // Whitelist check — public, fast-fail.
        if !self.accepted_key_ids.contains(&sig_key_id) {
            return Err(SigError::KeyIdUnknown {
                sig_key_id,
                oldest_active: self.oldest_active(),
            });
        }
        // Recompute the tag and compare in constant time.
        let tdk = self.tdk_handle.fetch(tenant_id, sig_key_id)?;
        let sig_key = derive_sig_key(tdk.as_bytes(), sig_key_id)?;
        let expected = keyed_mac(&sig_key, canonical_bytes);
        // subtle::ConstantTimeEq returns Choice; into() → bool.
        if expected.ct_eq(signature).into() {
            Ok(())
        } else {
            Err(SigError::Invalid)
        }
    }
}

/// Convenience: compute the canonical 32-byte sig over `canonical_bytes`
/// without holding a [`HkdfSigner`] (useful for canonical-vector tests +
/// the dual-side client SDK that only ever verifies). Mirrors the
/// production sign path 1:1.
///
/// # Errors
///
/// Surfaces any [`SigError`] from the HKDF Extract+Expand or BLAKE3 keyed-hash
/// path — the typical case is [`SigError::KeyIdReserved`] when
/// `sig_key_id == 0`.
pub fn compute_signature(
    tdk_bytes: &[u8; TDK_LEN],
    sig_key_id: u32,
    canonical_bytes: &[u8],
) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError> {
    if sig_key_id == RESERVED_SIG_KEY_ID {
        return Err(SigError::KeyIdReserved);
    }
    let sig_key = derive_sig_key(tdk_bytes, sig_key_id)?;
    Ok(keyed_mac(&sig_key, canonical_bytes))
}

/// Sibling-domain helper: compute an HKDF-SHA256 + BLAKE3 keyed-hash
/// MAC tag over `canonical_bytes` using a per-tenant TDK fetched via
/// the supplied [`TdkHandle`] but with a CALLER-SUPPLIED `info` byte
/// string. Used by sibling crates (e.g. `corelink-manifest`) that need
/// the same cripto stack with a domain-separated info string —
/// `b"manifest-sig"` (WI-S05-005) / `b"meta-manifest-sig"` (WI-S05-006
/// reserved) — without re-implementing the Extract+Expand+keyed-hash
/// pipeline AND without breaking the [`Tdk`] hygiene contract (the
/// raw bytes remain crate-private; the helper borrows them inside this
/// function and drops the [`Tdk`] before returning).
///
/// The function preserves every cripto invariant of the AC sig
/// pipeline:
///
/// - **Salt = `sig_key_id.to_le_bytes()`** — binds the rotation version
///   into Extract per ADR-0021 §1 / Lote 10.4bis P0 fix.
/// - **Length-bound output** = 32 bytes (BLAKE3 keyed-hash output;
///   matches [`AC_ENVELOPE_SIG_LEN`]).
/// - **Reserved sentinel rejection**: `sig_key_id == 0` rejected
///   eagerly per ADR-0021 §Sentinel.
///
/// Callers MUST pin their `info` byte string in a `pub const` and
/// gate it via a `tests::canonical_info_string` assertion (the same
/// CI-gate pattern this crate uses for `b"ac-sig"`). Drift detection
/// across the three info strings is the responsibility of the calling
/// crate's tests; this helper does not enumerate the strings.
///
/// # Errors
///
/// - [`SigError::KeyIdReserved`] when `sig_key_id == 0`.
/// - [`SigError::BackendError`] when the [`TdkHandle::fetch`] fails.
/// - [`SigError::TdkDerivationFailed`] when HKDF-Expand reports a
///   length error (unreachable in practice; surfaced as structural).
pub fn keyed_mac_with_info(
    tdk_handle: &dyn super::tdk::TdkHandle,
    tenant_id: Uuid,
    sig_key_id: u32,
    info: &[u8],
    canonical_bytes: &[u8],
) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError> {
    if sig_key_id == RESERVED_SIG_KEY_ID {
        return Err(SigError::KeyIdReserved);
    }
    let tdk = tdk_handle.fetch(tenant_id, sig_key_id)?;
    let salt = sig_key_id.to_le_bytes();
    let hk = Hkdf::<Sha256>::new(Some(&salt), tdk.as_bytes());
    let mut sig_key = Zeroizing::new([0u8; 32]);
    hk.expand(info, sig_key.as_mut_slice())
        .map_err(|e| SigError::TdkDerivationFailed(e.to_string()))?;
    Ok(keyed_mac(&sig_key, canonical_bytes))
}

/// Sibling-domain helper: compute an HKDF-SHA256 + BLAKE3 keyed-hash
/// MAC tag from raw `tdk_bytes` (32 bytes) with a CALLER-SUPPLIED
/// `info`. Mirrors [`keyed_mac_with_info`] but without the
/// [`TdkHandle`] indirection — used by canonical-vector tests in
/// sibling crates that need to pin a deterministic sig under a
/// fixed mock TDK.
///
/// # Errors
///
/// - [`SigError::KeyIdReserved`] when `sig_key_id == 0`.
/// - [`SigError::TdkDerivationFailed`] when HKDF-Expand reports a
///   length error.
pub fn keyed_mac_with_info_from_bytes(
    tdk_bytes: &[u8; TDK_LEN],
    sig_key_id: u32,
    info: &[u8],
    canonical_bytes: &[u8],
) -> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError> {
    if sig_key_id == RESERVED_SIG_KEY_ID {
        return Err(SigError::KeyIdReserved);
    }
    let salt = sig_key_id.to_le_bytes();
    let hk = Hkdf::<Sha256>::new(Some(&salt), tdk_bytes);
    let mut sig_key = Zeroizing::new([0u8; 32]);
    hk.expand(info, sig_key.as_mut_slice())
        .map_err(|e| SigError::TdkDerivationFailed(e.to_string()))?;
    Ok(keyed_mac(&sig_key, canonical_bytes))
}

/// Assert that the canonical preimage length is what callers expect —
/// a shape check used by the integration adapter in `corelink-worker`.
const _: () = assert!(AC_ENVELOPE_SIG_LEN == 32);
const _: () = assert!(AC_ENVELOPE_PREIMAGE_LEN == 121);

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
    use crate::sig::canonical::compose;
    use crate::sig::tdk::MockTdkHandle;

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    fn other_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
    }

    fn sample_canonical_bytes(sig_key_id: u32) -> [u8; AC_ENVELOPE_PREIMAGE_LEN] {
        compose(
            1,
            sig_key_id,
            fixed_tenant(),
            &[0xAB; 32],
            1234,
            &[0xCD; 32],
        )
        .unwrap()
    }

    fn fresh_signer_verifier(
        accepted_key_ids: Vec<u32>,
    ) -> (HkdfSigner, HkdfVerifier, Arc<MockTdkHandle>) {
        let mock = Arc::new(MockTdkHandle::new());
        for kid in &accepted_key_ids {
            mock.install_default(fixed_tenant(), *kid);
            mock.install_default(other_tenant(), *kid);
        }
        let signer_kid = *accepted_key_ids.last().unwrap();
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let signer = HkdfSigner::new(Arc::clone(&handle), signer_kid).unwrap();
        let verifier = HkdfVerifier::new(handle, accepted_key_ids).unwrap();
        (signer, verifier, mock)
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![1]);
        let bytes = sample_canonical_bytes(1);
        let sig = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        verifier.verify(fixed_tenant(), 1, &bytes, &sig).unwrap();
    }

    #[test]
    fn sign_with_reserved_key_id_rejected() {
        let (signer, _, _) = fresh_signer_verifier(vec![1]);
        let err = signer.sign(fixed_tenant(), 0, &[0u8; 121]).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn signer_rejects_zero_construction() {
        let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
        let err = HkdfSigner::new(mock, 0).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn verifier_rejects_empty_accepted() {
        let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
        let err = HkdfVerifier::new(mock, vec![]).unwrap_err();
        assert!(matches!(err, SigError::TdkDerivationFailed(_)));
    }

    #[test]
    fn verifier_rejects_zero_in_accepted() {
        let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
        let err = HkdfVerifier::new(mock, vec![0, 1, 2]).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn verify_with_reserved_key_id_rejected() {
        let (_, verifier, _) = fresh_signer_verifier(vec![1]);
        let err = verifier
            .verify(fixed_tenant(), 0, &[0u8; 121], &[0u8; AC_ENVELOPE_SIG_LEN])
            .unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn verify_with_unknown_key_id_rejected() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![2, 3]);
        let bytes = sample_canonical_bytes(2);
        let sig = signer.sign(fixed_tenant(), 2, &bytes).unwrap();
        let err = verifier.verify(fixed_tenant(), 1, &bytes, &sig).unwrap_err();
        match err {
            SigError::KeyIdUnknown {
                sig_key_id: 1,
                oldest_active: 2,
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn verify_with_wrong_length_signature_rejected() {
        let (_, verifier, _) = fresh_signer_verifier(vec![1]);
        let err = verifier
            .verify(fixed_tenant(), 1, &[0u8; 121], &[0u8; 16])
            .unwrap_err();
        match err {
            SigError::LengthMismatch { expected: 32, got: 16 } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn verify_rejects_byte_flip() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![1]);
        let bytes = sample_canonical_bytes(1);
        let mut sig = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        sig[5] ^= 0x01;
        let err = verifier.verify(fixed_tenant(), 1, &bytes, &sig).unwrap_err();
        assert_eq!(err, SigError::Invalid);
    }

    #[test]
    fn cross_tenant_signature_rejected() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![1]);
        let bytes = sample_canonical_bytes(1);
        let sig = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        let err = verifier.verify(other_tenant(), 1, &bytes, &sig).unwrap_err();
        assert_eq!(err, SigError::Invalid);
    }

    #[test]
    fn rotation_grace_accepts_old_and_new_envelopes() {
        // Verifier whitelist = [1, 2]; signer rotates from 1 → 2; both
        // pre-rotation and post-rotation envelopes verify.
        let (mut signer, verifier, _) = fresh_signer_verifier(vec![1, 2]);
        // Signer is at key 2 by default (last); pre-rotation: bump
        // signer down to key 1 to emit an "old" envelope.
        signer.rotate_to(1).unwrap();
        let pre_bytes = sample_canonical_bytes(1);
        let pre_sig = signer.sign(fixed_tenant(), 1, &pre_bytes).unwrap();
        verifier
            .verify(fixed_tenant(), 1, &pre_bytes, &pre_sig)
            .unwrap();

        // Rotation event: bump signer to key 2.
        signer.rotate_to(2).unwrap();
        let post_bytes = sample_canonical_bytes(2);
        let post_sig = signer.sign(fixed_tenant(), 2, &post_bytes).unwrap();
        verifier
            .verify(fixed_tenant(), 2, &post_bytes, &post_sig)
            .unwrap();
    }

    #[test]
    fn admit_then_retire_extends_and_expires_grace() {
        let (_signer, mut verifier, mock) = fresh_signer_verifier(vec![1, 2]);
        // Install a TDK for the new key + admit it via verifier API.
        mock.install_default(fixed_tenant(), 3);
        verifier.admit(3).unwrap();
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let new_signer = HkdfSigner::new(Arc::clone(&handle), 3).unwrap();
        let bytes = sample_canonical_bytes(3);
        let sig = new_signer.sign(fixed_tenant(), 3, &bytes).unwrap();
        verifier.verify(fixed_tenant(), 3, &bytes, &sig).unwrap();

        // Retire key 1; envelope signed with key 1 now rejected.
        verifier.retire(1).unwrap();
        let pre_signer = HkdfSigner::new(handle, 1).unwrap();
        let pre_bytes = sample_canonical_bytes(1);
        let pre_sig = pre_signer.sign(fixed_tenant(), 1, &pre_bytes).unwrap();
        let err = verifier
            .verify(fixed_tenant(), 1, &pre_bytes, &pre_sig)
            .unwrap_err();
        match err {
            SigError::KeyIdUnknown {
                sig_key_id: 1, ..
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn retire_last_key_rejected() {
        let (_, mut verifier, _) = fresh_signer_verifier(vec![1]);
        let err = verifier.retire(1).unwrap_err();
        assert!(matches!(err, SigError::TdkDerivationFailed(_)));
    }

    #[test]
    fn signer_rotate_zero_rejected() {
        let (mut signer, _, _) = fresh_signer_verifier(vec![1]);
        let err = signer.rotate_to(0).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn admit_zero_rejected() {
        let (_, mut verifier, _) = fresh_signer_verifier(vec![1]);
        let err = verifier.admit(0).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn compute_signature_matches_signer() {
        let (signer, _, mock) = fresh_signer_verifier(vec![1]);
        let bytes = sample_canonical_bytes(1);
        let sig_via_signer = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        let tdk_bytes = mock.fetch(fixed_tenant(), 1).unwrap();
        let mut tdk_arr = [0u8; TDK_LEN];
        tdk_arr.copy_from_slice(tdk_bytes.as_bytes());
        let sig_via_helper = compute_signature(&tdk_arr, 1, &bytes).unwrap();
        assert_eq!(sig_via_signer, sig_via_helper);
    }

    #[test]
    fn distinct_sig_key_ids_yield_distinct_sigs() {
        let (signer, _, mock) = fresh_signer_verifier(vec![1, 2]);
        let _ = mock; // installed both
        let mut s1 = signer.clone();
        s1.rotate_to(1).unwrap();
        let mut s2 = signer.clone();
        s2.rotate_to(2).unwrap();
        let bytes = sample_canonical_bytes(1);
        let sig1 = s1.sign(fixed_tenant(), 1, &bytes).unwrap();
        let sig2 = s2.sign(fixed_tenant(), 2, &bytes).unwrap();
        assert_ne!(sig1, sig2);
    }

}
