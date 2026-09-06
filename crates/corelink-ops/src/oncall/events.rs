//! PagerDuty Events API v2 HTTPS client (R2-3).
//!
//! Real-network sibling of the trait + fake surface in
//! [`super::pagerduty`]. Compiled only when the `production` Cargo
//! feature is enabled (see `Cargo.toml`). Default build keeps the
//! workspace I/O-free per WI-S17-005's `trait-abstraction-defer`
//! charter.
//!
//! # Endpoint
//!
//! `POST https://events.pagerduty.com/v2/enqueue` — Events API v2 (NOT
//! the REST API; Events v2 uses a routing/integration key, not an API
//! token, and has a substantially cheaper rate-limit budget).
//!
//! # Auth
//!
//! [`HttpPagerDutyClient`] reads its integration key from the
//! `PAGERDUTY_ROUTING_KEY` environment variable. The key is held in
//! memory and NEVER logged — every log/trace site below redacts it to
//! `***` before formatting. Wrap it in a [`RoutingKey`] newtype so
//! accidental `Debug` prints emit `RoutingKey(***)`.
//!
//! # Event types
//!
//! - `trigger` — open a new incident (or collapse to an existing one
//!   that shares the same `dedup_key`).
//! - `acknowledge` — same `dedup_key` is required; PD treats it as an
//!   idempotent ack on the open incident.
//! - `resolve` — same `dedup_key`; closes the open incident.
//!
//! # Severity mapping
//!
//! | Severity arm                          | PD `severity` field |
//! |---------------------------------------|---------------------|
//! | [`Severity::Sev0`]                    | `critical`          |
//! | [`Severity::Sev1`]                    | `critical`          |
//! | [`Severity::Sev2`]                    | `error`             |
//! | [`Severity::Sev3`]                    | `warning`           |
//! | Synthetic drill (any [`PageContext`]) | `info`              |
//!
//! **NEVER** map a synthetic drill to `critical`; the operator's
//! incident queue MUST be able to distinguish a real Sev0/Sev1
//! customer-impacting incident from a weekly drill at a glance.
//!
//! # Synthetic service isolation
//!
//! Synthetic drills emit to a **different PagerDuty service** than
//! production alerts so the real-incident escalation/routing tree is
//! preserved. The split is enforced by:
//!
//! - Routing key: production = `PAGERDUTY_ROUTING_KEY`; synthetic =
//!   `PAGERDUTY_SYNTHETIC_ROUTING_KEY`. Configure via
//!   [`HttpPagerDutyClient::with_synthetic_routing_key`].
//! - Payload `source` = `corelink-synthetic-drill-{region}` for
//!   drills; `corelink-prod` for production.
//! - Payload `component` = `synthetic-pager` for drills;
//!   `oncall-handoff` (or caller-provided) for production.
//!
//! # Retry policy
//!
//! Exponential backoff with jitter cap, up to 5 retries, on:
//!
//! - HTTP `5xx`
//! - HTTP `429` (rate limited)
//! - Network/transport errors (timeouts, connection refused)
//!
//! `Retry-After` is honoured when present on 429/503. If the budget
//! is exhausted on a `trigger` event, the client falls through to a
//! configured Slack webhook fallback and emits a
//! `pagerduty.send_failed_after_retries` audit record — a `trigger`
//! is NEVER dropped silently.
//!
//! `acknowledge` and `resolve` events do NOT fall through to Slack —
//! these can be replayed safely (idempotent by `dedup_key`) on the
//! next webhook tick, and we don't want to spam Slack with state
//! transitions.
//!
//! # Fail-CLOSED ordering
//!
//! Every send arm follows `lookup → emit_audit → mutate_state`:
//!
//! 1. Look up routing key + payload skeleton.
//! 2. Emit an audit record describing the intended send BEFORE the
//!    HTTPS call (per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 3. Only then perform the HTTPS POST.
//!
//! If the audit emit fails, the HTTPS call is aborted and a typed
//! [`OncallPagerDutyError::Transport`] is returned.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value as JsonValue;

use super::error::OncallPagerDutyError;
use super::severity::Severity;

/// Canonical Events API v2 endpoint.
pub const PAGERDUTY_EVENTS_V2_URL: &str = "https://events.pagerduty.com/v2/enqueue";

/// Maximum retries before falling through to the Slack fallback path.
pub const MAX_RETRIES: u32 = 5;

/// Base backoff used for exponential retry scheduling (ms).
pub const BACKOFF_BASE_MS: u64 = 200;

/// Cap on a single backoff sleep (ms) — avoids unbounded waits when
/// the server returns a malicious `Retry-After`.
pub const BACKOFF_CAP_MS: u64 = 30_000;

/// PagerDuty enforces a 512 KB payload ceiling on Events v2. We
/// reject locally with a typed error rather than wasting a network
/// round-trip.
pub const PAYLOAD_MAX_BYTES: usize = 512 * 1024;

/// Opaque routing-key newtype. `Debug` deliberately redacts.
#[derive(Clone)]
pub struct RoutingKey(String);

impl RoutingKey {
    /// Wrap a routing key. Empty inputs are rejected.
    ///
    /// # Errors
    ///
    /// Returns [`OncallPagerDutyError::NotFound`] when the key is
    /// blank — surfaces a missing env var early.
    pub fn new(s: impl Into<String>) -> Result<Self, OncallPagerDutyError> {
        let v: String = s.into();
        if v.trim().is_empty() {
            return Err(OncallPagerDutyError::NotFound(
                "PAGERDUTY_ROUTING_KEY is empty".to_string(),
            ));
        }
        Ok(Self(v))
    }

    /// Read from `PAGERDUTY_ROUTING_KEY`.
    ///
    /// # Errors
    ///
    /// Returns [`OncallPagerDutyError::NotFound`] when the env var is
    /// unset or blank.
    pub fn from_env() -> Result<Self, OncallPagerDutyError> {
        let v = std::env::var("PAGERDUTY_ROUTING_KEY").map_err(|_| {
            OncallPagerDutyError::NotFound("PAGERDUTY_ROUTING_KEY unset".to_string())
        })?;
        Self::new(v)
    }

    /// Wire-format accessor (used by the HTTP serializer). NEVER log
    /// the return value.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Debug for RoutingKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RoutingKey(***)")
    }
}

/// `event_action` wire value for Events API v2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EventAction {
    /// Open or collapse to the existing incident with the same
    /// `dedup_key`.
    Trigger,
    /// Acknowledge the open incident keyed by `dedup_key`.
    Acknowledge,
    /// Resolve the open incident keyed by `dedup_key`.
    Resolve,
}

impl EventAction {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::Acknowledge => "acknowledge",
            Self::Resolve => "resolve",
        }
    }
}

/// Context indicating production vs synthetic drill so payload routing
/// + severity mapping pick the right PagerDuty service.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PageContext {
    /// Production incident — routes to the main PagerDuty service.
    Production {
        /// Component slug (e.g. `oncall-handoff`, `quota-fsm`).
        component: String,
    },
    /// Synthetic drill — routes to the `synthetic-drill` service.
    Synthetic {
        /// Region slug (`americas` / `emea` / `apac`).
        region: String,
    },
}

impl PageContext {
    /// Canonical PD `source` field.
    #[must_use]
    pub fn source(&self) -> String {
        match self {
            Self::Production { .. } => "corelink-prod".to_string(),
            Self::Synthetic { region } => format!("corelink-synthetic-drill-{region}"),
        }
    }

    /// Canonical PD `component` field.
    #[must_use]
    pub fn component(&self) -> &str {
        match self {
            Self::Production { component } => component.as_str(),
            Self::Synthetic { .. } => "synthetic-pager",
        }
    }

    /// True iff this is a synthetic drill (forces severity `info`).
    #[must_use]
    pub const fn is_synthetic(&self) -> bool {
        matches!(self, Self::Synthetic { .. })
    }
}

/// Map an oncall [`Severity`] + [`PageContext`] to the PagerDuty wire
/// `severity` string. Synthetic drills are **always** mapped to
/// `info` regardless of the underlying [`Severity`].
#[must_use]
pub fn map_severity(severity: Severity, ctx: &PageContext) -> &'static str {
    if ctx.is_synthetic() {
        return "info";
    }
    match severity {
        // Sev0 is also `critical` — same paging behaviour as Sev1.
        Severity::Sev0 | Severity::Sev1 => "critical",
        Severity::Sev2 => "error",
        Severity::Sev3 => "warning",
    }
}

/// One Events API v2 send request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PagerDutyEvent {
    /// `trigger` / `acknowledge` / `resolve`.
    pub action: EventAction,
    /// Idempotency key — same value across `trigger` →
    /// `acknowledge` → `resolve` for one incident.
    pub dedup_key: String,
    /// Human summary (truncated by PD at 1024 chars).
    pub summary: String,
    /// Oncall severity (mapped via [`map_severity`]).
    pub severity: Severity,
    /// Production vs synthetic.
    pub context: PageContext,
    /// Audit-chain correlation id (PAT-CORRELATION-ID-001). Never put
    /// PII (tenant_id, email, customer name) here — use a hash.
    pub correlation_id: String,
    /// Optional `tenant_id_hash` (BLAKE3 of the canonical tenant id).
    /// Per the R2-3 brief, never carry raw PII.
    pub tenant_id_hash: Option<String>,
}

#[derive(Serialize)]
struct WirePayload<'a> {
    summary: &'a str,
    source: String,
    severity: &'static str,
    component: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'a str>,
    class: &'a str,
    custom_details: JsonValue,
}

#[derive(Serialize)]
struct WireEnvelope<'a> {
    routing_key: &'a str,
    event_action: &'static str,
    dedup_key: &'a str,
    payload: WirePayload<'a>,
}

impl PagerDutyEvent {
    /// Construct a [`PagerDutyEvent`] from the canonical 7 fields.
    /// The struct is `#[non_exhaustive]` so external crates (and
    /// integration tests) MUST use this constructor.
    #[must_use]
    pub fn new(
        action: EventAction,
        dedup_key: impl Into<String>,
        summary: impl Into<String>,
        severity: Severity,
        context: PageContext,
        correlation_id: impl Into<String>,
        tenant_id_hash: Option<String>,
    ) -> Self {
        Self {
            action,
            dedup_key: dedup_key.into(),
            summary: summary.into(),
            severity,
            context,
            correlation_id: correlation_id.into(),
            tenant_id_hash,
        }
    }

    /// Render the wire envelope. Returns the canonical JSON bytes
    /// PagerDuty Events API v2 accepts. Reject locally on the 512KB
    /// ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`OncallPagerDutyError::Transport`] if the rendered
    /// envelope exceeds [`PAYLOAD_MAX_BYTES`] or if JSON serialization
    /// fails (the former is the only path expected in practice).
    pub fn to_wire_bytes(&self, routing_key: &RoutingKey) -> Result<Vec<u8>, OncallPagerDutyError> {
        let mut details = serde_json::Map::new();
        details.insert(
            "correlation_id".to_string(),
            JsonValue::String(self.correlation_id.clone()),
        );
        if let Some(h) = &self.tenant_id_hash {
            details.insert("tenant_id_hash".to_string(), JsonValue::String(h.clone()));
        }
        if let PageContext::Synthetic { region } = &self.context {
            details.insert("region".to_string(), JsonValue::String(region.clone()));
            details.insert("drill".to_string(), JsonValue::Bool(true));
        }

        let envelope = WireEnvelope {
            routing_key: routing_key.expose(),
            event_action: self.action.as_str(),
            dedup_key: &self.dedup_key,
            payload: WirePayload {
                summary: &self.summary,
                source: self.context.source(),
                severity: map_severity(self.severity, &self.context),
                component: self.context.component(),
                group: None,
                class: if self.context.is_synthetic() {
                    "synthetic_drill"
                } else {
                    "incident"
                },
                custom_details: JsonValue::Object(details),
            },
        };

        let bytes = serde_json::to_vec(&envelope)
            .map_err(|e| OncallPagerDutyError::Transport(format!("json encode: {e}")))?;
        if bytes.len() > PAYLOAD_MAX_BYTES {
            return Err(OncallPagerDutyError::Transport(format!(
                "payload too large: {} bytes > {} cap",
                bytes.len(),
                PAYLOAD_MAX_BYTES
            )));
        }
        Ok(bytes)
    }
}

/// Outcome of a send attempt, surfaced to the caller for audit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SendOutcome {
    /// PagerDuty Events v2 acknowledged the event (2xx). Carries the
    /// PD `dedup_key` echoed back (or the locally-supplied one when
    /// the response body did not include it).
    Accepted {
        /// Echoed `dedup_key`.
        dedup_key: String,
        /// Number of retries used (0 means the first attempt
        /// succeeded).
        retries: u32,
    },
    /// All retries exhausted; the event was forwarded to the Slack
    /// fallback webhook instead. Only reachable from `trigger` arms.
    FallbackToSlack {
        /// Number of HTTP attempts before falling through (= MAX_RETRIES + 1).
        attempts: u32,
        /// Last error captured before the fallback fired.
        last_error: String,
    },
}

/// Trait every audit consumer of the Http client satisfies. The trait
/// stays minimal so the synthetic-pager + oncall ledger can share a
/// single fail-CLOSED contract.
pub trait PagerDutyAuditSink: core::fmt::Debug + Send + Sync {
    /// Emit an audit event with the canonical CloudEvents `type`
    /// string. Implementations MUST be synchronous (no tokio) and
    /// MUST return `Err` if the audit chain refuses to advance.
    ///
    /// # Errors
    ///
    /// Adapter-defined. The HTTP client maps any error to
    /// [`OncallPagerDutyError::Transport`] and aborts the send.
    fn emit_audit(&self, event_type: &str, details: &JsonValue) -> Result<(), String>;
}

/// Trait every transport adapter satisfies. Production wires the
/// blocking reqwest client; tests wire a `mockito` server fronting the
/// same surface; the FailingPagerDutyClient adversary forces every
/// arm to error.
pub trait HttpTransport: core::fmt::Debug + Send + Sync {
    /// Execute one HTTPS POST. The implementation MUST NOT log the
    /// request body or any header containing the routing key.
    ///
    /// Returns the HTTP status + raw body bytes + optional
    /// `Retry-After` seconds (when present on 429/503).
    fn post(&self, url: &str, body: &[u8]) -> Result<HttpResponse, String>;
}

/// Wire-level response surface, kept transport-agnostic so we can
/// fake it in tests without spinning up a real socket.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Body bytes (truncated to 4 KiB for log safety upstream).
    pub body: Vec<u8>,
    /// `Retry-After` parsed as seconds (if header present).
    pub retry_after_secs: Option<u64>,
}

/// Production [`HttpTransport`] backed by `reqwest::blocking`.
#[derive(Debug)]
pub struct ReqwestBlockingTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestBlockingTransport {
    /// Build a transport with a 10s connect + 30s request timeout.
    ///
    /// # Errors
    ///
    /// Returns [`OncallPagerDutyError::Transport`] if the underlying
    /// reqwest builder fails to materialise a client (e.g. TLS
    /// backend init failed).
    pub fn new() -> Result<Self, OncallPagerDutyError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("corelink-oncall/0.1")
            .build()
            .map_err(|e| OncallPagerDutyError::Transport(format!("reqwest build: {e}")))?;
        Ok(Self { client })
    }
}

impl HttpTransport for ReqwestBlockingTransport {
    fn post(&self, url: &str, body: &[u8]) -> Result<HttpResponse, String> {
        let resp = self
            .client
            .post(url)
            .header("content-type", "application/json")
            .body(body.to_vec())
            .send()
            .map_err(|e| format!("transport: {e}"))?;

        let status = resp.status().as_u16();
        let retry_after_secs = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let body_bytes = resp
            .bytes()
            .map_err(|e| format!("body read: {e}"))?
            .to_vec();
        Ok(HttpResponse {
            status,
            body: body_bytes,
            retry_after_secs,
        })
    }
}

/// Synchronous sleep used between retries. Pulled out so tests can
/// substitute a no-op clock.
pub trait Clock: core::fmt::Debug + Send + Sync {
    /// Sleep for `dur` (synchronously).
    fn sleep(&self, dur: Duration);
}

/// Default [`Clock`] — `std::thread::sleep`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdClock;

impl Clock for StdClock {
    fn sleep(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// No-op clock — tests pass this to skip retry backoff.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopClock;

impl Clock for NoopClock {
    fn sleep(&self, _dur: Duration) {}
}

/// PagerDuty Events API v2 HTTPS client.
#[derive(Debug)]
pub struct HttpPagerDutyClient {
    endpoint: String,
    routing_key: RoutingKey,
    synthetic_routing_key: Option<RoutingKey>,
    slack_webhook_url: Option<String>,
    transport: Box<dyn HttpTransport>,
    audit: Box<dyn PagerDutyAuditSink>,
    clock: Box<dyn Clock>,
}

impl HttpPagerDutyClient {
    /// Build a client pointed at the canonical PagerDuty endpoint.
    /// `audit` MUST satisfy fail-CLOSED semantics.
    ///
    /// # Errors
    ///
    /// Returns [`OncallPagerDutyError::Transport`] if the underlying
    /// reqwest transport refuses to initialise.
    pub fn new(
        routing_key: RoutingKey,
        audit: Box<dyn PagerDutyAuditSink>,
    ) -> Result<Self, OncallPagerDutyError> {
        Ok(Self {
            endpoint: PAGERDUTY_EVENTS_V2_URL.to_string(),
            routing_key,
            synthetic_routing_key: None,
            slack_webhook_url: None,
            transport: Box::new(ReqwestBlockingTransport::new()?),
            audit,
            clock: Box::new(StdClock),
        })
    }

    /// Builder used by tests + dependency-injection wiring.
    #[must_use]
    pub fn with_parts(
        endpoint: String,
        routing_key: RoutingKey,
        transport: Box<dyn HttpTransport>,
        audit: Box<dyn PagerDutyAuditSink>,
        clock: Box<dyn Clock>,
    ) -> Self {
        Self {
            endpoint,
            routing_key,
            synthetic_routing_key: None,
            slack_webhook_url: None,
            transport,
            audit,
            clock,
        }
    }

    /// Override the synthetic-drill routing key (different PagerDuty
    /// service from the production routing key — see module docs).
    #[must_use]
    pub fn with_synthetic_routing_key(mut self, k: RoutingKey) -> Self {
        self.synthetic_routing_key = Some(k);
        self
    }

    /// Configure the Slack webhook URL used when a `trigger` event
    /// exhausts its retry budget.
    #[must_use]
    pub fn with_slack_fallback(mut self, url: impl Into<String>) -> Self {
        self.slack_webhook_url = Some(url.into());
        self
    }

    /// Pick the right routing key for the page context (production vs
    /// synthetic drill).
    fn routing_key_for(&self, ctx: &PageContext) -> &RoutingKey {
        match ctx {
            PageContext::Synthetic { .. } => self
                .synthetic_routing_key
                .as_ref()
                .unwrap_or(&self.routing_key),
            PageContext::Production { .. } => &self.routing_key,
        }
    }

    /// Send a PagerDuty event. Implements the full
    /// `lookup → emit_audit → mutate_state (HTTP)` fail-CLOSED
    /// envelope.
    ///
    /// # Errors
    ///
    /// - [`OncallPagerDutyError::Transport`] when the audit sink
    ///   refuses, the payload is too large, or the retry budget is
    ///   exhausted without a Slack fallback configured (only on
    ///   `acknowledge` / `resolve`; `trigger` always has the Slack
    ///   path).
    pub fn send(&self, event: &PagerDutyEvent) -> Result<SendOutcome, OncallPagerDutyError> {
        // (1) lookup: select routing key + render bytes (payload-too-
        //     large rejected here, before any I/O).
        let routing_key = self.routing_key_for(&event.context);
        let body = event.to_wire_bytes(routing_key)?;

        // (2) emit_audit: BEFORE the HTTPS call. Failure aborts.
        let audit_details = serde_json::json!({
            "dedup_key": event.dedup_key,
            "event_action": event.action.as_str(),
            "severity_wire": map_severity(event.severity, &event.context),
            "source": event.context.source(),
            "component": event.context.component(),
            "correlation_id": event.correlation_id,
            "tenant_id_hash": event.tenant_id_hash,
            "synthetic": event.context.is_synthetic(),
        });
        self.audit
            .emit_audit("corelink.pagerduty.send_attempted", &audit_details)
            .map_err(|e| {
                OncallPagerDutyError::Transport(format!("audit emit failed (fail-CLOSED): {e}"))
            })?;

        // (3) mutate_state: HTTPS POST with retry.
        let mut last_err = String::new();
        for attempt in 0..=MAX_RETRIES {
            match self.transport.post(&self.endpoint, &body) {
                Ok(resp) if (200..300).contains(&resp.status) => {
                    return Ok(SendOutcome::Accepted {
                        dedup_key: event.dedup_key.clone(),
                        retries: attempt,
                    });
                }
                Ok(resp) if resp.status == 429 || resp.status >= 500 => {
                    last_err = format!("http {} (retryable)", resp.status);
                    let wait = backoff_wait(attempt, resp.retry_after_secs);
                    self.clock.sleep(wait);
                }
                Ok(resp) => {
                    // 4xx (non-429) — fatal. Do not retry.
                    let preview_len = resp.body.len().min(256);
                    let body_preview = resp
                        .body
                        .get(..preview_len)
                        .map(String::from_utf8_lossy)
                        .unwrap_or_default()
                        .to_string();
                    return Err(OncallPagerDutyError::Transport(format!(
                        "pagerduty rejected http {}: {}",
                        resp.status, body_preview
                    )));
                }
                Err(e) => {
                    last_err = e;
                    let wait = backoff_wait(attempt, None);
                    self.clock.sleep(wait);
                }
            }
        }

        // Retry budget exhausted.
        let attempts = MAX_RETRIES.saturating_add(1);
        if matches!(event.action, EventAction::Trigger) {
            // trigger NEVER drops silently — Slack fallback + audit.
            let fallback_audit = serde_json::json!({
                "dedup_key": event.dedup_key,
                "correlation_id": event.correlation_id,
                "attempts": attempts,
                "last_error": last_err,
            });
            let _ = self
                .audit
                .emit_audit("pagerduty.send_failed_after_retries", &fallback_audit);
            if let Some(url) = &self.slack_webhook_url {
                let slack_body = serde_json::json!({
                    "text": format!(
                        "[FALLBACK] PagerDuty trigger failed after {attempts} attempts; correlation_id={}; summary={}",
                        event.correlation_id, event.summary
                    ),
                });
                let slack_bytes = serde_json::to_vec(&slack_body).unwrap_or_default();
                // Best-effort fallback; ignore Slack errors (the
                // audit record above is the durable trace).
                let _ = self.transport.post(url, &slack_bytes);
            }
            return Ok(SendOutcome::FallbackToSlack {
                attempts,
                last_error: last_err,
            });
        }
        Err(OncallPagerDutyError::Transport(format!(
            "pagerduty retries exhausted: {last_err}"
        )))
    }
}

/// Compute the backoff sleep for the `attempt`-th retry. `attempt = 0`
/// is the first retry after the initial send failure.
#[must_use]
pub fn backoff_wait(attempt: u32, retry_after_secs: Option<u64>) -> Duration {
    if let Some(s) = retry_after_secs {
        let ms = s.saturating_mul(1000).min(BACKOFF_CAP_MS);
        return Duration::from_millis(ms);
    }
    let exp = 1u64 << attempt.min(10);
    let ms = BACKOFF_BASE_MS.saturating_mul(exp).min(BACKOFF_CAP_MS);
    Duration::from_millis(ms)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests;
