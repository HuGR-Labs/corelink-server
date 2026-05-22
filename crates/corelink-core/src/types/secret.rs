//! [`SecretWrap`] — thin domain-typed wrapper over [`secrecy::SecretString`].
//!
//! Wave-33 Stage 0: every credential / API-key / token across CoreLink
//! is already wrapped in `secrecy::SecretString` (Stripe client, BYOK
//! providers, Clerk env config, etc.). `SecretWrap` is the canonical
//! re-export point so future call sites can `use corelink_core::SecretWrap`
//! without each crate importing `secrecy` independently.
//!
//! Charter constraint preserved: `SecretString` from `secrecy` for
//! every credential currently wrapped. `Debug` / `Display` are
//! intentionally NOT impl'd on the wrapper — exposing the secret
//! requires going through [`secrecy::ExposeSecret`].

use core::fmt;

use secrecy::{ExposeSecret, SecretString};

/// Domain-typed credential wrapper over [`secrecy::SecretString`].
///
/// The inner [`SecretString`] zeroizes on drop and refuses to leak its
/// content via `Debug` / `Display`. Callers that need the raw bytes
/// must explicitly call [`Self::expose`].
#[non_exhaustive]
pub struct SecretWrap(SecretString);

impl SecretWrap {
    /// Wrap a raw `String` as a secret. The string is moved into the
    /// inner [`SecretString`] which will zeroize it on drop.
    #[must_use]
    pub fn new(secret: String) -> Self {
        Self(SecretString::from(secret))
    }

    /// Wrap an existing [`SecretString`] without re-allocating.
    #[must_use]
    pub const fn from_secret_string(s: SecretString) -> Self {
        Self(s)
    }

    /// Expose the inner secret as a `&str`. Use sparingly and never
    /// across log statements / `Debug` impls.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Borrow the inner [`SecretString`] for code that already speaks
    /// the `secrecy` API.
    #[must_use]
    pub fn as_secret_string(&self) -> &SecretString {
        &self.0
    }
}

impl fmt::Debug for SecretWrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretWrap([REDACTED])")
    }
}

impl From<String> for SecretWrap {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<SecretString> for SecretWrap {
    fn from(value: SecretString) -> Self {
        Self::from_secret_string(value)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn expose_returns_inner_string() {
        let s = SecretWrap::new("hunter2".to_owned());
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn debug_redacts_secret() {
        let s = SecretWrap::new("hunter2".to_owned());
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("hunter2"));
        assert!(dbg.contains("REDACTED"));
    }

    #[test]
    fn from_string_works() {
        let s: SecretWrap = "tok_abc".to_owned().into();
        assert_eq!(s.expose(), "tok_abc");
    }
}
