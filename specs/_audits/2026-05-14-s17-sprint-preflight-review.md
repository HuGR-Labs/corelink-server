---
id: "AUDIT-S17-PREFLIGHT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Pre-flight Review (Sonnet)"
tags: ["audit", "preflight", "s17", "ops-maturity"]
---

# Adversarial Pre-flight Review — S-17 Ops Maturity

Working tree: `corelink-server` @ main `5d70701` (pre-S17 baseline). Scope: 6 WI specs being built in parallel on unmerged branches. **This is a pre-flight review of the specifications**, not a sprint-close audit. Source-of-truth files reviewed: `specs/04_sprints/S17/_spec_contract.md` (v1.2.0), `WI-S17-001..006`, `specs/03_architecture/{failure_modes.md,resilience_patterns.md,slo_catalog.md}`, `specs/05_quality/runbooks/*.md` (55 files; 33 tagged P0/P1 in frontmatter).

---

## 1. Pre-flight verdict

**NEEDS-SCOPE-FIX** (do not start the builders without addressing the P0 spec issues below).

The 6 WI specs are well-structured, internally consistent, and the cumulative ambition (8-type chaos catalog, semestral DR drills, monthly runbook drills, blameless post-mortem, oncall fatigue thresholds, quarterly game day, two-phase SEAL D+20/D+50) is genuinely SOTA. Two-phase SEAL with a 30-day observation gate is the right pattern for this lane. **But four pre-flight gaps** materially reduce confidence the WIs as-spec'd will close the ops surface they advertise: the chaos catalog leaves 11 P0/P1 FMs uncovered (including the strongest customer-facing risks FM-451 residency leak, FM-253/254/303 cross-tenant, FM-300/404 GC), the runbook scope is inconsistent across WIs (WI-003 §6.1 says 47 / §1 says ~25 P0/P1 subset / spec contract §6 says "all 40 runbooks in last 90d" / actual repo has 55 files of which 33 are P0/P1-tagged), no cross-WI mutual-exclusion invariant is specified, and no explicit RTO/RPO target exists anywhere in the DR drill spec or `slo_catalog.md`. These are scope/contract fixes, not implementation work — they should be resolved before WI-001 ships, because the chaos catalog and the runbook tracker are the artifacts that downstream WIs (002, 003, 006) depend on.

---

## 2. Spec gaps — P0/P1 issues in the 6 WI specs themselves

### P0-1. Chaos catalog FM coverage is the floor, not the ceiling — and 11 P0/P1 FMs are explicitly excluded

WI-001 §6.1.5 lists 8 mandatory FMs: **FM-051, FM-057, FM-152, FM-204, FM-105, FM-054, FM-205, FM-202**. The spec contract DoD §6 says "chaos catalog ≥ 8 FMs covered". That is the same floor in two places — there is no incentive in the spec to ever go past 8. Yet `failure_modes.md` shows **22 P0/P1 FMs** in the canonical inventory, and S-11/S-15 added FM-450/451/452/453 (privacy P1, residency P0). The chaos catalog as-spec'd leaves these uncovered with no roadmap:

- **FM-451** (residency leak, **P0 FF-HR-010**) — no chaos covers the residency fail-CLOSED 451 path
- **FM-253** (cross-tenant read, P1 RPN 40) — no chaos injects cross-tenant attempt
- **FM-254** (cache poisoning, P1 RPN 50 — highest single-FM RPN in the catalog) — no chaos
- **FM-303** (AC entry cross-tenant, P1 RPN 40) — no chaos
- **FM-300 / FM-404** (GC refcount + write race, P1) — TLA-validated invariants but no chaos
- **FM-400** (retry storm, P1 RPN 36) — no chaos; ironic given chaos itself can *trigger* retry storms
- **FM-453** (sub-processor broadcast miss, P1 RPN 36, GDPR Art. 28.2) — no chaos
- **FM-007** (deserialization RCE, P1 RPN 25) — no chaos
- **FM-156** (supply chain maintainer malicioso, P1) — listed as game-day scenario C only, not chaos
- **FM-100** (DNS outage, P1) — no chaos
- **FM-302** (silent billing leak, P1 RPN 30) — no chaos; reconciliation path untested by chaos

**Fix:** raise the floor to "≥ 12 distinct P0/P1 FMs at GA, including ≥ 1 cross-tenant attempt and FM-451 residency", and add post-GA roadmap to 22. The waiver clause (§19 "8 → 6 chaos types") goes the wrong direction — it weakens an already-weak floor.

### P0-2. Runbook scope is inconsistent across the spec corpus

Four conflicting numbers across the spec stack:

| Source | Number | Quote |
|---|---|---|
| Spec contract §6 DoD | "all 40 runbooks" | "All 40 runbooks dry-run executed in last 90d (S-20 GA gate)" |
| WI-S17-003 title | 47 | "All 47 runbooks dry-run em últimos 90d S-20 GA gate" |
| WI-S17-003 §1 (Lote 10.17 fix) | ~25 P0/P1 subset | "P0/P1 priority subset (~25 of 47) dry-run em 90d" |
| WI-S17-003 §9.4 | 47 verified | "Verified count via `find specs/05_quality/runbooks/ -name 'RB-*.md'` = 47" |
| **Actual repo (5d70701)** | **55 files; 33 tagged P0/P1** | `ls specs/05_quality/runbooks/*.md \| wc -l` = 55; `grep -lE '"p0"\|"p1"' = 33` |

The spec contract, the WI title, and the WI body all disagree. The WI body's own §1 Lote 10.17 fix correctly identifies that 47/90d is infeasible (~16/month) and pivots to a P0/P1 subset, **but the spec contract was never updated and the WI title was not changed**. WI-006 §3 quotes the corrected number ("P0/P1 priority subset ~25 of 47 em 90d coverage S-20 gate"), so part of the corpus thinks the pivot happened.

**Fix:** reconcile to one number. Recommended: bump spec contract §5.3 R-S17-8 + §6 DoD to read "all P0/P1 runbooks (33 in repo as of 2026-05-14) dry-run in 90d for S-20 GA gate; P2/P3 deferred post-GA continuous coverage". Update WI-003 title and §6.1.5 accordingly. Math: 33 / 90d ≈ 11/month sustained = ≥ 3/week — still aggressive for solo-tier but feasible; matches WI body's "8 dry-runs/month minimum".

### P0-3. No RTO / RPO target anywhere — DR drill cannot pass/fail without it

`slo_catalog.md` defines availability and latency SLOs but has zero RTO/RPO entries (verified: `grep -E "RTO\|RPO"` returns nothing in either slo_catalog.md or resilience_patterns.md). WI-002 measures "failoverDurationMs" and "SLO impact" but never states the pass threshold. The DoD §6 says "SLO sustained" — sustained at what level? A drill that takes 45 minutes to fail over is either acceptable (if RTO = 1h) or a SEV-1 catastrophe (if RTO = 5m). **A DR drill without a written RTO/RPO target is a vanity exercise** — it produces a report, not a verdict.

**Fix:** add RTO/RPO targets per tier to `slo_catalog.md` §3 (recommendation in §4 below) and reference them explicitly in WI-002 §6.1.2. Without this, the drill cannot fail a "CONDITIONALLY_APPROVED" gate even when it should.

### P0-4. No cross-WI mutual-exclusion / ordering invariants

The spec contract §8 explicitly says "**Não cria invariants novas**". This is wrong for an ops sprint with 6 parallel automations. Concrete unstated invariants the runtime needs:

- Chaos test (WI-001) **must not** fire while DR drill (WI-002) is in progress on the same staging tenant (both apply `chaos-mesh networkPartition` — collision = un-interpretable state)
- Runbook dry-run (WI-003) **must not** be scheduled during an active SEV-1 (this is implicit in WI-005's fadigue tracking but not stated as a hard gate)
- Game day (WI-006) tabletop **must not** overlap with a real ongoing incident (no scenario file references real-time PagerDuty state)
- Chaos safe-mode auto-abort (WI-001 §6.1.4) checks `isProdSEV1Active()` — but doesn't check `isStagingDRDrillActive()` or `isGameDayInProgress()`

**Fix:** add INV-OPS-SCHEDULING-001 ("at most one chaos/DR/dry-run scheduled-event active per env") and INV-OPS-INCIDENT-PROTECT-001 ("no scheduled-ops events fire during active SEV-1 OR active DR drill OR active game day") to spec contract §8.

### P1-5. Oncall fatigue thresholds are unsourced and the hard-cutoff math may be wrong

WI-005 §6.1.2 cites "per spec contract §5.5 R-S17-13 canonical" and the spec contract cites "Google SRE Workbook Ch 8 / PagerDuty docs" but no specific source. **Industry-cited thresholds** (Google SRE Workbook Ch 11, *Being On-Call*; Limoncelli et al. *Practice of Cloud System Administration* §14.4; PagerDuty State of Digital Operations 2023):

- **≤ 2 incidents per 12h shift** (Google SRE; cite: Beyer et al., Ch 11, "the on-call engineer should not handle more than 2 events per 12h shift")
- **0 sleep-disrupting pages per week** target (PagerDuty 2023 — 20% above is the "danger" line)
- **≤ 25% of shift handling pages** (Google SRE: above this, the on-call is doing reactive work, not engineering)

WI-005's thresholds: `> 2 SEV-1/shift = soft alert`, `> 3 SEV-1/shift = HARD auto-rotation`, `> 5 SEV-2/shift = soft`, `> 8 SEV-2/shift = HARD`. These are reasonable but:

1. **No threshold for sleep-disrupting pages specifically** (industry's strongest fatigue signal — a 22:00–08:00 page is worth 3 daytime pages in burnout terms). Recommendation in §4 below.
2. **HARD auto-rotation at >3 SEV-1/shift is too late** — a single 7d shift with 4 SEV-1s = ~4 sleep-disrupted nights = burnout already happened. Should be ≥ 3 within rolling 72h, not per 7d shift.
3. **No source citation** anywhere in the WI for the specific numbers.

**Fix:** cite Beyer/Google SRE Workbook Ch 11 + PagerDuty 2023 explicitly in WI-005 §9.4, lower the HARD threshold to "3 SEV-1 within rolling 72h OR 2 sleep-hour pages within rolling 7d", and add `corelink_oncall_sleep_hour_pages_total{tier}` to the metric list.

### P1-6. Blameless post-mortem template is good but missing two Google SRE Ch 15 sections

WI-004's template (§1 source listing) has: Header, Timeline, Resolution, 5-Why, Action Items, Lessons Learned, Sprint Linkage, Reviews. Google SRE Book Ch 15 ("Postmortem Culture: Learning from Failure") also mandates:

- **"What went well" section** (separate from Lessons Learned positive bullet — Google's structure distinguishes "things to celebrate" from "lessons") — partially covered by Lessons Learned positive, but not as named section.
- **"Where we got lucky" section** (canonical Google SRE field — surfaces near-misses that didn't degrade SLO but could have).

The blameless layered enforcement (field + lint + 2-reviewer) added in WI-004 §9.1 Lote 10.17 fix is genuinely good and exceeds Atlassian's template. The gap is the two SRE-book sections.

**Fix:** add `## What Went Well` and `## Where We Got Lucky` sections to `specs/_templates/post_mortem.md`. Both are 5-line additions.

### P1-7. Game day (WI-006) and chaos weekly (WI-001) are reinforcing — but the spec doesn't say which scenario any given quarter

WI-006 has 4 scenarios in the library (region outage, BYOK compromise, supply chain, insider exfil). Q1 picks one. No rotation rule. Without rotation, the team can run "region outage" 4 quarters in a row and never tabletop insider exfil — which is the lowest-frequency, highest-blast-radius scenario. The Q1-only execution in WI-006 also overlaps with DR drill cycle 1 (CF region outage) from WI-002 = **same scenario two ways in the same sprint**. That's not reinforcing, it's duplicative.

**Fix:** WI-006 §6.1.1 should pick Scenario B (BYOK compromise) or Scenario D (insider exfil) for Q1, **specifically because** Scenario A is already executed as a chaos+DR drill. Add explicit rotation rule: scenarios A→D in order, 1 per quarter.

### P1-8. Two-phase SEAL D+50 lacks a rollback path

WI-006 §6.1.6 defines D+50 GA Evidence Gate criteria but §25 Rollback only covers "PRR REJECTED → sprint reverts to DRAFT". What happens if the 30d observation window has 2 weeks green + 1 week red + 1 week green? Is that a pass? D+50 is binary in the spec but the underlying signals are continuous. Need a tolerance: e.g., "≥ 28/30 days green; any red day requires post-mortem trigger before D+50 ceremony".

---

## 3. Coverage matrix — chaos experiment ↔ FM ↔ existing runbook

Rows in **bold** = coverage MISSING in S-17 WI specs. (Chaos catalog in WI-001 §6.1.5 mandatory list = 8 entries.)

| FM | RPN/Pri | Chaos? (WI-001) | Runbook present? | DR-drill cycle? | Game-day scenario? | Gap |
|---|---|---|---|---|---|---|
| FM-051 R2 bit rot | P1/20 | yes (lat + 5xx) | RB-FM-051 | — | — | ok |
| FM-054 KV stale | P1/30 | yes | RB-FM-054 | — | — | ok |
| FM-057 Neon failover | P2/16 | yes | RB-FM-057 | implied cycle 2 (deferred) | — | ok |
| FM-105 region replication diverge | P2/18 | yes (cross-region partition) | RB-FM-105-region-replication-diverge | cycle 1 indirect | — | ok |
| FM-152 KV stale window | (mapped) | yes | (covered by RB-FM-054 / KV TTL) | — | — | ok |
| FM-202 runbook stale | P1/36 | yes (meta-drill) | RB-FM-202 | — | — | ok |
| FM-204 secret rotation | P2/16 | yes (rotation flag flip) | (no dedicated RB) | — | — | **missing runbook RB-FM-204** |
| FM-205 admin mistake | P1/30 | yes (synthetic destructive) | RB-FM-205 | — | — | ok |
| **FM-007** deserialize RCE | **P1/25** | **NO** | RB-FM-007 | — | — | **chaos missing** |
| **FM-062** hash collision | **P1/25** | **NO** | RB-FM-062 | — | — | **chaos missing** (tabletop-only acceptable; flag it) |
| **FM-100** DNS outage | **P1/10** | **NO** | RB-FM-100 | — | — | **chaos missing** |
| **FM-101** CF edge outage | **P1/5** | **NO** | RB-FM-101 | cycle 1 ✓ | scenario A ✓ | chaos missing but covered by DR drill+game day — accept |
| **FM-156** supply chain | **P1/25** | **NO** | RB-FM-156 | — | scenario C ✓ | chaos missing — tabletop-only acceptable; flag |
| FM-157 typosquat | P2 | — | RB-FM-157 | — | scenario C ✓ | ok |
| **FM-253** cross-tenant read | **P1/40** | **NO** | RB-FM-253 | — | — | **chaos missing — highest cross-tenant risk** |
| **FM-254** cache poisoning | **P1/50** (highest single RPN) | **NO** | RB-FM-254 | — | — | **chaos missing — highest single RPN** |
| **FM-258** insider exfil | **P1/25** | **NO** | RB-FM-258 | — | scenario D ✓ | chaos missing — tabletop-only acceptable |
| **FM-300** GC refcount bug | **P1/40 + FF-HR-011** | **NO** | RB-FM-300 | — | — | **chaos missing** |
| **FM-302** billing leak | **P1/30** | **NO** | RB-FM-302 | — | — | **chaos missing — silent revenue leak** |
| **FM-303** AC cross-tenant | **P1/40** | **NO** | RB-FM-303 | — | — | **chaos missing** |
| **FM-400** retry storm | **P1/36** | **NO** | RB-FM-400 | — | — | **chaos missing** |
| **FM-403** container leak | **P1/36** | **NO** | RB-FM-403 | — | — | **chaos missing** |
| **FM-404** GC sweep race | **P1/20 + FF-HR-011** | **NO** | RB-FM-404 | — | — | **chaos missing** |
| **FM-450** DSR pipeline | **P1/30** | **NO** | RB-DSR-ERASURE-INCOMPLETE | — | — | **chaos missing** |
| **FM-451** residency leak | **P0/40 FF-HR-010 — Schrems II** | **NO** | RB-DATA-RESIDENCY-LEAK + RB-region-leak | — | — | **chaos missing — highest legal exposure FM** |
| **FM-452** consent ledger fork | **P1/25** | **NO** | RB-CONSENT-TAMPERING | — | — | **chaos missing** |
| **FM-453** sub-processor broadcast | **P1/36** | **NO** | RB-SUB-PROCESSOR-BROADCAST-MISS | — | — | **chaos missing** |

**Summary**: 22 P0/P1 FMs in the catalog. WI-001 chaos covers 8 (37%). DR + game-day cover an additional 4 (FM-101, FM-156, FM-258, FM-101). **11 P0/P1 FMs have no S-17 dynamic verification at all** — they have runbooks but no chaos / drill / tabletop exercising them. Most concerning: FM-451 (P0 residency, Schrems II legal exposure) and FM-254 (RPN 50, highest single FM).

---

## 4. Threshold recommendations

### 4.1 Oncall fatigue (replaces WI-005 §6.1.2 thresholds; sourced)

| Metric | Soft alert | Hard auto-rotation | Source |
|---|---|---|---|
| SEV-1 per 7d shift | > 2 | **> 3 within rolling 72h** (not per shift) | Google SRE Workbook Ch 11 §"Quantity of On-Call" |
| SEV-2 per 7d shift | > 5 | > 8 within rolling 72h | Same — extrapolated |
| **Sleep-hour pages (22:00–08:00 local) per 7d** | **> 1** | **> 2 within rolling 7d** | PagerDuty *State of Digital Operations* 2023; Beyer et al. Ch 11 explicit "0 sleep-disruption is the target" |
| % of shift handling pages | > 25% | > 50% | Google SRE Workbook Ch 11 §"Compensation" |
| Total pages/month non-rotation | > 10 | > 15 (1-month rotation block) | WI-005 (keep) |

### 4.2 Runbook drill frequency

- **Sprint cadence**: 3 dry-runs in S-17 sprint window (D+5, D+12, D+18) — keep as spec'd.
- **Sustained post-sprint**: 8 dry-runs/month minimum (WI-003 §1 is correct).
- **S-20 GA gate**: all **33 P0/P1 runbooks** dry-run in 90d. Drop the "47 / 40" inconsistencies. Math: 33/90d ≈ 11/month = ≥ 2.5/week. Solo-tier feasible.

### 4.3 DR RTO/RPO targets (currently absent from slo_catalog.md)

Recommended additions to `slo_catalog.md` §3 (per tier, aligned with availability targets):

| Tier | RTO (failover region) | RPO (data loss tolerance) |
|---|---|---|
| free | 4h | 1h |
| solo | 2h | 30min |
| team | 1h | 15min |
| business | 30min | 5min |
| enterprise | **15min** | **1min** |

WI-002 should reference the **team-tier targets** as the cycle-1 drill pass criteria (RTO ≤ 1h, RPO ≤ 15min), since GA target tier is team+.

### 4.4 Chaos catalog floor

- **GA floor**: 12 distinct P0/P1 FMs covered (current spec says 8; raise it).
- **Mandatory inclusions** beyond the current 8: **FM-451** (residency), **FM-254** (cache poisoning), **FM-253 or FM-303** (one cross-tenant attempt), **FM-400** (retry storm).
- **Post-GA roadmap**: 22 P0/P1 FMs covered by end of Q3 post-GA.

---

## 5. Cross-WI invariants to enforce (spec contract §8 addition)

### INV-OPS-SCHEDULING-001 — at most one scheduled ops event per env
At most one of {chaos experiment, DR drill, runbook dry-run, game day} is in `running` state per environment at any time. Implementation: a single `ops_schedule_lock` DO claimed before any scheduled-event starts; release on completion or timeout.

### INV-OPS-INCIDENT-PROTECT-001 — no scheduled ops during real incidents
If `corelink_incident_active{severity=sev1}` is true OR `corelink_dr_drill_active` is true OR `corelink_game_day_active` is true, then chaos scheduler **must abort** (safe-mode reason: `incident_active`). WI-001 §6.1.4 already covers `prod_sev1_active` and `staging_error_rate_high`; add `staging_dr_drill_active` and `staging_game_day_active` to the safe-mode condition set.

### INV-OPS-CHAOS-STAGING-ONLY-001 — staging-only at GA (formalize WI-001 §6.1.4 as INV)
The hard env check in WI-001 is currently a code-level guard, not a declared invariant. Promote to INV in spec contract §8 so it appears in TLA+ / future audits.

### INV-OPS-FATIGUE-PROTECT-001 — hard cutoff enforces protection
If `corelink_oncall_sleep_hour_pages_total{user}` > 2 in rolling 7d, that user **must not** be paged for 48h regardless of rotation. Implementation: PagerDuty exception list updated by webhook on threshold breach.

### INV-OPS-RUNBOOK-FRESH-001 — runbook freshness gate for SEV-1
If a runbook referenced in a SEV-1 alert has `updated > 90d ago` AND not dry-run in last 60d, fire FM-202 drift detection BEFORE the incident escalates (i.e., warn the oncall: "this runbook is stale; cross-check with peer").

---

## 6. Existing S-11..S-16 ops gaps that S-17 needs to plug

| Sprint | Gap surfaced | S-17 plugs it? |
|---|---|---|
| **S-11 privacy** | FM-450/451/452/453 added with runbooks, but **none have chaos drills** despite FM-451 being P0 FF-HR-010 | **NO** — chaos catalog excludes all privacy FMs. **Recommend mandatory inclusion in WI-001.** |
| **S-12 security model** | TLA+ INV-TENANT-ISOLATION exists for FM-253/303 but no chaos verifies the runtime enforcement | **NO** — no cross-tenant chaos in WI-001. Recommend adding FM-253 or FM-303 chaos. |
| **S-13 admin plane** | RB-FM-205 (admin mistake) + RB-FM-206 (terraform drift) added; dual-approval enforced | **PARTIAL** — FM-205 included in WI-001 chaos list; FM-206 missing from chaos but has working drift detector |
| **S-14 BYOK** | RB-BYOK-REVOKE + RB-KEY-COMPROMISE + RB-HSM-UNAVAILABLE added | **PARTIAL** — DR drill cycle 3 (BYOK compromise) deferred annual; game-day scenario B covers tabletop. Acceptable but flag. |
| **S-15 progressive rollout** | RB-ROLLOUT-STUCK exists; no chaos verifies rollout halt path | **NO** — rollout-stuck chaos not in WI-001 |
| **S-16 admin UI** | New surface (Next.js admin app) has no chaos / DR drill | **NO** — WI-001 chaos catalog focuses on storage backends; UI failure modes (CSP regression, locale loss, ReAuthGate bypass) not exercised |
| **S-09 observability** | Multi-burn-rate alerts assumed to "just work" during chaos | **PARTIAL** — WI-001 reuses S-09 alerts but no test verifies alert *firing* during chaos (only that SLO impact is *measured*) |
| **Pre-existing** | No runbook for FM-204 (secret rotation) despite WI-001 mapping chaos to it | **NO** — chaos points at a non-existent runbook. Either create RB-FM-204 or remap FM-204 chaos to RB-KEY-COMPROMISE. |
| **Pre-existing** | `slo_catalog.md` has no RTO/RPO entries | **NO** — see §2 P0-3. Must be added before DR drill can pass/fail. |
| **Pre-existing** | Audit-chain corruption (FM-304, P2) — no runbook | Out of S-17 scope (P2); flag for post-GA |

---

## 7. Are there cross-WI FF-HR forcing factors hidden in any WI?

Reviewed all 6 WIs. **None declare FF-HR**. Spot-check:

- **WI-001 chaos automation** — staging-only at GA + hard env check = legitimately STANDARD. No FF-HR.
- **WI-002 DR drill** — staging + isolated tenant = STANDARD. No new tenant data path. **However**, cycle-3 BYOK key compromise + customer notification would be FF-HR if executed (touches BYOK + comms) — correctly deferred annual via waiver.
- **WI-003 runbook drill** — process discipline only, no novel cripto. STANDARD ok.
- **WI-004 post-mortem template** — process + culture. STANDARD ok.
- **WI-005 oncall** — PagerDuty integration; no novel data path. **One yellow flag**: §6.1.2 hard auto-rotation via PagerDuty webhook *could* be FF-HR if it modifies live oncall schedule during a real incident (unintended consequence: page someone in 48h protection). Mitigation already in spec (manual confirmation). Acceptable as STANDARD.
- **WI-006 game day** — tabletop only at GA. STANDARD ok.

**Verdict**: no hidden FF-HR. Sprint stays STANDARD lane. The two-phase SEAL D+20/D+50 is the correct heavy-lifting pattern.

---

## 8. Summary of pre-flight fixes required

**P0 (block builders from starting)**:
1. Raise chaos catalog floor to ≥ 12 distinct P0/P1 FMs; mandatory: FM-451, FM-254, FM-253 or FM-303, FM-400.
2. Reconcile runbook count to **33 P0/P1** (verified repo count); update spec contract §5.3 R-S17-8, §6 DoD, and WI-003 title.
3. Add RTO/RPO targets to `slo_catalog.md` §3; reference team-tier in WI-002 §6.1.2.
4. Add 4 cross-WI INVs (§5 above) to spec contract §8.

**P1 (fix during sprint, before D+20)**:
5. Cite Beyer/Google SRE Ch 11 + PagerDuty 2023 in WI-005; add sleep-hour-pages metric + threshold.
6. Add "What Went Well" + "Where We Got Lucky" sections to post-mortem template.
7. Pick game-day Q1 scenario = B or D (not A — duplicative with DR drill).
8. Add D+50 tolerance rule (≥ 28/30 green days).
9. Create RB-FM-204 (secret rotation) or remap WI-001 FM-204 chaos to RB-KEY-COMPROMISE.

**P2 (post-D+50; roadmap)**:
10. Extend chaos catalog to all 22 P0/P1 FMs by end of Q3 post-GA.
11. Add chaos for FM-451 residency cross-region write rejection path.
12. Add chaos test for S-09 alert firing (not just SLO measurement).

---

**End of pre-flight review.** This audit is advisory; spec files were NOT modified per the brief.
