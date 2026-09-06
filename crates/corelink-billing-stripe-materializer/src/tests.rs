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

#[test]
fn tier_sql_unchanged_and_schema_correct_0039() {
    // The #172 fix — left intact, re-pinned here as the shared owner.
    assert_eq!(SQL_UPSERT_TIER.matches('?').count(), 4, "{SQL_UPSERT_TIER}");
    assert!(SQL_UPSERT_TIER.contains("'active'"), "{SQL_UPSERT_TIER}");
    assert!(
        SQL_UPSERT_TIER.contains("subscription_started_at_ms"),
        "{SQL_UPSERT_TIER}"
    );
    assert!(
        !SQL_UPSERT_TIER.contains("materialized_at_ms"),
        "must NOT reference materialized_at_ms: {SQL_UPSERT_TIER}"
    );
    assert!(
        SQL_READ_TIER.contains("WHERE tenant_id = ?"),
        "{SQL_READ_TIER}"
    );
}

#[test]
fn downgrade_tier_sql_writes_inactive_not_active_and_preserves_start_0039() {
    // CAA-360 MEDIUM fix: the cancel/downgrade path MUST write
    // subscription_state='inactive' (access gate OFF), NEVER 'active'
    // (which would leave a contradictory active-free row), and MUST NOT
    // reset subscription_started_at_ms in the DO UPDATE (a downgrade
    // preserves the original subscription start).
    let sql = SQL_DOWNGRADE_TIER;
    assert_eq!(sql.matches('?').count(), 4, "4 binds: {sql}");
    assert!(sql.contains("INTO tier_selections"), "wrong table: {sql}");
    assert!(sql.contains("ON CONFLICT(tenant_id)"), "wrong key: {sql}");
    // Writes 'inactive' on BOTH the INSERT VALUES and the DO UPDATE.
    assert!(
        sql.contains("'inactive'"),
        "must write the 'inactive' state: {sql}"
    );
    assert!(
        sql.contains("subscription_state = 'inactive'"),
        "DO UPDATE must set subscription_state='inactive': {sql}"
    );
    // MUST NOT write 'active' anywhere (the contradictory-row bug).
    assert!(
        !sql.contains("'active'"),
        "cancel path must NEVER write subscription_state='active': {sql}"
    );
    // The DO UPDATE must NOT reset the original subscription start — the
    // downgrade preserves it (only the grant path refreshes it). No
    // `subscription_started_at_ms = excluded.` clause may appear.
    assert!(
        !sql.contains("subscription_started_at_ms = excluded"),
        "downgrade must NOT reset subscription_started_at_ms on conflict: {sql}"
    );
}

/// Stripe-cancel tier-race fix: the container's cancel path must converge
/// with the signup-worker authority
/// (`apps/signup-worker/src/webhooks/stripe.ts`
/// `deactivateTierSelectionBySubscription`), which sets
/// `subscription_state='inactive'` and deliberately leaves `tier`
/// untouched. `SQL_DOWNGRADE_TIER`'s DO UPDATE must therefore NEVER
/// overwrite `tier` (previously `tier = excluded.tier`, which raced the
/// authority and stomped the historical tier label to whatever
/// `persist_tier_downgrade` passed in, e.g. `'free'`).
#[test]
fn downgrade_tier_sql_do_update_never_overwrites_tier_column() {
    let sql = SQL_DOWNGRADE_TIER;
    assert!(
        !sql.contains("tier = excluded.tier"),
        "cancel DO UPDATE must NOT overwrite the tier column \
             (the signup-worker is the tier authority on cancel): {sql}"
    );
    // The DO UPDATE clause itself must only touch subscription_state +
    // correlation_id — assert on the exact clause so a future edit that
    // reintroduces a tier write (even under a different token spelling)
    // is caught.
    let do_update = sql
        .split("DO UPDATE SET ")
        .nth(1)
        .expect("SQL must have a DO UPDATE SET clause");
    assert_eq!(
        do_update, "subscription_state = 'inactive', correlation_id = excluded.correlation_id",
        "DO UPDATE must set ONLY subscription_state + correlation_id: {sql}"
    );
}

// --- Behavioral pins of `SQL_UPSERT_TIER` against the REAL DDLs --------
//
// The signup-worker (PRIMARY billing authority,
// `apps/signup-worker/src/webhooks/stripe.ts` "must NOT resurrect a
// canceled subscription") guards its tier UPSERT at the SQL level with
// `AND status != 'canceled'`. The container materializer is the
// defense-in-depth SECOND writer and MUST converge to the same rule:
// a STALE `customer.subscription.updated(status=active)` redelivery that
// lands AFTER the `deleted` was processed must not flip the canceled
// tenant's access gate back ON.
//
// These tests execute [`SQL_UPSERT_TIER`] verbatim against in-memory
// SQLite loaded with migrations 0039 + 0055 exactly as shipped.

const DDL_0039: &str = include_str!("../../../migrations/d1/0039_tier_selection.sql");
const DDL_0055: &str = include_str!("../../../migrations/d1/0055_tenant_billing.sql");

fn billing_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory sqlite");
    conn.execute_batch(DDL_0039).expect("0039 DDL applies");
    conn.execute_batch(DDL_0055).expect("0055 DDL applies");
    conn
}

/// Seed a pre-existing `tier_selections` row in the given state via the
/// production downgrade statement (the real way an 'inactive' row comes
/// into being on cancel).
fn seed_tier_row(conn: &rusqlite::Connection, tenant_id: &str, tier: &str) {
    conn.execute(
        SQL_DOWNGRADE_TIER,
        rusqlite::params![tenant_id, tier, 1_000_i64, "seed"],
    )
    .expect("seed tier row");
}

fn seed_billing_status(conn: &rusqlite::Connection, tenant_id: &str, status: &str) {
    conn.execute(
        "INSERT INTO tenant_billing (tenant_id, status, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, 1_000, 1_000) \
             ON CONFLICT(tenant_id) DO UPDATE SET status = excluded.status",
        rusqlite::params![tenant_id, status],
    )
    .expect("seed tenant_billing row");
}

/// Run [`SQL_UPSERT_TIER`] exactly as the binders do — 4 positional binds:
/// (tenant_id, tier, subscription_started_at_ms, correlation_id).
fn run_upsert_tier(conn: &rusqlite::Connection, tenant_id: &str, tier: &str) {
    // rows_affected is 1 on grant/insert, **0 when the cancel guard
    // suppressed the DO UPDATE** — which is itself evidence the guard
    // fired. The state assertion below is the real oracle.
    let _ = conn
        .execute(
            SQL_UPSERT_TIER,
            rusqlite::params![tenant_id, tier, 2_000_i64, "evt_test"],
        )
        .expect("SQL_UPSERT_TIER executes against real DDLs");
}

fn tier_state(conn: &rusqlite::Connection, tenant_id: &str) -> String {
    conn.query_row(
        "SELECT subscription_state FROM tier_selections WHERE tenant_id = ?1",
        rusqlite::params![tenant_id],
        |r| r.get::<_, String>(0),
    )
    .expect("tier row exists")
}

/// T1 (THE BUG): a stale `subscription.updated(active)` arriving AFTER the
/// cancel must NOT resurrect access for a tenant whose canonical billing
/// status is `canceled`.
#[test]
fn upsert_tier_does_not_resurrect_canceled_subscription() {
    let conn = billing_db();
    seed_tier_row(&conn, "ten_cancel", "pro");
    seed_billing_status(&conn, "ten_cancel", "canceled");

    run_upsert_tier(&conn, "ten_cancel", "pro");

    assert_ne!(
        tier_state(&conn, "ten_cancel"),
        "active",
        "stale active event must NOT resurrect a canceled subscription"
    );
}

/// T2 (non-regression): with billing `paid`, the grant path still grants.
#[test]
fn upsert_tier_still_grants_when_billing_paid() {
    let conn = billing_db();
    seed_tier_row(&conn, "ten_paid", "pro");
    seed_billing_status(&conn, "ten_paid", "paid");

    run_upsert_tier(&conn, "ten_paid", "pro");

    assert_eq!(tier_state(&conn, "ten_paid"), "active");
}

/// T3 (non-regression): first purchase — no `tenant_billing` row yet —
/// must never be blocked by the cancel guard.
#[test]
fn upsert_tier_first_purchase_without_billing_row_grants() {
    let conn = billing_db();
    seed_tier_row(&conn, "ten_new", "free");
    assert!(
        !conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tenant_billing WHERE tenant_id = ?1)",
                rusqlite::params!["ten_new"],
                |r| r.get::<_, bool>(0),
            )
            .unwrap(),
        "fixture requires no tenant_billing row"
    );

    run_upsert_tier(&conn, "ten_new", "team");

    assert_eq!(tier_state(&conn, "ten_new"), "active");
}

/// T4 (KNOWN + ACCEPTED residual): the guard lives ONLY in the DO UPDATE
/// `WHERE`, so the INSERT branch (no prior `tier_selections` row) is NOT
/// blocked even when `tenant_billing.status='canceled'`. In practice this
/// state cannot arise from the cancel path — cancellation writes
/// `'inactive'` (never deletes), so "no tier row" means "never had a
/// tier". Do NOT close this by rewriting as INSERT…SELECT…WHERE NOT
/// EXISTS — that breaks the ON CONFLICT semantics + bind arity. Pinned
/// here so any accidental change to this trade-off is a conscious one.
#[test]
fn upsert_tier_insert_branch_is_not_gated_known_residual() {
    let conn = billing_db();
    seed_billing_status(&conn, "ten_never", "canceled");

    run_upsert_tier(&conn, "ten_never", "pro");

    assert_eq!(
        tier_state(&conn, "ten_never"),
        "active",
        "INSERT branch bypasses the cancel guard — documented residual"
    );
}
