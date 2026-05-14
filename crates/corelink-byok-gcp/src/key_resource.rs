//! Cloud KMS key resource path validation.
//!
//! Canonical forms accepted:
//!
//! ```text
//! projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}
//! projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}
//! ```
//!
//! Per Cloud KMS naming rules:
//! - `project` segment: `[a-z][a-z0-9-]{4,28}[a-z0-9]` (project IDs).
//! - `location`, `ring`, `key`: `[A-Za-z0-9_-]{1,63}`.
//! - `version`: positive integer.
//!
//! Use [`is_valid_cloud_kms_resource`] for reject-malformed validation
//! (R2-7 quality gate) and [`strip_version_suffix`] to derive the parent
//! CryptoKey path for `decrypt` / `get` operations.

// Note: when the `production` feature is off we still ship this module so
// the stub path can call it; the regex itself is only compiled when needed.

#[cfg(feature = "production")]
use std::sync::OnceLock;

#[cfg(feature = "production")]
fn full_regex() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Anchored on both ends. Allow either the bare CryptoKey or the
        // CryptoKey + cryptoKeyVersions/<N> form.
        #[allow(clippy::expect_used)]
        regex::Regex::new(
            r"^projects/[a-z][a-z0-9-]{4,28}[a-z0-9]/locations/[A-Za-z0-9_-]{1,63}/keyRings/[A-Za-z0-9_-]{1,63}/cryptoKeys/[A-Za-z0-9_-]{1,63}(/cryptoKeyVersions/[1-9][0-9]*)?$",
        )
        .expect("static regex compiles")
    })
}

/// Return `true` iff `s` matches the canonical Cloud KMS resource path.
#[cfg(feature = "production")]
#[must_use]
pub(crate) fn is_valid_cloud_kms_resource(s: &str) -> bool {
    full_regex().is_match(s)
}

/// Fallback validator when the `production` feature is off (used by the
/// stub for hygiene). Pure-Rust shape check; less strict than the regex.
#[cfg(not(feature = "production"))]
#[must_use]
pub(crate) fn is_valid_cloud_kms_resource(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();
    matches!(
        parts.as_slice(),
        ["projects", _, "locations", _, "keyRings", _, "cryptoKeys", _]
            | ["projects", _, "locations", _, "keyRings", _, "cryptoKeys", _, "cryptoKeyVersions", _]
    ) && parts.iter().all(|p| !p.is_empty())
}

/// If `s` ends with `/cryptoKeyVersions/<N>`, return the parent CryptoKey
/// path; otherwise return `s` unchanged.
#[cfg_attr(not(feature = "production"), allow(dead_code))]
pub(crate) fn strip_version_suffix(s: &str) -> String {
    if let Some(idx) = s.rfind("/cryptoKeyVersions/") {
        s[..idx].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn valid_resource_paths() {
        assert!(is_valid_cloud_kms_resource(
            "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk"
        ));
        assert!(is_valid_cloud_kms_resource(
            "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk/cryptoKeyVersions/3"
        ));
    }

    #[test]
    fn invalid_resource_paths() {
        // Wrong prefix.
        assert!(!is_valid_cloud_kms_resource(
            "arn:aws:kms:us-east-1:123:key/abc"
        ));
        // Missing segments.
        assert!(!is_valid_cloud_kms_resource("projects/p/locations/l/keyRings/r"));
        // Trailing slash.
        assert!(!is_valid_cloud_kms_resource(
            "projects/example-project/locations/us-east1/keyRings/byok/cryptoKeys/customer-cmk/"
        ));
        // Empty.
        assert!(!is_valid_cloud_kms_resource(""));
    }

    #[test]
    fn strip_version() {
        assert_eq!(
            strip_version_suffix("projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/3"),
            "projects/p/locations/l/keyRings/r/cryptoKeys/k"
        );
        assert_eq!(
            strip_version_suffix("projects/p/locations/l/keyRings/r/cryptoKeys/k"),
            "projects/p/locations/l/keyRings/r/cryptoKeys/k"
        );
    }
}
