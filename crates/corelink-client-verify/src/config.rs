//! [`VerifyConfig`] — knobs controlling [`ClientVerifier`](crate::ClientVerifier)
//! behavior.
//!
//! The canonical default per CTRL-CAS-002 is `enabled=true`, `warn_on_optout=true`.
//! Callers MUST go through [`VerifyConfig::new`] (default-on) or
//! [`VerifyConfig::disabled`] (explicit opt-out) — the underlying fields
//! are `pub(crate)` so a downstream caller cannot silently flip
//! `enabled = false` via a struct-update expression.

/// Static configuration for a [`ClientVerifier`](crate::ClientVerifier).
///
/// # Default-on contract
///
/// `VerifyConfig::default()` always returns `enabled=true`, `warn_on_optout=true`.
/// Bypassing the verify step requires the explicit
/// [`VerifyConfig::disabled`] constructor, which both flips the flag AND
/// causes the resulting [`ClientVerifier`](crate::ClientVerifier) to emit a
/// canonical warning + counter increment when constructed. CI gates the
/// default invariant via [`prop_default_is_enabled`] /
/// `default_is_enabled` regression tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyConfig {
    /// Whether verification is performed. Default `true`. Bypass requires
    /// the explicit [`VerifyConfig::disabled`] builder.
    pub(crate) enabled: bool,
    /// Whether a `tracing::warn!` log is emitted when the SDK constructs
    /// a verifier with `enabled=false`. Default `true`.
    pub(crate) warn_on_optout: bool,
}

impl VerifyConfig {
    /// Canonical default config: verify enabled, warn on opt-out.
    ///
    /// This is the path SDK callers should take. `Default::default()`
    /// returns the same value; the explicit constructor exists so
    /// `VerifyConfig::new()` reads as intent-revealing in code review.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            enabled: true,
            warn_on_optout: true,
        }
    }

    /// Explicit opt-out: verify disabled. Construction of the resulting
    /// verifier will emit a `tracing::warn!` + counter increment unless
    /// `warn_on_optout` is also disabled (test-only).
    ///
    /// Use only when the caller has another integrity check in place
    /// (e.g. they are the cache server itself doing a different verify).
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            warn_on_optout: true,
        }
    }

    /// Crate-internal constructor that disables the opt-out warning.
    /// Used by:
    /// - the property-test layer to avoid spamming the tracing
    ///   subscriber across thousands of iterations,
    /// - the FFI surface (feature `ffi`) when the C caller passes
    ///   `warn=0` to [`corelink_verifier_new_disabled`] (used by
    ///   Python pyO3 `__del__`-driven teardown paths that cannot
    ///   afford an extra log).
    ///
    /// Crate-private (`pub(crate)`) so the public Rust API surface
    /// does NOT expose a silent-disabled path; SDK FFI consumers must
    /// opt in via the explicit C-ABI flag, and Rust callers route
    /// through the warn-by-default [`disabled`] constructor.
    ///
    /// [`corelink_verifier_new_disabled`]: crate::ffi::corelink_verifier_new_disabled
    /// [`disabled`]: VerifyConfig::disabled
    #[cfg_attr(
        not(any(feature = "ffi", test)),
        allow(
            dead_code,
            reason = "Only consumed by the gated FFI surface and the in-crate test layer"
        )
    )]
    pub(crate) const fn disabled_silent() -> Self {
        Self {
            enabled: false,
            warn_on_optout: false,
        }
    }

    /// Whether verification is enabled. Public read-only accessor; the
    /// underlying field stays `pub(crate)` to forbid silent opt-out via
    /// struct-update expressions.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Whether a warning is emitted when the SDK constructs a verifier
    /// with `enabled=false`.
    #[must_use]
    pub const fn warn_on_optout(&self) -> bool {
        self.warn_on_optout
    }
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled_with_warn() {
        let cfg = VerifyConfig::default();
        assert!(cfg.enabled());
        assert!(cfg.warn_on_optout());
    }

    #[test]
    fn new_matches_default() {
        assert_eq!(VerifyConfig::new(), VerifyConfig::default());
    }

    #[test]
    fn disabled_is_not_enabled_but_warns() {
        let cfg = VerifyConfig::disabled();
        assert!(!cfg.enabled());
        assert!(
            cfg.warn_on_optout(),
            "disabled() must keep warn_on_optout=true; that is the canonical opt-out path"
        );
    }

    #[test]
    fn disabled_silent_is_quiet() {
        let cfg = VerifyConfig::disabled_silent();
        assert!(!cfg.enabled());
        assert!(
            !cfg.warn_on_optout(),
            "disabled_silent() must NOT warn — that is the contract"
        );
    }
}
