use super::*;
use crate::webhook::compute_signature;
use std::sync::Mutex;

const SECRET: &[u8] = b"whsec_prod_dispatch_unit";
const FIXED_TS: u64 = 1_715_000_000;

type FixtureBundle = (
    Arc<WebhookDispatcher>,
    Arc<InMemoryIdempotencyStore>,
    Arc<RecordingStateMaterializer>,
    Arc<RecordingAuditEmitter>,
    Arc<RecordingSliRecorder>,
);

fn fixture() -> FixtureBundle {
    let idem = Arc::new(InMemoryIdempotencyStore::new());
    let mat = Arc::new(RecordingStateMaterializer::new());
    let audit = Arc::new(RecordingAuditEmitter::new());
    let sli = Arc::new(RecordingSliRecorder::new());
    let clock = Arc::new(FixedClock::new(FIXED_TS, 0.123));
    let dispatcher = Arc::new(WebhookDispatcher::new(
        SECRET.to_vec(),
        idem.clone(),
        mat.clone(),
        audit.clone(),
        sli.clone(),
        clock,
    ));
    (dispatcher, idem, mat, audit, sli)
}

fn signed_envelope(id: &str, kind: &str, ts: u64) -> (Vec<u8>, String) {
    let body = serde_json::to_vec(&serde_json::json!({
        "id": id,
        "type": kind,
        "created": ts,
        "data": { "object": { "id": "sub_test", "status": "active" } }
    }))
    .unwrap();
    let sig = compute_signature(SECRET, ts, &body);
    let header = format!("t={ts},v1={sig}");
    (body, header)
}

#[derive(Debug, Default)]
struct LostAckInbox {
    state: Mutex<LostAckInboxState>,
}

#[derive(Debug, Default)]
struct LostAckInboxState {
    claimed: bool,
    terminal: bool,
    reserved: bool,
    fail_after_commit: bool,
}

impl LostAckInbox {
    fn fail_after_next_commit(&self) {
        self.state.lock().unwrap().fail_after_commit = true;
    }
}

impl DurableWebhookInbox for LostAckInbox {
    fn receive(
        &self,
        _event: &DurableWebhookEvent,
        _now_ms: u64,
    ) -> Result<InboxReceiveOutcome, String> {
        let state = self.state.lock().unwrap();
        if state.terminal {
            Ok(InboxReceiveOutcome::Terminal)
        } else {
            Ok(InboxReceiveOutcome::Received)
        }
    }

    fn claim(
        &self,
        event_id: &str,
        _owner: &str,
        _now_ms: u64,
        _lease_ms: u64,
    ) -> Result<Option<InboxClaim>, String> {
        let mut state = self.state.lock().unwrap();
        if state.claimed {
            Ok(None)
        } else {
            state.claimed = true;
            Ok(Some(InboxClaim::new(event_id.to_owned(), 1)))
        }
    }

    fn finish(
        &self,
        _claim: &InboxClaim,
        _owner: &str,
        _state: InboxTerminalState,
        _error: Option<&str>,
        _now_ms: u64,
    ) -> Result<bool, String> {
        Ok(false)
    }

    fn reserve_effect(
        &self,
        _claim: &InboxClaim,
        _owner: &str,
        _event: &DurableWebhookEvent,
        _effect_key: &str,
        _effect_kind: &str,
        _now_ms: u64,
    ) -> Result<EffectReservation, String> {
        let mut state = self.state.lock().unwrap();
        if state.terminal {
            Ok(EffectReservation::Applied)
        } else if state.reserved {
            Ok(EffectReservation::PendingRecovery)
        } else {
            state.reserved = true;
            Ok(EffectReservation::Reserved)
        }
    }

    fn abort_reserved_effect(
        &self,
        _claim: &InboxClaim,
        _owner: &str,
        _event: &DurableWebhookEvent,
        _effect_key: &str,
        _effect_kind: &str,
        _now_ms: u64,
    ) -> Result<bool, String> {
        self.state.lock().unwrap().reserved = false;
        Ok(true)
    }

    fn commit_effect(
        &self,
        _claim: &InboxClaim,
        _owner: &str,
        _event: &DurableWebhookEvent,
        _effect_key: &str,
        _effect_kind: &str,
        _now_ms: u64,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().unwrap();
        state.terminal = true;
        if state.fail_after_commit {
            state.fail_after_commit = false;
            return Err("simulated lost D1 response after atomic commit".to_owned());
        }
        Ok(true)
    }
}

#[test]
fn lost_ack_after_effect_commit_retries_as_terminal_without_reapplying() {
    let idem = Arc::new(InMemoryIdempotencyStore::new());
    let materializer = Arc::new(RecordingStateMaterializer::new());
    let inbox = Arc::new(LostAckInbox::default());
    inbox.fail_after_next_commit();
    let materializer_for_dispatch: Arc<dyn StateMaterializer> = materializer.clone();
    let dispatcher = WebhookDispatcher::new(
        SECRET.to_vec(),
        idem,
        materializer_for_dispatch,
        Arc::new(RecordingAuditEmitter::new()),
        Arc::new(RecordingSliRecorder::new()),
        Arc::new(FixedClock::new(FIXED_TS, 0.123)),
    )
    .with_durable_inbox(inbox);
    let (body, header) = signed_envelope("evt_lost_ack", "invoice.paid", FIXED_TS);

    assert_eq!(
        dispatcher.process(&body, Some(&header)),
        DispatchResponse::InternalError500,
        "a lost response remains retryable"
    );
    assert_eq!(
        dispatcher.process(&body, Some(&header)),
        DispatchResponse::Ok200,
        "only the durable terminal retry is acknowledged"
    );
    assert_eq!(
        materializer.call_count(),
        1,
        "the committed effect is never re-applied after a lost ACK"
    );
}
#[test]
fn post_mutation_failure_seals_pending_effect_without_a_second_apply() {
    let idem = Arc::new(InMemoryIdempotencyStore::new());
    let materializer = Arc::new(RecordingStateMaterializer::new());
    materializer.arm_error_after_record(MaterializerError::AppliedButUnconfirmed(
        "injected after durable business mutation".to_owned(),
    ));
    let materializer_for_dispatch: Arc<dyn StateMaterializer> = materializer.clone();
    let dispatcher = WebhookDispatcher::new(
        SECRET.to_vec(),
        idem,
        materializer_for_dispatch,
        Arc::new(RecordingAuditEmitter::new()),
        Arc::new(RecordingSliRecorder::new()),
        Arc::new(FixedClock::new(FIXED_TS, 0.123)),
    )
    .with_durable_inbox(Arc::new(LostAckInbox::default()));
    let (body, header) = signed_envelope("evt_post_mutation", "invoice.paid", FIXED_TS);

    assert_eq!(
        dispatcher.process(&body, Some(&header)),
        DispatchResponse::InternalError500
    );
    assert_eq!(
        dispatcher.process(&body, Some(&header)),
        DispatchResponse::Ok200
    );
    assert_eq!(
        materializer.call_count(),
        1,
        "pending witness fences replay"
    );
}
#[test]
fn happy_path_subscription_deleted_dispatches_audits_emits_sli() {
    let (d, idem, mat, audit, sli) = fixture();
    let (body, hdr) = signed_envelope("evt_d1", "customer.subscription.deleted", FIXED_TS);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Ok200);
    assert_eq!(mat.call_count(), 1);
    assert_eq!(idem.len(), 1);
    assert_eq!(audit.count_with_outcome(AuditOutcome::Dispatched), 1);
    assert_eq!(sli.count(), 1);
    assert_eq!(
        sli.observations()[0].metric_name,
        SLI_BILLING_STRIPE_EVENT_SECONDS
    );
    assert_eq!(
        sli.observations()[0].event_type,
        CanonicalWebhookEventType::SubscriptionDeleted
    );
}

#[test]
fn duplicate_event_acked_without_dispatch() {
    let (d, idem, mat, audit, _sli) = fixture();
    let (body, hdr) = signed_envelope("evt_dup", "invoice.paid", FIXED_TS);
    let r1 = d.process(&body, Some(&hdr));
    let r2 = d.process(&body, Some(&hdr));
    assert_eq!(r1, DispatchResponse::Ok200);
    assert_eq!(r2, DispatchResponse::Ok200);
    assert_eq!(mat.call_count(), 1, "no double dispatch");
    assert_eq!(idem.len(), 1);
    assert_eq!(audit.count_with_outcome(AuditOutcome::Duplicate), 1);
    assert_eq!(audit.count_with_outcome(AuditOutcome::Dispatched), 1);
}

#[test]
fn invalid_signature_returns_401_with_no_dispatch() {
    let (d, idem, mat, audit, _sli) = fixture();
    let (body, _) = signed_envelope("evt_evil", "invoice.paid", FIXED_TS);
    let bogus_header = format!("t={FIXED_TS},v1=deadbeefdeadbeefdeadbeefdeadbeef");
    let resp = d.process(&body, Some(&bogus_header));
    assert_eq!(resp, DispatchResponse::Unauthorized401);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

#[test]
fn missing_signature_header_returns_400() {
    let (d, _, mat, audit, _sli) = fixture();
    let (body, _) = signed_envelope("evt_x", "invoice.paid", FIXED_TS);
    let resp = d.process(&body, None);
    assert_eq!(resp, DispatchResponse::BadRequest400);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

#[test]
fn malformed_envelope_returns_422() {
    let (d, idem, mat, audit, _sli) = fixture();
    // Sign garbage that's NOT valid JSON.
    let body = b"not-json-at-all";
    let sig = compute_signature(SECRET, FIXED_TS, body);
    let hdr = format!("t={FIXED_TS},v1={sig}");
    let resp = d.process(body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Unprocessable422);
    assert_eq!(mat.call_count(), 0);
    assert_eq!(idem.len(), 0);
    assert_eq!(audit.count_with_outcome(AuditOutcome::EnvelopeInvalid), 1);
}

#[test]
fn replay_window_exceeded_returns_401() {
    let (d, _, _, audit, _sli) = fixture();
    // Sign 10 minutes in the past — > 300s tolerance.
    let old_ts = FIXED_TS - 600;
    let (body, hdr) = signed_envelope("evt_old", "invoice.paid", old_ts);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Unauthorized401);
    assert_eq!(audit.count_with_outcome(AuditOutcome::SignatureInvalid), 1);
}

#[test]
fn materializer_transient_failure_returns_500() {
    let (d, _, mat, audit, _sli) = fixture();
    mat.arm_error(MaterializerError::Transient("d1 unavailable".to_string()));
    let (body, hdr) = signed_envelope("evt_t1", "invoice.paid", FIXED_TS);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::InternalError500);
    assert_eq!(
        audit.count_with_outcome(AuditOutcome::MaterializerFailed),
        1
    );
}

#[test]
fn transient_failure_quarantines_then_retry_remains_in_dlq_for_replay() {
    // F-008 regression. The dedup row is committed BEFORE materialize,
    // so a transient materialize failure followed by a Stripe retry
    // (which short-circuits as AlreadyProcessed) would PERMANENTLY
    // DROP the state change. With the DLQ wired the event is captured
    // and stays operator-replayable.
    use crate::dlq::{DlqReplayOutcome, InMemoryWebhookDlqStore};

    let idem = Arc::new(InMemoryIdempotencyStore::new());
    let mat = Arc::new(RecordingStateMaterializer::new());
    let audit = Arc::new(RecordingAuditEmitter::new());
    let sli = Arc::new(RecordingSliRecorder::new());
    let dlq = Arc::new(InMemoryWebhookDlqStore::new());
    let clock = Arc::new(FixedClock::new(FIXED_TS, 0.123));
    let d = Arc::new(
        WebhookDispatcher::new(
            SECRET.to_vec(),
            idem.clone(),
            mat.clone(),
            audit.clone(),
            sli,
            clock,
        )
        .with_dlq(dlq.clone()),
    );

    let (body, hdr) = signed_envelope("evt_drop", "customer.subscription.deleted", FIXED_TS);

    // 1st delivery: materialize transiently fails → 500 + quarantine.
    mat.arm_error(MaterializerError::Transient(
        "d1 over-http blip".to_string(),
    ));
    let r1 = d.process(&body, Some(&hdr));
    assert_eq!(r1, DispatchResponse::InternalError500);
    // Dedup row IS committed (the F-008 root cause).
    assert_eq!(idem.len(), 1);
    // The event is now quarantined (NOT lost).
    let now_ms = FIXED_TS.saturating_mul(1_000);
    assert_eq!(
        dlq.depth(now_ms).unwrap(),
        1,
        "event quarantined on transient"
    );
    let row = dlq.get("evt_drop").unwrap().expect("dlq row present");
    assert_eq!(row.event_type, "customer.subscription.deleted");
    assert_eq!(row.attempt_count, 1);

    // 2nd delivery (Stripe retry): the dedup row short-circuits as
    // AlreadyProcessed → handler is SKIPPED (this is the silent-drop
    // window) → still 200 to Stripe, but the DLQ retains the event so
    // it can be replayed by an operator.
    let r2 = d.process(&body, Some(&hdr));
    assert_eq!(r2, DispatchResponse::Ok200);
    assert_eq!(mat.call_count(), 0, "retry short-circuits, no re-dispatch");
    assert_eq!(
        dlq.depth(now_ms).unwrap(),
        1,
        "event still recoverable via DLQ after the AlreadyProcessed retry"
    );

    // Operator replay drains the depth (proves the recovery path).
    dlq.record_replay(
        "evt_drop",
        "rep_1",
        "ops_oncall",
        now_ms + 5_000,
        DlqReplayOutcome::Succeeded,
    )
    .unwrap();
    assert_eq!(dlq.depth(now_ms + 6_000).unwrap(), 0);
}

#[test]
fn transient_failure_without_dlq_returns_500_and_does_not_panic() {
    // Back-compat: the no-DLQ path is unchanged (500, no quarantine
    // sink to consult). Asserts `with_dlq` is purely additive.
    let (d, _, mat, audit, _sli) = fixture();
    mat.arm_error(MaterializerError::Transient("d1 unavailable".to_string()));
    let (body, hdr) = signed_envelope("evt_nodlq", "invoice.paid", FIXED_TS);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::InternalError500);
    assert_eq!(
        audit.count_with_outcome(AuditOutcome::MaterializerFailed),
        1
    );
}

#[test]
fn materializer_invalid_payload_returns_422() {
    let (d, _, mat, audit, _sli) = fixture();
    mat.arm_error(MaterializerError::InvalidPayload(
        "missing customer".to_string(),
    ));
    let (body, hdr) = signed_envelope("evt_t2", "invoice.paid", FIXED_TS);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Unprocessable422);
    assert_eq!(
        audit.count_with_outcome(AuditOutcome::MaterializerInvalid),
        1
    );
}

#[test]
fn unknown_event_acked_without_dispatch() {
    let (d, idem, mat, audit, _sli) = fixture();
    let (body, hdr) = signed_envelope("evt_new", "stripe.future.type", FIXED_TS);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::Ok200);
    assert_eq!(mat.call_count(), 0);
    // Unknown DOES insert dedup row (so a retry hits duplicate).
    assert_eq!(idem.len(), 1);
    assert_eq!(audit.count_with_outcome(AuditOutcome::UnknownEventType), 1);
}

#[test]
fn observability_echo_events_acked_without_state_mutation() {
    for kind in [
        "customer.subscription.created",
        "customer.subscription.trial_will_end",
        "customer.created",
        "invoice.created",
    ] {
        let (d, _, mat, audit, _sli) = fixture();
        let (body, hdr) = signed_envelope(&format!("evt_{kind}"), kind, FIXED_TS);
        let resp = d.process(&body, Some(&hdr));
        assert_eq!(resp, DispatchResponse::Ok200, "kind={kind}");
        // The remaining echo events DO NOT call materializer methods.
        assert_eq!(mat.call_count(), 0, "kind={kind}");
        assert_eq!(
            audit.count_with_outcome(AuditOutcome::Dispatched),
            1,
            "kind={kind}"
        );
    }
}

#[test]
fn blake3_idempotency_token_stable_per_event_id() {
    let t1 = IdempotencyToken::from_event_id("evt_001");
    let t2 = IdempotencyToken::from_event_id("evt_001");
    let t3 = IdempotencyToken::from_event_id("evt_002");
    assert_eq!(t1, t2);
    assert_ne!(t1, t3);
    assert_eq!(t1.to_hex().len(), 64); // 32 bytes hex
}

#[test]
fn classify_matches_all_ten_sla_event_types() {
    for canon in CanonicalWebhookEventType::sla_event_types() {
        assert_eq!(CanonicalWebhookEventType::classify(canon.label()), canon);
    }
}

#[test]
fn state_mutator_flag_marks_canonical_six_including_refund() {
    let mut mutators = 0;
    for canon in CanonicalWebhookEventType::sla_event_types() {
        if canon.is_state_mutator() {
            mutators += 1;
        }
    }
    assert_eq!(mutators, 6);
}

#[test]
fn idempotency_outcome_already_processed_short_circuits_audit_outcome() {
    let (d, _idem, _mat, audit, sli) = fixture();
    let (body, hdr) = signed_envelope("evt_short", "invoice.paid", FIXED_TS);
    d.process(&body, Some(&hdr));
    d.process(&body, Some(&hdr));
    assert_eq!(audit.count_with_outcome(AuditOutcome::Duplicate), 1);
    // SLI emitted on BOTH calls (latency observed even for duplicate).
    assert_eq!(sli.count(), 2);
}

#[test]
fn audit_emit_failure_propagates_500() {
    let (d, _, _, audit, _sli) = fixture();
    audit.fail_with("audit chain unavailable");
    let (body, hdr) = signed_envelope("evt_a", "invoice.paid", FIXED_TS);
    let resp = d.process(&body, Some(&hdr));
    assert_eq!(resp, DispatchResponse::InternalError500);
}
