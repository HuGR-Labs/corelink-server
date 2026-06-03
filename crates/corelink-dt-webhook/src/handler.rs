//! In-memory `DtWebhookHandler` implementation.
//!
//! [`InMemoryDtWebhookHandler`] wires together HMAC verification, severity
//! classification, patch-locally suppression, alert routing, SLA measurement,
//! DLQ management, and metric emission.
//!
//! This implementation is designed for unit/integration tests and staging.
//! Production CF Worker wiring is deferred per the `trait-abstraction-defer`
//! charter pattern — the handler trait is the stable contract.
//!
//! # F-001 closure
//!
//! All mutable state is held behind `Arc<Mutex<>>` so the handler can be
//! cloned and shared across async tasks without `unsafe`.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::{
    dlq::{DlqEntry, InMemoryDlq},
    metrics::{MetricsSnapshot, SLA_BUDGET_MS},
    severity::{classify_cvss, routing_channels},
    types::{
        AlertChannel, AlertDelivered, ComponentMetadata, DtEventType, DtProjectUuid, DtSeverity,
        DtWebhookError, DtWebhookEvent, MockInjected, SyntheticCve, VulnerabilityMetadata,
    },
    DtWebhookHandler,
};

/// ADR validity window: 90 days (used by the chrono_mini sunset check).
#[allow(dead_code)]
const ADR_SUNSET_SECS: u64 = 90 * 24 * 3600;

/// In-memory implementation of [`DtWebhookHandler`].
///
/// Suitable for tests and staging environments. In production, replace the
/// alert sinks with real HTTP calls to Slack/Email/PagerDuty APIs.
#[derive(Debug, Clone)]
pub struct InMemoryDtWebhookHandler {
    inner: Arc<Mutex<HandlerState>>,
}

#[derive(Debug)]
struct HandlerState {
    /// HMAC shared secret (from `DT_WEBHOOK_SECRET` env var in production).
    webhook_secret: Vec<u8>,
    /// HMAC signature for the current event, if provided (set before each call).
    /// In real CF Worker this comes from the `X-Hub-Signature-256` header.
    current_signature: Option<String>,
    /// Recorded alert deliveries (used by tests to assert routing).
    deliveries: Vec<AlertDelivered>,
    /// Dead-letter queue.
    dlq: InMemoryDlq,
    /// Metric snapshot from the last handler invocation.
    last_metrics: MetricsSnapshot,
    /// Whether mock CVE injection is enabled (`DT_MOCK_INJECTION_ENABLED=true`).
    mock_injection_enabled: bool,
}

impl InMemoryDtWebhookHandler {
    /// Create a new handler with the given HMAC secret and mock-injection flag.
    pub fn new(webhook_secret: Vec<u8>, mock_injection_enabled: bool) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HandlerState {
                webhook_secret,
                current_signature: None,
                deliveries: Vec::new(),
                dlq: InMemoryDlq::new(),
                last_metrics: MetricsSnapshot::default(),
                mock_injection_enabled,
            })),
        }
    }

    /// Set the `X-Hub-Signature-256` header value for the *next* call to
    /// [`handle_webhook`](Self::handle_webhook).
    ///
    /// In production this is extracted from the HTTP request header. In tests
    /// it is set explicitly.
    pub fn set_signature(&self, sig: Option<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.current_signature = sig;
        }
    }

    /// Return a clone of all recorded deliveries (for test assertions).
    pub fn deliveries(&self) -> Vec<AlertDelivered> {
        self.inner
            .lock()
            .map(|g| g.deliveries.clone())
            .unwrap_or_default()
    }

    /// Return the last metrics snapshot.
    pub fn last_metrics(&self) -> MetricsSnapshot {
        self.inner
            .lock()
            .map(|g| g.last_metrics.clone())
            .unwrap_or_default()
    }

    /// Return a reference to the DLQ for test inspection.
    pub fn dlq(&self) -> InMemoryDlq {
        self.inner.lock().map(|g| g.dlq.clone()).unwrap_or_default()
    }

    /// Check whether a component's patch-locally annotation suppresses alerting.
    ///
    /// Suppression requires BOTH:
    /// 1. `component.patched_locally == true`.
    /// 2. A non-empty ADR ref that has not exceeded the 90-day sunset.
    fn check_patch_suppression(
        event: &DtWebhookEvent,
    ) -> Option<Result<AlertDelivered, DtWebhookError>> {
        let comp = &event.component;
        if !comp.patched_locally {
            return None;
        }
        let adr_ref = match comp.patched_locally_adr.as_deref() {
            Some(r) if !r.is_empty() => r,
            _ => {
                // patched_locally=true but no ADR ref → do NOT suppress (safety-first).
                return None;
            }
        };

        // Check 90-day sunset if ratified_date is present.
        if let Some(ratified) = &comp.adr_ratified_date {
            if let Ok(parsed) = ratified.parse::<chrono_mini::NaiveDate>() {
                let today = chrono_mini::today();
                let days = today.days_since(parsed);
                if days > 90 {
                    // ADR expired — do NOT suppress; alert normally.
                    warn!(
                        cve_id = %event.vulnerability.cve_id,
                        adr_ref,
                        days_since_ratified = days,
                        "ADR 90-day sunset expired; NOT suppressing alert"
                    );
                    return None;
                }
            }
        }

        info!(
            cve_id = %event.vulnerability.cve_id,
            adr_ref,
            "Suppressed CVE alert (patched locally per ADR cdx:patched_locally=true)"
        );
        Some(Err(DtWebhookError::PatchedLocallySuppressed {
            cve_id: event.vulnerability.cve_id.clone(),
            adr_ref: adr_ref.to_owned(),
        }))
    }

    /// Measure elapsed ms from the event's DT detection timestamp to now.
    fn elapsed_ms(event: &DtWebhookEvent) -> u64 {
        event
            .timestamp
            .elapsed()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Simulate delivery to a channel (in-memory; production uses real HTTP).
    ///
    /// Returns `Ok(())` always in this implementation — production
    /// implementations should propagate HTTP errors.
    fn deliver_to_channel(
        channel: &AlertChannel,
        severity: &DtSeverity,
        cve_id: &str,
    ) -> Result<(), DtWebhookError> {
        info!(
            channel = %channel,
            severity = %severity,
            cve_id,
            "Alert delivered (in-memory simulation)"
        );
        Ok(())
    }
}

#[async_trait]
impl DtWebhookHandler for InMemoryDtWebhookHandler {
    async fn handle_webhook(
        &self,
        event: DtWebhookEvent,
    ) -> Result<AlertDelivered, DtWebhookError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DtWebhookError::DtApiUnreachable("mutex poisoned".into()))?;

        // ── 1. HMAC verification ──────────────────────────────────────────────
        let body_bytes =
            serde_json::to_vec(&event).map_err(|e| DtWebhookError::ParseError(e.to_string()))?;

        match &guard.current_signature.clone() {
            Some(sig) => {
                crate::hmac::verify_signature(&guard.webhook_secret, &body_bytes, sig).map_err(
                    |_| {
                        guard.last_metrics.webhook_outcome = Some("hmac_invalid".into());
                        DtWebhookError::HmacInvalid
                    },
                )?;
            }
            None => {
                guard.last_metrics.webhook_outcome = Some("hmac_invalid".into());
                return Err(DtWebhookError::HmacInvalid);
            }
        }

        // ── 2. Patch-locally suppression ─────────────────────────────────────
        if let Some(suppressed) = Self::check_patch_suppression(&event) {
            guard.last_metrics.webhook_outcome = Some("ok".into());
            return suppressed;
        }

        // ── 3. Severity classification + routing ──────────────────────────────
        let severity = classify_cvss(event.vulnerability.cvss_score);
        let channels = routing_channels(&severity);

        // ── 4. Alert delivery ─────────────────────────────────────────────────
        let mut delivered_channels = Vec::new();
        for channel in &channels {
            match Self::deliver_to_channel(channel, &severity, &event.vulnerability.cve_id) {
                Ok(()) => {
                    delivered_channels.push(channel.clone());
                    guard
                        .last_metrics
                        .cve_alerts_by_severity_channel
                        .push((severity.to_string(), channel.to_string()));
                }
                Err(e) => {
                    // Push to DLQ on delivery failure (exponential backoff up to 3 attempts).
                    let dlq_entry = DlqEntry {
                        event: event.clone(),
                        attempt_count: 1,
                        last_error: e.to_string(),
                    };
                    // Best-effort DLQ push — log but do not abort.
                    if let Err(dlq_err) = guard.dlq.push(dlq_entry) {
                        warn!(error = %dlq_err, "DLQ push failed (capacity exceeded?)");
                    }
                    warn!(channel = %channel, error = %e, "Alert channel delivery failed");
                }
            }
        }

        // ── 5. SLA measurement ────────────────────────────────────────────────
        let latency_ms = Self::elapsed_ms(&event);
        guard.last_metrics.delivery_latency_ms = Some(latency_ms);
        guard.last_metrics.dlq_size = guard.dlq.len().unwrap_or(0);
        guard.last_metrics.webhook_outcome = Some("ok".into());

        let result = AlertDelivered {
            channels: delivered_channels,
            delivery_latency_ms: latency_ms,
            cve_id: event.vulnerability.cve_id.clone(),
            severity: severity.clone(),
        };
        guard.deliveries.push(result.clone());

        // SLA violation is logged but NOT returned as an error (non-5xx).
        if latency_ms > SLA_BUDGET_MS {
            warn!(
                actual_ms = latency_ms,
                budget_ms = SLA_BUDGET_MS,
                cve_id = %event.vulnerability.cve_id,
                "SLA VIOLATION: alert delivery latency exceeded 15 min budget"
            );
        }

        Ok(result)
    }

    async fn inject_mock_cve(
        &self,
        project_uuid: DtProjectUuid,
        synthetic_cve: SyntheticCve,
    ) -> Result<MockInjected, DtWebhookError> {
        let enabled = self
            .inner
            .lock()
            .map(|g| g.mock_injection_enabled)
            .unwrap_or(false);

        if !enabled {
            return Err(DtWebhookError::MockInjectionDisabled);
        }

        info!(
            project_uuid = %project_uuid,
            cve_id = %synthetic_cve.cve_id,
            cvss_score = synthetic_cve.cvss_score,
            "Mock CVE injection started (staging only)"
        );

        // Synthesise a DtWebhookEvent from the SyntheticCve.
        let event = DtWebhookEvent {
            event_type: DtEventType::NewVulnerability,
            project_uuid: project_uuid.clone(),
            component: ComponentMetadata {
                purl: "pkg:cargo/synthetic-test-crate@0.0.0".into(),
                name: "synthetic-test-crate".into(),
                version: "0.0.0".into(),
                patched_locally: false,
                patched_locally_adr: None,
                adr_ratified_date: None,
            },
            vulnerability: VulnerabilityMetadata {
                cve_id: synthetic_cve.cve_id.clone(),
                cvss_score: synthetic_cve.cvss_score,
                severity_label: None,
                description: synthetic_cve.description.clone(),
                sources: vec!["MOCK".into()],
            },
            timestamp: SystemTime::now(),
        };

        // Generate a valid HMAC for the synthetic event.
        let body_bytes =
            serde_json::to_vec(&event).map_err(|e| DtWebhookError::ParseError(e.to_string()))?;
        let secret = self
            .inner
            .lock()
            .map(|g| g.webhook_secret.clone())
            .unwrap_or_default();
        let sig = crate::hmac::sign(&secret, &body_bytes)?;

        self.set_signature(Some(sig));

        let t0 = std::time::Instant::now();
        let delivery = self.handle_webhook(event).await;
        let e2e_ms = t0.elapsed().as_millis() as u64;

        // Update mock injection metrics.
        if let Ok(mut guard) = self.inner.lock() {
            let outcome = match &delivery {
                Ok(_) if e2e_ms <= SLA_BUDGET_MS => "ok",
                Ok(_) => "sla_violation",
                Err(_) => "alert_missing",
            };
            guard.last_metrics.mock_injection_outcome = Some(outcome.to_owned());
        }

        delivery.map(|_| MockInjected {
            project_uuid,
            synthetic_cve_id: synthetic_cve.cve_id,
            sla_met: e2e_ms <= SLA_BUDGET_MS,
            e2e_latency_ms: e2e_ms,
        })
    }
}

/// Minimal date helper to avoid a heavy chrono dependency in wasm32 contexts.
mod chrono_mini {
    /// A simplistic naive date (year, month, day) for ADR sunset calculation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct NaiveDate {
        year: i32,
        month: u8,
        day: u8,
    }

    impl NaiveDate {
        /// Parse an ISO-8601 date string `"YYYY-MM-DD"`.
        ///
        /// Returns `Err(())` on any parse failure.
        pub fn parse_from_str(s: &str) -> Result<Self, ()> {
            let mut it = s.splitn(3, '-');
            let year = it.next().ok_or(())?.parse::<i32>().map_err(|_| ())?;
            let month = it.next().ok_or(())?.parse::<u8>().map_err(|_| ())?;
            let day = it.next().ok_or(())?.parse::<u8>().map_err(|_| ())?;
            if it.next().is_some() {
                return Err(());
            }
            Ok(Self { year, month, day })
        }

        /// Return approximate days elapsed since `other` (positive if `self > other`).
        pub fn days_since(self, other: NaiveDate) -> i64 {
            // Approximate: ignores leap seconds, uses Julian day number formula.
            let jd = |y: i32, m: u8, d: u8| -> i64 {
                let a = (14 - m as i32) / 12;
                let y2 = y + 4800 - a;
                let m2 = m as i32 + 12 * a - 3;
                d as i64 + (153 * m2 + 2) as i64 / 5 + 365 * y2 as i64 + y2 as i64 / 4
                    - y2 as i64 / 100
                    + y2 as i64 / 400
                    - 32045
            };
            jd(self.year, self.month, self.day) - jd(other.year, other.month, other.day)
        }
    }

    impl std::str::FromStr for NaiveDate {
        type Err = ();
        fn from_str(s: &str) -> Result<Self, ()> {
            NaiveDate::parse_from_str(s)
        }
    }

    /// Return today's date from the system clock (UTC approximation).
    pub fn today() -> NaiveDate {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Simplified: days since epoch → Gregorian calendar.
        let days = secs / 86400;
        // Algorithm from https://www.researchgate.net/publication/316558298
        let z = days as i64 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        NaiveDate {
            year: y as i32,
            month: m as u8,
            day: d as u8,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use crate::{
        hmac::sign,
        types::{ComponentMetadata, DtEventType, VulnerabilityMetadata},
    };
    use std::time::SystemTime;

    fn make_handler() -> InMemoryDtWebhookHandler {
        InMemoryDtWebhookHandler::new(b"test-secret".to_vec(), false)
    }

    fn make_event(cvss: f64) -> DtWebhookEvent {
        DtWebhookEvent {
            event_type: DtEventType::NewVulnerability,
            project_uuid: DtProjectUuid::new("proj-001").expect("valid"),
            component: ComponentMetadata {
                purl: "pkg:cargo/ring@0.16.0".into(),
                name: "ring".into(),
                version: "0.16.0".into(),
                patched_locally: false,
                patched_locally_adr: None,
                adr_ratified_date: None,
            },
            vulnerability: VulnerabilityMetadata {
                cve_id: "CVE-2024-00001".into(),
                cvss_score: cvss,
                severity_label: None,
                description: None,
                sources: vec!["NVD".into()],
            },
            timestamp: SystemTime::now(),
        }
    }

    fn signed_event(handler: &InMemoryDtWebhookHandler, event: &DtWebhookEvent) {
        let body = serde_json::to_vec(event).expect("serialize");
        let sig = sign(b"test-secret", &body).expect("sign");
        handler.set_signature(Some(sig));
    }

    #[tokio::test]
    async fn hmac_missing_rejected() {
        let handler = make_handler();
        let event = make_event(9.5);
        // No signature set → HmacInvalid.
        let result = handler.handle_webhook(event).await;
        assert!(matches!(result, Err(DtWebhookError::HmacInvalid)));
    }

    #[tokio::test]
    async fn valid_critical_event_delivers_three_channels() {
        let handler = make_handler();
        let event = make_event(9.5);
        signed_event(&handler, &event);
        let result = handler.handle_webhook(event).await.expect("ok");
        assert_eq!(result.severity, DtSeverity::Critical);
        assert_eq!(result.channels.len(), 3);
        assert!(result.channels.contains(&AlertChannel::PagerDuty));
    }

    #[tokio::test]
    async fn high_severity_no_pagerduty() {
        let handler = make_handler();
        let event = make_event(7.5);
        signed_event(&handler, &event);
        let result = handler.handle_webhook(event).await.expect("ok");
        assert_eq!(result.severity, DtSeverity::High);
        assert!(!result.channels.contains(&AlertChannel::PagerDuty));
    }

    #[tokio::test]
    async fn medium_severity_slack_only() {
        let handler = make_handler();
        let event = make_event(5.0);
        signed_event(&handler, &event);
        let result = handler.handle_webhook(event).await.expect("ok");
        assert_eq!(result.severity, DtSeverity::Medium);
        assert_eq!(result.channels, vec![AlertChannel::Slack]);
    }

    #[tokio::test]
    async fn low_severity_no_channels() {
        let handler = make_handler();
        let event = make_event(2.0);
        signed_event(&handler, &event);
        let result = handler.handle_webhook(event).await.expect("ok");
        assert_eq!(result.severity, DtSeverity::Low);
        assert!(result.channels.is_empty());
    }

    #[tokio::test]
    async fn mock_injection_disabled_returns_error() {
        let handler = make_handler(); // mock_injection_enabled = false
        let result = handler
            .inject_mock_cve(
                DtProjectUuid::new("proj-001").expect("valid"),
                SyntheticCve {
                    cve_id: "CVE-2026-9999".into(),
                    cvss_score: 9.8,
                    description: None,
                },
            )
            .await;
        assert!(matches!(result, Err(DtWebhookError::MockInjectionDisabled)));
    }

    #[tokio::test]
    async fn mock_injection_enabled_succeeds() {
        let handler = InMemoryDtWebhookHandler::new(b"test-secret".to_vec(), true);
        let result = handler
            .inject_mock_cve(
                DtProjectUuid::new("proj-001").expect("valid"),
                SyntheticCve {
                    cve_id: "CVE-2026-9999".into(),
                    cvss_score: 9.8,
                    description: Some("Synthetic test CVE".into()),
                },
            )
            .await
            .expect("mock injection should succeed");
        assert_eq!(result.synthetic_cve_id, "CVE-2026-9999");
        assert!(result.sla_met); // In-memory is instant.
    }

    #[tokio::test]
    async fn patched_locally_suppressed() {
        let handler = make_handler();
        let mut event = make_event(9.5);
        event.component.patched_locally = true;
        event.component.patched_locally_adr = Some("ADR-0042".into());
        event.component.adr_ratified_date = Some("2026-05-01".into()); // Recent.
        signed_event(&handler, &event);
        let result = handler.handle_webhook(event).await;
        assert!(matches!(
            result,
            Err(DtWebhookError::PatchedLocallySuppressed { .. })
        ));
    }
}
