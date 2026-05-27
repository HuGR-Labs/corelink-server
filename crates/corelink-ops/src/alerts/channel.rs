//! Alert channel result types and helpers.

/// Outcome of a single alert channel dispatch.
#[derive(Debug, Clone)]
pub enum ChannelOutcome {
    /// Channel succeeded.
    Success(String),
    /// Channel failed; message describes the failure.
    Failed(String),
    /// Channel disabled in config.
    Disabled,
}

impl ChannelOutcome {
    /// Return true if this channel succeeded.
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

/// Summary of all channel outcomes for a single alert dispatch.
#[derive(Debug, Clone)]
pub struct AlertDispatchSummary {
    /// Dashboard channel outcome.
    pub dashboard: ChannelOutcome,
    /// Email channel outcome.
    pub email: ChannelOutcome,
    /// In-app notification channel outcome.
    pub in_app: ChannelOutcome,
    /// Slack webhook channel outcome.
    pub slack: ChannelOutcome,
}

impl AlertDispatchSummary {
    /// Return true if at least one channel succeeded.
    #[must_use]
    pub fn any_success(&self) -> bool {
        self.dashboard.is_success()
            || self.email.is_success()
            || self.in_app.is_success()
            || self.slack.is_success()
    }

    /// Return a comma-separated list of succeeded channels.
    #[must_use]
    pub fn succeeded_channels(&self) -> String {
        let mut channels = Vec::new();
        if self.dashboard.is_success() {
            channels.push("dashboard");
        }
        if self.email.is_success() {
            channels.push("email");
        }
        if self.in_app.is_success() {
            channels.push("in_app");
        }
        if self.slack.is_success() {
            channels.push("slack");
        }
        channels.join(",")
    }
}
