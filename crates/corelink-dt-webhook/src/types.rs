//! Domain types for the Dependency-Track webhook handler (WI-S12-005).
//!
//! All types derive [`Debug`], [`Clone`], and [`serde`] traits where applicable.
//! No `unwrap` / `expect` / `panic` in any `impl` block — all fallible conversions
//! return `Result`.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Newtype wrappers
// ---------------------------------------------------------------------------

/// Opaque project UUID from Dependency-Track.
///
/// Validated on construction: must be a non-empty, non-whitespace ASCII string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DtProjectUuid(String);

impl DtProjectUuid {
    /// Create a new `DtProjectUuid`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the string is empty or contains only whitespace.
    pub fn new(s: impl Into<String>) -> Result<Self, DtWebhookError> {
        let s = s.into();
        if s.trim().is_empty() {
            return Err(DtWebhookError::InvalidProjectUuid(s));
        }
        Ok(Self(s))
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DtProjectUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Event taxonomy
// ---------------------------------------------------------------------------

/// Dependency-Track webhook event types (canonical DT v4.11 event taxonomy).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum DtEventType {
    /// A new vulnerability matching a tracked component was found.
    NewVulnerability,
    /// A component that is now part of an SBOM is vulnerable.
    NewVulnerableDependency,
    /// An SBOM BOM was successfully processed by DT.
    BomProcessed,
    /// Policy violation detected.
    PolicyViolation,
    /// Project audit changed.
    ProjectAuditChange,
}

// ---------------------------------------------------------------------------
// Component + vulnerability metadata
// ---------------------------------------------------------------------------

/// Metadata about the affected component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMetadata {
    /// Package URL (PURL) — e.g. `pkg:cargo/ring@0.16.0`.
    pub purl: String,
    /// Component name.
    pub name: String,
    /// Component version string.
    pub version: String,
    /// Whether the component is patched locally via `[patch.crates-io]`.
    /// When `true` and `patched_locally_adr` is a valid, non-expired ADR ref,
    /// the alert is suppressed.
    #[serde(default)]
    pub patched_locally: bool,
    /// ADR reference that justifies the local patch (e.g. `"ADR-0042"`).
    /// The 90-day sunset clock starts from the ADR's `ratified_date`.
    #[serde(default)]
    pub patched_locally_adr: Option<String>,
    /// ISO-8601 date when the ADR was ratified (used for 90-day sunset check).
    #[serde(default)]
    pub adr_ratified_date: Option<String>,
}

/// Metadata about the detected vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityMetadata {
    /// CVE identifier (e.g. `"CVE-2024-12345"`).
    pub cve_id: String,
    /// CVSS v3.x base score (0.0 – 10.0).
    pub cvss_score: f64,
    /// Textual severity from DT (used as fallback if CVSS unavailable).
    pub severity_label: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// CVE sources that matched (NVD, OSV, GHSA, etc.).
    #[serde(default)]
    pub sources: Vec<String>,
}

// ---------------------------------------------------------------------------
// Webhook event (top-level)
// ---------------------------------------------------------------------------

/// A fully-parsed Dependency-Track webhook event.
///
/// Corresponds to the DT v4.11+ canonical JSON payload. The `timestamp` field
/// is the DT detection time used to measure alert delivery SLA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtWebhookEvent {
    /// Event type discriminant.
    pub event_type: DtEventType,
    /// Affected project UUID.
    pub project_uuid: DtProjectUuid,
    /// Component metadata.
    pub component: ComponentMetadata,
    /// Vulnerability metadata.
    pub vulnerability: VulnerabilityMetadata,
    /// DT detection timestamp (used for SLA measurement).
    #[serde(skip, default = "SystemTime::now")]
    pub timestamp: SystemTime,
}

// ---------------------------------------------------------------------------
// Severity classification
// ---------------------------------------------------------------------------

/// CVE severity classification mapped from CVSS v3.x base score.
///
/// Routing rules:
/// - `Critical` (CVSS 9.0–10.0) → Slack + Email + PagerDuty SEV-2.
/// - `High` (CVSS 7.0–8.9) → Slack + Email.
/// - `Medium` (CVSS 4.0–6.9) → Slack only.
/// - `Low` (CVSS 0.1–3.9) → log only.
/// - `Info` (CVSS 0.0) → log only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DtSeverity {
    /// CVSS 9.0–10.0 — PagerDuty SEV-2 + Slack + Email.
    Critical,
    /// CVSS 7.0–8.9 — Slack + Email.
    High,
    /// CVSS 4.0–6.9 — Slack only.
    Medium,
    /// CVSS 0.1–3.9 — log only.
    Low,
    /// CVSS 0.0 — log only.
    Info,
}

impl std::fmt::Display for DtSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DtSeverity::Critical => f.write_str("critical"),
            DtSeverity::High => f.write_str("high"),
            DtSeverity::Medium => f.write_str("medium"),
            DtSeverity::Low => f.write_str("low"),
            DtSeverity::Info => f.write_str("info"),
        }
    }
}

// ---------------------------------------------------------------------------
// Alert channels
// ---------------------------------------------------------------------------

/// Alert delivery channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertChannel {
    /// Slack `#supply-chain-cve-alerts`.
    Slack,
    /// Email to `security@corelink.dev`.
    Email,
    /// PagerDuty SEV-2 incident trigger.
    PagerDuty,
}

impl std::fmt::Display for AlertChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertChannel::Slack => f.write_str("slack"),
            AlertChannel::Email => f.write_str("email"),
            AlertChannel::PagerDuty => f.write_str("pagerduty"),
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Successful alert delivery receipt.
#[derive(Debug, Clone)]
pub struct AlertDelivered {
    /// Channels to which the alert was successfully delivered.
    pub channels: Vec<AlertChannel>,
    /// Measured latency from DT detection timestamp to delivery completion (ms).
    pub delivery_latency_ms: u64,
    /// CVE identifier.
    pub cve_id: String,
    /// Classified severity.
    pub severity: DtSeverity,
}

/// Mock CVE injection result (test/staging only).
#[derive(Debug, Clone)]
pub struct MockInjected {
    /// Project UUID the synthetic CVE was injected into.
    pub project_uuid: DtProjectUuid,
    /// Synthetic CVE identifier used (e.g. `"CVE-2026-9999"`).
    pub synthetic_cve_id: String,
    /// Whether the E2E alert path fired within the 15-min SLA.
    pub sla_met: bool,
    /// Measured E2E latency in ms.
    pub e2e_latency_ms: u64,
}

/// Synthetic CVE specification for mock injection.
#[derive(Debug, Clone)]
pub struct SyntheticCve {
    /// CVE identifier to synthesise (must match pattern `CVE-YYYY-9NNN`).
    pub cve_id: String,
    /// CVSS score to use for the synthetic CVE.
    pub cvss_score: f64,
    /// Description text (optional; for audit trail).
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by [`crate::DtWebhookHandler`] implementations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DtWebhookError {
    /// DT API was unreachable (e.g. 503 sustained).
    #[error("DT API unreachable: {0}")]
    DtApiUnreachable(String),

    /// Slack webhook delivery failed.
    #[error("Slack webhook failed: HTTP {status}")]
    SlackFailed {
        /// HTTP status code returned by Slack.
        status: u16,
    },

    /// Email delivery failed.
    #[error("Email send failed: {0}")]
    EmailFailed(String),

    /// PagerDuty trigger failed.
    #[error("PagerDuty trigger failed: HTTP {status}")]
    PagerDutyFailed {
        /// HTTP status code returned by PagerDuty.
        status: u16,
    },

    /// HMAC `X-Hub-Signature-256` verification failed (→ HTTP 401).
    #[error("HMAC signature verification failed")]
    HmacInvalid,

    /// Alert delivery latency exceeded the 15-min SLA budget.
    ///
    /// This error is logged but NOT surfaced as HTTP 5xx to the caller.
    #[error("alert SLA violated: latency {actual_ms}ms > 15 min budget (900000ms)")]
    SlaViolation {
        /// Actual measured latency in ms.
        actual_ms: u64,
    },

    /// DLQ exceeded the 1000-event cap (→ SEV-2 alert).
    #[error("DLQ capacity exceeded: {size} events > 1000 cap")]
    DlqCapacityExceeded {
        /// Current DLQ size at the time of the overflow.
        size: usize,
    },

    /// JSON deserialisation of the webhook payload failed.
    #[error("webhook payload parse error: {0}")]
    ParseError(String),

    /// Project UUID was invalid (empty / whitespace).
    #[error("invalid project UUID: {0:?}")]
    InvalidProjectUuid(String),

    /// Mock CVE injection is disabled (not in test/staging).
    #[error("mock CVE injection disabled (DT_MOCK_INJECTION_ENABLED not set)")]
    MockInjectionDisabled,

    /// Event was suppressed because the component is patched locally.
    ///
    /// This is a non-error outcome; it is returned as `Err` so that callers
    /// can distinguish it from a successful alert delivery.
    #[error(
        "CVE alert suppressed: component patched locally per ADR {adr_ref:?} (cdx:patched_locally=true)"
    )]
    PatchedLocallySuppressed {
        /// CVE identifier.
        cve_id: String,
        /// ADR reference that justifies the suppression.
        adr_ref: String,
    },
}
