//! [`AlerterConfig`] — multi-channel alerter configuration.

/// Configuration for [`MultiChannelAlerter`].
///
/// [`MultiChannelAlerter`]: super::alerter::MultiChannelAlerter
///
/// # Example
///
/// ```rust
/// use corelink_ops::alerts::AlerterConfig;
///
/// let config = AlerterConfig::default();
/// assert!(!config.email_enabled || config.sendgrid_api_key.is_none());
/// ```
#[derive(Debug, Clone)]
pub struct AlerterConfig {
    /// Enable dashboard alert channel (D1 INSERT + WebSocket fanout).
    pub dashboard_enabled: bool,

    /// Enable email alert channel (SendGrid / SES).
    pub email_enabled: bool,

    /// SendGrid API key (None = use SES fallback or stub in CI).
    pub sendgrid_api_key: Option<String>,

    /// Enable in-app notification channel (D1 + WebSocket).
    pub in_app_enabled: bool,

    /// Enable Slack webhook channel (optional; customer-configured).
    pub slack_enabled: bool,

    /// Slack webhook URL (customer-supplied; None = disabled).
    pub slack_webhook_url: Option<String>,

    /// Whether to run in test/stub mode (all channels succeed silently).
    pub stub_mode: bool,
}

impl Default for AlerterConfig {
    fn default() -> Self {
        Self {
            dashboard_enabled: true,
            email_enabled: true,
            sendgrid_api_key: None,
            in_app_enabled: true,
            slack_enabled: false,
            slack_webhook_url: None,
            stub_mode: true, // safe default for CI; production sets false
        }
    }
}
