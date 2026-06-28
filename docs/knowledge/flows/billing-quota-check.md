---
type: "RequestFlow"
title: "Billing quota check flow"
description: "The synchronous per-tenant monthly $-ceiling gate every billable op pays (QuotaGate -> QuotaGuard, atomic check-and-accrue), plus the asynchronous runner usage-event ingest seam that feeds reconciliation."
source_files:
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/tenant_quota.rs"
  - "crates/corelink-container/src/routes/billing_ingest.rs"
checkpoint_sha: "57fd1bbeba017a3a9ac60d1a045728295fcf88d7"
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
3. Clock gate: `QuotaGuard::check` fails closed 503 if the wall clock is unavailable — it cannot reason about the cycle boundary without a trustworthy clock (`crates/corelink-container/src/tenant_quota.rs:764-773`).
4. Row load: the tenant's `tenant_quota` row is loaded; a store error is 503, a missing row is treated as a fresh default-tripwire tenant (`crates/corelink-container/src/tenant_quota.rs:781-790`).
5. Cycle decision: a brand-new row or an elapsed cycle opens a fresh accrual baseline; otherwise the steady path continues from the prior accrued total (`crates/corelink-container/src/tenant_quota.rs:793-796`).
6. Steady path — atomic check-and-accrue: the ceiling test is serialized WITH the increment in one statement (`UPDATE ... WHERE accrued + delta <= budget RETURNING accrued`); over-ceiling -> 402, store error -> 503 (`crates/corelink-container/src/tenant_quota.rs:859-896`; trait at `crates/corelink-container/src/tenant_quota.rs:250-250`).
7. Fresh-tenant path: a brand-new tenant's FIRST op is ceiling-checked via `seed_checked_accrue`, so a single fat first op cannot bypass the cap; an over-budget first op is 402 (`crates/corelink-container/src/tenant_quota.rs:832-857`).
8. Batch variant: `check_batch` charges `n x cost` in ONE atomic check-and-accrue (saturating product) rather than N round-trips (`crates/corelink-container/src/tenant_quota.rs:737-744`; `crates/corelink-container/src/routes.rs:269-271`).
9. Decoupled usage ingest: `POST /internal/v1/billing/usage` gates on a DEDICATED `BILLING_INGEST_AUTH_KEY` (constant-time), validates the whole batch, then idempotently stages each record (`crates/corelink-container/src/routes/billing_ingest.rs:480-515`).
10. Idempotent persist: each record is staged with `ON CONFLICT DO NOTHING` (dedup by `(tenant_id, request_id)`); a backend fault is 503 so the runner can safely retry, accepted/deduped tallies return 202 (`crates/corelink-container/src/routes/billing_ingest.rs:517-542`).

# Invariants

- The gate is fail-closed on every uncertainty: no clock -> 503, store error -> 503, accrual fault -> 503; only an explicit under-ceiling allow proceeds (`crates/corelink-container/src/tenant_quota.rs:764-790`).
- The ceiling check and the spend accrual are atomic (serialized in one DB statement), closing the TOCTOU over-admission window where concurrent ops each read the same pre-accrual baseline (`crates/corelink-container/src/tenant_quota.rs:859-896`).
- A brand-new tenant's first op is ceiling-checked, not accrued unconditionally — a fat first op cannot bypass the $-ceiling (`crates/corelink-container/src/tenant_quota.rs:832-857`).
- A batch is charged proportionally in ONE atomic statement, never per-op-looped and never as a single flat charge (`crates/corelink-container/src/tenant_quota.rs:737-744`).
- The usage-ingest secret is DEDICATED and distinct from the Worker<->container and introspect/mint secrets, keeping ingest's blast radius tight (`crates/corelink-container/src/routes/billing_ingest.rs:480-487`).
- Usage staging is idempotent by `(tenant_id, request_id)`, so a retried runner push never double-counts (`crates/corelink-container/src/routes/billing_ingest.rs:517-531`).

# Gotchas

- A 402 here means the monthly ceiling tripped ("raise the cap or wait for the cycle to reset"); a 503 means the cost-check itself could not be completed — these are NOT interchangeable and the handler must not serve on either.
- The atomic `check_and_accrue` is the production D1 path; the default trait impl falls back to the pre-fix two-step for non-D1 stores (tests / in-memory), which is NOT TOCTOU-safe and is test-only (`crates/corelink-container/src/tenant_quota.rs:250-271`).
- The billing-ingest endpoint stages raw records ONLY — it computes no aggregate, advances no hash chain, and touches no Stripe surface; that is the aggregator cron's job downstream.

# Citations

1. `crates/corelink-container/src/routes.rs:219-227` — `QuotaGate` usage contract (called at the top of a billable handler).
2. `crates/corelink-container/src/routes.rs:238-252` — `from_env` cost resolution + `check` delegating to `QuotaGuard::check`.
3. `crates/corelink-container/src/routes.rs:269-271` — `check_batch` delegating the proportional batch charge.
4. `crates/corelink-container/src/tenant_quota.rs:250-250` — the `check_and_accrue` store-trait method (atomic ceiling+accrual).
5. `crates/corelink-container/src/tenant_quota.rs:737-744` — `check_batch`: one atomic `n x cost` charge with a saturating product.
6. `crates/corelink-container/src/tenant_quota.rs:764-899` — `check`: clock gate, row load, cycle decision, steady/fresh atomic accrual, 402/503 outcomes.
7. `crates/corelink-container/src/routes/billing_ingest.rs:475-543` — `handle_ingest`: dedicated-secret gate, batch validate, idempotent stage, 202/503.
</content>
