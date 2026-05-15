//! Targeted regression tests that close mutation-testing surface
//! coverage gaps identified by
//! `cargo mutants -p corelink-tier-selection` on 2026-05-14.
//!
//! See `specs/_audits/2026-05-14-mutation-baseline.md` for full
//! mutant-by-mutant classification.

#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]

use std::sync::Arc;

use corelink_tier_selection::{
    audit::{
        canonical_tier_selection_audit_event_strings, InMemoryTierSelectionAuditSink,
        TierSelectionAuditEventType, TierSelectionAuditRecord, TierSelectionAuditSink,
    },
    compute_stripe_signature, parse_stripe_signature_header,
    stripe::StripeCheckoutSessionCompletedEvent,
    tier::{canonical_tiers, TierKind},
    tier_selection_schema_version, verify_stripe_signature, AlwaysDenyDpaGate,
    DpaAcceptanceGate, InMemoryDpaGate, InMemoryStripeClient, StripeClient, StripeCustomerId,
    SubscriptionActivationReceipt, SubscriptionState, TenantCtx, TenantId, TierError,
    TierSelectionLedger, TierSelectionReceipt, STRIPE_REPLAY_WINDOW_MS,
    TIER_SELECTION_LOCK_WINDOW_MS,
};

// =====================================================================
// TierKind::as_str + matchers — kills "" / "xyzzy" and `matches!`
// branch mutations.
// =====================================================================

#[test]
fn tier_kind_as_str_canonical_strings_stable_and_distinct() {
    let pairs = [
        (TierKind::Free, "free"),
        (TierKind::Starter, "starter"),
        (TierKind::Team, "team"),
        (TierKind::Pro, "pro"),
        (TierKind::Enterprise, "enterprise"),
    ];
    for (t, expected) in &pairs {
        assert_eq!(t.as_str(), *expected, "{:?} as_str", t);
        assert!(!t.as_str().is_empty());
        assert_ne!(t.as_str(), "xyzzy");
        assert_eq!(format!("{}", t), *expected);
    }
    // canonical_tiers list must match.
    let canon = canonical_tiers();
    assert_eq!(canon.len(), 5);
    for (i, (t, _)) in pairs.iter().enumerate() {
        assert_eq!(canon[i], *t);
    }
    // Five distinct strings.
    let mut sorted: Vec<&str> = pairs.iter().map(|(_, s)| *s).collect();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 5);
}

#[test]
fn tier_kind_requires_stripe_checkout_only_paid_tiers() {
    // Free: NO stripe, NO inquiry.
    assert!(!TierKind::Free.requires_stripe_checkout());
    assert!(!TierKind::Free.routes_to_inquiry_form());
    // Paid: stripe yes, inquiry no.
    for t in [TierKind::Starter, TierKind::Team, TierKind::Pro] {
        assert!(t.requires_stripe_checkout(), "{t:?}");
        assert!(!t.routes_to_inquiry_form(), "{t:?}");
    }
    // Enterprise: NO stripe direct, inquiry yes.
    assert!(!TierKind::Enterprise.requires_stripe_checkout());
    assert!(TierKind::Enterprise.routes_to_inquiry_form());
}

// =====================================================================
// TierSelectionAuditEventType canonical strings.
// =====================================================================

#[test]
fn audit_event_type_strings_canonical_and_distinct() {
    let pairs = [
        (
            TierSelectionAuditEventType::TierSelectAttempted,
            "corelink.onboarding.tier_select_attempted",
        ),
        (
            TierSelectionAuditEventType::DpaFirstViolationAttempt,
            "corelink.onboarding.dpa_first_violation_attempt",
        ),
        (
            TierSelectionAuditEventType::TierActivatedFree,
            "corelink.onboarding.tier_activated_free",
        ),
        (
            TierSelectionAuditEventType::StripeCheckoutSessionCreated,
            "corelink.onboarding.stripe_checkout_session_created",
        ),
        (
            TierSelectionAuditEventType::EnterpriseRouteBypassAttempt,
            "corelink.onboarding.enterprise_route_bypass_attempt",
        ),
        (
            TierSelectionAuditEventType::StripeSubscriptionActivated,
            "corelink.onboarding.stripe_subscription_activated",
        ),
        (
            TierSelectionAuditEventType::StripeWebhookDuplicate,
            "corelink.onboarding.stripe_webhook_duplicate",
        ),
        (
            TierSelectionAuditEventType::StripeWebhookInvalidSignature,
            "corelink.onboarding.stripe_webhook_invalid_signature",
        ),
    ];
    for (ev, expected) in &pairs {
        assert_eq!(ev.as_str(), *expected);
        assert!(ev.as_str().starts_with("corelink.onboarding."));
        assert!(!ev.as_str().is_empty());
        assert_ne!(ev.as_str(), "xyzzy");
        assert_eq!(format!("{}", ev), *expected);
    }
    let canonical = canonical_tier_selection_audit_event_strings();
    let collected: Vec<&str> = pairs.iter().map(|(_, s)| *s).collect();
    assert_eq!(canonical.as_slice(), collected.as_slice());
    let mut sorted: Vec<&str> = collected.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 8);
}

// =====================================================================
// In-memory audit sink — len / is_empty / has_event with known counts.
// =====================================================================

#[test]
fn in_memory_audit_sink_len_is_empty_has_event() {
    let sink = InMemoryTierSelectionAuditSink::new();
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);
    assert!(!sink.has_event(TierSelectionAuditEventType::TierSelectAttempted));

    let rec = TierSelectionAuditRecord::new(
        TierSelectionAuditEventType::TierSelectAttempted,
        TenantId::new("t-audit"),
        Some(TierKind::Free),
        100,
        "corr-1",
    );
    sink.emit(&rec).unwrap();
    assert!(!sink.is_empty());
    assert_eq!(sink.len(), 1);
    assert!(sink.has_event(TierSelectionAuditEventType::TierSelectAttempted));
    // Different event type not present.
    assert!(!sink.has_event(TierSelectionAuditEventType::DpaFirstViolationAttempt));
    let snap = sink.snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0], rec);
}

// =====================================================================
// Tenant / StripeCustomerId / TenantCtx round-trip.
// =====================================================================

#[test]
fn newtype_round_trip_kills_default_default_mutants() {
    let tid = TenantId::new("t-x42");
    assert_eq!(tid.as_str(), "t-x42");
    assert_eq!(format!("{}", tid), "t-x42");
    let cust = StripeCustomerId::new("cus_x42");
    assert_eq!(cust.as_str(), "cus_x42");
    let ctx = TenantCtx::new(TenantId::new("t-ctx"), 9999, "corr-ctx");
    assert_eq!(ctx.tenant_id.as_str(), "t-ctx");
    assert_eq!(ctx.now_ms, 9999);
    assert_eq!(ctx.correlation_id, "corr-ctx");
}

// =====================================================================
// DPA gate — In-memory accept / revoke + AlwaysDenyDpaGate.
// =====================================================================

#[test]
fn in_memory_dpa_gate_accept_then_revoke() {
    let gate = InMemoryDpaGate::new();
    let tid = TenantId::new("t-dpa");
    // Initial: not accepted.
    assert!(!gate.is_accepted(&tid, "v1"));
    gate.accept(tid.clone(), "v1");
    assert!(gate.is_accepted(&tid, "v1"));
    // Different version: not accepted.
    assert!(!gate.is_accepted(&tid, "v2"));
    // Different tenant: not accepted.
    assert!(!gate.is_accepted(&TenantId::new("t-other"), "v1"));
    gate.revoke(tid.clone(), "v1");
    assert!(!gate.is_accepted(&tid, "v1"));
}

#[test]
fn always_deny_dpa_gate_returns_false_for_any_inputs() {
    let gate = AlwaysDenyDpaGate;
    assert!(!gate.is_accepted(&TenantId::new("anyone"), "anything"));
    assert!(!gate.is_accepted(&TenantId::new(""), ""));
}

// =====================================================================
// Stripe signature compute + verify — exercises HMAC details.
// =====================================================================

#[test]
fn stripe_signature_compute_is_deterministic_and_hex_64() {
    let s1 = compute_stripe_signature(b"sec", 100, b"payload");
    let s2 = compute_stripe_signature(b"sec", 100, b"payload");
    assert_eq!(s1, s2);
    // HMAC-SHA256 → 32 bytes → 64 hex chars.
    assert_eq!(s1.len(), 64);
    // Different secret -> different signature.
    let s_diff = compute_stripe_signature(b"sec2", 100, b"payload");
    assert_ne!(s1, s_diff);
    // Different timestamp -> different signature.
    let s_diff_ts = compute_stripe_signature(b"sec", 101, b"payload");
    assert_ne!(s1, s_diff_ts);
    // Different payload -> different signature.
    let s_diff_pl = compute_stripe_signature(b"sec", 100, b"payload2");
    assert_ne!(s1, s_diff_pl);
}

#[test]
fn parse_stripe_signature_header_extracts_t_and_v1() {
    let (ts, sig) =
        parse_stripe_signature_header("t=1700000000,v1=deadbeefcafe").expect("parse");
    assert_eq!(ts, 1_700_000_000);
    assert_eq!(sig, "deadbeefcafe");
    // Whitespace tolerated.
    let (ts2, sig2) =
        parse_stripe_signature_header(" t=42 , v1=abcd ").expect("parse with ws");
    assert_eq!(ts2, 42);
    assert_eq!(sig2, "abcd");
}

#[test]
fn parse_stripe_signature_header_missing_fields_rejected() {
    assert!(matches!(
        parse_stripe_signature_header("v1=abc"),
        Err(TierError::InvalidSignature(_))
    ));
    assert!(matches!(
        parse_stripe_signature_header("t=100"),
        Err(TierError::InvalidSignature(_))
    ));
    assert!(matches!(
        parse_stripe_signature_header(""),
        Err(TierError::InvalidSignature(_))
    ));
}

#[test]
fn verify_stripe_signature_replay_window_boundary() {
    let secret = b"whsec_test";
    let payload = b"{}";
    let ts = 1_700_000_000_u64;
    let sig = compute_stripe_signature(secret, ts, payload);
    let header = format!("t={ts},v1={sig}");

    // Inside the 5-min window: ok.
    let now_inside = ts * 1000 + (STRIPE_REPLAY_WINDOW_MS / 2);
    assert!(verify_stripe_signature(secret, &header, payload, now_inside).is_ok());

    // Exactly outside the window + 1 ms: rejected.
    let now_outside = ts * 1000 + STRIPE_REPLAY_WINDOW_MS + 1;
    assert!(matches!(
        verify_stripe_signature(secret, &header, payload, now_outside),
        Err(TierError::InvalidSignature(_))
    ));

    // Future-dated timestamp: rejected.
    let future_ts = ts + (STRIPE_REPLAY_WINDOW_MS / 1000) + 60;
    let future_sig = compute_stripe_signature(secret, future_ts, payload);
    let future_header = format!("t={future_ts},v1={future_sig}");
    assert!(matches!(
        verify_stripe_signature(secret, &future_header, payload, ts * 1000),
        Err(TierError::InvalidSignature(_))
    ));
}

// =====================================================================
// InMemoryStripeClient — deterministic session id format + sessions
// snapshot len tracks creation count.
// =====================================================================

// In-memory Stripe client exercised end-to-end via the ledger
// (cross-crate non-exhaustive struct prevents direct construction).
#[test]
fn in_memory_stripe_client_session_count_grows_via_ledger() {
    let dpa = InMemoryDpaGate::new();
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let audit = InMemoryTierSelectionAuditSink::new();
    // Two distinct tenants that both accept DPA.
    dpa.accept(TenantId::new("t-s1"), "v1");
    dpa.accept(TenantId::new("t-s2"), "v1");
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit),
        "v1",
        "ok",
        "cancel",
    );
    assert_eq!(stripe.sessions().len(), 0);
    let _ = ledger
        .select_tier(
            &TenantCtx::new(TenantId::new("t-s1"), 1000, "c1"),
            TierKind::Starter,
            "u1@x.com",
        )
        .unwrap();
    assert_eq!(stripe.sessions().len(), 1);
    let _ = ledger
        .select_tier(
            &TenantCtx::new(TenantId::new("t-s2"), 1000, "c2"),
            TierKind::Team,
            "u2@x.com",
        )
        .unwrap();
    assert_eq!(stripe.sessions().len(), 2);
    // Sessions all start with cs_fake_ + url contains session_id.
    for s in stripe.sessions() {
        assert!(s.session_id.starts_with("cs_fake_"));
        assert!(s.url.contains(&s.session_id));
        assert!(s.stripe_customer_id.as_str().starts_with("cus_fake_"));
    }
}

#[test]
fn in_memory_stripe_client_arm_failure_is_one_shot_via_ledger() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-arm"), "v1");
    let stripe = InMemoryStripeClient::new();
    stripe.arm_failure();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit),
        "v1",
        "ok",
        "cancel",
    );
    let ctx = TenantCtx::new(TenantId::new("t-arm"), 1000, "corr");
    // First call hits the armed failure.
    let err = ledger
        .select_tier(&ctx, TierKind::Starter, "u@x.com")
        .unwrap_err();
    assert!(matches!(err, TierError::Stripe(_)));
    // The ledger's failure path releases the row lock, so a retry
    // proceeds (armed failure already consumed → Stripe succeeds).
    let ok = ledger
        .select_tier(&ctx, TierKind::Starter, "u@x.com")
        .expect("post-armed retry should succeed");
    assert!(matches!(ok, TierSelectionReceipt::CheckoutRedirect { .. }));
}

// =====================================================================
// TIER_SELECTION_LOCK_WINDOW_MS / STRIPE_REPLAY_WINDOW_MS canonical
// values — kills `replace ... with 0` constant mutants.
// =====================================================================

#[test]
fn lock_window_canonical_value() {
    assert_eq!(TIER_SELECTION_LOCK_WINDOW_MS, 60_000);
    // 60s sanity bounds: > 1s, < 1h.
    assert!(TIER_SELECTION_LOCK_WINDOW_MS > 1_000);
    assert!(TIER_SELECTION_LOCK_WINDOW_MS < 60 * 60 * 1_000);
}

#[test]
fn replay_window_canonical_value() {
    assert_eq!(STRIPE_REPLAY_WINDOW_MS, 300_000);
    // 5-min sanity bounds.
    assert!(STRIPE_REPLAY_WINDOW_MS > 60_000);
    assert!(STRIPE_REPLAY_WINDOW_MS < 60 * 60 * 1_000);
}

#[test]
fn tier_selection_schema_version_is_one() {
    // Kills `replace tier_selection_schema_version -> u32 with 0`.
    assert_eq!(tier_selection_schema_version(), 1);
    assert_ne!(tier_selection_schema_version(), 0);
}

// =====================================================================
// Ledger — DPA-FIRST enforcement on Free tier with EXACT error variant.
// =====================================================================

#[test]
fn ledger_free_tier_dpa_first_enforces_before_activation() {
    let dpa = AlwaysDenyDpaGate;
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        Arc::new(InMemoryStripeClient::new()),
        Arc::new(audit.clone()),
        "v1",
        "https://ok",
        "https://cancel",
    );
    let ctx = TenantCtx::new(TenantId::new("t-dpa-free"), 100, "corr");
    let err = ledger
        .select_tier(&ctx, TierKind::Free, "u@x.com")
        .unwrap_err();
    // Must be the EXACT DpaRequired variant (not Stripe, not Audit).
    assert!(matches!(err, TierError::DpaRequired));
    // Audit chain must carry attempt + violation.
    assert!(audit.has_event(TierSelectionAuditEventType::TierSelectAttempted));
    assert!(audit.has_event(TierSelectionAuditEventType::DpaFirstViolationAttempt));
    // Tier-activation event NOT present.
    assert!(!audit.has_event(TierSelectionAuditEventType::TierActivatedFree));
    assert!(ledger.row(&TenantId::new("t-dpa-free")).is_none());
}

#[test]
fn ledger_enterprise_tier_rejects_with_use_inquiry_form() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-ent"), "v1");
    let audit = InMemoryTierSelectionAuditSink::new();
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit.clone()),
        "v1",
        "ok",
        "cancel",
    );
    let ctx = TenantCtx::new(TenantId::new("t-ent"), 100, "corr");
    let err = ledger
        .select_tier(&ctx, TierKind::Enterprise, "u@x.com")
        .unwrap_err();
    assert!(matches!(err, TierError::UseInquiryForm));
    // Stripe MUST NOT have been called (enterprise route rejected
    // BEFORE Stripe per WI §6.6).
    assert_eq!(stripe.sessions().len(), 0);
    assert!(audit.has_event(TierSelectionAuditEventType::EnterpriseRouteBypassAttempt));
}

// =====================================================================
// Ledger getters that previously only had zero-value assertions —
// exercise non-zero paths so `-> 0 / -> 1` constant mutants fail.
// =====================================================================

#[test]
fn ledger_checkout_session_count_is_nonzero_after_paid_select() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-cnt-1"), "v1");
    dpa.accept(TenantId::new("t-cnt-2"), "v1");
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        Arc::new(InMemoryStripeClient::new()),
        Arc::new(audit),
        "v1",
        "https://ok",
        "https://cancel",
    );
    assert_eq!(ledger.checkout_session_count(), 0);
    let _ = ledger
        .select_tier(
            &TenantCtx::new(TenantId::new("t-cnt-1"), 1000, "c1"),
            TierKind::Starter,
            "u1@x.com",
        )
        .unwrap();
    // 1 session created — kills `-> 0` AND `-> 1` (we'll see 1 here,
    // but the next call pushes us to 2).
    assert_eq!(ledger.checkout_session_count(), 1);
    let _ = ledger
        .select_tier(
            &TenantCtx::new(TenantId::new("t-cnt-2"), 1000, "c2"),
            TierKind::Pro,
            "u2@x.com",
        )
        .unwrap();
    // 2 sessions — kills `-> 1` constant.
    assert_eq!(ledger.checkout_session_count(), 2);
}

#[test]
fn ledger_processed_event_count_grows_on_successful_webhook() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-evt-1"), "v1");
    dpa.accept(TenantId::new("t-evt-2"), "v1");
    let audit = InMemoryTierSelectionAuditSink::new();
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit),
        "v1",
        "https://ok",
        "https://cancel",
    );
    assert_eq!(ledger.processed_event_count(), 0);
    // Drive two distinct subscriptions to active.
    for (tenant, evt, tier) in [
        ("t-evt-1", "evt_aaa", TierKind::Starter),
        ("t-evt-2", "evt_bbb", TierKind::Pro),
    ] {
        let _ = ledger
            .select_tier(
                &TenantCtx::new(TenantId::new(tenant), 1000, "corr"),
                tier,
                "u@x.com",
            )
            .unwrap();
        let event = StripeCheckoutSessionCompletedEvent::new(
            evt,
            format!("cs_{tenant}"),
            TenantId::new(tenant),
            tier,
            StripeCustomerId::new(format!("cus_{tenant}")),
            2_000,
        );
        let r = ledger.on_checkout_completed(&event).unwrap();
        assert!(matches!(r, SubscriptionActivationReceipt::Activated { .. }));
    }
    // 2 distinct events processed — kills `-> 0` and `-> 1`.
    assert_eq!(ledger.processed_event_count(), 2);
}

// =====================================================================
// UNIQUE-active subscription check (`subscription_state == Active`)
// — kills the `== / !=` mutation on ledger.rs line ~255.
// Asserts the precise non-Active state (PendingCheckout) is allowed
// to retry without `AlreadyActive`.
// =====================================================================

#[test]
fn ledger_pending_checkout_state_allows_retry_not_already_active() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-pending"), "v1");
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit),
        "v1",
        "https://ok",
        "https://cancel",
    );
    let ctx = TenantCtx::new(TenantId::new("t-pending"), 1000, "corr");
    // First select: row goes to PendingCheckout + lock held for 60s.
    let _ = ledger
        .select_tier(&ctx, TierKind::Starter, "u@x.com")
        .unwrap();
    let row = ledger.row(&TenantId::new("t-pending")).unwrap();
    assert_eq!(row.subscription_state, SubscriptionState::PendingCheckout);
    // After lock window expires (now_ms past lock expiry), a retry
    // with the SAME tenant + PendingCheckout state MUST succeed
    // (NOT return AlreadyActive). This kills the `==` -> `!=`
    // mutation on the UNIQUE-active check at ledger.rs:255.
    let later_ctx = TenantCtx::new(
        TenantId::new("t-pending"),
        1000 + TIER_SELECTION_LOCK_WINDOW_MS + 1,
        "corr-2",
    );
    let r = ledger.select_tier(&later_ctx, TierKind::Pro, "u@x.com");
    // The original (correct) check is `state == Active` so a
    // PendingCheckout state does NOT trigger AlreadyActive → call
    // proceeds. The mutated `!=` would trigger AlreadyActive →
    // r.is_err() == true with `AlreadyActive`. We assert success.
    assert!(
        r.is_ok(),
        "PendingCheckout state must NOT be treated as Active; got {:?}",
        r
    );
}

#[test]
fn ledger_active_subscription_rejects_re_select() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-active"), "v1");
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let audit = InMemoryTierSelectionAuditSink::new();
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit),
        "v1",
        "https://ok",
        "https://cancel",
    );
    let ctx = TenantCtx::new(TenantId::new("t-active"), 1000, "corr");
    let _ = ledger
        .select_tier(&ctx, TierKind::Starter, "u@x.com")
        .unwrap();
    // Drive the subscription to Active via webhook.
    let event = StripeCheckoutSessionCompletedEvent::new(
        "evt_active",
        "cs_active",
        TenantId::new("t-active"),
        TierKind::Starter,
        StripeCustomerId::new("cus_active"),
        2_000,
    );
    let r = ledger.on_checkout_completed(&event).unwrap();
    assert!(matches!(r, SubscriptionActivationReceipt::Activated { .. }));
    // Now retry select after lock window expires; MUST be AlreadyActive
    // (kills the `==` -> `!=` mutation which would let the call
    // through).
    let later_ctx = TenantCtx::new(
        TenantId::new("t-active"),
        1000 + TIER_SELECTION_LOCK_WINDOW_MS + 1,
        "corr-2",
    );
    let err = ledger
        .select_tier(&later_ctx, TierKind::Pro, "u@x.com")
        .unwrap_err();
    assert!(matches!(err, TierError::AlreadyActive));
}

#[test]
fn ledger_paid_tier_with_dpa_accepted_returns_checkout_redirect_with_session() {
    let dpa = InMemoryDpaGate::new();
    dpa.accept(TenantId::new("t-paid"), "v1");
    let audit = InMemoryTierSelectionAuditSink::new();
    let stripe = InMemoryStripeClient::new();
    let stripe_arc: Arc<dyn StripeClient> = Arc::new(stripe.clone());
    let ledger = TierSelectionLedger::new(
        Arc::new(dpa),
        stripe_arc,
        Arc::new(audit),
        "v1",
        "https://ok",
        "https://cancel",
    );
    let ctx = TenantCtx::new(TenantId::new("t-paid"), 1000, "corr");
    let r = ledger
        .select_tier(&ctx, TierKind::Pro, "u@x.com")
        .expect("must succeed");
    match r {
        TierSelectionReceipt::CheckoutRedirect {
            tier,
            session_id,
            checkout_url,
            tenant_id,
        } => {
            assert_eq!(tier, TierKind::Pro);
            assert_eq!(tenant_id, TenantId::new("t-paid"));
            assert!(session_id.starts_with("cs_fake_"));
            assert!(checkout_url.contains(&session_id));
        }
        other => panic!("expected CheckoutRedirect, got {other:?}"),
    }
    // Exactly one Stripe Checkout session was created.
    assert_eq!(stripe.sessions().len(), 1);
}
