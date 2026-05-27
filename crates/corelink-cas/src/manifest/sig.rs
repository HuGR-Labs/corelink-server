//! Manifest sig — HKDF-SHA256 + BLAKE3 keyed-hash with
//! `info = b"manifest-sig"` domain separation (WI-S05-005 §6.1.X).
//!
//! Mirrors the cripto stack landed in `corelink-ac::sig` (WI-S04-004)
//! 1:1 — same primitives, same constant-time discipline, same `TdkHandle`
//! per-tenant secret indirection — but with an INDEPENDENT HKDF info
//! string `b"manifest-sig"`. Domain separation:
//!
//! - **`b"ac-sig"`** (WI-S04-004) — AC envelope signing, never touches
//!   manifest payloads.
//! - **`b"manifest-sig"`** (this module) — multipart manifest envelope.
//! - **`b"meta-manifest-sig"`** (forward; reserved for the stitched
//!   parent-manifest flow in WI-S05-006).
//!
//! The three info strings are byte-distinct AND non-prefix
//! (HKDF-Expand uses `info` as a label so a shared prefix could enable
//! truncation attacks if the implementation ever changed the
//! length-binding semantic). The CI test
//! `tests::canonical_info_string` asserts byte-equal AND distinctness.
//!
//! ## Reuse over re-implement
//!
//! The HKDF Extract+Expand + BLAKE3 keyed-hash + constant-time compare
//! pipeline is shared with `corelink-ac::sig::hkdf_signer`. To avoid
//! duplicating that pipeline we depend on the
//! `corelink_ac::sig::compute_signature` helper which derives a
//! sig_key from `(tdk_bytes, sig_key_id)` under `info = b"ac-sig"` —
//! BUT we DON'T call it directly; we re-derive locally with
//! `info = b"manifest-sig"`. The helpers `derive_sig_key` + `keyed_mac`
//! below are byte-equal to the AC versions modulo the info string.

use std::sync::Arc;

use corelink_ac::sig::{
    keyed_mac_with_info, keyed_mac_with_info_from_bytes, SigError, TdkHandle, RESERVED_SIG_KEY_ID,
};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::manifest::bounds::MANIFEST_SIG_LEN;

/// HKDF info bytes — `b"manifest-sig"` per WI-S05-005 §9.3. Constant;
/// CI gate in `tests::canonical_info_string` asserts byte-equal AND
/// distinctness from `b"ac-sig"` + `b"meta-manifest-sig"`.
pub const HKDF_INFO_MANIFEST_SIG: &[u8] = b"manifest-sig";

/// Reserved info-string prefix for the stitched-manifest flow. Reserved
/// here so an integration test in WI-S05-006 can prove the three
/// (canonical, manifest, meta-manifest) info strings are byte-distinct
/// AND no one is a prefix of another.
pub const HKDF_INFO_META_MANIFEST_SIG_RESERVED: &[u8] = b"meta-manifest-sig";

/// Compute the manifest-domain MAC tag via the canonical
/// [`corelink_ac::sig::keyed_mac_with_info`] helper. Pinned info string
/// = [`HKDF_INFO_MANIFEST_SIG`] (`b"manifest-sig"`).
fn manifest_mac(
    tdk_handle: &dyn TdkHandle,
    tenant_id: Uuid,
    sig_key_id: u32,
    canonical_bytes: &[u8],
) -> Result<[u8; MANIFEST_SIG_LEN], SigError> {
    keyed_mac_with_info(
        tdk_handle,
        tenant_id,
        sig_key_id,
        HKDF_INFO_MANIFEST_SIG,
        canonical_bytes,
    )
}

/// Manifest signer — production impl. Holds an `Arc<dyn TdkHandle>` for
/// per-tenant secret resolution; cheap to clone (only the Arc is
/// bumped). Mirrors the [`corelink_ac::sig::HkdfSigner`] shape with the
/// canonical `info = b"manifest-sig"` substitution.
#[derive(Clone)]
pub struct ManifestSigner {
    tdk_handle: Arc<dyn TdkHandle>,
    current_key_id: u32,
}

impl std::fmt::Debug for ManifestSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManifestSigner")
            .field("current_key_id", &self.current_key_id)
            .field("tdk_handle", &self.tdk_handle)
            .finish()
    }
}

impl ManifestSigner {
    /// Construct a fresh signer.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when `current_key_id == 0`
    ///   (the never-issued sentinel per ADR-0021 §Sentinel).
    pub fn new(tdk_handle: Arc<dyn TdkHandle>, current_key_id: u32) -> Result<Self, SigError> {
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

    /// Bump `current_key_id` to a new rotation value.
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

    /// Sign `canonical_bytes` for `(tenant_id, sig_key_id)` and return
    /// the canonical 32-byte tag.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when `sig_key_id == 0`.
    /// - [`SigError::BackendError`] when [`TdkHandle::fetch`] fails.
    /// - [`SigError::TdkDerivationFailed`] on an HKDF-Expand length
    ///   error (unreachable in practice; surfaced as structural).
    pub fn sign(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
    ) -> Result<[u8; MANIFEST_SIG_LEN], SigError> {
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        manifest_mac(
            self.tdk_handle.as_ref(),
            tenant_id,
            sig_key_id,
            canonical_bytes,
        )
    }
}

/// Manifest verifier — production impl. Holds an `Arc<dyn TdkHandle>` plus
/// a sorted `accepted_key_ids` whitelist. Mirrors
/// [`corelink_ac::sig::HkdfVerifier`].
#[derive(Clone)]
pub struct ManifestVerifierSig {
    tdk_handle: Arc<dyn TdkHandle>,
    accepted_key_ids: Vec<u32>,
}

impl std::fmt::Debug for ManifestVerifierSig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManifestVerifierSig")
            .field("accepted_key_ids", &self.accepted_key_ids)
            .field("tdk_handle", &self.tdk_handle)
            .finish()
    }
}

impl ManifestVerifierSig {
    /// Construct a fresh verifier.
    ///
    /// # Errors
    ///
    /// - [`SigError::KeyIdReserved`] when any element of
    ///   `accepted_key_ids` is `0`.
    /// - [`SigError::TdkDerivationFailed`] (with reason `accepted_key_ids
    ///   must be non-empty`) when `accepted_key_ids` is empty.
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

    /// Smallest currently-accepted `sig_key_id`.
    #[must_use]
    pub fn oldest_active(&self) -> u32 {
        self.accepted_key_ids.first().copied().unwrap_or(1)
    }

    /// Add a new `sig_key_id` to the accepted whitelist.
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

    /// Remove a `sig_key_id` from the accepted whitelist.
    ///
    /// # Errors
    ///
    /// - [`SigError::TdkDerivationFailed`] when retiring would empty
    ///   the whitelist.
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

    /// Verify `signature` over `canonical_bytes` for
    /// `(tenant_id, sig_key_id)`. Constant-time on the success +
    /// cripto-mismatch path.
    ///
    /// # Errors
    ///
    /// - [`SigError::LengthMismatch`] when `signature.len() !=
    ///   MANIFEST_SIG_LEN`.
    /// - [`SigError::KeyIdReserved`] / [`SigError::KeyIdUnknown`] for
    ///   sentinel / out-of-whitelist key ids.
    /// - [`SigError::Invalid`] on constant-time cripto compare failure.
    /// - [`SigError::BackendError`] / [`SigError::TdkDerivationFailed`]
    ///   on TDK / HKDF backend errors.
    pub fn verify(
        &self,
        tenant_id: Uuid,
        sig_key_id: u32,
        canonical_bytes: &[u8],
        signature: &[u8],
    ) -> Result<(), SigError> {
        if signature.len() != MANIFEST_SIG_LEN {
            return Err(SigError::LengthMismatch {
                expected: MANIFEST_SIG_LEN,
                got: signature.len(),
            });
        }
        if sig_key_id == RESERVED_SIG_KEY_ID {
            return Err(SigError::KeyIdReserved);
        }
        if !self.accepted_key_ids.contains(&sig_key_id) {
            return Err(SigError::KeyIdUnknown {
                sig_key_id,
                oldest_active: self.oldest_active(),
            });
        }
        let expected = manifest_mac(
            self.tdk_handle.as_ref(),
            tenant_id,
            sig_key_id,
            canonical_bytes,
        )?;
        if expected.ct_eq(signature).into() {
            Ok(())
        } else {
            Err(SigError::Invalid)
        }
    }
}

/// Convenience: compute the canonical 32-byte sig over `canonical_bytes`
/// without holding a [`ManifestSigner`] (mirrors
/// `corelink_ac::sig::compute_signature`).
///
/// # Errors
///
/// Surfaces any [`SigError`] from the HKDF Extract+Expand or BLAKE3
/// keyed-hash path.
pub fn compute_signature(
    tdk_bytes: &[u8; 32],
    sig_key_id: u32,
    canonical_bytes: &[u8],
) -> Result<[u8; MANIFEST_SIG_LEN], SigError> {
    keyed_mac_with_info_from_bytes(
        tdk_bytes,
        sig_key_id,
        HKDF_INFO_MANIFEST_SIG,
        canonical_bytes,
    )
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
    use corelink_ac::sig::MockTdkHandle;

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000c01").unwrap()
    }

    fn other_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000d02").unwrap()
    }

    fn fresh_signer_verifier(
        accepted: Vec<u32>,
    ) -> (ManifestSigner, ManifestVerifierSig, Arc<MockTdkHandle>) {
        let mock = Arc::new(MockTdkHandle::new());
        for kid in &accepted {
            mock.install_default(fixed_tenant(), *kid);
            mock.install_default(other_tenant(), *kid);
        }
        let signer_kid = *accepted.last().unwrap();
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let signer = ManifestSigner::new(Arc::clone(&handle), signer_kid).unwrap();
        let verifier = ManifestVerifierSig::new(handle, accepted).unwrap();
        (signer, verifier, mock)
    }

    #[test]
    fn canonical_info_string() {
        // CI gate: HKDF info bytes drift = global signature mismatch.
        assert_eq!(HKDF_INFO_MANIFEST_SIG, b"manifest-sig");
        assert_eq!(HKDF_INFO_MANIFEST_SIG.len(), 12);
        // Distinct from corelink-ac sig info (b"ac-sig").
        assert_ne!(
            HKDF_INFO_MANIFEST_SIG,
            corelink_ac::sig::HKDF_INFO_AC_SIG
        );
        // No prefix relationship in either direction (truncation-attack
        // defense — see module rustdoc).
        assert!(!HKDF_INFO_MANIFEST_SIG.starts_with(corelink_ac::sig::HKDF_INFO_AC_SIG));
        assert!(!corelink_ac::sig::HKDF_INFO_AC_SIG.starts_with(HKDF_INFO_MANIFEST_SIG));
        // Reserved meta-manifest info string distinct + non-prefix.
        assert_eq!(HKDF_INFO_META_MANIFEST_SIG_RESERVED, b"meta-manifest-sig");
        assert!(!HKDF_INFO_META_MANIFEST_SIG_RESERVED.starts_with(HKDF_INFO_MANIFEST_SIG));
        assert!(!HKDF_INFO_MANIFEST_SIG.starts_with(HKDF_INFO_META_MANIFEST_SIG_RESERVED));
    }

    #[test]
    fn sign_verify_roundtrips() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![1]);
        let bytes = [0xAB; 102];
        let sig = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        verifier
            .verify(fixed_tenant(), 1, &bytes, &sig)
            .unwrap();
    }

    #[test]
    fn manifest_sig_distinct_from_ac_sig() {
        // Same TDK bytes + same canonical bytes + same sig_key_id produce
        // DIFFERENT sigs across the two domains (this is the whole point
        // of HKDF info-domain separation).
        let mock = Arc::new(MockTdkHandle::new());
        mock.install_default(fixed_tenant(), 1);
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let manifest_signer = ManifestSigner::new(Arc::clone(&handle), 1).unwrap();
        let bytes = [0xCD; 102];
        let manifest_sig = manifest_signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        // AC-domain sig with the same inputs.
        let ac_signer = corelink_ac::sig::HkdfSigner::new(handle, 1).unwrap();
        let ac_sig = corelink_ac::sig::SignatureSigner::sign(
            &ac_signer,
            fixed_tenant(),
            1,
            &bytes,
        )
        .unwrap();
        assert_ne!(manifest_sig, ac_sig, "domain separation MUST diverge sigs");
    }

    #[test]
    fn cross_tenant_signature_rejected() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![1]);
        let bytes = [0x11; 102];
        let sig = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        let err = verifier
            .verify(other_tenant(), 1, &bytes, &sig)
            .unwrap_err();
        assert_eq!(err, SigError::Invalid);
    }

    #[test]
    fn reserved_key_id_rejected_sign() {
        let (signer, _, _) = fresh_signer_verifier(vec![1]);
        let err = signer.sign(fixed_tenant(), 0, &[0u8; 102]).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn reserved_key_id_rejected_verify() {
        let (_, verifier, _) = fresh_signer_verifier(vec![1]);
        let err = verifier
            .verify(fixed_tenant(), 0, &[0u8; 102], &[0u8; MANIFEST_SIG_LEN])
            .unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn unknown_key_id_rejected_verify() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![2, 3]);
        let bytes = [0x22; 102];
        let sig = signer.sign(fixed_tenant(), 2, &bytes).unwrap();
        let err = verifier
            .verify(fixed_tenant(), 1, &bytes, &sig)
            .unwrap_err();
        match err {
            SigError::KeyIdUnknown {
                sig_key_id: 1,
                oldest_active: 2,
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn length_mismatch_rejected() {
        let (_, verifier, _) = fresh_signer_verifier(vec![1]);
        let err = verifier
            .verify(fixed_tenant(), 1, &[0u8; 102], &[0u8; 16])
            .unwrap_err();
        match err {
            SigError::LengthMismatch {
                expected: 32,
                got: 16,
            } => {}
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn byte_flip_rejected() {
        let (signer, verifier, _) = fresh_signer_verifier(vec![1]);
        let bytes = [0x33; 102];
        let mut sig = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        sig[5] ^= 0x01;
        let err = verifier
            .verify(fixed_tenant(), 1, &bytes, &sig)
            .unwrap_err();
        assert_eq!(err, SigError::Invalid);
    }

    #[test]
    fn rotation_grace_accepts_old_and_new() {
        let (mut signer, verifier, _) = fresh_signer_verifier(vec![1, 2]);
        // Pre-rotation: sign with key 1 (default signer is at key 2 since
        // `last()` of accepted; rotate_to(1)).
        signer.rotate_to(1).unwrap();
        let pre_bytes = [0x44; 102];
        let pre_sig = signer.sign(fixed_tenant(), 1, &pre_bytes).unwrap();
        verifier
            .verify(fixed_tenant(), 1, &pre_bytes, &pre_sig)
            .unwrap();

        // Rotation event: bump signer to key 2.
        signer.rotate_to(2).unwrap();
        let post_bytes = [0x55; 102];
        let post_sig = signer.sign(fixed_tenant(), 2, &post_bytes).unwrap();
        verifier
            .verify(fixed_tenant(), 2, &post_bytes, &post_sig)
            .unwrap();
    }

    #[test]
    fn admit_then_retire_extends_and_expires_grace() {
        let (_signer, mut verifier, mock) = fresh_signer_verifier(vec![1, 2]);
        mock.install_default(fixed_tenant(), 3);
        verifier.admit(3).unwrap();
        let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
        let new_signer = ManifestSigner::new(Arc::clone(&handle), 3).unwrap();
        let bytes = [0x66; 102];
        let sig = new_signer.sign(fixed_tenant(), 3, &bytes).unwrap();
        verifier.verify(fixed_tenant(), 3, &bytes, &sig).unwrap();

        verifier.retire(1).unwrap();
        let pre_signer = ManifestSigner::new(handle, 1).unwrap();
        let pre_bytes = [0x77; 102];
        let pre_sig = pre_signer.sign(fixed_tenant(), 1, &pre_bytes).unwrap();
        let err = verifier
            .verify(fixed_tenant(), 1, &pre_bytes, &pre_sig)
            .unwrap_err();
        assert!(matches!(err, SigError::KeyIdUnknown { sig_key_id: 1, .. }));
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
    fn empty_accepted_rejected() {
        let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
        let err = ManifestVerifierSig::new(mock, vec![]).unwrap_err();
        assert!(matches!(err, SigError::TdkDerivationFailed(_)));
    }

    #[test]
    fn zero_in_accepted_rejected() {
        let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
        let err = ManifestVerifierSig::new(mock, vec![0, 1]).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }

    #[test]
    fn compute_signature_matches_signer() {
        // The MockTdkHandle::install_default helper derives every TDK
        // via the public `derive_default_mock_tdk` recipe — so we
        // recompute the same TDK bytes locally without needing access
        // to the crate-private `Tdk::as_bytes` accessor.
        let (signer, _, _) = fresh_signer_verifier(vec![1]);
        let bytes = [0x88; 102];
        let sig_via_signer = signer.sign(fixed_tenant(), 1, &bytes).unwrap();
        let tdk_arr = corelink_ac::sig::derive_default_mock_tdk(fixed_tenant(), 1);
        let sig_via_helper = compute_signature(&tdk_arr, 1, &bytes).unwrap();
        assert_eq!(sig_via_signer, sig_via_helper);
    }

    #[test]
    fn signer_rejects_zero_construction() {
        let mock = Arc::new(MockTdkHandle::new()) as Arc<dyn TdkHandle>;
        let err = ManifestSigner::new(mock, 0).unwrap_err();
        assert_eq!(err, SigError::KeyIdReserved);
    }
}
