---
id: "ADR-0068"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-12"
updated: "2026-06-12"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "quota", "cost-ceiling", "hugit-p2", "g1", "fail-closed", "abuse"]
---

# ADR-0068 — Per-Tenant Monthly $-Ceiling: a Fail-Closed Spend Cap (G1)

## Status

ACTIVE — tech-lead decision (hugit-P2 foundation gap G1, 2026-06-12). WP-FOUND-2;
the economic safety net for the "policy-capped from day 1" mandate.

## Context

CoreLink enforces a per-tenant **rate** limit (req/s, `ratelimit_buckets`), but there is
**no monetary bound** — a tenant operating *within* the rate limit can still accumulate
unbounded **cost** over a month (the hugit campaign runs on cheap third-party infra where
runaway spend is the real blast-radius risk). A rate limit caps *velocity*, not
*cumulative dollars*.

## Decision

Enforce a **per-tenant monthly $-ceiling** at the metering/quota layer, **fail-closed**:

- New `tenant_quota` D1 table: per-tenant `monthly_budget_usd` + `accrued_usd` (+ cycle
  anchor). A quota middleware checks `accrued + cost(op) <= monthly_budget` **before**
  serving a billable operation; over-ceiling → reject (`402 Payment Required` /
  `429`) until the cycle resets or the owner raises the cap.
- Launch default: a **symbolic $5/mo tripwire** (deliberately conservative — bounds
  day-1 cost while real usage calibrates the number).

## Rationale

- **$ ≠ rate.** A slow-but-steady pattern stays under the rate limit yet racks cost; only
  a cumulative-$ bound stops it. This is the economic tripwire the G1 gap named.
- **Fail-closed is correct for a cost cap.** Over the ceiling, protect the *business*
  (reject) rather than availability (serve + eat cost) — the opposite trade-off from an
  SLO limiter, and the right one for "policy-capped from day 1".
- **Cheap to add, high blast-radius reduction:** one D1 table + a middleware check.

## Alternatives rejected

- **Rate-limit only:** does not bound cumulative spend (the actual G1 gap).
- **Post-hoc billing alerts (fail-open):** detects overspend *after* it happens — too late
  for a hard cap; alerts complement but don't replace the enforced ceiling.

## Consequences

- New `tenant_quota` migration (additive) + the quota middleware on billable routes.
- Needs a per-op `cost(op)` estimate (the metering already tracks usage; map to $).
- The ceiling is owner-tunable per tenant; the $5 default is a tripwire, not a product tier.
- Pairs with the existing rate limit (velocity) — the two together bound both axes.

## References

- `ratelimit_buckets` (existing velocity limit). hugit-P2 handoff (G1 + non-interference
  probes X6/X10/X11 ride this cap). CLAUDE.md product strategy (margin / win-win economics).
