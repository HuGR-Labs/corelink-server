//! WI-S19-001 property tests pinning the canonical invariants of
//! `corelink-signup`.
//!
//! Per S-07 P1-2 fix + autonomous execution charter: the
//! `PROPTEST_CASES` env var overrides the case count at runtime
//! (nightly runs with 100k via `PROPTEST_CASES=100000`; PR CI defaults
//! to 10k). Spec contract S-19 §5.1 R-S19-1..R-S19-3 + WI §6.1.7
//! mandate ≥ 10k iterations to ratify
//! `INV-ONBOARD-ATOMIC-PROVISIONING`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use proptest::prelude::*;

use corelink_signup::orchestrator::InMemoryProvisionRecord;
use corelink_signup::{
    AtomicSignupStore, Bcp47Locale, BillingIntent, CorrelationId, IdempotencyKey,
    InMemoryAtomicSignupStore, InMemoryBillingClient, InMemorySignupAuditSink, OrchestrationError,
    OrchestrationStep, PrimaryRegion, SignupAuditEventType, SignupOrchestrator, SignupOutcome,
    SignupRequest, StripeOutageBillingClient, UserEmailHash,
};

/// `PROPTEST_CASES` env var override (S-07 P1-2 nightly 100k pattern).
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn cfg() -> ProptestConfig {
    ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_locale() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("en-US".to_string()),
        Just("pt-BR".to_string()),
        Just("de-DE".to_string()),
        Just("fr-FR".to_string()),
        Just("zz-XX".to_string()),
        "[a-z]{2}-[A-Z]{2}",
    ]
}

fn arb_step() -> impl Strategy<Value = OrchestrationStep> {
    prop_oneof![
        Just(OrchestrationStep::InsertTenant),
        Just(OrchestrationStep::InsertDpa),
        Just(OrchestrationStep::InsertFirstPat),
        Just(OrchestrationStep::InsertUsageCounterAndCommit),
    ]
}

fn req(idem: &str, email: &str, locale: &str, cid: &str) -> SignupRequest {
    SignupRequest::new(
        cid,
        UserEmailHash::new(email),
        Bcp47Locale::new(locale),
        IdempotencyKey::new(idem),
        CorrelationId::new(cid),
    )
}

fn happy_orchestrator() -> SignupOrchestrator<
    InMemorySignupAuditSink,
    InMemoryAtomicSignupStore,
    InMemoryBillingClient,
    InMemoryProvisionRecord,
> {
    SignupOrchestrator::new(
        InMemorySignupAuditSink::new(),
        InMemoryAtomicSignupStore::new(),
        InMemoryBillingClient::new(),
        InMemoryProvisionRecord::new(),
    )
}

// ---------------------------------------------------------------------------
// Property 1 — idempotency: same key → same signup_id
// (R-S19-1 + spec contract §5.1).
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_idempotency_key_same_signup_id(
        idem in "[a-zA-Z0-9_]{1,32}",
        email in "[a-z0-9]{1,16}",
        locale_a in arb_locale(),
        locale_b in arb_locale(),
        cid_a in "[a-zA-Z0-9_]{1,32}",
        cid_b in "[a-zA-Z0-9_]{1,32}",
    ) {
        let o = happy_orchestrator();
        let r1 = o.provision(&req(&idem, &email, &locale_a, &cid_a)).unwrap();
        let r2 = o.provision(&req(&idem, &email, &locale_b, &cid_b)).unwrap();
        let s1 = match r1.outcome {
            SignupOutcome::Provisioned { signup_id, .. } => signup_id,
            SignupOutcome::Rejected { .. } => return Ok(()),
            other => {
                prop_assert!(false, "expected Provisioned or Rejected, got {:?}", other);
                return Ok(());
            }
        };
        match r2.outcome {
            SignupOutcome::Duplicate { signup_id, .. } => {
                prop_assert_eq!(signup_id, s1);
            }
            other => prop_assert!(false, "expected Duplicate on replay, got {:?}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// Property 2 — atomicity: any error in middle of orchestration → no
// partial tenant row (INV-ONBOARD-ATOMIC-PROVISIONING; WI §6.1.7).
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_atomicity_any_failure_rolls_back(
        idem in "[a-zA-Z0-9_]{1,32}",
        email in "[a-z0-9]{1,16}",
        locale in arb_locale(),
        cid in "[a-zA-Z0-9_]{1,32}",
        step in arb_step(),
    ) {
        let audit = InMemorySignupAuditSink::new();
        let store = InMemoryAtomicSignupStore::new();
        store.inject_failure_at(step);
        let billing = InMemoryBillingClient::new();
        let prov = InMemoryProvisionRecord::new();
        let o = SignupOrchestrator::new(audit.clone(), store.clone(), billing, prov);

        let res = o.provision(&req(&idem, &email, &locale, &cid));

        // Storage error MUST surface; zero committed rows.
        prop_assert!(res.is_err());
        let err = res.err().unwrap();
        let observed_step = match err {
            OrchestrationError::Storage { step, .. } => step,
            other => {
                prop_assert!(false, "expected Storage error, got {:?}", other);
                return Ok(());
            }
        };
        prop_assert_eq!(observed_step, step);
        prop_assert_eq!(store.committed_tenant_count().unwrap(), 0);

        // Audit chain MUST carry `started` then `failed` (audit-emit-
        // BEFORE-mutation fail-CLOSED).
        let snap = audit.snapshot();
        let kinds: Vec<_> = snap.iter().map(|r| r.event_type).collect();
        prop_assert!(kinds.contains(&SignupAuditEventType::Started));
        prop_assert!(kinds.contains(&SignupAuditEventType::Failed));
    }
}

// ---------------------------------------------------------------------------
// Property 3 — chaos / Stripe outage → SignupOutcome::Deferred {
// billing: BillingIntent::Deferred }; no partial state leak (the D1
// atomic tx is OUTSIDE the billing boundary per Lote 10.19 P0).
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_chaos_stripe_outage_deferred_billing(
        idem in "[a-zA-Z0-9_]{1,32}",
        email in "[a-z0-9]{1,16}",
        locale in arb_locale(),
        cid in "[a-zA-Z0-9_]{1,32}",
    ) {
        let audit = InMemorySignupAuditSink::new();
        let store = InMemoryAtomicSignupStore::new();
        let billing = StripeOutageBillingClient;
        let prov = InMemoryProvisionRecord::new();
        let o = SignupOrchestrator::new(audit.clone(), store.clone(), billing, prov);

        let r = o.provision(&req(&idem, &email, &locale, &cid)).unwrap();
        let billing_intent = match r.outcome {
            SignupOutcome::Deferred { billing, .. } => billing,
            other => {
                prop_assert!(false, "expected Deferred, got {:?}", other);
                return Ok(());
            }
        };
        prop_assert_eq!(billing_intent, BillingIntent::Deferred);

        // The atomic D1 tx is committed exactly once.
        prop_assert_eq!(store.committed_tenant_count().unwrap(), 1);

        // Audit chain: `started` + `deferred` (no `completed`; no
        // `failed`).
        let snap = audit.snapshot();
        let kinds: Vec<_> = snap.iter().map(|r| r.event_type).collect();
        prop_assert!(kinds.contains(&SignupAuditEventType::Started));
        prop_assert!(kinds.contains(&SignupAuditEventType::Deferred));
        prop_assert!(!kinds.contains(&SignupAuditEventType::Completed));
        prop_assert!(!kinds.contains(&SignupAuditEventType::Failed));
    }
}

// ---------------------------------------------------------------------------
// Property 4 — PAT-CORRELATION-ID-001: every audit event carries the
// inbound correlation id.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_correlation_id_propagated_to_all_audit_events(
        idem in "[a-zA-Z0-9_]{1,32}",
        email in "[a-z0-9]{1,16}",
        locale in arb_locale(),
        cid in "[a-zA-Z0-9_]{1,32}",
    ) {
        let audit = InMemorySignupAuditSink::new();
        let o = SignupOrchestrator::new(
            audit.clone(),
            InMemoryAtomicSignupStore::new(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );
        // Accept either Rejected (e.g. via empty key collisions — none
        // here, key is len ≥ 1) or Provisioned; either way the audit
        // events MUST carry the inbound cid.
        let _ = o.provision(&req(&idem, &email, &locale, &cid));
        let snap = audit.snapshot();
        for r in &snap {
            prop_assert_eq!(r.correlation_id.as_str(), cid.as_str());
        }
    }
}

// ---------------------------------------------------------------------------
// Property 5 — region pin is deterministic in the locale and obeys
// the Lote 10.16 canonical map (pt-BR → sam; de/fr/es-ES → eu;
// otherwise enam).
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_region_pin_deterministic_lote_10_16(
        idem in "[a-zA-Z0-9_]{1,32}",
        email in "[a-z0-9]{1,16}",
        locale in arb_locale(),
        cid in "[a-zA-Z0-9_]{1,32}",
    ) {
        let o = happy_orchestrator();
        let r = o.provision(&req(&idem, &email, &locale, &cid)).unwrap();
        let observed = match r.outcome {
            SignupOutcome::Provisioned { primary_region, .. } => primary_region,
            SignupOutcome::Rejected { .. } => return Ok(()),
            other => {
                prop_assert!(false, "unexpected outcome {:?}", other);
                return Ok(());
            }
        };
        let lower = locale.to_ascii_lowercase();
        let expected = if lower.starts_with("pt-br") || lower == "pt" {
            PrimaryRegion::Sam
        } else if lower.starts_with("de") || lower.starts_with("fr") || lower.starts_with("es-es") {
            PrimaryRegion::Eu
        } else if lower.starts_with("ja")
            || lower.starts_with("ko")
            || lower.starts_with("zh")
            || lower.starts_with("en-sg")
            || lower.starts_with("en-hk")
        {
            PrimaryRegion::Apac
        } else {
            PrimaryRegion::Enam
        };
        prop_assert_eq!(observed, expected);
    }
}

// ---------------------------------------------------------------------------
// Property 6 — audit-emit-BEFORE-mutate fail-CLOSED: a failing audit
// sink aborts the mutation. With FailingSignupAuditSink the
// orchestrator MUST return OrchestrationError::Audit before any tenant
// row is committed.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_audit_emit_before_mutate_fail_closed(
        idem in "[a-zA-Z0-9_]{1,32}",
        email in "[a-z0-9]{1,16}",
        locale in arb_locale(),
        cid in "[a-zA-Z0-9_]{1,32}",
    ) {
        use corelink_signup::FailingSignupAuditSink;
        let store = InMemoryAtomicSignupStore::new();
        let o = SignupOrchestrator::new(
            FailingSignupAuditSink,
            store.clone(),
            InMemoryBillingClient::new(),
            InMemoryProvisionRecord::new(),
        );
        let err = o.provision(&req(&idem, &email, &locale, &cid)).unwrap_err();
        match &err {
            OrchestrationError::Audit(inner) => {
                // Inner SignupAuditEmitError is a foreign error type whose
                // Debug form must not be empty (it must carry a cause).
                prop_assert!(!format!("{inner:?}").is_empty(),
                    "Audit inner must carry a cause");
            }
            other => prop_assert!(false, "expected OrchestrationError::Audit, got {other:?}"),
        }
        prop_assert_eq!(store.committed_tenant_count().unwrap(), 0);
    }
}

// ---------------------------------------------------------------------------
// Smoke test (always-on; not gated by PROPTEST_CASES — verifies that
// the property test harness itself wires through the orchestrator
// end-to-end without runtime-fn surprises).
// ---------------------------------------------------------------------------

#[test]
fn smoke_one_signup_provisioned() {
    let o = happy_orchestrator();
    let r = o
        .provision(&req("idem-smoke", "smoke", "en-US", "evt_smoke"))
        .unwrap();
    assert!(matches!(r.outcome, SignupOutcome::Provisioned { .. }));
}
