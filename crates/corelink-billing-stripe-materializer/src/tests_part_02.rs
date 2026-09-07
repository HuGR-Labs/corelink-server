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
