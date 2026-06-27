---
type: "TenancyControl"
title: "Per-tenant monthly $-ceiling"
description: "The fail-CLOSED cumulative-dollar spend cap that bounds each tenant's monthly cost blast-radius, orthogonal to the rate limit and the request quota."
source_files:
  - "crates/corelink-container/src/tenant_quota.rs"
  - "crates/corelink-container/src/main.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
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
  via the `tenant_quota` row, not a product tier (`crates/corelink-container/src/tenant_quota.rs:59-64`).
- `QuotaGuard::check` reads the wall clock first and fail-CLOSES with `503` when it is unavailable
  (`now_ms == 0`), because without a trustworthy clock the cycle boundary is unknowable
  (`crates/corelink-container/src/tenant_quota.rs:743-752`).
- A quota-store transport/decode error also returns `503` — a billable op that cannot be cost-checked is
  never served (`crates/corelink-container/src/tenant_quota.rs:762-768`).
- The ceiling decision is an atomic DB-side `check_and_accrue` (`accrued + delta <= budget`) in the
  durable `D1QuotaStore`, so two concurrent ops cannot both read the same baseline and both pass; over the
  ceiling returns `402` (`crates/corelink-container/src/tenant_quota.rs:794-804`). NOTE: the durable store
  is consulted per-op only in the abstract — the live guard fronts it with `LeasedQuotaStore` (below), so
  the ceiling is enforced **approximately, by budget-lease**, not literally once per op.
- A cycle rolls (accrued resets to 0, the anchor advances) once the clock is `CYCLE_LENGTH_MS` (~30 days)
  past the tenant's `cycle_anchor_ms` (`crates/corelink-container/src/tenant_quota.rs:155-158`).
- The batch variant charges `n * cost_each` in ONE atomic check using `saturating_mul` to stop integer
  overflow from an untrusted batch size (`crates/corelink-container/src/tenant_quota.rs:716-722`).

# Invariants

- Money is integer micro-dollars throughout; floating point is never used for the cap
  (`crates/corelink-container/src/tenant_quota.rs:32-37`).
- Every uncertain path fail-CLOSES: over-ceiling → `402`, store/clock error → `503`; only an explicit
  in-budget `Allow` proceeds (`crates/corelink-container/src/tenant_quota.rs:18-30`).
- The accrue is a serialized DB-side increment, so concurrent ops at the cycle boundary cannot lose an
  update or over-admit spend (`crates/corelink-container/src/tenant_quota.rs:777-810`).

# Gotchas

- The per-op cost is a deliberately coarse FLAT charge (default `$0.001/op`, ~5000 ops to the `$5`
  tripwire) — it is a preventive tripwire, NOT precise per-byte metering; that is a separate post-launch
  concern (`crates/corelink-container/src/tenant_quota.rs:75-89`).
- The guard is `None` (so billable routes run WITHOUT the gate) when the storage env is unset, so a
  credential-less local/CI run is unaffected — the cap only arms in a real deployment
  (`crates/corelink-container/src/tenant_quota.rs:105-116`).
- **The ceiling is APPROXIMATE BY DESIGN — `LeasedQuotaStore` (budget-lease), not per-op atomic.** The
  production guard does NOT hit D1 on every billable op: `quota_guard_from_env` wraps the durable
  `D1QuotaStore` in a `LeasedQuotaStore` (`crates/corelink-container/src/tenant_quota.rs:112-125`). On the
  first op of a `(tenant, cycle)` it atomically debits a CHUNK of budget — `DEFAULT_LEASE_OPS = 16` ops'
  worth — from D1 up front, then serves the next ~15 ops **from an in-memory lease without touching D1**,
  refilling when the lease drains (`crates/corelink-container/src/tenant_quota.rs:366-451`). This amortises
  the D1-over-HTTP round-trip ~16:1. The fail-CLOSED ceiling is preserved (a lease is acquired only when
  the inner atomic `check_and_accrue` returns `Ok(true)`; a refill that would breach the ceiling falls
  back to progressively smaller partial leases then fail-CLOSES) and the durable side NEVER exceeds the
  budget — but the consequence is that the cap is enforced at LEASE-CHUNK granularity, and a container
  crash discards the unused tail of an in-flight lease (the tenant is then slightly *under*-charged,
  bounded at `lease_ops * cost_per_op` ≈ `$0.016` on a `$5`/mo ceiling). It over-charges, never over-serves.
- **The prod-FATAL watchdog that backstops this gate is CIRCULAR.** The boot path treats a missing native
  PAT gate as FATAL only when prod is *detected*, and prod-detection is itself
  `StorageEnv::from_env().is_some() && PAT_SIGNING_KEY` (`crates/corelink-container/src/main.rs:246-250`),
  with `std::process::exit(1)` wired ONLY to the native-PAT-gate check (`:253-266`). The `$-ceiling` guard
  (along with byte-accounting and request-quota) ALSO disarms to `None` precisely when `StorageEnv` is
  absent. So a dropped `R2_S3_*`/`D1` var makes prod-detection FALSE → the watchdog never fires → the
  container boots happily with the `$-ceiling` silently OFF and no boot failure. The same config that
  disables the controls also disables the watchdog that is supposed to catch their absence.

# Citations

1. `crates/corelink-container/src/tenant_quota.rs:18-30` — the fail-CLOSED posture (`402`/`503`/Allow).
2. `crates/corelink-container/src/tenant_quota.rs:32-37` — integer micro-dollar units, no floating point.
3. `crates/corelink-container/src/tenant_quota.rs:59-64` — the `$5/mo` launch tripwire constant.
4. `crates/corelink-container/src/tenant_quota.rs:75-89` — the coarse flat per-op cost model.
5. `crates/corelink-container/src/tenant_quota.rs:105-116` — `None` guard when the storage env is unset (dev/CI).
6. `crates/corelink-container/src/tenant_quota.rs:155-158` — cycle-elapsed test against `CYCLE_LENGTH_MS`.
7. `crates/corelink-container/src/tenant_quota.rs:716-722` — `check_batch` atomic charge with `saturating_mul`.
8. `crates/corelink-container/src/tenant_quota.rs:743-752` — clock-unavailable `503` fail-close.
9. `crates/corelink-container/src/tenant_quota.rs:762-768` — store-unavailable `503` fail-close.
10. `crates/corelink-container/src/tenant_quota.rs:777-810` — atomic roll + check-and-accrue (no lost-update / over-admission).
11. `crates/corelink-container/src/tenant_quota.rs:794-804` — over-ceiling `402 Payment Required`.
12. `crates/corelink-container/src/tenant_quota.rs:112-125` — `quota_guard_from_env` fronts the durable `D1QuotaStore` with `LeasedQuotaStore` (the budget-lease wrapper).
13. `crates/corelink-container/src/tenant_quota.rs:366-451` — `DEFAULT_LEASE_OPS = 16` + the `LeasedQuotaStore` mechanism (debit a chunk up front, serve subsequent ops in-memory, fail-CLOSED refill).
14. `crates/corelink-container/src/main.rs:246-250` — prod-detection = `StorageEnv::from_env().is_some() && PAT_SIGNING_KEY` (the circular watchdog: the same signal that arms the controls arms the FATAL check).
15. `crates/corelink-container/src/main.rs:253-266` — `std::process::exit(1)` wired ONLY to the native-PAT-gate check; the metering guards just return `None` when `StorageEnv` is absent.
