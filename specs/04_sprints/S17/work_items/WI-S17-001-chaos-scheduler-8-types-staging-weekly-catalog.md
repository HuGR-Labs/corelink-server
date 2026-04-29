---
id: "WI-S17-001"
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
tags: ["wi", "s17", "ops", "chaos-engineering", "chaos-automation", "chaos-catalog", "deterministic-seed", "safe-mode", "staging", "standard"]
---

# WI-S17-001 — Chaos Scheduler + 8 Chaos Types Staging Weekly (Latency Injection R2 GET +500ms / D1 Query +200ms / Neon Query +300ms / KV +100ms; Failure Injection R2 5xx 1% / D1 Timeout 0.5% / Neon Connection Drop 0.1%; Resource Exhaustion DO Storage Near Limit / KV Quota Near Limit; Network Partition Edge-to-Origin 10s / Cross-region 30s) + Deterministic Seed Reproducible (State Captured Pre/Post + Seed em `chaos_run_state.json`; SLO Impact Measured) + Safe-mode Auto-abort (Chaos Test Halts se Prod SEV-1 OR Staging Error Rate > 50%; Hard Env Check Before Chaos via `process.env.CHAOS_TARGET === 'staging'` Enforcement at Code Level; Alert on Any Prod Hit = CRITICAL Post-mortem Trigger) + Chaos Catalog em `specs/05_quality/chaos/<experiment>.md` per FM Coverage ≥ 8 FMs (Each Chaos Type Documented com FM Mapping + Reproducible Seed + Safe-mode Threshold + Reviewer SRE) + Chaos Automation Tooling Integration (Gremlin / chaos-mesh / Litmus Open-source; CF Workers Cron Scheduler Weekly Orchestration) + Chaos Test Reproducibility Verified (Deterministic Seed + State Captured + 7y Archive for Compliance per Quality Standard 14.s17.1)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-17](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S17-001 |
| Título | Chaos automation framework + 8 chaos types staging weekly + chaos catalog ≥ 8 FMs + safe-mode auto-abort + deterministic seed reproducible. |
| Sprint | S-17 |
| Lane | STANDARD |
| Forcing factors | none (chaos staging-only at GA; well-bounded surface — env check before chaos at code level; consume SLO/observability já validated em S-09; não introduz novo path tenant data flow) |

## 1. Intent

Foundation WI do S-17. Entrega o **chaos engineering automation framework** em staging com 8 chaos types canonical (latency + failure + resource exhaustion + network partition), deterministic seed reproducible (state captured pre/post + seed em `chaos_run_state.json`), safe-mode auto-abort (env check `CHAOS_TARGET === 'staging'` at code level + alert on prod hit = CRITICAL trigger), chaos catalog ≥ 8 FMs documented em `specs/05_quality/chaos/<experiment>.md`, tooling integration (Gremlin / chaos-mesh / Litmus open-source), CF Workers cron scheduler weekly orchestrator. Foundation layer para WI-S17-002..006.

```typescript
// File: infra/chaos/scheduler.ts (CF Worker cron)
const CHAOS_EXPERIMENTS = [
  { id: 'lat-r2-get', fm: 'FM-150', kind: 'latency', target: 'r2', delay_ms: 500 },
  { id: 'lat-d1-query', fm: 'FM-150', kind: 'latency', target: 'd1', delay_ms: 200 },
  { id: 'lat-neon-query', fm: 'FM-150', kind: 'latency', target: 'neon', delay_ms: 300 },
  { id: 'lat-kv', fm: 'FM-150', kind: 'latency', target: 'kv', delay_ms: 100 },
  { id: 'fail-r2-5xx', fm: 'FM-051', kind: 'failure', target: 'r2', rate: 0.01 },
  { id: 'fail-d1-timeout', fm: 'FM-057', kind: 'failure', target: 'd1', rate: 0.005 },
  { id: 'fail-neon-drop', fm: 'FM-057', kind: 'failure', target: 'neon', rate: 0.001 },
  { id: 'res-do-storage', fm: 'FM-059', kind: 'resource_exhaustion', target: 'do', threshold: 0.95 },
  // 9th adicional opt: { id: 'res-kv-quota', fm: 'FM-059', kind: 'resource_exhaustion', target: 'kv', threshold: 0.95 }
  // 10th adicional opt: { id: 'net-edge-origin', fm: 'FM-101', kind: 'network_partition', target: 'edge_to_origin', duration_s: 10 }
  // 11th adicional opt: { id: 'net-cross-region', fm: 'FM-105', kind: 'network_partition', target: 'cross_region', duration_s: 30 }
] as const;

export async function runChaosExperiment(experimentId: string, seed: string) {
  // SAFE-MODE HARD ENFORCEMENT (must be FIRST line)
  if (process.env.CHAOS_TARGET !== 'staging') {
    await alertCritical('CHAOS_PROD_HIT_ATTEMPTED', { experimentId, env: process.env.CHAOS_TARGET });
    throw new Error('CHAOS_SAFE_MODE_ABORT: chaos prohibited outside staging');
  }
  if (await isProdSEV1Active()) {
    await alertCritical('CHAOS_ABORTED_PROD_SEV1', { experimentId });
    return { aborted: true, reason: 'prod_sev1_active' };
  }
  if (await getStagingErrorRate() > 0.50) {
    await alertCritical('CHAOS_ABORTED_STAGING_ERROR_RATE_HIGH', { experimentId });
    return { aborted: true, reason: 'staging_error_rate_high' };
  }

  // Deterministic seed reproducible
  const stateBefore = await captureState();
  const result = await executeExperiment(experimentId, seed);
  const stateAfter = await captureState();
  const sloImpact = await measureSLOImpact(stateBefore, stateAfter);

  await archiveReport({ experimentId, seed, stateBefore, stateAfter, sloImpact, retention_years: 7 });
  return { passed: result.passed, sloImpact };
}
```

## 2. Narrative

Netflix Chaos Engineering Principles + Google SRE Workbook Ch 12 são canonical references. CoreLink S-17 entrega chaos automation production-grade com staging-only at GA enforcement (chaos em prod é anti-scope estrito per spec contract §10; hard rule). Tooling open-source: Gremlin SaaS (free tier 8 experiments) ou chaos-mesh (CNCF; free; full control) ou Litmus (CNCF; free); decisão runtime via tooling spike. CF Workers cron scheduler weekly orchestrator (CF native; free tier baseline).

Chaos catalog em `specs/05_quality/chaos/<experiment>.md` per FM coverage ≥ 8 FMs documented com:
- FM mapping (which FMs from `failure_modes.md` this chaos covers)
- Reproducible seed (deterministic; runtime injects seed)
- Safe-mode threshold (chaos halts conditions)
- Reviewer SRE
- Expected SLO impact

**Risk justification STANDARD lane (zero forcing factors)**:
- Chaos staging-only at GA — hard env check before chaos at code level + alert on prod hit = well-bounded surface.
- Não introduz tenant data path novo; consume SLO/observability já validated em S-09.
- Chaos automation tooling open-source (Gremlin / chaos-mesh / Litmus); never custom from scratch.
- Deterministic seed reproducible + state captured pre/post + 7y archive para compliance.

## 3. Customer Impact & Journey

**Persona — SRE / Internal Engineer**:
- Chaos automation weekly em staging por 4 semanas pré-GA = production confidence promote GA.
- Deterministic seed reproducible = chaos runs reproducible for debugging.
- Safe-mode auto-abort = zero tolerance for prod hit; CRITICAL post-mortem trigger.
- Chaos catalog ≥ 8 FMs covered = systematic FM coverage baseline.

## 4. Capability Mapping

- **CAP-OPS-001** (chaos engineering automation) — IMPLEMENTA primary.
- **CAP-OPS-006** (chaos catalog + taxonomy) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + `failure_modes.md §3`.

## 5. Tipo

Foundation WI; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **Chaos automation tooling integration** em `infra/chaos/`:
   - Tooling spike + decision: Gremlin / chaos-mesh / Litmus (open-source preferred; ADR committed).
   - CF Workers cron scheduler weekly orchestrator (`infra/chaos/scheduler.ts`).
   - Configuration via `chaos_config.json` (experiment IDs + seeds + safe-mode thresholds).

2. **8 chaos types canonical** scripted:
   - **Latency injection**: R2 GET +500ms, D1 query +200ms, Neon query +300ms, KV +100ms.
   - **Failure injection**: R2 5xx 1%, D1 timeout 0.5%, Neon connection drop 0.1%.
   - **Resource exhaustion**: DO storage near limit (≥ 95%), KV quota near limit (≥ 95%).
   - **Network partition**: edge-to-origin 10s, cross-region 30s (additional opt; 8 baseline).

3. **Deterministic seed reproducible (Lote 10.17 codex P2 canonical strengthening — full archive manifest)**:
   - Seed input per experiment run (deterministic; PRNG state captured).
   - **Full archive manifest** em `chaos_run_state.json` (Lote 10.17 codex P2 fix; prior version archived seed + state but missing tool/version/config/runtime — codex P2 finding "7y reproducible underspecified"):
     - `seed`: deterministic PRNG seed (uint64).
     - `state_pre`: R2 + D1 + KV + DO state snapshot pre-chaos (hash digest + content reference).
     - `state_post`: same shape as state_pre.
     - **`tool_name`**: chaos tool identifier (e.g., "chaos-mesh", "gremlin", "litmus").
     - **`tool_version`**: exact tool version (semver + git commit if applicable; e.g., "chaos-mesh@2.6.3+git@abc123").
     - **`config_hash`**: SHA-256 hash of `chaos_config.json` content at run time (deterministic config).
     - **`runtime_env`**: Node.js + tooling runtime + OS kernel version (e.g., `{node: "20.10.0", os: "linux-6.5.0", arch: "x86_64"}`).
     - `chaos_type_id` + `fm_id` mapping.
     - `started_ts` + `completed_ts` + `duration_seconds`.
     - `slo_impact_metrics` (multi-burn-rate measurements during run).
   - SLO impact measured via existing multi-burn-rate alerts (S-09 SEALED reuse).
   - **Reproducibility verified — exact replay (Lote 10.17 codex P2 canonical fix; "equivalent within tolerance" rejected)**: same seed + same tool_version + same config_hash + same runtime_env = **bit-identical state_post hash** (CI gate via `chaos_replay_check.py` runs sample experiment; expected output hash em `chaos_replay_oracles.json`; deviation = PR fail).
   - 7y archive em R2 audit bucket (compliance retention).

4. **Safe-mode auto-abort**:
   - **HARD env check BEFORE chaos**: `process.env.CHAOS_TARGET === 'staging'` enforced at code level (first line of runChaosExperiment).
   - Alert on any prod hit = CRITICAL post-mortem trigger (zero tolerance).
   - Chaos test halts se prod SEV-1 active.
   - Chaos test halts se staging error rate > 50%.
   - Audit log every safe-mode abort.

5. **Chaos catalog ≥ 8 distinct FMs covered (Lote 10.17 codex P1 canonical fix)** em `specs/05_quality/chaos/`:
   - **Canonical scope**: 8 distinct **FM-IDs** (não markdown files count) covered cumulatively. Markdown file count pode ≥ 8 mas distinct FM ID coverage é o gate.
   - **Mandatory 8 FMs** (canonical list — Lote 10.17 codex P1 alignment per failure_modes.md inventory):
     - **FM-051** (R2 bit rot) — chaos: latency R2 GET +500ms + R2 5xx 1%.
     - **FM-057** (Neon failover) — chaos: D1 timeout 0.5% + Neon connection drop 0.1% + Neon query +300ms.
     - **FM-152** (KV stale window) — chaos: KV +100ms + KV quota near limit.
     - **FM-204** (secret rotation in-flight) — chaos: rotation flag flip mid-traffic + key swap timing.
     - **FM-105** (region replication diverge) — chaos: cross-region partition 30s.
     - **FM-054** (KV global leak) — chaos: KV namespace mis-routing inject.
     - **FM-205** (admin mistake) — chaos: synthetic destructive op blocked (verifies dual-approval).
     - **FM-202** (runbook stale) — chaos: meta-drill triggered by drift detector.
   - Each FM em catalog documented com: chaos_type_id, FM_id mapping, reproducible seed, safe-mode threshold, reviewer SRE, expected SLO impact.
   - **AC gate**: PR de novo chaos type requer **distinct FM-id mapping** + chaos catalog entry + safe-mode threshold + reviewer SRE per Quality Standard 14.s17.6 — NÃO basta criar 8 markdown files referencing same FM.
   - CI check: `validate_chaos_catalog.py` verifies ≥ 8 distinct `fm_id` fields across catalog em paths `specs/05_quality/chaos/*.md`.

6. **Métricas Prometheus** snake_case canonical:
   - `corelink_chaos_run_total{experiment, outcome, env}` (counter).
   - `corelink_chaos_safe_mode_abort_total{trigger, env}` (counter; trigger ∈ prod_sev1|staging_error_rate_high|prod_target_violation).

### 6.2 Out-of-scope (deferred)

- Chaos in prod GA (anti-scope estrito; hard rule per spec contract §10).
- Custom chaos framework (use Gremlin / chaos-mesh / Litmus open-source).
- Chaos in customer environments (chaos-as-a-service; pós-GA enterprise).
- AI-powered chaos generation (pós-GA Q1+).
- Visual regression chaos testing (pós-GA Q1).

## 7. Anti-Scope

- Skip safe-mode auto-abort (CRITICAL gap).
- Skip deterministic seed reproducible (chaos runs not reproducible = compliance fail).
- Skip 7y archive (compliance fail).
- Custom chaos framework (use open-source).
- Chaos config flag without admin auth (S-13 admin plane reuse mandatory).
- Chaos catalog entry without reviewer SRE (drift baseline).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Chaos automation framework

  Scenario: Chaos scheduler weekly em staging
    Given CF Workers cron scheduler configured
    When weekly cron fires
    Then chaos experiment selected per rotation
    And experiment executed em staging
    And state captured pre/post + seed
    And SLO impact measured
    And report archived 7y

  Scenario: Safe-mode auto-abort prod hit (CRITICAL)
    Given process.env.CHAOS_TARGET === 'production' (misconfig)
    When runChaosExperiment invoked
    Then HARD env check fails imediato
    And alert CHAOS_PROD_HIT_ATTEMPTED fires (CRITICAL)
    And chaos test aborted
    And post-mortem trigger fires

  Scenario: Safe-mode auto-abort prod SEV-1
    Given prod SEV-1 active
    When chaos cron fires
    Then chaos test aborted reason=prod_sev1_active
    And alert CHAOS_ABORTED_PROD_SEV1 fires

  Scenario: Safe-mode auto-abort staging error rate high
    Given staging error rate > 50%
    When chaos cron fires
    Then chaos test aborted reason=staging_error_rate_high
    And alert fires

  Scenario: Deterministic seed reproducibility
    Given chaos experiment X com seed S
    When run twice (different ts)
    Then results equivalent within tolerance
    And state captured pre/post both runs identical baseline

  Scenario: Chaos catalog ≥ 8 FMs covered
    Given specs/05_quality/chaos/ directory
    When inventory taken
    Then ≥ 8 markdown files (per chaos type)
    And each documented com FM mapping + seed + safe-mode threshold + reviewer SRE
    And PR de novo chaos type requer entry

  Scenario: 7y archive for compliance
    Given chaos run completed
    When report archived
    Then state pre/post + seed + SLO impact stored em R2
    And retention_years=7 metadata
    And immutable (no overwrite)

  Scenario: Métricas Prometheus snake_case
    Given chaos run completed
    When métricas emitted
    Then corelink_chaos_run_total counter incremented
    And labels {experiment, outcome, env=staging}
    And NUNCA per-tenant labels

  Scenario: Chaos in prod inadvertent (hard rule)
    Given any code path attempting chaos em prod
    When invoked
    Then HARD env check enforce code level fails
    And CRITICAL post-mortem trigger
```

## 9. Design Decisions

### 9.1 Why staging-only at GA (não prod)

- Chaos em prod = customer impact risk; CoreLink GA postura conservative.
- Netflix chaos prod gradual após years of staging baseline; CoreLink S-17 entrega staging baseline only.
- Pós-GA roadmap: chaos prod gradual com customer opt-in (S-Y deferred).

### 9.2 Why open-source tooling (Gremlin / chaos-mesh / Litmus)

- Custom framework anti-scope (per spec contract §10).
- Gremlin SaaS free tier 8 experiments (suficiente for 8 chaos types canonical).
- chaos-mesh CNCF: full control + free + Kubernetes-native.
- Litmus CNCF: free + multi-cloud.
- Decisão runtime via tooling spike + ADR committed.

### 9.3 Why deterministic seed reproducible

- Chaos runs reproducible = debugging baseline.
- Compliance baseline (SOC 2 + ISO 27001).
- 7y archive requirement per Quality Standard 14.s17.1.

### 9.4 Why safe-mode auto-abort hard at code level

- Chaos in prod inadvertent = CRITICAL impact (per spec contract §15 row 8).
- Hard env check at code level (first line) = zero tolerance enforcement.
- Alert on prod hit = CRITICAL post-mortem trigger.

### 9.5 Why chaos catalog ≥ 8 FMs (não 26)

- 8 = critical paths (latency / failure / resource exhaustion / network partition).
- 26 = full FM inventory (post-GA expansion).
- Quality Standard 14.s17.6: PR de novo chaos type requer entry + threshold + reviewer SRE.

### 9.6 Why CF Workers cron (não custom scheduler)

- CF native + free tier baseline.
- Reuse existing infra.
- Cron canonical pattern.

### 9.7 ADR potencial?

- Sim — ADR mandatory para tooling decision (Gremlin vs chaos-mesh vs Litmus).
- ADR committed em `specs/_decisions/ADR-XXXX-chaos-tooling-decision.md` durante WI execution.

## 10. Completeness Criteria

- [ ] **10.s17.001.1** Chaos automation tooling integrated (Gremlin / chaos-mesh / Litmus; ADR committed).
- [ ] **10.s17.001.2** CF Workers cron scheduler weekly em staging (EVT-018).
- [ ] **10.s17.001.3** 8 chaos types canonical scripted.
- [ ] **10.s17.001.4** Deterministic seed reproducible verified (state pre/post + seed em chaos_run_state.json).
- [ ] **10.s17.001.5** Safe-mode auto-abort HARD env check enforced at code level + alert on prod hit (EVT-027).
- [ ] **10.s17.001.6** Chaos catalog ≥ 8 FMs documented em `specs/05_quality/chaos/<experiment>.md` (EVT-018).
- [ ] **10.s17.001.7** SLO impact measured via S-09 multi-burn-rate alerts reuse.
- [ ] **10.s17.001.8** 7y archive metadata + immutable (compliance).
- [ ] **10.s17.001.9** 1st chaos run executed em staging (EVT-023).
- [ ] **10.s17.001.10** Métricas snake_case Prometheus emitting.

## 11. DoD

- [ ] Tooling decision committed (ADR).
- [ ] CF Workers cron scheduler deployed em staging.
- [ ] 8 chaos types scripted + tested em staging.
- [ ] Deterministic seed reproducible verified.
- [ ] Safe-mode auto-abort tested (3 scenarios: prod target violation, prod SEV-1, staging error rate high).
- [ ] Chaos catalog ≥ 8 FMs documented.
- [ ] 1st chaos run executed em staging + report archived 7y.
- [ ] Métricas emitting em staging.
- [ ] Adversarial scenarios 5+ documented.

## 12. Invariants Validated

- **PAT-DEGRADE-001** (chaos test partial degradation) — IMPLEMENTA primary reflection em scheduler.
- **CTRL-PRIV-001** (zero PII em logs) — chaos test reports sanitized; no PII em state captured.
- **Não introduz INVs novas** (sprint operational; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Chaos scheduler | `infra/chaos/scheduler.ts` | TypeScript |
| Chaos config | `infra/chaos/chaos_config.json` | JSON |
| Chaos catalog | `specs/05_quality/chaos/<experiment>.md` × 8 | Markdown |
| ADR tooling decision | `specs/_decisions/ADR-XXXX-chaos-tooling-decision.md` | Markdown |
| 1st chaos run report | `specs/_audits/2026-XX-XX-chaos-run-1.md` | Markdown |
| Chaos run state archive | R2 `evidence-chaos/<run_id>.json` (7y retention) | JSON |

## 14. Quality Standards

- **14.s17.001.1** Chaos test reproducible (deterministic seed); report retained 7y for compliance (per Quality Standard 14.s17.1).
- **14.s17.001.2** Safe-mode auto-abort enforced at code level (zero tolerance prod hit).
- **14.s17.001.3** Chaos catalog discipline: PR de novo chaos type requer entry + safe-mode threshold + reviewer SRE (per Quality Standard 14.s17.6).
- **14.s17.001.4** Cost regression gate: chaos infra ≤ $50/mês (chaos-mesh free tier or equivalent).

## 15. Test Plan

### Unit tests
- Safe-mode env check enforced (test prod target violation throws).
- Safe-mode prod SEV-1 check enforced (test aborts with reason).
- Safe-mode staging error rate check enforced (test aborts with reason).
- Deterministic seed reproducibility (same seed = same result).

### Integration tests
- 8 chaos types execute em staging successfully.
- State captured pre/post written to R2.
- SLO impact measured via S-09 alerts.
- Report archived 7y metadata.

### Adversarial scenarios (5+)
1. Chaos config flag set to production (HARD env check catches; alert + abort).
2. Prod SEV-1 active during cron fire (chaos aborts; alert).
3. Staging error rate spike > 50% (chaos aborts; alert).
4. Chaos report includes PII inadvertent (sanitization fails; CTRL-PRIV-001 violation; redaction reinforce).
5. Chaos catalog entry added without reviewer SRE (PR review catches; PR fails).

### Chaos verification
- Run 1st chaos experiment em staging; verify state pre/post + seed captured + SLO impact measured + report archived.

## 16. Failure Modes

- **FM-150** (transient API): chaos test simulates + retry verification.
- **FM-051** (R2 bit rot): chaos R2 5xx covers + retry.
- **FM-057** (Neon failover): chaos D1/Neon timeout covers.
- **FM-059** (DO quota exceeded): chaos resource exhaustion covers.
- **FM-101** (CF edge outage): chaos network partition covers (additional opt).
- **FM-105** (region replication diverge): chaos cross-region partition covers (additional opt).
- **FM-202** (runbook stale): NÃO covered by chaos (covered by WI-S17-003 dry-run); listed for context.

## 17. Controls

- **CTRL-PRIV-001** (zero PII em logs) enforced em chaos reports sanitized.
- **PAT-DEGRADE-001** chaos test partial degradation reflected.
- Chaos config flag em admin plane S-13 reuse (auth + dual-approval).

## 18. Resilience Patterns

- **PAT-DEGRADE-001** (chaos test partial degradation): chaos test simula partial degradation com graceful degradation expected; SLO impact measured.
- Safe-mode auto-abort (defensive in-depth pattern).
- Deterministic seed (reproducibility pattern).
- 7y archive (compliance pattern).

## 19. Observability

`corelink_chaos_*` métricas Prometheus snake_case canonical:
- `corelink_chaos_run_total{experiment, outcome, env}` (counter).
- `corelink_chaos_safe_mode_abort_total{trigger, env}` (counter; trigger ∈ prod_sev1|staging_error_rate_high|prod_target_violation).
- Alert se outcome=aborted + trigger=prod_target_violation = CRITICAL.
- Alert se 0 runs em 7d (cron not firing).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: chaos config flag em admin plane S-13 reuse requires admin auth + dual-approval; never chaos config via URL params.
- **Tampering**: chaos catalog em git-tracked + reviewer SRE; chaos run state archive immutable em R2.
- **Repudiation**: EVT-023 (CHAOS_EXPERIMENT_REPORT) audit trail forensic-grade; archived 7y.
- **Information disclosure**: chaos report sanitized; CTRL-PRIV-001 enforced; never PII em state captured.
- **DoS**: safe-mode auto-abort se staging error rate > 50% (defensive); CF Workers free tier rate-limited.
- **Elevation of privilege**: chaos config requires admin auth (S-13 reuse); never bypass.

**LINDDUN delta**:
- Linkability: chaos métricas tenant-agnostic (no per-tenant labels).
- Identifiability: chaos report sanitized (no individual user identifiable).
- Non-repudiation: EVT-023 forensic-grade.
- Detectability: safe-mode alerts + métricas.
- Disclosure: chaos config scope-limited admin S-13 reuse.

## 21. Dependencies

### Hard blockers
- S-09 SEALED (multi-burn-rate alerts for SLO impact measurement; audit events R2 bucket).
- S-13 SEALED (admin plane permite chaos config flag — staging-only enforcement).

### Soft blockers
- S-01..S-10 SEALED (sistemas reais para gerar chaos contra).

### Outbound
- WI-S17-002 (DR drill consume chaos automation patterns).
- WI-S17-006 (game day consume chaos catalog).

## 22. Effort PERT

O: 16h, M: 24h, P: 38h → PERT **25.0h** (per spec contract §12; foundation WI; tooling integration + 8 experiments + safe-mode + catalog + 7y archive).

## 23. Cost Analysis

**Direct cost**:
- chaos-mesh / Gremlin free tier: $0/mês.
- CF Workers cron: included em existing tier ($0).
- R2 7y archive: ~$5/mês (low-volume chaos reports).
- Grafana Cloud chaos métricas: included em existing tier.

**Total**: ~$5/mês.

**Indirect cost**: 0 prod incidents during chaos (safe-mode enforced) + chaos confidence baseline = priceless.

## 24. Post-mortem Hooks

- Chaos in prod inadvertent → CRITICAL post-mortem + Security review + staging-only enforcement reinforce.
- Chaos test escapes safe-mode (impact prod) → CRITICAL post-mortem (zero tolerance).
- Chaos catalog entry added without reviewer SRE → PR review reinforce.

## 25. Rollback / Recovery

Chaos automation rollback: disable cron schedule + abort active runs. RTO ≤ 5min. Recovery via chaos config rollback git revert.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Chaos in prod inadvertent | L | H | CRITICAL | M | LOW | HARD env check at code level + alert prod hit + CRITICAL post-mortem trigger |
| R-002 | Chaos test quebra staging dev cycle | M | L | LOW (é staging) | L | LOW | Safe-mode auto-abort + scheduler off-hours staging + isolated tenant |
| R-003 | Chaos test infra cost (tooling overhead) | L | M | LOW | L | LOW | Open-source tooling (chaos-mesh free); CF Workers cron free tier |
| R-004 | Deterministic seed not reproducible | L | M | MEDIUM | L | LOW | State captured pre/post; seed input verified; reproducibility test em CI |
| R-005 | Chaos catalog drift (entries outdated) | M | M | LOW | M | LOW | PR review reviewer SRE; quarterly review cadence |
| R-006 | Tooling vendor lock-in (Gremlin SaaS) | L | M | MEDIUM | L | LOW | Open-source preferred (chaos-mesh / Litmus); ADR documents migration path |

## 27. Knowledge Transfer

- Tech talk (1h): "Chaos engineering em CoreLink — staging-only baseline + safe-mode + 8 types".
- Doc `docs/internal/s17-chaos-runbook.md` — chaos run procedure.
- Onboarding test (3 questions): safe-mode triggers + deterministic seed rationale + chaos catalog discipline.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

**Staffing reality (per ADR-0034 solo-tier)**:

| Status atual (2026-04-29) | Roles |
|---|---|
| **Confirmed (3)** | Owner (Gustavo Schneiter); Final Approver (Gustavo Schneiter); Engineer (Gustavo Schneiter — solo founder dual-hat) |
| **Pending Tier-1 hire/contract (4 specialized canonical roles em STANDARD)** | SRE Lead, Oncall Manager, QA, Compliance officer |

**Recommended path**: ADR-0034 solo-tier waiver + Option C parallel staffing track (SRE Lead + Oncall Manager).

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _TBD; emphatic — chaos automation + safe-mode + catalog discipline_ | _pending_ | _pending_ |
| 4 | Engineer | Gustavo Schneiter | _pending_ | _pending_ |
| 5 | Oncall Manager | _TBD; emphatic — chaos cadence + on-call coordination_ | _pending_ | _pending_ |
| 6 | QA | _TBD; emphatic — adversarial chaos scenarios + reproducibility_ | _pending_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — chaos report sanitization + CTRL-PRIV-001_ | _pending_ | _pending_ |

> Compliance officer canonical em sprint-level PRR (WI-S17-006) dado scope 7y archive for compliance + LINDDUN review; folded em Privacy officer at WI-001 level.

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S17-001 (cycle 12.S17.0; STANDARD lane; chaos automation + 8 types + safe-mode + catalog ≥ 8 FMs + deterministic seed). |

## 30. Anti-patterns evitados

- Skip safe-mode auto-abort (CRITICAL gap).
- Skip deterministic seed reproducible (compliance fail).
- Custom chaos framework (use open-source).
- Chaos in prod inadvertent (HARD env check at code level).
- Chaos config without admin auth (S-13 reuse mandatory).
- Chaos catalog entry without reviewer SRE.

---

**Fim WI-S17-001.**
