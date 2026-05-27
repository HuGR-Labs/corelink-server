//! Secret-redaction helpers. The `DRATA_API_KEY` is NEVER logged
//! plaintext — operators must be able to copy/paste audit chain entries
//! without leaking the production token.

/// Redact a Drata Bearer-token key to `drata-***<last4>`.
///
/// - For keys ≥ 4 characters the last 4 characters are preserved so an
///   operator can disambiguate two rotated keys.
/// - For shorter strings the full body is masked.
#[must_use]
pub fn redact_api_key(raw: &str) -> String {
    if raw.is_empty() {
        return "drata-***".to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= 4 {
        return "drata-***".to_string();
    }
    let start = chars.len() - 4;
    let tail: String = chars
        .get(start..)
        .map(|s| s.iter().collect())
        .unwrap_or_default();
    format!("drata-***{tail}")
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
    fn redact_short_key() {
        assert_eq!(redact_api_key(""), "drata-***");
        assert_eq!(redact_api_key("abc"), "drata-***");
        assert_eq!(redact_api_key("abcd"), "drata-***");
    }

    #[test]
    fn redact_normal_key() {
        let raw = "drata_live_sk_1234567890abcdef";
        let r = redact_api_key(raw);
        assert!(r.starts_with("drata-***"));
        assert!(r.ends_with("cdef"));
        // The full secret body is gone.
        assert!(!r.contains("1234567890"));
    }
}
