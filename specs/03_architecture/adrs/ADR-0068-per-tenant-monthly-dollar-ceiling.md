---
id: "ADR-0068"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-12"
updated: "2026-07-09"
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
- Default ceiling: **effectively-unlimited** (`$1,000,000/mo`) — see the 2026-07-09
  reconciliation below. (The original launch default was a symbolic `$5/mo` tripwire;
  it was retired as a default wall because it was redundant with, and far tighter than,
  the tier request-cap.)

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

## Reconciliation (2026-07-09) — default is now effectively-unlimited

The `$5/mo` launch default was an **uncalibrated tripwire**, and reconciling it against
the real pricing/quota model showed it was both redundant and miscalibrated:

- At the placeholder `$0.001/op` cost (`DEFAULT_COST_PER_OP_MICROS`), the `$5` default
  tripped at **~5,000 ops/month** — **~100× BELOW** the free tier's own product quota
  (`worker/src/lib/quota.ts`: free = 500,000 requests/mo, enforced `429`) and ~1000× above
  real Cloudflare COGS. It silently `402`'d every self-serve tenant far below what they
  bought.
- The **real** cost protection is the per-tier **storage cap** (`402`) + **request/mo cap**
  (`429`) in `worker/src/lib/quota.ts` plus the per-second **rate limit**
  (`ratelimit_layer.rs`). Those are pricing-consistent (flat, hard-capped tiers per
  `marketing/sales/PRICING-WORKSHEET.md`) and are untouched by this reconciliation. The
  `$`-ceiling was a redundant second wall on the count axis, mis-set 100× too tight.

**Decision (amendment):** the default `monthly_budget_usd_micros` is now an
**effectively-unlimited `$1,000,000/mo`** (`1_000_000_000_000` micro-USD) in BOTH the Rust
constant `DEFAULT_MONTHLY_BUDGET_USD_MICROS` (the value a normal tenant is governed by —
it is the no-row read default AND the value `seed_checked_accrue` writes into a fresh
tenant's row) AND the migration `0066` column default (the DB-level fallback for
column-omitting INSERTs). A large finite value is used deliberately — there is **no
`0 = unlimited` sentinel** (a `0` budget would wall everything), and the `>= 0` CHECK is
preserved. The gate itself, its fail-CLOSED posture, the atomic check-and-accrue, and the
lease are all unchanged.

**The per-tenant override is retained as the deliberate backstop.** The
`monthly_budget_usd_micros` column stays owner-tunable: an operator `UPDATE` sets a real
ceiling per tenant. This is primarily for the **team/enterprise** tiers, whose product
request quota is unbounded (`MAX_SAFE_INTEGER`) and which are **contract-priced** — their
cost blast-radius is bounded by a per-contract `$`-ceiling the operator sets at onboarding
(see the go-live runbook onboarding checklist). Pre-launch tenant rows already seeded at
the old `$5` default can be raised by the same operator `UPDATE` if desired.

## References

- `ratelimit_buckets` (existing velocity limit). hugit-P2 handoff (G1 + non-interference
  probes X6/X10/X11 ride this cap). CLAUDE.md product strategy (margin / win-win economics).
