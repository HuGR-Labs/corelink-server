---
type: "ADR"
title: "ADR-S19-001 — Tier taxonomy amendment: 5 → 6 tiers"
description: "Amends the sealed S-19 tier taxonomy from 5 to 6 tiers (adds Solo + Max, removes Team) while carrying forward every other R-S19-7/R-S19-8 onboarding clause unchanged."
source_files:
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "launch", "tier-taxonomy", "stripe-checkout", "pricing", "onboarding", "s19"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S19-001 — Tier taxonomy amendment: 5 → 6 tiers

The sealed S-19 spec froze a 5-tier ladder (free/starter/team/pro/enterprise), but post-launch packaging analysis found it under-served both ends of the self-serve funnel and that `team` overlapped its neighbours without a distinct job-to-be-done. This ADR is the sanctioned non-sealed amendment that re-shapes the ladder to six tiers — adding Solo (entry paid) and Max (premium self-serve), removing Team — without editing immutable sealed history and without touching any invariant, security control, or latency/cost gate. It is the authoritative source for the live tier domain that checkout, billing, and the pricing page transcribe. Related: [the per-tenant dollar ceiling](/tenancy/dollar-ceiling.md) and [the billing/quota check flow](/flows/billing-quota-check.md).

# Context

R-S19-7 (sealed) specified 5 visible tiers and WI-S19-004 transcribed that as the 5-member `TierKind` enum and a "5 tiers visible" acceptance criterion; the product decision re-shapes this to a 6-tier ladder — ADD `Solo` ($15) between Free and Starter, ADD `Max` ($149) between Pro and Enterprise, REMOVE `Team` (folded into the Starter/Pro span) — with snake_case wire strings, as set out at `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:42-85`.

# Decision

The decision amends the tier count 5 → 6, widens the paid set to {Solo, Starter, Pro, Max} (Free instant-activation, Enterprise to inquiry form), amends the Stripe plan→tier map (remove `plan_team`; add `plan_solo`/`plan_max`), introduces the `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` price vars, and deliberately keeps prices OUT of the `TierKind` enum so pricing stays a billing concern — all DPA-first and webhook-driven onboarding clauses carry forward unchanged, recorded at `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:86-145`.

# Consequences

The code is already aligned (the 6-member enum, the amended Stripe map, the price vars), the persisted tier domain is widened additively via the standard 12-step D1 rebuild migration 0062 (destructive in mechanism, purely additive in effect — `'team'` retained for back-compat and retired at the app layer), and the sealed S-19 docs remain untouched historical record with a one-directional ADR→spec back-link, per `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:146-205`.

# Citations

1. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:42-85` — the sealed 5-tier baseline and the 6-tier re-shape (add Solo/Max, remove Team) with wire strings (Context).
2. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:86-145` — the amendment decision: paid set, Stripe plan map, price vars, prices-out-of-enum, carried-forward clauses.
3. `specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md:146-205` — code-aligned consequences, the additive 0062 rebuild migration, and the sealed-history spec hygiene.
