---
type: "ADR"
title: "ADR-S13-005 — Progressive rollout controller + auto-rollback + monthly budget cap"
description: "Why deploys progress 1%->10%->50%->100% with 3 orthogonal auto-rollback triggers and a 30% monthly rollback-budget cap on measured error-budget burn."
source_files:
  - "specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s13", "progressive-rollout", "auto-rollback", "error-budget"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S13-005 — Progressive rollout controller + auto-rollback + monthly budget cap

A bad deploy that touches 100% of customers at once (FM-200) is a P1 failure mode — catastrophic for a crypto-touching change to auth, the audit chain, or secret rotation. This ADR (DRAFT, pending Architect + SRE + Security ratification) records the design of a 4-stage progressive rollout controller with three orthogonal auto-rollback triggers and a measured-burn monthly rollback-budget cap, modeled on the Google SRE Workbook canary chapter and AWS cell-based architecture.

# Context

Without progressive rollout every deployment touches all customers simultaneously; FM-200 (bad deploy = 100% blast radius) is a P1 failure mode, and a crypto-touching regression would be catastrophic (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:24-27`).

# Decision

- **4-stage progression** 1% → 10% → 50% → 100% with 15/30/60/0 min minimum dwell (~105 min total); 4 stages beat 3 because the 10%→100% jump is a 10× ratio step too coarse to catch capacity-class bugs (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:31-37`).
- **Auto-rollback — 3 independent triggers (any fires)**, each sustained 5 min to filter transient variance: error rate > baseline + 3σ, SLO burn-rate > 14.4 over 1h, and p99 latency > baseline + 50%; the three are orthogonal so no single blind spot remains (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:39-49`).
- **Monthly rollback budget cap 30%** on **measured** error-budget burn (basis points of actual SLO consumption, not a fixed 5% estimate) over a **rolling 30d** window to prevent calendar-month reset gaming; manual override needs Architect + Security + ADR waiver (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:51-59`).
- **DO singleton per environment** via `idFromName("rollout-controller-{env}")` + a D1 `UNIQUE WHERE status='active'` backstop, so staging and prod roll out independently without racing on the traffic-shift percentage (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:61-65`).
- **Cloudflare native gradual deploy** (atomic per-cell, no custom load balancer) and a **cosign signature gate** at `start()` so unsigned deploys never enter rollout (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:67-75`).

# Consequences

- Positive: FM-200 blast radius cut from 100% to 1% initial exposure; auto-rollback detection ≤10 min p99, rollback ≤5 min p99; false-positive flood protected by the 30% cap; SOC 2 CC8.1 evidence per transition (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:79-85`).
- Negative: ~105 min minimum rollout duration (no faster hot-fix without a waiver), a dependency on CF gradual-deploy availability, and the cap can freeze legitimate rollouts if genuine regressions consume 30% in one month (manual override available) (`specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:87-90`).

# Citations

1. `specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:24-27` — FM-200 (100% blast radius) as the P1 driver.
2. `specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:31-49` — the 4-stage progression and the 3 orthogonal sustained auto-rollback triggers.
3. `specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:51-75` — measured-burn 30% rolling budget cap, per-env DO singleton, CF native deploy, and cosign gate.
4. `specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md:79-90` — positive and negative consequences.
