---
type: "Runbook"
title: "Cross-team tech-lead handoff: the hugit P2 wave-plan response"
description: "The worked example of how CoreLink answers another team's integration ask — a cold-verified gap inventory plus a disjoint, contract-frozen, non-interfering wave plan."
source_files:
  - "docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "handoff", "tech-lead", "wave-plan", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Cross-team tech-lead handoff: the hugit P2 wave-plan response

When another HuGR team (here, hugit) asks CoreLink to close a set of integration seams, the response
is itself an artifact: a handoff doc that first cold-verifies what already exists versus what is
absent, then lays out a disjoint, contract-frozen, dependency-ordered wave plan that runs in parallel
with the launch under strict non-interference. This concept captures that operating pattern using the
2026-06-11 hugit-P2 response as the canonical worked example — the gap inventory, the
below-the-ask surprises (G0/G1), the conflict-free WP slicing, and the Mac-aware CI discipline.
Related: [the engineering onboarding runbook](/ops/engineering-onboarding.md).

# Role

It is the template for a CoreLink cross-team response: never a verbal "yes", but a verified
tense-discipline assessment plus a slice that an agent fleet can execute without conflicts and without
endangering the launch path.

# How it works

- The doc opens by framing itself as a tech-lead response to a specific incoming ask, with owner directive `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:1-12`.
- A tense-discipline table classifies each seam A–F as exists / partial / absent with file evidence `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:14-23`.
- It surfaces the gaps below the explicit ask — G0 (unprovisioned tenant + non-durable pilot store) and G1 (no monthly $-ceiling) `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:25-46`.
- The wave plan is a table of disjoint owner-files per WP with size, dependency, and non-interference columns `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:55-65`.
- Sequencing is Mac-aware and launch-first: judgment-only ADRs run during the CI storm, builds are sequenced `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:66-77`.
- The tech lead records the architecture calls (ADR-D/E/F) it will make, subject to owner veto `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:87-98`.

# Invariants

- Each WP owns disjoint files so the fan-out is conflict-free; the orchestrator owns the shared route-registration scaffold `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:52-65`.
- Non-interference caps bound the other team: hugit runs free tier + the G1 $-ceiling tripwire, off the launch/money path `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:79-85`.
- CI discipline is local incremental verify only when the Mac has headroom; never orbit CI, never a storm `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:100-106`.

# Gotchas

- The seam machinery can exist while the public API does not — e.g. CAS per-hash erase exists internally (DSR) but is absent as a public API `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:19-19`.
- Some asks are confirmed OUT of CoreLink scope (GitHub infra, runner fabric exec, cold trajectory bytes) and must be returned, not built `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:43-46`.

# Citations

1. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:1-12` — response framing + owner directive.
2. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:14-23` — the seam A–F tense-discipline assessment.
3. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:19-19` — machinery-exists-but-no-public-API example.
4. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:25-46` — gaps below the ask (G0/G1) + out-of-scope.
5. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:43-46` — confirmed out-of-scope items.
6. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:52-65` — disjoint owner-files wave-plan table.
7. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:66-77` — Mac-aware launch-first sequencing.
8. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:79-85` — non-interference caps.
9. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:87-98` — the ADR architecture calls.
10. `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md:100-106` — CI discipline.
