//! Constant-time secret comparison + API-key shape validation.
//!
//! Per `INV-OBS-CT-SECRET-EQ`, every comparison of a customer-supplied
//! API key / password / shared-secret against the configured canonical
//! value goes through [`constant_time_secret_eq`] (powered by
//! `subtle::ConstantTimeEq`). This removes a class of timing oracles
//! when the customer mis-pastes a key — without this, a naive `==`
//! comparison would early-exit on the first mismatching byte, leaking
//! the matching prefix length over a sufficiently large sample (per
//! `corelink-audit` Lote 10.6bis pattern + Cloudflare Workers
//! constant-time guidance).

use subtle::ConstantTimeEq;

use crate::otel::error::SecretValidationError;

/// Constant-time equality check between two secret byte sequences.
///
/// Returns `true` iff the two slices are byte-identical. Length
/// mismatch returns `false` without inspecting the bytes (per
/// `subtle::ConstantTimeEq` semantics — the length itself is not a
/// secret here because the canonical band is published in
/// [`validate_api_key_shape`]).
///
/// # Examples
///
/// ```
/// use corelink_telemetry::otel::constant_time_secret_eq;
/// assert!(constant_time_secret_eq(b"abc", b"abc"));
/// assert!(!constant_time_secret_eq(b"abc", b"abd"));
/// assert!(!constant_time_secret_eq(b"abc", b"abcd"));
/// ```
#[must_use]
pub fn constant_time_secret_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// Validate the canonical shape (length + ASCII) of a secret at
/// config-load time.
///
/// Vendors publish canonical lengths for their API keys:
///
/// - Datadog API key: 32-char hex (`[0-9a-f]{32}`); APP key: 40-char hex.
///   Conservative band: `[16, 128]` ASCII.
/// - Grafana Cloud HMAC token / Prom remote-write password: opaque
///   ASCII; conservative band `[16, 256]`.
/// - OTel Collector bearer token: opaque ASCII; conservative band
///   `[8, 4096]`.
///
/// This function enforces only the structural shape; the real
/// `auth-reject` signal is the vendor's HTTP 401/403 at first-use.
///
/// # Errors
///
/// Returns [`SecretValidationError::Empty`] /
/// [`SecretValidationError::LengthOutOfRange`] /
/// [`SecretValidationError::NonAscii`] per the canonical band.
pub fn validate_api_key_shape(
    secret: &str,
    min: usize,
    max: usize,
) -> Result<(), SecretValidationError> {
    if secret.is_empty() {
        return Err(SecretValidationError::Empty);
    }
    let len = secret.len();
    if len < min || len > max {
        return Err(SecretValidationError::LengthOutOfRange {
            actual: len,
            min,
            max,
        });
    }
    if !secret.is_ascii() {
        return Err(SecretValidationError::NonAscii);
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_returns_true_for_identical_bytes() {
        assert!(constant_time_secret_eq(b"hello", b"hello"));
        assert!(constant_time_secret_eq(b"", b""));
    }

    #[test]
    fn ct_eq_returns_false_for_byte_mismatch() {
        assert!(!constant_time_secret_eq(b"hello", b"hellp"));
    }

    #[test]
    fn ct_eq_returns_false_for_length_mismatch() {
        assert!(!constant_time_secret_eq(b"hello", b"hello!"));
        assert!(!constant_time_secret_eq(b"a", b""));
    }

    #[test]
    fn validate_shape_empty_rejected() {
        let err = validate_api_key_shape("", 16, 128).unwrap_err();
        assert_eq!(err, SecretValidationError::Empty);
    }

    #[test]
    fn validate_shape_too_short_rejected() {
        let err = validate_api_key_shape("abc", 16, 128).unwrap_err();
        assert!(matches!(
            err,
            SecretValidationError::LengthOutOfRange { .. }
        ));
    }

    #[test]
    fn validate_shape_too_long_rejected() {
        let big = "x".repeat(200);
        let err = validate_api_key_shape(&big, 16, 128).unwrap_err();
        assert!(matches!(
            err,
            SecretValidationError::LengthOutOfRange { .. }
        ));
    }

    #[test]
    fn validate_shape_non_ascii_rejected() {
        let s = "abcdef".to_owned() + "\u{1F600}";
        let err = validate_api_key_shape(&s, 4, 128).unwrap_err();
        assert_eq!(err, SecretValidationError::NonAscii);
    }

    #[test]
    fn validate_shape_canonical_datadog_key_accepted() {
        let dd = "a".repeat(32);
        validate_api_key_shape(&dd, 16, 128).unwrap();
    }
}
