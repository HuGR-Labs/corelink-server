//! Real HTTP Slack incoming-webhook client (`reqwest::blocking`).
//!
//! This is the production wiring referenced by every consumer crate via
//! the shared [`crate::client::SharedSlackClient`] trait. It uses
//! `reqwest::blocking` to keep the call chain synchronous (no tokio
//! runtime is required in `src/`; downstream tests may spin one up via
//! `wiremock`, but the client itself does not).

use std::sync::Arc;
use std::time::Duration;

use crate::audit::{
    SlackAuditEvent, SlackAuditOutcome, SlackAuditSink,
};
use crate::channel::WebhookRegistry;
use crate::client::{SendOutcome, SharedSlackClient, SlackClientError};
use crate::message::SlackMessage;
use crate::redact::redact_webhook;
use crate::retry::{RetryDecision, RetryPolicy};

/// Real Slack incoming-webhook client.
pub struct SlackHttpClient {
    registry: WebhookRegistry,
    audit: Arc<dyn SlackAuditSink>,
    retry: RetryPolicy,
    http: reqwest::blocking::Client,
    /// Sleep function — tests inject a no-op so they do not wait
    /// hundreds of ms.
    #[allow(clippy::type_complexity)]
    sleeper: Box<dyn Fn(Duration) + Send + Sync>,
}

impl core::fmt::Debug for SlackHttpClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SlackHttpClient")
            .field("registry_channels", &self.registry.len())
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl SlackHttpClient {
    /// Construct a production client.
    ///
    /// # Errors
    ///
    /// Returns the underlying `reqwest` error if the HTTP client could
    /// not be configured (TLS init failure, etc).
    pub fn new(
        registry: WebhookRegistry,
        audit: Arc<dyn SlackAuditSink>,
    ) -> Result<Self, reqwest::Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("corelink-slack-real/0.1")
            .build()?;
        Ok(Self {
            registry,
            audit,
            retry: RetryPolicy::r2_4_default(),
            http,
            sleeper: Box::new(std::thread::sleep),
        })
    }

    /// Construct a client with a custom retry policy + sleeper. Used by
    /// tests to inject a fast (no-op) sleeper.
    ///
    /// # Errors
    ///
    /// Returns the underlying `reqwest` error if the HTTP client could
    /// not be configured.
    pub fn with_overrides<F>(
        registry: WebhookRegistry,
        audit: Arc<dyn SlackAuditSink>,
        retry: RetryPolicy,
        sleeper: F,
    ) -> Result<Self, reqwest::Error>
    where
        F: Fn(Duration) + Send + Sync + 'static,
    {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("corelink-slack-real/0.1")
            .build()?;
        Ok(Self {
            registry,
            audit,
            retry,
            http,
            sleeper: Box::new(sleeper),
        })
    }

    fn emit_audit(
        &self,
        outcome: SlackAuditOutcome,
        message: &SlackMessage,
        webhook_redacted: &str,
        status: Option<u16>,
        attempts: u32,
        reason: Option<String>,
    ) -> Result<(), SlackClientError> {
        let evt = SlackAuditEvent {
            outcome,
            channel: message.channel,
            webhook_redacted: webhook_redacted.to_string(),
            final_status: status,
            attempts,
            reason,
        };
        self.audit.emit(&evt)?;
        Ok(())
    }
}

impl SharedSlackClient for SlackHttpClient {
    fn send(&self, message: &SlackMessage) -> Result<SendOutcome, SlackClientError> {
        let url = self.registry.url(message.channel)?.to_string();
        let webhook_redacted = redact_webhook(&url);
        let payload = message.to_block_kit_json();

        let mut attempt: u32 = 0;
        // We iterate at most `max_retries + 1` times.
        loop {
            let send_result = self.http.post(&url).json(&payload).send();

            let status_code: Option<u16> = match &send_result {
                Ok(resp) => Some(resp.status().as_u16()),
                Err(_) => None,
            };

            let decision = self.retry.decide(status_code, attempt);
            match decision {
                RetryDecision::Success => {
                    let status = status_code.unwrap_or(0);
                    // Best-effort thread_ts extraction — incoming
                    // webhooks return `ok` text on success, but if the
                    // body is JSON with a `ts` field we pick it up.
                    let thread_ts = send_result
                        .ok()
                        .and_then(|resp| resp.text().ok())
                        .and_then(|body| {
                            serde_json::from_str::<serde_json::Value>(&body)
                                .ok()
                                .and_then(|v| {
                                    v.get("ts")
                                        .and_then(|t| t.as_str().map(str::to_string))
                                })
                        });
                    self.emit_audit(
                        SlackAuditOutcome::Sent,
                        message,
                        &webhook_redacted,
                        Some(status),
                        attempt + 1,
                        None,
                    )?;
                    return Ok(SendOutcome {
                        channel: message.channel,
                        status,
                        attempts: attempt + 1,
                        thread_ts,
                    });
                }
                RetryDecision::Retry { delay } => {
                    (self.sleeper)(delay);
                    attempt = attempt.saturating_add(1);
                    continue;
                }
                RetryDecision::GiveUp => {
                    let attempts = attempt + 1;
                    let reason = match (status_code, send_result) {
                        (Some(s), _) => format!("status {s}"),
                        (None, Err(e)) => format!("transport: {e}"),
                        (None, Ok(_)) => "transport: unknown".to_string(),
                    };
                    self.emit_audit(
                        SlackAuditOutcome::Failed,
                        message,
                        &webhook_redacted,
                        status_code,
                        attempts,
                        Some(reason.clone()),
                    )?;
                    return Err(match status_code {
                        Some(s) if s == 429 || (500..600).contains(&s) => {
                            SlackClientError::TransportExhausted { attempts, reason }
                        }
                        Some(s) => SlackClientError::PermanentReject {
                            status: s,
                            reason,
                        },
                        None => SlackClientError::TransportExhausted { attempts, reason },
                    });
                }
            }
        }
    }
}
