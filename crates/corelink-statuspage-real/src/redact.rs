//! Credential redaction helper.
//!
//! Per CTRL-PRIV-001 the `STATUSPAGE_API_KEY` is NEVER logged plaintext.
//! Audit envelopes carry only the redacted form produced here.

/// Redact a Statuspage `Authorization: OAuth <api_key>` header value.
///
/// Returns `OAuth ***<last4>` only when the trimmed key has more than
/// four Unicode codepoints; shorter keys are fully masked as `OAuth ***`.
/// Never returns more than the last 4 Unicode codepoints of the trimmed key.
#[must_use]
pub fn redact_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    // A fingerprint of a key with at most four codepoints would expose
    // the *entire* credential, contrary to CTRL-PRIV-001.
    if trimmed.chars().count() <= 4 {
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
    fn redact_short_key_never_exposes_entire_credential() {
        for key in ["a", "ab", "abcd", "🙂🙂🙂🙂", "  ab  "] {
            assert_eq!(redact_api_key(key), "OAuth ***");
        }
        assert_eq!(redact_api_key("abcde"), "OAuth ***bcde");
    }

    #[test]
    fn redact_never_contains_full_key() {
        let key = "supersecretstatuspageapikey1234";
        let red = redact_api_key(key);
        assert!(!red.contains("supersecret"));
        assert!(!red.contains("statuspageapikey"));
        assert!(red.ends_with("1234"));
    }

    #[test]
    fn redact_repeated_prefix_character_is_only_in_authorized_tail() {
        // The first `I` is masked; the second is part of the permitted
        // four-character fingerprint. A substring check cannot distinguish
        // them, but exact output equality can.
        assert_eq!(redact_api_key("IIA0A"), "OAuth ***IA0A");
    }
}
