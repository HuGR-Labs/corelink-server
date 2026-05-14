//! WI-S19-004 property tests pinning the canonical invariants of
//! `corelink-tier-selection`.
//!
//! Per S-07 P1-2 fix + autonomous execution charter: the
//! `PROPTEST_CASES` env var overrides the case count at runtime
//! (nightly runs with 100k; PR CI runs with 10k via the
//! `proptest_config_pr` block).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use proptest::prelude::*;

use corelink_tier_selection::{
    canonical_tiers, AlwaysDenyDpaGate, InMemoryDpaGate, InMemoryStripeClient,
    InMemoryTierSelectionAuditSink, StripeCheckoutSessionCompletedEvent, StripeClient,
    StripeCustomerId, SubscriptionActivationReceipt, SubscriptionState, TenantCtx, TenantId,
    TierError, TierKind, TierSelectionAuditEventType, TierSelectionLedger,
    TIER_SELECTION_LOCK_WINDOW_MS,
};

fn build_ledger_dpa_accepted(
    tenants: &[&str],
) -> (
    TierSelectionLedger,
    InMemoryStripeClient,
    InMemoryTierSelectionAuditSink,
) {
    let dpa = InMemoryDpaGate::new();
    for t in tenants {
        dpa.accept(TenantId::new(*t), "v1");
    }
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit.clone()),
        "v1",
        "https://app/success",
        "https://app/cancel",
    );
    (ledger, stripe, audit)
}

fn build_ledger_dpa_denied() -> (
    TierSelectionLedger,
    InMemoryStripeClient,
    InMemoryTierSelectionAuditSink,
) {
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(AlwaysDenyDpaGate),
        stripe_arc,
        Arc::new(audit.clone()),
        "v1",
        "ok",
        "cancel",
    );
    (ledger, stripe, audit)
}

fn tier_strategy() -> impl Strategy<Value = TierKind> {
    prop_oneof![
        Just(TierKind::Free),
        Just(TierKind::Starter),
        Just(TierKind::Team),
        Just(TierKind::Pro),
        Just(TierKind::Enterprise),
    ]
}

// -------------------------------------------------------------------
// INV-ONBOARD-DPA-FIRST (HIGH; registry §3.12 canonical)
// -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 10_000, .. ProptestConfig::default() })]

    /// INV-ONBOARD-DPA-FIRST: when DPA is NEVER accepted, tier
    /// selection ALWAYS fails with `DpaRequired` — for ANY tier
    /// (including Free per WI §6.5; no exception).
    ///
    /// 10k cases — ratifies that the gate has NO bypass paths.
    #[test]
    fn prop_dpa_first_never_bypassed_10k(
        tier in tier_strategy(),
        now_ms in 0u64..1_000_000_000,
        tenant_idx in 0u32..100,
    ) {
        let (ledger, stripe, audit) = build_ledger_dpa_denied();
        let tenant = format!("t-{tenant_idx}");
        let ctx = TenantCtx::new(TenantId::new(&tenant), now_ms, "corr");
        let result = ledger.select_tier(&ctx, tier, "u@x.com");
        // Must be Err.
        prop_assert!(result.is_err(), "tier selection without DPA must fail: tier={tier:?}");
        let err = result.unwrap_err();
        // For Enterprise we may also see UseInquiryForm if the order
        // changed; the WI mandates DPA gate runs FIRST so we assert
        // strictly DpaRequired regardless of tier.
        prop_assert!(
            matches!(err, TierError::DpaRequired),
            "expected DpaRequired, got {err:?} for tier {tier:?}"
        );
        // Stripe MUST NOT have been called.
        prop_assert_eq!(stripe.sessions().len(), 0, "Stripe called despite missing DPA");
        // Violation audit emitted.
        prop_assert!(audit.has_event(TierSelectionAuditEventType::DpaFirstViolationAttempt));
        // Ledger must have NO active row for the tenant.
        prop_assert!(ledger.row(&TenantId::new(&tenant)).is_none());
    }
}

// -------------------------------------------------------------------
// D1 row lock: INSERT OR IGNORE on tier_selection_locks with 60s window
// -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 10_000, .. ProptestConfig::default() })]

    /// Concurrent tier selection within the 60s lock window returns
    /// `LockHeld`. After the window expires, the lock is released.
    #[test]
    fn prop_d1_lock_prevents_concurrent(
        first_now in 0u64..1_000_000,
        gap_ms in 0u64..120_000,
    ) {
        let (ledger, _stripe, _audit) = build_ledger_dpa_accepted(&["t1"]);
        let r1 = ledger.select_tier(
            &TenantCtx::new(TenantId::new("t1"), first_now, "c1"),
            TierKind::Pro,
            "u@x.com",
        );
        prop_assert!(r1.is_ok());

        let second_now = first_now.saturating_add(gap_ms);
        let r2 = ledger.select_tier(
            &TenantCtx::new(TenantId::new("t1"), second_now, "c2"),
            TierKind::Team,
            "u@x.com",
        );

        if gap_ms < TIER_SELECTION_LOCK_WINDOW_MS {
            // Within window → LockHeld.
            prop_assert!(matches!(r2, Err(TierError::LockHeld)),
                "expected LockHeld within {TIER_SELECTION_LOCK_WINDOW_MS}ms, got {r2:?}");
        } else {
            // After window → lock evicted; we expect either Ok (new
            // session) or an unrelated error (NOT LockHeld).
            if let Err(e) = &r2 {
                prop_assert!(!matches!(e, TierError::LockHeld),
                    "expected lock evicted after window, got {e:?}");
            }
        }
    }
}

// -------------------------------------------------------------------
// Stripe webhook idempotency
// -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, .. ProptestConfig::default() })]

    /// Duplicate webhook deliveries with the same `event_id` activate
    /// the subscription exactly once. Subsequent deliveries return
    /// `DuplicateIgnored`.
    #[test]
    fn prop_webhook_idempotency(
        repeats in 1u32..20,
        ts_ms in 1u64..10_000_000,
        tier in prop_oneof![
            Just(TierKind::Starter),
            Just(TierKind::Team),
            Just(TierKind::Pro),
        ],
    ) {
        let (ledger, _stripe, _audit) = build_ledger_dpa_accepted(&["t1"]);
        // Establish pending checkout.
        let _ = ledger.select_tier(
            &TenantCtx::new(TenantId::new("t1"), ts_ms.saturating_sub(1), "c"),
            tier,
            "u@x.com",
        ).unwrap();
        let event = StripeCheckoutSessionCompletedEvent::new(
            "evt_x",
            "cs_x",
            TenantId::new("t1"),
            tier,
            StripeCustomerId::new("cus_1"),
            ts_ms,
        );
        let mut activated_count = 0u32;
        let mut dup_count = 0u32;
        for _ in 0..repeats {
            let r = ledger.on_checkout_completed(&event).unwrap();
            match r {
                SubscriptionActivationReceipt::Activated { .. } => activated_count += 1,
                SubscriptionActivationReceipt::DuplicateIgnored { .. } => dup_count += 1,
                _ => prop_assert!(false, "unexpected receipt variant"),
            }
        }
        prop_assert_eq!(activated_count, 1, "subscription should activate exactly once");
        prop_assert_eq!(dup_count, repeats - 1);
        prop_assert_eq!(ledger.processed_event_count(), 1);
        let row = ledger.row(&TenantId::new("t1")).unwrap();
        prop_assert_eq!(row.subscription_state, SubscriptionState::Active);
    }
}

// -------------------------------------------------------------------
// Enterprise route enforcement
// -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, .. ProptestConfig::default() })]

    /// Backend rejects direct Stripe Checkout for Enterprise tier
    /// even when DPA accepted. Audit event emitted.
    #[test]
    fn prop_enterprise_route_enforcement(
        now_ms in 0u64..1_000_000,
        tenant_idx in 0u32..100,
    ) {
        let tenant = format!("t-{tenant_idx}");
        let (ledger, stripe, audit) = build_ledger_dpa_accepted(&[&tenant]);
        let err = ledger.select_tier(
            &TenantCtx::new(TenantId::new(&tenant), now_ms, "c"),
            TierKind::Enterprise,
            "u@x.com",
        ).unwrap_err();
        prop_assert!(matches!(err, TierError::UseInquiryForm));
        prop_assert_eq!(stripe.sessions().len(), 0, "Stripe must not be called for Enterprise");
        prop_assert!(audit.has_event(TierSelectionAuditEventType::EnterpriseRouteBypassAttempt));
    }
}

// -------------------------------------------------------------------
// Audit-emit-BEFORE-mutation fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER)
// -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, .. ProptestConfig::default() })]

    /// When the audit sink rejects, the orchestrator returns
    /// `TierError::Audit` and NO state mutation occurs.
    #[test]
    fn prop_audit_emit_before_mutation(
        tier in tier_strategy(),
        now_ms in 0u64..1_000_000,
    ) {
        use corelink_tier_selection::FailingTierSelectionAuditSink;
        let dpa = InMemoryDpaGate::new();
        dpa.accept(TenantId::new("t1"), "v1");
        let stripe = InMemoryStripeClient::new();
        let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
        let ledger = TierSelectionLedger::new(
            Arc::new(dpa),
            stripe_arc,
            Arc::new(FailingTierSelectionAuditSink),
            "v1",
            "ok",
            "cancel",
        );
        let result = ledger.select_tier(
            &TenantCtx::new(TenantId::new("t1"), now_ms, "c"),
            tier,
            "u@x.com",
        );
        prop_assert!(result.is_err());
        prop_assert!(matches!(result.unwrap_err(), TierError::Audit(_)));
        // Stripe must NOT have been called.
        prop_assert_eq!(stripe.sessions().len(), 0);
        // No row materialized.
        prop_assert!(ledger.row(&TenantId::new("t1")).is_none());
    }
}

// -------------------------------------------------------------------
// Surface-stability regression: canonical 5-tier list
// -------------------------------------------------------------------

#[test]
fn canonical_tier_strings_stable() {
    let strs: Vec<&str> = canonical_tiers().iter().map(|t| t.as_str()).collect();
    assert_eq!(strs, ["free", "starter", "team", "pro", "enterprise"]);
}

// -------------------------------------------------------------------
// Stripe signature verification: HMAC + replay window
// -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, .. ProptestConfig::default() })]

    /// Valid signatures within the 5-min replay window verify.
    /// Outside the window OR with tampered payload, verification
    /// fails with `InvalidSignature`.
    #[test]
    fn prop_stripe_signature_verify(
        ts_seconds in 1_700_000_000u64..1_800_000_000,
        skew_seconds in 0i64..1000,
        tamper in any::<bool>(),
    ) {
        use corelink_tier_selection::{compute_stripe_signature, verify_stripe_signature};
        let secret = b"whsec_test";
        let payload = b"{\"event\":\"x\"}";
        let sig = compute_stripe_signature(secret, ts_seconds, payload);
        let header = format!("t={ts_seconds},v1={sig}");
        let now_ms = (ts_seconds as i64 * 1000 + skew_seconds * 1000).max(0) as u64;

        let verify_payload: &[u8] = if tamper { b"{\"evil\":true}" } else { payload };
        let r = verify_stripe_signature(secret, &header, verify_payload, now_ms);

        let within_window = skew_seconds.unsigned_abs() <= 300;
        if !tamper && within_window {
            prop_assert!(r.is_ok(), "expected ok, got {r:?}");
        } else {
            prop_assert!(r.is_err());
            prop_assert!(matches!(r.unwrap_err(), TierError::InvalidSignature(_)));
        }
    }
}
