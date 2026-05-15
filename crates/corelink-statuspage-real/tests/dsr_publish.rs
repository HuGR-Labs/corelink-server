//! Integration test — `StatuspageHttpClient::publish_dsr_metric`
//! against a WireMock-hosted Statuspage Public-Metric API surface.
//!
//! Covers the canonical paths called out in the wave-16 charter:
//!
//! 1. **Happy path** — 1-day aggregation → publish → 201 Created;
//!    request body shape matches `to_metric_body()`; `OAuth …` auth
//!    header asserted on the wire; audit emits `Published` exactly
//!    once.
//! 2. **Auth failure (401)** — emits `corelink.privacy.statuspage_auth_failed.v1`
//!    + returns [`StatuspageClientError::AuthFailed`].
//! 3. **Rate-limit observed (>1 in 5 min)** — second publish denied
//!    locally + backoff Duration suggested with jitter.
//! 4. **API key never appears plaintext in audit envelope.**
//!
//! `wiremock` requires a tokio runtime — we use `#[tokio::test]` and
//! spawn the synchronous `StatuspageHttpClient::publish_dsr_metric` on
//! `tokio::task::spawn_blocking` to bridge the sync API.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;
use std::time::Duration;

use corelink_privacy_erasure_worker::DsrCompletionStats;
use corelink_statuspage_real::{
    bridge_to_report, DsrCompletionReport, InMemoryStatuspageAuditSink, RetryPolicy,
    StatuspageAuditOutcome, StatuspageBackend, StatuspageClientError, StatuspageHttpClient,
    StatuspageRateLimiter,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const API_KEY: &str = "abcdef1234567890";
const PAGE_ID: &str = "abc123page";
const METRIC_ID: &str = "metric-dsr-resolution";

fn sample_report() -> DsrCompletionReport {
    // canonical 24h window starting at unix epoch second 1_700_000_000
    DsrCompletionReport::new(1_700_000_000, 1_700_086_400, 47, 2, 0, 12)
        .expect("canonical 24h window with sane p95")
}

fn build_client(
    base_url: String,
    audit: Arc<InMemoryStatuspageAuditSink>,
    rate_limiter: StatuspageRateLimiter,
) -> StatuspageHttpClient {
    StatuspageHttpClient::with_overrides(
        base_url,
        PAGE_ID,
        METRIC_ID,
        API_KEY,
        audit,
        RetryPolicy::wave16_default(),
        rate_limiter,
        |_d: Duration| {}, // no-op sleeper so retry tests run fast
    )
    .expect("client builds")
}

#[tokio::test]
async fn happy_path_publish_returns_201_and_emits_published_audit() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .and(header("authorization", format!("OAuth {API_KEY}").as_str()))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "created_at": "2026-05-15T00:00:00Z",
            "metric_id": METRIC_ID,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let limiter = StatuspageRateLimiter::new();
    let url = server.uri();
    let audit_for_spawn = audit.clone();
    let report = sample_report();

    let outcome = tokio::task::spawn_blocking(move || {
        let client = build_client(url, audit_for_spawn, limiter);
        client.publish_dsr_metric(&report, 1_000)
    })
    .await
    .expect("blocking task")
    .expect("publish succeeds");

    assert_eq!(outcome.status, 201);
    assert_eq!(outcome.attempts, 1);
    assert_eq!(outcome.page_id, PAGE_ID);
    assert_eq!(outcome.metric_id, METRIC_ID);

    let events = audit.snapshot();
    assert_eq!(events.len(), 1);
    let evt = events.first().expect("one event");
    assert_eq!(evt.outcome, StatuspageAuditOutcome::Published);
    assert_eq!(evt.final_status, Some(201));
    // Credential never plaintext in audit.
    assert!(!evt.api_key_redacted.contains(API_KEY));
    assert!(evt.api_key_redacted.ends_with("7890"));
    assert_eq!(evt.p95_hours_observed, 12);
    assert_eq!(evt.window_end_unix_s, 1_700_086_400);
}

#[tokio::test]
async fn auth_failure_emits_auth_failed_audit_and_returns_auth_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "Could not authenticate"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let limiter = StatuspageRateLimiter::new();
    let url = server.uri();
    let audit_for_spawn = audit.clone();
    let report = sample_report();

    let err = tokio::task::spawn_blocking(move || {
        let client = build_client(url, audit_for_spawn, limiter);
        client.publish_dsr_metric(&report, 1_000)
    })
    .await
    .expect("blocking task")
    .expect_err("auth must fail");

    match err {
        StatuspageClientError::AuthFailed { status } => assert_eq!(status, 401),
        other => panic!("expected AuthFailed, got {other:?}"),
    }

    let events = audit.snapshot();
    assert_eq!(events.len(), 1);
    let evt = events.first().expect("one event");
    assert_eq!(evt.outcome, StatuspageAuditOutcome::AuthFailed);
    assert_eq!(
        StatuspageAuditOutcome::AuthFailed.event_type(),
        "corelink.privacy.statuspage_auth_failed.v1"
    );
    assert_eq!(evt.final_status, Some(401));
    // Credential never plaintext on error path either.
    assert!(!evt.api_key_redacted.contains(API_KEY));
}

#[tokio::test]
async fn worker_aggregate_bridges_and_publishes_to_statuspage() {
    // End-to-end: emulate the wave-16 publish job. Worker emits a
    // 24h-rolling `DsrCompletionStats`; the bridge converts to a
    // `DsrCompletionReport`; the real HTTP client publishes.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .and(header("authorization", format!("OAuth {API_KEY}").as_str()))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;

    let stats = DsrCompletionStats {
        window_start_unix_s: 1_700_000_000,
        window_end_unix_s: 1_700_086_400,
        verified_complete_count: 47,
        verified_partial_count: 2,
        sla_breached_count: 0,
        p95_resolution_hours: 18,
    };
    let report = bridge_to_report(&stats).expect("bridge converts");

    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let limiter = StatuspageRateLimiter::new();
    let url = server.uri();
    let audit_for_spawn = audit.clone();

    let outcome = tokio::task::spawn_blocking(move || {
        let client = build_client(url, audit_for_spawn, limiter);
        client.publish_dsr_metric(&report, 1_000)
    })
    .await
    .expect("blocking task")
    .expect("bridged publish succeeds");
    assert_eq!(outcome.status, 201);
    let evt = audit.snapshot();
    assert_eq!(evt.first().unwrap().outcome, StatuspageAuditOutcome::Published);
    assert_eq!(evt.first().unwrap().p95_hours_observed, 18);
}

#[tokio::test]
async fn rate_limit_blocks_second_publish_within_five_minutes_with_jitter() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .respond_with(ResponseTemplate::new(201))
        // exactly one request should hit the server — the second is
        // denied locally by the limiter.
        .expect(1)
        .mount(&server)
        .await;

    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let limiter = StatuspageRateLimiter::new();
    let url = server.uri();
    let audit_for_spawn = audit.clone();
    let report = sample_report();

    let (first, second) = tokio::task::spawn_blocking(move || {
        let client = build_client(url, audit_for_spawn, limiter);
        let first = client.publish_dsr_metric(&report, 0);
        // 2 min later — well inside the 5-min window.
        let second = client.publish_dsr_metric(&report, 120_000);
        (first, second)
    })
    .await
    .expect("blocking task");

    let first = first.expect("first publish succeeds");
    assert_eq!(first.status, 201);

    let err = second.expect_err("second publish must be rate-limited");
    match err {
        StatuspageClientError::RateLimited {
            retry_after_ms,
            jitter_ms,
        } => {
            // 5 min - 2 min = 3 min remaining.
            assert_eq!(retry_after_ms, 3 * 60 * 1_000);
            // jitter = retry_after / 8.
            assert_eq!(jitter_ms, retry_after_ms / 8);
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }

    let events = audit.snapshot();
    assert_eq!(events.len(), 2);
    assert_eq!(events.first().unwrap().outcome, StatuspageAuditOutcome::Published);
    assert_eq!(
        events.get(1).unwrap().outcome,
        StatuspageAuditOutcome::RateLimited
    );
    assert_eq!(
        StatuspageAuditOutcome::RateLimited.event_type(),
        "corelink.privacy.statuspage_rate_limited.v1"
    );
}
