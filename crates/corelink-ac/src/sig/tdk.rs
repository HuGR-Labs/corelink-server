//! Tenant Derivation Key (TDK) handle abstraction (WI-S04-004 §6.1.8).
//!
//! [`TdkHandle`] is the canonical seam between the HKDF signer / verifier
//! and the underlying secret store (Cloudflare Secrets / KMS in
//! production). Following the charter trait-abstraction-defer pattern:
//!
//! - This crate ships [`TdkHandle`] (the canonical surface) +
//!   [`MockTdkHandle`] (a deterministic test fixture that maps
//!   `(tenant_id, sig_key_id) -> Tdk` via an in-memory `BTreeMap`).
//! - The real `CfSecretsTdkHandle` shim that wires the Worker
//!   `binding.get(...)` round-trip ships in WI-S04-006 alongside the
//!   conformance suite + the in-cache miniflare smoke. The trait surface
//!   is wasm32-clean (no tokio dep; sync `fetch` returns
//!   `Result<Tdk, SigError>` so the HKDF sign / verify hot path can
//!   call it inline).
//!
//! ## TDK hygiene
//!
//! Every TDK round-trip wraps the bytes in [`Tdk`], a newtype around
//! [`zeroize::Zeroizing<Vec<u8>>`]:
//!
//! - **Zero-on-drop**: `Zeroizing` zeroes the heap allocation when the
//!   `Tdk` is dropped, defending against post-mortem memory dump
//!   attacks. CF Workers may reuse memory pages across requests; this
//!   prevents cross-request leak.
//! - **Redacted Debug**: `fmt::Debug` writes `Tdk(REDACTED)` so the
//!   bytes never reach a tracing log line.
//! - **No Display, no PartialEq, no AsRef<[u8]>** — the only way to
//!   read the bytes is via the `pub(crate)` `as_bytes` accessor used
//!   inside this crate's signer / verifier. Callers outside the crate
//!   cannot extract the bytes.
//! - **Length-checked construction**: [`Tdk::new`] requires exactly
//!   [`super::TDK_LEN`] bytes (32) — the HKDF Extract step accepts any
//!   IKM length but our deployed TDK is a fixed 32-byte secret per
//!   `key_management.md` §3.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use uuid::Uuid;
use zeroize::Zeroizing;

use super::error::SigError;

/// Canonical TDK byte length (`32`). Matches the symmetric MAC key
/// width per `key_management.md` §3.
pub const TDK_LEN: usize = 32;

/// Per-tenant TDK secret with zero-on-drop hygiene.
///
/// Held by [`TdkHandle::fetch`] callers for the duration of the HKDF
/// Extract+Expand step, then dropped before the signer surface
/// returns. Construction is length-checked.
pub struct Tdk(Zeroizing<Vec<u8>>);

impl Tdk {
    /// Construct a fresh TDK from raw bytes. Returns `None` if the
    /// input length is not [`TDK_LEN`].
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        if bytes.len() != TDK_LEN {
            return None;
        }
        Some(Self(Zeroizing::new(bytes)))
    }

    /// Construct a fresh TDK from a 32-byte fixed array.
    #[must_use]
    pub fn from_array(bytes: [u8; TDK_LEN]) -> Self {
        Self(Zeroizing::new(bytes.to_vec()))
    }

    /// Borrow the underlying TDK bytes. Crate-internal access only —
    /// the HKDF signer / verifier reads this; no caller outside the
    /// crate can.
    #[must_use]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Tdk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never expose TDK bytes via Debug — leak-by-print prevention.
        f.write_str("Tdk(REDACTED)")
    }
}

/// Canonical TDK handle abstraction.
///
/// Production impl wraps Cloudflare Secrets binding + an in-memory 5min
/// TTL cache (deferred to WI-S04-006 alongside the real D1 binding shim
/// per charter trait-abstraction-defer). Test impl is
/// [`MockTdkHandle`].
pub trait TdkHandle: Send + Sync + fmt::Debug {
    /// Fetch the TDK for the `(tenant_id, sig_key_id)` pair. Production
    /// callers SHOULD cache the result for ≤ 5 min via an internal
    /// in-memory cache; this trait surface is sync so the hot path
    /// stays cheap.
    ///
    /// # Errors
    ///
    /// - [`SigError::BackendError`] when the backing secret store
    ///   round-trip fails or the tenant has no TDK provisioned for
    ///   the requested `sig_key_id`.
    fn fetch(&self, tenant_id: Uuid, sig_key_id: u32) -> Result<Tdk, SigError>;
}

/// In-memory test fixture for [`TdkHandle`].
///
/// Deterministic, side-effect-free. Pre-populated via
/// [`MockTdkHandle::install`] / [`MockTdkHandle::install_default`]; the
/// canonical fixture for the `tests/canonical_vectors_sig.rs` corpus
/// uses [`MockTdkHandle::install_default`] so every test that does not
/// care about per-tenant TDK isolation gets a working backend.
#[derive(Default)]
pub struct MockTdkHandle {
    map: Mutex<BTreeMap<(Uuid, u32), [u8; TDK_LEN]>>,
}

impl fmt::Debug for MockTdkHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = match self.map.lock() {
            Ok(g) => g.len(),
            // Poisoned lock: surface zero — Debug output is for SRE
            // diagnostics, never load-bearing.
            Err(_) => 0,
        };
        f.debug_struct("MockTdkHandle")
            .field("entries", &len)
            .finish()
    }
}

impl MockTdkHandle {
    /// Construct an empty mock. Use [`Self::install`] to add entries.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a TDK for `(tenant_id, sig_key_id)`. Overwrites any
    /// existing entry.
    pub fn install(&self, tenant_id: Uuid, sig_key_id: u32, tdk: [u8; TDK_LEN]) {
        if let Ok(mut g) = self.map.lock() {
            g.insert((tenant_id, sig_key_id), tdk);
        }
    }

    /// Install a deterministic TDK for `(tenant_id, sig_key_id)` whose
    /// bytes are derived from `(tenant_id, sig_key_id)`. Convenience
    /// helper for property tests + canonical vectors.
    ///
    /// Derivation: `BLAKE3("corelink-ac-tdk-mock-v1" || tenant_id ||
    /// sig_key_id_be)` — gives every `(tenant_id, sig_key_id)` pair a
    /// distinct, reproducible TDK.
    pub fn install_default(&self, tenant_id: Uuid, sig_key_id: u32) {
        let tdk = derive_default_mock_tdk(tenant_id, sig_key_id);
        self.install(tenant_id, sig_key_id, tdk);
    }
}

impl TdkHandle for MockTdkHandle {
    fn fetch(&self, tenant_id: Uuid, sig_key_id: u32) -> Result<Tdk, SigError> {
        let g = self
            .map
            .lock()
            .map_err(|_| SigError::BackendError("MockTdkHandle: poisoned lock".to_string()))?;
        match g.get(&(tenant_id, sig_key_id)) {
            Some(bytes) => Ok(Tdk::from_array(*bytes)),
            None => Err(SigError::BackendError(format!(
                "MockTdkHandle: no TDK for (tenant_id={tenant_id}, sig_key_id={sig_key_id})"
            ))),
        }
    }
}

/// Deterministic mock TDK derivation used by
/// [`MockTdkHandle::install_default`]. Public-but-`#[doc(hidden)]` so
/// tests outside this crate can pre-populate fixtures without knowing
/// the cripto recipe.
#[doc(hidden)]
#[must_use]
pub fn derive_default_mock_tdk(tenant_id: Uuid, sig_key_id: u32) -> [u8; TDK_LEN] {
    let mut h = blake3::Hasher::new();
    h.update(b"corelink-ac-tdk-mock-v1");
    h.update(tenant_id.as_bytes());
    h.update(&sig_key_id.to_be_bytes());
    *h.finalize().as_bytes()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    #[test]
    fn tdk_new_rejects_wrong_length() {
        assert!(Tdk::new(vec![0u8; 31]).is_none());
        assert!(Tdk::new(vec![0u8; 33]).is_none());
        assert!(Tdk::new(vec![0u8; TDK_LEN]).is_some());
    }

    #[test]
    fn tdk_debug_redacts_bytes() {
        let tdk = Tdk::from_array([0xFF; TDK_LEN]);
        let s = format!("{tdk:?}");
        assert_eq!(s, "Tdk(REDACTED)");
        assert!(!s.contains("ff"));
    }

    #[test]
    fn mock_tdk_returns_installed_value() {
        let h = MockTdkHandle::new();
        h.install(fixed_tenant(), 1, [0xAB; TDK_LEN]);
        let tdk = h.fetch(fixed_tenant(), 1).unwrap();
        assert_eq!(tdk.as_bytes(), &[0xAB; TDK_LEN]);
    }

    #[test]
    fn mock_tdk_fetch_returns_backend_error_when_unknown() {
        let h = MockTdkHandle::new();
        let err = h.fetch(fixed_tenant(), 99).unwrap_err();
        assert!(matches!(err, SigError::BackendError(_)));
        assert_eq!(err.audit_code(), "backend_error");
    }

    #[test]
    fn install_default_distinct_per_pair() {
        let h = MockTdkHandle::new();
        h.install_default(fixed_tenant(), 1);
        h.install_default(fixed_tenant(), 2);
        let other = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
        h.install_default(other, 1);

        let t11 = h.fetch(fixed_tenant(), 1).unwrap();
        let t12 = h.fetch(fixed_tenant(), 2).unwrap();
        let t21 = h.fetch(other, 1).unwrap();
        // All three should differ.
        assert_ne!(t11.as_bytes(), t12.as_bytes());
        assert_ne!(t11.as_bytes(), t21.as_bytes());
        assert_ne!(t12.as_bytes(), t21.as_bytes());
    }

    #[test]
    fn derive_default_mock_tdk_is_deterministic() {
        let a = derive_default_mock_tdk(fixed_tenant(), 1);
        let b = derive_default_mock_tdk(fixed_tenant(), 1);
        assert_eq!(a, b);
    }
}
