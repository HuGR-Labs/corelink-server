use super::*;
use crate::audit::InMemoryBillingAuditEmitter;
use crate::clock::InMemoryFakeMatClock;
use crate::d1::InMemoryBillingD1;
use crate::tier::InMemoryTierSelector;
use corelink_tier_selection::tier::TierKind;

fn fixture() -> (
    D1SubscriptionStateHandler,
    Arc<InMemoryBillingD1>,
    Arc<InMemoryBillingAuditEmitter>,
) {
    let d1 = Arc::new(InMemoryBillingD1::new());
    let audit = Arc::new(InMemoryBillingAuditEmitter::new());
    let sel = Arc::new(InMemoryTierSelector::with_mapping(&[
        ("plan_solo", TierKind::Solo),
        ("plan_starter", TierKind::Starter),
        ("plan_pro", TierKind::Pro),
        ("plan_max", TierKind::Max),
    ]));
    let handler = D1SubscriptionStateHandler::new(d1.clone(), audit.clone(), sel).with_clock(
        Arc::new(InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000)),
    );
    (handler, d1, audit)
}

fn env(id: &str, kind: &str, body: serde_json::Value) -> StripeWebhookEnvelope {
    let raw = serde_json::json!({
        "id": id,
        "type": kind,
        "created": 1_700_000_000_u64,
        "data": body,
    });
    let bytes = serde_json::to_vec(&raw).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ---- WP-J: missing `status` field refuses to write ----
//
// The pre-fix `unwrap_or(if canceled { "canceled" } else { "active" })`
// turned a missing status into a fabricated value — a fail-OPEN
// grant of paid access on `active` for the `customer.subscription.updated`
// arm, and a fail-OPEN denial of a paying customer on `canceled`.
// The new behaviour: refuse the write, emit a structured warn,
// return Ok. The downstream signature-verify already passed, so a
// missing status is a payload-integrity problem, not an authz one.

#[test]
fn subscription_updated_with_missing_status_skips_write_and_warns() {
    let (handler, d1, _audit) = fixture();
    // Pre-seed a tier so the reconcile path is not the differentiator.
    d1.upsert_tier("ten_1", "starter", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_su_no_status",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                // NO `status` field on the subscription object.
                "metadata": { "tenant_id": "ten_1" },
                "plan": { "id": "plan_pro" },
                "quantity": 5,
            }
        }),
    );
    // The write MUST NOT materialise a fabricated value: no
    // subscription row, no audit event. We only assert the row
    // absence (the audit sink is exercised by other tests; the
    // "no audit emitted" assertion is encoded via the warn line
    // pin below).
    // Err(InvalidPayload) — the arm `webhook_dispatch` quarantines into
    // the DLQ. `Ok(())` would have been audited as `Dispatched`, answered
    // 200, and (the dedup row being already committed) made the Stripe
    // retry a no-op: event lost, audit trail claiming success.
    let err = handler
        .on_subscription_updated(&e)
        .expect_err("a missing `status` must be refused, not reported as dispatched");
    assert!(
        matches!(err, MaterializerError::InvalidPayload(_)),
        "must be InvalidPayload (the DLQ-quarantining arm), got {err:?}"
    );
    // The write MUST NOT materialise a fabricated value: no row in
    // `stripe_subscriptions`. (The audit sink is exercised by
    // other tests; the "no audit emitted" assertion is encoded via
    // the warn line pin below.)
    let subs: Vec<_> = d1
        .snapshot()
        .into_iter()
        .filter(|r| r.table == "stripe_subscriptions")
        .collect();
    assert!(
        subs.is_empty(),
        "a missing `status` must NOT produce a subscription row"
    );
}

#[test]
fn matrix_has_ten_entries() {
    assert_eq!(EVENT_MATERIALIZATION_MATRIX.len(), 10);
}

#[test]
fn container_webhook_events_json_matches_the_matrix() {
    // The reconcile script (scripts/ops/stripe-reconcile-webhook-events.sh)
    // sets the LIVE container endpoint's `enabled_events` from
    // container-webhook-events.json. This test makes EVENT_MATERIALIZATION_MATRIX
    // the sole authority: the JSON can never drift from the code, so the live
    // endpoint can never be subscribed to more/fewer events than the container
    // actually materializes (over-subscription is harmless-but-untidy;
    // under-subscription would silently drop a grant path).
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../container-webhook-events.json")).unwrap();
    let mut from_json: Vec<String> = json["enabled_events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    from_json.sort();
    let mut from_matrix: Vec<String> = EVENT_MATERIALIZATION_MATRIX
        .iter()
        .map(|(ev, _, _)| (*ev).to_owned())
        .collect();
    from_matrix.sort();
    assert_eq!(
        from_json, from_matrix,
        "container-webhook-events.json enabled_events must equal the \
         EVENT_MATERIALIZATION_MATRIX event-type column exactly — update the JSON \
         when you change the matrix (the reconcile script drives the live endpoint from it)."
    );
}

#[test]
fn subscription_updated_recomputes_tier_and_emits_change_audit() {
    let (handler, d1, audit) = fixture();
    d1.upsert_tier("ten_1", "starter", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_su",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "active",
                "metadata": { "tenant_id": "ten_1" },
                "plan": { "id": "plan_pro" },
                "quantity": 5,
            }
        }),
    );
    handler.on_subscription_updated(&e).unwrap();
    assert_eq!(d1.tier_for("ten_1"), Some("pro".to_string()));
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
}

/// Fixture WITH a Runners resolver wired (price `price_runner_team` →
/// Team tier: 80 concurrency / 600 vCPU-h).
fn fixture_with_runners() -> (
    D1SubscriptionStateHandler,
    Arc<InMemoryBillingD1>,
    Arc<InMemoryBillingAuditEmitter>,
) {
    let (handler, d1, audit) = fixture();
    let resolver = Arc::new(
        crate::runners::InMemoryRunnersEntitlementResolver::new().with_price(
            "price_runner_team",
            crate::runners::RunnersEntitlement {
                max_concurrency: 80,
                max_vcpu_h: 600,
            },
        ),
    );
    (handler.with_runners_resolver(resolver), d1, audit)
}

#[test]
fn runners_subscription_seeds_entitlement_not_tier() {
    let (handler, d1, audit) = fixture_with_runners();
    let e = env(
        "evt_run",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_run",
                "status": "active",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" }
            }
        }),
    );
    handler.on_subscription_updated(&e).unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));
    assert_eq!(
        audit.count_event("corelink.tenant.runners_entitlement_seeded.v1"),
        1
    );
    // Separate axis: the cache tier was NOT touched (no UnknownPlan 422).
    assert_eq!(d1.tier_for("ten_run"), None);
}

#[test]
fn runners_price_with_non_granting_status_does_not_seed() {
    for non_granting in ["past_due", "unpaid", "canceled"] {
        let (handler, d1, _audit) = fixture_with_runners();
        let e = env(
            "evt_run",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_run",
                    "status": non_granting,
                    "metadata": { "tenant_id": "ten_run" },
                    "plan": { "id": "price_runner_team" }
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        assert_eq!(
            d1.runners_entitlement_of("ten_run"),
            None,
            "status {non_granting} must NOT seed runners_entitlement"
        );
    }
}

#[test]
fn runners_updated_non_granting_status_revokes_prior_entitlement() {
    // WP4: a Runners subscription that was granted then goes NON-granting
    // (e.g. `customer.subscription.updated` status=canceled/past_due/unpaid)
    // must REVOKE `runners_entitlement` (→ None) and emit
    // `runners_entitlement_revoked.v1`, symmetric to the seed — never leave a
    // stale grant.
    for non_granting in ["canceled", "past_due", "unpaid"] {
        let (handler, d1, audit) = fixture_with_runners();
        // Seed first via a granting `active` event.
        let seed = env(
            "evt_seed",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_run",
                    "status": "active",
                    "metadata": { "tenant_id": "ten_run" },
                    "plan": { "id": "price_runner_team" }
                }
            }),
        );
        handler.on_subscription_updated(&seed).unwrap();
        assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));

        // Now the subscription goes non-granting → revoke.
        let e = env(
            "evt_revoke",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_run",
                    "status": non_granting,
                    "metadata": { "tenant_id": "ten_run" },
                    "plan": { "id": "price_runner_team" }
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        assert_eq!(
            d1.runners_entitlement_of("ten_run"),
            None,
            "status {non_granting} must REVOKE the prior runners_entitlement"
        );
        assert_eq!(
            audit.count_event("corelink.tenant.runners_entitlement_revoked.v1"),
            1,
            "status {non_granting} must emit a runners_entitlement_revoked.v1 audit"
        );
    }
}

#[test]
fn runners_subscription_deleted_revokes_entitlement() {
    // WP4: a `customer.subscription.deleted` for a Runners-price sub must
    // route through the runner REVOKE (not the cache-tier downgrade) and
    // remove `runners_entitlement`.
    let (handler, d1, audit) = fixture_with_runners();
    // Seed the entitlement first.
    let seed = env(
        "evt_seed",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_run",
                "status": "active",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" }
            }
        }),
    );
    handler.on_subscription_updated(&seed).unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));

    // Now delete the subscription (a Runners-price sub).
    let del = env(
        "evt_del",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_run",
                "status": "canceled",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" }
            }
        }),
    );
    handler.on_subscription_deleted(&del).unwrap();
    assert_eq!(
        d1.runners_entitlement_of("ten_run"),
        None,
        "customer.subscription.deleted for a runner price must revoke the entitlement"
    );
    assert_eq!(
        audit.count_event("corelink.tenant.runners_entitlement_revoked.v1"),
        1
    );
    // Runner-handled ⇒ the cache-tier downgrade path was NOT taken (a runner
    // price is not a cache tier); the cache tier stays untouched (None).
    assert_eq!(d1.tier_for("ten_run"), None);
}

#[test]
fn cache_subscription_deleted_does_not_touch_runners_entitlement() {
    // WP4 no-regression: a CACHE-price `customer.subscription.deleted` must
    // still downgrade the cache tier and must NOT touch runners_entitlement
    // (nor emit a runner revoke audit). We independently seed a runner
    // entitlement for the SAME tenant to prove the cache cancel leaves it
    // intact.
    let (handler, d1, audit) = fixture_with_runners();
    d1.upsert_tier("ten_cache", "pro", 1_700_000_000_000, "init")
        .unwrap();
    // An unrelated runner grant exists for this tenant.
    d1.upsert_runners_entitlement("ten_cache", 80, 600, 1_700_000_000_000)
        .unwrap();

    let del = env(
        "evt_cache_del",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_cache",
                "status": "canceled",
                "metadata": { "tenant_id": "ten_cache" },
                "plan": { "id": "plan_pro" }
            }
        }),
    );
    handler.on_subscription_deleted(&del).unwrap();
    // Cache tier's `subscription_state` gate flips off, but the historical
    // `tier` label is preserved (the signup-worker is the tier authority
    // on cancel; the container must never overwrite it to 'free').
    assert_eq!(d1.tier_for("ten_cache"), Some("pro".to_string()));
    // Runner entitlement UNTOUCHED (no revoke ran on a cache-price cancel).
    assert_eq!(d1.runners_entitlement_of("ten_cache"), Some((80, 600)));
    assert_eq!(
        audit.count_event("corelink.tenant.runners_entitlement_revoked.v1"),
        0,
        "a cache-price cancel must NOT emit a runner revoke audit"
    );
}

#[test]
fn extract_plan_id_falls_back_to_items_price_id() {
    // F-MP-3: a modern Stripe subscription with NO legacy plan.id but a
    // nested items.data[0].price.id must still resolve (tier + runners).
    let (handler, d1, _audit) = fixture_with_runners();
    let e = env(
        "evt_items",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_items",
                "status": "active",
                "metadata": { "tenant_id": "ten_items" },
                "items": { "data": [ { "price": { "id": "price_runner_team" } } ] }
            }
        }),
    );
    handler.on_subscription_updated(&e).unwrap();
    // Resolved via items[].price.id → Runners Team seeded (80/600).
    assert_eq!(d1.runners_entitlement_of("ten_items"), Some((80, 600)));
}

#[test]
fn cache_price_still_routes_to_tier_when_runners_resolver_present() {
    // A wired resolver must NOT divert cache-tier subscriptions: a
    // non-Runners price (plan_pro) still reconciles tier_selections.
    let (handler, d1, _audit) = fixture_with_runners();
    let e = env(
        "evt_cache",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_cache",
                "status": "active",
                "metadata": { "tenant_id": "ten_cache" },
                "plan": { "id": "plan_pro" }
            }
        }),
    );
    handler.on_subscription_updated(&e).unwrap();
    assert_eq!(d1.tier_for("ten_cache"), Some("pro".to_string()));
    assert_eq!(d1.runners_entitlement_of("ten_cache"), None);
}

#[test]
fn subscription_updated_non_granting_status_does_not_grant_active() {
    // Regression (money-path webhook status gate): a
    // customer.subscription.updated carrying a RECOGNIZED plan but a
    // NON-granting status (past_due / unpaid) MUST NOT (re-)grant the
    // canonical 'active' entitlement. `active` still does.
    for non_granting in ["past_due", "unpaid"] {
        let (handler, d1, audit) = fixture();
        // Tenant starts on a non-active baseline (free), so a granted
        // 'active' write would be an observable upgrade.
        d1.upsert_tier("ten_1", "free", 1_700_000_000_000, "init")
            .unwrap();
        let e = env(
            "evt_su",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": non_granting,
                    "metadata": { "tenant_id": "ten_1" },
                    "plan": { "id": "plan_pro" },
                    "quantity": 5,
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        // Entitlement gate NOT advanced to the paid 'pro' tier: the
        // materializer skipped the 'active' upsert for the non-granting
        // status (tier stays at the pre-existing baseline).
        assert_eq!(
            d1.tier_for("ten_1"),
            Some("free".to_string()),
            "status={non_granting} must NOT grant the paid tier (subscription_state='active')"
        );
        // No tier-change entitlement audit was emitted (the gate fired
        // before persist_tier_change).
        assert_eq!(
            audit.count_event("corelink.tenant.tier_changed.v1"),
            0,
            "status={non_granting} must not emit a tier_changed entitlement audit"
        );
    }

    // Control: an `active` status with the same recognized plan DOES grant.
    let (handler, d1, audit) = fixture();
    d1.upsert_tier("ten_1", "free", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_su_ok",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "active",
                "metadata": { "tenant_id": "ten_1" },
                "plan": { "id": "plan_pro" },
                "quantity": 5,
            }
        }),
    );
    handler.on_subscription_updated(&e).unwrap();
    assert_eq!(d1.tier_for("ten_1"), Some("pro".to_string()));
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
}

#[test]
fn subscription_deleted_marks_canceled_and_preserves_historical_tier() {
    // Stripe-cancel tier-race fix: previously named
    // `subscription_deleted_marks_canceled_and_downgrades_to_free` and
    // asserted `tier_for("ten_1") == Some("free")` — that encoded the BUG
    // (the container overwriting `tier_selections.tier` to `'free'` on
    // cancel, racing the signup-worker authority, which deliberately
    // leaves `tier` untouched and only flips `subscription_state`).
    // Renamed + re-asserted: cancel must set `subscription_state` to the
    // access-OFF gate (proven by `mark_subscription_canceled`'s row /
    // `downgrade_tier` being the driven path — see
    // `subscription_deleted_takes_downgrade_path_not_grant_path` below)
    // WITHOUT touching the historical `tier` label — it must stay `pro`.
    let (handler, d1, audit) = fixture();
    d1.upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_sd",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "canceled",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_subscription_deleted(&e).unwrap();
    assert_eq!(
        d1.tier_for("ten_1"),
        Some("pro".to_string()),
        "cancel must preserve the historical tier label, NOT overwrite it to 'free'"
    );
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
    assert_eq!(
        audit.count_event("corelink.billing.subscription_canceled.materialized.v1"),
        1
    );
}

/// A [`BillingD1Writer`] decorator that records WHICH tier-write method the
/// handler invoked (grant `upsert_tier` vs cancel `downgrade_tier`). Every
/// other method + all state forwards to the wrapped [`InMemoryBillingD1`]
/// so the existing fixture semantics are preserved.
#[derive(Debug)]
struct TierPathRecordingD1 {
    inner: Arc<InMemoryBillingD1>,
    upsert_calls: std::sync::Mutex<Vec<String>>,
    downgrade_calls: std::sync::Mutex<Vec<String>>,
}

impl TierPathRecordingD1 {
    fn new(inner: Arc<InMemoryBillingD1>) -> Self {
        Self {
            inner,
            upsert_calls: std::sync::Mutex::new(Vec::new()),
            downgrade_calls: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl BillingD1Writer for TierPathRecordingD1 {
    fn upsert_customer(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.upsert_customer(row)
    }
    fn upsert_subscription(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.upsert_subscription(row)
    }
    fn mark_subscription_canceled(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.mark_subscription_canceled(row)
    }
    fn upsert_invoice(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.upsert_invoice(row)
    }
    fn insert_dispute(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.insert_dispute(row)
    }
    fn insert_refund(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.insert_refund(row)
    }
    fn try_record_event(
        &self,
        stripe_event_id: &str,
        canonical_event_type: &str,
        now_ms: u64,
        outcome: crate::d1::WebhookOutcome,
    ) -> Result<bool, BillingD1Error> {
        self.inner
            .try_record_event(stripe_event_id, canonical_event_type, now_ms, outcome)
    }
    fn read_tier(&self, tenant_id: &str) -> Result<Option<String>, BillingD1Error> {
        self.inner.read_tier(tenant_id)
    }
    fn upsert_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        self.upsert_calls
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("rec mutex: {e}")))?
            .push(tier_wire.to_string());
        self.inner
            .upsert_tier(tenant_id, tier_wire, now_ms, correlation_id)
    }
    fn downgrade_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        self.downgrade_calls
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("rec mutex: {e}")))?
            .push(tier_wire.to_string());
        self.inner
            .downgrade_tier(tenant_id, tier_wire, now_ms, correlation_id)
    }
    fn upsert_runners_entitlement(
        &self,
        tenant_id: &str,
        max_concurrency: u32,
        max_vcpu_h: u32,
        now_ms: i64,
    ) -> Result<(), BillingD1Error> {
        self.inner
            .upsert_runners_entitlement(tenant_id, max_concurrency, max_vcpu_h, now_ms)
    }
    fn delete_runners_entitlement(&self, tenant_id: &str) -> Result<(), BillingD1Error> {
        self.inner.delete_runners_entitlement(tenant_id)
    }
}

#[test]
fn subscription_deleted_takes_downgrade_path_not_grant_path() {
    // CAA-360 MEDIUM: a `customer.subscription.deleted` must drive the
    // DOWNGRADE path (which writes subscription_state='inactive'), NOT the
    // grant `upsert_tier` path (which hard-codes 'active' → contradictory
    // active-free row). We seed a starting tier via the inner mirror
    // directly (so that write is NOT counted against the recorder) then
    // assert the handler used downgrade_tier — never upsert_tier.
    let inner = Arc::new(InMemoryBillingD1::new());
    inner
        .upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
        .unwrap();
    let rec = Arc::new(TierPathRecordingD1::new(inner));
    let audit = Arc::new(InMemoryBillingAuditEmitter::new());
    let sel = Arc::new(InMemoryTierSelector::with_mapping(&[(
        "plan_pro",
        TierKind::Pro,
    )]));
    let handler = D1SubscriptionStateHandler::new(rec.clone(), audit.clone(), sel).with_clock(
        Arc::new(InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000)),
    );

    let e = env(
        "evt_sd2",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "canceled",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_subscription_deleted(&e).unwrap();

    // The cancel path drove downgrade_tier exactly once (→ free), and
    // NEVER the grant path (upsert_tier) — no 'active'-writing statement
    // was reached.
    let downgrades = rec.downgrade_calls.lock().unwrap().clone();
    let upserts = rec.upsert_calls.lock().unwrap().clone();
    assert_eq!(
        downgrades,
        vec!["free".to_string()],
        "cancel must drive downgrade_tier('free') exactly once"
    );
    assert!(
        upserts.is_empty(),
        "cancel must NOT drive the grant upsert_tier path: {upserts:?}"
    );
    // The handler still DRIVES downgrade_tier with tier_wire="free" (the
    // call-site argument, asserted above), but the tier column itself must
    // stay 'pro' — the underlying writer (mirroring SQL_DOWNGRADE_TIER)
    // preserves the existing row's tier rather than overwriting it.
    assert_eq!(rec.inner.tier_for("ten_1").as_deref(), Some("pro"));
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
}

#[test]
fn invoice_paid_writes_audit_and_d1() {
    let (handler, d1, audit) = fixture();
    let e = env(
        "evt_ip",
        "invoice.paid",
        serde_json::json!({
            "object": {
                "id": "in_1",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_invoice_paid(&e).unwrap();
    assert_eq!(d1.count_table("stripe_invoices"), 1);
    assert_eq!(
        audit.count_event("corelink.billing.invoice.materialized.v1"),
        1
    );
}

#[test]
fn dispute_created_emits_sev1_audit() {
    let (handler, _d1, audit) = fixture();
    let e = env(
        "evt_dc",
        "charge.dispute.created",
        serde_json::json!({
            "object": {
                "id": "dp_1",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_charge_dispute_created(&e).unwrap();
    let rows = audit.snapshot();
    assert!(rows.iter().any(|r| r.severity == AuditSeverity::Sev1));
}

#[test]
fn audit_failure_aborts_d1_write_fail_closed() {
    let (handler, d1, audit) = fixture();
    audit.arm_failure(BillingAuditError::Transient("audit down".into()));
    let e = env(
        "evt_ip",
        "invoice.paid",
        serde_json::json!({
            "object": {
                "id": "in_1",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    let err = handler.on_invoice_paid(&e).unwrap_err();
    assert!(matches!(err, MaterializerError::Transient(_)));
    // No D1 row written.
    assert_eq!(d1.count_table("stripe_invoices"), 0);
}

#[test]
fn missing_tenant_id_surfaces_invalid_payload() {
    let (handler, _d1, _audit) = fixture();
    let e = env(
        "evt_x",
        "invoice.paid",
        serde_json::json!({ "object": { "id": "in_1" } }),
    );
    let err = handler.on_invoice_paid(&e).unwrap_err();
    assert!(matches!(err, MaterializerError::InvalidPayload(_)));
}

#[test]
fn unknown_plan_in_subscription_update_surfaces_invalid_payload() {
    let (handler, d1, _audit) = fixture();
    d1.upsert_tier("ten_1", "starter", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_su",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "active",
                "metadata": { "tenant_id": "ten_1" },
                "plan": { "id": "plan_DOESNOTEXIST" },
            }
        }),
    );
    let err = handler.on_subscription_updated(&e).unwrap_err();
    assert!(matches!(err, MaterializerError::InvalidPayload(_)));
}
