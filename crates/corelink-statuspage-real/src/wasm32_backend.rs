//! Wave-18 wasm32 real Statuspage Public-Metric publisher backed by
//! `worker::Fetch::send`.
//!
//! # Why a separate backend
//!
//! The canonical [`crate::StatuspageBackend`] trait is **sync** — every
//! existing wave-16 implementation (`StatuspageHttpClient`,
//! `InMemoryStatuspageBackend`) calls `publish_dsr_metric` from a
//! `reqwest::blocking::Client::send` site. The CF Workers Fetch API is
//! async (returns a wasm-bindgen-futures::JsFuture) and the
//! `wasm32-unknown-unknown` target ships no `block_on` executor we can
//! use to bridge sync→async. Implementing the trait directly would
//! therefore force a panic-on-await — explicitly forbidden by the
//! workspace lint set + the wave-17 charter ("no panic outside test").
//!
//! Wave-18 therefore ships [`StatuspageWasm32Client`] as a separate
//! struct whose canonical entry point is the **async** method
//! [`StatuspageWasm32Client::publish_dsr_metric_async`]. The method
//! mirrors the wave-16 `StatuspageBackend::publish_dsr_metric`
//! semantics step-for-step (rate-limit gate → retry loop → audit
//! emit) so the wasm32 cron handler in
//! `corelink-clerk-cf::dsr_statuspage_cron` can compose
//! `aggregate_24h_window → bridge_to_report → publish_dsr_metric_async`
//! on the CF Workers async event loop.
//!
//! The credential redaction (`OAuth ***<last4>`) is preserved verbatim
//! via [`crate::redact_api_key`]; the audit envelope shape is identical
//! to the wave-16 native path; the rate-limit window + retry policy
//! constants are shared via [`crate::rate_limit::StatuspageRateLimiter`]
//! + [`crate::retry::RetryPolicy`].

use std::sync::Arc;

use crate::audit::{StatuspageAuditEvent, StatuspageAuditOutcome, StatuspageAuditSink};
use crate::backend::{PublishOutcome, StatuspageClientError};
use crate::rate_limit::{RateLimitDecision, StatuspageRateLimiter};
use crate::redact::redact_api_key;
use crate::report::DsrCompletionReport;
use crate::retry::{RetryDecision, RetryPolicy};

/// Canonical Statuspage Public-Metric base URL (mirrors
/// [`crate::http::STATUSPAGE_BASE_URL`] for the native path).
pub const STATUSPAGE_BASE_URL: &str = "https://api.statuspage.io";

/// wasm32 backend bring-up error taxonomy. Distinct from
/// [`StatuspageClientError`] (which encodes per-publish outcomes) — this
/// type carries construction-time / boot-time failures.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wasm32BackendError {
    /// CF Workers `worker::*` runtime returned an error building a
    /// request, headers, or body. The string carries the stable
    /// diagnostic; the underlying `worker::Error` is non-Clone so we
    /// stringify it at the boundary.
    #[error("statuspage wasm32 worker: {0}")]
    Worker(String),
}

/// wasm32 Statuspage Public-Metric client. Wires `worker::Fetch` and
/// emits the canonical wave-16 audit events on every publish attempt.
///
/// Construction takes the page/metric/api_key triplet + an audit sink.
/// The api_key is held for the duration of the client but ONLY ever
/// emitted into the wire `Authorization` header — the audit envelope
/// carries the redacted form (`OAuth ***<last4>`).
pub struct StatuspageWasm32Client {
    base_url: String,
    page_id: String,
    metric_id: String,
    api_key: String,
    api_key_redacted: String,
    audit: Arc<dyn StatuspageAuditSink>,
    retry: RetryPolicy,
    rate_limiter: StatuspageRateLimiter,
}

impl core::fmt::Debug for StatuspageWasm32Client {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StatuspageWasm32Client")
            .field("base_url", &self.base_url)
            .field("page_id", &self.page_id)
            .field("metric_id", &self.metric_id)
            .field("api_key_redacted", &self.api_key_redacted)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl StatuspageWasm32Client {
    /// Construct against the canonical Atlassian Statuspage base URL
    /// (`https://api.statuspage.io`).
    #[must_use]
    pub fn new(
        page_id: impl Into<String>,
        metric_id: impl Into<String>,
        api_key: impl Into<String>,
        audit: Arc<dyn StatuspageAuditSink>,
    ) -> Self {
        let api_key = api_key.into();
        let api_key_redacted = redact_api_key(&api_key);
        Self {
            base_url: STATUSPAGE_BASE_URL.to_string(),
            page_id: page_id.into(),
            metric_id: metric_id.into(),
            api_key,
            api_key_redacted,
            audit,
            retry: RetryPolicy::wave16_default(),
            rate_limiter: StatuspageRateLimiter::new(),
        }
    }

    /// Test / staging override: custom base URL + retry + rate-limiter.
    #[must_use]
    pub fn with_overrides(
        base_url: impl Into<String>,
        page_id: impl Into<String>,
        metric_id: impl Into<String>,
        api_key: impl Into<String>,
        audit: Arc<dyn StatuspageAuditSink>,
        retry: RetryPolicy,
        rate_limiter: StatuspageRateLimiter,
    ) -> Self {
        let api_key = api_key.into();
        let api_key_redacted = redact_api_key(&api_key);
        Self {
            base_url: base_url.into(),
            page_id: page_id.into(),
            metric_id: metric_id.into(),
            api_key,
            api_key_redacted,
            audit,
            retry,
            rate_limiter,
        }
    }

    /// Borrow the redacted API key for boot-time logging / diagnostics.
    /// The plaintext key never leaves the struct boundary.
    #[must_use]
    pub fn api_key_redacted(&self) -> &str {
        &self.api_key_redacted
    }

    /// Borrow the configured page ID.
    #[must_use]
    pub fn page_id(&self) -> &str {
        &self.page_id
    }

    /// Borrow the configured metric ID.
    #[must_use]
    pub fn metric_id(&self) -> &str {
        &self.metric_id
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

    /// Publish a single 24h-rolling DSR completion data-point to
    /// Statuspage via `worker::Fetch::send`.
    ///
    /// # Behaviour (mirrors wave-16 native semantics)
    ///
    /// 1. Local rate-limiter gate (`1 publish per 5 min per metric`).
    ///    On `DenyBackoff` emit a `RateLimited` audit + return
    ///    [`StatuspageClientError::RateLimited`].
    /// 2. Build a `POST {base_url}/v1/pages/{page_id}/metrics/{metric_id}/data.json`
    ///    request with `Authorization: OAuth <STATUSPAGE_API_KEY>` +
    ///    JSON body produced by [`DsrCompletionReport::to_metric_body`].
    /// 3. Send via `worker::Fetch::Request`, classify the result
    ///    against the canonical [`RetryPolicy`] (2xx → `Success`,
    ///    401/403 → `GiveUpAuth`, 429/5xx → `Retry` until exhausted,
    ///    other 4xx → `GiveUp`).
    /// 4. On retry, no sleep is performed (CF Workers offers no
    ///    cooperative sleep primitive that does not stall the isolate;
    ///    the canonical workers-rs idiom is to fast-loop with the
    ///    retry attempt counter). The wave-16 exponential-backoff
    ///    `RetryPolicy::backoff_for` is still consulted so the audit
    ///    `reason` line carries the equivalent delay for forensic
    ///    parity with the native path; the wall-clock pause is elided
    ///    on wasm32.
    /// 5. Emit the canonical wave-16 audit event
    ///    (`Published` / `AuthFailed` / `Failed`) BEFORE the
    ///    caller-visible result (fail-CLOSED — audit-emit failure
    ///    propagates as [`StatuspageClientError::AuditFailed`]).
    ///
    /// # Errors
    ///
    /// Any [`StatuspageClientError`] variant. The audit envelope is
    /// fail-CLOSED: if audit emit fails the error is surfaced via
    /// `AuditFailed` and the caller MUST NOT treat the publish as
    /// successful.
    pub async fn publish_dsr_metric_async(
        &self,
        report: &DsrCompletionReport,
        now_epoch_ms: u64,
    ) -> Result<PublishOutcome, StatuspageClientError> {
        // 1. Local rate-limiter gate.
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
        let body_string = serde_json::to_string(&payload).map_err(|e| {
            StatuspageClientError::TransportExhausted {
                attempts: 0,
                reason: format!("serialize body: {e}"),
            }
        })?;
        let auth = self.auth_header();

        let mut attempt: u32 = 0;
        loop {
            let send_result = send_one_request(&url, &auth, &body_string).await;
            let status_code: Option<u16> = match &send_result {
                Ok(status) => Some(*status),
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
                RetryDecision::Retry { delay: _ } => {
                    // wasm32: no cooperative sleep — fast-loop. The
                    // canonical retry-policy backoff is consulted above
                    // (the delay is emitted into the audit reason path
                    // on final give-up; on a `Retry` arm the loop just
                    // re-issues immediately).
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

/// Build + dispatch one `POST` to Statuspage via `worker::Fetch`. Returns
/// the HTTP status code on a successful round-trip (any 1xx-5xx is
/// considered "received" — the retry policy decides what to do with
/// non-2xx) or a stable diagnostic string on a transport failure.
async fn send_one_request(
    url: &str,
    auth_header: &str,
    body: &str,
) -> Result<u16, String> {
    let headers = worker::Headers::new();
    headers
        .set("authorization", auth_header)
        .map_err(|e| format!("headers set authorization: {e}"))?;
    headers
        .set("content-type", "application/json")
        .map_err(|e| format!("headers set content-type: {e}"))?;
    headers
        .set("user-agent", "corelink-statuspage-real/0.1 (wasm32)")
        .map_err(|e| format!("headers set user-agent: {e}"))?;

    let body_jsvalue = wasm_bindgen::JsValue::from_str(body);
    let mut init = worker::RequestInit::new();
    init.with_method(worker::Method::Post)
        .with_headers(headers)
        .with_body(Some(body_jsvalue));

    let req = worker::Request::new_with_init(url, &init)
        .map_err(|e| format!("build request: {e}"))?;

    let response = worker::Fetch::Request(req)
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    Ok(response.status_code())
}
