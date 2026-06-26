---
type: "ADR"
title: "ADR-S11-009 — 3 Jurisdictional Breach-Notification Templates Coverage Rationale"
description: "Why CoreLink pre-drafts breach-notification templates for exactly 3 jurisdictions (BR/EU/US-CA) pre-GA and defers UK, other US states, India, and China to post-GA enterprise expansion."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "breach-notification", "lgpd", "gdpr", "ccpa", "jurisdictional-coverage"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-009 — 3 Jurisdictional Breach-Notification Templates Coverage Rationale

A breach-notification template takes 4–8h to draft under legal review, and that drafting time eats directly into the 72h regulatory SLA during an actual incident. Pre-drafting removes the bottleneck — but each template carries legal-review cost, so this ADR bounds pre-GA coverage to the three jurisdictions (BR + EU + US-CA) that reach 90%+ of the pre-GA customer base, deferring the rest behind a quarterly escalation trigger.

# Context

WI-S11-006 needs pre-drafted templates because ad-hoc drafting consumes the 72h SLA under GDPR Art. 33 + LGPD Art. 48 + CCPA §1798.82. The design question is how many jurisdictions to cover pre-GA, against a landscape that includes BR, EU, US-CA, UK (ICO), other US states (VA/CO/CT/TX), India (DPDPA), China (PIPL), and Canada (PIPEDA/Law 25) (`specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:23-42`).

# Decision

Cover **3 jurisdictions** — BR + EU + US-CA — pre-GA, and defer UK ICO, other US-state AGs, India DPDPA, China PIPL, and Canada PIPEDA to post-GA enterprise expansion (S-19+) (`specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:44-47`). The three are sufficient because HuGR is Brazil-incorporated (all customers are LGPD titulares, non-optional), the Irish DPC as EU lead supervisory authority covers the whole EU in one template, and California's CCPA is the de-facto US standard that other states' templates can be lightly adapted from. UK is deferred because UK GDPR is structurally identical and the Irish DPC template covers ~90% (derivable in ~2h on demand); other US states share CCPA's structure; and India/China are deferred because DPDPA rules are pending and PIPL needs in-China residency — neither has pre-GA accounts (`specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:49-78`). This decision is reviewed quarterly and escalated immediately if an enterprise prospect materializes in an uncovered jurisdiction (`specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:80-87`).

# Consequences

Pre-drafted templates cover 90%+ of pre-GA customers and bound legal-review burden to 3 templates, making the ≤4h time-to-decision SLO achievable. The residual risk: a breach touching UK or other-US-state residents needs an ad-hoc notification under time pressure, mitigated by a pre-GA international-privacy legal retainer (`specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:89-99`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:23-42` — Context: 72h SLA pressure + the full jurisdiction landscape.
2. `specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:44-47` — Decision: cover BR+EU+US-CA, defer the rest.
3. `specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:49-78` — Rationale: why the 3 suffice and why UK/US-states/India/China are deferred.
4. `specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:80-87` — the quarterly review/escalation trigger.
5. `specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md:89-99` — Consequences and residual-risk mitigation.
