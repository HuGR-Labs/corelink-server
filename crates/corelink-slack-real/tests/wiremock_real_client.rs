//! Integration tests for [`SlackHttpClient`] using `wiremock`.
//!
//! Seven tests covering: header render, field escape, thread_ts,
//! retry on 5xx, give-up on 4xx, channel routing, retry exhaustion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use std::sync::Arc;
use std::time::Duration;

use corelink_slack_real::{
    InMemorySlackAuditSink, MessageTemplate, RetryPolicy, SendOutcome, SharedSlackClient,
    SlackChannel, SlackClientError, SlackHttpClient, SlackMessage, WebhookRegistry,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fast_sleeper(_: Duration) {}

async fn fresh_server() -> MockServer {
    MockServer::start().await
}

fn registry_for(channel: SlackChannel, url: &str) -> WebhookRegistry {
    let mut r = WebhookRegistry::new();
    r.insert(channel, url);
    r
}

/// Build + send entirely inside `spawn_blocking` so the
/// `reqwest::blocking::Client`'s internal tokio runtime is constructed
/// AND dropped on a blocking thread (never inside the async test
/// runtime where dropping a runtime is forbidden).
async fn build_and_send(
    registry: WebhookRegistry,
    audit: Arc<InMemorySlackAuditSink>,
    msg: SlackMessage,
) -> Result<SendOutcome, SlackClientError> {
    tokio::task::spawn_blocking(move || {
        let client = SlackHttpClient::with_overrides(
            registry,
            audit,
            RetryPolicy::r2_4_default(),
            fast_sleeper,
        )
        .unwrap();
        client.send(&msg)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn header_rendered_into_block_kit() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let url = format!("{}/webhook", server.uri());
    let audit = Arc::new(InMemorySlackAuditSink::new());
    let msg = SlackMessage::new(
        SlackChannel::AlertsSev1,
        "Hello SEV1",
        "footer",
        "2026-05-14T00:00:00Z",
    );
    let outcome = build_and_send(
        registry_for(SlackChannel::AlertsSev1, &url),
        audit.clone(),
        msg,
    )
    .await
    .unwrap();
    assert_eq!(outcome.status, 200);

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    let header = body
        .get("blocks")
        .and_then(|b| b.as_array())
        .and_then(|a| a.first())
        .and_then(|h| h.get("text"))
        .and_then(|t| t.get("text"))
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(header, "Hello SEV1");
}

#[tokio::test]
async fn fields_are_escaped_no_mrkdwn_injection() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let url = format!("{}/webhook", server.uri());
    let audit = Arc::new(InMemorySlackAuditSink::new());
    let msg = SlackMessage::new(
        SlackChannel::EnterpriseInquiries,
        "h",
        "f",
        "2026-05-14T00:00:00Z",
    )
    .with_field("note", "*BOLD* <a> &b _underscore_");

    build_and_send(
        registry_for(SlackChannel::EnterpriseInquiries, &url),
        audit,
        msg,
    )
    .await
    .unwrap();

    let received = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    let blocks = body.get("blocks").unwrap().as_array().unwrap();
    let section = &blocks[1];
    let fields = section.get("fields").unwrap().as_array().unwrap();
    let text = fields[0].get("text").unwrap().as_str().unwrap();
    assert!(!text.contains("*BOLD*"));
    assert!(text.contains("\\*BOLD\\*"));
    assert!(text.contains("&lt;a&gt;"));
    assert!(text.contains("&amp;b"));
    assert!(text.contains("\\_underscore\\_"));
}

#[tokio::test]
async fn thread_ts_round_trips() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let url = format!("{}/webhook", server.uri());
    let audit = Arc::new(InMemorySlackAuditSink::new());
    let msg =
        SlackMessage::new(SlackChannel::AlertsSev2, "h", "f", "t").in_thread("1715607600.000100");
    build_and_send(registry_for(SlackChannel::AlertsSev2, &url), audit, msg)
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(
        body.get("thread_ts").unwrap().as_str().unwrap(),
        "1715607600.000100"
    );
}

#[tokio::test]
async fn retries_on_5xx_then_succeeds() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let url = format!("{}/webhook", server.uri());
    let audit = Arc::new(InMemorySlackAuditSink::new());
    let msg = MessageTemplate::AlertSev1 {
        service: "edge".into(),
        summary: "5xx spike".into(),
        runbook: "https://rb".into(),
        pager_link: "https://pd".into(),
        timestamp_iso: "2026-05-14T00:00:00Z".into(),
    }
    .render();

    let outcome = build_and_send(
        registry_for(SlackChannel::AlertsSev1, &url),
        audit.clone(),
        msg,
    )
    .await
    .unwrap();
    assert_eq!(outcome.status, 200);
    assert_eq!(outcome.attempts, 3);
    assert_eq!(audit.len(), 1);
    assert_eq!(
        audit.snapshot()[0].outcome,
        corelink_slack_real::SlackAuditOutcome::Sent
    );
}

#[tokio::test]
async fn give_up_on_4xx_no_retry() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let url = format!("{}/webhook", server.uri());
    let audit = Arc::new(InMemorySlackAuditSink::new());
    let msg = SlackMessage::new(SlackChannel::BreachNotifications, "h", "f", "t");
    let err = build_and_send(
        registry_for(SlackChannel::BreachNotifications, &url),
        audit.clone(),
        msg,
    )
    .await
    .unwrap_err();
    match err {
        SlackClientError::PermanentReject { status, .. } => assert_eq!(status, 403),
        other => panic!("expected PermanentReject, got {other:?}"),
    }
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(audit.len(), 1);
    assert_eq!(
        audit.snapshot()[0].outcome,
        corelink_slack_real::SlackAuditOutcome::Failed
    );
    // Webhook URL in audit must be redacted (we used a 127.0.0.1 URL so
    // the redactor falls back to "<redacted>").
    assert!(audit.snapshot()[0]
        .webhook_redacted
        .starts_with("<redacted>"));
}

#[tokio::test]
async fn channel_routing_directs_to_correct_webhook() {
    let sev1 = fresh_server().await;
    let sev2 = fresh_server().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&sev1)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&sev2)
        .await;

    let mut registry = WebhookRegistry::new();
    registry.insert(SlackChannel::AlertsSev1, format!("{}/sev1", sev1.uri()));
    registry.insert(SlackChannel::AlertsSev2, format!("{}/sev2", sev2.uri()));
    let audit = Arc::new(InMemorySlackAuditSink::new());

    let msg = SlackMessage::new(SlackChannel::AlertsSev1, "h", "f", "t");
    build_and_send(registry, audit, msg).await.unwrap();

    assert_eq!(sev1.received_requests().await.unwrap().len(), 1);
    assert_eq!(sev2.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn exhausts_retries_on_persistent_5xx() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let url = format!("{}/webhook", server.uri());
    let audit = Arc::new(InMemorySlackAuditSink::new());
    let msg = SlackMessage::new(SlackChannel::OncallHandoff, "h", "f", "t");
    let err = build_and_send(
        registry_for(SlackChannel::OncallHandoff, &url),
        audit.clone(),
        msg,
    )
    .await
    .unwrap_err();
    match err {
        SlackClientError::TransportExhausted { attempts, .. } => assert_eq!(attempts, 4),
        other => panic!("expected TransportExhausted, got {other:?}"),
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
    assert_eq!(
        audit.snapshot()[0].outcome,
        corelink_slack_real::SlackAuditOutcome::Failed
    );
}
