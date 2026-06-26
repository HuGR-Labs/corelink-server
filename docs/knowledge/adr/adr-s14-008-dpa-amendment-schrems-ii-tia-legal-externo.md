---
type: "ADR"
title: "ADR-S14-008 — DPA amendment + Schrems II TIA + legal-externo review path"
description: "Ratifies the enterprise DPA + Schrems II Transfer Impact Assessment artifacts and the external-counsel review path that unblocks GDPR/LGPD-regulated contract closure."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "compliance", "dpa", "schrems-ii", "gdpr", "lgpd", "byok", "s14"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-008 — DPA amendment + Schrems II TIA + legal-externo review path

Enterprise procurement in the EU/EEA, Brazil, and US-regulated sectors stalls without a Processor-side Data Processing Agreement and a Schrems II transfer assessment, so revenue is blocked until those legal artifacts exist and are externally reviewed. This ADR ratifies authoring those artifacts as Legal-reviewable templates (not final legal text), structuring the transfer assessment on the EU-standard framework, and engaging external GDPR counsel — because CoreLink ships at solo-founder tier with no in-house counsel. It is the compliance keystone that lets the residency + BYOK technical work (S-14) translate into signed customer contracts.

# Context

The S-14 residency/BYOK/erasure stack must be backed by contractual artifacts that regulated customers require: a GDPR Art. 28 DPA committing to per-region residency, a Schrems II Transfer Impact Assessment documenting supplementary measures for the US sub-processor (Cloudflare), evidence of external legal review, a lighthouse signed DPA, and a 12th Legal-Counsel sign-off — without which transfer obligations under GDPR Art. 46 / LGPD Art. 33 §1 go unmet, as recorded in the ADR's context `specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md:38-47`.

# Decision

The decision authors a comprehensive DPA-amendment template structured for external redline (D1), structures the TIA on the EDPB Recommendations 01/2020 framework with customer-held BYOK CMK as the primary effectiveness argument (D2), engages an external GDPR/Schrems II law firm rather than relying on solo-tier in-house review (D3), closes one lighthouse enterprise customer DPA as contract-closure evidence (D4), and adds a 12th Legal-Counsel sign-off for this legal-touching work item (D5), as set out across the decision section `specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md:53-115`. If external review misses the D+30 sprint window, the WAIVER-S14-001 path is taken with a 90-day expiry and a D+60 hard deadline (D6) — see [WAIVER-S14-001](/adr/waiver-s14-001-legal-externo-timeline.md).

# Consequences

Enterprise contract closure is unblocked and the BYOK effectiveness argument satisfies the EDPB Use Case 6 standard, but the path carries external-counsel cost ($15-30k initial + ~$5k/quarter), a 6-week review lead that may force the waiver, and a single dual-hat DPO that must be externalized post-Series A — the trade-offs recorded at `specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md:128-145`.

# Citations

1. `specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md:38-47` — the five enterprise artifacts required and the GDPR Art. 46 / LGPD Art. 33 §1 obligations at stake (Context).
2. `specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md:53-115` — the D1-D5 decisions (DPA template, EDPB-framework TIA, external counsel, lighthouse DPA, 12th sign-off).
3. `specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md:128-145` — the positive/negative consequences and the cost + DPO trade-offs.
