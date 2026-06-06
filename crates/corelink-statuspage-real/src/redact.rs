//! Credential redaction helper.
//!
//! Per CTRL-PRIV-001 the `STATUSPAGE_API_KEY` is NEVER logged plaintext.
//! Audit envelopes carry only the redacted form produced here.

/// Redact a Statuspage `Authorization: OAuth <api_key>` header value.
///
/// Returns `OAuth ***<last4>` when the key is long enough to keep a
/// non-empty trailing fingerprint, or `OAuth ***` otherwise. Never
/// returns more than the last 4 bytes of the key.
#[must_use]
pub fn redact_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return "OAuth ***".to_string();
    }
    // Keep at most 4 trailing chars by codepoint to avoid splitting on
    // a UTF-8 boundary (Statuspage keys are ASCII but defense-in-depth).
    let tail: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if tail.is_empty() {
        return "OAuth ***".to_string();
    }
    format!("OAuth ***{tail}")
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
    fn redact_keeps_only_last_four() {
        let red = redact_api_key("abcdef1234567890");
        assert_eq!(red, "OAuth ***7890");
    }

    #[test]
    fn redact_empty_returns_canonical_marker() {
        assert_eq!(redact_api_key(""), "OAuth ***");
        assert_eq!(redact_api_key("   "), "OAuth ***");
    }

    #[test]
    fn redact_short_key_still_masked() {
        let red = redact_api_key("ab");
        assert_eq!(red, "OAuth ***ab");
    }

    #[test]
    fn redact_never_contains_full_key() {
        let key = "supersecretstatuspageapikey1234";
        let red = redact_api_key(key);
        assert!(!red.contains("supersecret"));
        assert!(!red.contains("statuspageapikey"));
        assert!(red.ends_with("1234"));
    }
}
