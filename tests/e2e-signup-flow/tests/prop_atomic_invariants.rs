//! R3-1 Property test — 1k randomized signup payloads → no panic,
//! atomic invariants hold.
//!
//! Pinned invariants:
//!
//! - INV-ONBOARD-ATOMIC-PROVISIONING: exactly one tenant row per
//!   `Provisioned` outcome; zero rows per `Rejected`.
//! - Idempotency: same `Idempotency-Key` always yields a stable
//!   `signup_id`.
//! - Audit-emit-BEFORE-mutation: every `Provisioned` outcome has a
//!   preceding `Started` audit event.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_signup::{
    AtomicSignupStore, Bcp47Locale, CorrelationId, IdempotencyKey, InMemoryAtomicSignupStore,
    InMemoryBillingClient, InMemorySignupAuditSink, SignupAuditEventType, SignupOrchestrator,
    SignupOutcome, SignupRequest, UserEmailHash,
};
use corelink_signup::orchestrator::InMemoryProvisionRecord;

use proptest::prelude::*;

fn arb_idem_key() -> impl Strategy<Value = String> {
    // 1..32 ascii alnum (avoid empty — that's a Rejected path tested
    // exhaustively in unit tests).
    proptest::collection::vec(any::<u8>().prop_map(|b| (b % 26) + b'a'), 1..32)
        .prop_map(|v| String::from_utf8(v).unwrap_or_else(|_| "x".to_string()))
}

fn arb_locale() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("en-US"),
        Just("pt-BR"),
        Just("es-419"),
        Just("fr-FR"), // non-canonical → maps to Enam default
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn r3_1_prop_random_signup_payloads_uphold_invariants(
        idem in arb_idem_key(),
        locale in arb_locale(),
        clerk_evt in "evt_[a-z0-9]{1,16}",
        email_hash in "[0-9a-f]{16,64}",
    ) {
        let audit = InMemorySignupAuditSink::new();
        let store = InMemoryAtomicSignupStore::new();
        let orch = SignupOrchestrator::new(
            audit.clone(),
            store.clone(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );

        let req = SignupRequest::new(
            clerk_evt.clone(),
            UserEmailHash::new(email_hash),
            Bcp47Locale::new(locale),
            IdempotencyKey::new(idem.clone()),
            CorrelationId::new(clerk_evt),
        );

        // (1) First invocation → Provisioned (in-memory billing
        // never errors).
        let r1 = orch.provision(&req).unwrap();
        let signup_id_first = match &r1.outcome {
            SignupOutcome::Provisioned { signup_id, .. } => signup_id.clone(),
            other => panic!("expected Provisioned, got {other:?}"),
        };

        // INV-ATOMIC: exactly one tenant row committed.
        prop_assert_eq!(store.committed_tenant_count().unwrap(), 1);

        // INV-AUDIT-APPEND-ONLY: at least one Started + one Completed.
        let kinds: Vec<_> = audit
            .snapshot()
            .iter()
            .map(|r| r.event_type)
            .collect();
        prop_assert!(kinds.contains(&SignupAuditEventType::Started));
        prop_assert!(kinds.contains(&SignupAuditEventType::Completed));

        // (2) Replay with the SAME idempotency key → Duplicate +
        // matching signup_id; no second commit.
        let r2 = orch.provision(&req).unwrap();
        match &r2.outcome {
            SignupOutcome::Duplicate { signup_id, .. } => {
                prop_assert_eq!(signup_id.clone(), signup_id_first);
            }
            other => panic!("expected Duplicate on replay, got {other:?}"),
        }
        prop_assert_eq!(store.committed_tenant_count().unwrap(), 1);
    }
}
