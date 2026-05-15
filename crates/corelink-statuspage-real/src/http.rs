//! Real Atlassian Statuspage Public-Metric HTTP client (reqwest blocking).
//!
//! Wire: `POST {base_url}/v1/pages/{page_id}/metrics/{metric_id}/data.json`
//! with `Authorization: OAuth <STATUSPAGE_API_KEY>` and a JSON body
//! shaped as [`crate::report::DsrCompletionReport::to_metric_body`].
//!
//! Per Atlassian Statuspage API v1 a successful publish returns
//! `201 Created` (body echoes the inserted data-point). 4xx auth
//! failures are surfaced as a distinct audit outcome
//! (`statuspage_auth_failed.v1`); 429 / 5xx retry per
//! [`crate::retry::RetryPolicy`].

use std::sync::Arc;
use std::time::Duration;

use crate::audit::{StatuspageAuditEvent, StatuspageAuditOutcome, StatuspageAuditSink};
use crate::backend::{PublishOutcome, StatuspageBackend, StatuspageClientError};
use crate::rate_limit::{RateLimitDecision, StatuspageRateLimiter};
use crate::redact::redact_api_key;
use crate::report::DsrCompletionReport;
use crate::retry::{RetryDecision, RetryPolicy};

/// Canonical Statuspage base URL. Override via
/// [`StatuspageHttpClient::with_overrides`] in tests (WireMock).
pub const STATUSPAGE_BASE_URL: &str = "https://api.statuspage.io";

/// Real Statuspage Public-Metric client.
pub struct StatuspageHttpClient {
    base_url: String,
    page_id: String,
    metric_id: String,
    api_key: String,
    api_key_redacted: String,
    audit: Arc<dyn StatuspageAuditSink>,
    retry: RetryPolicy,
    rate_limiter: StatuspageRateLimiter,
    http: reqwest::blocking::Client,
    #[allow(clippy::type_complexity)]
    sleeper: Box<dyn Fn(Duration) + Send + Sync>,
}

impl core::fmt::Debug for StatuspageHttpClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StatuspageHttpClient")
            .field("base_url", &self.base_url)
            .field("page_id", &self.page_id)
            .field("metric_id", &self.metric_id)
            .field("api_key_redacted", &self.api_key_redacted)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl StatuspageHttpClient {
    /// Construct a production client (base URL = canonical Atlassian
    /// API, retry = canonical wave-16 default, sleeper = thread::sleep).
    ///
    /// # Errors
    ///
    /// Returns the underlying `reqwest` error if the HTTP client could
    /// not be configured (TLS init failure, etc).
    pub fn new(
        page_id: impl Into<String>,
        metric_id: impl Into<String>,
        api_key: impl Into<String>,
        audit: Arc<dyn StatuspageAuditSink>,
    ) -> Result<Self, reqwest::Error> {
        let api_key = api_key.into();
        let api_key_redacted = redact_api_key(&api_key);
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("corelink-statuspage-real/0.1")
            .build()?;
        Ok(Self {
            base_url: STATUSPAGE_BASE_URL.to_string(),
            page_id: page_id.into(),
            metric_id: metric_id.into(),
            api_key,
            api_key_redacted,
            audit,
            retry: RetryPolicy::wave16_default(),
            rate_limiter: StatuspageRateLimiter::new(),
            http,
            sleeper: Box::new(std::thread::sleep),
        })
    }

    /// Test-override constructor: custom base URL, retry, sleeper, +
    /// optional rate-limiter (e.g. to share with the publish job).
    ///
    /// # Errors
    ///
    /// Returns the underlying `reqwest` error if the HTTP client could
    /// not be configured.
    #[allow(clippy::too_many_arguments)]
    pub fn with_overrides<F>(
        base_url: impl Into<String>,
        page_id: impl Into<String>,
        metric_id: impl Into<String>,
        api_key: impl Into<String>,
        audit: Arc<dyn StatuspageAuditSink>,
        retry: RetryPolicy,
        rate_limiter: StatuspageRateLimiter,
        sleeper: F,
    ) -> Result<Self, reqwest::Error>
    where
        F: Fn(Duration) + Send + Sync + 'static,
    {
        let api_key = api_key.into();
        let api_key_redacted = redact_api_key(&api_key);
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("corelink-statuspage-real/0.1")
            .build()?;
        Ok(Self {
            base_url: base_url.into(),
            page_id: page_id.into(),
            metric_id: metric_id.into(),
            api_key,
            api_key_redacted,
            audit,
            retry,
            rate_limiter,
            http,
            sleeper: Box::new(sleeper),
        })
    }

    fn metric_url(&self) -> String {
        format!(
            "{}/v1/pages/{}/metrics/{}/data.json",
            self.base_url.trim_end_matches('/'),
            self.page_id,
            self.metric_id
        )
    }

    fn auth_header(&self) -> String {
        format!("OAuth {}", self.api_key)
    }

    fn emit_audit(
        &self,
        outcome: StatuspageAuditOutcome,
        report: &DsrCompletionReport,
        status: Option<u16>,
        attempts: u32,
        reason: Option<String>,
    ) -> Result<(), StatuspageClientError> {
        let evt = StatuspageAuditEvent {
            outcome,
            page_id: self.page_id.clone(),
            metric_id: self.metric_id.clone(),
            api_key_redacted: self.api_key_redacted.clone(),
            final_status: status,
            attempts,
            reason,
            p95_hours_observed: report.p95_resolution_hours,
            window_end_unix_s: report.window_end_unix_s,
        };
        self.audit.emit(&evt)?;
        Ok(())
    }
}

impl StatuspageBackend for StatuspageHttpClient {
    fn publish_dsr_metric(
        &self,
        report: &DsrCompletionReport,
        now_epoch_ms: u64,
    ) -> Result<PublishOutcome, StatuspageClientError> {
        // Local rate-limiter gate (audit-emit BEFORE returning).
        match self
            .rate_limiter
            .decide(&self.page_id, &self.metric_id, now_epoch_ms)
        {
            RateLimitDecision::DenyBackoff { retry_after, jitter } => {
                let retry_after_ms = u64::try_from(retry_after.as_millis()).unwrap_or(u64::MAX);
                let jitter_ms = u64::try_from(jitter.as_millis()).unwrap_or(u64::MAX);
                self.emit_audit(
                    StatuspageAuditOutcome::RateLimited,
                    report,
                    None,
                    0,
                    Some(format!(
                        "local rate-limiter: retry_after_ms={retry_after_ms} jitter_ms={jitter_ms}"
                    )),
                )?;
                return Err(StatuspageClientError::RateLimited {
                    retry_after_ms,
                    jitter_ms,
                });
            }
            RateLimitDecision::Allow => {}
        }

        let url = self.metric_url();
        let payload = report.to_metric_body();
        let mut attempt: u32 = 0;
        loop {
            let send_result = self
                .http
                .post(&url)
                .header(reqwest::header::AUTHORIZATION, self.auth_header())
                .json(&payload)
                .send();

            let status_code: Option<u16> = match &send_result {
                Ok(resp) => Some(resp.status().as_u16()),
                Err(_) => None,
            };

            let decision = self.retry.decide(status_code, attempt);
            match decision {
                RetryDecision::Success => {
                    let status = status_code.unwrap_or(0);
                    self.emit_audit(
                        StatuspageAuditOutcome::Published,
                        report,
                        Some(status),
                        attempt + 1,
                        None,
                    )?;
                    return Ok(PublishOutcome {
                        page_id: self.page_id.clone(),
                        metric_id: self.metric_id.clone(),
                        status,
                        attempts: attempt + 1,
                    });
                }
                RetryDecision::Retry { delay } => {
                    (self.sleeper)(delay);
                    attempt = attempt.saturating_add(1);
                    continue;
                }
                RetryDecision::GiveUpAuth => {
                    let attempts = attempt + 1;
                    let status = status_code.unwrap_or(0);
                    self.emit_audit(
                        StatuspageAuditOutcome::AuthFailed,
                        report,
                        Some(status),
                        attempts,
                        Some(format!("status {status}")),
                    )?;
                    return Err(StatuspageClientError::AuthFailed { status });
                }
                RetryDecision::GiveUp => {
                    let attempts = attempt + 1;
                    let reason = match (status_code, send_result) {
                        (Some(s), _) => format!("status {s}"),
                        (None, Err(e)) => format!("transport: {e}"),
                        (None, Ok(_)) => "transport: unknown".to_string(),
                    };
                    self.emit_audit(
                        StatuspageAuditOutcome::Failed,
                        report,
                        status_code,
                        attempts,
                        Some(reason.clone()),
                    )?;
                    return Err(match status_code {
                        Some(s) if s == 429 || (500..600).contains(&s) => {
                            StatuspageClientError::TransportExhausted { attempts, reason }
                        }
                        Some(s) => StatuspageClientError::PermanentReject { status: s, reason },
                        None => StatuspageClientError::TransportExhausted { attempts, reason },
                    });
                }
            }
        }
    }
}
