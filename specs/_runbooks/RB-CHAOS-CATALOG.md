---
id: "RB-CHAOS-CATALOG"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "chaos", "catalog", "s17", "ops-maturity", "fm-coverage", "cleanup-pass"]
---

# RB-CHAOS-CATALOG — Chaos experiment catalog & FM ↔ runbook cross-reference

> **Status:** stub reference. Authoritative per-experiment specs live in
> `specs/05_quality/chaos/<experiment>.md` (created by WI-S17-001 in the
> parallel worktree). This document is the WI-S17-006 *cleanup pass*
> deliverable: it provides the cross-reference table the per-experiment
> docs do not own, plus the cleanup-pass discipline notes.
>
> Per spec contract S-17 §5.6 + 14.s17.6: every chaos PR requires a
> catalog entry + safe-mode threshold + SRE reviewer. This file is the
> index those PRs link from.

---

## 1. Coverage cross-reference (chaos experiment ↔ FM ↔ runbook)

8 chaos experiments at GA covering ≥ 8 FMs per DoD §6. Cleanup-pass
refinements (seed determinism, safe-mode thresholds, FM mappings) flagged
inline.

| # | Chaos experiment | Type | Primary FM | Secondary FM | Runbook(s) | Safe-mode threshold | Seed strategy | Cleanup note |
|---|---|---|---|---|---|---|---|---|
| 1 | R2 GET latency injection +500 ms | latency | FM-150 (transient API) | FM-153 (Grafana outage cascade) | RB-FM-153-grafana-cloud-outage | staging error rate > 50 % OR prod SEV-1 → abort | xorshift64 seeded `(experiment_id, run_ts // 3600)` | seed determinism verified 2026-05-14 reproducibility check |
| 2 | D1 query latency +200 ms | latency | FM-150 | FM-300 (gc refcount) | RB-FM-300-gc-refcount-bug | same | same | mapping clarified — was tagged FM-300 only, now primary FM-150 |
| 3 | Neon query latency +300 ms | latency | FM-150 | FM-302 (billing leak) | RB-FM-302-billing-leak | same | same | threshold tightened: was 60 %, now 50 % (matches §5.1 R-S17-3) |
| 4 | KV latency +100 ms | latency | FM-150 | FM-254 (cache poisoning) | RB-FM-254-cache-poisoning | same | same | — |
| 5 | R2 5xx failure injection 1 % | failure | FM-400 (retry storm) | FM-150 | RB-FM-400-retry-storm | staging error rate > 50 % OR prod SEV-1 → abort | seeded `(experiment_id, run_ts // 3600)` | safe-mode auto-abort tested 2026-05-14; passes |
| 6 | D1 timeout failure 0.5 % | failure | FM-400 | FM-305 (tombstone lost) | RB-FM-305-tombstone-lost | same | same | — |
| 7 | DO storage near-limit | resource exhaustion | FM-059 (DO quota exceeded) | FM-202 (runbook stale) | RB-FM-059-do-quota-exceeded · RB-FM-206-terraform-drift | quota > 95 % → safe-mode | seeded `(experiment_id, run_ts // 86400)` (daily) | mapping added — was only FM-059, secondary FM-202 (runbook coverage) noted post-1st-run |
| 8 | Edge-to-origin partition 10 s | network partition | FM-160 (auth invalid storm) | FM-156 (dep malicious) | RB-FM-160-auth-invalid-storm · RB-FM-156-dep-malicious | partition > 30 s OR prod SEV-1 → abort | seeded `(experiment_id, run_ts // 3600)` | — |

Notes:
- All experiments **staging-only at GA** (spec contract §10 anti-scope:
  ❌ chaos in prod). Code-level enforcement: `WHERE env = 'staging'`
  guard in chaos scheduler entry point (WI-S17-001).
- 7-year archive: each run emits EVT-023 + EVT-018 (chaos catalog
  cleanup) to R2 with `retain_until = now + 7y` per Quality Standard
  14.s17.1.

---

## 2. Cleanup-pass discipline (post-1st-run)

Per WI-S17-006 §6.1 item 2 ("Chaos catalog cleanup post-1st run"):

1. **Seed determinism refined.** All 8 experiments now declare seed
   strategy explicitly in column "Seed strategy" above. Hourly-bucket
   seeds (`run_ts // 3600`) give 24 reproducible distinct runs/day; daily
   bucket for storage-pressure (only 1 useful run/day).

2. **Safe-mode thresholds refined.** Standard threshold for latency /
   failure experiments harmonised at `staging error rate > 50 % OR prod
   SEV-1 → abort` (was inconsistent 50–60 % across early drafts).
   Resource-exhaustion experiment 7 keeps its specific `quota > 95 %`
   threshold (matches FM-059 trigger).

3. **FM mappings refined.** Three mapping updates from the 1st run:
   - Experiment 2 (D1 latency) — primary FM corrected from FM-300 to
     FM-150 (transient API). FM-300 (gc refcount) remains secondary.
   - Experiment 7 (DO storage near-limit) — secondary FM-202 (runbook
     stale) added; the 1st run revealed that the DO quota runbook had
     drift that the chaos run surfaced.
   - All experiments now name at least one runbook explicitly.

4. **Committed.** This file is the cleanup-pass artifact;
   per-experiment files live under `specs/05_quality/chaos/`.

---

## 3. Runbook drill cadence linkage

Per PAT-RUNBOOK-DRILL-001 (monthly) — WI-S17-003 selects a P0/P1
runbook each month. The chaos catalog above is the primary input for
that selection: runbooks named in column "Runbook(s)" are first-choice
candidates because they have a paired chaos experiment to drill against.

---

## 4. Cross-references

- Per-experiment specs: `specs/05_quality/chaos/*.md` (WI-S17-001).
- Tabletop template: `specs/_runbooks/RB-TABLETOP-TEMPLATE.md`.
- Game day report Q2-2026: `specs/_audits/2026-05-14-s17-tabletop-byok-revoke.md`.
- Adversarial summary: `specs/_audits/2026-05-14-s17-adversarial-summary.md`.
- Spec contract: `specs/04_sprints/S17/_spec_contract.md` §5.1 + §9 + §14 + §15.

---

## 5. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S17-006 builder) | Initial chaos catalog cleanup pass — 8 experiments × FM × runbook cross-reference; seed determinism / safe-mode threshold / FM mapping refinements documented. |
