//! Slack channel routing + per-channel webhook URL registry.

use std::collections::BTreeMap;

use thiserror::Error;

/// Canonical Slack channel routing target.
///
/// Each variant maps to a distinct Slack incoming webhook URL so that
/// routing isolation is preserved (a leaked webhook for the lighthouse
/// channel cannot post into the SEV1 alert channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SlackChannel {
    /// SEV1 production-impacting alerts (paging on-call).
    AlertsSev1,
    /// SEV2 degraded-service alerts (no-page; eyeballs).
    AlertsSev2,
    /// New enterprise inquiry intake (`#sales-leads`).
    EnterpriseInquiries,
    /// Privacy breach notification dispatch (S-13 / S-19 lane).
    BreachNotifications,
    /// Oncall handoff (start/end of shift) notifications.
    OncallHandoff,
    /// Lighthouse customer progression updates (S-20).
    LighthouseCustomers,
}

impl SlackChannel {
    /// Canonical wire string (lowercase snake).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlertsSev1 => "alerts_sev1",
            Self::AlertsSev2 => "alerts_sev2",
            Self::EnterpriseInquiries => "enterprise_inquiries",
            Self::BreachNotifications => "breach_notifications",
            Self::OncallHandoff => "oncall_handoff",
            Self::LighthouseCustomers => "lighthouse_customers",
        }
    }

    /// Env var name for this channel's webhook URL.
    #[must_use]
    pub const fn env_var(self) -> &'static str {
        match self {
            Self::AlertsSev1 => "SLACK_WEBHOOK_URL_ALERTS_SEV1",
            Self::AlertsSev2 => "SLACK_WEBHOOK_URL_ALERTS_SEV2",
            Self::EnterpriseInquiries => "SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES",
            Self::BreachNotifications => "SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS",
            Self::OncallHandoff => "SLACK_WEBHOOK_URL_ONCALL_HANDOFF",
            Self::LighthouseCustomers => "SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS",
        }
    }

    /// Iterate the full canonical channel set.
    #[must_use]
    pub fn all() -> [Self; 6] {
        [
            Self::AlertsSev1,
            Self::AlertsSev2,
            Self::EnterpriseInquiries,
            Self::BreachNotifications,
            Self::OncallHandoff,
            Self::LighthouseCustomers,
        ]
    }
}

/// Webhook URL registry — maps each [`SlackChannel`] to its incoming
/// webhook URL.
///
/// Per-channel URLs are loaded from the canonical environment variables
/// (see [`SlackChannel::env_var`]). Missing channels do not error at
/// load time — they raise [`WebhookRegistryError::ChannelUnconfigured`]
/// when a caller attempts to dispatch to them.
#[derive(Clone, Debug, Default)]
pub struct WebhookRegistry {
    urls: BTreeMap<SlackChannel, String>,
}

impl WebhookRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) the webhook URL for a channel.
    pub fn insert(&mut self, channel: SlackChannel, url: impl Into<String>) {
        self.urls.insert(channel, url.into());
    }

    /// Look up the webhook URL for a channel.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRegistryError::ChannelUnconfigured`] when the
    /// channel has no configured webhook URL.
    pub fn url(&self, channel: SlackChannel) -> Result<&str, WebhookRegistryError> {
        self.urls
            .get(&channel)
            .map(String::as_str)
            .ok_or(WebhookRegistryError::ChannelUnconfigured { channel })
    }

    /// True if a channel has a configured webhook.
    #[must_use]
    pub fn has(&self, channel: SlackChannel) -> bool {
        self.urls.contains_key(&channel)
    }

    /// Number of configured channels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.urls.len()
    }

    /// True if no channels are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }

    /// Load the registry from process environment via the canonical
    /// `SLACK_WEBHOOK_URL_<CHANNEL>` env vars.
    ///
    /// Channels with no env var set are simply omitted from the
    /// registry; the caller learns about missing channels lazily when
    /// they attempt to dispatch (preferred over startup-time hard
    /// failure so that partial deploys remain usable).
    #[must_use]
    pub fn from_env() -> Self {
        let mut r = Self::new();
        for ch in SlackChannel::all() {
            if let Ok(url) = std::env::var(ch.env_var()) {
                if !url.is_empty() {
                    r.insert(ch, url);
                }
            }
        }
        r
    }
}

/// Errors raised by [`WebhookRegistry`].
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebhookRegistryError {
    /// Caller asked for a channel that was never configured.
    #[error("slack channel {channel:?} has no configured webhook URL (env var {})", channel.env_var())]
    ChannelUnconfigured {
        /// Channel that was requested.
        channel: SlackChannel,
    },
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
    fn channel_canonical_strings() {
        assert_eq!(SlackChannel::AlertsSev1.as_str(), "alerts_sev1");
        assert_eq!(SlackChannel::AlertsSev2.as_str(), "alerts_sev2");
        assert_eq!(
            SlackChannel::EnterpriseInquiries.as_str(),
            "enterprise_inquiries"
        );
        assert_eq!(
            SlackChannel::BreachNotifications.as_str(),
            "breach_notifications"
        );
        assert_eq!(SlackChannel::OncallHandoff.as_str(), "oncall_handoff");
        assert_eq!(
            SlackChannel::LighthouseCustomers.as_str(),
            "lighthouse_customers",
        );
    }

    #[test]
    fn env_var_naming() {
        assert_eq!(
            SlackChannel::AlertsSev1.env_var(),
            "SLACK_WEBHOOK_URL_ALERTS_SEV1"
        );
        assert_eq!(
            SlackChannel::BreachNotifications.env_var(),
            "SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS"
        );
    }

    #[test]
    fn registry_insert_and_lookup() {
        let mut r = WebhookRegistry::new();
        r.insert(SlackChannel::AlertsSev1, "https://hooks.slack.com/a");
        assert!(r.has(SlackChannel::AlertsSev1));
        assert_eq!(r.url(SlackChannel::AlertsSev1).unwrap(), "https://hooks.slack.com/a");
        let err = r.url(SlackChannel::OncallHandoff).unwrap_err();
        assert!(matches!(
            err,
            WebhookRegistryError::ChannelUnconfigured { channel: SlackChannel::OncallHandoff }
        ));
    }

    #[test]
    fn registry_routing_isolation() {
        // Distinct channels MUST resolve to distinct URLs (no
        // accidental sharing).
        let mut r = WebhookRegistry::new();
        r.insert(SlackChannel::AlertsSev1, "https://hooks.slack.com/sev1");
        r.insert(SlackChannel::AlertsSev2, "https://hooks.slack.com/sev2");
        assert_ne!(
            r.url(SlackChannel::AlertsSev1).unwrap(),
            r.url(SlackChannel::AlertsSev2).unwrap()
        );
    }

    #[test]
    fn all_channels_returns_six_distinct() {
        let all = SlackChannel::all();
        assert_eq!(all.len(), 6);
        let mut set: Vec<_> = all.iter().map(|c| c.as_str()).collect();
        set.sort_unstable();
        set.dedup();
        assert_eq!(set.len(), 6);
    }
}
