//! Per-tenant monthly **$-ceiling** middleware (WP-FOUND-2 / G1) — the
//! economic fail-CLOSED spend cap mandated by RATIFIED **ADR-0068**
//! (`specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md`).
//!
//! # Why — `$` is not `rate`
//!
//! The container already enforces a per-tenant **rate** limit
//! (`ratelimit_buckets` + the tier→RPS map; the in-route gate lives at
//! [`crate::routes::audit_analytics`]'s `rate_limit_check`). That bounds
//! **velocity** (req/s) — NOT cumulative **dollars**. A slow-but-steady
//! tenant stays under the rate limit yet can accrue unbounded monthly
//! cost (the real blast-radius risk for the hugit campaign on cheap
//! third-party infra). ADR-0068 closes that gap with a second,
//! orthogonal axis: a per-tenant monthly **cumulative-$** ceiling,
//! checked **before** a billable op is served.
//!
//! # Fail-CLOSED
//!
//! For a COST cap the correct trade-off is the opposite of an SLO
//! limiter: over the ceiling we protect the **business** (reject), not
//! availability (serve + eat cost). Concretely the gate fail-CLOSES on
//! every uncertain path:
//!
//! - over the ceiling → `402 Payment Required`;
//! - quota-store transport / decode error → `503 Service Unavailable`
//!   (we do NOT serve a billable op we could not cost-check);
//! - wall clock unavailable (`now_ms == 0`) → `503`.
//!
//! Only an explicit `Allow` (room under the ceiling, store reachable)
//! returns `None` and lets the request proceed.
//!
//! # Units — integer micro-dollars
//!
//! All amounts are signed `i64` **micro-dollars** (USD * 1_000_000), to
//! match the `tenant_quota` D1 table (migration 0066). Floating point is
//! never used for money. The launch tripwire is [`DEFAULT_MONTHLY_BUDGET_USD_MICROS`]
//! (= `5_000_000`, i.e. $5).
//!
//! # Wiring (the seam the lead registers)
//!
//! Production builds a [`QuotaGuard`] from an `Arc<dyn QuotaStore>`
//! ([`D1QuotaStore`] over [`crate::storage::d1_http::D1HttpClient`]) +
//! an `Arc<dyn WallClock>` ([`crate::wall_clock::SystemWallClock`]),
//! stashes it in each billable route's state, and calls
//! [`QuotaGuard::check`] at the TOP of the billable handler — alongside
//! the existing rate-limit gate — returning the response on `Some`.
//! Tests inject [`InMemoryQuotaStore`] + [`crate::wall_clock::InMemoryFakeWallClock`].

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::response::Response;

use crate::wall_clock::WallClock;

/// The symbolic launch tripwire: **$5/mo**, in micro-dollars (ADR-0068).
///
/// Deliberately conservative — it bounds day-1 cost blast radius while
/// real usage calibrates the number. It is a TRIPWIRE, not a product
/// tier; the ceiling is owner-tunable per tenant in `tenant_quota`.
pub const DEFAULT_MONTHLY_BUDGET_USD_MICROS: i64 = 5_000_000;

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
/// `$0.001/op` the ADR-0068 `$5/mo` ceiling (`5_000_000` micro-USD) trips
/// at ≈ `5000` ops/month. The point is to bound day-1 cost blast radius,
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
    async fn put(&self, tenant_id: &str, state: QuotaState, updated_at_ms: i64)
        -> Result<(), String>;

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
///
/// # What this wrapper is NOT
///
/// It does NOT change the [`QuotaStore`] trait, [`QuotaGuard`], or any
/// route — it is a drop-in `Arc<dyn QuotaStore>` the lead wires at
/// construction. It only overrides the hot-path entry points
/// ([`QuotaStore::check_and_accrue`] and [`QuotaStore::seed_checked_accrue`],
/// which the guard funnels every billable op through); `get`, `put`,
/// `accrue`, and `roll_if_stale` pass straight through to the inner store
/// (the guard's roll + seed bookkeeping stays exact and durable).
///
/// # Accepted approximation (audit #6, ACCEPTED — by design)
///
/// This is an **approximate** `$`-ceiling, accepted explicitly so it is a
/// documented design choice, not a hidden surprise. With the default
/// `lease_ops = 16` ([`DEFAULT_LEASE_OPS`]):
///
/// * **D1 consulted ~once per 16 ops.** The durable inner store (the sole
///   ceiling authority) is touched only at lease acquire/refill — roughly
///   once every 16 billable ops — so in-memory ops between refills do not
///   re-check D1.
/// * **Up to ~16-op revocation/staleness latency.** A budget change made
///   durably (e.g. a ceiling drop, or charges from another container) is
///   not observed until the current in-memory lease drains and a refill
///   re-reads the inner store — i.e. up to `lease_ops` ops of staleness.
/// * **Crash ⇒ slight under-charge.** A container crash loses the UNUSED
///   tail of an in-flight lease; that budget was already debited durably,
///   so the tenant is slightly *under*-charged (safe for us), never
///   over-served (invariants 1 & 3). The overshoot is bounded at
///   `lease_ops * cost_per_op` micro-dollars (`$0.016` on a `$5`/mo
///   ceiling — see [`DEFAULT_LEASE_OPS`]).
#[derive(Debug)]
pub struct LeasedQuotaStore {
    inner: Arc<dyn QuotaStore>,
    /// Ops pre-bought per lease acquisition (defaults to [`DEFAULT_LEASE_OPS`]).
    lease_ops: i64,
    /// Flat per-op cost, in micro-dollars, used to size a lease debit.
    /// Captured once at construction (matching the route-state caching of
    /// [`cost_per_op_micros`]) so lease sizing never re-reads env per op.
    cost_per_op: i64,
    /// Per-tenant lease state, behind a `Mutex` (invariant 5).
    leases: Mutex<HashMap<String, Lease>>,
}

/// One tenant's in-memory lease: budget already debited from the inner
/// durable store, available to serve in memory, scoped to a cycle anchor.
#[derive(Debug, Clone, Copy)]
struct Lease {
    /// Remaining pre-paid budget for this lease, in micro-dollars. Each
    /// served op decrements this; when it can no longer cover an op a
    /// refill is attempted (invariant 2/3).
    remaining_micros: i64,
    /// The inner store's `cycle_anchor_ms` this lease was acquired against.
    /// A mismatch on the next op ⇒ the cycle rolled ⇒ discard (invariant 4).
    cycle_anchor_ms: i64,
}

impl LeasedQuotaStore {
    /// Wrap an inner durable [`QuotaStore`] with the default lease chunk
    /// ([`DEFAULT_LEASE_OPS`]) and the env-resolved per-op cost
    /// ([`cost_per_op_micros`]).
    #[must_use]
    pub fn new(inner: Arc<dyn QuotaStore>) -> Self {
        Self::with_config(inner, DEFAULT_LEASE_OPS, cost_per_op_micros())
    }

    /// Wrap an inner store with an explicit lease chunk + per-op cost (the
    /// seam tests use to drive amortisation deterministically). Both are
    /// clamped to `>= 1` so a misconfiguration degrades to "one op per
    /// lease" (i.e. pass-through), never to a zero-cost lease that could
    /// serve for free.
    #[must_use]
    pub fn with_config(inner: Arc<dyn QuotaStore>, lease_ops: i64, cost_per_op: i64) -> Self {
        Self {
            inner,
            lease_ops: lease_ops.max(1),
            cost_per_op: cost_per_op.max(1),
            leases: Mutex::new(HashMap::new()),
        }
    }

    /// The size, in micro-dollars, of a full lease chunk.
    fn full_chunk_micros(&self) -> i64 {
        self.lease_ops.saturating_mul(self.cost_per_op)
    }

    /// Try to satisfy `cost_micros` for `tenant` from an existing VALID
    /// in-memory lease (matching `valid_anchor_ms`, invariant 4), under the
    /// caller-held lock. Returns `true` when served from the lease (no inner
    /// call needed); `false` when no usable lease exists (drained, absent,
    /// or stale cycle) and the caller must refill against the inner store.
    fn try_consume_locked(
        leases: &mut HashMap<String, Lease>,
        tenant: &str,
        cost_micros: i64,
        valid_anchor_ms: i64,
    ) -> bool {
        if let Some(lease) = leases.get_mut(tenant) {
            // Invariant 4: a lease from a different cycle must never serve.
            if lease.cycle_anchor_ms == valid_anchor_ms && lease.remaining_micros >= cost_micros {
                lease.remaining_micros -= cost_micros;
                return true;
            }
        }
        false
    }

    /// The core charge path shared by `check_and_accrue` and
    /// `seed_checked_accrue`. `seed` selects which inner entry point a
    /// refill uses (the seed path for a brand-new tenant's first op, the
    /// steady path otherwise) — both are atomic + fail-CLOSED in the inner
    /// store, so the lease inherits those semantics.
    ///
    /// Returns the inner-store contract: `Ok(true)` served, `Ok(false)`
    /// ceiling exceeded (caller ⇒ 402), `Err` store fault (caller ⇒ 503).
    async fn charge(
        &self,
        tenant_id: &str,
        cost_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
        seed: bool,
    ) -> Result<bool, String> {
        // A non-positive cost cannot consume budget; treat as served
        // (mirrors the guard clamping negative cost to 0 upstream).
        if cost_micros <= 0 {
            return Ok(true);
        }

        // The cycle anchor this op belongs to. The guard hands us the
        // rolled/fresh anchor on the roll path and the existing anchor on
        // the steady path — exactly the anchor a freshly-acquired lease
        // should carry — so it is the authoritative lease key (invariant 4),
        // and the served-from-lease fast path needs NO extra inner read.
        let anchor = seed_anchor_ms;

        // Fast path: serve from a valid existing lease, fully in memory.
        {
            let mut leases = self
                .leases
                .lock()
                .map_err(|_| "LeasedQuotaStore: poisoned lock".to_owned())?;
            if Self::try_consume_locked(&mut leases, tenant_id, cost_micros, anchor) {
                return Ok(true);
            }
        }

        // Slow path: no usable lease — acquire/refill from the inner
        // durable store. The inner debit is the up-front charge (invariant
        // 1) and the fail-CLOSED gate (invariant 2). We try a full chunk
        // first, then progressively smaller chunks down to exactly this op,
        // so a near-ceiling tenant gets a partial lease rather than a hard
        // reject one op early (invariant 3 — partial lease). The op's own
        // cost is always included in the debited chunk, so the very op
        // triggering the refill is itself paid for up front.
        let full_chunk = self.full_chunk_micros().max(cost_micros);

        let mut chunk = full_chunk;
        loop {
            let attempt = chunk.max(cost_micros);
            let acquired = if seed {
                self.inner
                    .seed_checked_accrue(tenant_id, attempt, seed_anchor_ms, updated_at_ms)
                    .await?
            } else {
                self.inner
                    .check_and_accrue(tenant_id, attempt, seed_anchor_ms, updated_at_ms)
                    .await?
            };
            if acquired {
                // Debited `attempt` micro-dollars in the inner store up
                // front (invariant 1). Serve THIS op from it and bank the
                // remainder as the lease (invariant 3: remainder ≤ one
                // chunk). Replace any stale lease for this tenant.
                let mut leases = self
                    .leases
                    .lock()
                    .map_err(|_| "LeasedQuotaStore: poisoned lock".to_owned())?;
                let _ = leases.insert(
                    tenant_id.to_owned(),
                    Lease {
                        remaining_micros: attempt - cost_micros,
                        cycle_anchor_ms: anchor,
                    },
                );
                return Ok(true);
            }
            // The inner store rejected this chunk (over ceiling). If we were
            // already asking for the minimum (this op alone), the ceiling is
            // genuinely exceeded — fail-CLOSED 402 (invariant 2/3). Drop any
            // stale lease so we never serve it later.
            if attempt <= cost_micros {
                if let Ok(mut leases) = self.leases.lock() {
                    let _ = leases.remove(tenant_id);
                }
                return Ok(false);
            }
            // Otherwise try a smaller chunk (halve toward `cost_micros`).
            chunk = (chunk / 2).max(cost_micros);
        }
    }
}

#[async_trait::async_trait]
impl QuotaStore for LeasedQuotaStore {
    // Read / absolute-write / unconditional-accrue / roll pass straight
    // through: the guard uses these for exact bookkeeping (seed, cycle
    // roll) that must stay durable and unleased. Only the two
    // ceiling-guarded hot-path entry points are leased (below).
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        self.inner.get(tenant_id).await
    }

    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner.put(tenant_id, state, updated_at_ms).await
    }

    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.inner
            .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }

    /// Leased steady-state hot path: serve from the in-memory lease when
    /// possible; refill (up-front durable debit, fail-CLOSED, partial near
    /// the ceiling) otherwise. See [`LeasedQuotaStore`] invariants 1–5.
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        self.charge(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms, false)
            .await
    }

    /// Leased brand-new-tenant first-op path: identical leasing, but a
    /// refill uses the inner [`QuotaStore::seed_checked_accrue`] so the
    /// first lease for a fresh tenant is acquired with the seed-and-check
    /// atomic (red-team #6 discipline preserved).
    async fn seed_checked_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        self.charge(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms, true)
            .await
    }

    async fn roll_if_stale(&self, tenant_id: &str, now_ms: i64) -> Result<(), String> {
        self.inner.roll_if_stale(tenant_id, now_ms).await
    }

    /// WP-2a step 2: serve the rolling/cycle READ from the in-memory lease too,
    /// so a warm op makes ZERO D1 round-trips (no inner `get`, no inner
    /// `check_and_accrue`). See [`QuotaStore::try_serve_from_lease`] for the
    /// contract and the per-invariant safety argument.
    ///
    /// A warm serve is granted ONLY when a lease for this tenant exists whose
    /// cached cycle has NOT elapsed at `now_ms` AND whose remaining pre-paid
    /// budget covers this op. In every other case (no lease, drained lease, a
    /// stale/rolled cycle, non-positive cost) it returns `Ok(None)`, and the
    /// guard falls back to the durable path — which is the sole ceiling
    /// authority and the fail-CLOSED gate, exactly as before this optimisation.
    async fn try_serve_from_lease(
        &self,
        tenant_id: &str,
        cost_micros: i64,
        now_ms: i64,
    ) -> Result<Option<bool>, String> {
        // A non-positive cost is served by the durable path (which clamps it);
        // do not special-case it here so the two paths stay behaviourally
        // identical for the zero-cost op.
        if cost_micros <= 0 {
            return Ok(None);
        }
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| "LeasedQuotaStore: poisoned lock".to_owned())?;
        if let Some(lease) = leases.get_mut(tenant_id) {
            // Invariant 5 (cycle correctness): if the lease's OWN cached cycle
            // has elapsed at `now_ms`, treat the cached state as stale and defer
            // to the durable roll path (a rolled ceiling is never admitted past
            // this bounded — ≤ one lease — window). `cycle_elapsed` mirrors the
            // exact boundary test the guard applies to the durable row, so the
            // in-memory decision matches the durable one.
            let cached = QuotaState {
                // Only the anchor drives the cycle decision; budget/accrued are
                // not needed here (the lease already carries pre-paid remainder).
                monthly_budget_usd_micros: 0,
                accrued_usd_micros: 0,
                cycle_anchor_ms: lease.cycle_anchor_ms,
            };
            if cached.cycle_elapsed(now_ms) {
                return Ok(None);
            }
            // Invariants 1-4: consume only the already-durably-debited remainder;
            // never over-serve (an op the lease cannot cover falls through to the
            // durable ceiling authority).
            if lease.remaining_micros >= cost_micros {
                lease.remaining_micros -= cost_micros;
                return Ok(Some(true));
            }
        }
        Ok(None)
    }
}

/// The per-tenant monthly $-ceiling gate.
///
/// Holds the quota store + wall clock collaborators. Clone is cheap
/// (two `Arc`s) so it drops into per-route state alongside the existing
/// rate-limit collaborators.
#[derive(Clone, Debug)]
pub struct QuotaGuard {
    store: Arc<dyn QuotaStore>,
    clock: Arc<dyn WallClock>,
}

impl QuotaGuard {
    /// Construct a guard from its collaborators.
    #[must_use]
    pub fn new(store: Arc<dyn QuotaStore>, clock: Arc<dyn WallClock>) -> Self {
        Self { store, clock }
    }

    /// FAIL-CLOSED ceiling check for a batch of `n` ops each costing
    /// `cost_micros_each` micro-dollars against `tenant` (F12 batch variant).
    ///
    /// Charges `n * cost_micros_each` in ONE atomic check-and-accrue to avoid
    /// N sequential D1 round-trips. The product is computed with
    /// `saturating_mul` to prevent integer overflow from untrusted batch sizes.
    /// Delegates to [`Self::check`] with the scaled cost.
    ///
    /// `n = 0` charges nothing and returns `None` (allow).
    pub async fn check_batch(&self, tenant: &str, n: usize, cost_micros_each: i64) -> Option<Response> {
        if n == 0 {
            return None;
        }
        let n_i64 = i64::try_from(n).unwrap_or(i64::MAX);
        let total_cost = cost_micros_each.saturating_mul(n_i64);
        self.check(tenant, total_cost).await
    }

    /// FAIL-CLOSED ceiling check for a billable op costing `cost_micros`
    /// micro-dollars against `tenant`.
    ///
    /// Returns `None` when the request MAY proceed (room under the
    /// ceiling, store reachable). Returns `Some(response)` when it must
    /// be REJECTED:
    ///
    /// - `402 Payment Required` — `accrued + cost` would exceed the
    ///   monthly ceiling (the cap tripped);
    /// - `503 Service Unavailable` — wall clock unavailable, or the
    ///   quota store errored (we will not serve a billable op we could
    ///   not cost-check).
    ///
    /// On the `Allow` path this also (a) rolls the cycle if elapsed and
    /// (b) accrues `cost_micros` so the next call sees the updated total.
    /// A persistence error on that accrual is itself fail-CLOSED (`503`)
    /// — accruing-then-serving must be atomic from the cap's view, so a
    /// write we could not confirm rejects the op.
    pub async fn check(&self, tenant: &str, cost_micros: i64) -> Option<Response> {
        let now_ms_u64 = self.clock.now_ms();
        if now_ms_u64 == 0 {
            // Fail-CLOSED: without a trustworthy clock we cannot reason
            // about the cycle boundary (mirrors the rate-limit gate's
            // `clock_unavailable` 503 discipline).
            return Some(crate::quota_error::quota_unavailable_response(
                "wall clock unavailable",
            ));
        }
        // `now_ms` fits i64 for any realistic wall clock (year ~292M).
        let now_ms = i64::try_from(now_ms_u64).unwrap_or(i64::MAX);

        // A negative cost is a programming error upstream; treat it as 0
        // rather than crediting the tenant.
        let cost = cost_micros.max(0);

        // WP-2a step 2 — WARM-LEASE FAST PATH (ZERO D1 hops): before the `get`
        // READ, ask the store whether it can serve this op entirely from an
        // in-memory lease that was already debited durably up front and whose
        // cycle is still current. When it can (`Ok(Some(true))`), the whole gate
        // resolves in memory — no `get`, no `check_and_accrue` D1 round-trip —
        // and this is the common CAS hot-path outcome. A store with no lease
        // (the durable `D1QuotaStore`, the in-memory test store) returns
        // `Ok(None)` and we fall through to the exact durable path below, so no
        // invariant changes: the warm serve consumes only pre-paid lease budget
        // (never over-serves past the ceiling), and any drained/absent/stale
        // lease or store fault defers to (or fail-CLOSES on) the durable
        // authority. See [`QuotaStore::try_serve_from_lease`].
        match self.store.try_serve_from_lease(tenant, cost, now_ms).await {
            Ok(Some(true)) => return None,
            // `Ok(Some(false))` is not part of the contract (a store either
            // serves warm or defers); treat it defensively as "defer".
            Ok(Some(false)) | Ok(None) => {}
            Err(_) => {
                return Some(crate::quota_error::quota_unavailable_response(
                    "quota store unavailable",
                ));
            }
        }

        // Load the current quota row (or treat a missing row as a fresh
        // default-tripwire tenant). Store error ⇒ 503.
        let existing = match self.store.get(tenant).await {
            Ok(s) => s,
            Err(_) => {
                return Some(crate::quota_error::quota_unavailable_response(
                    "quota store unavailable",
                ));
            }
        };
        let state = existing.unwrap_or_else(|| QuotaState::fresh(now_ms));

        // Decide whether this op opens a fresh cycle. A roll (or a
        // brand-new row) zeroes the accrued baseline; otherwise the
        // baseline is the prior accrued value.
        let rolling = existing.is_none() || state.cycle_elapsed(now_ms);

        if rolling {
            // ── Cycle roll / fresh row path (CAA-360 #5/#20) ──────────────────
            // The prior implementation did a read-decide-absolute-`put` here,
            // which is a lost-update TOCTOU: two concurrent ops at the cycle
            // boundary each computed `accrued = cost` and overwrote each other,
            // so only ONE op's spend was counted (over-admission). Now atomic.
            if existing.is_some() {
                // Stale existing row: atomically roll it (idempotent conditional
                // reset — a no-op if a concurrent op already rolled) …
                if self.store.roll_if_stale(tenant, now_ms).await.is_err() {
                    return Some(crate::quota_error::quota_unavailable_response(
                        "quota accrual failed",
                    ));
                }
                // … then the SAME atomic check-and-accrue as the steady path, so
                // concurrent post-roll ops each accrue under one serialized
                // budget check (no over-admission).
                match self.store.check_and_accrue(tenant, cost, now_ms, now_ms).await {
                    Ok(true) => {}
                    Ok(false) => {
                        return Some(crate::quota_error::quota_exceeded_response());
                    }
                    Err(_) => {
                        return Some(crate::quota_error::quota_unavailable_response(
                            "quota accrual failed",
                        ));
                    }
                }
            } else {
                // Brand-new tenant (no row): seed atomically AND ceiling-check
                // the FIRST op (red-team #6). The prior path called `accrue`
                // UNCONDITIONALLY, so a first op larger than the default
                // tripwire (e.g. a fat `check_batch`) bypassed the $-ceiling.
                // `seed_checked_accrue` seeds `accrued = cost` only when
                // `cost <= budget`, and races a concurrent first-op through the
                // same atomic ceiling-guarded increment (no lost-update, no
                // over-admission).
                match self.store.seed_checked_accrue(tenant, cost, now_ms, now_ms).await {
                    Ok(true) => {}
                    Ok(false) => {
                        return Some(crate::quota_error::quota_exceeded_response());
                    }
                    Err(_) => {
                        return Some(crate::quota_error::quota_unavailable_response(
                            "quota accrual failed",
                        ));
                    }
                }
            }
        } else {
            // ── Steady-state path (F13 fix): atomic check-and-accrue ─────────
            //
            // Previously the ceiling check (`baseline + cost > budget`) was a
            // separate read-for-decision NOT serialized with the accrual write,
            // creating a TOCTOU over-admission window: concurrent requests could
            // each read the same pre-accrual baseline, all pass the ceiling
            // test, and all proceed slightly past the cap.
            //
            // `check_and_accrue` serializes the check with the increment in a
            // single DB statement (`UPDATE … WHERE accrued + delta <= budget
            // RETURNING accrued`). The production D1 override issues that atomic
            // SQL; the in-memory store overrides it under its `Mutex`. There is
            // no non-atomic trait default — it fails CLOSED (`Err`) so a backend
            // that forgets to override can never silently over-admit.
            //
            // Returns `Ok(false)` when the ceiling would be exceeded (reject
            // 402) and `Err(_)` on store error (reject 503).
            match self
                .store
                .check_and_accrue(tenant, cost, state.cycle_anchor_ms, now_ms)
                .await
            {
                Ok(true) => {} // accrued + allowed; proceed
                Ok(false) => {
                    return Some(crate::quota_error::quota_exceeded_response());
                }
                Err(_) => {
                    return Some(crate::quota_error::quota_unavailable_response(
                        "quota accrual failed",
                    ));
                }
            }
        }
        None
    }
}

/// In-memory [`QuotaStore`] for tests + dev/CI (NON-durable).
///
/// Mirrors the `InMemory*` collaborator fakes used across the container
/// (e.g. `InMemoryBillingD1`): hermetic, no D1 round-trip, lost on
/// restart. Production wires [`D1QuotaStore`] instead.
#[derive(Debug, Default)]
pub struct InMemoryQuotaStore {
    rows: Mutex<HashMap<String, QuotaState>>,
}

impl InMemoryQuotaStore {
    /// Construct an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: Mutex::new(HashMap::new()),
        }
    }

    /// Seed a tenant's row (test helper).
    pub fn seed(&self, tenant_id: &str, state: QuotaState) {
        if let Ok(mut rows) = self.rows.lock() {
            let _ = rows.insert(tenant_id.to_owned(), state);
        }
    }
}

#[async_trait::async_trait]
impl QuotaStore for InMemoryQuotaStore {
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        let rows = self
            .rows
            .lock()
            .map_err(|_| "InMemoryQuotaStore: poisoned lock".to_owned())?;
        Ok(rows.get(tenant_id).copied())
    }

    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        _updated_at_ms: i64,
    ) -> Result<(), String> {
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "InMemoryQuotaStore: poisoned lock".to_owned())?;
        let _ = rows.insert(tenant_id.to_owned(), state);
        Ok(())
    }

    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        _updated_at_ms: i64,
    ) -> Result<(), String> {
        // The `Mutex` makes this read-modify-write atomic in-process, so it
        // faithfully models the D1 `accrued = accrued + delta` increment
        // (the production store does the add in SQL).
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "InMemoryQuotaStore: poisoned lock".to_owned())?;
        let row = rows
            .entry(tenant_id.to_owned())
            .or_insert_with(|| QuotaState::fresh(seed_anchor_ms));
        row.accrued_usd_micros = row.accrued_usd_micros.saturating_add(delta_micros);
        Ok(())
    }

    /// Atomic check-and-accrue under the single in-process `Mutex` — the
    /// in-memory analogue of D1's serialized
    /// `UPDATE … WHERE accrued + delta <= budget`. Required because the trait
    /// default now fails CLOSED (no non-atomic generic fallback); holding the
    /// lock across the ceiling test and the increment makes the check + accrue
    /// indivisible in-process, so the result matches the D1 store exactly
    /// (no row is created on the rejected path, mirroring the prior two-step).
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        _updated_at_ms: i64,
    ) -> Result<bool, String> {
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "InMemoryQuotaStore: poisoned lock".to_owned())?;
        let (budget, accrued) = match rows.get(tenant_id) {
            Some(s) => (s.monthly_budget_usd_micros, s.accrued_usd_micros),
            None => (DEFAULT_MONTHLY_BUDGET_USD_MICROS, 0),
        };
        if accrued.saturating_add(delta_micros) > budget {
            return Ok(false);
        }
        let row = rows
            .entry(tenant_id.to_owned())
            .or_insert_with(|| QuotaState::fresh(seed_anchor_ms));
        row.accrued_usd_micros = row.accrued_usd_micros.saturating_add(delta_micros);
        Ok(true)
    }
}

/// Production [`QuotaStore`] over the `tenant_quota` D1 table (migration
/// 0066), reached via the async [`crate::storage::d1_http::D1HttpClient`].
///
/// All SQL is parameterised (positional binds); the tenant scope rides
/// in `WHERE tenant_id = ?1` on every statement (INV-TENANT-ISOLATION).
#[derive(Debug)]
pub struct D1QuotaStore {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl D1QuotaStore {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl QuotaStore for D1QuotaStore {
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        self.client.tenant_quota_lookup(tenant_id).await
    }

    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.client
            .tenant_quota_upsert(tenant_id, state, updated_at_ms)
            .await
    }

    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.client
            .tenant_quota_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }

    /// Production override for F13 — atomic check-and-accrue in ONE D1
    /// statement, eliminating the TOCTOU over-admission window.
    ///
    /// SQL:
    /// ```sql
    /// UPDATE tenant_quota
    ///    SET accrued_usd_micros = accrued_usd_micros + ?2,
    ///        updated_at_ms      = ?3
    ///  WHERE tenant_id = ?1
    ///    AND accrued_usd_micros + ?2 <= monthly_budget_usd_micros
    /// RETURNING accrued_usd_micros
    /// ```
    ///
    /// - **Row returned** → the increment was applied within budget → `Ok(true)`.
    /// - **No row returned** → either the ceiling would be exceeded OR the
    ///   row doesn't exist yet. A missing row is handled by the caller
    ///   (see [`QuotaGuard::check`] for the seed path), so this method
    ///   returns `Ok(false)` in both cases (fail-CLOSED; the caller then
    ///   seeds the row and retries on the `put` path if appropriate).
    /// - **D1 error** → `Err(String)` → caller rejects 503 (fail-CLOSED).
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        _seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        let rows = self
            .client
            .query(
                "UPDATE tenant_quota \
                    SET accrued_usd_micros = accrued_usd_micros + ?2, \
                        updated_at_ms      = ?3 \
                  WHERE tenant_id = ?1 \
                    AND accrued_usd_micros + ?2 <= monthly_budget_usd_micros \
                 RETURNING accrued_usd_micros",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(delta_micros),
                    serde_json::Value::from(updated_at_ms),
                ],
            )
            .await?;
        // A non-empty result set means the WHERE predicate matched and the
        // increment was applied. An empty set means either the ceiling would
        // be exceeded or no row exists — both are treated as "denied" here;
        // the cycle-roll / fresh-row path uses `put` via `QuotaGuard::check`.
        Ok(!rows.is_empty())
    }

    /// Production override for red-team #6 — atomic seed-and-ceiling-check of a
    /// brand-new tenant's first op in a SINGLE statement.
    ///
    /// SQL:
    /// ```sql
    /// INSERT INTO tenant_quota
    ///     (tenant_id, monthly_budget_usd_micros, accrued_usd_micros,
    ///      cycle_anchor_ms, updated_at_ms)
    /// VALUES (?1, ?4, ?2, ?3, ?5)
    /// ON CONFLICT(tenant_id) DO UPDATE
    ///     SET accrued_usd_micros = accrued_usd_micros + ?2,
    ///         updated_at_ms      = ?5
    ///   WHERE accrued_usd_micros + ?2 <= monthly_budget_usd_micros
    /// RETURNING accrued_usd_micros
    /// ```
    ///
    /// - **INSERT path** (no row yet): the row is created with
    ///   `accrued = delta`; the new row is only returned when
    ///   `delta <= budget` is enforced app-side first (a fresh row carries the
    ///   default tripwire, so we reject a first op above it BEFORE the INSERT
    ///   rather than persist an over-cap row).
    /// - **ON CONFLICT path** (a concurrent first-op already seeded the row):
    ///   the increment applies only under the ceiling — `RETURNING` yields a
    ///   row ⇒ `Ok(true)`, no row ⇒ ceiling exceeded ⇒ `Ok(false)`.
    /// - **D1 error** → `Err(String)` → caller rejects 503 (fail-CLOSED).
    async fn seed_checked_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        // The first op of a brand-new tenant must itself fit under the default
        // tripwire that the freshly-INSERTed row will carry. Reject above-cap
        // first ops BEFORE persisting a row, so we never seed an over-ceiling
        // row (the ON CONFLICT WHERE only guards the increment path).
        if delta_micros > DEFAULT_MONTHLY_BUDGET_USD_MICROS {
            return Ok(false);
        }
        let rows = self
            .client
            .query(
                "INSERT INTO tenant_quota \
                     (tenant_id, monthly_budget_usd_micros, accrued_usd_micros, \
                      cycle_anchor_ms, updated_at_ms) \
                 VALUES (?1, ?4, ?2, ?3, ?5) \
                 ON CONFLICT(tenant_id) DO UPDATE \
                     SET accrued_usd_micros = accrued_usd_micros + ?2, \
                         updated_at_ms      = ?5 \
                   WHERE accrued_usd_micros + ?2 <= monthly_budget_usd_micros \
                 RETURNING accrued_usd_micros",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(delta_micros),
                    serde_json::Value::from(seed_anchor_ms),
                    serde_json::Value::from(DEFAULT_MONTHLY_BUDGET_USD_MICROS),
                    serde_json::Value::from(updated_at_ms),
                ],
            )
            .await?;
        // A returned row means either the INSERT created the seed row or the
        // ON CONFLICT increment stayed under the ceiling. An empty set means
        // the existing row's increment would breach the ceiling (the INSERT
        // path always returns its new row).
        Ok(!rows.is_empty())
    }

    /// Production override (CAA-360 #5/#20): atomic conditional cycle reset in a
    /// SINGLE statement. Rolls ONLY if the row is still stale; a concurrent op
    /// that already advanced the anchor makes this a no-op (idempotent), so two
    /// boundary ops cannot both reset+absolute-write and lose spend.
    async fn roll_if_stale(&self, tenant_id: &str, now_ms: i64) -> Result<(), String> {
        self.client
            .query(
                "UPDATE tenant_quota \
                    SET accrued_usd_micros = 0, cycle_anchor_ms = ?2, updated_at_ms = ?2 \
                  WHERE tenant_id = ?1 AND cycle_anchor_ms + ?3 <= ?2",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(now_ms),
                    serde_json::Value::from(CYCLE_LENGTH_MS),
                ],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::wall_clock::InMemoryFakeWallClock;
    use axum::http::StatusCode;

    const T0: u64 = 1_700_000_000_000;

    fn guard_with(
        store: InMemoryQuotaStore,
        now_ms: u64,
    ) -> (QuotaGuard, Arc<InMemoryFakeWallClock>) {
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(now_ms));
        let guard = QuotaGuard::new(Arc::new(store), clock.clone());
        (guard, clock)
    }

    #[tokio::test]
    async fn under_ceiling_allows_and_accrues() {
        let store = InMemoryQuotaStore::new(); // fresh tenant → $5 tripwire
        let (guard, _clock) = guard_with(store, T0);
        // $1 op against a fresh tenant — allowed.
        assert!(guard.check("tenant-a", 1_000_000).await.is_none());
        // A second $1 op — still under $5, allowed (accrual carried).
        assert!(guard.check("tenant-a", 1_000_000).await.is_none());
    }

    #[tokio::test]
    async fn brand_new_tenant_first_op_over_ceiling_rejects_402() {
        // Red-team #6: a brand-new tenant (NO row) whose VERY FIRST billable op
        // exceeds the default $5 tripwire must be rejected 402 — the prior
        // fresh-row path called `accrue` unconditionally and would have
        // admitted (and persisted) this over-cap op. A $6 op (> $5 default)
        // must trip the ceiling on the first op.
        let store = InMemoryQuotaStore::new(); // empty → brand-new tenant
        let (guard, _clock) = guard_with(store, T0);
        let resp = guard
            .check("tenant-new-fat", 6_000_000)
            .await
            .expect("first op over ceiling must be rejected");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn brand_new_tenant_first_op_under_ceiling_seeds_and_allows() {
        // Red-team #6 control: a fresh tenant's first op UNDER the ceiling is
        // still admitted and seeds the row (accrual carried to the next op).
        let store = InMemoryQuotaStore::new();
        let (guard, _clock) = guard_with(store, T0);
        // First op: $4 (< $5) — allowed, seeds accrued = $4.
        assert!(guard.check("tenant-new-ok", 4_000_000).await.is_none());
        // Second op: $2 would project to $6 > $5 — now rejected (proves the
        // first op's spend was actually persisted, not bypassed).
        let resp = guard
            .check("tenant-new-ok", 2_000_000)
            .await
            .expect("second op trips the cap");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn over_ceiling_rejects_402_fail_closed() {
        let store = InMemoryQuotaStore::new();
        store.seed(
            "tenant-b",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 4_500_000, // $4.50 spent
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let (guard, _clock) = guard_with(store, T0);
        // A $1 op would project to $5.50 > $5 ⇒ rejected 402.
        let resp = guard.check("tenant-b", 1_000_000).await.expect("rejected");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn over_ceiling_reject_body_is_structured_json() {
        // The 402 reject flows the CENTRALIZED structured JSON body
        // (crate::quota_error) — not the old plain-text marker.
        use axum::body::to_bytes;
        let store = InMemoryQuotaStore::new();
        store.seed(
            "tenant-json",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 4_500_000,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let (guard, _clock) = guard_with(store, T0);
        let resp = guard.check("tenant-json", 1_000_000).await.expect("rejected");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
        );
        let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json.get("error").and_then(|v| v.as_str()), Some("quota_exceeded"));
        assert_eq!(json.get("retriable").and_then(|v| v.as_bool()), Some(false));
        assert!(json.get("docs_url").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn cycle_roll_resets_accrued_then_accrues_correctly() {
        // CAA-360 #5/#20: a tenant AT the cap whose cycle has fully elapsed must
        // roll (accrued reset) and admit the new op — exercising the new
        // roll_if_stale + check_and_accrue path that replaced the lost-update
        // read-decide-absolute-`put`.
        let store = InMemoryQuotaStore::new();
        store.seed(
            "tenant-roll",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 5_000_000, // pinned at the cap last cycle
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        // One full cycle + 1ms past the anchor ⇒ the cycle is stale.
        let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
        let (guard, _clock) = guard_with(store, later);
        // Despite being at the cap last cycle, the op is admitted (rolled).
        assert!(
            guard.check("tenant-roll", 1_000_000).await.is_none(),
            "post-roll op must be admitted — the cycle reset accrued to 0"
        );
        // A second op in the SAME new cycle accrues on top ($1 + $1 < $5) —
        // proving the roll set accrued to this op's cost, not stale + cost.
        assert!(
            guard.check("tenant-roll", 1_000_000).await.is_none(),
            "second post-roll op accrues within the fresh cycle"
        );
    }

    #[tokio::test]
    async fn exact_ceiling_is_allowed() {
        let store = InMemoryQuotaStore::new();
        store.seed(
            "tenant-c",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 4_000_000,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let (guard, _clock) = guard_with(store, T0);
        // Exactly hits $5 (4 + 1) — `>` test means equal is allowed.
        assert!(guard.check("tenant-c", 1_000_000).await.is_none());
        // Now at the cap; one more micro-dollar trips it.
        let resp = guard.check("tenant-c", 1).await.expect("rejected");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn cycle_rolls_after_30_days() {
        let store = InMemoryQuotaStore::new();
        store.seed(
            "tenant-d",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000,
                accrued_usd_micros: 5_000_000, // fully spent
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        // Advance past the 30-day window — the cycle should roll, freeing
        // the full budget again.
        let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
        let (guard, _clock) = guard_with(store, later);
        assert!(guard.check("tenant-d", 1_000_000).await.is_none());
    }

    #[tokio::test]
    async fn clock_unavailable_fail_closed_503() {
        let store = InMemoryQuotaStore::new();
        let (guard, _clock) = guard_with(store, 0); // now_ms == 0
        let resp = guard.check("tenant-e", 1).await.expect("rejected");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn fail_closed_reject_body_is_structured_json_with_reason() {
        // The 503 fail-CLOSED reject flows the CENTRALIZED structured JSON body
        // (crate::quota_error), preserving the marker as the `reason` field.
        use axum::body::to_bytes;
        let store = InMemoryQuotaStore::new();
        let (guard, _clock) = guard_with(store, 0); // now_ms == 0 ⇒ clock unavailable
        let resp = guard.check("tenant-503", 1).await.expect("rejected");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
        );
        let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json.get("error").and_then(|v| v.as_str()), Some("quota_unavailable"));
        assert_eq!(
            json.get("reason").and_then(|v| v.as_str()),
            Some("wall clock unavailable"),
        );
        assert_eq!(json.get("retriable").and_then(|v| v.as_bool()), Some(true));
    }

    #[derive(Debug)]
    struct ErroringStore;

    #[async_trait::async_trait]
    impl QuotaStore for ErroringStore {
        async fn get(&self, _tenant_id: &str) -> Result<Option<QuotaState>, String> {
            Err("simulated D1 transport error".to_owned())
        }
        async fn put(
            &self,
            _tenant_id: &str,
            _state: QuotaState,
            _updated_at_ms: i64,
        ) -> Result<(), String> {
            Err("simulated D1 transport error".to_owned())
        }
        async fn accrue(
            &self,
            _tenant_id: &str,
            _delta_micros: i64,
            _seed_anchor_ms: i64,
            _updated_at_ms: i64,
        ) -> Result<(), String> {
            Err("simulated D1 transport error".to_owned())
        }
    }

    #[tokio::test]
    async fn store_error_fail_closed_503() {
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(Arc::new(ErroringStore), clock);
        let resp = guard.check("tenant-f", 1).await.expect("rejected");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ───────────────────────── WP-2a: LeasedQuotaStore ─────────────────────────

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// An inner [`QuotaStore`] that delegates to a real [`InMemoryQuotaStore`]
    /// but COUNTS every ceiling-guarded inner call (`check_and_accrue` +
    /// `seed_checked_accrue`) — the amortisation oracle (invariant-test (a)).
    /// `get`/`put`/`accrue`/`roll_if_stale` are NOT counted (the lease passes
    /// them straight through, so counting them would not measure amortisation).
    #[derive(Debug)]
    struct CountingStore {
        inner: InMemoryQuotaStore,
        ceiling_calls: AtomicUsize,
    }

    impl CountingStore {
        fn new() -> Self {
            Self {
                inner: InMemoryQuotaStore::new(),
                ceiling_calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.ceiling_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl QuotaStore for CountingStore {
        async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
            self.inner.get(tenant_id).await
        }
        async fn put(
            &self,
            tenant_id: &str,
            state: QuotaState,
            updated_at_ms: i64,
        ) -> Result<(), String> {
            self.inner.put(tenant_id, state, updated_at_ms).await
        }
        async fn accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<(), String> {
            self.inner
                .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
        async fn check_and_accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<bool, String> {
            let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
            self.inner
                .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
        async fn seed_checked_accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<bool, String> {
            let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
            self.inner
                .seed_checked_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
    }

    /// (a) amortisation: N ops within lease chunks of `LEASE_OPS` cause only
    /// `ceil(N / LEASE_OPS)` inner ceiling-guarded calls.
    #[tokio::test]
    async fn lease_amortises_inner_calls() {
        const LEASE_OPS: i64 = 4;
        const COST: i64 = 1_000; // $0.001/op
        let counting = Arc::new(CountingStore::new());
        // Generous budget so no op is ever rejected: 100 ops worth.
        counting.inner.seed(
            "t-amort",
            QuotaState {
                monthly_budget_usd_micros: 100 * COST,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(counting.clone(), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        // 10 ops; lease chunk = 4 → ceil(10/4) = 3 inner calls.
        for _ in 0..10 {
            assert!(guard.check("t-amort", COST).await.is_none());
        }
        assert_eq!(
            counting.calls(),
            3,
            "10 ops with a 4-op lease must hit the inner store only ceil(10/4)=3 times"
        );
    }

    /// (b) over-ceiling still 402 once lease + inner are exhausted.
    #[tokio::test]
    async fn lease_over_ceiling_rejects_402() {
        const LEASE_OPS: i64 = 8;
        const COST: i64 = 1_000_000; // $1/op
        let inner: Arc<dyn QuotaStore> = Arc::new(InMemoryQuotaStore::new());
        // Budget = exactly $3 (3 ops) on a fresh-seeded tenant.
        inner
            .put(
                "t-cap",
                QuotaState {
                    monthly_budget_usd_micros: 3_000_000,
                    accrued_usd_micros: 0,
                    cycle_anchor_ms: i64::try_from(T0).unwrap(),
                },
                i64::try_from(T0).unwrap(),
            )
            .await
            .unwrap();
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(inner, LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        // First op tries an 8-op chunk ($8) — over the $3 ceiling — so the
        // partial-lease fallback halves down to a 3-op chunk that fits.
        // 3 ops are then served (1 from the refill + 2 from the lease tail).
        assert!(guard.check("t-cap", COST).await.is_none());
        assert!(guard.check("t-cap", COST).await.is_none());
        assert!(guard.check("t-cap", COST).await.is_none());
        // 4th op: lease drained, inner at the $3 ceiling → 402.
        let resp = guard.check("t-cap", COST).await.expect("4th op rejected");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    /// (c) inner-store error once the lease is drained → 503 (fail-CLOSED).
    /// A store that serves the FIRST lease then errors on every subsequent
    /// inner call models a D1 outage mid-cycle.
    #[derive(Debug)]
    struct ErrorAfterFirstLease {
        inner: InMemoryQuotaStore,
        seen_first_refill: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl QuotaStore for ErrorAfterFirstLease {
        async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
            self.inner.get(tenant_id).await
        }
        async fn put(
            &self,
            tenant_id: &str,
            state: QuotaState,
            updated_at_ms: i64,
        ) -> Result<(), String> {
            self.inner.put(tenant_id, state, updated_at_ms).await
        }
        async fn accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<(), String> {
            self.inner
                .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
        async fn check_and_accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<bool, String> {
            if self.seen_first_refill.fetch_add(1, Ordering::SeqCst) >= 1 {
                return Err("simulated D1 outage on refill".to_owned());
            }
            self.inner
                .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
    }

    #[tokio::test]
    async fn lease_drained_then_inner_error_fail_closed_503() {
        const LEASE_OPS: i64 = 2;
        const COST: i64 = 1_000;
        let store = ErrorAfterFirstLease {
            inner: InMemoryQuotaStore::new(),
            seen_first_refill: AtomicUsize::new(0),
        };
        store.inner.seed(
            "t-503",
            QuotaState {
                monthly_budget_usd_micros: 100 * COST,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(Arc::new(store), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        // First 2 ops: one inner refill (the 2-op lease), both served.
        assert!(guard.check("t-503", COST).await.is_none());
        assert!(guard.check("t-503", COST).await.is_none());
        // 3rd op: lease drained → refill → inner errors → 503 (fail-CLOSED).
        let resp = guard.check("t-503", COST).await.expect("refill error → 503");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// (d) charge-never-lost: the total debited to the inner durable store is
    /// always ≥ the cost of the ops actually served (we never serve more than
    /// was paid for up front). Here 10 ops are served; the inner accrued must
    /// be ≥ 10 ops' cost (it is exactly the rounded-up lease chunks).
    #[tokio::test]
    async fn lease_charge_never_lost() {
        const LEASE_OPS: i64 = 4;
        const COST: i64 = 1_000;
        let inner = Arc::new(InMemoryQuotaStore::new());
        inner.seed(
            "t-debit",
            QuotaState {
                monthly_budget_usd_micros: 1_000 * COST,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(inner.clone(), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        let ops = 10_i64;
        for _ in 0..ops {
            assert!(guard.check("t-debit", COST).await.is_none());
        }
        let debited = inner
            .get("t-debit")
            .await
            .unwrap()
            .unwrap()
            .accrued_usd_micros;
        let served = ops * COST;
        assert!(
            debited >= served,
            "inner debited ({debited}) must be ≥ served ({served}) — charge-never-lost"
        );
        // And bounded by invariant 3: at most one extra lease chunk debited.
        assert!(
            debited <= served + LEASE_OPS * COST,
            "over-charge bounded by one lease chunk"
        );
    }

    /// (e) cycle roll invalidates the stale lease: an op after the cycle rolls
    /// must NOT be served from the old-cycle lease (it would be free, escaping
    /// the new cycle's ceiling). Proven by accounting — after the roll the op
    /// is debited against the FRESH (reset) inner row.
    #[tokio::test]
    async fn lease_cycle_roll_invalidates_stale_lease() {
        const LEASE_OPS: i64 = 8;
        const COST: i64 = 1_000_000; // $1/op
        let inner = Arc::new(InMemoryQuotaStore::new());
        inner.seed(
            "t-roll",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000, // $5
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(inner.clone(), LEASE_OPS, COST));
        // Cycle 1 at T0: one op pre-buys a lease. The full 8-op chunk ($8)
        // exceeds the $5 ceiling, so the partial-lease fallback halves to a
        // 4-op chunk ($4, fits under $5) and debits exactly that up front.
        let clock1 = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard1 = QuotaGuard::new(leased.clone(), clock1);
        assert!(guard1.check("t-roll", COST).await.is_none());
        // Inner shows the cycle-1 partial-lease chunk debited ($4).
        let cycle1_debit = inner
            .get("t-roll")
            .await
            .unwrap()
            .unwrap()
            .accrued_usd_micros;
        assert_eq!(cycle1_debit, 4_000_000, "partial lease debited a 4-op chunk");

        // Advance one full cycle: the inner cycle rolls (accrued→0, anchor
        // advances). The old-cycle lease (anchored at T0) must be discarded.
        let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
        let clock2 = Arc::new(InMemoryFakeWallClock::at_unix_ms(later));
        let guard2 = QuotaGuard::new(leased, clock2);
        // This op rolls the inner row and acquires a NEW lease against the new
        // cycle — it is debited against the freshly-reset inner row, NOT
        // served free from the stale lease.
        assert!(guard2.check("t-roll", COST).await.is_none());
        let post = inner.get("t-roll").await.unwrap().unwrap();
        assert!(
            post.cycle_anchor_ms >= i64::try_from(later).unwrap()
                || post.cycle_anchor_ms != i64::try_from(T0).unwrap(),
            "the inner cycle must have rolled (anchor advanced past T0)"
        );
        // A fresh chunk was debited in the new cycle (would be 0 if the stale
        // lease had served the op for free → that is the bug this guards).
        assert!(
            post.accrued_usd_micros > 0,
            "post-roll op must debit the fresh inner cycle, not be served free \
             from the stale lease"
        );
    }

    /// (f) concurrency: many concurrent ops on the same tenant never
    /// double-spend the lease nor over-serve — the inner durable debit always
    /// covers everything served, and the inner ceiling is never breached.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn lease_concurrent_ops_no_double_spend() {
        const LEASE_OPS: i64 = 4;
        const COST: i64 = 1_000;
        const N: usize = 200;
        let inner = Arc::new(InMemoryQuotaStore::new());
        inner.seed(
            "t-conc",
            QuotaState {
                monthly_budget_usd_micros: 10_000 * COST, // never trips
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(inner.clone(), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = Arc::new(QuotaGuard::new(leased, clock));

        let mut handles = Vec::with_capacity(N);
        for _ in 0..N {
            let g = guard.clone();
            handles.push(tokio::spawn(async move {
                g.check("t-conc", COST).await.is_none()
            }));
        }
        let mut served = 0_i64;
        for h in handles {
            if h.await.unwrap() {
                served += 1;
            }
        }
        assert_eq!(served, N as i64, "all ops under the high ceiling are served");
        // charge-never-lost under concurrency: inner debited ≥ served.
        let debited = inner
            .get("t-conc")
            .await
            .unwrap()
            .unwrap()
            .accrued_usd_micros;
        assert!(
            debited >= served * COST,
            "concurrent inner debit ({debited}) must cover all served ({}) — no \
             double-spend / over-serve",
            served * COST
        );
    }

    // ───────────── WP-2a step 2: warm-lease READ served from memory ─────────────

    /// An inner [`QuotaStore`] that delegates to a real [`InMemoryQuotaStore`]
    /// but COUNTS every `get` READ — the read-amortisation oracle. This is the
    /// spy that proves the per-op D1 `get` is GONE on the warm path: after a
    /// lease is acquired, subsequent warm ops must NOT increment this counter.
    #[derive(Debug)]
    struct GetCountingStore {
        inner: InMemoryQuotaStore,
        get_calls: AtomicUsize,
        ceiling_calls: AtomicUsize,
    }

    impl GetCountingStore {
        fn new() -> Self {
            Self {
                inner: InMemoryQuotaStore::new(),
                get_calls: AtomicUsize::new(0),
                ceiling_calls: AtomicUsize::new(0),
            }
        }
        fn gets(&self) -> usize {
            self.get_calls.load(Ordering::SeqCst)
        }
        fn ceiling(&self) -> usize {
            self.ceiling_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl QuotaStore for GetCountingStore {
        async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
            let _ = self.get_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.get(tenant_id).await
        }
        async fn put(
            &self,
            tenant_id: &str,
            state: QuotaState,
            updated_at_ms: i64,
        ) -> Result<(), String> {
            self.inner.put(tenant_id, state, updated_at_ms).await
        }
        async fn accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<(), String> {
            self.inner
                .accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
        async fn check_and_accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<bool, String> {
            let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
            self.inner
                .check_and_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
        async fn seed_checked_accrue(
            &self,
            tenant_id: &str,
            delta_micros: i64,
            seed_anchor_ms: i64,
            updated_at_ms: i64,
        ) -> Result<bool, String> {
            let _ = self.ceiling_calls.fetch_add(1, Ordering::SeqCst);
            self.inner
                .seed_checked_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
                .await
        }
    }

    /// SPY-PROOF the per-op D1 `get` is GONE on the warm path (the core WP-2a
    /// step-2 deliverable). N ops within lease chunks of `LEASE_OPS` cause only
    /// `ceil(N / LEASE_OPS)` inner `get` READS — not one per op — AND the same
    /// `ceil(N / LEASE_OPS)` ceiling-guarded writes. Warm ops between refills
    /// make ZERO D1 round-trips of EITHER kind.
    #[tokio::test]
    async fn warm_lease_serves_read_from_memory_no_per_op_get() {
        const LEASE_OPS: i64 = 4;
        const COST: i64 = 1_000;
        let spy = Arc::new(GetCountingStore::new());
        spy.inner.seed(
            "t-warm",
            QuotaState {
                monthly_budget_usd_micros: 100 * COST, // generous — never trips
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(spy.clone(), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        // 10 ops, 4-op lease → ceil(10/4)=3 refills. Each refill does ONE `get`
        // + ONE ceiling write; the 7 warm ops in between do NEITHER.
        for _ in 0..10 {
            assert!(guard.check("t-warm", COST).await.is_none());
        }
        assert_eq!(
            spy.gets(),
            3,
            "the per-op D1 `get` is GONE: 10 ops with a 4-op lease hit `get` only \
             ceil(10/4)=3 times (once per refill), NOT once per op"
        );
        assert_eq!(
            spy.ceiling(),
            3,
            "ceiling-guarded writes also amortised to ceil(10/4)=3"
        );
        // Total D1 round-trips for 10 ops = 3 gets + 3 writes = 6, i.e. the 7
        // warm ops made ZERO D1 round-trips.
        assert_eq!(
            spy.gets() + spy.ceiling(),
            6,
            "the 7 warm ops between refills made ZERO D1 round-trips"
        );
    }

    /// INVARIANT 2 (never over-SERVE): the warm-read fast path must NOT admit an
    /// over-ceiling op. A tenant whose lease is drained AND whose durable row is
    /// at the ceiling is refused 402 — the durable atomic stays the sole ceiling
    /// authority even with the read served from memory.
    #[tokio::test]
    async fn warm_read_still_refuses_over_ceiling_402() {
        const LEASE_OPS: i64 = 8;
        const COST: i64 = 1_000_000; // $1/op
        let spy = Arc::new(GetCountingStore::new());
        // Budget exactly $3 → at most 3 ops ever, on a fresh cycle.
        spy.inner.seed(
            "t-cap2",
            QuotaState {
                monthly_budget_usd_micros: 3_000_000,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(spy.clone(), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        // The $3 ceiling admits exactly 3 ops. The 8-op chunk overshoots $3, so
        // the partial-lease fallback halves (8→4→2) to a 2-op chunk that fits —
        // so a warm op is served between refills, but the lease is small. What
        // this test PROVES is invariant 2: no matter how the read is served, the
        // 4th op is refused 402 (the durable atomic is the sole ceiling
        // authority) AND at least one op was served warm from memory.
        assert!(guard.check("t-cap2", COST).await.is_none()); // op1: refill (get)
        assert!(guard.check("t-cap2", COST).await.is_none()); // op2: warm (no get)
        assert!(guard.check("t-cap2", COST).await.is_none()); // op3: refill (get)
        // 4th op: durable row now at the $3 ceiling → the warm path cannot cover
        // it → durable path → atomic ceiling refuses → 402. The read served from
        // memory did NOT let an over-ceiling op slip through.
        let resp = guard.check("t-cap2", COST).await.expect("4th op over ceiling");
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
        // Fewer gets than ops (4 ops, 3 gets): op2 was served warm from memory —
        // proving the read is served from the lease, WITHOUT ever admitting the
        // over-ceiling 4th op (invariant 2 holds with the read served warm).
        assert!(
            spy.gets() < 4,
            "at least one op was served warm from memory — fewer gets ({}) than ops (4)",
            spy.gets()
        );
    }

    /// INVARIANT 3 (fail-CLOSED): a drained lease + an unreachable inner store
    /// refuses 503 — the warm-read path cannot mask an outage, because a drained
    /// lease returns `Ok(None)` and the guard hits the (now-erroring) durable
    /// path. Reuses [`ErrorAfterFirstLease`].
    #[tokio::test]
    async fn warm_read_drained_then_inner_error_fail_closed_503() {
        const LEASE_OPS: i64 = 2;
        const COST: i64 = 1_000;
        let store = ErrorAfterFirstLease {
            inner: InMemoryQuotaStore::new(),
            seen_first_refill: AtomicUsize::new(0),
        };
        store.inner.seed(
            "t-503b",
            QuotaState {
                monthly_budget_usd_micros: 100 * COST,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(Arc::new(store), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        // Op1: no lease → refill (2-op lease). Op2: warm from memory (no inner
        // call, so no error). Op3: lease drained → refill → inner errors → 503.
        assert!(guard.check("t-503b", COST).await.is_none());
        assert!(guard.check("t-503b", COST).await.is_none()); // warm, in-memory
        let resp = guard.check("t-503b", COST).await.expect("refill error → 503");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// INVARIANT 3b (fail-CLOSED on a poisoned lease lock): if
    /// `try_serve_from_lease` itself errors, the guard rejects 503, never
    /// fail-open. Driven by a fake whose warm path returns `Err`.
    #[derive(Debug)]
    struct WarmErrStore;
    #[async_trait::async_trait]
    impl QuotaStore for WarmErrStore {
        async fn get(&self, _t: &str) -> Result<Option<QuotaState>, String> {
            Ok(None)
        }
        async fn put(&self, _t: &str, _s: QuotaState, _u: i64) -> Result<(), String> {
            Ok(())
        }
        async fn accrue(&self, _t: &str, _d: i64, _a: i64, _u: i64) -> Result<(), String> {
            Ok(())
        }
        async fn try_serve_from_lease(
            &self,
            _tenant_id: &str,
            _cost_micros: i64,
            _now_ms: i64,
        ) -> Result<Option<bool>, String> {
            Err("simulated poisoned lease lock".to_owned())
        }
    }

    #[tokio::test]
    async fn warm_read_lease_lock_error_fail_closed_503() {
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(Arc::new(WarmErrStore), clock);
        let resp = guard.check("t-warmerr", 1_000).await.expect("warm err → 503");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// INVARIANT 4 (bounded overshoot unchanged): serving the READ from memory
    /// debits NO new budget, so the durable over-charge stays bounded by exactly
    /// one lease chunk — identical to WP-2a step 1. 10 ops, 4-op lease.
    #[tokio::test]
    async fn warm_read_overshoot_still_bounded_one_chunk() {
        const LEASE_OPS: i64 = 4;
        const COST: i64 = 1_000;
        let inner = Arc::new(InMemoryQuotaStore::new());
        inner.seed(
            "t-bound",
            QuotaState {
                monthly_budget_usd_micros: 1_000 * COST,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(inner.clone(), LEASE_OPS, COST));
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(leased, clock);

        let ops = 10_i64;
        for _ in 0..ops {
            assert!(guard.check("t-bound", COST).await.is_none());
        }
        let debited = inner.get("t-bound").await.unwrap().unwrap().accrued_usd_micros;
        let served = ops * COST;
        assert!(debited >= served, "charge-never-lost: debited ≥ served");
        assert!(
            debited <= served + LEASE_OPS * COST,
            "over-charge still bounded by one lease chunk even with the read served warm"
        );
    }

    /// INVARIANT 5 (cycle correctness within the documented staleness): once
    /// `now_ms` crosses the lease's own cycle boundary, the warm path treats the
    /// cached cycle as STALE and defers to the durable roll — a rolled ceiling is
    /// never admitted warm past the ≤ one-lease window. Proven by accounting: the
    /// post-roll op debits the FRESHLY-RESET inner row (would be free from the
    /// stale lease if the cycle check were missing).
    #[tokio::test]
    async fn warm_read_stale_cycle_defers_to_durable_roll() {
        const LEASE_OPS: i64 = 8;
        const COST: i64 = 1_000_000; // $1/op
        let inner = Arc::new(InMemoryQuotaStore::new());
        inner.seed(
            "t-cycle",
            QuotaState {
                monthly_budget_usd_micros: 5_000_000, // $5
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        let leased: Arc<dyn QuotaStore> =
            Arc::new(LeasedQuotaStore::with_config(inner.clone(), LEASE_OPS, COST));

        // Cycle 1: one op pre-buys a lease (8-op chunk $8 > $5 → partial 4-op
        // $4 chunk fits). The lease has remaining budget banked (3 ops).
        let clock1 = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard1 = QuotaGuard::new(leased.clone(), clock1);
        assert!(guard1.check("t-cycle", COST).await.is_none());
        assert_eq!(
            inner.get("t-cycle").await.unwrap().unwrap().accrued_usd_micros,
            4_000_000,
            "cycle-1 partial lease debited a 4-op chunk (banked remainder exists)"
        );

        // Advance one full cycle. The stale lease (anchored at T0) still has
        // banked remainder, but its cycle has elapsed at `later`. The warm path
        // MUST defer (cycle_elapsed → Ok(None)) so the durable roll runs and the
        // op debits the fresh cycle — it is NOT served free from the stale lease.
        let later = T0 + u64::try_from(CYCLE_LENGTH_MS).unwrap() + 1;
        let clock2 = Arc::new(InMemoryFakeWallClock::at_unix_ms(later));
        let guard2 = QuotaGuard::new(leased, clock2);
        assert!(guard2.check("t-cycle", COST).await.is_none());
        let post = inner.get("t-cycle").await.unwrap().unwrap();
        assert!(
            post.cycle_anchor_ms != i64::try_from(T0).unwrap(),
            "the inner cycle rolled (anchor advanced past T0)"
        );
        assert!(
            post.accrued_usd_micros > 0,
            "post-roll op debited the fresh inner cycle — the warm path did NOT \
             serve it free from the stale-cycle lease"
        );
    }

    /// Control: a store WITHOUT a lease (the durable/in-memory store) inherits
    /// the `Ok(None)` default, so the read is NOT short-circuited and behaviour
    /// is byte-for-byte the pre-WP-2a-step-2 path (one `get` per op).
    #[tokio::test]
    async fn no_lease_store_still_reads_per_op() {
        let spy = Arc::new(GetCountingStore::new());
        spy.inner.seed(
            "t-nolease",
            QuotaState {
                monthly_budget_usd_micros: 100_000,
                accrued_usd_micros: 0,
                cycle_anchor_ms: i64::try_from(T0).unwrap(),
            },
        );
        // NOTE: no LeasedQuotaStore wrapper — the spy IS the store.
        let clock = Arc::new(InMemoryFakeWallClock::at_unix_ms(T0));
        let guard = QuotaGuard::new(spy.clone(), clock);
        for _ in 0..5 {
            assert!(guard.check("t-nolease", 1_000).await.is_none());
        }
        assert_eq!(
            spy.gets(),
            5,
            "a store without a lease inherits the Ok(None) default → one `get` per op"
        );
    }
}
