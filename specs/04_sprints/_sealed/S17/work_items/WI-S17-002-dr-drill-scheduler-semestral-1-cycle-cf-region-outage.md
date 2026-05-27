---
id: "WI-S17-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-17"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
tags: ["wi", "s17", "ops", "dr-drill", "semestral", "cf-region-outage", "failover", "slo-impact", "7y-archive", "standard"]
---

# WI-S17-002 — DR Drill Scheduler + Semestral Cadence Calendar (Every 6 Months; Cycle 1 = Simulate CF Region Outage in Staging + Failover to Secondary Region + SLO Sustained Measured; Cycle 2 = Simulate D1 Primary Loss + Restore from Backup; Cycle 3 = Simulate BYOK Key Compromise + Crypto-erase + Customer Notification — Cycles 2/3 Deferred Annual at GA via Waiver Opt) + 1 Cycle Completed em Staging (Cycle 1 CF Region Outage; Full Execution; Full Report) + DR Drill Report Includes Pre-drill State + Drill Timeline + SLO Impact Measured + Lessons Learned + Runbook Updates Needed (Archived 7y for Compliance per Quality Standard 14.s17.7) + Drill em Staging Only + Isolated Tenant for Chaos (Anti-prod Hit Guarantee) + SRE Lead Facilitates Drill Execution + Report Committed em `specs/_audits/2026-XX-XX-dr-drill-cycle-1.md`

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-17](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S17-002 |
| Título | DR drill tooling + semestral cadence calendar + 1 cycle completed (CF region outage simulate + failover + SLO sustained + 7y archive). |
| Sprint | S-17 |
| Lane | STANDARD |
| Forcing factors | none (DR drill staging-only at GA; isolated tenant; consume failover patterns já em S-09; não introduz novo path tenant data flow) |

## 1. Intent

DR drill tooling + semestral cadence calendar + 1 cycle completed em staging (cycle 1 = simulate CF region outage + failover to secondary region + SLO sustained measured; full execution + full report archived 7y for compliance). Cycle 2 (D1 primary loss + restore from backup) e cycle 3 (BYOK key compromise + crypto-erase + customer notification) deferred annual at GA via waiver opt (per spec contract §19 waiver policy).

```typescript
// File: infra/dr_drills/cycle_1_cf_region_outage.ts
export async function executeCycle1() {
  // **Lote 10.17 codex P0 canonical fix — staging-only enforcement code-level guard FIRST**
  // Prior version had "env=staging enforced" como Gherkin assertion only; no code-level check
  // = bypassable. Canonical: explicit env check é primeira linha; throws immediately se env != staging.
  if (process.env.CHAOS_TARGET !== 'staging' && process.env.DRILL_TARGET !== 'staging') {
    throw new Error(
      `FATAL: DR drill cycle 1 attempted in non-staging env (CHAOS_TARGET=${process.env.CHAOS_TARGET}, DRILL_TARGET=${process.env.DRILL_TARGET}). ` +
      `Staging-only at GA per CTRL-CHAOS-001 + safe-mode hard rule. CRITICAL post-mortem trigger.`
    );
  }
  if (process.env.NODE_ENV === 'production') {
    throw new Error('FATAL: DR drill cycle 1 attempted in production. Hard-fail.');
  }

  const drill_id = `dr-drill-cycle-1-${Date.now()}`;
  const stateBefore = await captureFullState({ tenant: 'isolated_chaos_tenant' });

  // Simulate CF region outage staging (gated by env check above)
  await chaosMesh.applyNetworkPartition({
    target: 'edge-staging-region-1',
    duration_s: 3600  // 1 hour drill window
  });

  const failoverStartTs = Date.now();
  await waitForFailover({ targetRegion: 'edge-staging-region-2' });
  const failoverEndTs = Date.now();
  const failoverDurationMs = failoverEndTs - failoverStartTs;

  // SLO sustained measured during failover
  const sloImpact = await measureSLOImpact(stateBefore, failoverStartTs, failoverEndTs);

  await chaosMesh.removeNetworkPartition('edge-staging-region-1');
  const stateAfter = await captureFullState({ tenant: 'isolated_chaos_tenant' });

  const report = {
    drill_id,
    cycle: 1,
    scenario: 'cf_region_outage_simulate',
    pre_state: stateBefore,
    drill_timeline: { failoverStartTs, failoverEndTs, failoverDurationMs },
    slo_impact: sloImpact,
    post_state: stateAfter,
    lessons_learned: [],  // populated post-drill
    runbook_updates_needed: [],  // populated post-drill
    retention_years: 7
  };
  await archiveDRDrillReport(report);
  return report;
}
```

## 2. Narrative

DR drill semestral cadence calendar (every 6 months sustained pós-GA per spec contract §5.2 R-S17-5). Cycle 1 (CF region outage simulate) executed em S-17 sprint timeline em staging (isolated tenant for chaos; anti-prod hit guarantee). Cycles 2/3 deferred annual at GA via waiver opt + ADR (per spec contract §19; SRE lead approves).

Report 7y archive for compliance baseline (SOC 2 + ISO 27001; per Quality Standard 14.s17.7) com: pre-drill state, drill timeline, SLO impact measured, lessons learned, runbook updates needed.

**Risk justification STANDARD lane**:
- DR drill staging-only at GA + isolated tenant = well-bounded surface.
- Reuse failover patterns já em S-09 SEALED (multi-burn-rate alerts + audit events R2 reuse for SLO impact measurement).
- Não introduz novo path tenant data flow.
- 7y archive compliance baseline (SOC 2 + ISO 27001).

## 3. Customer Impact & Journey

**Persona — SRE / Compliance auditor**:
- DR drill 1 cycle completed em staging com SLO impact measurement = recovery capability validated.
- Report archived 7y = compliance baseline (SOC 2 + ISO 27001).
- Lessons learned + runbook updates feed FM-202 mitigation cycle.
- Quarterly cadence start sustained pós-GA = continuous improvement baseline.

## 4. Capability Mapping

- **CAP-OPS-002** (DR drill scheduler) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + `slo_catalog.md` + `failure_modes.md`.

## 5. Tipo

DR drill execution WI; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **DR drill scheduler tooling** em `infra/dr_drills/`:
   - Cycle 1 script `cycle_1_cf_region_outage.ts` (simulate CF region outage + failover + SLO measured).
   - Cycle 2 script `cycle_2_d1_primary_loss.ts` (deferred annual at GA via waiver opt; placeholder).
   - Cycle 3 script `cycle_3_byok_compromise.ts` (deferred annual at GA via waiver opt; placeholder).
   - Semestral cadence calendar published em `specs/04_sprints/_sealed/S17/dr_drill_cadence.md`.

2. **1 cycle completed em staging** (cycle 1):
   - Simulate CF region outage staging (chaos-mesh network partition reuse from WI-S17-001).
   - Failover to secondary region (edge-staging-region-2).
   - SLO sustained measured (multi-burn-rate alerts S-09 reuse; SLO error budget consumed).
   - Isolated tenant for chaos (`isolated_chaos_tenant`; anti-prod hit guarantee).
   - SRE lead facilitates drill execution.

3. **DR drill report includes**:
   - Pre-drill state (full state capture).
   - Drill timeline (events ts + actor).
   - SLO impact measured (numerical; vs target).
   - Lessons learned (positive + negative).
   - Runbook updates needed (FM-202 mitigation cycle feed).
   - Archived 7y for compliance.

4. **Report committed** em `specs/_audits/2026-XX-XX-dr-drill-cycle-1.md`:
   - Full report markdown + R2 archive (immutable).
   - SRE lead + Final Approver review + sign.

5. **Métricas Prometheus** snake_case canonical:
   - `corelink_dr_drill_total{cycle, outcome}` (counter).
   - `corelink_dr_drill_slo_impact_seconds{cycle, slo}` (gauge).

### 6.2 Out-of-scope (deferred)

- Cycle 2 D1 primary loss (deferred annual at GA via waiver opt; placeholder script).
- Cycle 3 BYOK key compromise (deferred annual at GA via waiver opt; reuses S-14 BYOK + crypto-erase patterns).
- DR drill em prod (anti-scope estrito; staging-only at GA).
- Customer-facing DR drill notification (pós-GA enterprise).

## 7. Anti-Scope

- Skip DR drill cycle 1 (mandatory ship gate per DoD §6).
- DR drill em prod (anti-scope estrito).
- Skip 7y archive (compliance fail).
- DR drill without SRE lead facilitation.
- DR drill without SLO impact measurement.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: DR drill scheduler + 1 cycle completed

  Scenario: DR drill cycle 1 CF region outage simulate
    Given infra/dr_drills/cycle_1_cf_region_outage.ts script
    When executed em staging
    Then chaos-mesh network partition applied
    And failover to secondary region observed
    And SLO sustained measured
    And state captured pre/post

  Scenario: DR drill report 7y archive
    Given cycle 1 completed
    When report archived
    Then includes pre-drill state + drill timeline + SLO impact + lessons + runbook updates
    And retention_years=7
    And immutable em R2

  Scenario: DR drill em staging only + isolated tenant
    Given cycle 1 execution
    When chaos applied
    Then env=staging enforced
    And tenant=isolated_chaos_tenant enforced
    And no prod hit

  Scenario: SRE lead facilitates drill
    Given cycle 1 execution
    When drill begins
    Then SRE lead identified em report header
    And drill timeline events attributed

  Scenario: Lessons learned + runbook updates feed FM-202 cycle
    Given cycle 1 report
    When lessons captured
    Then runbook updates needed listed
    And FM-202 mitigation cycle fed (input para WI-S17-003 dry-run cadence)

  Scenario: Cycles 2/3 deferred annual via waiver
    Given cycle 2 + cycle 3 placeholder scripts
    When cycle execution scheduled
    Then waiver applied (annual at GA)
    And ADR committed
    And expiry next sprint review

  Scenario: Métricas Prometheus snake_case
    Given cycle 1 completed
    When métricas emitted
    Then corelink_dr_drill_total counter incremented
    And corelink_dr_drill_slo_impact_seconds gauge set
    And NUNCA per-tenant labels
```

## 9. Design Decisions

### 9.1 Why semestral cadence (não quarterly)

- Semestral = balance between drill frequency + ops cost.
- Quarterly = ideal but requires SRE team headcount (anti-scope per spec contract §10).
- Pós-GA roadmap: increase to quarterly cadence post-S-Y.

### 9.2 Why cycle 1 = CF region outage (não cycle 2 D1 loss ou cycle 3 BYOK)

- CF region outage = highest-frequency real-world risk (P0 per FM-101).
- D1 primary loss = lower frequency (Neon backup baseline).
- BYOK key compromise = lowest frequency (CRITICAL but rare; reuses S-14 patterns).
- Cycle 1 first = production confidence baseline.

### 9.3 Why 7y archive (não 1y)

- SOC 2 + ISO 27001 baseline 7y retention.
- Per Quality Standard 14.s17.7 canonical.

### 9.4 Why isolated tenant for chaos (não shared staging tenant)

- Isolated tenant = anti-prod hit guarantee (cross-tenant safety).
- Shared staging tenant = risk dev cycle interruption.
- Per spec contract §15 row 1 mitigation.

### 9.5 ADR potencial?

- Sim — ADR para cycles 2/3 deferred annual at GA (waiver opt + expiry).
- ADR committed em `specs/_decisions/ADR-XXXX-dr-drill-cycles-2-3-deferred.md`.

## 10. Completeness Criteria

- [ ] **10.s17.002.1** DR drill tooling em `infra/dr_drills/` committed.
- [ ] **10.s17.002.2** Cycle 1 script + execution em staging completed (EVT-023).
- [ ] **10.s17.002.3** SLO sustained measured (S-09 multi-burn-rate alerts reuse).
- [ ] **10.s17.002.4** Isolated tenant for chaos (cross-tenant safety).
- [ ] **10.s17.002.5** SRE lead facilitates drill execution.
- [ ] **10.s17.002.6** Report archived 7y compliance (EVT-017 + EVT-023).
- [ ] **10.s17.002.7** Lessons learned + runbook updates documented.
- [ ] **10.s17.002.8** Semestral cadence calendar published.
- [ ] **10.s17.002.9** Cycles 2/3 deferred annual via waiver opt + ADR.
- [ ] **10.s17.002.10** Métricas snake_case Prometheus emitting.

## 11. DoD

- [ ] DR drill tooling committed.
- [ ] Cycle 1 executed em staging (full execution; full report).
- [ ] Report committed + 7y archive.
- [ ] Semestral cadence calendar published.
- [ ] Cycles 2/3 deferred annual ADR committed.
- [ ] Métricas emitting.
- [ ] Adversarial scenarios 4+ documented.

## 12. Invariants Validated

- **PAT-DEGRADE-001** (chaos test partial degradation): DR drill simula partial degradation com graceful failover expected.
- **CTRL-PRIV-001** (zero PII em logs) em DR drill report sanitized.
- **Não introduz INVs novas** (sprint operational; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DR drill cycle 1 script | `infra/dr_drills/cycle_1_cf_region_outage.ts` | TypeScript |
| DR drill cycles 2/3 placeholders | `infra/dr_drills/cycle_2_*.ts`, `cycle_3_*.ts` | TypeScript |
| Semestral cadence calendar | `specs/04_sprints/_sealed/S17/dr_drill_cadence.md` | Markdown |
| Cycle 1 report | `specs/_audits/2026-XX-XX-dr-drill-cycle-1.md` | Markdown |
| ADR cycles 2/3 deferred | `specs/_decisions/ADR-XXXX-dr-drill-cycles-2-3-deferred.md` | Markdown |
| Cycle 1 archive | R2 `evidence-dr-drills/cycle-1.json` (7y retention) | JSON |

## 14. Quality Standards

- **14.s17.002.1** DR drill report quality: includes pre-state + timeline + SLO impact + lessons + runbook updates; archived 7y (per Quality Standard 14.s17.7).
- **14.s17.002.2** DR drill em staging only + isolated tenant (anti-prod hit guarantee).
- **14.s17.002.3** SRE lead facilitates drill execution (canonical).
- **14.s17.002.4** Cost regression gate: DR drill infra ≤ $50/mês.

## 15. Test Plan

### Cycle 1 dry-run preview
- Pre-execute em test env (isolated tenant) to validate failover patterns.
- Verify SLO measurement instrumentation correct.

### Cycle 1 execution em staging
- Full execution per script.
- State captured pre/post.
- SLO impact measured.
- Failover duration measured.

### Adversarial scenarios (4+)
1. DR drill cycle 1 encontra unrecoverable failure (per spec contract §15 row 4): drill em staging only + isolated tenant + report findings → fix em sprint subsequente.
2. DR drill report omits lessons learned (Quality gap; PR review reinforce).
3. DR drill cycle 1 SLO measurement instrumentation broken (S-09 multi-burn-rate alerts reuse fails); pre-execute dry-run preview catches.
4. DR drill cycle 1 chaos hits prod inadvertent (CRITICAL; HARD env check from WI-S17-001 catches; alert + abort).

### Verification
- Verify cycle 1 report committed + 7y archive immutable.
- Verify semestral cadence calendar published.
- Verify cycles 2/3 deferred ADR committed.

## 16. Failure Modes

- **FM-101** (CF edge outage): cycle 1 simulates this directly.
- **FM-105** (region replication diverge): cycle 1 indirectly tests cross-region failover.
- **FM-202** (runbook stale): DR drill report runbook updates feed mitigation cycle (input para WI-S17-003).
- **DR drill encontra unrecoverable failure** (per spec contract §15 row 4): drill em staging + isolated tenant + report findings → fix sprint subsequente.

## 17. Controls

- **CTRL-PRIV-001** (zero PII em DR drill report sanitized).
- Reuse S-14 BYOK + crypto-erase patterns (cycle 3 deferred).

## 18. Resilience Patterns

- **PAT-DEGRADE-001** chaos test partial degradation reflected.
- Failover pattern (CF region failover; reuse S-09 multi-burn-rate alerts).
- 7y archive (compliance pattern).

## 19. Observability

`corelink_dr_drill_*` métricas Prometheus snake_case:
- `corelink_dr_drill_total{cycle, outcome}` (counter; alert se outcome=fail).
- `corelink_dr_drill_slo_impact_seconds{cycle, slo}` (gauge; review per cycle).
- Alert se semestral cadence missed (cycle skipped).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: DR drill chaos config em admin plane S-13 reuse requires admin auth.
- **Tampering**: DR drill report git-tracked + 7y archive immutable em R2.
- **Repudiation**: EVT-023 (CHAOS_EXPERIMENT_REPORT) + EVT-017 (RUNBOOK_EXECUTION) audit trail.
- **Information disclosure**: DR drill report sanitized; CTRL-PRIV-001 enforced; no PII em report.
- **DoS**: drill em staging only + isolated tenant (anti-prod hit).
- **Elevation of privilege**: DR drill chaos requires admin auth (S-13 reuse).

**LINDDUN delta**:
- Linkability: DR drill métricas tenant-agnostic.
- Identifiability: DR drill report sanitized (aggregate metrics).
- Non-repudiation: EVT-017 + EVT-023 forensic-grade.
- Detectability: DR drill alerts + métricas.
- Disclosure: DR drill scope-limited admin S-13 reuse.

## 21. Dependencies

### Hard blockers
- WI-S17-001 SEALED (chaos automation tooling reuse for network partition).
- S-09 SEALED (multi-burn-rate alerts for SLO measurement).

### Soft blockers
- S-14 SEALED (BYOK setup; only required for cycle 3 deferred annual).

### Outbound
- WI-S17-003 (runbook dry-runs feed lessons learned cycle).
- WI-S17-006 (game day quarterly cadence consume DR drill patterns).

## 22. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; DR drill tooling + cycle 1 execution + report + cadence calendar).

## 23. Cost Analysis

**Direct cost**:
- Chaos-mesh reuse: $0/mês.
- R2 7y archive cycle 1: ~$2/mês.

**Total**: ~$2/mês.

**Indirect cost**: Recovery capability validated + 7y archive compliance baseline = priceless.

## 24. Post-mortem Hooks

- DR drill fails recovery → 5-Why mandatório + runbook updates required.
- DR drill cycle 1 chaos hits prod inadvertent → CRITICAL post-mortem.
- DR drill report omits lessons → quality review.

## 25. Rollback / Recovery

DR drill rollback: chaos-mesh remove network partition + state restore from pre-drill capture. RTO ≤ 30min.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | DR drill encontra unrecoverable failure | L | M | HIGH (delay GA) | L | LOW | Drill em staging only + isolated tenant + report findings → fix em sprint subsequente |
| R-002 | DR drill cycle 1 chaos hits prod inadvertent | L | H | CRITICAL | M | LOW | HARD env check (WI-S17-001 reuse) + isolated tenant |
| R-003 | DR drill report omits lessons learned | M | M | LOW | M | LOW | Template enforcement + SRE lead review |
| R-004 | Cycles 2/3 deferred sin ADR (compliance gap) | M | L | LOW | L | LOW | ADR mandatory + expiry next sprint review |

## 27. Knowledge Transfer

- Tech talk (1h): "DR drill em CoreLink — semestral cadence + cycle 1 CF region outage walkthrough".
- Doc `docs/internal/s17-dr-drill-runbook.md` — DR drill execution procedure.
- Onboarding test (3 questions): DR drill cadence + isolated tenant rationale + 7y archive compliance baseline.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

**Staffing reality (per ADR-0034 solo-tier)**:

| Status atual (2026-04-29) | Roles |
|---|---|
| **Confirmed (3)** | Owner; Final Approver; Engineer (Gustavo Schneiter — solo founder) |
| **Pending Tier-1 hire/contract (4)** | SRE Lead, Oncall Manager, QA, Compliance officer |

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _TBD; emphatic — DR drill facilitation + cadence ownership_ | _pending_ | _pending_ |
| 4 | Engineer | Gustavo Schneiter | _pending_ | _pending_ |
| 5 | Oncall Manager | _TBD_ | _pending_ | _pending_ |
| 6 | QA | _TBD_ | _pending_ | _pending_ |
| 7 | Compliance officer | _TBD; emphatic — 7y archive + audit baseline_ | _pending_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S17-002 (cycle 12.S17.0; STANDARD lane; DR drill scheduler + cycle 1 CF region outage + semestral cadence + 7y archive). |

## 30. Anti-patterns evitados

- Skip DR drill cycle 1 (mandatory ship gate).
- DR drill em prod (anti-scope estrito).
- Skip 7y archive (compliance fail).
- DR drill without SRE lead facilitation.
- DR drill without SLO impact measurement.
- Cycles 2/3 deferred sin ADR (compliance gap).

---

**Fim WI-S17-002.**
