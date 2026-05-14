//! `corelink-dt-webhook` — Dependency-Track webhook handler + CVE alert routing
//! (WI-S12-005, HIGH_RISK lane FF-HR-005).
//!
//! # What this crate ships
//!
//! - [`DtWebhookHandler`] trait: HMAC-verified webhook receive + severity
//!   classification + alert routing to Slack/Email/PagerDuty within the
//!   **≤ 15 min p99** SLA (SLO-SUPPLY-CVE-DETECTION).
//! - [`types`] module: all domain types (`DtWebhookEvent`, `DtSeverity`,
//!   `AlertDelivered`, `AlertChannel`, `DtWebhookError`, `ComponentMetadata`,
//!   `VulnerabilityMetadata`, `DtEventType`, `DtProjectUuid`, `SyntheticCve`,
//!   `MockInjected`).
//! - [`hmac`] module: HMAC-SHA256 `X-Hub-Signature-256` verification using
//!   constant-time comparison (prevents timing attacks; `subtle` crate).
//! - [`severity`] module: CVSS-to-`DtSeverity` classification + routing rules.
//! - [`handler`] module: `InMemoryDtWebhookHandler` — in-memory orchestrator
//!   wiring every invariant (for tests and staging). Production CF Worker wiring
//!   is deferred per `trait-abstraction-defer` charter pattern.
//! - [`dlq`] module: dead-letter queue (in-memory; production uses CF KV).
//! - [`metrics`] module: 6 Prometheus metric definitions (counter/histogram/gauge).
//!
//! # CVE alert routing rules
//!
//! | Severity  | CVSS range | Channels                          |
//! |-----------|------------|-----------------------------------|
//! | Critical  | 9.0–10.0   | Slack + Email + PagerDuty SEV-2   |
//! | High      | 7.0–8.9    | Slack + Email                     |
//! | Medium    | 4.0–6.9    | Slack only                        |
//! | Low       | 0.1–3.9    | Log only                          |
//! | Info      | 0.0        | Log only                          |
//!
//! # Invariants enforced
//!
//! - `HMAC_REQUIRED`: every incoming webhook is rejected (HTTP 401) if
//!   `X-Hub-Signature-256` is missing or invalid.
//! - `DLQ_BOUNDED`: DLQ capped at 1000 events; SEV-2 alert if exceeded.
//! - `SLA_15MIN`: alert delivery latency measured; `SlaViolation` error
//!   emitted if delivery exceeds 900 000 ms (15 min).
//! - `PATCHED_LOCALLY_SUPPRESSED`: events with `cdx:patched_locally=true`
//!   annotation + valid ADR ref (within 90-day sunset) suppress the alert
//!   and emit a structured log entry.
//!
//! # Prometheus metrics (6 canonical)
//!
//! All metrics are prefixed `corelink_supply_dt_`.
//!
//! | Metric | Type | Labels |
//! |--------|------|--------|
//! | `cve_alerts_total` | Counter | `severity`, `channel` |
//! | `alert_delivery_duration_seconds` | Histogram | — (p50/p95/p99) |
//! | `webhook_total` | Counter | `outcome` |
//! | `dlq_size_gauge` | Gauge | — |
//! | `uptime_ratio_gauge` | Gauge | — |
//! | `mock_injection_total` | Counter | `outcome` |

#![forbid(unsafe_code)]

pub mod dlq;
pub mod handler;
pub mod hmac;
pub mod metrics;
pub mod severity;
pub mod types;

pub use handler::InMemoryDtWebhookHandler;
pub use types::{
    AlertChannel, AlertDelivered, ComponentMetadata, DtEventType, DtProjectUuid, DtSeverity,
    DtWebhookError, DtWebhookEvent, MockInjected, SyntheticCve, VulnerabilityMetadata,
};

use async_trait::async_trait;

/// Core webhook handler contract.
///
/// Implementations must:
/// 1. Verify the `X-Hub-Signature-256` HMAC before processing any event.
/// 2. Classify severity from CVSS score.
/// 3. Route to the appropriate alert channels within the 15-min SLA.
/// 4. Emit `corelink_supply_dt_webhook_total` + per-channel
///    `corelink_supply_dt_cve_alerts_total` metrics on every call.
///
/// The `inject_mock_cve` method is **gated to test/staging only** via the
/// `DT_MOCK_INJECTION_ENABLED` environment flag; production implementations
/// MUST return `DtWebhookError::MockInjectionDisabled` if the flag is absent.
#[async_trait]
pub trait DtWebhookHandler: Send + Sync {
    /// Receive a DT webhook event; classify severity; route to Slack/Email/PagerDuty.
    ///
    /// SLA: ≤ 15 min p99 from CVE detection to alert delivery.
    ///
    /// Returns [`DtWebhookError::HmacInvalid`] (→ HTTP 401) if HMAC check fails.
    /// Returns [`DtWebhookError::SlaViolation`] (logged; not surfaced as HTTP 5xx)
    /// if delivery latency exceeds 900 000 ms.
    async fn handle_webhook(
        &self,
        event: DtWebhookEvent,
    ) -> Result<AlertDelivered, DtWebhookError>;

    /// Mock CVE injection (test/staging only).
    ///
    /// Synthesises a `CVE-YYYY-9999` entry in the DT staging vulnerability DB
    /// and verifies that the E2E alert path fires within the 15-min SLA.
    ///
    /// # Errors
    ///
    /// Returns [`DtWebhookError::MockInjectionDisabled`] if the environment flag
    /// `DT_MOCK_INJECTION_ENABLED=true` is not set.
    async fn inject_mock_cve(
        &self,
        project_uuid: DtProjectUuid,
        synthetic_cve: SyntheticCve,
    ) -> Result<MockInjected, DtWebhookError>;
}
