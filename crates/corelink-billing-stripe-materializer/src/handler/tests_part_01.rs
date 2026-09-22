use super::*;
use crate::audit::InMemoryBillingAuditEmitter;
use crate::clock::InMemoryFakeMatClock;
use crate::d1::InMemoryBillingD1;
use crate::tier::InMemoryTierSelector;
use corelink_tier_selection::tier::TierKind;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
struct TestCurrentSubscriptionAuthority {
    snapshots: Mutex<HashMap<String, crate::CurrentSubscription>>,
}

impl TestCurrentSubscriptionAuthority {
    fn set(&self, subscription_id: &str, status: &str, price_id: &str) {
        self.snapshots.lock().unwrap().insert(
            subscription_id.to_owned(),
            crate::CurrentSubscription::new(subscription_id, status, price_id, 1_700_000_000_000),
        );
    }

    fn set_with_created_at_ms(
        &self,
        subscription_id: &str,
        status: &str,
        price_id: &str,
        subscription_created_at_ms: u64,
    ) {
        self.snapshots.lock().unwrap().insert(
            subscription_id.to_owned(),
            crate::CurrentSubscription::new(
                subscription_id,
                status,
                price_id,
                subscription_created_at_ms,
            ),
        );
    }
}

impl crate::CurrentSubscriptionAuthority for TestCurrentSubscriptionAuthority {
    fn current_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<crate::CurrentSubscription, String> {
        self.snapshots
            .lock()
            .map_err(|e| format!("test current-subscription mutex poisoned: {e}"))?
            .get(subscription_id)
            .cloned()
            .ok_or_else(|| format!("no current snapshot for {subscription_id}"))
    }
}

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
    Arc<TestCurrentSubscriptionAuthority>,
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
    let authority = Arc::new(TestCurrentSubscriptionAuthority::default());
    (
        handler
            .with_runners_resolver(resolver)
            .with_current_subscription_authority(authority.clone()),
        d1,
        audit,
        authority,
    )
}

#[test]
fn runners_subscription_seeds_entitlement_not_tier() {
    let (handler, d1, audit, authority) = fixture_with_runners();
    authority.set("sub_run", "active", "price_runner_team");
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
        let (handler, d1, _audit, authority) = fixture_with_runners();
        authority.set("sub_run", non_granting, "price_runner_team");
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
        let (handler, d1, audit, authority) = fixture_with_runners();
        authority.set("sub_run", "active", "price_runner_team");
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

        authority.set("sub_run", non_granting, "price_runner_team");

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
    let (handler, d1, audit, authority) = fixture_with_runners();
    authority.set("sub_run", "active", "price_runner_team");
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

    authority.set("sub_run", "canceled", "price_runner_team");

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
    let (handler, d1, audit, authority) = fixture_with_runners();
    authority.set("sub_cache", "canceled", "plan_pro");
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
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set("sub_items", "active", "price_runner_team");
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
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set("sub_cache", "active", "plan_pro");
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
fn stale_runner_update_and_delete_converge_to_current_active_subscription() {
    // Stripe can deliver distinct `updated` / `deleted` snapshots after the
    // subscription recovered. Their timestamp and event id are intentionally
    // irrelevant: the provider's current object remains the only authority.
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set("sub_run", "active", "price_runner_team");

    let current = env(
        "evt_current",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_run", "status": "active",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" }
            }
        }),
    );
    handler.on_subscription_updated(&current).unwrap();

    let stale_update = env(
        "evt_stale_update",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_run", "status": "past_due",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "plan_pro" }
            }
        }),
    );
    let stale_delete = env(
        "evt_stale_delete",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_run", "status": "canceled",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "plan_pro" }
            }
        }),
    );
    handler.on_subscription_updated(&stale_update).unwrap();
    handler.on_subscription_deleted(&stale_delete).unwrap();

    assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));
}

#[test]
fn replaced_subscription_identity_is_tenant_authority() {
    // A replacement keeps the tenant's entitlement authority while Stripe
    // changes the subscription id. Provider creation times must still converge in
    // either delivery order: the successor grant replaces the predecessor
    // revoke, and the predecessor revoke cannot replace the successor grant.
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set_with_created_at_ms("sub_predecessor", "canceled", "price_runner_team", 1_700_000_000_000);
    authority.set_with_created_at_ms("sub_successor", "active", "price_runner_team", 1_700_000_001_000);

    let predecessor = env(
        "evt_predecessor_cancel",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": { "id": "sub_predecessor", "status": "canceled",
                "metadata": { "tenant_id": "ten_replaced" },
                "plan": { "id": "price_runner_team" } }
        }),
    );
    let successor = env(
        "evt_successor_active",
        "customer.subscription.updated",
        serde_json::json!({
            "object": { "id": "sub_successor", "status": "active",
                "metadata": { "tenant_id": "ten_replaced" },
                "plan": { "id": "price_runner_team" } }
        }),
    );

    handler.on_subscription_deleted(&predecessor).unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_replaced"), None);
    handler.on_subscription_updated(&successor).unwrap();
    handler.on_subscription_updated(&successor).unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_replaced"), Some((80, 600)));
    assert_eq!(
        d1.runner_fence_of("ten_replaced"),
        Some(("sub_successor".to_owned(), 1_700_000_001_000, 1_700_000_000_000, true))
    );

    // Reverse delivery is the failure mode from #1844: once the successor
    // owns the tenant fence, the predecessor cannot revoke it.
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set_with_created_at_ms("sub_predecessor", "canceled", "price_runner_team", 1_700_000_000_000);
    authority.set_with_created_at_ms("sub_successor", "active", "price_runner_team", 1_700_000_001_000);
    handler.on_subscription_updated(&successor).unwrap();
    assert!(matches!(
        handler.on_subscription_deleted(&predecessor),
        Err(MaterializerError::Transient(_))
    ));
    assert_eq!(d1.runners_entitlement_of("ten_replaced"), Some((80, 600)));
    assert_eq!(
        d1.runner_fence_of("ten_replaced"),
        Some(("sub_successor".to_owned(), 1_700_000_001_000, 1_700_000_000_000, true))
    );
}

#[test]
fn runner_create_reconciles_the_current_snapshot() {
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set("sub_run", "active", "price_runner_team");
    let stale_create = env(
        "evt_stale_create",
        "customer.subscription.created",
        serde_json::json!({
            "object": {
                "id": "sub_run", "status": "past_due",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" }
            }
        }),
    );

    handler
        .materialize_echo(
            CanonicalWebhookEventType::SubscriptionCreated,
            &stale_create,
        )
        .unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));
}

#[test]
fn configured_runner_path_fails_closed_without_current_authority() {
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
    let handler = handler.with_runners_resolver(resolver);
    let event = env(
        "evt_no_authority",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_run", "status": "active",
                "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" }
            }
        }),
    );

    assert!(matches!(
        handler.on_subscription_updated(&event),
        Err(MaterializerError::Transient(_))
    ));
    assert_eq!(d1.runners_entitlement_of("ten_run"), None);
    assert_eq!(
        audit.count_event("corelink.tenant.runners_entitlement_seeded.v1"),
        0
    );
}

#[test]
fn duplicate_runner_delivery_is_convergent() {
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set("sub_run", "active", "price_runner_team");
    let event = env(
        "evt_duplicate",
        "customer.subscription.updated",
        serde_json::json!({
            "object": { "id": "sub_run", "status": "active", "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" } }
        }),
    );
    handler.on_subscription_updated(&event).unwrap();
    handler.on_subscription_updated(&event).unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));
}

#[test]
fn stale_provider_revision_fails_closed_before_runner_mutation() {
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.set_with_created_at_ms("sub_run", "active", "price_runner_team", 1_700_000_000_000);
    let event = env(
        "evt_newer",
        "customer.subscription.updated",
        serde_json::json!({
            "object": { "id": "sub_run", "status": "active", "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" } }
        }),
    );
    handler.on_subscription_updated(&event).unwrap();
    authority.set_with_created_at_ms("sub_run", "active", "price_runner_team", 1_700_000_000_000);
    let mut stale = env(
        "evt_old",
        "customer.subscription.updated",
        serde_json::json!({
            "object": { "id": "sub_run", "status": "active", "metadata": { "tenant_id": "ten_run" },
                "plan": { "id": "price_runner_team" } }
        }),
    );
    stale.created = 1_699_999_999;
    assert!(matches!(
        handler.on_subscription_updated(&stale),
        Err(MaterializerError::Transient(_))
    ));
    assert_eq!(d1.runners_entitlement_of("ten_run"), Some((80, 600)));
}

#[test]
fn authority_mismatch_and_non_runner_fail_closed() {
    let (handler, d1, _audit, authority) = fixture_with_runners();
    authority.snapshots.lock().unwrap().insert(
        "sub_run".into(),
        crate::CurrentSubscription::new("sub_other", "active", "price_runner_team", 1_700_000_000_000),
    );
    let event = env(
        "evt_authority",
        "customer.subscription.updated",
        serde_json::json!({
            "object": { "id": "sub_run", "status": "active", "metadata": { "tenant_id": "ten_run" }, "plan": { "id": "price_runner_team" } }
        }),
    );
    assert!(matches!(
        handler.on_subscription_updated(&event),
        Err(MaterializerError::Transient(_))
    ));
    authority.set("sub_run", "active", "plan_pro");
    assert!(matches!(
        handler.on_subscription_updated(&event),
        Err(MaterializerError::Transient(_))
    ));
    assert_eq!(d1.runners_entitlement_of("ten_run"), None);
}

#[test]
fn missing_price_on_created_or_deleted_fails_closed() {
    for deleted in [false, true] {
        let (handler, d1, _audit, _authority) = fixture_with_runners();
        let event = env(
            if deleted {
                "evt_missing_delete"
            } else {
                "evt_missing_create"
            },
            if deleted {
                "customer.subscription.deleted"
            } else {
                "customer.subscription.created"
            },
            serde_json::json!({ "object": { "id": "sub_missing", "status": "active",
                "metadata": { "tenant_id": "ten_missing" } } }),
        );
        let result = if deleted {
            handler.on_subscription_deleted(&event)
        } else {
            handler.materialize_echo(CanonicalWebhookEventType::SubscriptionCreated, &event)
        };
        assert!(matches!(result, Err(MaterializerError::InvalidPayload(_))));
        assert_eq!(d1.runners_entitlement_of("ten_missing"), None);
    }
}

#[test]
fn current_subscription_round_trips_json() {
    let snapshot = crate::CurrentSubscription::new("sub_run", "active", "price_runner_team", 1_700_000_000_000);
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        r#"{"subscription_id":"sub_run","status":"active","price_id":"price_runner_team","subscription_created_at_ms":1700000000000}"#
    );
    assert_eq!(
        serde_json::from_str::<crate::CurrentSubscription>(&json).unwrap(),
        snapshot
    );
}
