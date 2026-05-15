//! Wave-17 integration test — `DsrStatuspagePublishScheduler::run_once`
//! against a WireMock-hosted Statuspage Public-Metric surface.
//!
//! Five canonical scenarios per the wave-17 charter:
//!
//! 1. **Happy path** — 24h window with rows → aggregate → bridge →
//!    WireMock `201` → `succeeded` audit + ledger row.
//! 2. **No-rows-in-window** — empty 24h slice → `skipped /
//!    empty_window` audit + ledger row + no HTTP call.
//! 3. **Statuspage 401** — wave-16 client maps to
//!    `StatuspageClientError::AuthFailed` → `failed` scheduler audit
//!    + scheduler `Err(Publish(AuthFailed))`.
//! 4. **Rate-limit 429** — WireMock returns 429; the wave-16
//!    `RetryPolicy::wave16_default` retries 3× and then surfaces a
//!    `TransportExhausted` error → `failed` audit + Err.
//! 5. **D1 read failure** — `InMemoryD1RowSource::with_read_failure`
//!    → `failed` audit + `Err(RowSource)`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;
use std::time::Duration;

use corelink_dsr_statuspage_scheduler::{
    DsrStatuspagePublishScheduler, InMemoryCronRunLog, InMemoryD1RowSource,
    InMemorySchedulerAuditSink, RunOutcome, SchedulerAuditOutcome, SchedulerError, SkipReason,
    CRON_EXPRESSION,
};
use corelink_privacy_erasure_worker::event::{
    BackendCompletion, BackendErasureOutcome, BackendKind, ErasureDecision, ErasurePlan,
    ErasureRequest, ErasureSalt, canonical_cloudevent_types,
};
use corelink_privacy_erasure_worker::report::ErasureReport;
use corelink_privacy_erasure_worker::verification_job::VerificationOutcome;
use corelink_statuspage_real::{
    InMemoryStatuspageAuditSink, RetryPolicy, StatuspageBackend, StatuspageClientError,
    StatuspageHttpClient, StatuspageRateLimiter,
};
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE_ID: &str = "page-abc";
const METRIC_ID: &str = "metric-dsr-resolution";
const API_KEY: &str = "abcdef1234567890";

// 2026-05-15 06:00:00 UTC.
const NOW_UNIX_S: u64 = 1_778_824_800;

fn fixed_uuid(seed: u8) -> Uuid {
    let mut b = [0u8; 16];
    for (i, x) in b.iter_mut().enumerate() {
        *x = seed.wrapping_add(i as u8);
    }
    Uuid::from_bytes(b)
}

fn outcome_in_window() -> VerificationOutcome {
    // verified_at_ms inside [NOW - 86_400, NOW). Pick NOW - 1h.
    let verified_at_ms = (NOW_UNIX_S - 3_600) * 1_000;
    let generated_at_ms = verified_at_ms - (5 * 3_600 * 1_000);
    let request = ErasureRequest::new(
        fixed_uuid(1),
        fixed_uuid(2),
        fixed_uuid(3),
        ErasureSalt::synthetic_for_test(7),
        generated_at_ms,
    );
    let plan = ErasurePlan::canonical(&request, generated_at_ms);
    let completions = vec![BackendCompletion {
        dsr_id: request.dsr_id,
        tenant_id: request.tenant_id,
        backend: BackendKind::D1,
        outcome: BackendErasureOutcome::Erased { records_deleted: 1 },
        idempotency_key: format!("{}:{}", request.dsr_id, BackendKind::D1.as_str()),
        started_at_ms: generated_at_ms,
        completed_at_ms: verified_at_ms,
        retry_count: 0,
        verification_hash: [0u8; 32],
    }];
    let report = ErasureReport {
        dsr_id: request.dsr_id,
        tenant_id: request.tenant_id,
        plan,
        completions: completions.clone(),
        verified_at_ms,
        verified_complete: true,
        cloudevent_types: canonical_cloudevent_types()
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    VerificationOutcome {
        decision: ErasureDecision::VerifiedComplete { completions },
        report: Some(report),
        signature: None,
        object_key: None,
    }
}

fn build_client(
    base_url: String,
    audit: Arc<InMemoryStatuspageAuditSink>,
) -> StatuspageHttpClient {
    StatuspageHttpClient::with_overrides(
        base_url,
        PAGE_ID,
        METRIC_ID,
        API_KEY,
        audit,
        RetryPolicy::wave16_default(),
        StatuspageRateLimiter::new(),
        |_d: Duration| {}, // no-op sleeper
    )
    .expect("client builds")
}

#[test]
fn cron_expression_is_06_utc_daily() {
    assert_eq!(CRON_EXPRESSION, "0 6 * * *");
}

#[tokio::test]
async fn happy_path_aggregates_publishes_and_emits_succeeded() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .and(header("authorization", format!("OAuth {API_KEY}").as_str()))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "created_at": "2026-05-15T06:00:00Z",
            "metric_id": METRIC_ID,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let stp_audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let url = server.uri();
    let scheduler_audit = Arc::new(InMemorySchedulerAuditSink::new());
    let cron_log = Arc::new(InMemoryCronRunLog::new());
    let scheduler_audit_for_spawn = scheduler_audit.clone();
    let cron_log_for_spawn = cron_log.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let client = build_client(url, stp_audit);
        let backend: Arc<dyn StatuspageBackend> = Arc::new(client);
        let row_source = Arc::new(InMemoryD1RowSource::with_rows(vec![outcome_in_window()]));
        let scheduler = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log_for_spawn,
            backend,
            scheduler_audit_for_spawn,
            PAGE_ID,
            METRIC_ID,
        );
        scheduler.run_once(NOW_UNIX_S)
    })
    .await
    .expect("blocking task")
    .expect("happy path returns Ok");
    let is_pub = matches!(outcome, RunOutcome::Published { .. });
    assert!(is_pub);
    let snap = scheduler_audit.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].outcome, SchedulerAuditOutcome::Scheduled);
    assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Succeeded);
    assert_eq!(cron_log.snapshot().len(), 1);
}

#[tokio::test]
async fn empty_window_skips_with_no_http_call() {
    // No WireMock route mounted — if scheduler tries to publish the
    // server returns 404 and the wave-16 client treats it as a
    // PermanentReject. Asserting `cron_log` len + audit shape is
    // sufficient evidence the skip fired before any HTTP attempt.
    let server = MockServer::start().await;
    let stp_audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let url = server.uri();
    let scheduler_audit = Arc::new(InMemorySchedulerAuditSink::new());
    let cron_log = Arc::new(InMemoryCronRunLog::new());
    let scheduler_audit_for_spawn = scheduler_audit.clone();
    let cron_log_for_spawn = cron_log.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let client = build_client(url, stp_audit.clone());
        let backend: Arc<dyn StatuspageBackend> = Arc::new(client);
        let row_source = Arc::new(InMemoryD1RowSource::new());
        let scheduler = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log_for_spawn,
            backend,
            scheduler_audit_for_spawn,
            PAGE_ID,
            METRIC_ID,
        );
        (scheduler.run_once(NOW_UNIX_S), stp_audit)
    })
    .await
    .expect("blocking task");
    let run = outcome.0.expect("empty window returns Ok(Skipped)");
    let is_empty_skip = matches!(
        run,
        RunOutcome::Skipped {
            reason: SkipReason::EmptyWindow
        }
    );
    assert!(is_empty_skip);
    // No `Published` / `Failed` events on the Statuspage client audit
    // sink — the HTTP layer was never reached.
    assert!(outcome.1.is_empty());
    let snap = scheduler_audit.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Skipped);
    assert_eq!(snap[1].skip_reason, Some(SkipReason::EmptyWindow));
    // Ledger row inserted (seizes the day slot).
    assert_eq!(cron_log.snapshot().len(), 1);
}

#[tokio::test]
async fn statuspage_401_emits_failed_audit_and_returns_err() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "unauthorized",
        })))
        .expect(1)
        .mount(&server)
        .await;
    let stp_audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let url = server.uri();
    let scheduler_audit = Arc::new(InMemorySchedulerAuditSink::new());
    let cron_log = Arc::new(InMemoryCronRunLog::new());
    let scheduler_audit_for_spawn = scheduler_audit.clone();
    let cron_log_for_spawn = cron_log.clone();
    let result = tokio::task::spawn_blocking(move || {
        let client = build_client(url, stp_audit);
        let backend: Arc<dyn StatuspageBackend> = Arc::new(client);
        let row_source = Arc::new(InMemoryD1RowSource::with_rows(vec![outcome_in_window()]));
        let scheduler = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log_for_spawn,
            backend,
            scheduler_audit_for_spawn,
            PAGE_ID,
            METRIC_ID,
        );
        scheduler.run_once(NOW_UNIX_S)
    })
    .await
    .expect("blocking task");
    let err = result.expect_err("401 must produce Err");
    let is_auth = matches!(
        err,
        SchedulerError::Publish(StatuspageClientError::AuthFailed { .. })
    );
    assert!(is_auth);
    let snap = scheduler_audit.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].outcome, SchedulerAuditOutcome::Scheduled);
    assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Failed);
    // Ledger row recorded with `Failed` status — locks the day so the
    // CF runtime does NOT retry the same publish multiple times.
    assert_eq!(cron_log.snapshot().len(), 1);
}

#[tokio::test]
async fn rate_limit_429_exhausts_retries_emits_failed() {
    let server = MockServer::start().await;
    // WireMock returns 429 on every attempt; the wave-16 retry policy
    // exhausts on the 4th try (1 initial + 3 retries) and surfaces
    // `TransportExhausted`.
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/pages/{PAGE_ID}/metrics/{METRIC_ID}/data.json"
        )))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let stp_audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let url = server.uri();
    let scheduler_audit = Arc::new(InMemorySchedulerAuditSink::new());
    let cron_log = Arc::new(InMemoryCronRunLog::new());
    let scheduler_audit_for_spawn = scheduler_audit.clone();
    let cron_log_for_spawn = cron_log.clone();
    let result = tokio::task::spawn_blocking(move || {
        let client = build_client(url, stp_audit);
        let backend: Arc<dyn StatuspageBackend> = Arc::new(client);
        let row_source = Arc::new(InMemoryD1RowSource::with_rows(vec![outcome_in_window()]));
        let scheduler = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log_for_spawn,
            backend,
            scheduler_audit_for_spawn,
            PAGE_ID,
            METRIC_ID,
        );
        scheduler.run_once(NOW_UNIX_S)
    })
    .await
    .expect("blocking task");
    let err = result.expect_err("429 must exhaust retries");
    let is_exhausted = matches!(
        err,
        SchedulerError::Publish(StatuspageClientError::TransportExhausted { .. })
    );
    assert!(is_exhausted);
    let snap = scheduler_audit.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Failed);
    // Failed-day ledger row populated.
    assert_eq!(cron_log.snapshot().len(), 1);
}

#[tokio::test]
async fn d1_read_failure_emits_failed_and_returns_err() {
    let server = MockServer::start().await;
    let stp_audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let url = server.uri();
    let scheduler_audit = Arc::new(InMemorySchedulerAuditSink::new());
    let cron_log = Arc::new(InMemoryCronRunLog::new());
    let scheduler_audit_for_spawn = scheduler_audit.clone();
    let cron_log_for_spawn = cron_log.clone();
    let result = tokio::task::spawn_blocking(move || {
        let client = build_client(url, stp_audit);
        let backend: Arc<dyn StatuspageBackend> = Arc::new(client);
        // D1 read fails — scheduler must NOT reach the publish layer.
        let row_source = Arc::new(InMemoryD1RowSource::with_read_failure());
        let scheduler = DsrStatuspagePublishScheduler::new(
            row_source,
            cron_log_for_spawn,
            backend,
            scheduler_audit_for_spawn,
            PAGE_ID,
            METRIC_ID,
        );
        scheduler.run_once(NOW_UNIX_S)
    })
    .await
    .expect("blocking task");
    let err = result.expect_err("d1 read failure must produce Err");
    let is_row = matches!(err, SchedulerError::RowSource(_));
    assert!(is_row);
    let snap = scheduler_audit.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[1].outcome, SchedulerAuditOutcome::Failed);
    // Ledger NOT populated — failure happened BEFORE the seize point.
    assert_eq!(cron_log.snapshot().len(), 0);
}
