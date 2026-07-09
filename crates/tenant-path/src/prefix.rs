//! Core derivation primitives. Kept private to the crate; re-exported from
//! `lib.rs` to surface a deliberately small public API.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Length of a [`TenantPrefix`] in ASCII characters / bytes.
///
/// Canonical per `remote_cache_product_profile.md §7.1`:
/// `HMAC16 = b64url_no_pad(HMAC_SHA256(TDK, tenant_id_bytes))[..16]`.
pub const TENANT_PREFIX_LEN: usize = 16;

/// Length of a [`TenantDerivationKey`] in raw bytes (HMAC-SHA256 key size).
const TDK_LEN: usize = 32;

/// Length in chars of `URL_SAFE_NO_PAD(HMAC-SHA256(...))` output.
/// `ceil(32 * 8 / 6) = 43`.
const HMAC_B64_LEN: usize = 43;

type HmacSha256 = Hmac<Sha256>;

/// Per-region tenant derivation key (HMAC-SHA256 secret).
///
/// Stored bytes are [`Zeroize`]d on drop and never exposed via [`Debug`].
/// `PartialEq` / `Eq` are deliberately **not** implemented to discourage
/// logging and accidental leaks; if constant-time comparison ever becomes
/// necessary it should be added via a dedicated `pub(crate)` helper at that
/// time, not preemptively.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct TenantDerivationKey([u8; TDK_LEN]);

impl TenantDerivationKey {
    /// Construct a TDK from a 32-byte secret loaded from a Cloudflare Secrets
    /// binding (or equivalent KMS-backed source).
    ///
    /// Takes [`Zeroizing<[u8; 32]>`] (rather than a bare `[u8; 32]`) so that
    /// the caller's source buffer is **securely zeroed when the wrapper is
    /// dropped at the end of `from_bytes`**. The internal copy held inside the
    /// returned `TenantDerivationKey` is itself zeroed on drop via
    /// [`ZeroizeOnDrop`].
    ///
    /// Lifetime of the secret bytes:
    /// 1. Caller holds `Zeroizing<[u8; 32]>`.
    /// 2. Inside this function, the `[u8; 32]` is copied (it implements
    ///    `Copy`) into the new `TenantDerivationKey`. For a brief window
    ///    while this function's stack frame is live, the bytes exist in
    ///    two places simultaneously — but both are owned by zeroize-aware
    ///    types; nothing leaks past the function boundary.
    /// 3. The `Zeroizing` argument is dropped at end-of-scope and scrubs
    ///    the caller-side copy.
    /// 4. Only the `TenantDerivationKey` retains the bytes from then on,
    ///    and they are scrubbed when it is dropped.
    ///
    /// Callers are responsible for ensuring the underlying bytes came from a
    /// trusted source — this constructor performs no validation beyond the
    /// size invariant that the type system already enforces.
    #[must_use]
    pub fn from_bytes(bytes: Zeroizing<[u8; TDK_LEN]>) -> Self {
        Self(*bytes)
    }

    /// Borrow the raw key bytes for the HMAC engine. Crate-private — never
    /// expose this to callers.
    pub(crate) fn as_bytes(&self) -> &[u8; TDK_LEN] {
        &self.0
    }
}

impl core::fmt::Debug for TenantDerivationKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TenantDerivationKey(REDACTED)")
    }
}

/// Per-tenant path prefix used in R2 / KV / D1 keys.
///
/// The internal byte buffer holds exactly [`TENANT_PREFIX_LEN`] ASCII bytes
/// drawn from the URL-safe base64 alphabet (RFC 4648 §5, no padding). The
/// field is private so that **only** [`derive_prefix`] can produce a valid
/// `TenantPrefix`.
///
/// External callers serialize via [`Self::as_str`] or [`std::fmt::Display`].
/// Raw byte access is intentionally not exposed to comply with
/// `WI-S01-001 §7` anti-scope "Não expor raw bytes de TenantPrefix fora do
/// crate" — D1 prepared statements that bind a `BLOB(16)` parameter can call
/// `prefix.as_str().as_bytes()` to obtain a `&[u8]` of length 16.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TenantPrefix([u8; TENANT_PREFIX_LEN]);

impl TenantPrefix {
    /// View the prefix as a `&str`. Always valid UTF-8 because the buffer
    /// only contains ASCII characters from the URL-safe base64 alphabet.
    #[must_use]
    #[allow(
        clippy::expect_used,
        reason = "URL_SAFE_NO_PAD output is restricted to ASCII; from_utf8 is total here"
    )]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0)
            .expect("TenantPrefix bytes are always ASCII from the URL-safe base64 alphabet")
    }
}

impl core::fmt::Display for TenantPrefix {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl core::fmt::Debug for TenantPrefix {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TenantPrefix({})", self.as_str())
    }
}

/// Derive the canonical tenant path prefix for `tenant_id` under `tdk`.
///
/// # Algorithm
///
/// 1. `mac = HMAC-SHA256(key = tdk, msg = tenant_id.as_bytes())` (32 bytes)
/// 2. `b64 = base64::URL_SAFE_NO_PAD.encode(mac)` (43 ASCII chars)
/// 3. `prefix = b64[..16]` (16 ASCII chars; ~96 bits of entropy, sufficient
///    for tenant namespacing per `remote_cache_product_profile.md §7.1`)
///
/// `tenant_id.as_bytes()` returns the canonical 16-byte big-endian UUID
/// representation, stable across versions of the `uuid` crate.
///
/// # Determinism & injectivity
///
/// HMAC-SHA256 is a deterministic PRF: same `(tdk, tenant_id)` ⇒ same prefix.
/// With 96 bits of output entropy, the probability that two independently
/// chosen tenant IDs produce the same prefix is `≈ 2^-96` per pair. Birthday
/// bound: ~`2^48` tenants would be required for the probability of *any*
/// collision in the system to exceed 50%; at any plausible tenant scale
/// (e.g. `10^9 ≈ 2^30` tenants) the expected number of pairwise collisions is
/// `≈ 2^{2·30 - 96 - 1} = 2^{-37}` — negligible. Collisions never grant
/// authorization (which is enforced by layers 1-4 of `auth_model.md §8.1`),
/// only namespace co-residence.
#[must_use]
#[allow(
    clippy::expect_used,
    reason = "HMAC-SHA256 with a fixed 32-byte key cannot fail; URL_SAFE_NO_PAD into a 43-byte buffer cannot overflow"
)]
pub fn derive_prefix(tdk: &TenantDerivationKey, tenant_id: Uuid) -> TenantPrefix {
    let mut mac = HmacSha256::new_from_slice(tdk.as_bytes())
        .expect("HMAC-SHA256 accepts any 32-byte key by construction");
    mac.update(tenant_id.as_bytes());
    let digest = mac.finalize().into_bytes();

    let mut buf = [0u8; HMAC_B64_LEN];
    let written = URL_SAFE_NO_PAD
        .encode_slice(digest.as_slice(), &mut buf)
        .expect("URL_SAFE_NO_PAD of 32 bytes fits in 43 bytes (no padding)");
    debug_assert_eq!(
        written, HMAC_B64_LEN,
        "base64 length contract violated: 32 raw bytes must produce 43 url-safe chars"
    );

    let mut out = [0u8; TENANT_PREFIX_LEN];
    out.copy_from_slice(&buf[..TENANT_PREFIX_LEN]);
    TenantPrefix(out)
}
