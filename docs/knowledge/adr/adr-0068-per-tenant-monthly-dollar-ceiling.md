---
type: "ADR"
title: "ADR-0068 — Per-tenant monthly $-ceiling: a fail-closed spend cap (G1)"
description: "Why CoreLink enforces a cumulative per-tenant monthly dollar ceiling, fail-closed, on top of the existing velocity rate limit."
source_files:
  - "specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "quota", "cost-ceiling", "fail-closed", "abuse", "hugit-p2"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0068 — Per-tenant monthly $-ceiling: a fail-closed spend cap (G1)

CoreLink's existing per-tenant `ratelimit_buckets` cap *velocity* (req/s) but not *cumulative dollars*
— a tenant operating within the rate limit can still accrue unbounded monthly cost, the real
blast-radius risk on the cheap third-party infra the campaign runs on. This ADR adds the economic
tripwire the hugit-P2 foundation gap G1 named: a per-tenant monthly $-ceiling enforced fail-closed at
the metering/quota layer. It is the cost-axis sibling of the velocity limiter and pairs with the
"policy-capped from day 1" mandate; it is also referenced by ADR-0069 as a sibling hot-path gate.

# Context

A rate limit caps velocity, not cumulative spend: a slow-but-steady pattern stays under the rate limit
yet racks cost over a month, and post-hoc billing alerts (fail-open) only detect overspend after it
happens — too late for a hard cap. Only a cumulative-$ bound stops the actual G1 gap.

# Decision

Enforce a **per-tenant monthly $-ceiling at the metering/quota layer, fail-closed.** A new
`tenant_quota` D1 table holds per-tenant `monthly_budget_usd` + `accrued_usd` (+ a cycle anchor); a
quota middleware checks `accrued + cost(op) <= monthly_budget` *before* serving a billable operation,
and over-ceiling rejects (`402 Payment Required` / `429`) until the cycle resets or the owner raises
the cap. Fail-closed is the correct trade for a cost cap: over the ceiling, protect the business
(reject) rather than availability (serve + eat cost) — the opposite of an SLO limiter.

The launch default was a symbolic `$5/mo` tripwire, but the **2026-07-09 reconciliation** amended it:
the `$5` default was an uncalibrated placeholder that tripped `402` at ~5000 ops/month — ~100× BELOW
the free tier's own request quota and redundant with the real per-tier request/storage caps + rate
limit — so the default is now **effectively-unlimited (`$1,000,000/mo`)**. The per-tenant ceiling is
retained as an owner-tunable **backstop**, set per contract for the unbounded team/enterprise tiers.

# Consequences

- A new additive `tenant_quota` migration plus the quota middleware on billable routes; it needs a
  per-op `cost(op)` estimate mapping tracked usage to dollars.
- The ceiling is owner-tunable per tenant; the default is now effectively-unlimited (`$1,000,000/mo`),
  not a tripwire — a per-tenant operator backstop rather than a normal-usage wall.
- It pairs with the existing rate limit so the two together bound both axes (velocity *and* spend);
  alerts complement but do not replace the enforced ceiling.

# Citations

1. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:24-30` — the Context: rate
   limits cap velocity not cumulative dollars, naming the G1 gap.
2. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:32-44` — the Decision: the
   fail-closed `tenant_quota` ceiling, the pre-serve middleware check, and the effectively-unlimited default.
3. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:60-65` — the Consequences:
   the additive migration + middleware + `cost(op)` estimate, and pairing with the velocity limit.
4. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:57-58` — the Alternatives-rejected:
   post-hoc billing alerts (fail-open) only detect overspend after it happens — too late for a hard cap.
5. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:67-99` — the 2026-07-09
   reconciliation: the `$5` default was redundant/miscalibrated; default is now effectively-unlimited,
   the ceiling a per-tenant operator backstop for the unbounded team/enterprise tiers.
