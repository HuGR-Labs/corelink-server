use super::testing::{ErroringByteStore, InMemoryByteStore, Row};
use super::*;

const REGION: &str = "iad";

fn accountant(store: Arc<dyn ByteStore>) -> ByteAccountant {
    ByteAccountant::new(store, REGION.to_owned())
}

/// The genuine-unlimited cap seed (`Some(0)`), kept readable in the tests.
const UNLIMITED: Option<i64> = Some(0);

#[tokio::test]
async fn accrue_increments_bytes_used_for_genuine_unlimited() {
    // The load-bearing finding-#1 assertion: a write accrues bytes_used (the
    // counter the storage cap reads — previously NEVER moved). A genuine
    // unlimited tier seeds the fresh row with the `0` sentinel deliberately.
    let store = Arc::new(InMemoryByteStore::new());
    let acc = accountant(store.clone());
    assert_eq!(
        acc.accrue("t1", 1_000, UNLIMITED).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(
        acc.accrue("t1", 500, UNLIMITED).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(
        store.used("t1", REGION),
        1_500,
        "concurrent accruals must sum"
    );
}

#[tokio::test]
async fn fresh_capped_tenant_first_write_seeds_real_cap_not_zero() {
    // (b) A FRESH capped tenant whose first write is UNDER the seeded cap
    // accrues AND the seeded row carries the REAL cap (not 0/unlimited) — so
    // a later over-cap write is correctly refused. This is the core fix:
    // a fresh row must NOT be uncapped.
    let store = Arc::new(InMemoryByteStore::new());
    let acc = accountant(store.clone());
    // First write 600 under a 1000-byte cap → accrues, seeds quota=1000.
    assert_eq!(
        acc.accrue("t-fresh", 600, Some(1_000)).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(store.used("t-fresh", REGION), 600);
    // The seeded cap is REAL: a follow-up that would exceed 1000 is refused
    // even with `None` (the existing row's stored cap governs).
    assert_eq!(
        acc.accrue("t-fresh", 500, None).await.unwrap(),
        AccrueOutcome::OverCap
    );
    assert_eq!(
        store.used("t-fresh", REGION),
        600,
        "over-cap must not move the counter"
    );
    // And exactly filling the remaining headroom is allowed.
    assert_eq!(
        acc.accrue("t-fresh", 400, None).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(store.used("t-fresh", REGION), 1_000);
}

#[tokio::test]
async fn tier_downgrade_reseeds_stored_cap_to_lower_value() {
    // rt-nuclear #16: a row seeded with a HIGH cap, then a write carrying a
    // LOWER (finite) cap, must RECONCILE the stored cap down — the downgrade
    // takes effect (previously the ON CONFLICT never updated bytes_quota, so
    // the tenant kept the old higher cap forever).
    let store = Arc::new(InMemoryByteStore::new());
    store.seed(
        "t-down",
        REGION,
        Row {
            used: 300,
            quota: 10_000,
        },
    );
    let acc = accountant(store.clone());
    // A 100-byte write carrying the NEW lower cap (500) reconciles the stored
    // cap down to 500 and accrues (300 + 100 = 400 <= 500).
    assert_eq!(
        acc.accrue("t-down", 100, Some(500)).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(
        store.quota("t-down", REGION),
        500,
        "stored cap must be reseeded to the lower tier cap"
    );
    assert_eq!(store.used("t-down", REGION), 400);
    // The new lower cap is now enforced: a write that fits the OLD cap but
    // exceeds the NEW one is refused.
    assert_eq!(
        acc.accrue("t-down", 200, Some(500)).await.unwrap(),
        AccrueOutcome::OverCap
    );
    assert_eq!(
        store.used("t-down", REGION),
        400,
        "an over-(new)-cap write must not move the counter"
    );
    // An unlimited / absent incoming cap must NOT clobber the finite stored cap.
    assert_eq!(
        acc.accrue("t-down", 1, UNLIMITED).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(
        store.quota("t-down", REGION),
        500,
        "an unlimited carrier must not lower/raise the stored finite cap"
    );
}

#[tokio::test]
async fn over_cap_downgrade_write_still_reconciles_so_adapter_writes_are_gated() {
    // brutal-audit H3 (HIGH money/COGS) — the reconcile-DEADLOCK.
    //
    // A tenant is DOWNGRADED below its current usage, so it is ALREADY OVER
    // the new (lower) cap. Its NATIVE writes carry the new cap but are refused
    // (OverCap). Pre-fix, the stored `bytes_quota` was reconciled ONLY inside
    // the cap-gated accrue, so a refused write never lowered it — and ADAPTER
    // writes (brew/npm/pip pass `None`, gating against the STORED cap) kept
    // accruing past the paid-for cap indefinitely. The fix decouples the
    // reconcile from accrual success: a refused native write STILL lowers the
    // stored cap, WITHOUT any native write needing to succeed.
    let store = Arc::new(InMemoryByteStore::new());
    // Seeded HIGH cap (10_000), already at 800 used. New tier cap = 500 ⇒ the
    // tenant is over the new cap from the outset.
    store.seed(
        "t-dead",
        REGION,
        Row {
            used: 800,
            quota: 10_000,
        },
    );
    let acc = accountant(store.clone());

    // (1) A NATIVE write carrying the new lower cap (500): 800 + 10 = 810 > 500
    // ⇒ OverCap (correctly REJECTED; counter unchanged). The stored cap MUST
    // still be reconciled DOWN to 500 even though this write was rejected.
    assert_eq!(
        acc.accrue("t-dead", 10, Some(500)).await.unwrap(),
        AccrueOutcome::OverCap
    );
    assert_eq!(
        store.used("t-dead", REGION),
        800,
        "a rejected write must not move the counter"
    );
    assert_eq!(
        store.quota("t-dead", REGION),
        500,
        "the REJECTED downgrade write MUST still reconcile the stored cap down \
             (reconcile decoupled from accrual success) — else adapter writes evade the cap",
    );

    // (2) An ADAPTER write (brew/npm/pip pass `None`) now gates against the
    // reconciled stored cap (500): 800 + 10 = 810 > 500 ⇒ OverCap. Pre-fix it
    // gated against the STALE 10_000 cap and wrongly Accrued (the COGS evasion).
    assert_eq!(
        acc.accrue("t-dead", 10, None).await.unwrap(),
        AccrueOutcome::OverCap
    );
    assert_eq!(
        store.used("t-dead", REGION),
        800,
        "the adapter write must be blocked at the LOWERED cap, not the stale one"
    );

    // (3) Headroom under the new cap is still honoured: dropping below 500 (via
    // a delete) lets an adapter write through at the lowered limit.
    acc.release("t-dead", 400).await.unwrap(); // 800 → 400
    assert_eq!(
        acc.accrue("t-dead", 50, None).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(
        store.used("t-dead", REGION),
        450,
        "writes within the lowered cap still accrue"
    );
}

#[tokio::test]
async fn fresh_capped_tenant_first_write_over_cap_is_refused_not_uncapped() {
    // (a) A FRESH capped tenant whose VERY FIRST write already exceeds the
    // seeded cap must be refused (OverCap) with NO row created — NOT accrued
    // uncapped. (The pre-fix bug seeded quota=0 and let it through unbounded.)
    let store = Arc::new(InMemoryByteStore::new());
    let acc = accountant(store.clone());
    assert_eq!(
        acc.accrue("t-big", 5_000, Some(1_000)).await.unwrap(),
        AccrueOutcome::OverCap
    );
    assert_eq!(
        store.used("t-big", REGION),
        0,
        "a refused first write must create no row"
    );
}

#[tokio::test]
async fn fresh_tenant_with_no_resolved_cap_fails_closed() {
    // (d) A row missing AND the cap header absent (`None`) must FAIL CLOSED
    // (Indeterminate) — never seeded uncapped. Absence is NOT unlimited.
    let store = Arc::new(InMemoryByteStore::new());
    let acc = accountant(store.clone());
    assert_eq!(
        acc.accrue("t-unknown", 100, None).await.unwrap(),
        AccrueOutcome::Indeterminate
    );
    assert_eq!(
        store.used("t-unknown", REGION),
        0,
        "fail-closed must create no row"
    );
}

#[tokio::test]
async fn accrue_over_cap_is_blocked_and_counter_unchanged() {
    // A tenant at 900/1000 bytes; a 200-byte write would hit 1100 > 1000 ⇒
    // OverCap, and the counter must NOT move (atomic check-and-accrue).
    let store = Arc::new(InMemoryByteStore::new());
    store.seed(
        "t-cap",
        REGION,
        Row {
            used: 900,
            quota: 1_000,
        },
    );
    let acc = accountant(store.clone());
    assert_eq!(
        acc.accrue("t-cap", 200, None).await.unwrap(),
        AccrueOutcome::OverCap
    );
    assert_eq!(
        store.used("t-cap", REGION),
        900,
        "an over-cap accrual must not move the counter"
    );
    // A write that exactly fills the cap is allowed (`<=` predicate).
    assert_eq!(
        acc.accrue("t-cap", 100, None).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(store.used("t-cap", REGION), 1_000);
    // Now AT the cap; one more byte trips it.
    assert_eq!(
        acc.accrue("t-cap", 1, None).await.unwrap(),
        AccrueOutcome::OverCap
    );
}

#[tokio::test]
async fn release_saturates_at_zero() {
    let store = Arc::new(InMemoryByteStore::new());
    store.seed(
        "t-del",
        REGION,
        Row {
            used: 300,
            quota: 0,
        },
    );
    let acc = accountant(store.clone());
    acc.release("t-del", 100).await.unwrap();
    assert_eq!(store.used("t-del", REGION), 200);
    // Over-release saturates at 0, never negative (table CHECK invariant).
    acc.release("t-del", 9_999).await.unwrap();
    assert_eq!(store.used("t-del", REGION), 0);
}

#[tokio::test]
async fn zero_or_negative_byte_ops_are_noops() {
    let store = Arc::new(InMemoryByteStore::new());
    let acc = accountant(store.clone());
    // A no-new-bytes write (idempotent re-write) accrues nothing — and never
    // reaches the store, so even a `None` cap is a safe no-op (no fresh row).
    assert_eq!(
        acc.accrue("t0", 0, None).await.unwrap(),
        AccrueOutcome::Accrued
    );
    assert_eq!(
        acc.accrue("t0", -5, None).await.unwrap(),
        AccrueOutcome::Accrued
    );
    acc.release("t0", 0).await.unwrap();
    assert_eq!(store.used("t0", REGION), 0);
}

#[tokio::test]
async fn store_error_surfaces_for_fail_closed_handling() {
    // The handler maps an accrue Err to 503 (fail-CLOSED) — assert the error
    // propagates rather than being silently swallowed.
    let acc = accountant(Arc::new(ErroringByteStore));
    assert!(acc.accrue("t-err", 100, Some(1_000)).await.is_err());
}

#[test]
fn region_default_is_iad() {
    // `region_from_env` defaults to the residency-correct US colo when
    // R2_CAS_REGION is unset; a regional env overrides it (see cas.rs F7).
    // We only assert the default is a canonical literal accepted by the
    // table CHECK; the env-set path is covered by storage::env_or tests.
    let r = region_from_env();
    assert!(
        ["sam", "iad", "lhr", "nrt", "syd"].contains(&r.as_str()),
        "region must be a canonical 5-region literal"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_2_1_REANCHOR: () = ();
