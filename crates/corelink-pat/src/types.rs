//! Opaque newtypes for PAT lifecycle data.
//!
//! Compile-time prevention of accidental plaintext leakage is the
//! load-bearing design constraint: a [`PatPlaintext`] does **not**
//! implement `Display`, `Serialize`, or a public `Debug` showing
//! bytes. The single legal path to a `String` is the explicit
//! [`PatPlaintext::into_string`] consumer call inside the
//! `mint()` response handler — and that handler is the only place
//! the plaintext surfaces to the user (one-time display per
//! `auth_model.md §2.2` + `key_management.md §3.13`).
//!
//! `PatHash` (PHC string) is logged-safe; `PatId` is the DB primary
//! key surface; `PatSigningKey` wraps the per-region HMAC key and
//! redacts in `Debug`.

use std::fmt;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::scopes::PatScopes;

/// PAT plaintext newtype. Constructed only by the [`mint()`]
/// function (re-exported from `crate::mint::mint`); the only consumer
/// is the token-creation API endpoint that returns the canonical
/// `corelink_<env>_…` string to the user **once**.
///
/// [`mint()`]: crate::mint::mint
///
/// # Invariants
///
/// - No `Display` impl; no `Serialize` impl; no public `Debug` impl
///   that reveals bytes.
/// - Single `into_string()` consumer; an attempt to clone or copy
///   the inner value through any other channel is a compile error.
/// - The inner `String` is zeroized on drop via the
///   `zeroize::Zeroize` trait — equivalent to wrapping it in
///   `Zeroizing<String>`.
pub struct PatPlaintext(String);

impl PatPlaintext {
    pub(crate) fn new(s: String) -> Self {
        Self(s)
    }

    /// Consume the plaintext, returning the underlying `String`.
    /// **Caller MUST hand the result directly to the user-facing API
    /// response and MUST NOT log, persist, or branch on its contents.**
    /// The `into_*` rename (vs `as_str`) intentionally forces a `move`
    /// so the plaintext cannot be retained inside this crate's stack
    /// frames after the response has been emitted.
    #[must_use]
    pub fn into_string(self) -> String {
        // Move out of `self.0` into a local, then prevent the Drop on
        // `self` from running on a now-empty inner string.
        let mut inner = String::new();
        let mut me = self;
        core::mem::swap(&mut inner, &mut me.0);
        // `me.0` is now empty; the Drop scrub below is a no-op.
        inner
    }

    /// **Test-only** length accessor. Production code never branches
    /// on the plaintext length; this is provided so tests can sanity
    /// check format without exposing the bytes.
    #[doc(hidden)]
    #[must_use]
    pub fn len_for_test(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for PatPlaintext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NEVER reveal length, env, or any prefix of the bytes — even
        // metadata can leak structure to log aggregators.
        write!(f, "PatPlaintext(<redacted>)")
    }
}

impl Drop for PatPlaintext {
    fn drop(&mut self) {
        // SAFETY (sound; not unsafe code): `String::zeroize` from the
        // `zeroize` crate scrubs the heap buffer in place. After this
        // the buffer's bytes are zero; the allocation is then freed
        // by the `String` drop.
        self.0.zeroize();
    }
}

/// PHC string holding an Argon2id hash. Safe to log, persist, and
/// pass through tracing spans — the embedded salt + hash bytes are
/// computationally bound by the OWASP-2024 cost parameters.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatHash(String);

impl PatHash {
    /// Construct from a PHC string (no validation here; verify
    /// happens during `verify_argon2id` at the parser boundary).
    #[must_use]
    pub fn from_phc_string(s: String) -> Self {
        Self(s)
    }

    /// Borrow the canonical PHC string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the inner `String` for DB persistence.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for PatHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // PHC string is safe but verbose; render the algorithm prefix
        // for diagnostic value, not the salt/hash bytes.
        let head = self.0.split('$').take(4).collect::<Vec<_>>().join("$");
        write!(f, "PatHash({head}$<…>)")
    }
}

/// PAT primary-key surface (Neon `pat.id` UUIDv7 column per
/// `data_model.md §4.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatId(pub Uuid);

impl PatId {
    /// Generate a fresh PAT id (UUIDv7; time-ordered for B-tree
    /// locality per Neon SoT layout).
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for PatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// 16-character base32 (Crockford-set) lookup key embedded in every
/// PAT plaintext. The token_id is **not** secret — it indexes the
/// `pat` table (Neon `pat.token_id` UNIQUE column) and lets the verify
/// path do an O(1) lookup before the (expensive) Argon2id verify.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatTokenId(String);

/// Canonical [`PatTokenId`] length in characters (16 base32 chars =
/// 80 bits; collision probability for 2^32 active PATs is ~2^-16,
/// well under the `auth_model.md §2.2` budget).
pub const PAT_TOKEN_ID_LEN: usize = 16;

impl PatTokenId {
    /// Construct a `PatTokenId` from an arbitrary string. Returns
    /// `None` if the string is not exactly 16 chars from the
    /// Crockford base32 alphabet (`0-9`, `A-Z` excluding `I L O U`).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.len() != PAT_TOKEN_ID_LEN {
            return None;
        }
        for c in raw.chars() {
            if !is_crockford_b32(c) {
                return None;
            }
        }
        Some(Self(raw.to_owned()))
    }

    pub(crate) fn from_validated(s: String) -> Self {
        Self(s)
    }

    /// Borrow the canonical 16-char string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Predicate: is `c` a valid Crockford base32 char (uppercase only,
/// excludes `I L O U`)?
#[inline]
const fn is_crockford_b32(c: char) -> bool {
    matches!(c,
        '0'..='9'
        | 'A'..='H' | 'J' | 'K' | 'M' | 'N'
        | 'P'..='T' | 'V'..='Z')
}

impl fmt::Debug for PatTokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Token id is non-secret; safe to render directly.
        write!(f, "PatTokenId({})", self.0)
    }
}

impl fmt::Display for PatTokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Per-region HMAC signing key for the PAT fast-fail layer (`hmac_sig`
/// segment). Rotated every 24h with overlap per
/// `key_management.md §3.2.1`; this newtype redacts in `Debug` and
/// scrubs on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PatSigningKey(Vec<u8>);

impl PatSigningKey {
    /// Construct from raw bytes. Rejects keys shorter than 32 bytes
    /// (HMAC-SHA256 block size minimum for canonical security).
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, crate::PatError> {
        if bytes.len() < 32 {
            return Err(crate::PatError::SigningKeyTooShort);
        }
        Ok(Self(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for PatSigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PatSigningKey(<redacted len={}>)", self.0.len())
    }
}

/// Distinct PAT environments (the `<env>` segment of the canonical
/// format). The wire literals `pat | ci | ro` are exhaustive at
/// SEAL; a future `exec` (Phase 2 executor) is reserved per
/// `auth_model.md §1.5` and would be added here behind a major bump.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatEnv {
    /// User-emitted Personal Access Token.
    Pat,
    /// CI runner machine token.
    Ci,
    /// Read-only token (forks, mirrors).
    Ro,
}

impl PatEnv {
    /// Canonical 2-3 char wire literal for the `<env>` segment.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Pat => "pat",
            Self::Ci => "ci",
            Self::Ro => "ro",
        }
    }

    /// Inverse of [`Self::as_wire`]. Returns `None` for any string
    /// not exactly `"pat" | "ci" | "ro"`.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "pat" => Some(Self::Pat),
            "ci" => Some(Self::Ci),
            "ro" => Some(Self::Ro),
            _ => None,
        }
    }
}

impl fmt::Display for PatEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

/// Tenant identifier surface (UUID column `pat.tenant_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(pub Uuid);

/// Principal identifier surface (UUID column `pat.principal_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrincipalId(pub Uuid);

/// Persistent PAT row (the canonical Neon `pat` row representation;
/// per `data_model.md §4.1` minus DB-derived audit timestamps).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pat {
    /// PAT primary key (UUIDv7).
    pub id: PatId,
    /// Indexed lookup key embedded in the plaintext.
    pub token_id: PatTokenId,
    /// PHC string — Argon2id hash of the `random_secret` segment.
    pub hash: PatHash,
    /// Bitset of granted scopes.
    pub scopes: PatScopes,
    /// Environment (env segment of plaintext).
    pub env: PatEnv,
    /// Owning tenant.
    pub tenant_id: TenantId,
    /// Owning principal (Clerk `sub`).
    pub principal_id: PrincipalId,
    /// Mint timestamp.
    pub issued_at: SystemTime,
    /// Optional expiration timestamp.
    pub expires_at: Option<SystemTime>,
    /// Signing key id (rotation generation; per `key_management.md §3.2.1`).
    pub signing_key_id: u32,
}
