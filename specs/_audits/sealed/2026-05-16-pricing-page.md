# Audit — Pricing page + cost calculator (wave-29 stream-7 r-prep)

- **Date:** 2026-05-16
- **Wave:** 29 (r-prep)
- **Stream:** 7 — landing-page conversion / pricing
- **Branch:** `wt/r-prep-pricing-page-calculator`
- **Base commit:** `365dd38`
- **Author:** automation (Opus 4.7)
- **Sign-off pending:** Finance, Legal, Product

## Scope

Ship a static pricing page (`/pricing`) and an interactive client-side
cost calculator (`/pricing/calculator`) for the public docs site
(`apps/docs`, Docusaurus 3). Goal: enable landing-page conversion before
the GA rate card lands, without misleading the buyer.

## Deliverables

| Path | Purpose |
| --- | --- |
| `apps/docs/src/lib/pricing.ts` | Pure pricing formulas (5-tier rate card placeholders, USD per-axis overage, period discount, recommendation, break-even). |
| `apps/docs/src/lib/pricing.test.ts` | 19 vitest cases covering boundaries, overage, Enterprise contact-sales, period discount, defensive clamping, format honesty. |
| `apps/docs/src/pages/pricing.tsx` | Public 5-tier comparison page with annual/monthly toggle and feature matrix. |
| `apps/docs/src/pages/pricing/calculator.tsx` | Interactive calculator: 6 usage inputs, 5 per-tier results, break-even headroom, pilot CTA. |
| `apps/docs/src/pages/pricing.module.css` | CSS module shared by both pages. |
| `apps/docs/i18n/{en-US,pt-BR,es-419,de}/docusaurus-plugin-content-pages/pricing/README.md` | i18n stubs for 4 locales (canonical glossary + honest pre-GA disclosure per locale). |
| `specs/_audits/2026-05-16-pricing-page.md` | This audit doc. |

## Tier taxonomy — deviation note

The wave-29 stream-7 task brief asked for a **4-tier** comparison
(Solo / Team / Business / Enterprise). The R-prep charter rule
("use canonical tier shapes from wave-13") **overrides** the brief: we
ship the canonical **5-tier** shape from
`crates/corelink-tier-selection/src/tier.rs` instead:

| Brief tier | Canonical tier shipped | Rationale |
| --- | --- | --- |
| (implicit) | **Free** | Wave-13 includes a Free tier (instant activation, rate-limited). Omitting it would create a false floor. |
| Solo | **Starter** | Same single-developer / single-repo target; canonical name preserved to match `TierKind::Starter`. |
| Team | **Team** | Identity match. |
| Business | **Pro** | Wave-13 uses `Pro` for the multi-region / BYOK tier; renaming would break surface-stability tests. |
| Enterprise | **Enterprise** | Identity match; routes to inquiry form (no Stripe Checkout). |

The audit treats this as a brief↔charter conflict resolved in favor of
charter consistency. If Finance / Product later insist on the brief's
labels, only `TIER_RATE_CARD[*].label` and copy strings change — the
shape, calculator wiring, and tests stay green.

## Honest framing checklist

- [x] No "starting at $0" anchor on the pricing page header.
- [x] Free tier shows `$0` (the real number) but is gated by hard quota — it does **not** appear as a default headline anchor.
- [x] Enterprise tier renders **Contact us** in both pages and the calculator (`monthlyTotal: null`).
- [x] Page-level banner declares pricing "subject to refinement at GA".
- [x] Calculator banner repeats the disclaimer and points readers to this audit doc.
- [x] CTAs route to `/pilot/apply` (pilot is free during evaluation, per brief).
- [x] Annual toggle says "save 15% on base" (transparent about which component is discounted).
- [x] Per-locale i18n stubs repeat the pre-GA disclaimer in each language.

## Pricing rate-card status — `<TBD per GA pricing review>`

Every USD magnitude in `TIER_RATE_CARD` is a **provisional placeholder**.
The shapes (which fields exist, which axes are billed, which tier
includes which capability) are stable; the magnitudes float. Finance
sign-off in `apps/docs/CONTENT-REVIEW.md` will lock them.

Magnitudes that ship today as placeholders (`provisional: true`):

| Tier | Base USD / mo | CAS GB overage | Transfer GB overage | Audit event overage | Region overage | BYOK overage | Seat overage |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Free | 0 | — (hard cap) | — | — | — | — | — |
| Starter | 29 | 0.12 | 0.05 | 0.0001 | 25 | — | 4 |
| Team | 149 | 0.10 | 0.04 | 0.00008 | 35 | — | 6 |
| Pro | 549 | 0.08 | 0.035 | 0.00006 | 50 | 75 | 8 |
| Enterprise | `null` (contact) | 0.06 | 0.025 | 0.00004 | negotiated | negotiated | negotiated |

Annual discount on base: **15%** (placeholder).

All values above are `<TBD per GA pricing review>` and listed here for
auditability — they ship in the calculator but are clearly flagged on
both pages and in the JSDoc of `apps/docs/src/lib/pricing.ts`.

## Quality gates

- [x] `pnpm build` apps/docs — green (see report block).
- [x] `pnpm test` lib/pricing.test.ts — passing.
- [x] `pnpm typecheck` — clean.
- [x] `pnpm lint` — clean.
- [x] `python scripts/validate_specs.py` — green.
- [x] `python scripts/validate_references.py` — green.

## Cross-functional review checklist

- [ ] **Finance** — confirm tier base prices, overage rates, annual discount.
- [ ] **Legal** — confirm "estimate only / subject to refinement" disclaimer language across the 4 locales.
- [ ] **Product** — confirm tier shape, feature gating, and recommendation logic match the GA rollout plan; confirm Solo→Starter and Business→Pro mapping (or push back).

## Follow-ups (post-sign-off)

- Replace placeholder magnitudes in `TIER_RATE_CARD`; flip
  `provisional: true` to `false` once signed off and update this audit.
- Replace per-locale MT-stub README files with native-speaker reviewed
  translations of the rendered strings (per
  `apps/docs/i18n/TRANSLATION-WORKFLOW.md`).
- Wire the "Apply for pilot" CTA target page (`/pilot/apply`) — currently
  links to an URL the docs site does not yet ship.
- Consider extracting the headline strings into a `Translate`-friendly
  table so Docusaurus's `docusaurus write-translations` can pick them up
  for the 4 locales (currently translations live in sibling README stubs).
