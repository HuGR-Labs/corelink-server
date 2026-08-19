# ADR — Edge-local per-tenant request metering via Durable Objects (P3)

- Status: **PROPOSED** (design only — no code lands from this ADR; it exists to make the one
  embedded product decision explicit before any implementation is scheduled).
- Date: 2026-08-19
- Area: billing / quota / multi-region hot path
- Supersedes-interim: the `EDGE_ASYNC_METER` async-KV metering (Forma 1, LIVE) — the interim that
  moved the count off the synchronous hot path but **still writes D1 US primary**.
- Related: migration `0071_monthly_request_counts.sql`, `worker/src/lib/quota.ts`
  (`incrementMonthlyRequestCount` / `tryFastRequestCount` / `checkRequestQuota`), ADR-0070
  (three-tier replica cache), the multi-region closure campaign (P3).

## 1. Context — what exists today

The monthly **request-count** cap (rate-card `requestsPerMonthMax`: free 500K, solo 2M, starter 6M,
pro/org 20M, max 80M; team/enterprise uncapped — `quota.ts:85-92`) is enforced by a single **atomic
UPSERT** into `monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms)`
(migration 0071). One row per `(tenant_id, YYYY-MM UTC)`; a new calendar month INSERTs a fresh row
(implicit reset); the UPSERT increments-and-returns the post-increment count in one round trip, so
there is no read-modify-write race across concurrent isolates.

**The problem P3 targets:** that UPSERT **always targets the D1 US (ENAM) primary**. It is the last
mandatory cross-region **write** on the request hot path. For a SAM/APAC caller it adds a
trans-Pacific/Atlantic round trip to *every metered request*, and it is a per-request D1 write
(cost + primary contention). The interim `EDGE_ASYNC_METER=on` (LIVE, all 5 regions) moved it into
`ctx.waitUntil` so it no longer blocks the response — but the cross-region write still happens, and
async metering weakens the *enforcement* edge (the response may return before the count is durable).

## 2. Goal + the inviolable invariant

Move the counter to a **per-tenant, edge-local Durable Object** that hibernates to ~0 cost when
idle, enforces the cap without a synchronous cross-region hop, and reconciles to D1
(`monthly_request_counts`) asynchronously for billing/history.

**INVARIANT (the whole reason this is "the hard piece"):**
- **No under-count** — a tenant must not be able to exceed `requestsPerMonthMax` unboundedly (revenue
  leak / quota bypass). Bounded, quantified slack is acceptable *only if* explicitly chosen (§4).
- **No over-count** — a tenant must not be charged for, or 402'd on, requests they did not make
  (false quota-exceeded = customer harm + support cost). Over-count is **never** acceptable.

A design that cannot state its worst-case count error in closed form does not ship.

## 3. The topology problem (why this isn't a mechanical port)

A Durable Object is **single-homed**: each DO id resolves to exactly one instance in one location.

- **Naïve "one DO per tenant"** gives an *exact* global count (all increments serialize through the
  one instance) — but a request that lands on the SAM worker for a tenant whose DO lives in US
  **still cross-region-hops to the US DO**. It replaces the D1-US write with a DO-US call: cheaper
  and hibernating, but the hop is *not* eliminated. It does reduce cost + primary contention and is
  strictly better than the D1 UPSERT, just not "edge-local."
- **"One DO per (tenant, region)"** is genuinely edge-local (the SAM worker talks to a SAM-homed
  shard, no hop) — but now the **global** monthly cap is the SUM of N regional shards, none of which
  sees the others in real time. The cap can be exceeded by up to the reconcile-window's worth of
  traffic before any shard notices. This is the under-count risk, and it is **fundamental** to
  edge-local sharding, not an implementation detail.

There is no free lunch: **exact global enforcement requires a single serialization point (a hop for
someone); true edge-locality requires giving up real-time global exactness.**

## 4. THE product decision (owner's call — this ADR exists to surface it)

> **How tight must the monthly request cap be enforced, and is bounded over-serve acceptable in
> exchange for edge-locality + lower COGS?**

This is a business/pricing call, not an engineering one:

- **(D-exact)** The cap is a hard contractual ceiling; a tenant must never serve one request past it.
  → single-DO-per-tenant (accept the cross-region hop for far tenants; still cheaper/greener than
  the D1 UPSERT). Simplest to prove correct.
- **(D-edge)** Edge-locality + COGS win is worth letting a tenant occasionally serve a *bounded*
  overage before the reconcile catches up. → per-(tenant,region) shards with a **headroom budget**:
  each region enforces an allocated sub-budget; a coordinator periodically redistributes unused
  headroom. Worst-case over-serve = Σ(per-region slack) and MUST be logged + bounded. Only viable
  because the caps are generous (≥500K/mo) so a few-second reconcile window is a rounding error at
  the cap — but it is still a deliberate "we may serve slightly past the cap" policy the owner signs.

**Recommendation (mine, pending owner):** **D-exact via single-DO-per-tenant.** Rationale: (a) the
current cost pain is the *D1 write*, not the hop per se — a hibernating per-tenant DO already kills
the per-request D1 primary write and the enforcement-weakening async gap, capturing most of the win;
(b) request caps are a contractual ceiling where "we let them slightly exceed it" is a worse story
than "sometimes a far request pays a few ms more"; (c) it keeps the invariant *provable* (one
serialization point = exact count). Revisit D-edge only if measured DO-hop latency for far tenants
proves material AND the owner accepts bounded over-serve.

## 5. Design (for the recommended D-exact path)

- **`RequestMeterDO`** — id = `idFromName(tenant_id)`. DO storage holds `{ year_month, count }`.
  Follows the existing DO conventions (`EventLogDO` / `ReplicationCoordinatorDO` in `wrangler.toml`).
- **Increment RPC** — `meter(tenantId, tier)`: on call, roll the month if `year_month` changed
  (fresh count=0), `count += 1`, compare to `requestsPerMonthMax(tier)`; return `{ count, exceeded }`.
  Single-threaded per DO ⇒ exact, no UPSERT race. Hibernates between calls (storage survives).
- **Worker path** — replace the `incrementMonthlyRequestCount` D1 UPSERT (`index.ts:2835`) with a
  stub call to the tenant's `RequestMeterDO`. Keep the tier resolution + `secondsUntilNextMonthStart`
  retry-after unchanged.
- **DO → D1 async reconcile** — the DO write-behinds `(tenant_id, year_month, count)` into
  `monthly_request_counts` via `ctx.waitUntil` on a throttle (e.g. every K increments or T seconds),
  so billing/history + an operator view stay populated. D1 becomes the *reconciled ledger*, not the
  *enforcement authority*. On DO cold-start after eviction, seed `count` from the D1 row (last
  reconciled) — so a lost-since-reconcile delta is the only slack, and it is an **under**-count of at
  most (increments since last reconcile), which D-exact must bound by making reconcile frequent
  enough OR by treating the DO as the source of truth and D1 as pure history (preferred: DO storage
  is durable across hibernation; a true DO loss is a platform event, not routine).
- **Enforcement authority** = the DO. D1 = ledger. Migration 0071 table is retained (reconcile
  target + existing billing reconcile lane reads it).

## 6. Failure modes to prove (test plan, before any flip)

1. **Month rollover** at the DO — the increment that crosses the UTC month boundary resets to a fresh
   count; two concurrent requests either side of midnight land in the right months.
2. **DO eviction/hibernation** mid-month — storage survives; count is not lost; cold re-read is correct.
3. **Reconcile crash** — a failed D1 write-behind never loses the DO's authoritative count (DO stays
   source of truth); the next reconcile is idempotent (UPSERT to `count`, not `+=`).
4. **Concurrency** — N concurrent `meter()` calls serialize; final count == N (no lost update, the
   race the atomic UPSERT was designed to prevent must not regress).
5. **Uncapped tiers** (team/enterprise = `MAX_SAFE_INTEGER`) — never 402, still reconciled for usage.
6. **Cutover** — dual-write (DO + legacy UPSERT) behind a flag, compare counts on real traffic (the
   `EDGE_ASYNC_METER=shadow` idiom), prove parity per region BEFORE the DO becomes authoritative.

## 7. Rollout (build-inert, owner-flips — the proven repo pattern)

- Land `RequestMeterDO` + binding + reconcile INERT behind a flag (`EDGE_DO_METER` unset/off = today's
  exact D1 path). Ship + deploy inert (zero behavior change), like the F3.2 / edge-read chapters.
- `shadow`: dual-run, compare DO count vs D1 UPSERT per region, prove parity on live traffic.
- `serve`: DO authoritative, D1 = reconciled ledger. Owner-gated flip, per-region, instant rollback
  (unset the flag → the D1 UPSERT path resumes; the table was never abandoned).

## 8. Why nothing is coded yet

The §4 product fork (exact vs bounded-over-serve) changes the DO topology, the invariant, and the
test matrix. Implementing before it is decided would bake a billing-correctness assumption the owner
never ratified — the exact "gambiarra" the zero-debt mandate forbids. Once the fork is answered this
becomes a normal techlead-decompose implementation (DO + binding + reconcile + the §6 tests + the §7
inert rollout), not research.

## Decision requested

Owner to pick §4 **(D-exact — recommended)** or **(D-edge)**. On D-exact, implementation is
schedulable as-is; on D-edge, add the coordinator + headroom-budget design + the over-serve bound
before scheduling.
