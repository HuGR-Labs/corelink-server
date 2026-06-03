//! Property tests for WI-S11-005 sub-processor emit crate.
//!
//! PROPTEST_CASES: runtime fn per S-07 P1-2 lesson.
//! prop_assert! pattern: let valid = matches!(...); prop_assert!(valid);
//! NEVER prop_assert!(matches!(...)).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy::sub_processor::{
    audit::{FailingSubProcessorAuditSink, InMemorySubProcessorAuditSink},
    broadcast::{BroadcastStore, InMemoryBroadcastStore},
    dkim::{derive_dkim_key, dkim_keys_differ},
    emitter::{
        make_broadcast_entry, ChangeRequest, InMemorySubProcessorEmitter, ObjectionRequest,
        PublishRequest, ResolutionRequest, SubProcessorEmitter,
    },
    event::{
        canonical_objection_statuses, hash_recipient_email, EmailLocale, ObjectionDecision,
        ObjectionPayload, ObjectionTicketStatus, SubProcessorChangedPayload, SubProcessorDiff,
        SubProcessorInfo, SubProcessorPublishedPayload,
    },
    objection::{InMemoryObjectionStore, ObjectionStore},
};

use proptest::prelude::*;

/// Runtime PROPTEST_CASES — NEVER compile-time const (S-07 P1-2).
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn make_emitter() -> InMemorySubProcessorEmitter {
    InMemorySubProcessorEmitter::new(
        Arc::new(InMemorySubProcessorAuditSink::new()),
        Arc::new(InMemoryBroadcastStore::new()),
        Arc::new(InMemoryObjectionStore::new()),
    )
}

fn sample_sp_info(id: &str) -> SubProcessorInfo {
    SubProcessorInfo {
        id: id.to_owned(),
        name: format!("{id} Corp"),
        role: "Infrastructure provider".into(),
        data_categories_processed: vec!["account_metadata".into()],
        region: "Multi-region".into(),
        certifications: vec!["SOC 2 Type II".into()],
        dpa_url: format!("https://{id}.example/dpa"),
        primary_jurisdiction: "United States".into(),
        contract_signed_at: "2026-04-23".into(),
    }
}

fn sample_publish_request() -> PublishRequest {
    PublishRequest {
        source: "corelink/cd-pipeline".into(),
        event_id: "01JTEST000PUBLISHED000001".into(),
        timestamp: "2026-05-13T00:00:00Z".into(),
        region: "weur".into(),
        payload: SubProcessorPublishedPayload {
            version: "1.0.0".into(),
            published_at: "2026-05-13T00:00:00Z".into(),
            sub_processors: vec![sample_sp_info("cloudflare")],
        },
    }
}

fn sample_change_request(broadcast_entries_count: usize) -> ChangeRequest {
    let entries: Vec<_> = (0..broadcast_entries_count)
        .map(|i| {
            make_broadcast_entry(
                &format!("LOG{i:0>20}"),
                "BROADCAST01234567890123456",
                "2.0.0",
                &format!("tenant-{i}"),
                &hash_recipient_email(&format!("user-{i}@example.com")),
                EmailLocale::EnUs,
                "2026-05-13T00:00:00Z",
            )
        })
        .collect();

    ChangeRequest {
        source: "corelink/cd-pipeline".into(),
        event_id: "01JTEST000CHANGED000000001".into(),
        timestamp: "2026-05-13T01:00:00Z".into(),
        region: "weur".into(),
        payload: SubProcessorChangedPayload {
            old_version: "1.0.0".into(),
            new_version: "2.0.0".into(),
            changed_at: "2026-05-13T01:00:00Z".into(),
            diff: SubProcessorDiff {
                added: vec!["sentry".into()],
                removed: vec![],
                modified: vec![],
            },
            broadcast_trigger_ts: "2026-05-13T01:00:00Z".into(),
        },
        broadcast_entries: entries,
    }
}

fn sample_objection_request(tenant: &str, subject: &str, sp_id: &str) -> ObjectionRequest {
    ObjectionRequest {
        source: "corelink/objection-handler".into(),
        event_id: "01JTEST000OBJECTION000001".into(),
        timestamp: "2026-05-13T02:00:00Z".into(),
        region: "weur".into(),
        objection_id: format!("OBJ{tenant}{subject}{sp_id}12345678901"),
        payload: ObjectionPayload {
            tenant_id: tenant.to_owned(),
            subject_id: subject.to_owned(),
            sub_processor_id: sp_id.to_owned(),
            sub_processors_version: "2.0.0".into(),
            objection_reason: "GDPR data residency concern".into(),
            proposed_alternative: Some("Self-hosted".into()),
            filed_at: "2026-05-13T02:00:00Z".into(),
        },
        subject_id_hash: hash_recipient_email(subject),
        expected_resolution_at: "2026-05-27T02:00:00Z".into(),
    }
}

// ─── AC-001: publish emits audit event, no broadcast ────────────────────────

#[test]
fn test_publish_emits_one_audit_event() {
    let sink = Arc::new(InMemorySubProcessorAuditSink::new());
    let broadcast_store = Arc::new(InMemoryBroadcastStore::new());
    let objection_store = Arc::new(InMemoryObjectionStore::new());
    let emitter =
        InMemorySubProcessorEmitter::new(sink.clone(), broadcast_store.clone(), objection_store);
    emitter
        .publish(sample_publish_request())
        .expect("publish must succeed");
    assert_eq!(sink.len(), 1, "publish must emit exactly 1 audit event");
    assert!(
        broadcast_store.is_empty(),
        "publish must NOT seed broadcast log"
    );
}

// ─── AC-007: fail-CLOSED on audit emit failure ───────────────────────────────

#[test]
fn test_publish_fail_closed_on_audit_failure() {
    let broadcast_store = Arc::new(InMemoryBroadcastStore::new());
    let objection_store = Arc::new(InMemoryObjectionStore::new());
    let emitter = InMemorySubProcessorEmitter::new(
        Arc::new(FailingSubProcessorAuditSink),
        broadcast_store.clone(),
        objection_store,
    );
    let result = emitter.publish(sample_publish_request());
    assert!(
        result.is_err(),
        "publish must return Err when audit sink fails"
    );
    assert!(
        broadcast_store.is_empty(),
        "broadcast store must be UNCHANGED when audit fails (fail-CLOSED)"
    );
}

#[test]
fn test_change_fail_closed_on_audit_failure() {
    let broadcast_store = Arc::new(InMemoryBroadcastStore::new());
    let objection_store = Arc::new(InMemoryObjectionStore::new());
    let emitter = InMemorySubProcessorEmitter::new(
        Arc::new(FailingSubProcessorAuditSink),
        broadcast_store.clone(),
        objection_store,
    );
    let result = emitter.record_change(sample_change_request(5));
    assert!(
        result.is_err(),
        "record_change must return Err when audit sink fails"
    );
    assert!(
        broadcast_store.is_empty(),
        "broadcast log must NOT be seeded when audit emit fails (fail-CLOSED)"
    );
}

#[test]
fn test_objection_fail_closed_on_audit_failure() {
    let broadcast_store = Arc::new(InMemoryBroadcastStore::new());
    let objection_store = Arc::new(InMemoryObjectionStore::new());
    let emitter = InMemorySubProcessorEmitter::new(
        Arc::new(FailingSubProcessorAuditSink),
        broadcast_store.clone(),
        objection_store.clone(),
    );
    let result = emitter.file_objection(sample_objection_request("t1", "s1", "sentry"));
    assert!(
        result.is_err(),
        "file_objection must return Err when audit sink fails"
    );
    assert!(
        objection_store.is_empty(),
        "objection store must be UNCHANGED when audit fails (fail-CLOSED)"
    );
}

// ─── AC-002: change seeds broadcast log ─────────────────────────────────────

#[test]
fn test_change_seeds_broadcast_log() {
    let emitter = make_emitter();
    emitter
        .record_change(sample_change_request(10))
        .expect("record_change must succeed");
    let (_, total) = emitter
        .delivery_rate("BROADCAST01234567890123456")
        .expect("delivery_rate must succeed");
    assert_eq!(total, 10, "10 broadcast entries must be seeded");
}

// ─── AC-003: delivery confirmation tracking ─────────────────────────────────

#[test]
fn test_delivery_confirmation_tracking() {
    let broadcast_store = Arc::new(InMemoryBroadcastStore::new());
    let emitter = InMemorySubProcessorEmitter::new(
        Arc::new(InMemorySubProcessorAuditSink::new()),
        broadcast_store.clone(),
        Arc::new(InMemoryObjectionStore::new()),
    );
    // Seed 3 entries
    emitter
        .record_change(sample_change_request(3))
        .expect("record_change");

    // Mark first entry delivered
    let entries = broadcast_store.all_entries();
    let first_id = entries.first().expect("at least one entry").log_id.clone();
    emitter
        .update_delivery_status(
            &first_id,
            corelink_privacy::sub_processor::event::DeliveryStatus::Delivered,
            Some("2026-05-13T02:00:00Z".into()),
            None,
        )
        .expect("update_delivery_status");

    let (delivered, total) = emitter
        .delivery_rate("BROADCAST01234567890123456")
        .expect("delivery_rate");
    assert_eq!(total, 3);
    assert_eq!(delivered, 1);
}

// ─── AC-004: objection happy path ───────────────────────────────────────────

#[test]
fn test_objection_happy_path() {
    let objection_store = Arc::new(InMemoryObjectionStore::new());
    let emitter = InMemorySubProcessorEmitter::new(
        Arc::new(InMemorySubProcessorAuditSink::new()),
        Arc::new(InMemoryBroadcastStore::new()),
        objection_store.clone(),
    );
    let req = sample_objection_request("tenant-abc", "subject-001", "sentry");
    let objection_id = req.objection_id.clone();
    emitter.file_objection(req).expect("file_objection");

    let ticket = objection_store
        .get(&objection_id)
        .expect("get ticket")
        .expect("ticket should exist");
    let valid = matches!(ticket.ticket_status, ObjectionTicketStatus::Pending);
    assert!(valid, "new ticket must be in Pending state");
    assert_eq!(ticket.sub_processor_id, "sentry");
    assert_eq!(ticket.tenant_id, "tenant-abc");
}

// ─── AC-005: objection resolution ───────────────────────────────────────────

#[test]
fn test_objection_resolution_accepted() {
    let objection_store = Arc::new(InMemoryObjectionStore::new());
    let sink = Arc::new(InMemorySubProcessorAuditSink::new());
    let emitter = InMemorySubProcessorEmitter::new(
        sink.clone(),
        Arc::new(InMemoryBroadcastStore::new()),
        objection_store.clone(),
    );
    let req = sample_objection_request("tenant-abc", "subject-001", "sentry");
    let objection_id = req.objection_id.clone();
    emitter.file_objection(req).expect("file_objection");

    let resolution = ResolutionRequest {
        objection_id: objection_id.clone(),
        source: "corelink/privacy-officer".into(),
        event_id: "01JTEST000RESOLUTION000001".into(),
        timestamp: "2026-05-20T10:00:00Z".into(),
        region: "weur".into(),
        new_status: ObjectionTicketStatus::Accepted,
        decision: Some(ObjectionDecision::WorkaroundOffered),
        resolution_note: Some("Self-hosted Sentry adopted for tenant-abc".into()),
        resolved_at: Some("2026-05-20T10:00:00Z".into()),
        sub_processors_version: "2.0.0".into(),
    };
    emitter
        .resolve_objection(resolution)
        .expect("resolve_objection");

    let ticket = objection_store
        .get(&objection_id)
        .expect("get")
        .expect("exists");
    let valid = matches!(ticket.ticket_status, ObjectionTicketStatus::Accepted);
    assert!(valid, "resolved ticket must be Accepted");
    assert!(ticket.resolution_decision.is_some());
}

#[test]
fn test_objection_invalid_state_transition_rejected() {
    let emitter = make_emitter();
    let req = sample_objection_request("t1", "s1", "sentry");
    let objection_id = req.objection_id.clone();
    emitter.file_objection(req).expect("file_objection");

    // Terminal → any should fail
    let resolution = ResolutionRequest {
        objection_id: objection_id.clone(),
        source: "corelink/privacy-officer".into(),
        event_id: "01JTEST000RES2000000001".into(),
        timestamp: "2026-05-20T10:00:00Z".into(),
        region: "weur".into(),
        new_status: ObjectionTicketStatus::Accepted,
        decision: Some(ObjectionDecision::Accepted),
        resolution_note: None,
        resolved_at: Some("2026-05-20T10:00:00Z".into()),
        sub_processors_version: "2.0.0".into(),
    };
    emitter
        .resolve_objection(resolution.clone())
        .expect("first resolution");

    // Try to transition from Accepted (terminal) → Terminated (invalid)
    let bad_resolution = ResolutionRequest {
        objection_id: objection_id.clone(),
        new_status: ObjectionTicketStatus::Terminated,
        event_id: "01JTEST000RES3000000001".into(),
        ..resolution
    };
    let result = emitter.resolve_objection(bad_resolution);
    assert!(result.is_err(), "Terminal→Terminated must be rejected");
}

// ─── T-2.2: property test idempotency broadcast UNIQUE constraint ─────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_broadcast_idempotency_unique_constraint(
        tenant_suffix in "[a-z]{3,8}",
        email_suffix in "[a-z]{3,8}",
    ) {
        let broadcast_store = Arc::new(InMemoryBroadcastStore::new());

        let broadcast_id = "BCAST1234567890123456789";
        let tenant_id = format!("tenant-{tenant_suffix}");
        let email_hash = hash_recipient_email(&format!("user-{email_suffix}@example.com"));

        let entry1 = make_broadcast_entry(
            "LOG00000000000000000001",
            broadcast_id,
            "1.0.0",
            &tenant_id,
            &email_hash,
            EmailLocale::EnUs,
            "2026-05-13T00:00:00Z",
        );
        let entry2 = make_broadcast_entry(
            "LOG00000000000000000002", // different log_id
            broadcast_id,              // same broadcast_id
            "1.0.0",
            &tenant_id,                // same tenant
            &email_hash,               // same email
            EmailLocale::EnUs,         // same notification type
            "2026-05-13T01:00:00Z",
        );

        // First insert must succeed
        let r1 = broadcast_store.insert(entry1);
        let valid_first = r1.is_ok();
        prop_assert!(valid_first, "first broadcast insert must succeed");

        // Second insert with same UNIQUE key must fail
        let r2 = broadcast_store.insert(entry2);
        let valid_second = r2.is_err();
        prop_assert!(valid_second, "duplicate broadcast insert must fail (UNIQUE constraint)");
    }
}

// ─── T-2.3: mandatory all-plans (legal_obligation basis) ─────────────────────

#[test]
fn test_mandatory_all_plans_no_tier_gating() {
    // GDPR Art. 28.2 + LGPD Art. 39: sub_processor_notifications is
    // legal_obligation basis — ALL 5 canonical plans receive, NOT opt-out-able.
    // This test seeds 5 entries (one per canonical plan) and verifies all 5
    // are in the broadcast log.
    let plans = ["free", "solo", "team", "business", "enterprise"];
    let emitter = make_emitter();

    let entries: Vec<_> = plans
        .iter()
        .enumerate()
        .map(|(i, plan)| {
            make_broadcast_entry(
                &format!("LOG{i:0>20}"),
                "BROADCAST_ALLPLANS_12345678",
                "2.0.0",
                &format!("tenant-{plan}"),
                &hash_recipient_email(&format!("{plan}@example.com")),
                EmailLocale::EnUs,
                "2026-05-13T00:00:00Z",
            )
        })
        .collect();

    let req = ChangeRequest {
        source: "corelink/cd-pipeline".into(),
        event_id: "01JTEST000ALLPLANS0000001".into(),
        timestamp: "2026-05-13T01:00:00Z".into(),
        region: "weur".into(),
        payload: SubProcessorChangedPayload {
            old_version: "1.0.0".into(),
            new_version: "2.0.0".into(),
            changed_at: "2026-05-13T01:00:00Z".into(),
            diff: SubProcessorDiff {
                added: vec!["sentry".into()],
                removed: vec![],
                modified: vec![],
            },
            broadcast_trigger_ts: "2026-05-13T01:00:00Z".into(),
        },
        broadcast_entries: entries,
    };
    emitter.record_change(req).expect("record_change");

    let (_, total) = emitter
        .delivery_rate("BROADCAST_ALLPLANS_12345678")
        .expect("delivery_rate");
    assert_eq!(
        total, 5,
        "ALL 5 canonical plans must be in broadcast log (GDPR Art. 28.2 + LGPD Art. 39)"
    );
}

// ─── T-2.5 / S-5.5: DKIM cross-tenant isolation ──────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_dkim_cross_tenant_isolation(
        tenant_a in "[a-z]{4,12}",
        tenant_b in "[a-z]{4,12}",
    ) {
        // Same tenant → same key (trivially)
        // Different tenants → MUST have different keys (cross-tenant isolation)
        let secret = b"test-workspace-master-secret-for-props";
        if tenant_a == tenant_b {
            let k1 = derive_dkim_key(secret, &tenant_a).expect("derive a");
            let k2 = derive_dkim_key(secret, &tenant_b).expect("derive b");
            let valid = k1 == k2;
            prop_assert!(valid, "same tenant must yield same DKIM key");
        } else {
            let differ = dkim_keys_differ(secret, &tenant_a, &tenant_b);
            prop_assert!(differ, "different tenants must yield different DKIM keys: {tenant_a} vs {tenant_b}");
        }
    }
}

// ─── T-2.7: objection state machine property test ────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn prop_objection_state_machine_valid_transitions(
        tenant_suffix in "[a-z]{3,6}",
        subject_suffix in "[a-z]{3,6}",
    ) {
        // Verify that the state machine allows valid transitions and rejects invalid ones
        use ObjectionTicketStatus::*;

        let valid_transitions = [
            (Pending, InReview),
            (Pending, Accepted),
            (Pending, Terminated),
            (Pending, Withdrawn),
            (InReview, Accepted),
            (InReview, Terminated),
            (InReview, Withdrawn),
        ];

        let invalid_transitions = [
            (Accepted, InReview),
            (Accepted, Pending),
            (Accepted, Terminated),
            (Terminated, Accepted),
            (Terminated, InReview),
            (Withdrawn, Accepted),
            (Withdrawn, InReview),
        ];

        for (from, to) in &valid_transitions {
            let valid = from.can_transition_to(*to);
            prop_assert!(valid, "valid transition {:?}->{:?} must be allowed tenant={tenant_suffix}", from, to);
        }

        for (from, to) in &invalid_transitions {
            let valid = !from.can_transition_to(*to);
            prop_assert!(valid, "invalid transition {:?}->{:?} must be rejected tenant={tenant_suffix}", from, to);
        }

        // Terminal states must not transition to each other
        let terminal_states = [Accepted, Terminated, Withdrawn];
        for state in &terminal_states {
            for next_state in canonical_objection_statuses() {
                if !state.is_terminal() || *state == next_state {
                    continue;
                }
                let is_terminal_block = !state.can_transition_to(next_state);
                prop_assert!(is_terminal_block,
                    "terminal state {:?} must not transition to {:?} tenant={tenant_suffix}",
                    state, next_state);
            }
        }

        let _ = (tenant_suffix, subject_suffix);
    }
}

// ─── recipient email hash: raw email never in audit ───────────────────────────

#[test]
fn test_recipient_email_hash_deterministic() {
    let h1 = hash_recipient_email("user@example.com");
    let h2 = hash_recipient_email("user@example.com");
    assert_eq!(h1, h2, "hash must be deterministic");
    assert_eq!(h1.len(), 64, "SHA-256 hex must be 64 chars");
    assert!(!h1.contains('@'), "hash must not contain email chars");
}

#[test]
fn test_recipient_email_hash_differs_per_email() {
    let h1 = hash_recipient_email("user1@example.com");
    let h2 = hash_recipient_email("user2@example.com");
    assert_ne!(h1, h2, "different emails must yield different hashes");
}

// ─── SubProcessorDiff.has_changes ───────────────────────────────────────────

#[test]
fn test_diff_has_changes() {
    let empty = SubProcessorDiff {
        added: vec![],
        removed: vec![],
        modified: vec![],
    };
    assert!(!empty.has_changes(), "empty diff has no changes");

    let with_add = SubProcessorDiff {
        added: vec!["sentry".into()],
        removed: vec![],
        modified: vec![],
    };
    assert!(with_add.has_changes(), "diff with add has changes");
}
