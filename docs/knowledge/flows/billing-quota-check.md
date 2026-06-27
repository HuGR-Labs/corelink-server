---
type: "RequestFlow"
title: "Billing quota check flow"
description: "The synchronous per-tenant monthly $-ceiling gate every billable op pays (QuotaGate -> QuotaGuard, atomic check-and-accrue), plus the asynchronous runner usage-event ingest seam that feeds reconciliation."
source_files:
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/tenant_quota.rs"
  - "crates/corelink-container/src/routes/billing_ingest.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
provenance: "AUTHORED"
tags: ["flows", "billing", "quota", "tenancy", "request-flow"]
timestamp: "2026-06-26T00:00:00Z"
---

# Billing quota check flow

CoreLink protects its own margin with a per-tenant monthly cumulative-dollar ceiling (ADR-0068): unlike a rate limiter, which caps velocity, this caps total spend and — once tripped — rejects to protect the business, not to smooth traffic. Every billable handler pays this gate at the top, fail-closed, before doing work. Separately, the runners fabric pushes per-lease usage records to a durable ingest seam that the billing aggregator later drains; that ingest does only an idempotent raw persist and never touches the $-ceiling. This flow covers both: the synchronous gate (`routes.rs` `QuotaGate` -> `tenant_quota.rs` `QuotaGuard`) that the [CAS write flow](/flows/cas-write.md) invokes, and the asynchronous usage-ingest endpoint (`routes/billing_ingest.rs`).

# Role

The `QuotaGate` is the economic fail-closed spend cap on the hot path: it converts a billable op into a micro-dollar charge and admits or rejects against the tenant's monthly ceiling. The billing-ingest endpoint is the downstream durability seam — it stages raw runner-lease usage for the aggregator's rollup and Stripe reconciliation, deliberately decoupled from the synchronous gate.

# How it works

1. Placement: a billable handler holds `Option<QuotaGate>` and calls `gate.check(&tenant)` at the top, after scope/rate-limit and before the work; absent in dev/CI (`crates/corelink-container/src/routes.rs:219-227`).
2. Cost resolution: `QuotaGate` resolves the flat per-op micro-dollar cost once at build and delegates `check` to `QuotaGuard::check` (`crates/corelink-container/src/routes.rs:238-252`).
3. Clock gate: `QuotaGuard::check` fails closed 503 if the wall clock is unavailable — it cannot reason about the cycle boundary without a trustworthy clock (`crates/corelink-container/src/tenant_quota.rs:743-754`).
4. Row load: the tenant's `tenant_quota` row is loaded; a store error is 503, a missing row is treated as a fresh default-tripwire tenant (`crates/corelink-container/src/tenant_quota.rs:760-770`).
5. Cycle decision: a brand-new row or an elapsed cycle opens a fresh accrual baseline; otherwise the steady path continues from the prior accrued total (`crates/corelink-container/src/tenant_quota.rs:772-776`).
6. Steady path — atomic check-and-accrue: the ceiling test is serialized WITH the increment in one statement (`UPDATE ... WHERE accrued + delta <= budget RETURNING accrued`); over-ceiling -> 402, store error -> 503 (`crates/corelink-container/src/tenant_quota.rs:838-876`; trait at `crates/corelink-container/src/tenant_quota.rs:250-250`). NOTE: this describes the INNER `D1QuotaStore`. The live guard does NOT run this statement per op — it is fronted by `LeasedQuotaStore` (step 6a).
6a. Budget LEASE — the production guard amortises D1: `quota_guard_from_env` wraps the durable `D1QuotaStore` in a `LeasedQuotaStore` (`crates/corelink-container/src/tenant_quota.rs:112-125`). On the first op of a `(tenant, cycle)` it atomically pre-debits a CHUNK — `DEFAULT_LEASE_OPS = 16` ops' worth — from D1 via the inner atomic `check_and_accrue`, then serves the next ~15 ops from an in-memory lease WITHOUT touching D1, refilling when drained (`crates/corelink-container/src/tenant_quota.rs:366-451`). So the per-op atomic D1 statement of step 6 is the lease-acquire path, NOT every billable op; in steady state ~15 of 16 ops are served from memory. The fail-CLOSED ceiling is preserved (a lease is acquired only on inner `Ok(true)`; a breaching refill steps down to smaller partial leases then fail-CLOSES) and the DURABLE side never exceeds budget — but the cap is enforced at lease-chunk granularity (over-charges on a crash-discarded lease tail, never over-serves), not literally per op.
7. Fresh-tenant path: a brand-new tenant's FIRST op is ceiling-checked via `seed_checked_accrue`, so a single fat first op cannot bypass the cap; an over-budget first op is 402 (`crates/corelink-container/src/tenant_quota.rs:811-837`).
8. Batch variant: `check_batch` charges `n x cost` in ONE atomic check-and-accrue (saturating product) rather than N round-trips (`crates/corelink-container/src/tenant_quota.rs:716-723`; `crates/corelink-container/src/routes.rs:269-271`).
9. Decoupled usage ingest: `POST /internal/v1/billing/usage` gates on a DEDICATED `BILLING_INGEST_AUTH_KEY` (constant-time), validates the whole batch, then idempotently stages each record (`crates/corelink-container/src/routes/billing_ingest.rs:453-493`).
10. Idempotent persist: each record is staged with `ON CONFLICT DO NOTHING` (dedup by `(tenant_id, request_id)`); a backend fault is 503 so the runner can safely retry, accepted/deduped tallies return 202 (`crates/corelink-container/src/routes/billing_ingest.rs:495-521`).

# Invariants

- The gate is fail-closed on every uncertainty: no clock -> 503, store error -> 503, accrual fault -> 503; only an explicit under-ceiling allow proceeds (`crates/corelink-container/src/tenant_quota.rs:743-770`).
- The ceiling check and the spend accrual are atomic (serialized in one DB statement) IN THE INNER `D1QuotaStore`, closing the TOCTOU over-admission window where concurrent ops each read the same pre-accrual baseline (`crates/corelink-container/src/tenant_quota.rs:838-876`). The live guard reaches that statement through `LeasedQuotaStore`, which pre-debits a 16-op chunk and serves ~15 ops from an in-memory lease without D1 — so the per-op serialization is an attribute of the lease-acquire, not of every billable op; the DURABLE budget still never over-admits, but the cap is enforced approximately, at lease-chunk granularity (`crates/corelink-container/src/tenant_quota.rs:112-125`, `:366-451`).
- A brand-new tenant's first op is ceiling-checked, not accrued unconditionally — a fat first op cannot bypass the $-ceiling (`crates/corelink-container/src/tenant_quota.rs:811-837`).
- A batch is charged proportionally in ONE atomic statement, never per-op-looped and never as a single flat charge (`crates/corelink-container/src/tenant_quota.rs:716-723`).
- The usage-ingest secret is DEDICATED and distinct from the Worker<->container and introspect/mint secrets, keeping ingest's blast radius tight (`crates/corelink-container/src/routes/billing_ingest.rs:453-465`).
- Usage staging is idempotent by `(tenant_id, request_id)`, so a retried runner push never double-counts (`crates/corelink-container/src/routes/billing_ingest.rs:495-509`).

# Gotchas

- A 402 here means the monthly ceiling tripped ("raise the cap or wait for the cycle to reset"); a 503 means the cost-check itself could not be completed — these are NOT interchangeable and the handler must not serve on either.
- The atomic `check_and_accrue` is the production D1 path; the default trait impl falls back to the pre-fix two-step for non-D1 stores (tests / in-memory), which is NOT TOCTOU-safe and is test-only (`crates/corelink-container/src/tenant_quota.rs:838-851`).
- The billing-ingest endpoint stages raw records ONLY — it computes no aggregate, advances no hash chain, and touches no Stripe surface; that is the aggregator cron's job downstream.

# Citations

1. `crates/corelink-container/src/routes.rs:219-227` — `QuotaGate` usage contract (called at the top of a billable handler).
2. `crates/corelink-container/src/routes.rs:238-252` — `from_env` cost resolution + `check` delegating to `QuotaGuard::check`.
3. `crates/corelink-container/src/routes.rs:269-271` — `check_batch` delegating the proportional batch charge.
4. `crates/corelink-container/src/tenant_quota.rs:250-250` — the `check_and_accrue` store-trait method (atomic ceiling+accrual).
5. `crates/corelink-container/src/tenant_quota.rs:716-723` — `check_batch`: one atomic `n x cost` charge with a saturating product.
6. `crates/corelink-container/src/tenant_quota.rs:743-878` — `check`: clock gate, row load, cycle decision, steady/fresh atomic accrual, 402/503 outcomes.
7. `crates/corelink-container/src/routes/billing_ingest.rs:453-521` — `handle_ingest`: dedicated-secret gate, batch validate, idempotent stage, 202/503.
8. `crates/corelink-container/src/tenant_quota.rs:112-125` — `quota_guard_from_env` fronts the durable `D1QuotaStore` with `LeasedQuotaStore` (the production hot-path store is the budget-lease wrapper, not raw D1).
9. `crates/corelink-container/src/tenant_quota.rs:366-451` — `DEFAULT_LEASE_OPS = 16` + the `LeasedQuotaStore` mechanism: pre-debit a chunk, serve ~15 ops in-memory, fail-CLOSED refill (the per-op atomic statement is the lease-acquire path).
</content>
