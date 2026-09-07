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
#[non_exhaustive]
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
    /// B-007 near-$-ceiling early-warning sink. `None` unless
    /// [`NEAR_CEILING_ALERT_SINK_ENV`] is explicitly on at construction, so
    /// the feature ships INERT: unset ⇒ `None` ⇒ the near-ceiling branch emits
    /// only the existing structured `tracing::warn!` (byte-identical to today).
    /// When `Some`, the near-ceiling branch ALSO fires a [`PagerDutyEvent`]
    /// fire-and-forget OFF the lease hot path (never awaited in `charge`).
    near_ceiling_sink: Option<Arc<dyn PagerDutyDispatcher>>,
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

/// B-007: build the near-$-ceiling [`PagerDutyEvent`] from ONLY the tenant id
/// and the three integer quota metrics — a PURE, PII/secret-free helper (no
/// PAT, no secret, no user data; the `routing_key`/Integration Key lives in
/// the future real dispatcher's own config, never in the event).
///
/// `dedup_key` is `near-ceiling:{tenant_id}` so repeat near-ceiling signals
/// for one tenant collapse to a single open incident (Events API v2 dedup-key
/// idempotency); `event_action` is `Trigger`, `severity` `sev2`, `source`
/// `tenant-quota`, `summary` a fixed template embedding the three metrics.
fn build_near_ceiling_event(
    tenant_id: &str,
    granted_micros: i64,
    full_chunk_micros: i64,
    cycle_anchor_ms: i64,
) -> PagerDutyEvent {
    PagerDutyEvent {
        // Service routing is a placeholder until B-008 wires the real
        // per-service Integration Key; the inert in-memory sink ignores it.
        service: PagerDutyServiceKey::ProdUs,
        event_action: PagerDutyEventAction::Trigger,
        dedup_key: format!("near-ceiling:{tenant_id}"),
        severity: "sev2",
        summary: format!(
            "tenant-quota: tenant {tenant_id} is within one lease-chunk of its monthly \
             $-ceiling (granted_micros={granted_micros}, full_chunk_micros={full_chunk_micros}, \
             cycle_anchor_ms={cycle_anchor_ms})"
        ),
        source: "tenant-quota",
        runbook_url: NEAR_CEILING_RUNBOOK_URL.to_owned(),
    }
}

impl LeasedQuotaStore {
    /// Wrap an inner durable [`QuotaStore`] with the default lease chunk
    /// ([`DEFAULT_LEASE_OPS`]) and the env-resolved per-op cost
    /// ([`cost_per_op_micros`]).
    #[must_use]
    pub fn new(inner: Arc<dyn QuotaStore>) -> Self {
        let mut store = Self::with_config(inner, DEFAULT_LEASE_OPS, cost_per_op_micros());
        // B-007: resolve the near-ceiling sink from the env flag at
        // construction. Default-off ⇒ `None` ⇒ the feature ships inert.
        store.near_ceiling_sink = Self::near_ceiling_sink_from_env();
        store
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
            // B-007: the seam constructor defaults the sink OFF; `new()`
            // resolves it from the env flag, tests inject it via
            // [`with_near_ceiling_sink`].
            near_ceiling_sink: None,
        }
    }

    /// B-007 builder: inject a near-$-ceiling alert sink (tests pass an
    /// [`InMemoryPagerDutyDispatcher`]). Production resolves the sink from
    /// [`NEAR_CEILING_ALERT_SINK_ENV`] in [`new`](Self::new) instead.
    #[must_use]
    pub fn with_near_ceiling_sink(mut self, sink: Arc<dyn PagerDutyDispatcher>) -> Self {
        self.near_ceiling_sink = Some(sink);
        self
    }

    /// B-007: resolve the near-ceiling sink from [`NEAR_CEILING_ALERT_SINK_ENV`].
    /// Returns `None` (feature inert) unless the var is explicitly truthy;
    /// only then is a dispatcher constructed. Today the only bindable sink is
    /// the in-memory dispatcher, which captures events but pages NOBODY — the
    /// real HTTPS `PagerDutyEventsApiV2HttpsDispatcher` + per-service
    /// `routing_key` (Integration Key) secret are B-008 (owner).
    fn near_ceiling_sink_from_env() -> Option<Arc<dyn PagerDutyDispatcher>> {
        let enabled = std::env::var(NEAR_CEILING_ALERT_SINK_ENV)
            .ok()
            .is_some_and(|v| matches!(v.trim(), "1" | "true" | "TRUE"));
        if !enabled {
            return None;
        }
        tracing::info!(
            "tenant-quota: {NEAR_CEILING_ALERT_SINK_ENV} is ON — near-$-ceiling \
             early-warning signals route to a PagerDuty sink (B-007). NOTE: the \
             only bindable dispatcher today is in-memory (pages nobody); the real \
             HTTPS dispatcher + routing_key are B-008."
        );
        Some(Arc::new(InMemoryPagerDutyDispatcher::new()))
    }

    /// The size, in micro-dollars, of a full lease chunk.
    fn full_chunk_micros(&self) -> i64 {
        self.lease_ops.saturating_mul(self.cost_per_op)
    }

    /// B-007: emit the near-$-ceiling early-warning to the configured sink,
    /// fire-and-forget, OFF the quota lease hot path.
    ///
    /// Returns `None` when no sink is configured (the default-off case — the
    /// caller in `charge` never awaits, so the hot path is byte-identical to
    /// today). When a sink is set, the [`PagerDutyEvent`] is built by the pure
    /// [`build_near_ceiling_event`] helper (PII/secret-free — only the tenant
    /// id + the three integer quota metrics) and dispatched inside a
    /// `tokio::spawn`; the SYNC [`PagerDutyDispatcher::dispatch`] is wrapped in
    /// `spawn_blocking` so even the future real blocking-HTTPS dispatcher
    /// (B-008) never blocks a runtime worker thread. Dispatch errors are
    /// logged (fail-OPEN) inside the spawned task and can NEVER change the
    /// quota decision. The returned [`JoinHandle`](tokio::task::JoinHandle) is
    /// dropped by the hot-path caller (fire-and-forget) and is only awaited by
    /// tests to observe the dispatch deterministically.
    fn emit_near_ceiling(
        &self,
        tenant_id: &str,
        granted_micros: i64,
        full_chunk_micros: i64,
        cycle_anchor_ms: i64,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let sink = Arc::clone(self.near_ceiling_sink.as_ref()?);
        let event = build_near_ceiling_event(
            tenant_id,
            granted_micros,
            full_chunk_micros,
            cycle_anchor_ms,
        );
        Some(tokio::spawn(async move {
            // spawn_blocking wraps the SYNC dispatch so a bound blocking-HTTPS
            // impl (B-008) never blocks a runtime worker on the quota path.
            match tokio::task::spawn_blocking(move || sink.dispatch(event)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(
                    error = %e,
                    "tenant-quota: near-$-ceiling alert dispatch failed (fail-OPEN — quota decision unaffected)"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    "tenant-quota: near-$-ceiling alert task join failed (fail-OPEN — quota decision unaffected)"
                ),
            }
        }))
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
                {
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
                }
                // Near-$-ceiling telemetry (WP-C). The refill loop had to shrink
                // below a full chunk to fit under the ceiling (it only halves
                // when the inner store rejected a larger chunk), so this tenant
                // is now within one lease-chunk of its monthly $-cap. Emit a
                // structured warn as an early-warning signal BEFORE the hard 402
                // (`quota_exceeded`). This is a by-product of the partial-lease
                // logic — no extra D1 round-trip on the hot path — and is
                // naturally low-volume: it can only fire on the last chunk(s) a
                // cycle has left, and the served-from-lease fast path above skips
                // it entirely. The structured log lands it in the container's logs
                // (queryable in the CF dashboard); B-007 ALSO routes it to a
                // PagerDutyDispatcher sink via `emit_near_ceiling` — a
                // tokio::spawn fire-and-forget (NO extra D1 round-trip / NO
                // .await on the hot path), gated default-off by
                // `NEAR_CEILING_ALERT_SINK` so it ships inert. The real HTTPS
                // dispatcher + per-service routing_key secret are B-008 (owner).
                if attempt < full_chunk {
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        granted_micros = attempt,
                        full_chunk_micros = full_chunk,
                        cycle_anchor_ms = anchor,
                        "tenant-quota: near monthly $-ceiling — refill granted a partial lease (within one chunk of the cap)"
                    );
                    // Fire-and-forget: the handle is intentionally dropped so
                    // `charge` returns Ok(true) WITHOUT awaiting the dispatch
                    // (preserves the "no extra D1 round-trip on the hot path"
                    // invariant). No-op when the sink is None (default-off).
                    let _ = self.emit_near_ceiling(tenant_id, attempt, full_chunk, anchor);
                }
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

#[allow(dead_code)]
const B126_M2_IMPL_1_REANCHOR: () = ();
