---
type: "TenancyControl"
title: "Per-tenant monthly $-ceiling"
description: "The fail-CLOSED cumulative-dollar spend cap that bounds each tenant's monthly cost blast-radius, orthogonal to the rate limit and the request quota."
source_files:
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs"
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs"
  - "crates/corelink-container/src/tenant_quota.rs"
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs"
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs"
source_blobs:
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs@799eaeb27a48bc57741395f246281917b5b83165"
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs@266a669375a2834fd2d36fb037d768fe26a92e2a"
  - "crates/corelink-container/src/tenant_quota.rs@fed79370a37849bfec9835b249a6eede26e64cbd"
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs@5c47ca025bbc025671e9e556397af8becd492af7"
  - "crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs@ad87d445fa898315f19e0dfc5368e217e67a94e3"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
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

- The default ceiling is **effectively-unlimited** (`$1,000,000/mo` = `1_000_000_000_000` micro-dollars);
  it is owner-tunable per tenant via the `tenant_quota` row, not a product tier. ADR-0068 reconciliation
  (2026-07-09): the prior `$5` default was an uncalibrated tripwire that tripped ~100× BELOW the tier's
  own request-cap, so it was removed as a default wall and the ceiling is now a per-tenant operator
  backstop (primarily for the unbounded team/enterprise tiers) (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:15-21`).
- `QuotaGuard::check` reads the wall clock first and fail-CLOSES with `503` when it is unavailable
  (`now_ms == 0`), because without a trustworthy clock the cycle boundary is unknowable
  (`crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:186-194`).
- The LAST per-op D1 READ is now served from the lease when warm: BEFORE the durable `get`, `check`
  consults `try_serve_from_lease` (WP-2a step 2), which serves the rolling/cycle-decision READ from a
  pre-paid, cycle-current in-memory lease (ZERO D1) and returns `Ok(Some(true))`; a drained/absent/stale
  lease returns `Ok(None)` and falls through to the durable path, an internal fault is `503`. This changes
  only WHERE the read is served — the durable atomic accrue remains the SOLE ceiling authority, so the
  over-serve bound is unchanged (`crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:202-224`,
  `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:321-367`).
- A quota-store transport/decode error also returns `503` — a billable op that cannot be cost-checked is
  never served (`crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:228-236`).
- The DURABLE ceiling authority is an atomic DB-side `check_and_accrue` (`UPDATE … WHERE accrued + ?delta <= budget RETURNING …`), so the inner store can never durably exceed `accrued + delta <= budget`; over the ceiling returns `402` (`crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs:69-102`). **CF-4 hardened the trait floor: the `QuotaStore` trait's DEFAULT `check_and_accrue` is no longer a non-atomic `get` → check → `accrue` two-step — it now FAILS CLOSED (returns `Err`, mapped to `503`), so a future non-D1 backend that forgets to override can never silently over-admit past the `$`-ceiling; the `InMemoryQuotaStore` carries an explicit atomic override under its `Mutex` and the `D1QuotaStore` override is unchanged (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:219-236`, `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:417-440`).**
- **Production does NOT call the durable store on every op.** `quota_guard_from_env` fronts `D1QuotaStore` with a `LeasedQuotaStore` (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:71-83`): the hot path pre-DEBITS a 16-op chunk into an in-memory lease via ONE inner `check_and_accrue`, then serves subsequent ops FROM MEMORY without a per-op D1 round-trip — both the accrue-write and, WP-2a step 2, the rolling-decision read (via `try_serve_from_lease`) — refilling (with a halving partial-lease fallback near the ceiling) when the lease drains (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs:123-451`). Consequences, all by design: (1) **over-CHARGE, never over-SERVE under a stable ceiling** — the full chunk is debited durably UP FRONT, so a near-ceiling refill may CHARGE for ops it never serves (over-charge), and a crash loses the already-debited unused tail (the tenant is then slightly under-served for budget already spent) — but no op is served without first being paid for in D1. (2) The default chunk and per-op cost are 16 ops and `$0.001`, so the maximum unused prepaid tail is `$0.016` (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:44`, `crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs:187-210`). This absolute bound applies regardless of the tenant's configured monthly ceiling. (3) **up to ~16-op staleness** — a durable budget change (ceiling drop, or charges from another container) is not seen until the current lease drains. A ceiling decrease can therefore leave the already-paid balance in a warm lease available until it drains; the default remaining lease is bounded by the `$0.016` chunk. Once a lease needs a refill, the D1 atomic `accrued + delta <= budget` check applies the current ceiling and fails closed if the next chunk would exceed it.
- A cycle rolls (accrued resets to 0, the anchor advances) once the clock is `CYCLE_LENGTH_MS` (~30 days)
  past the tenant's `cycle_anchor_ms` (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:110-116`).
- The batch variant charges `n * cost_each` in ONE atomic check using `saturating_mul` to stop integer
  overflow from an untrusted batch size (`crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:155-176`).

# Invariants

- Money is integer micro-dollars throughout; floating point is never used for the cap
  (`crates/corelink-container/src/tenant_quota.rs:32-37`).
- Every uncertain path fail-CLOSES: over-ceiling → `402`, store/clock error → `503`; only an explicit
  in-budget `Allow` proceeds (`crates/corelink-container/src/tenant_quota.rs:18-30`).
- The DURABLE accrue is a serialized DB-side increment (the `D1QuotaStore` override), so concurrent ops
  at the cycle boundary cannot lose an update or over-admit DURABLE spend (the atomic
  check-and-accrue `crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs:69-102` + the atomic conditional
  cycle-roll `crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs:170-181`). The in-memory `LeasedQuotaStore`
  in front amortises the round-trip but never over-SERVES: every op is paid for in D1 before it is served,
  so the cap is a hard over-serve bound even though it permits a bounded over-CHARGE
  (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:71-83`, `crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs:123-451`).

# Gotchas

- The per-op cost is a deliberately coarse FLAT charge (default `$0.001/op`) — a preventive backstop,
  NOT precise per-byte metering (that is a separate post-launch concern). Post the ADR-0068
  reconciliation the effectively-unlimited `$1,000,000/mo` default is no normal-usage wall
  (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:30-44`).
- The guard is `None` (so billable routes run WITHOUT the gate) when the storage env is unset, so a
  credential-less local/CI run is unaffected — the cap only arms in a real deployment
  (`crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:60-71`).

# Citations

1. `crates/corelink-container/src/tenant_quota.rs:18-30` — the fail-CLOSED posture (`402`/`503`/Allow).
2. `crates/corelink-container/src/tenant_quota.rs:32-37` — integer micro-dollar units, no floating point.
3. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:15-21` — the effectively-unlimited (`$1,000,000/mo`) default-ceiling constant (ADR-0068 reconciliation).
4. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:30-44` — the coarse flat per-op cost model.
5. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:60-71` — `None` guard when the storage env is unset (dev/CI).
6. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:110-116` — cycle-elapsed test against `CYCLE_LENGTH_MS`.
7. `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:155-176` — `check_batch` atomic charge with `saturating_mul`.
8. `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:186-194` — clock-unavailable `503` fail-close.
9. `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:228-236` — store-unavailable `503` fail-close.
10. `crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs:69-102` (atomic check-and-accrue) + `crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs:170-181` (atomic conditional roll) — the `D1QuotaStore` durable roll + check-and-accrue (no lost-update / over-admission).
11. `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:245-248` — over-ceiling `402 Payment Required` (guard mapping via `quota_exceeded_response`).
12. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:219-236` — the `QuotaStore` trait DEFAULT `check_and_accrue`: CF-4 made it FAIL CLOSED (`Err`→503, no non-atomic generic fallback); every backend MUST provide its own atomic check-and-increment. In-memory atomic override at `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:417-440`.
13. `crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs:69-102` — `D1QuotaStore::check_and_accrue`: the single atomic `UPDATE … WHERE accrued + ?delta <= budget RETURNING …` — the SOLE durable ceiling authority.
14. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:71-83` — `quota_guard_from_env` fronts `D1QuotaStore` with `LeasedQuotaStore` (the live production wiring).
15. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs:123-451` — `LeasedQuotaStore`: the in-memory 16-op budget lease (amortised D1 round-trip, default `$0.016` unused-tail allowance, fail-CLOSED on drained+unreachable). Its `QuotaStore` impl is annotated `#[async_trait::async_trait]` (explicit crate path post the axum-0.8 bump, which dropped the re-exported `axum::async_trait`).
16. `crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs:321-367` (trait method) + `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:81-124` (`LeasedQuotaStore` override) + `crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs:202-224` (`check` call site) — `try_serve_from_lease`: the WP-2a warm-lease READ short-circuit; the last per-op D1 read is served from the lease when warm, the durable atomic accrue remains the sole ceiling authority so the over-serve bound is unchanged.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
