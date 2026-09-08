use super::*;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct CapturingAudit {
    events: Arc<Mutex<Vec<(String, JsonValue)>>>,
    fail: bool,
}

impl CapturingAudit {
    fn new() -> Self {
        Self::default()
    }
    fn failing() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        }
    }
    fn snapshot(&self) -> Vec<(String, JsonValue)> {
        self.events.lock().unwrap().clone()
    }
}

impl PagerDutyAuditSink for CapturingAudit {
    fn emit_audit(&self, event_type: &str, details: &JsonValue) -> Result<(), String> {
        if self.fail {
            return Err("audit refused".to_string());
        }
        self.events
            .lock()
            .unwrap()
            .push((event_type.to_string(), details.clone()));
        Ok(())
    }
}

type CallLog = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
type ResponseQueue = Arc<Mutex<Vec<Result<HttpResponse, String>>>>;

#[derive(Debug)]
struct ScriptedTransport {
    responses: ResponseQueue,
    calls: CallLog,
}

impl ScriptedTransport {
    fn new(responses: Vec<Result<HttpResponse, String>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    fn last_body(&self) -> Option<Vec<u8>> {
        self.calls.lock().unwrap().last().map(|(_, b)| b.clone())
    }
    fn calls_to(&self, url: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, _)| u == url)
            .count()
    }
}

impl HttpTransport for ScriptedTransport {
    fn post(&self, url: &str, body: &[u8]) -> Result<HttpResponse, String> {
        self.calls
            .lock()
            .unwrap()
            .push((url.to_string(), body.to_vec()));
        let mut g = self.responses.lock().unwrap();
        if g.is_empty() {
            return Err("scripted transport exhausted".to_string());
        }
        g.remove(0)
    }
}

fn ok_response() -> HttpResponse {
    HttpResponse {
        status: 202,
        body: br#"{"status":"success","dedup_key":"abc"}"#.to_vec(),
        retry_after_secs: None,
    }
}
fn status_response(s: u16) -> HttpResponse {
    HttpResponse {
        status: s,
        body: Vec::new(),
        retry_after_secs: None,
    }
}

fn mk_client(
    transport: Box<dyn HttpTransport>,
    audit: Box<dyn PagerDutyAuditSink>,
) -> HttpPagerDutyClient {
    HttpPagerDutyClient::with_parts(
        "http://test.local/v2/enqueue".to_string(),
        RoutingKey::new("rk-prod").unwrap(),
        transport,
        audit,
        Box::new(NoopClock),
    )
}

fn prod_event(action: EventAction, dedup: &str, sev: Severity) -> PagerDutyEvent {
    PagerDutyEvent {
        action,
        dedup_key: dedup.to_string(),
        summary: "test alert".to_string(),
        severity: sev,
        context: PageContext::Production {
            component: "oncall".to_string(),
        },
        correlation_id: "corr-1".to_string(),
        tenant_id_hash: Some("th".to_string()),
    }
}

fn synth_event(action: EventAction, dedup: &str) -> PagerDutyEvent {
    PagerDutyEvent {
        action,
        dedup_key: dedup.to_string(),
        summary: "drill".to_string(),
        severity: Severity::Sev2,
        context: PageContext::Synthetic {
            region: "americas".to_string(),
        },
        correlation_id: "corr-synth".to_string(),
        tenant_id_hash: None,
    }
}

#[test]
fn trigger_success_no_retry() {
    let t = Arc::new(ScriptedTransport::new(vec![Ok(ok_response())]));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    );
    let out = client
        .send(&prod_event(EventAction::Trigger, "k1", Severity::Sev1))
        .unwrap();
    assert!(matches!(
        out,
        SendOutcome::Accepted { ref dedup_key, retries: 0 } if dedup_key == "k1"
    ));
    // audit fires BEFORE the http call.
    let events = audit.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "corelink.pagerduty.send_attempted");
}

#[test]
fn acknowledge_reuses_dedup_key() {
    let t = Arc::new(ScriptedTransport::new(vec![
        Ok(ok_response()),
        Ok(ok_response()),
    ]));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    );
    client
        .send(&prod_event(EventAction::Trigger, "shared", Severity::Sev1))
        .unwrap();
    client
        .send(&prod_event(
            EventAction::Acknowledge,
            "shared",
            Severity::Sev1,
        ))
        .unwrap();
    // both http calls used the same dedup_key in the body.
    let calls = t.calls.lock().unwrap();
    for (_, body) in calls.iter() {
        let v: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(v["dedup_key"], "shared");
    }
}

#[test]
fn resolve_clears_with_same_dedup_key() {
    let t = Arc::new(ScriptedTransport::new(vec![Ok(ok_response())]));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    );
    let out = client
        .send(&prod_event(EventAction::Resolve, "rk", Severity::Sev2))
        .unwrap();
    assert!(matches!(out, SendOutcome::Accepted { .. }));
    let body = t.last_body().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["event_action"], "resolve");
    assert_eq!(v["dedup_key"], "rk");
}

#[test]
fn severity_mapping_canonical() {
    let prod = PageContext::Production {
        component: "x".to_string(),
    };
    assert_eq!(map_severity(Severity::Sev0, &prod), "critical");
    assert_eq!(map_severity(Severity::Sev1, &prod), "critical");
    assert_eq!(map_severity(Severity::Sev2, &prod), "error");
    assert_eq!(map_severity(Severity::Sev3, &prod), "warning");
    let synth = PageContext::Synthetic {
        region: "emea".to_string(),
    };
    // Synthetic ALWAYS maps to info — never critical.
    assert_eq!(map_severity(Severity::Sev0, &synth), "info");
    assert_eq!(map_severity(Severity::Sev1, &synth), "info");
    assert_eq!(map_severity(Severity::Sev2, &synth), "info");
}

#[test]
fn synthetic_uses_different_routing_key_and_service() {
    let t = Arc::new(ScriptedTransport::new(vec![Ok(ok_response())]));
    let audit = Arc::new(CapturingAudit::new());
    let client = HttpPagerDutyClient::with_parts(
        "http://test.local/v2/enqueue".to_string(),
        RoutingKey::new("rk-prod").unwrap(),
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
        Box::new(NoopClock),
    )
    .with_synthetic_routing_key(RoutingKey::new("rk-synth").unwrap());

    client
        .send(&synth_event(EventAction::Trigger, "drill-1"))
        .unwrap();
    let body = t.last_body().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["routing_key"], "rk-synth");
    assert_eq!(v["payload"]["source"], "corelink-synthetic-drill-americas");
    assert_eq!(v["payload"]["component"], "synthetic-pager");
    assert_eq!(v["payload"]["severity"], "info");
    assert_eq!(v["payload"]["class"], "synthetic_drill");
}

#[test]
fn rate_limit_429_retries_then_succeeds() {
    let responses = vec![
        Ok(HttpResponse {
            status: 429,
            body: Vec::new(),
            retry_after_secs: Some(1),
        }),
        Ok(status_response(503)),
        Ok(ok_response()),
    ];
    let t = Arc::new(ScriptedTransport::new(responses));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    );
    let out = client
        .send(&prod_event(EventAction::Trigger, "rl", Severity::Sev1))
        .unwrap();
    assert!(matches!(
        out,
        SendOutcome::Accepted { retries, .. } if retries == 2
    ));
    assert_eq!(t.call_count(), 3);
}

#[test]
fn network_error_trigger_falls_back_to_slack() {
    let responses: Vec<Result<HttpResponse, String>> = (0..(MAX_RETRIES + 1))
        .map(|_| Err::<HttpResponse, _>("network down".to_string()))
        .chain(std::iter::once(Ok(status_response(200)))) // slack
        .collect();
    let t = Arc::new(ScriptedTransport::new(responses));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    )
    .with_slack_fallback("http://slack.test/hook");

    let out = client
        .send(&prod_event(EventAction::Trigger, "down", Severity::Sev1))
        .unwrap();
    assert!(matches!(out, SendOutcome::FallbackToSlack { .. }));
    // 1 slack call after 6 (MAX_RETRIES+1) PD attempts.
    assert_eq!(t.calls_to("http://slack.test/hook"), 1);
    // audit recorded the failure.
    let events = audit.snapshot();
    assert!(events
        .iter()
        .any(|(t, _)| t == "pagerduty.send_failed_after_retries"));
}

#[test]
fn payload_too_large_rejected_locally() {
    let big = "x".repeat(PAYLOAD_MAX_BYTES + 10);
    let evt = PagerDutyEvent {
        action: EventAction::Trigger,
        dedup_key: "big".to_string(),
        summary: big,
        severity: Severity::Sev2,
        context: PageContext::Production {
            component: "c".to_string(),
        },
        correlation_id: "corr".to_string(),
        tenant_id_hash: None,
    };
    let t = Arc::new(ScriptedTransport::new(Vec::new()));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    );
    let err = client.send(&evt).unwrap_err();
    assert!(matches!(err, OncallPagerDutyError::Transport(_)));
    // size check fires BEFORE the audit emit (lookup phase).
    assert_eq!(audit.snapshot().len(), 0);
    assert_eq!(t.call_count(), 0);
}

#[test]
fn audit_failure_aborts_http_call_fail_closed() {
    let t = Arc::new(ScriptedTransport::new(vec![Ok(ok_response())]));
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit::failing()),
    );
    let err = client
        .send(&prod_event(EventAction::Trigger, "k", Severity::Sev1))
        .unwrap_err();
    assert!(matches!(err, OncallPagerDutyError::Transport(_)));
    assert_eq!(t.call_count(), 0, "no HTTP call when audit fails");
}

#[test]
fn routing_key_debug_redacts_secret() {
    let k = RoutingKey::new("super-secret-key").unwrap();
    let dbg = format!("{k:?}");
    assert!(!dbg.contains("super-secret-key"));
    assert!(dbg.contains("***"));
}

#[test]
fn routing_key_from_env_rejects_blank() {
    std::env::set_var("PAGERDUTY_ROUTING_KEY", "");
    let r = RoutingKey::from_env();
    std::env::remove_var("PAGERDUTY_ROUTING_KEY");
    assert!(r.is_err());
}

#[test]
fn backoff_respects_retry_after_header() {
    let w = backoff_wait(0, Some(2));
    assert_eq!(w, Duration::from_millis(2000));
    let w_cap = backoff_wait(0, Some(10_000));
    assert!(w_cap <= Duration::from_millis(BACKOFF_CAP_MS));
}

#[test]
fn no_pii_in_payload() {
    // tenant_id, email, customer name must NEVER appear; only the
    // hashed id + correlation_id are emitted.
    let evt = prod_event(EventAction::Trigger, "k", Severity::Sev1);
    let rendered = evt.to_wire_bytes(&RoutingKey::new("rk").unwrap()).unwrap();
    let s = String::from_utf8(rendered).unwrap();
    assert!(s.contains("tenant_id_hash"));
    assert!(!s.contains("\"tenant_id\":"));
    assert!(!s.contains("@")); // no email-shaped value
}

#[test]
fn fatal_4xx_does_not_retry() {
    let responses = vec![Ok(status_response(400))];
    let t = Arc::new(ScriptedTransport::new(responses));
    let audit = Arc::new(CapturingAudit::new());
    let client = mk_client(
        Box::new(ScriptedTransport {
            responses: t.responses.clone(),
            calls: t.calls.clone(),
        }),
        Box::new(CapturingAudit {
            events: audit.events.clone(),
            fail: false,
        }),
    );
    let err = client
        .send(&prod_event(EventAction::Trigger, "k", Severity::Sev1))
        .unwrap_err();
    assert!(matches!(err, OncallPagerDutyError::Transport(_)));
    assert_eq!(t.call_count(), 1);
}
