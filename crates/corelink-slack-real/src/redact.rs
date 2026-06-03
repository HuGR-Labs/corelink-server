//! Webhook URL redaction (CTRL-PRIV-001).
//!
//! Slack incoming webhook URLs have the canonical shape
//! `https://hooks.slack.com/services/T0000/B0000/<secret>`. The secret
//! segment is the bearer credential — leaking it grants posting access
//! to the channel. We log only the team + bot-id prefix and a `***`
//! placeholder for the secret.

/// Redact a Slack incoming webhook URL for safe logging.
///
/// Maps `https://hooks.slack.com/services/T0000/B0000/XYZ` →
/// `https://hooks.slack.com/services/T0000/B0000/***`. Returns
/// `"<redacted>"` when the input does not match the expected shape.
#[must_use]
pub fn redact_webhook(url: &str) -> String {
    let prefix = "https://hooks.slack.com/services/";
    let Some(rest) = url.strip_prefix(prefix) else {
        return "<redacted>".to_string();
    };
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 3 {
        return "<redacted>".to_string();
    }
    let team = parts.first().copied().unwrap_or("****");
    let bot = parts.get(1).copied().unwrap_or("****");
    format!("{prefix}{team}/{bot}/***")
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
    fn redacts_secret_segment() {
        let s = redact_webhook("https://hooks.slack.com/services/T00ABCD/B0EFGHI/longsecret");
        assert_eq!(s, "https://hooks.slack.com/services/T00ABCD/B0EFGHI/***");
    }

    #[test]
    fn non_slack_url_redacted_opaquely() {
        assert_eq!(redact_webhook("https://example.com/abc"), "<redacted>");
    }

    #[test]
    fn truncated_url_redacted_opaquely() {
        assert_eq!(
            redact_webhook("https://hooks.slack.com/services/T0000"),
            "<redacted>"
        );
    }
}
