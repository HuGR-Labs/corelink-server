---
type: "TenancyControl"
title: "Per-tenant monthly $-ceiling"
description: "The fail-CLOSED cumulative-dollar spend cap that bounds each tenant's monthly cost blast-radius, orthogonal to the rate limit and the request quota."
source_files:
  - "crates/corelink-container/src/tenant_quota.rs"
checkpoint_sha: "bc6fb3c06a01a330245625e8cb3a8a8dae49bf1d"
provenance: "AUTHORED"
tags: ["tenancy", "quota", "billing", "dollar-ceiling", "adr-0068", "fail-closed"]
timestamp: "2026-06-26T00:00:00Z"
---

# Per-tenant monthly $-ceiling

A per-tenant *rate* limit bounds velocity (req/s) but not cumulative *cost*: a slow-but-steady tenant
can stay under the rate limit and still accrue unbounded monthly spend — the real blast-radius risk on
cheap third-party infra. RATIFIED ADR-0068 closes that gap with a second, orthogonal axis: a per-tenant
monthly cumulative-dollar ceiling, checked *before* a billable op is served. Because this is a COST cap
and not an availability limiter, its uncertainty posture is the opposite of an SLO limiter — over the
ceiling, or on any path it cannot cost-check, it protects the business and REJECTS rather than serving
and eating the cost.

# Role

This is the economic guardrail of the tenancy layer. It sits at the top of every billable data-plane
handler alongside the rate-limit gate, and it is the cost half of the abuse-control triad — distinct
from the [request-quota](/tenancy/request-quota.md) (count axis) and the
[storage-quota header](/tenancy/storage-quota-header.md) (bytes axis). It is the gate the
billing-quota-check request flow invokes; all amounts are integer micro-dollars (USD × 1,000,000),
never floating point.

# How it works

- The launch tripwire is `$5/mo` expressed as `5_000_000` micro-dollars; it is owner-tunable per tenant
  via the `tenant_quota` row, not a product tier (`crates/corelink-container/src/tenant_quota.rs:58-63`).
- `QuotaGuard::check` reads the wall clock first and fail-CLOSES with `503` when it is unavailable
  (`now_ms == 0`), because without a trustworthy clock the cycle boundary is unknowable
  (`crates/corelink-container/src/tenant_quota.rs:898-906`).
- The LAST per-op D1 READ is now served from the lease when warm: BEFORE the durable `get`, `check`
  consults `try_serve_from_lease` (WP-2a step 2), which serves the rolling/cycle-decision READ from a
  pre-paid, cycle-current in-memory lease (ZERO D1) and returns `Ok(Some(true))`; a drained/absent/stale
  lease returns `Ok(None)` and falls through to the durable path, an internal fault is `503`. This changes
  only WHERE the read is served — the durable atomic accrue remains the SOLE ceiling authority, so the
  over-serve bound is unchanged (`crates/corelink-container/src/tenant_quota.rs:914-936`,
  `crates/corelink-container/src/tenant_quota.rs:405-412`).
- A quota-store transport/decode error also returns `503` — a billable op that cannot be cost-checked is
  never served (`crates/corelink-container/src/tenant_quota.rs:940-948`).
- The DURABLE ceiling authority is an atomic DB-side `check_and_accrue` (`UPDATE … WHERE accrued + ?delta <= budget RETURNING …`), so the inner store can never durably exceed `accrued + delta <= budget`; over the ceiling returns `402` (`crates/corelink-container/src/tenant_quota.rs:1214-1242`). **CF-4 hardened the trait floor: the `QuotaStore` trait's DEFAULT `check_and_accrue` is no longer a non-atomic `get` → check → `accrue` two-step — it now FAILS CLOSED (returns `Err`, mapped to `503`), so a future non-D1 backend that forgets to override can never silently over-admit past the `$`-ceiling; the `InMemoryQuotaStore` carries an explicit atomic override under its `Mutex` and the `D1QuotaStore` override is unchanged (`crates/corelink-container/src/tenant_quota.rs:260-277`, `crates/corelink-container/src/tenant_quota.rs:1121-1144`).**
- **Production does NOT call the durable store on every op.** `quota_guard_from_env` fronts `D1QuotaStore` with a `LeasedQuotaStore` (`crates/corelink-container/src/tenant_quota.rs:116-128`): the hot path pre-DEBITS a 16-op chunk into an in-memory lease via ONE inner `check_and_accrue`, then serves subsequent ops FROM MEMORY without a per-op D1 round-trip, refilling (with a halving partial-lease fallback near the ceiling) when the lease drains (`crates/corelink-container/src/tenant_quota.rs:415-841`). Consequences, all by design: (1) **over-CHARGE, never over-SERVE** — the full chunk is debited durably UP FRONT, so a near-ceiling refill may CHARGE for ops it never serves (over-charge), and a crash loses the already-debited unused tail (the tenant is then slightly under-served for budget already spent) — but no op is ever served without first being paid for in D1. (2) **bounded overshoot ~0.32%** — `16 ops × $0.001 = $0.016` against the `$5/mo` tripwire is the worst-case crash-tail. (3) **up to ~16-op staleness** — a durable budget change (ceiling drop, or charges from another container) is not seen until the current lease drains. The durable D1 atomic `accrued + delta <= budget` remains the SOLE ceiling authority, so this stays a hard cost-blast-radius bound: a tenant can be slightly over-charged but can NEVER be over-served past the ceiling.
- A cycle rolls (accrued resets to 0, the anchor advances) once the clock is `CYCLE_LENGTH_MS` (~30 days)
  past the tenant's `cycle_anchor_ms` (`crates/corelink-container/src/tenant_quota.rs:155-161`).
- The batch variant charges `n * cost_each` in ONE atomic check using `saturating_mul` to stop integer
  overflow from an untrusted batch size (`crates/corelink-container/src/tenant_quota.rs:870-877`).

# Invariants

- Money is integer micro-dollars throughout; floating point is never used for the cap
  (`crates/corelink-container/src/tenant_quota.rs:32-37`).
- Every uncertain path fail-CLOSES: over-ceiling → `402`, store/clock error → `503`; only an explicit
  in-budget `Allow` proceeds (`crates/corelink-container/src/tenant_quota.rs:18-30`).
- The DURABLE accrue is a serialized DB-side increment (the `D1QuotaStore` override), so concurrent ops
  at the cycle boundary cannot lose an update or over-admit DURABLE spend (the atomic
  check-and-accrue `crates/corelink-container/src/tenant_quota.rs:1214-1242` + the atomic conditional
  cycle-roll `crates/corelink-container/src/tenant_quota.rs:1315-1329`). The in-memory `LeasedQuotaStore`
  in front amortises the round-trip but never over-SERVES: every op is paid for in D1 before it is served,
  so the cap is a hard over-serve bound even though it permits a bounded over-CHARGE
  (`crates/corelink-container/src/tenant_quota.rs:116-128`, `crates/corelink-container/src/tenant_quota.rs:415-841`).

# Gotchas

- The per-op cost is a deliberately coarse FLAT charge (default `$0.001/op`, ~5000 ops to the `$5`
  tripwire) — it is a preventive tripwire, NOT precise per-byte metering; that is a separate post-launch
  concern (`crates/corelink-container/src/tenant_quota.rs:75-89`).
- The guard is `None` (so billable routes run WITHOUT the gate) when the storage env is unset, so a
  credential-less local/CI run is unaffected — the cap only arms in a real deployment
  (`crates/corelink-container/src/tenant_quota.rs:105-116`).

# Citations

1. `crates/corelink-container/src/tenant_quota.rs:18-30` — the fail-CLOSED posture (`402`/`503`/Allow).
2. `crates/corelink-container/src/tenant_quota.rs:32-37` — integer micro-dollar units, no floating point.
3. `crates/corelink-container/src/tenant_quota.rs:58-63` — the `$5/mo` launch tripwire constant.
4. `crates/corelink-container/src/tenant_quota.rs:75-89` — the coarse flat per-op cost model.
5. `crates/corelink-container/src/tenant_quota.rs:105-116` — `None` guard when the storage env is unset (dev/CI).
6. `crates/corelink-container/src/tenant_quota.rs:155-161` — cycle-elapsed test against `CYCLE_LENGTH_MS`.
7. `crates/corelink-container/src/tenant_quota.rs:870-877` — `check_batch` atomic charge with `saturating_mul`.
8. `crates/corelink-container/src/tenant_quota.rs:898-906` — clock-unavailable `503` fail-close.
9. `crates/corelink-container/src/tenant_quota.rs:940-948` — store-unavailable `503` fail-close.
10. `crates/corelink-container/src/tenant_quota.rs:1214-1242` (atomic check-and-accrue) + `crates/corelink-container/src/tenant_quota.rs:1315-1329` (atomic conditional roll) — the `D1QuotaStore` durable roll + check-and-accrue (no lost-update / over-admission).
11. `crates/corelink-container/src/tenant_quota.rs:972-975` — over-ceiling `402 Payment Required` (guard mapping via `quota_exceeded_response`).
12. `crates/corelink-container/src/tenant_quota.rs:260-277` — the `QuotaStore` trait DEFAULT `check_and_accrue`: CF-4 made it FAIL CLOSED (`Err`→503, no non-atomic generic fallback); every backend MUST provide its own atomic check-and-increment. In-memory atomic override at `crates/corelink-container/src/tenant_quota.rs:1121-1144`.
13. `crates/corelink-container/src/tenant_quota.rs:1214-1242` — `D1QuotaStore::check_and_accrue`: the single atomic `UPDATE … WHERE accrued + ?delta <= budget RETURNING …` — the SOLE durable ceiling authority.
14. `crates/corelink-container/src/tenant_quota.rs:116-128` — `quota_guard_from_env` fronts `D1QuotaStore` with `LeasedQuotaStore` (the live production wiring).
15. `crates/corelink-container/src/tenant_quota.rs:415-841` — `LeasedQuotaStore`: the in-memory 16-op budget lease (amortised D1 round-trip, bounded ~0.32% overshoot, over-charge-not-over-serve, fail-CLOSED on drained+unreachable).
16. `crates/corelink-container/src/tenant_quota.rs:405-412` (trait method) + `crates/corelink-container/src/tenant_quota.rs:798-841` (`LeasedQuotaStore` override) + `crates/corelink-container/src/tenant_quota.rs:914-936` (`check` call site) — `try_serve_from_lease`: the WP-2a warm-lease READ short-circuit; the last per-op D1 read is served from the lease when warm, the durable atomic accrue remains the sole ceiling authority so the over-serve bound is unchanged.
