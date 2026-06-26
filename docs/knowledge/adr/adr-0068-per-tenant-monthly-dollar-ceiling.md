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
(reject) rather than availability (serve + eat cost) — the opposite of an SLO limiter. The launch
default is a deliberately conservative symbolic $5/mo tripwire while real usage calibrates the number.

# Consequences

- A new additive `tenant_quota` migration plus the quota middleware on billable routes; it needs a
  per-op `cost(op)` estimate mapping tracked usage to dollars.
- The ceiling is owner-tunable per tenant; the $5 default is a tripwire, not a product tier.
- It pairs with the existing rate limit so the two together bound both axes (velocity *and* spend);
  alerts complement but do not replace the enforced ceiling.

# Citations

1. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:24-30` — the Context: rate
   limits cap velocity not cumulative dollars, naming the G1 gap.
2. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:32-41` — the Decision: the
   fail-closed `tenant_quota` ceiling, the pre-serve middleware check, and the $5 tripwire default.
3. `specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md:58-63` — the Consequences:
   the additive migration + middleware + `cost(op)` estimate, and pairing with the velocity limit.
