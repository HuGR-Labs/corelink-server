---
id: "ADR-S19-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-09"
updated: "2026-06-09"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "tier-taxonomy", "onboarding", "tier-selection", "stripe-checkout", "pricing", "s19", "amendment"]
references:
  - "specs/04_sprints/_sealed/S19/_spec_contract.md"
  - "specs/04_sprints/_sealed/S19/work_items/WI-S19-004-tier-selection-stripe-checkout-inv-onboard-dpa-first-d1-lock.md"
  - "crates/corelink-tier-selection/src/tier.rs"
  - "crates/corelink-container/src/main.rs"
  - "migrations/d1/0057_tenant_tier.sql"
  - "migrations/d1/0039_tier_selection.sql"
---

# ADR-S19-001 — Tier taxonomy amendment: 5 → 6 tiers (Free/Solo/Starter/Pro/Max/Enterprise)

## Status

**ACCEPTED** (2026-06-09).

This ADR **amends** the tier taxonomy frozen in the sealed S-19 spec contract
requirement **R-S19-7** (`specs/04_sprints/_sealed/S19/_spec_contract.md §5.3`)
and elaborated in the sealed work item **WI-S19-004**
(`.../work_items/WI-S19-004-...d1-lock.md §1, §6.1, §8`). Those sealed documents
remain **immutable history** and are NOT edited; this ADR supersedes the *tier
count and membership* portion of R-S19-7 only. All other R-S19-7 / R-S19-8
clauses (Stripe Checkout reuse, `tenant_id → stripe_customer_id` mapping,
webhook-driven subscription state, INV-ONBOARD-DPA-FIRST D1 lock, enterprise =
"Contact us" inquiry-form route, free = instant activation) carry forward
unchanged.

## Context

R-S19-7 (sealed) reads, verbatim:

> **R-S19-7**: Tier selection UI (S-16 frontend) integrado Stripe Checkout
> (S-10 reuse):
> - **5 tiers visible (free/starter/team/pro/enterprise — enterprise =
>   "Contact us").**
> - Stripe Checkout redirect com tenant_id mapped to Stripe customer.
> - Webhook handler (S-10 reuse) atualiza local subscription state.

WI-S19-004 §1 / §6.1 transcribed that as the canonical 5-member
`TierKind` enum `{ Free, Starter, Team, Pro, Enterprise }` and the
acceptance criterion "Tier picker shows 5 tiers visible" (§8).

Post-launch pricing/packaging analysis concluded the original 5-tier ladder
under-serves the bottom of the self-serve funnel (no entry paid SKU between
the free tier and the first "real" paid tier) and the top of it (no premium
self-serve SKU between Pro and the white-glove Enterprise inquiry path). The
`team` SKU, conversely, was found to overlap the adjacent tiers in both price
and quota and to add funnel friction without a distinct job-to-be-done.

The product decision is therefore to re-shape the ladder from **5 → 6 tiers**:

- **ADD `Solo`** — a low-priced entry paid SKU positioned **between Free and
  Starter**.
- **ADD `Max`** — a premium self-serve paid SKU positioned **between Pro and
  Enterprise**.
- **REMOVE `Team`** — folded away (its job is now covered by the
  Starter/Pro span).

The new canonical ladder is, in order:

| # | Tier | Wire string | Checkout behaviour | Price (page + Stripe script only) |
|---|---|---|---|---|
| 1 | Free | `free` | Instant activation; no Stripe Checkout | $0 |
| 2 | Solo | `solo` | Stripe Checkout (paid) | $15 |
| 3 | Starter | `starter` | Stripe Checkout (paid) | $35 |
| 4 | Pro | `pro` | Stripe Checkout (paid) | $50 |
| 5 | Max | `max` | Stripe Checkout (paid) | $149 |
| 6 | Enterprise | `enterprise` | "Contact us" → WI-S19-005 inquiry form | "Contact us" / inquiry |

Wire strings are snake_case: `"free"`, `"solo"`, `"starter"`, `"pro"`,
`"max"`, `"enterprise"`.

## Decision

1. **Tier count is amended 5 → 6.** R-S19-7's "5 tiers visible
   (free/starter/team/pro/enterprise)" is **superseded** by the 6-tier
   taxonomy **Free / Solo / Starter / Pro / Max / Enterprise**. The acceptance
   criterion "Tier picker shows 5 tiers visible" (WI-S19-004 §8) is read,
   under this amendment, as "Tier picker shows **6** tiers visible".

2. **`requires_stripe_checkout` now covers the 4 paid tiers: Solo, Starter,
   Pro, Max.** Previously the paid set was {Starter, Team, Pro}; the paid set
   is now **{Solo, Starter, Pro, Max}**. As before, `Free` is instant
   activation (no Stripe Checkout) and `Enterprise` routes to the WI-S19-005
   inquiry form (`routes_to_inquiry_form`). The DPA-first invariant
   (R-S19-8 / INV-ONBOARD-DPA-FIRST) still applies **tenant-wide, all tiers
   including Free**, exactly as in the sealed WI.

3. **Stripe plan → tier map is amended** accordingly (canonical map lives in
   `crates/corelink-container/src/main.rs`, plan-id resolver in
   `crates/corelink-billing-stripe-materializer/src/tier.rs`):

   | Stripe plan id | Tier |
   |---|---|
   | `plan_solo` | Solo |
   | `plan_starter` | Starter |
   | `plan_pro` | Pro |
   | `plan_max` | Max |

   Change vs. the sealed map: **REMOVE `("plan_team", Team)`**; **KEEP**
   `plan_starter` and `plan_pro`; **ADD** `("plan_solo", Solo)` and
   `("plan_max", Max)`.

4. **Stripe price environment variables** (consumed by the pricing page and
   the Stripe provisioning script only — NOT by the `TierKind` enum):
   `STRIPE_PRICE_ID_SOLO`, `STRIPE_PRICE_ID_STARTER`, `STRIPE_PRICE_ID_PRO`,
   `STRIPE_PRICE_ID_MAX`. (`STRIPE_PRICE_ID_TEAM` is retired.)

5. **Prices** ($15 Solo / $35 Starter / $50 Pro / $149 Max; $0 Free;
   Enterprise = "Contact us") are display/checkout values for the pricing page
   and the Stripe script only and are deliberately **NOT encoded in the
   `TierKind` enum** (the enum stays a pure taxonomy; pricing is a billing
   concern that can move without a code change to the type).

## Scope of supersession (precise)

- **Superseded:** the *cardinality and membership* of the visible tier set in
  R-S19-7 — i.e. "5 tiers … free/starter/team/pro/enterprise". After this ADR
  the canonical visible set is the 6 listed above and `team` is **removed**.
- **Carried forward unchanged from R-S19-7 / R-S19-8 / WI-S19-004:** Stripe
  Checkout (S-10 reuse); `tenant_id → stripe_customer_id` 1:1 mapping;
  webhook-driven local subscription-state update; `Free` = instant activation;
  `Enterprise` = "Contact us" → WI-S19-005 inquiry form with backend route
  enforcement (direct paid Checkout for Enterprise rejected 422
  `use_inquiry_form`); INV-ONBOARD-DPA-FIRST via D1 `BEGIN IMMEDIATE`
  transactional check (applies to **all** tiers, Free included); cost
  regression gate ≤ 200 ms p99; the Prometheus `{tier}`-labelled metrics.

This ADR changes **only** which tier values are valid/visible and the paid
set + Stripe plan map; it changes no invariant, no security control, and no
latency/cost gate.

## Consequences

### Code (already aligned to this amendment)

- **`crates/corelink-tier-selection/src/tier.rs`** — canonical `TierKind`
  enum is the 6-member taxonomy `{ Free, Solo, Starter, Pro, Max, Enterprise }`
  with snake_case wire strings and the paid-set / inquiry-form helpers.
- **`crates/corelink-container/src/main.rs`** and
  **`crates/corelink-billing-stripe-materializer/src/tier.rs`** — Stripe
  plan → tier map is `{plan_solo→Solo, plan_starter→Starter, plan_pro→Pro,
  plan_max→Max}` (no `plan_team`).
- Pricing page + Stripe provisioning script consume
  `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}`.

### Data / migration

- The `tier_selections.tier` and `stripe_checkout_sessions.tier` inline CHECKs
  (`migrations/d1/0039_tier_selection.sql`) define the persisted tier domain.
  **Migration `0062_expand_tier_selections_6tier.sql`** widens both to accept
  `'solo'` + `'max'` (retaining `'team'` for back-compat). Because SQLite/D1
  cannot relax an inline CHECK in place, 0062 uses the standard 12-step table
  **rebuild** (`CREATE …_new` with the widened CHECK → `INSERT … SELECT` 1:1
  copy → `DROP` → `RENAME`), re-creating every index incl. the UNIQUE partial
  `idx_tenant_active_subscription` (INV-ONBOARD-DPA-FIRST). This is destructive
  in **mechanism** (rebuild) but purely **additive in effect** — the accepted
  value set only grows and zero rows are dropped or mutated. The
  `check_migrations_additive.py` gate records this via per-line
  `additive-allowed: ADR-0062` annotations on the `DROP`/`RENAME` tokens;
  **ADR-0062** is the migration-mechanism record (this ADR governs the
  taxonomy; cf. ADR-0036, D1 migration governance).
  `'team'` is additionally retired at the application/UI layer (new checkout
  flows never mint `team`), so legacy rows are never orphaned.
- The `tenant.tier` CHECK (`migrations/d1/0057_tenant_tier.sql`) already
  accepts `'solo'`; no change is needed there.
- INV-TENANT-ISOLATION and backward-compatibility (existing rows keep their
  current tier; default `'free'`) are preserved.

### Spec hygiene

- Sealed S-19 documents are untouched and remain the historical record. This
  ADR is the authoritative, **non-sealed** amendment; readers of R-S19-7 must
  follow the `superseded_by`-style cross-reference recorded here (sealed files
  cannot be edited to carry a back-link, so the binding is one-directional:
  this ADR → R-S19-7).
- The `validate_specs.py` 463/0 gate validates front-matter **schema**, not
  tier count, so this amendment keeps the gate green.

## Alternatives considered

- **Edit the sealed WI-S19-004 / `_spec_contract.md` in place.** Rejected:
  `_sealed/` is immutable history; mutating it would destroy the audit trail
  and violate the sealing discipline. An amendment ADR is the sanctioned
  mechanism.
- **Keep `Team` and only add `Solo` + `Max` (7 tiers).** Rejected: `Team`
  overlaps Starter/Pro in price and quota and adds funnel friction without a
  distinct JTBD; the frozen taxonomy contract is 6 tiers with `Team` removed.
- **Encode prices in the `TierKind` enum.** Rejected: pricing is a billing
  concern that must be able to change (promos, currency, repricing) without a
  type-level code change; the enum stays a pure taxonomy.

## References

- Sealed (history, not edited): `specs/04_sprints/_sealed/S19/_spec_contract.md`
  §5.3 R-S19-7 / R-S19-8; `.../work_items/WI-S19-004-...d1-lock.md` §1, §6.1,
  §8, §12.
- Code: `crates/corelink-tier-selection/src/tier.rs`;
  `crates/corelink-container/src/main.rs`;
  `crates/corelink-billing-stripe-materializer/src/tier.rs`.
- Migration: `migrations/d1/0057_tenant_tier.sql`;
  `migrations/d1/0039_tier_selection.sql`; ADR-0036 (D1 schema migration
  governance).
