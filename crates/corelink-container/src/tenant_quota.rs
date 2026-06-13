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

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

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
    let store: Arc<dyn QuotaStore> = Arc::new(D1QuotaStore::new(Arc::new(client)));
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
#[axum::async_trait]
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
            return Some(
                (StatusCode::SERVICE_UNAVAILABLE, "wall clock unavailable").into_response(),
            );
        }
        // `now_ms` fits i64 for any realistic wall clock (year ~292M).
        let now_ms = i64::try_from(now_ms_u64).unwrap_or(i64::MAX);

        // A negative cost is a programming error upstream; treat it as 0
        // rather than crediting the tenant.
        let cost = cost_micros.max(0);

        // Load the current quota row (or treat a missing row as a fresh
        // default-tripwire tenant). Store error ⇒ 503.
        let existing = match self.store.get(tenant).await {
            Ok(s) => s,
            Err(_) => {
                return Some(
                    (StatusCode::SERVICE_UNAVAILABLE, "quota store unavailable").into_response(),
                );
            }
        };
        let state = existing.unwrap_or_else(|| QuotaState::fresh(now_ms));

        // Decide whether this op opens a fresh cycle. A roll (or a
        // brand-new row) zeroes the accrued baseline; otherwise the
        // baseline is the prior accrued value.
        let rolling = existing.is_none() || state.cycle_elapsed(now_ms);
        let baseline_accrued = if rolling { 0 } else { state.accrued_usd_micros };

        // The ceiling test. `baseline + cost > budget` ⇒ over the cap.
        // NOTE: this read-for-ceiling is intentionally NOT serialized with
        // the accrual write — a precise cap would need a DB transaction.
        // The cap is a coarse preventive tripwire (see the cost-model note
        // on `QuotaGuard`), so a small over-shoot under extreme concurrency
        // is acceptable; what MUST be correct is that accrual never
        // *under*-counts (lost-update), which the atomic increment below
        // guarantees.
        let projected = baseline_accrued.saturating_add(cost);
        if projected > state.monthly_budget_usd_micros {
            return Some(
                (
                    StatusCode::PAYMENT_REQUIRED,
                    "monthly $-ceiling exceeded; raise the cap or wait for the cycle to reset",
                )
                    .into_response(),
            );
        }

        // Under the ceiling — accrue then allow. The accrual must be
        // durable before we serve; a write error fail-CLOSES (503).
        if rolling {
            // Cycle roll / fresh row: absolute write of the new baseline
            // (accrued = this op's cost, anchor = now). An absolute write
            // is correct here because the new value is computed, not
            // derived from the (stale / absent) prior row.
            let rolled = QuotaState {
                monthly_budget_usd_micros: state.monthly_budget_usd_micros,
                accrued_usd_micros: cost,
                cycle_anchor_ms: now_ms,
            };
            if self.store.put(tenant, rolled, now_ms).await.is_err() {
                return Some(
                    (StatusCode::SERVICE_UNAVAILABLE, "quota accrual failed").into_response(),
                );
            }
        } else {
            // Steady state: DB-atomic increment of the accrued counter by
            // this op's DELTA (`accrued = accrued + cost`). The DB does the
            // add, so concurrent ops cannot lose each other's spend.
            if self
                .store
                .accrue(tenant, cost, state.cycle_anchor_ms, now_ms)
                .await
                .is_err()
            {
                return Some(
                    (StatusCode::SERVICE_UNAVAILABLE, "quota accrual failed").into_response(),
                );
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

#[axum::async_trait]
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

#[axum::async_trait]
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

    #[derive(Debug)]
    struct ErroringStore;

    #[axum::async_trait]
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
}
