# Pricing page — en-US locale

The canonical English source for `/pricing` and `/pricing/calculator` is
the TypeScript page under:

- `apps/docs/src/pages/pricing.tsx`
- `apps/docs/src/pages/pricing/calculator.tsx`

This directory exists so the Docusaurus content-pages i18n plugin has a
parallel locale directory for translation overrides. No override is
needed for en-US (it IS the source locale).

The launch rate card (Free / Pro $25/mo or $250/yr / Enterprise) is
concrete — no longer provisional. See
`specs/_audits/2026-05-27-pricing-benchmarks.md` §5 +
`specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.E.
