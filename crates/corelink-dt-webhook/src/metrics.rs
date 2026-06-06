//! Prometheus metric definitions for the DT webhook handler (WI-S12-005).
//!
//! All metrics are prefixed `corelink_supply_dt_`.
//!
//! # Metric catalogue (6 canonical)
//!
//! | Metric | Type | Labels |
//! |--------|------|--------|
//! | [`CVE_ALERTS_TOTAL`] | Counter | `severity`, `channel` |
//! | [`ALERT_DELIVERY_DURATION_SECONDS`] | Histogram | — |
//! | [`WEBHOOK_TOTAL`] | Counter | `outcome` |
//! | [`DLQ_SIZE_GAUGE`] | Gauge | — |
//! | [`UPTIME_RATIO_GAUGE`] | Gauge | — |
//! | [`MOCK_INJECTION_TOTAL`] | Counter | `outcome` |
//!
//! These are string constants suitable for emission in text-format Prometheus
//! output (CF Worker analytics or a Prometheus push-gateway). The actual
//! numeric counters are maintained by the [`crate::handler::InMemoryDtWebhookHandler`].

/// Counter metric name for CVE alerts delivered per severity and channel.
///
/// Labels: `severity` ∈ `critical|high|medium|low|info`,
///         `channel` ∈ `slack|email|pagerduty`.
pub const CVE_ALERTS_TOTAL: &str = "corelink_supply_dt_cve_alerts_total";

/// Histogram metric name for alert delivery duration from CVE detection.
///
/// Buckets (seconds): 30, 60, 120, 300, 600, 900 (= 15 min SLA boundary).
pub const ALERT_DELIVERY_DURATION_SECONDS: &str =
    "corelink_supply_dt_alert_delivery_duration_seconds";

/// Counter metric name for webhook requests by outcome.
///
/// Labels: `outcome` ∈ `ok|hmac_invalid|slack_failed|email_failed|pagerduty_failed|parse_error`.
pub const WEBHOOK_TOTAL: &str = "corelink_supply_dt_webhook_total";

/// Gauge metric name for current DLQ size (target = 0).
pub const DLQ_SIZE_GAUGE: &str = "corelink_supply_dt_dlq_size_gauge";

/// Gauge metric name for DT instance availability ratio per 5-min interval.
///
/// Target ≥ 0.999 (99.9% sustained 30d).
pub const UPTIME_RATIO_GAUGE: &str = "corelink_supply_dt_uptime_ratio_gauge";

/// Counter metric name for mock CVE injection attempts.
///
/// Labels: `outcome` ∈ `ok|sla_violation|alert_missing`.
pub const MOCK_INJECTION_TOTAL: &str = "corelink_supply_dt_mock_injection_total";

/// SLA budget in milliseconds (15 minutes).
pub const SLA_BUDGET_MS: u64 = 900_000;

/// DLQ capacity cap — SEV-2 alert fires when exceeded.
pub const DLQ_CAP: usize = 1_000;

/// Prometheus histogram buckets for `corelink_supply_dt_alert_delivery_duration_seconds`.
pub const ALERT_DELIVERY_BUCKETS_SECONDS: &[f64] = &[30.0, 60.0, 120.0, 300.0, 600.0, 900.0];

/// In-memory metric snapshot produced by the handler on each call.
///
/// Production integrations can serialise this to Prometheus text format or
/// push to a gateway. In tests it is used to assert correct metric emissions.
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    /// Incremented per (severity, channel) pair on successful alert delivery.
    pub cve_alerts_by_severity_channel: Vec<(String, String)>,
    /// Delivery latency in ms recorded for the histogram.
    pub delivery_latency_ms: Option<u64>,
    /// Webhook outcome label.
    pub webhook_outcome: Option<String>,
    /// Current DLQ size.
    pub dlq_size: usize,
    /// Mock injection outcome label (if applicable).
    pub mock_injection_outcome: Option<String>,
}
