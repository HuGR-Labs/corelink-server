//! Vault Transit key-name validation.
//!
//! Vault Transit key names must be alphanumeric + dashes + underscores
//! (matching the regex `^[A-Za-z0-9_-]+$`). Anything containing `/`, `..`,
//! whitespace, or path traversal characters is rejected pre-flight so a
//! malformed identifier can never reach the Vault HTTP layer (and never
//! contribute to a URL injection).

use regex::Regex;
use std::sync::OnceLock;

/// Matches a valid Vault Transit key name.
fn key_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Construction is infallible for a static literal; on the impossible
    // failure we fall back to a regex that matches nothing, so callers
    // simply observe "malformed" rather than a panic.
    RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9_-]+$").unwrap_or_else(|_| {
            #[allow(clippy::expect_used)]
            Regex::new("$^").expect("trivial regex must compile")
        })
    })
}

/// Returns `true` if `name` is a valid Vault Transit key name.
#[must_use]
pub fn is_valid_key_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 256 {
        return false;
    }
    key_name_re().is_match(name)
}

/// Extract the bare key name from a `transit/keys/<name>` style identifier.
///
/// Returns the input unchanged if it does not contain a `/`.
#[must_use]
pub fn extract_key_name(key_arn_or_id: &str) -> &str {
    key_arn_or_id.rsplit('/').next().unwrap_or(key_arn_or_id)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn accepts_alphanumeric() {
        assert!(is_valid_key_name("my-key-01"));
        assert!(is_valid_key_name("ABC_xyz_9"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_valid_key_name("../etc/passwd"));
        assert!(!is_valid_key_name("a/b"));
        assert!(!is_valid_key_name(""));
    }

    #[test]
    fn rejects_whitespace_and_specials() {
        assert!(!is_valid_key_name("my key"));
        assert!(!is_valid_key_name("k\tab"));
        assert!(!is_valid_key_name("k$y"));
    }

    #[test]
    fn extract_strips_prefix() {
        assert_eq!(extract_key_name("transit/keys/my-key"), "my-key");
        assert_eq!(extract_key_name("plain-name"), "plain-name");
    }
}
