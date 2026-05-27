//! R2-3 integration test — exercises [`HttpPagerDutyClient`] against a
//! `mockito` HTTP server fronting the same surface as PagerDuty's
//! Events API v2 `/v2/enqueue` endpoint.
//!
//! Requires the `production` feature to expose [`HttpPagerDutyClient`]
//! + [`ReqwestBlockingTransport`].

#![cfg(feature = "production")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use std::sync::{Arc, Mutex};

use corelink_ops::oncall::events::{
    EventAction, HttpPagerDutyClient, NoopClock, PageContext, PagerDutyAuditSink, PagerDutyEvent,
    ReqwestBlockingTransport, RoutingKey, SendOutcome,
};
use corelink_ops::oncall::severity::Severity;
use proptest::prelude::*;
use serde_json::Value as JsonValue;

#[derive(Debug, Default)]
struct CountingAudit {
    events: Arc<Mutex<Vec<String>>>,
}

impl PagerDutyAuditSink for CountingAudit {
    fn emit_audit(&self, event_type: &str, _details: &JsonValue) -> Result<(), String> {
        self.events.lock().unwrap().push(event_type.to_string());
        Ok(())
    }
}

fn mk_event(action: EventAction, dedup: &str) -> PagerDutyEvent {
    PagerDutyEvent::new(
        action,
        dedup,
        "integration",
        Severity::Sev1,
        PageContext::Production {
            component: "test".to_string(),
        },
        "corr-int",
        None,
    )
}

#[test]
fn http_trigger_succeeds_against_mockito() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v2/enqueue")
        .with_status(202)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"success","dedup_key":"k-mock"}"#)
        .expect(1)
        .create();

    let audit = CountingAudit::default();
    let events_handle = audit.events.clone();
    let endpoint = format!("{}/v2/enqueue", server.url());
    let client = HttpPagerDutyClient::with_parts(
        endpoint,
        RoutingKey::new("rk").unwrap(),
        Box::new(ReqwestBlockingTransport::new().unwrap()),
        Box::new(audit),
        Box::new(NoopClock),
    );
    let out = client.send(&mk_event(EventAction::Trigger, "k-mock")).unwrap();
    assert!(matches!(out, SendOutcome::Accepted { .. }));
    mock.assert();
    let recorded = events_handle.lock().unwrap();
    assert!(recorded
        .iter()
        .any(|e| e == "corelink.pagerduty.send_attempted"));
}

#[test]
fn http_429_then_success_against_mockito() {
    let mut server = mockito::Server::new();
    let _rate = server
        .mock("POST", "/v2/enqueue")
        .with_status(429)
        .with_header("retry-after", "0")
        .expect(1)
        .create();
    let _ok = server
        .mock("POST", "/v2/enqueue")
        .with_status(202)
        .with_body(r#"{"status":"success"}"#)
        .expect(1)
        .create();

    let endpoint = format!("{}/v2/enqueue", server.url());
    let client = HttpPagerDutyClient::with_parts(
        endpoint,
        RoutingKey::new("rk").unwrap(),
        Box::new(ReqwestBlockingTransport::new().unwrap()),
        Box::new(CountingAudit::default()),
        Box::new(NoopClock),
    );
    let out = client
        .send(&mk_event(EventAction::Trigger, "k-429"))
        .unwrap();
    match out {
        SendOutcome::Accepted { retries, .. } => assert!(retries >= 1),
        other => panic!("expected Accepted after retry, got {other:?}"),
    }
}

/// Property: every random `dedup_key` is serialised verbatim into the
/// wire envelope — PagerDuty treats the same key as the dedup target,
/// so idempotency holds regardless of input content. We round-trip
/// the JSON to assert the bit-for-bit preservation.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: proptest_cases(), ..ProptestConfig::default() })]
    #[test]
    fn prop_dedup_key_idempotent(
        dedup in "[a-zA-Z0-9_:.-]{1,64}",
    ) {
        let evt = mk_event(EventAction::Trigger, &dedup);
        let bytes = evt
            .to_wire_bytes(&RoutingKey::new("rk").unwrap())
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        prop_assert_eq!(v["dedup_key"].as_str().unwrap(), &dedup);
        prop_assert_eq!(v["event_action"].as_str().unwrap(), "trigger");
    }
}
