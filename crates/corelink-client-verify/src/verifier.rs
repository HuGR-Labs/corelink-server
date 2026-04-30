//! [`ClientVerifier`] — synchronous BLAKE3 digest verify with
//! constant-time compare and a hard default-on policy.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::config::VerifyConfig;
use crate::digest::Digest;
use crate::error::VerifyError;

/// Process-wide counter of constructed `disabled()` verifiers.
///
/// Incremented inside [`ClientVerifier::new`] when `config.enabled ==
/// false`. The Grafana telemetry pipeline (S-15 SDK observability)
/// scrapes this via the FFI accessor [`opt_out_total`] and exposes it
/// as `corelink_client_verify_optout_total{lang}` per WI §6.1.5.
///
/// `Ordering::Relaxed` is sufficient: the counter is monotonic, has no
/// happens-before responsibilities, and a small skew between the
/// observed value and the actual emission count is tolerable for a
/// long-tail observability metric. Using `Relaxed` documents that
/// readers MUST NOT use this to enforce safety-critical invariants;
/// the canonical default-on enforcement comes from the type system
/// (private fields + builder pattern), not from this counter.
static OPT_OUT_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Read the process-wide opt-out counter.
///
/// Test infrastructure and the S-15 telemetry exporter use this to
/// observe how many times a `VerifyConfig::disabled()` has been wrapped
/// in a `ClientVerifier`. Read-only by design — the counter is
/// monotonic non-decreasing and never reset (a process-wide reset
/// would mask the very signal the metric exists to surface).
#[must_use]
pub fn opt_out_total() -> u64 {
    OPT_OUT_TOTAL.load(Ordering::Relaxed)
}

/// Client-side BLAKE3 digest verifier.
///
/// Constructed with a [`VerifyConfig`]; [`ClientVerifier::verify`] then
/// computes BLAKE3 of the body and compares against the claimed digest
/// in constant time. The default-on contract is enforced by
/// `VerifyConfig`: callers cannot construct a `ClientVerifier` whose
/// configuration silently opted out without going through the explicit
/// [`VerifyConfig::disabled`] path, which also emits a warning log +
/// counter increment.
#[derive(Debug, Clone, Copy)]
pub struct ClientVerifier {
    config: VerifyConfig,
}

impl ClientVerifier {
    /// Construct a verifier from `config`.
    ///
    /// If `config.enabled() == false`:
    /// - `OPT_OUT_TOTAL` is incremented (always, regardless of
    ///   `warn_on_optout`).
    /// - A `tracing::warn!` event is emitted when `warn_on_optout`
    ///   is set, carrying the canonical message
    ///   `"client_verify=disabled; proceed at own risk"`.
    #[must_use]
    pub fn new(config: VerifyConfig) -> Self {
        if !config.enabled() {
            // Counter is unconditional so the S-15 scrape sees every
            // opt-out construction, even ones that suppress the warn
            // log via `warn_on_optout=false` (e.g. property tests).
            OPT_OUT_TOTAL.fetch_add(1, Ordering::Relaxed);
            if config.warn_on_optout() {
                tracing::warn!(
                    target: "corelink_client_verify::optout",
                    code = "COR_CAS_VERIFY_DISABLED",
                    "client_verify=disabled; proceed at own risk"
                );
            }
        }
        Self { config }
    }

    /// Construct a default-on verifier (equivalent to
    /// `ClientVerifier::new(VerifyConfig::default())`).
    #[must_use]
    pub fn default_on() -> Self {
        Self::new(VerifyConfig::default())
    }

    /// Borrow the config the verifier was constructed with. Read-only.
    #[must_use]
    pub const fn config(&self) -> &VerifyConfig {
        &self.config
    }

    /// Verify `body` matches `expected`.
    ///
    /// On a default-on verifier:
    /// - `Ok(())` if `BLAKE3(body) == expected` (constant-time
    ///   compare via `subtle::ConstantTimeEq`).
    /// - `Err(VerifyError::DigestMismatch)` otherwise; both hex-rendered
    ///   digests are attached for diagnostic surfaces.
    ///
    /// On a verifier constructed via [`VerifyConfig::disabled`], this
    /// method returns [`VerifyError::VerifyDisabled`] **without**
    /// computing the BLAKE3. Callers expecting opt-out should not
    /// invoke `verify` at all; the method does not silently
    /// short-circuit to `Ok(())` because doing so would mask a calling
    /// bug ("I thought I had verify on but I shipped a disabled config").
    pub fn verify(&self, body: &[u8], expected: &Digest) -> Result<(), VerifyError> {
        if !self.config.enabled() {
            return Err(VerifyError::VerifyDisabled);
        }
        let computed = Digest::compute(body);
        if computed.verify_constant_time(expected) {
            Ok(())
        } else {
            Err(VerifyError::DigestMismatch {
                expected: expected.to_hex(),
                computed: computed.to_hex(),
            })
        }
    }
}

impl Default for ClientVerifier {
    fn default() -> Self {
        Self::default_on()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled() {
        let v = ClientVerifier::default();
        assert!(v.config().enabled());
        assert!(v.config().warn_on_optout());
    }

    #[test]
    fn verify_happy_path() {
        let v = ClientVerifier::default_on();
        let body = b"hello world";
        let d = Digest::compute(body);
        assert!(v.verify(body, &d).is_ok());
    }

    #[test]
    fn verify_mismatch_returns_hex_digests() {
        let v = ClientVerifier::default_on();
        let body = b"hello world";
        let wrong = Digest::compute(b"goodbye world");
        let err = v.verify(body, &wrong).expect_err("should mismatch");
        match err {
            VerifyError::DigestMismatch { expected, computed } => {
                assert_eq!(expected, wrong.to_hex());
                assert_eq!(computed, Digest::compute(body).to_hex());
                assert_ne!(expected, computed);
            }
            VerifyError::VerifyDisabled => panic!("default verifier must verify"),
        }
    }

    #[test]
    fn disabled_short_circuits_to_verify_disabled_error() {
        // Use silent variant to keep the test log clean; counter still ticks.
        let before = opt_out_total();
        let v = ClientVerifier::new(VerifyConfig::disabled_silent());
        let after = opt_out_total();
        assert_eq!(after, before + 1);

        let body = b"hello world";
        let d = Digest::compute(body);
        let err = v.verify(body, &d).expect_err("disabled must err");
        assert!(matches!(err, VerifyError::VerifyDisabled));
        assert_eq!(err.code(), "COR_CAS_VERIFY_DISABLED");
    }
}
