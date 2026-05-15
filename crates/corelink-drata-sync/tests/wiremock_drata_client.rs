//! Integration tests for [`DrataHttpClient`] using `wiremock`.
//!
//! Covers: 2xx receipt parsing, idempotency key header set, bearer
//! token sent, retry on 5xx, give-up on 4xx, transport exhaustion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use std::time::Duration;

use corelink_drata_sync::{
    record_sha256, DrataClient, DrataClientError, DrataHttpClient, EvidenceRecord, EvidenceStream,
    RetryPolicy,
};
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fast_sleeper(_: Duration) {}

async fn fresh_server() -> MockServer {
    MockServer::start().await
}

fn audit_logs_record() -> EvidenceRecord {
    EvidenceRecord::new(
        EvidenceStream::AuditLogs,
        "outbox#42",
        1_700_000_000_000,
        [("event_type", "corelink.auth.signin"), ("tenant_hash", "abc")],
    )
}

async fn build_and_push(
    base_url: String,
    api_key: String,
    record: EvidenceRecord,
) -> Result<corelink_drata_sync::DrataReceipt, DrataClientError> {
    let key = record_sha256(&record).expect("hash");
    tokio::task::spawn_blocking(move || {
        let client = DrataHttpClient::with_overrides(
            base_url,
            api_key,
            RetryPolicy::r5p_default(),
            fast_sleeper,
        )
        .expect("build client");
        client.push(&record, &key)
    })
    .await
    .expect("join")
}

#[tokio::test]
async fn success_returns_receipt_id_and_sets_bearer() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/audit-logs"))
        .and(header("Authorization", "Bearer drata_test_key_abc"))
        .and(header_exists("Idempotency-Key"))
        .respond_with(
            ResponseTemplate::new(201).set_body_string(r#"{"receipt_id":"rcp_42"}"#),
        )
        .mount(&server)
        .await;

    let receipt = build_and_push(
        server.uri(),
        "drata_test_key_abc".into(),
        audit_logs_record(),
    )
    .await
    .expect("ok");
    assert_eq!(receipt.receipt_id, "rcp_42");
    assert_eq!(receipt.http_status, 201);
}

#[tokio::test]
async fn falls_back_to_idem_receipt_when_body_plain_text() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/audit-logs"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let receipt = build_and_push(
        server.uri(),
        "drata_test_key_abc".into(),
        audit_logs_record(),
    )
    .await
    .expect("ok");
    assert!(receipt.receipt_id.starts_with("idem-"));
}

#[tokio::test]
async fn retries_on_5xx_then_succeeds() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/audit-logs"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/audit-logs"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"receipt_id":"rcp_after_retry"}"#),
        )
        .mount(&server)
        .await;

    let receipt = build_and_push(
        server.uri(),
        "drata_test_key_abc".into(),
        audit_logs_record(),
    )
    .await
    .expect("ok");
    assert_eq!(receipt.receipt_id, "rcp_after_retry");
}

#[tokio::test]
async fn permanent_4xx_is_not_retried() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/audit-logs"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let err = build_and_push(
        server.uri(),
        "drata_test_key_abc".into(),
        audit_logs_record(),
    )
    .await
    .expect_err("must be 4xx error");
    match err {
        DrataClientError::PermanentReject { status, .. } => assert_eq!(status, 403),
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test]
async fn transport_exhausted_after_max_retries() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/audit-logs"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let err = build_and_push(
        server.uri(),
        "drata_test_key_abc".into(),
        audit_logs_record(),
    )
    .await
    .expect_err("must exhaust");
    match err {
        DrataClientError::TransportExhausted { attempts, .. } => assert_eq!(attempts, 4),
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test]
async fn stream_endpoint_routing_is_correct() {
    let server = fresh_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/evidence/incident-response"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"receipt_id":"rcp_ir"}"#),
        )
        .expect(1)
        .mount(&server)
        .await;

    let record = EvidenceRecord::new(
        EvidenceStream::IncidentResponse,
        "pd#inc-1",
        1,
        [("severity", "sev2")],
    );
    let receipt = build_and_push(server.uri(), "k".repeat(20), record)
        .await
        .expect("ok");
    assert_eq!(receipt.receipt_id, "rcp_ir");
}
