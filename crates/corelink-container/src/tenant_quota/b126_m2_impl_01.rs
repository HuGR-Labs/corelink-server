/// Cosmetic runbook deep-link carried on the near-ceiling [`PagerDutyEvent`]
/// (the [`PagerDutyEvent::runbook_url`] field is required). It is inert today
/// — the in-memory sink pages nobody; B-008 wires the real HTTPS dispatcher
/// that would surface this link.
const NEAR_CEILING_RUNBOOK_URL: &str = "https://corelink.io/runbooks/RB-QUOTA-DOLLAR-CEILING.md";

/// Default-off env flag gating the near-$-ceiling alert sink (B-007). Mirrors
/// the `AUDIT_DRAIN_LEASE_ENABLED` secure-inert-default precedent
/// (`routes/audit_drain.rs`): unset / forwarded-`""` / anything but an explicit
/// truthy value ⇒ the sink is `None` and behavior is byte-identical to today
/// (bare `tracing::warn!` only).
const NEAR_CEILING_ALERT_SINK_ENV: &str = "NEAR_CEILING_ALERT_SINK";

/// The default per-tenant monthly ceiling: **effectively-unlimited**
/// (`$1,000,000/mo` in micro-dollars). ADR-0068 reconciliation (2026-07-09):
/// the prior `$5` tripwire tripped ~100× BELOW the tier request-cap and is
/// removed as a default wall; the ceiling stays an owner-tunable per-tenant
/// backstop — set it per contract for the unbounded team/enterprise tiers.
pub const DEFAULT_MONTHLY_BUDGET_USD_MICROS: i64 = 1_000_000_000_000;

/// Cycle length: ~30 days, in milliseconds. When the wall clock has
/// advanced at least this far past a tenant's `cycle_anchor_ms`, the
/// cycle rolls (accrued resets to 0, the anchor advances).
pub const CYCLE_LENGTH_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// Env var naming the FLAT per-billable-op cost charged against the
/// monthly $-ceiling, in micro-dollars. See [`DEFAULT_COST_PER_OP_MICROS`].
pub const COST_PER_OP_MICROS_ENV: &str = "QUOTA_COST_PER_OP_MICROS";

/// Default FLAT per-op cost: `1000` micro-dollars = `$0.001` per billable
/// op.
///
/// # Cost model (deliberately coarse)
///
/// Every billable data-plane op (CAS/AC read+write, Bazel REAPI, Turbo,
/// sccache) is charged the SAME flat cost regardless of byte size. It is
/// a **preventive tripwire, NOT precise metering**: at the default
/// `$0.001/op` the effectively-unlimited `$1,000,000/mo` default (post
/// ADR-0068 reconciliation) is no normal-usage wall — bound blast radius,
/// not to bill exactly — precise per-byte metering is a separate,
/// post-launch concern. Operators tune the per-op cost via
/// [`COST_PER_OP_MICROS_ENV`] and the per-tenant ceiling via the
/// `tenant_quota.monthly_budget_usd_micros` column.
pub const DEFAULT_COST_PER_OP_MICROS: i64 = 1_000;

/// Resolve the flat per-op cost from [`COST_PER_OP_MICROS_ENV`], falling
/// back to [`DEFAULT_COST_PER_OP_MICROS`] when the var is unset, empty,
/// non-numeric, or negative (a negative cost would credit the tenant —
/// rejected). The value is read once per call; callers cache it in route
/// state at boot.
#[must_use]
pub fn cost_per_op_micros() -> i64 {
    std::env::var(COST_PER_OP_MICROS_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(DEFAULT_COST_PER_OP_MICROS)
}

/// Build the production [`QuotaGuard`] from process env (the same
/// [`crate::storage::StorageEnv`] the D1 adapters use) + the system wall
/// clock. Returns `None` — the billable routes then run WITHOUT the
/// $-ceiling gate — when the storage env is unset/invalid (dev/CI), so a
/// credential-less local run is unaffected. Mirrors
/// [`crate::adapter_cache::d1_map_from_env`]'s fail-CLOSED env-gate.
#[must_use]
pub fn quota_guard_from_env() -> Option<Arc<QuotaGuard>> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let client = crate::storage::d1_http::D1HttpClient::new(&storage_env)
        .map_err(|e| tracing::warn!(error = %e, "tenant-quota: D1 client init failed"))
        .ok()?;
    // WP-2a integration: front the durable D1 store with the in-process budget
    // lease so the hot path debits a chunk once and serves subsequent ops' ACCRUE
    // from memory, removing the per-request D1-over-HTTP accrue-WRITE hop AND —
    // WP-2a step 2 — the rolling-decision READ hop when warm, via
    // `try_serve_from_lease` (cold/drained ops fall through). The lease debits
    // up-front in `inner` (charge-never-lost), stays fail-CLOSED when drained +
    // inner-unreachable, and bounds overshoot to one lease chunk — see
    // `LeasedQuotaStore`. The guard's seed/cycle-roll bookkeeping is unchanged
    // (those methods pass straight through to `inner`).
    let inner: Arc<dyn QuotaStore> = Arc::new(D1QuotaStore::new(Arc::new(client)));
    let store: Arc<dyn QuotaStore> = Arc::new(LeasedQuotaStore::new(inner));
    let clock: Arc<dyn WallClock> = Arc::new(crate::wall_clock::SystemWallClock::new());
    Some(Arc::new(QuotaGuard::new(store, clock)))
}

/// One tenant's quota state, mirroring the `tenant_quota` D1 row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct QuotaState {
    /// Owner-tunable monthly ceiling, in micro-dollars.
    pub monthly_budget_usd_micros: i64,
    /// Cumulative cost accrued in the current cycle, in micro-dollars.
    pub accrued_usd_micros: i64,
    /// Cycle-start wall-clock (Unix epoch ms).
    pub cycle_anchor_ms: i64,
}

impl QuotaState {
    /// A fresh quota row anchored at `now_ms`, carrying the default $5
    /// tripwire and zero accrued spend.
    #[must_use]
    pub fn fresh(now_ms: i64) -> Self {
        Self {
            monthly_budget_usd_micros: DEFAULT_MONTHLY_BUDGET_USD_MICROS,
            accrued_usd_micros: 0,
            cycle_anchor_ms: now_ms,
        }
    }

    /// Whether `now_ms` is at least [`CYCLE_LENGTH_MS`] past the anchor
    /// (i.e. the cycle should roll). A non-positive / pre-anchor `now_ms`
    /// never rolls — the cycle only advances forward.
    #[must_use]
    pub fn cycle_elapsed(&self, now_ms: i64) -> bool {
        now_ms.saturating_sub(self.cycle_anchor_ms) >= CYCLE_LENGTH_MS
    }
}

/// Backing store for per-tenant quota rows.
///
/// The methods are split so the read (cheap, on the hot path) and the
/// two distinct write shapes can be driven independently. Both are
/// async because the production impl ([`D1QuotaStore`]) reaches D1 over
/// HTTP.
///
/// # Two write shapes — why
///
/// [`Self::put`] is an **absolute** write (the row is overwritten with
/// the supplied `state`). It is used to seed a fresh tenant and to roll
/// the cycle (zero the accrued counter, advance the anchor) — both cases
/// where the new accrued value is computed by the guard, not derived
/// from the prior row.
///
/// [`Self::accrue`] is an **atomic DB-side increment** of the accrued
/// counter (`accrued = accrued + delta`). It exists to close the
/// TOCTOU lost-update window: two concurrent ops that each read the same
/// prior accrued value and write back `prior + cost` would lose one
/// op's spend under a blind overwrite. Pushing the `+ delta` into the
/// DB's `ON CONFLICT … DO UPDATE` makes the increment atomic per row, so
/// concurrent accruals sum instead of clobbering each other.
///
/// [`Self::check_and_accrue`] (F13 fix) **serializes the ceiling check with
/// the atomic accrual** in a single DB statement, eliminating the TOCTOU
/// over-admission window where concurrent requests can each read the same
/// pre-accrual baseline, all pass the ceiling test, and all proceed past the
/// cap. It has NO safe generic default — every backend MUST provide its own
/// atomic implementation (`D1QuotaStore` via the serialized
/// `UPDATE … WHERE accrued + delta <= budget RETURNING accrued` statement,
/// `InMemoryQuotaStore` under its in-process `Mutex`); the trait-level default
/// fails CLOSED (returns `Err`) rather than silently over-admitting via a
/// non-atomic two-step.
#[async_trait::async_trait]
pub trait QuotaStore: std::fmt::Debug + Send + Sync {
    /// Read a tenant's quota row. `Ok(None)` ⇒ no row yet (treated as a
    /// fresh default-tripwire tenant by the guard). `Err` ⇒ transport /
    /// decode failure (the guard fail-CLOSES on this).
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String>;

    /// **Absolute** write of a tenant's quota row (upsert). Used to seed
    /// a fresh row and to roll the cycle (the guard supplies the new
    /// accrued / anchor). `updated_at_ms` is the wall-clock of the write.
    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String>;

    /// **Atomic increment** of the accrued counter by `delta_micros`
    /// (`accrued_usd_micros = accrued_usd_micros + delta_micros`),
    /// seeding a fresh row at the default tripwire when none exists. The
    /// add happens in the DB (`ON CONFLICT … DO UPDATE`), NOT in the app,
    /// so concurrent accruals cannot lose each other's spend (TOCTOU
    /// lost-update fix). `seed_anchor_ms` is used only on the INSERT
    /// (fresh-row) path; an existing row keeps its anchor.
    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String>;

    /// **Atomic check-and-accrue** (F13 fix): atomically increments the
    /// accrued counter by `delta_micros` ONLY IF the result would not
    /// exceed `monthly_budget_usd_micros`.
    ///
    /// Returns:
    /// - `Ok(true)`  — the increment was applied (under the ceiling);
    /// - `Ok(false)` — the ceiling would be exceeded (accrual was NOT
    ///   applied; the caller must reject with 402);
    /// - `Err(_)`    — store / transport error (the caller must reject with
    ///   503, fail-CLOSED).
    ///
    /// The check-and-increment is a SINGLE DB statement:
    ///
    /// ```sql
    /// UPDATE tenant_quota
    ///    SET accrued_usd_micros = accrued_usd_micros + ?delta
    ///  WHERE tenant_id = ?tenant
    ///    AND accrued_usd_micros + ?delta <= monthly_budget_usd_micros
    /// RETURNING accrued_usd_micros
    /// ```
    ///
    /// When no row is returned the ceiling was exceeded (or the row does
    /// not exist — treated as ceiling exceeded to stay fail-CLOSED;
    /// callers must seed the row via [`Self::put`] when `get` returns
    /// `None`).
    ///
    /// There is NO safe generic default. A `get` + `accrue` two-step has a
    /// TOCTOU over-admission window (concurrent ops each read the same
    /// pre-accrual baseline, all pass the ceiling test, and all proceed past
    /// the cap) — i.e. a backend that fell back to a default two-step would
    /// silently let spend exceed the `$`-ceiling. So the default fails CLOSED:
    /// it returns `Err`, which the caller maps to a 503 (a hard deny), never an
    /// over-admission. Every backend MUST override this with an atomic
    /// check-and-increment: `D1QuotaStore` with the serialized SQL,
    /// `InMemoryQuotaStore` under its in-process `Mutex`.
    async fn check_and_accrue(
        &self,
        _tenant_id: &str,
        _delta_micros: i64,
        _seed_anchor_ms: i64,
        _updated_at_ms: i64,
    ) -> Result<bool, String> {
        // Fail-CLOSED: refuse rather than over-admit. A non-atomic generic
        // default (get → accrue) would let a future non-D1 backend silently
        // over-spend past the `$`-ceiling under concurrency, so there is
        // deliberately no working default — every backend MUST provide its
        // own atomic accrue (D1: serialized SQL; in-memory: under the Mutex).
        Err(
            "QuotaStore::check_and_accrue has no default: a backend MUST provide \
             an atomic check-and-accrue (the trait refuses to over-admit)"
                .to_owned(),
        )
    }

    /// **Atomic seed-and-check** for a tenant's FIRST billable op (red-team
    /// #6): atomically INSERT a fresh quota row (default tripwire,
    /// `accrued = delta_micros`, anchored at `seed_anchor_ms`) IFF that first
    /// op fits under the default ceiling, and — if a row already exists by the
    /// time the statement runs (a concurrent first-op won the race) — fall
    /// through to the SAME ceiling-guarded increment as the steady path.
    ///
    /// This closes the gap where a brand-new tenant's first op bypassed the
    /// $-ceiling: the prior fresh-row path called [`Self::accrue`]
    /// UNCONDITIONALLY, so a single first op larger than the budget (e.g. a
    /// fat batch) could exceed the cap before any check ran.
    ///
    /// Returns:
    /// - `Ok(true)`  — seeded (or incremented) within the ceiling;
    /// - `Ok(false)` — the first op alone would exceed the ceiling (NOT
    ///   applied; caller rejects 402);
    /// - `Err(_)`    — store / transport error (caller rejects 503).
    ///
    /// The default implementation is a get → check → seed two-step (correct
    /// for the single-threaded in-memory test store, which serializes via its
    /// `Mutex`); `D1QuotaStore` overrides it with a single atomic
    /// `INSERT … ON CONFLICT DO UPDATE … WHERE accrued + delta <= budget`.
    async fn seed_checked_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        // A row may have appeared since the caller's `get` (concurrent
        // first-op). If so, route through the ceiling-guarded increment so
        // the two first-ops sum under one budget check rather than the
        // second blindly seeding over the first.
        if self.get(tenant_id).await?.is_some() {
            return self
                .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await;
        }
        // Brand-new row: the first op must itself fit under the default
        // tripwire (a fresh tenant always carries `DEFAULT_MONTHLY_BUDGET`).
        if delta_micros > DEFAULT_MONTHLY_BUDGET_USD_MICROS {
            return Ok(false);
        }
        self.accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await?;
        Ok(true)
    }

    /// **Atomic cycle-roll** (CAA-360 #5/#20): if the tenant's stored cycle is
    /// stale (`cycle_anchor_ms + CYCLE_LENGTH_MS <= now_ms`), reset its accrued
    /// counter to 0 and advance the anchor to `now_ms` — in a SINGLE conditional
    /// statement so two concurrent ops at the cycle boundary cannot both
    /// reset+absolute-write and lose each other's spend (the prior roll path used
    /// a read-decide-`put` absolute write, a lost-update TOCTOU). Idempotent: if
    /// the row is fresh, not yet stale, or already rolled by a concurrent op, it
    /// is a no-op. After this, the caller routes through the SAME atomic
    /// [`Self::check_and_accrue`] as the steady-state path.
    ///
    /// The default impl is a non-atomic get→put (fine for the single-threaded
    /// in-memory test store); `D1QuotaStore` overrides it with the atomic
    /// conditional `UPDATE`.
    async fn roll_if_stale(&self, tenant_id: &str, now_ms: i64) -> Result<(), String> {
        if let Some(state) = self.get(tenant_id).await? {
            if state.cycle_elapsed(now_ms) {
                self.put(
                    tenant_id,
                    QuotaState {
                        monthly_budget_usd_micros: state.monthly_budget_usd_micros,
                        accrued_usd_micros: 0,
                        cycle_anchor_ms: now_ms,
                    },
                    now_ms,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// **Warm-lease fast path (WP-2a step 2): serve the rolling/cycle READ from
    /// memory too.** The steady-state gate ([`QuotaGuard::check`]) normally
    /// (a) `get`s the row to make the rolling/cycle decision, THEN (b) atomically
    /// `check_and_accrue`s. WP-2a step 1 already served (b) from an in-memory
    /// lease; this method lets a store ALSO serve (a) from memory, so a warm op
    /// makes ZERO D1 round-trips.
    ///
    /// Contract (consulted BEFORE `get`, only for a single billable op of
    /// `cost_micros` at wall-clock `now_ms`):
    ///
    /// - `Ok(Some(true))`  — the store served the op ENTIRELY from an in-memory
    ///   lease that was already debited durably up front (charge-never-lost) and
    ///   whose cycle is still current at `now_ms`. The guard returns `Allow`
    ///   without any `get`/accrue D1 hop. This can NEVER over-serve past the
    ///   ceiling: the served budget was already debited by the durable atomic
    ///   `check_and_accrue` at lease-acquire time.
    /// - `Ok(None)`        — no usable in-memory state (drained/absent lease, a
    ///   stale cycle, or a store with no lease at all). The guard MUST fall back
    ///   to the durable `get` + roll/seed/`check_and_accrue` path, which stays
    ///   the SOLE ceiling authority and the fail-CLOSED gate.
    /// - `Err(_)`          — an internal fault (e.g. a poisoned lease lock);
    ///   the guard rejects `503` fail-CLOSED, never fail-open.
    ///
    /// The DEFAULT is `Ok(None)`: a store with no in-memory lease (the durable
    /// [`D1QuotaStore`], the [`InMemoryQuotaStore`]) never short-circuits the
    /// read, so its behaviour is byte-for-byte unchanged. Only
    /// [`LeasedQuotaStore`] overrides it.
    ///
    /// # Why this cannot weaken any of the five invariants
    ///
    /// 1. **charge-never-lost** — untouched: the lease this consumes was debited
    ///    durably up front by the inner atomic `check_and_accrue`; this method
    ///    only decrements the already-paid in-memory remainder.
    /// 2. **never over-SERVE past the ceiling** — untouched: the durable atomic
    ///    `accrued + delta <= budget` remains the sole ceiling authority; a
    ///    warm-serve consumes only pre-paid lease budget, and an over-ceiling
    ///    tenant has no coverable lease → `Ok(None)` → durable path → `402`.
    /// 3. **fail-CLOSED** — untouched: this method never reaches the inner
    ///    store, so it cannot mask an outage; a drained/absent lease returns
    ///    `Ok(None)` and the guard hits the durable path (which `503`s on fault).
    /// 4. **bounded overshoot** — untouched: no new budget is debited here, so
    ///    the crash-tail bound (≤ one lease chunk) is exactly as before.
    /// 5. **cycle correctness** — preserved: a warm serve is granted ONLY when
    ///    the lease's own cached `cycle_anchor_ms` has NOT elapsed at `now_ms`
    ///    (`cycle_elapsed(now_ms) == false`). At the cycle boundary the cached
    ///    cycle is treated as stale → `Ok(None)` → the durable roll path runs.
    ///    The staleness window is exactly the documented "≤ one lease" bound:
    ///    the lease is discarded the instant `now_ms` crosses `CYCLE_LENGTH_MS`.
    async fn try_serve_from_lease(
        &self,
        _tenant_id: &str,
        _cost_micros: i64,
        _now_ms: i64,
    ) -> Result<Option<bool>, String> {
        Ok(None)
    }
}

/// Default lease chunk size, in **ops**: how many billable ops one
/// in-memory lease pre-buys from the durable inner store in a single
/// debit. See [`LeasedQuotaStore`] for why this number is small.
///
/// At the default `$0.001/op` ([`DEFAULT_COST_PER_OP_MICROS`]) a `16`-op
/// lease pre-debits `16_000` micro-dollars (`$0.016`) — so the worst-case
/// per-tenant *overshoot* of the true monthly ceiling (one in-flight lease
/// lost to a crash that already passed its budget gate, see invariant 3)
/// is bounded at **`LEASE_OPS * cost_per_op` micro-dollars** =
/// `16 * 1_000 = 16_000` micro-USD (`$0.016`) against a `$5`/mo
/// (`5_000_000` micro-USD) tripwire — a `0.32 %` worst-case overshoot,
/// negligible for a preventive cost cap, in exchange for amortising the
/// per-request D1-over-HTTP accrue-WRITE hop 16:1 on the CAS hot path (the
/// rolling-decision READ is also lease-served when warm — see [`LeasedQuotaStore`]).
pub const DEFAULT_LEASE_OPS: i64 = 16;

/// A per-`(tenant, cycle)` in-memory lease over an inner durable
/// [`QuotaStore`], to amortise the per-request D1-over-HTTP
/// **accrue-write** on the CAS hot path — WITHOUT weakening the
/// fail-CLOSED `$`-ceiling (WP-2a; preserves the #318 paid-without-payment
/// fail-closed gate and every CAA-360 invariant of the inner store).
///
/// # Scope of the optimisation
///
/// This lease removes BOTH of the gate's per-op D1 hops on the WARM path.
/// The gate ([`QuotaGuard::check`]) does (a) a `get` to read the
/// rolling-decision / cycle-anchor row, THEN (b) an atomic `check_and_accrue`
/// write. This wrapper short-circuits the WRITE hop (b) via the lease chunk
/// (subsequent ops in a lease serve without an inner `check_and_accrue`), AND
/// — WP-2a step 2 — short-circuits the READ hop (a) via
/// [`Self::try_serve_from_lease`]: while a pre-paid, cycle-current lease is
/// live the guard skips D1 ENTIRELY. Only a cold/drained/stale lease or a
/// cycle-roll falls to the durable `get` + atomic-accrue authority;
/// `get`/`put`/`accrue`/`roll_if_stale` still pass straight through, unleased.
///
/// # The problem
///
/// The hot path calls the `$`-ceiling gate on EVERY billable op. Against
/// the production [`D1QuotaStore`] each hop is a synchronous D1-over-HTTP
/// round-trip to `api.cloudflare.com` (~0.3–0.7 s) — a measured root cause
/// of CAS latency. This wrapper amortises the accrue-WRITE hop AND the
/// rolling-decision READ when warm (WP-2a step 2 — see scope note above).
///
/// # The mechanism — budget LEASING
///
/// This wrapper holds a small in-memory **lease** keyed by
/// `(tenant, cycle_anchor)`. On the first op of a tenant/cycle it debits a
/// CHUNK of budget — `lease_ops` ops worth — from the *inner* durable
/// store atomically up front (via the inner [`QuotaStore::check_and_accrue`]
/// for `lease_ops * cost`), then serves subsequent ops **from the lease,
/// in memory, without touching the inner store** until the lease is
/// drained. When a lease drains it refills the same way. This amortises
/// the D1 accrue-WRITE hop `lease_ops : 1` AND (WP-2a step 2) the
/// rolling-decision READ when warm — see the scope note above.
///
/// # The five INVIOLABLE invariants (and how each is upheld)
///
/// 1. **charge-never-lost.** Budget is debited in the INNER (durable D1)
///    store at lease-acquire time, UP FRONT — never only in memory. A
///    container crash loses at most the UNUSED tail of an in-flight lease
///    (the tenant is then slightly *under*-charged — safe for us), and
///    NEVER over-serves silently: every op served has already been paid
///    for in D1.
///
/// 2. **fail-CLOSED preserved.** A lease is acquired ONLY when the inner
///    `check_and_accrue` (or `seed_checked_accrue`) returns `Ok(true)`. If
///    the lease is drained AND a refill cannot be acquired — the inner
///    store is over-ceiling (`Ok(false)`) or unreachable (`Err`) — the op
///    is REJECTED by propagating the inner result, exactly as today
///    (`402` over ceiling, `503` on store/clock fault). Never fail-open.
///
/// 3. **bounded overshoot.** Because a full chunk is debited up front, a
///    tenant can be CHARGED for up to `lease_ops` ops it may not actually
///    serve (the unused tail of an in-flight lease) — i.e. we slightly
///    *over*-charge, never *over*-serve. The durable side NEVER exceeds the
///    ceiling: the inner store's atomic `accrued + delta <= budget` is the
///    sole authority, and when a full-chunk refill would breach it the
///    refill falls back to a **partial lease** — it asks the inner store
///    for progressively smaller chunks (`lease_ops` worth, halving down to
///    a single op) so the last admitted ops are debited exactly, then
///    fail-CLOSES. Worst-case *overshoot* is therefore only the crash-loss
///    tail of invariant 1, bounded at `lease_ops * cost_per_op`
///    micro-dollars (see [`DEFAULT_LEASE_OPS`] for the `$0.016` worst
///    case on a `$5`/mo ceiling).
///
/// 4. **cycle-roll safe.** The lease is keyed by the inner store's
///    `cycle_anchor_ms` (the anchor the guard hands to this op). A served
///    op consumes a lease only when that lease's anchor matches the op's
///    anchor; if the cycle rolled (the inner store reset accrued and
///    advanced the anchor) the stale-cycle lease no longer matches and is
///    DISCARDED, and a fresh lease is acquired against the new cycle. An
///    old-cycle lease is never served into a new cycle.
///
/// 5. **concurrency-safe.** All lease state lives behind a `Mutex`, and the
///    lease accounting (decrement-or-refill) is performed under that lock
///    with the SAME lost-update discipline as the inner code: a served op
///    either consumes one op's worth of an existing valid lease or triggers
///    a refill, with no TOCTOU window where two concurrent ops both spend
///    the last unit. The async inner refill is NOT held across the lock —
///    the lock is taken to consume/insert, released around the await — and
///    every refill goes through the inner store's own atomic
///    `check_and_accrue`/`seed_checked_accrue`, so two concurrent refills on
///    the same tenant each debit the inner store atomically (one may
///    briefly hold two leases' worth of budget — still bounded by invariant
///    3 and still never over-served).
include!("b126_m2_impl_01_part_02.rs");
