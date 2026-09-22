use super::*;

fn row(table: &str) -> MaterializedRow {
    MaterializedRow {
        table: table.to_string(),
        tenant_id: "ten_1".to_string(),
        stripe_id: "obj_1".to_string(),
        stripe_event_id: "evt_1".to_string(),
        payload: serde_json::json!({}),
        materialized_at_ms: 1_700_000_000_000,
    }
}

fn runner_revision(
    subscription_id: &str,
    subscription_created_at_ms: u64,
    stripe_event_created_at_ms: u64,
    stripe_event_id: &str,
) -> RunnerEntitlementRevision<'_> {
    RunnerEntitlementRevision {
        subscription_id,
        subscription_created_at_ms,
        stripe_event_created_at_ms,
        stripe_event_id,
    }
}

#[test]
fn customer_upsert_rejects_wrong_table() {
    let d1 = InMemoryBillingD1::new();
    let err = d1.upsert_customer(row("not_stripe_customers")).unwrap_err();
    assert!(matches!(err, BillingD1Error::InvalidPayload(_)));
}

#[test]
fn idempotency_record_returns_false_on_replay() {
    let d1 = InMemoryBillingD1::new();
    assert!(d1
        .try_record_event("evt_1", "invoice.paid", 1, WebhookOutcome::Dispatched)
        .unwrap());
    assert!(!d1
        .try_record_event("evt_1", "invoice.paid", 2, WebhookOutcome::Dispatched)
        .unwrap());
    assert_eq!(d1.distinct_events(), 1);
}

#[test]
fn outcome_recorded_as_acknowledged_unknown_for_unknown_events() {
    // FAILS on the old code: the old writer had no `outcome` param at
    // all, so this could not even compile against it — and the old SQL
    // hardcoded 'dispatched' regardless of what the caller intended.
    let d1 = InMemoryBillingD1::new();
    assert!(d1
        .try_record_event(
            "evt_unknown_1",
            "some.future.event",
            1,
            WebhookOutcome::AcknowledgedUnknown,
        )
        .unwrap());
    assert_eq!(
        d1.outcome_for("evt_unknown_1"),
        Some(WebhookOutcome::AcknowledgedUnknown)
    );
}

#[test]
fn outcome_recorded_as_dispatched_for_handled_events() {
    let d1 = InMemoryBillingD1::new();
    assert!(d1
        .try_record_event(
            "evt_handled_1",
            "invoice.paid",
            1,
            WebhookOutcome::Dispatched
        )
        .unwrap());
    assert_eq!(
        d1.outcome_for("evt_handled_1"),
        Some(WebhookOutcome::Dispatched)
    );
}

#[test]
fn arm_failure_short_circuits_writes() {
    let d1 = InMemoryBillingD1::new();
    d1.arm_failure(BillingD1Error::Transient("d1 down".into()));
    let err = d1.upsert_customer(row("stripe_customers")).unwrap_err();
    assert!(matches!(err, BillingD1Error::Transient(_)));
    // Clear and try again — should now succeed.
    d1.clear_failure();
    d1.upsert_customer(row("stripe_customers")).unwrap();
    assert_eq!(d1.count_table("stripe_customers"), 1);
}

#[test]
fn runners_entitlement_seed_then_revoke_then_idempotent() {
    let d1 = InMemoryBillingD1::new();
    assert_eq!(d1.runners_entitlement_of("ten_1"), None);
    d1.upsert_runners_entitlement("ten_1", 80, 600, 1_700_000_000_000)
        .unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_1"), Some((80, 600)));
    // Revoke removes the entry.
    d1.delete_runners_entitlement("ten_1").unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_1"), None);
    // Revoking an absent entitlement is an idempotent no-op.
    d1.delete_runners_entitlement("ten_1").unwrap();
    assert_eq!(d1.runners_entitlement_of("ten_1"), None);
}

#[test]
fn runners_entitlement_cas_is_idempotent_and_rejects_stale_provider_revisions() {
    let d1 = InMemoryBillingD1::new();
    assert_eq!(
        d1.cas_runners_entitlement("ten_1", runner_revision("sub_run", 1_700_000_000_000, 2_000, "evt_2"), Some((80, 600)), 2)
            .unwrap(),
        EntitlementCasOutcome::Applied
    );
    assert_eq!(d1.runners_entitlement_of("ten_1"), Some((80, 600)));
    assert_eq!(
        d1.cas_runners_entitlement("ten_1", runner_revision("sub_run", 1_700_000_000_000, 2_000, "evt_2"), Some((80, 600)), 3)
            .unwrap(),
        EntitlementCasOutcome::Duplicate
    );
    assert_eq!(
        d1.cas_runners_entitlement("ten_1", runner_revision("sub_run", 1_700_000_000_000, 1_000, "evt_1"), None, 4)
            .unwrap(),
        EntitlementCasOutcome::Stale
    );
    assert_eq!(
        d1.runners_entitlement_of("ten_1"),
        Some((80, 600)),
        "stale revoke must not remove a newer grant"
    );
    assert_eq!(
        d1.cas_runners_entitlement("ten_1", runner_revision("sub_run", 1_700_000_000_000, 3_000, "evt_3"), None, 5)
            .unwrap(),
        EntitlementCasOutcome::Applied
    );
    assert_eq!(d1.runners_entitlement_of("ten_1"), None);
    assert_eq!(
        d1.cas_runners_entitlement("ten_1", runner_revision("sub_run", 1_700_000_000_000, 2_000, "evt_2"), Some((80, 600)), 6)
            .unwrap(),
        EntitlementCasOutcome::Stale
    );
}

#[test]
fn runner_cas_total_orders_same_second_replacement_identities() {
    let d1 = InMemoryBillingD1::new();
    // Stripe exposes whole-second subscription creation times. When two
    // replacement identities share that second, their immutable provider ids
    // decide the winner before either event timestamp is considered.
    assert_eq!(
        d1.cas_runners_entitlement(
            "ten_equal", runner_revision("sub_aaa_predecessor", 1_700_000_000_000, 5_000,
            "evt_predecessor_grant"), Some((80, 600)), 1,
        ).unwrap(),
        EntitlementCasOutcome::Applied,
    );
    assert_eq!(
        d1.cas_runners_entitlement(
            "ten_equal", runner_revision("sub_zzz_successor", 1_700_000_000_000, 4_000,
            "evt_successor_cancel"), None, 2,
        ).unwrap(),
        EntitlementCasOutcome::Applied,
    );
    assert_eq!(d1.runners_entitlement_of("ten_equal"), None);
    // A replay of the lexically lower predecessor cannot re-grant after the
    // successor cancellation, even with a later event timestamp.
    assert_eq!(
        d1.cas_runners_entitlement(
            "ten_equal", runner_revision("sub_aaa_predecessor", 1_700_000_000_000, 9_000,
            "evt_predecessor_replay"), Some((80, 600)), 3,
        ).unwrap(),
        EntitlementCasOutcome::Stale,
    );
    assert_eq!(d1.runners_entitlement_of("ten_equal"), None);
}

#[test]
fn runner_cas_sql_keeps_fence_and_mutation_tenant_scoped() {
    assert!(SQL_ADVANCE_RUNNER_ENTITLEMENT_FENCE.contains("ON CONFLICT(tenant_id)"));
    assert!(SQL_ADVANCE_RUNNER_ENTITLEMENT_FENCE.contains("is_granting"));
    assert!(SQL_ADVANCE_RUNNER_ENTITLEMENT_FENCE.contains("subscription_created_at_ms"));
    assert!(SQL_ADVANCE_RUNNER_ENTITLEMENT_FENCE.contains("stripe_event_id"));
    assert!(SQL_ADVANCE_RUNNER_ENTITLEMENT_FENCE.contains("stripe_subscription_id >"));
    assert!(SQL_CAS_UPSERT_RUNNERS_ENTITLEMENT.contains("stripe_event_created_at_ms"));
    assert!(SQL_CAS_UPSERT_RUNNERS_ENTITLEMENT.contains("stripe_subscription_id = ?"));
    assert!(SQL_CAS_DELETE_RUNNERS_ENTITLEMENT.contains("subscription_created_at_ms"));
    assert!(SQL_CAS_DELETE_RUNNERS_ENTITLEMENT.contains("stripe_subscription_id = ?"));
    assert!(SQL_CAS_UPSERT_RUNNERS_ENTITLEMENT.contains("WHERE EXISTS"));
    assert!(SQL_CAS_DELETE_RUNNERS_ENTITLEMENT.contains("WHERE tenant_id = ?"));
}

#[test]
fn delete_runners_entitlement_sql_shape() {
    let sql = SQL_DELETE_RUNNERS_ENTITLEMENT;
    assert!(
        sql.trim_start().to_ascii_lowercase().starts_with("delete"),
        "{sql}"
    );
    assert!(sql.contains("FROM runners_entitlement"), "{sql}");
    assert!(
        sql.contains("WHERE tenant_id = ?"),
        "must be tenant-scoped by a single bind: {sql}"
    );
    assert_eq!(sql.matches('?').count(), 1, "single tenant_id bind: {sql}");
}

#[test]
fn downgrade_tier_preserves_existing_tier_never_writes_free() {
    // Stripe-cancel tier-race fix: on an EXISTING row, downgrade_tier must
    // preserve `tier` (mirrors SQL_DOWNGRADE_TIER no longer overwriting it)
    // even though the caller passes `tier_wire="free"` — only
    // `subscription_state` (tracked out-of-band by production; this mirror
    // only tracks `tier`) may change. FAILS on the old behaviour (which
    // unconditionally overwrote the entry to `("free", …)`).
    let d1 = InMemoryBillingD1::new();
    d1.upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
        .unwrap();
    d1.downgrade_tier("ten_1", "free", 1_700_000_100_000, "evt_cancel")
        .unwrap();
    assert_eq!(
        d1.tier_for("ten_1"),
        Some("pro".to_string()),
        "cancel must NOT overwrite the historical tier label to 'free'"
    );
}

#[test]
fn downgrade_tier_seeds_tier_wire_when_no_row_exists() {
    // The INSERT (new-row) branch still needs a tier value for the
    // NOT-NULL column — only the conflict/existing-row branch preserves.
    let d1 = InMemoryBillingD1::new();
    assert_eq!(d1.tier_for("ten_new"), None);
    d1.downgrade_tier("ten_new", "free", 1_700_000_000_000, "evt_cancel")
        .unwrap();
    assert_eq!(d1.tier_for("ten_new"), Some("free".to_string()));
}

#[test]
fn tier_read_then_upsert_then_read_back() {
    let d1 = InMemoryBillingD1::new();
    assert_eq!(d1.read_tier("ten_1").unwrap(), None);
    d1.upsert_tier("ten_1", "pro", 1_700_000_000_000, "corr_1")
        .unwrap();
    assert_eq!(d1.read_tier("ten_1").unwrap(), Some("pro".to_string()));
}

// ───────────────────────────────────────────────────────────────────────
// Canonical SQL shape pins — reconcile the shared literals against the
// DEPLOYED schema (`migrations/d1/0048_*` + `0044_*` + `0039_*`). A
// regression here is a money-path launch-blocker (a wrong column name
// is a 100% silent write failure in production), so these run under the
// default `cargo test --lib` (NOT gated behind `cf-billing-real`).
// ───────────────────────────────────────────────────────────────────────

/// The JSON column is `payload_json` on EVERY 0048 table — the old
/// `payload` name does not exist and would fail every INSERT.
#[test]
fn no_materializer_sql_references_legacy_payload_column() {
    for sql in [
        SQL_UPSERT_CUSTOMER,
        SQL_UPSERT_SUBSCRIPTION,
        SQL_MARK_SUBSCRIPTION_CANCELED,
        SQL_UPSERT_INVOICE,
        SQL_INSERT_DISPUTE,
        SQL_INSERT_REFUND,
    ] {
        assert!(
            sql.contains("payload_json"),
            "must use the deployed `payload_json` column: {sql}"
        );
        // The bare token `payload ` (with a trailing space / paren)
        // must never appear — only `payload_json`.
        assert!(
            !sql.contains("payload ") && !sql.contains("payload,") && !sql.contains("payload)"),
            "must NOT reference the non-existent `payload` column: {sql}"
        );
    }
}

#[test]
fn customer_upsert_matches_0048() {
    let sql = SQL_UPSERT_CUSTOMER;
    assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
    assert!(sql.contains("INSERT INTO stripe_customers"), "{sql}");
    // Natural PK conflict target — NOT a composite (tenant_id, …).
    assert!(sql.contains("ON CONFLICT(stripe_customer_id)"), "{sql}");
    assert!(!sql.contains("ON CONFLICT(tenant_id"), "{sql}");
    // tenant_id is the first bound column (verify_first_bind contract).
    assert!(
        sql.contains("(tenant_id, stripe_customer_id"),
        "tenant_id must be bind #1: {sql}"
    );
}

#[test]
fn subscription_upsert_binds_status_not_null_0048() {
    let sql = SQL_UPSERT_SUBSCRIPTION;
    assert_eq!(sql.matches('?').count(), 6, "6 binds incl. status: {sql}");
    assert!(sql.contains("INSERT INTO stripe_subscriptions"), "{sql}");
    assert!(sql.contains("ON CONFLICT(stripe_subscription_id)"), "{sql}");
    // `status` is NOT NULL with no default → MUST be bound + upserted.
    assert!(sql.contains("status"), "must bind NOT-NULL status: {sql}");
    assert!(
        sql.contains("status = excluded.status"),
        "must refresh status on conflict: {sql}"
    );
}

#[test]
fn mark_canceled_sets_status_and_is_tenant_scoped() {
    let sql = SQL_MARK_SUBSCRIPTION_CANCELED;
    assert!(
        sql.trim_start().to_ascii_lowercase().starts_with("update"),
        "{sql}"
    );
    assert!(
        sql.contains("status = 'canceled'"),
        "cancel must set status='canceled': {sql}"
    );
    assert!(
        sql.contains("WHERE tenant_id = ?"),
        "UPDATE must be tenant-scoped: {sql}"
    );
    assert!(sql.contains("stripe_subscription_id = ?"), "{sql}");
}

#[test]
fn invoice_upsert_binds_outcome_not_null_0048() {
    let sql = SQL_UPSERT_INVOICE;
    assert_eq!(sql.matches('?').count(), 6, "6 binds incl. outcome: {sql}");
    assert!(sql.contains("INSERT INTO stripe_invoices"), "{sql}");
    assert!(sql.contains("ON CONFLICT(stripe_invoice_id)"), "{sql}");
    // `outcome` is NOT NULL + CHECK IN('paid','payment_failed').
    assert!(sql.contains("outcome"), "must bind NOT-NULL outcome: {sql}");
    assert!(
        sql.contains("outcome = excluded.outcome"),
        "must refresh outcome on conflict: {sql}"
    );
}

#[test]
fn dispute_insert_matches_0048() {
    let sql = SQL_INSERT_DISPUTE;
    assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
    assert!(sql.contains("INSERT INTO stripe_disputes"), "{sql}");
    assert!(sql.contains("stripe_dispute_id"), "{sql}");
    // severity + schema_version have DEFAULTs and are intentionally omitted.
    assert!(
        !sql.contains("severity"),
        "DEFAULTed severity omitted: {sql}"
    );
}

#[test]
fn refund_insert_uses_charge_id_pk_0048() {
    let sql = SQL_INSERT_REFUND;
    assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
    assert!(sql.contains("INSERT INTO stripe_refunds"), "{sql}");
    // PK is the parent charge id; the non-existent stripe_refund_id
    // column must NEVER appear (the latent bug).
    assert!(sql.contains("stripe_charge_id"), "{sql}");
    assert!(
        !sql.contains("stripe_refund_id"),
        "must NOT reference the non-existent stripe_refund_id: {sql}"
    );
}

#[test]
fn webhook_dedup_insert_matches_0044_untenanted() {
    let sql = SQL_INSERT_WEBHOOK_EVENT_PROCESSED;
    assert!(
        sql.contains("INSERT INTO stripe_webhook_events_processed"),
        "{sql}"
    );
    // Deployed 0044 columns — NOT the old (tenant_id, stripe_event_id,
    // canonical_event_type, processed_at_ms) shape.
    for col in [
        "event_id",
        "event_type",
        "processed_at_ms",
        "outcome",
        "correlation_id",
    ] {
        assert!(sql.contains(col), "missing deployed column `{col}`: {sql}");
    }
    // The table is un-tenanted — no tenant_id column exists.
    assert!(!sql.contains("tenant_id"), "0044 has no tenant_id: {sql}");
    assert!(
        !sql.contains("canonical_event_type"),
        "renamed→event_type: {sql}"
    );
    // outcome is now a BOUND parameter, not a hardcoded literal — the
    // caller decides `dispatched` vs `acknowledged_unknown` per event
    // (see `WebhookOutcome`). The old 'dispatched' literal must be gone.
    assert!(
        !sql.contains("'dispatched'"),
        "outcome must be a bind, not a hardcoded literal: {sql}"
    );
    // RETURNING lets the native writer detect insert-vs-ignore.
    assert!(sql.contains("RETURNING event_id"), "{sql}");
    // 5 binds: event_id, event_type, processed_at_ms, outcome, correlation_id.
    assert_eq!(sql.matches('?').count(), 5, "5 binds: {sql}");
}
