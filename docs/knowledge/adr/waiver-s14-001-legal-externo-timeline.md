---
type: "ADR"
title: "WAIVER-S14-001 — Legal-externo review timeline exception"
description: "An INACTIVE waiver template that activates only if external legal review of the DPA/TIA misses the D+30 sprint window, with compensating controls, a 90-day expiry, and explicit revalidation triggers."
source_files:
  - "specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "waiver", "compliance", "legal-externo", "dpa", "tia", "s14"]
timestamp: "2026-06-26T00:00:00Z"
---

# WAIVER-S14-001 — Legal-externo review timeline exception

The DPA + Schrems II TIA templates ([ADR-S14-008](/adr/adr-s14-008-dpa-amendment-schrems-ii-tia-legal-externo.md)) need external-counsel review, but a 6-week legal lead may exceed the sprint window. This is the INACTIVE waiver template that activates only if that review misses D+30 — documenting the accepted residual risk with explicit compensating controls, a 90-day expiry, and revalidation triggers rather than letting a compliance gap go silent. It exists so a missed legal-review deadline is a bounded, monitored, time-boxed exception, not an unrecorded hole — fitting the zero-silent-debt mandate.

# Context

The waiver's trigger is that external legal review of the DPA and TIA templates is not completed by D+30 of WI-S14-008; until then it carries `Status: INACTIVE (template only)` and carries the explicit warning that activating it means the DPA/TIA are not yet legally reviewed and no enterprise customer DPA may be signed while active (absent a partial sign-off), as recorded at `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:31-60`. The compliance gap it covers spans the DPA, TIA, and the 12th Legal-Counsel sign-off, assessed HIGH but time-bounded, per `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:63-71`.

# Decision

While active, the waiver holds in place compensating controls — no enterprise DPA signed pending external sign-off, templates marked `PENDING_LEGAL_REVIEW`, active external engagement, prospect transparency, internal-only review, and weekly Compliance-Officer monitoring — recorded at `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:76-87`. It resolves when the external sign-off letter is committed (status → RESOLVED) or at the D+60 GA Evidence Gate hard deadline (escalate to SEV-2), and expiry without resolution escalates to SEV-1 halting all enterprise DPA engagements, per `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:90-97`.

# Consequences

A missed legal-review deadline becomes a bounded, monitored exception with a defined resolution and escalation path rather than a silent gap, at the cost that no enterprise DPA can close while the waiver is active — the trade-off recorded in the compensating-controls and revalidation sections at `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:76-97`.

# Citations

1. `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:31-60` — the INACTIVE status, activation warning, and the D+30 trigger condition (Context).
2. `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:63-71` — the DPA/TIA/12th-sign-off compliance gap and its HIGH (time-bounded) risk assessment.
3. `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:76-87` — the six compensating controls in force while active.
4. `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md:90-97` — the revalidation triggers, D+60 hard deadline, and expiry escalation.
