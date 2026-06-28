---
type: "RequestFlow"
title: "Billing quota check flow"
description: "The synchronous per-tenant monthly $-ceiling gate every billable op pays (QuotaGate -> QuotaGuard, atomic check-and-accrue), plus the asynchronous runner usage-event ingest seam that feeds reconciliation."
source_files:
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/tenant_quota.rs"
  - "crates/corelink-container/src/routes/billing_ingest.rs"
checkpoint_sha: "2cfa6827a54076ff11847be74f24dee4afe3222a"
provenance: "AUTHORED"
tags: ["flows", "billing", "quota", "tenancy", "request-flow"]
timestamp: "2026-06-26T00:00:00Z"
---

# Billing quota check flow

CoreLink protects its own margin with a per-tenant monthly cumulative-dollar ceiling (ADR-0068): unlike a rate limiter, which caps velocity, this caps total spend and — once tripped — rejects to protect the business, not to smooth traffic. Every billable handler pays this gate at the top, fail-closed, before doing work. Separately, the runners fabric pushes per-lease usage records to a durable ingest seam that the billing aggregator later drains; that ingest does only an idempotent raw persist and never touches the $-ceiling. This flow covers both: the synchronous gate (`routes.rs` `QuotaGate` -> `tenant_quota.rs` `QuotaGuard`) that the [CAS write flow](/flows/cas-write.md) invokes, and the asynchronous usage-ingest endpoint (`routes/billing_ingest.rs`).

# Role

The `QuotaGate` is the economic fail-closed spend cap on the hot path: it converts a billable op into a micro-dollar charge and admits or rejects against the tenant's monthly ceiling. The billing-ingest endpoint is the downstream durability seam — it stages raw runner-lease usage for the aggregator's rollup and Stripe reconciliation, deliberately decoupled from the synchronous gate.

# How it works

1. Placement: a billable handler holds `Option<QuotaGate>` and calls `gate.check(&tenant)` at the top, after scope/rate-limit and before the work; absent in dev/CI (`crates/corelink-container/src/routes.rs:220-234`).
2. Cost resolution: `QuotaGate` resolves the flat per-op micro-dollar cost once at build and delegates `check` to `QuotaGuard::check` (`crates/corelink-container/src/routes.rs:241-259`).
3. Clock gate: `QuotaGuard::check` fails closed 503 if the wall clock is unavailable — it cannot reason about the cycle boundary without a trustworthy clock (`crates/corelink-container/src/tenant_quota.rs:786-795`).
4. Row load: the tenant's `tenant_quota` row is loaded; a store error is 503, a missing row is treated as a fresh default-tripwire tenant (`crates/corelink-container/src/tenant_quota.rs:805-813`).
5. Cycle decision: a brand-new row or an elapsed cycle opens a fresh accrual baseline; otherwise the steady path continues from the prior accrued total (`crates/corelink-container/src/tenant_quota.rs:815-818`).
6. Steady path — atomic check-and-accrue: the ceiling test is serialized WITH the increment in one statement (`UPDATE ... WHERE accrued + delta <= budget RETURNING accrued`); over-ceiling -> 402, store error -> 503 (`crates/corelink-container/src/tenant_quota.rs:899-919`). The store trait has NO non-atomic default — CF-4 made the default `check_and_accrue` fail CLOSED (`Err`), so a backend that forgets to override can never silently over-admit (`crates/corelink-container/src/tenant_quota.rs:260-277`).
7. Fresh-tenant path: a brand-new tenant's FIRST op is ceiling-checked via `seed_checked_accrue`, so a single fat first op cannot bypass the cap; an over-budget first op is 402 (`crates/corelink-container/src/tenant_quota.rs:854-880`).
8. Batch variant: `check_batch` charges `n x cost` in ONE atomic check-and-accrue (saturating product) rather than N round-trips (`crates/corelink-container/src/tenant_quota.rs:759-766`; `crates/corelink-container/src/routes.rs:276-278`). The PRODUCTION guard wraps its inner store in a `LeasedQuotaStore`, so the $-ceiling is enforced as a LEASED/approximate cap: each accrue debits a small ops-chunk lease up front and serves subsequent ops against the warm lease — worst-case overshoot is bounded to ONE lease chunk, so it never over-serves materially (`crates/corelink-container/src/tenant_quota.rs:118-127`, `crates/corelink-container/src/tenant_quota.rs:359-373`).
9. Decoupled usage ingest: `POST /internal/v1/billing/usage` gates on a DEDICATED `BILLING_INGEST_AUTH_KEY` (constant-time), validates the whole batch, then idempotently stages each record (`crates/corelink-container/src/routes/billing_ingest.rs:480-515`).
10. Idempotent persist: each record is staged with `ON CONFLICT DO NOTHING` (dedup by `(tenant_id, request_id)`); a backend fault is 503 so the runner can safely retry, accepted/deduped tallies return 202 (`crates/corelink-container/src/routes/billing_ingest.rs:517-542`).

# Invariants

- The gate is fail-closed on every uncertainty: no clock -> 503, store error -> 503, accrual fault -> 503; only an explicit under-ceiling allow proceeds (`crates/corelink-container/src/tenant_quota.rs:786-813`).
- The ceiling check and the spend accrual are atomic (serialized in one DB statement), closing the TOCTOU over-admission window where concurrent ops each read the same pre-accrual baseline (`crates/corelink-container/src/tenant_quota.rs:899-919`).
- A brand-new tenant's first op is ceiling-checked, not accrued unconditionally — a fat first op cannot bypass the $-ceiling (`crates/corelink-container/src/tenant_quota.rs:854-880`).
- A batch is charged proportionally in ONE atomic statement, never per-op-looped and never as a single flat charge (`crates/corelink-container/src/tenant_quota.rs:759-766`).
- In production the cap is LEASED, not exact-per-op: `LeasedQuotaStore` pre-debits an ops-chunk so the hot path serves against a warm lease, bounding worst-case overshoot to one lease chunk — approximate, but it never over-serves materially (`crates/corelink-container/src/tenant_quota.rs:118-127`).
- The usage-ingest secret is DEDICATED and distinct from the Worker<->container and introspect/mint secrets, keeping ingest's blast radius tight (`crates/corelink-container/src/routes/billing_ingest.rs:480-487`).
- Usage staging is idempotent by `(tenant_id, request_id)`, so a retried runner push never double-counts (`crates/corelink-container/src/routes/billing_ingest.rs:517-531`).

# Gotchas

- A 402 here means the monthly ceiling tripped ("raise the cap or wait for the cycle to reset"); a 503 means the cost-check itself could not be completed — these are NOT interchangeable and the handler must not serve on either.
- The atomic `check_and_accrue` is the production D1 path. CF-4 hardened the store trait: the DEFAULT `check_and_accrue` no longer falls back to a non-atomic `get` + `accrue` two-step — it now fails CLOSED (returns `Err`, mapped to 503), so a future non-D1 backend that forgets to override can never silently over-admit past the $-ceiling; the in-memory test store carries an explicit atomic override under its `Mutex`, and the live D1 atomic SQL path is unchanged (`crates/corelink-container/src/tenant_quota.rs:260-277`, `crates/corelink-container/src/tenant_quota.rs:1004-1027`).
- The billing-ingest endpoint stages raw records ONLY — it computes no aggregate, advances no hash chain, and touches no Stripe surface; that is the aggregator cron's job downstream.

# Citations

1. `crates/corelink-container/src/routes.rs:220-234` — `QuotaGate` usage contract (called at the top of a billable handler).
2. `crates/corelink-container/src/routes.rs:241-259` — `from_env` cost resolution + `check` delegating to `QuotaGuard::check`.
3. `crates/corelink-container/src/routes.rs:276-278` — `check_batch` delegating the proportional batch charge.
4. `crates/corelink-container/src/tenant_quota.rs:260-277` — the `check_and_accrue` store-trait method; CF-4: the default fails CLOSED (`Err`/503, no non-atomic fallback) — every backend MUST provide its own atomic check-and-increment.
5. `crates/corelink-container/src/tenant_quota.rs:759-766` — `check_batch`: one atomic `n x cost` charge with a saturating product.
6. `crates/corelink-container/src/tenant_quota.rs:786-922` — `check`: clock gate, row load, cycle decision, steady/fresh atomic accrual, 402/503 outcomes.
7. `crates/corelink-container/src/tenant_quota.rs:118-127` — `quota_guard_from_env` wraps the inner store in `LeasedQuotaStore`: the production $-ceiling is LEASED/approximate, overshoot bounded to one lease chunk.
8. `crates/corelink-container/src/routes/billing_ingest.rs:475-543` — `handle_ingest`: dedicated-secret gate, batch validate, idempotent stage, 202/503.
</content>
