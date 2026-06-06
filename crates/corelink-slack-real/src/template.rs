//! Per-channel structured message templates.
//!
//! Templates own the schema (no untrusted-input string interpolation).
//! Each variant carries a structured payload; rendering produces a
//! [`SlackMessage`] with the appropriate header / fields / footer for
//! the destination channel.

use crate::channel::SlackChannel;
use crate::message::{SlackActionButton, SlackMessage};

/// Canonical structured templates per channel.
///
/// New templates MUST extend this enum (not callers building messages
/// freehand) so the schema audit surface stays bounded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageTemplate {
    /// SEV1 alert payload. Body fields: service, summary, runbook,
    /// pager_link. Action: `ack` button to acknowledge from chat.
    AlertSev1 {
        /// Affected service.
        service: String,
        /// One-line incident summary.
        summary: String,
        /// Runbook URL.
        runbook: String,
        /// PagerDuty incident link.
        pager_link: String,
        /// Footer timestamp (ISO-8601).
        timestamp_iso: String,
    },
    /// SEV2 alert payload. Body fields: service, summary, runbook.
    AlertSev2 {
        /// Affected service.
        service: String,
        /// One-line incident summary.
        summary: String,
        /// Runbook URL.
        runbook: String,
        /// Footer timestamp (ISO-8601).
        timestamp_iso: String,
    },
    /// New enterprise inquiry intake. Body fields: company, role,
    /// expected_gb_per_month, residency. Action: `assign` button.
    NewEnterpriseInquiry {
        /// Sales-leads inquiry id.
        inquiry_id: String,
        /// Submitter company name (PII pre-redacted upstream).
        company: String,
        /// Submitter role.
        role: String,
        /// Expected monthly volume label.
        expected_gb_per_month: String,
        /// Residency requirement label.
        residency: String,
        /// Footer timestamp.
        timestamp_iso: String,
    },
    /// Privacy breach notification — internal heads-up that an external
    /// notification dispatch is in flight.
    BreachNotification {
        /// Breach incident id.
        breach_id: String,
        /// Affected tenant count (no PII, just count).
        affected_tenant_count: u64,
        /// Notification SLA hours remaining.
        sla_hours_remaining: u32,
        /// Footer timestamp.
        timestamp_iso: String,
    },
    /// Oncall handoff (start/end of shift).
    OncallHandoff {
        /// Outgoing engineer handle (`@user`).
        outgoing: String,
        /// Incoming engineer handle (`@user`).
        incoming: String,
        /// Region (`us-east`, `eu-west`, `ap-south`).
        region: String,
        /// Footer timestamp.
        timestamp_iso: String,
    },
    /// Lighthouse customer progression update.
    LighthouseProgression {
        /// Lighthouse customer name (display only — PII redacted).
        customer: String,
        /// Source state.
        from_state: String,
        /// Target state.
        to_state: String,
        /// Footer timestamp.
        timestamp_iso: String,
    },
}

impl MessageTemplate {
    /// Canonical channel this template renders for.
    #[must_use]
    pub const fn channel(&self) -> SlackChannel {
        match self {
            Self::AlertSev1 { .. } => SlackChannel::AlertsSev1,
            Self::AlertSev2 { .. } => SlackChannel::AlertsSev2,
            Self::NewEnterpriseInquiry { .. } => SlackChannel::EnterpriseInquiries,
            Self::BreachNotification { .. } => SlackChannel::BreachNotifications,
            Self::OncallHandoff { .. } => SlackChannel::OncallHandoff,
            Self::LighthouseProgression { .. } => SlackChannel::LighthouseCustomers,
        }
    }

    /// Render to a Block Kit [`SlackMessage`].
    #[must_use]
    pub fn render(&self) -> SlackMessage {
        match self {
            Self::AlertSev1 {
                service,
                summary,
                runbook,
                pager_link,
                timestamp_iso,
            } => SlackMessage::new(
                SlackChannel::AlertsSev1,
                format!("SEV1 — {service}"),
                "corelink-alerting",
                timestamp_iso,
            )
            .with_field("service", service)
            .with_field("summary", summary)
            .with_field("runbook", runbook)
            .with_field("pager", pager_link)
            .with_action(SlackActionButton::new("ack", "Acknowledge").with_style("primary")),

            Self::AlertSev2 {
                service,
                summary,
                runbook,
                timestamp_iso,
            } => SlackMessage::new(
                SlackChannel::AlertsSev2,
                format!("SEV2 — {service}"),
                "corelink-alerting",
                timestamp_iso,
            )
            .with_field("service", service)
            .with_field("summary", summary)
            .with_field("runbook", runbook),

            Self::NewEnterpriseInquiry {
                inquiry_id,
                company,
                role,
                expected_gb_per_month,
                residency,
                timestamp_iso,
            } => SlackMessage::new(
                SlackChannel::EnterpriseInquiries,
                format!("New enterprise inquiry — {company}"),
                format!("inquiry {inquiry_id}"),
                timestamp_iso,
            )
            .with_field("company", company)
            .with_field("role", role)
            .with_field("expected_gb_per_month", expected_gb_per_month)
            .with_field("residency", residency)
            .with_action(SlackActionButton::new("assign", "Assign to me")),

            Self::BreachNotification {
                breach_id,
                affected_tenant_count,
                sla_hours_remaining,
                timestamp_iso,
            } => SlackMessage::new(
                SlackChannel::BreachNotifications,
                format!("Breach dispatch — {breach_id}"),
                "corelink-privacy",
                timestamp_iso,
            )
            .with_field("breach_id", breach_id)
            .with_field("affected_tenant_count", affected_tenant_count.to_string())
            .with_field("sla_hours_remaining", sla_hours_remaining.to_string()),

            Self::OncallHandoff {
                outgoing,
                incoming,
                region,
                timestamp_iso,
            } => SlackMessage::new(
                SlackChannel::OncallHandoff,
                format!("Oncall handoff — {region}"),
                "corelink-oncall",
                timestamp_iso,
            )
            .with_field("outgoing", outgoing)
            .with_field("incoming", incoming)
            .with_field("region", region),

            Self::LighthouseProgression {
                customer,
                from_state,
                to_state,
                timestamp_iso,
            } => SlackMessage::new(
                SlackChannel::LighthouseCustomers,
                format!("Lighthouse — {customer}"),
                "corelink-lighthouse",
                timestamp_iso,
            )
            .with_field("customer", customer)
            .with_field("from", from_state)
            .with_field("to", to_state),
        }
    }
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
    fn alert_sev1_renders_with_action() {
        let t = MessageTemplate::AlertSev1 {
            service: "edge".into(),
            summary: "5xx spike".into(),
            runbook: "https://runbook/edge".into(),
            pager_link: "https://pd/123".into(),
            timestamp_iso: "2026-05-14T00:00:00Z".into(),
        };
        assert_eq!(t.channel(), SlackChannel::AlertsSev1);
        let m = t.render();
        assert!(m.header.starts_with("SEV1"));
        assert_eq!(m.actions.len(), 1);
        assert_eq!(m.actions[0].action_id, "ack");
    }

    #[test]
    fn enterprise_inquiry_renders_company() {
        let t = MessageTemplate::NewEnterpriseInquiry {
            inquiry_id: "inq-1".into(),
            company: "AcmeCorp".into(),
            role: "CTO".into(),
            expected_gb_per_month: "500".into(),
            residency: "eu-west".into(),
            timestamp_iso: "2026-05-14T00:00:00Z".into(),
        };
        assert_eq!(t.channel(), SlackChannel::EnterpriseInquiries);
        let m = t.render();
        assert!(m.header.contains("AcmeCorp"));
        assert!(m.fields.iter().any(|f| f.key == "company"));
    }
}
