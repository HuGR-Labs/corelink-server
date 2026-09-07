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
        self.charge(
            tenant_id,
            delta_micros,
            seed_anchor_ms,
            updated_at_ms,
            false,
        )
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
    pub async fn check_batch(
        &self,
        tenant: &str,
        n: usize,
        cost_micros_each: i64,
    ) -> Option<Response> {
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
                match self
                    .store
                    .check_and_accrue(tenant, cost, now_ms, now_ms)
                    .await
                {
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
                match self
                    .store
                    .seed_checked_accrue(tenant, cost, now_ms, now_ms)
                    .await
                {
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

include!("b126_m2_impl_02_part_02.rs");
