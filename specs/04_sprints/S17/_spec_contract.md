---
id: "SPEC-CONTRACT-S17"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s17", "ops", "chaos-engineering", "dr-drills", "runbooks", "post-mortem", "sre", "standard", "sota-v1.1"]
---

# Spec Contract — S-17: Ops Maturity (Chaos Automation + DR Drills + Runbook Discipline + Oncall)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-17 |
| Nome | Ops Maturity |
| Lane | STANDARD |
| Lane forcing factors | n/a (STANDARD) — operational maturity sprint; not introducing new tenant data path |
| Duração estimada | 4 semanas |
| WIs antecipados | 6 |
| SOTA target | SRE ops maturity production-grade — chaos weekly + DR semestral + runbook drill monthly + post-mortem blameless + oncall fadigue tracking |

## 1. Objetivo

**Operacionalizar a disciplina SRE** que torna CoreLink production-resilient: chaos engineering automation com **chaos experiments semanais em staging** (4 semanas mínimo de execution antes de promote para GA), **DR drills semestrais** (simular CF region outage + failover), **runbook dry-runs mensais** (PAT-RUNBOOK-DRILL-001; oncall executa 1 P0/P1 runbook por mês), incident template + post-mortem blameless workflow, oncall rotation com fadigue tracking. Sem ops maturity, produção eventualmente falha **silenciosamente** ou **catastrófica em primeira crise**.

**Por que SOTA:** competitors lançam GA com chaos esporádico ou apenas em produção. CoreLink S-17 entrega: (a) chaos weekly em staging por 4 semanas pré-GA (deterministic seed reproducible); (b) DR drill com SLO impact measurement; (c) runbook drill cadence mensal sustained. Reference: **Netflix Chaos Engineering Principles**, **Google SRE Workbook Ch 12 (Chaos Engineering)** + **Ch 8 (On-Call)**, **Gremlin chaos taxonomy**.

**Codex finding:** sprint duração era 2.5 semanas mas DoD pedia "4 semanas chaos test" — corrigido para 4 semanas (15-20 dias úteis com buffer 5d) com chaos test executado on parallel ao sprint próprio.

## 2. Lane + forcing factors

- **Lane:** STANDARD (5–8 sign-offs).
- **Não FF-HR**: ops sprint não introduz tenant data path novo; consome SLO/observability já validated.
- **Atenção em chaos discipline**: chaos em prod não autorizado at GA; staging-only weekly por 4 semanas.

## 3. Inherits_from

```yaml
inherits_from:
  - "RESILIENCE-PATTERNS"       # PAT-RUNBOOK-DRILL-001, PAT-CORRELATION-ID-001, PAT-DEGRADE-001
  - "FAILURE-MODES"             # FM-202 (runbook stale), 26 FMs P0/P1
  - "OBSERVABILITY-MODEL"       # SLO impact measurement, fadigue metrics
  - "SLO-CATALOG"               # multi-burn-rate alerts already from S-09
  - "SECURITY-MODEL"            # post-mortem para security incidents
  - "PRIVACY-MODEL"             # post-mortem privacy incidents
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-OPS-001** | Chaos engineering automation | Weekly staging chaos com deterministic seed; cobertura ≥ 8 FMs; auto-rollback se SEV-1 prod. |
| **CAP-OPS-002** | DR drill scheduler | Semestral cadence; full region outage simulation; SLO impact measurement; report. |
| **CAP-OPS-003** | Runbook dry-run tracker + EVT-017 | Monthly cadence; oncall executa 1 P0/P1 runbook; output em EVT-017; runbook drift detection. |
| **CAP-OPS-004** | Incident template + post-mortem workflow | Template em `_templates/incident.md` + `_templates/post_mortem.md`; blameless culture enforced. |
| **CAP-OPS-005** | Oncall rotation + fadigue tracking | PagerDuty schedule; fadigue metric (> 2 SEV-1 per shift = alert); rotation health dashboard. |
| **CAP-OPS-006** | Chaos catalog + taxonomy | Documented chaos experiments per FM; reproducible seed; safe-mode auto-abort. |
| **CAP-OPS-007** | Game day exercises | Quarterly tabletop exercises; major scenario drills (region outage, BYOK key compromise, supply chain attack). |

## 5. Requirements específicos

### 5.1 Chaos Automation (CAP-OPS-001 + CAP-OPS-006)

- **R-S17-1**: Chaos scheduler em staging weekly com 8 chaos types:
  - **Latency injection**: R2 GET +500ms, D1 query +200ms, Neon query +300ms, KV +100ms.
  - **Failure injection**: R2 5xx 1%, D1 timeout 0.5%, Neon connection drop 0.1%.
  - **Resource exhaustion**: DO storage near limit, KV quota near limit.
  - **Network partition**: edge-to-origin 10s, cross-region 30s.
- **R-S17-2**: Deterministic seed por test run (reproducible); state captured pre/post; SLO impact measured.
- **R-S17-3**: Safe-mode auto-abort: chaos test halts se prod SEV-1 OR staging error rate > 50%.
- **R-S17-4**: Chaos catalog em `specs/05_quality/chaos/<experiment>.md` per FM coverage ≥ 8 FMs.

### 5.2 DR Drills (CAP-OPS-002)

- **R-S17-5**: DR drill semestral (every 6 months) calendar:
  - Cycle 1: simulate CF region outage in staging; failover to secondary region; SLO sustained.
  - Cycle 2: simulate D1 primary loss + restore from backup.
  - Cycle 3: simulate BYOK key compromise + crypto-erase + customer notification.
- **R-S17-6**: DR drill report includes: pre-drill state, drill timeline, SLO impact measured, lessons learned, runbook updates needed.

### 5.3 Runbook Discipline (CAP-OPS-003)

- **R-S17-7**: Runbook dry-run workflow:
  - Monthly cadence: oncall executa 1 P0/P1 runbook (rotating which one).
  - Output em EVT-017 com timing + discrepancies + updates.
  - Runbook drift detection (FM-202 mitigation): if dry-run > 2× expected → flag for review.
- **R-S17-8**: All 40 runbooks dry-run executed em últimos 90d (S-20 GA gate).

### 5.4 Incident Template + Post-Mortem (CAP-OPS-004)

- **R-S17-9**: Incident template `specs/_templates/incident.md` com:
  - Header: severity, start/end ts, services affected, customer impact estimate.
  - Timeline: events com timestamps + actor.
  - Resolution: actions taken + verification.
- **R-S17-10**: Post-mortem template `specs/_templates/post_mortem.md` blameless culture:
  - 5-Why analysis.
  - Action items com owner + due date.
  - Lessons learned (positive + negative).
  - **Sin nomeação de blame**; foco em system/process improvements.
- **R-S17-11**: Post-mortems retroactively linked a sprint (sprint owner accepts/rejects action items).

### 5.5 Oncall Rotation + Fadigue (CAP-OPS-005)

- **R-S17-12**: PagerDuty schedule:
  - 24/7 rotation com 3 regions (US/EU/APAC) — APAC GA pós-S-20.
  - Tier 1 (primary) + Tier 2 (escalate) + Tier 3 (architect/security if needed).
  - Shift duration: 7 dias máximo; followed by 2-week protection period (no oncall).
- **R-S17-13**: Fadigue metric:
  - SEV-1s per shift; > 2 SEV-1/shift = alert + manager review.
  - SEV-2s per shift; > 5 SEV-2/shift = alert.
  - Total page count per month; > 10 alerts in non-rotation = burnout signal.
- **R-S17-14**: Rotation health dashboard.

### 5.6 Game Days (CAP-OPS-007)

- **R-S17-15**: Quarterly tabletop exercises (4-hour session):
  - Scenario presented; team works through response per runbooks.
  - Observe gaps em runbooks/process; iterate.
  - Outputs: action items + runbook updates.

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6.
- [ ] **4 weeks chaos tests** executados em staging sem incidents não-detectados (sustained 4-week period concurrent ao sprint subsequente — não dentro do sprint S-17 timeline 4-week duration; documented as post-sprint observation period required for promotion gate) (EVT-023).
- [ ] **1 DR drill completo** em staging (simulate CF region outage) — full report (EVT-023 + EVT-017).
- [ ] **3 runbook dry-runs** com EVT-017 coletados (P0/P1) (EVT-017).
- [ ] **Incident template** + **post-mortem template** em `specs/_templates/` reviewed Engineering + SRE (EVT-016).
- [ ] **Post-mortem template tested** em 1 incident sintético (full flow) (EVT-016).
- [ ] **PagerDuty schedule** publicado + rotation iniciada (EVT-026 if exists; EVT-018 alternative).
- [ ] **Fadigue dashboard live**: SEV-1/shift, SEV-2/shift, total pages/month per oncall (EVT-021).
- [ ] **Chaos catalog** documented ≥ 8 FMs covered em `specs/05_quality/chaos/` (EVT-018).
- [ ] **1 game day exercise** executed quarterly cadence start (EVT-023).
- [ ] **PRR STANDARD**: SRE lead + Engineer + Oncall manager + Product + QA + Compliance officer + Privacy officer (post-mortem privacy incidents).

## 7. Completeness Criteria (delta local)

- [ ] **10.s17.1** Oncall schedule published; rotation começou (sustained 30d minimum to validate fadigue metrics).
- [ ] **10.s17.2** Chaos test coverage ≥ 8 FMs do failure_modes §3 (cobertura crítica; remaining FMs covered post-GA).
- [ ] **10.s17.3** **DR drill semestral** scheduled + 1 cycle completed em staging.
- [ ] **10.s17.4** **Runbook dry-run cadence**: monthly sustained 3 months minimum (S-20 gate exige 90d).
- [ ] **10.s17.5** **Post-mortem blameless culture** documented + trained (engineering all-hands).
- [ ] **10.s17.6** **Game day exercise** quarterly cadence start.
- [ ] **10.s17.7** **Chaos test reproducible** com deterministic seed; report archive 7y.

## 8. Invariants

### Mantidas

- **PAT-RUNBOOK-DRILL-001** executado monthly (resilience_patterns).
- **PAT-CORRELATION-ID-001** in all incidents (facilita debugging).

### Não cria invariants novas (sprint operational; invariants são em outros sprints).

## 9. Quality Standards (delta local)

- **14.s17.1 Chaos tests reproducible** (deterministic seed); report retained 7y for compliance.
- **14.s17.2 Runbooks atualizados** após dry-run se discrepância > 2× expected.
- **14.s17.3 Post-mortem blameless culture** enforced — review by SRE lead; no name shaming.
- **14.s17.4 Oncall fadigue management**: shift duration ≤ 7d + 2w protection period; SEV-1 > 2/shift = alert.
- **14.s17.5 Game day quarterly cadence** sustained pós-GA.
- **14.s17.6 Chaos catalog discipline**: PR de novo chaos type requer chaos catalog entry + safe-mode threshold + reviewer SRE.
- **14.s17.7 DR drill report quality**: includes pre-state, timeline, SLO impact, lessons, runbook updates; archived 7y.
- **14.s17.8 Incident response time tracking**: MTTA (mean time to acknowledge), MTTR (mean time to resolve); targets MTTA < 5min, MTTR < 30min para SEV-1.

## 10. Anti-scope

- ❌ Full SRE team hiring (post-GA demand-driven; S-17 estabelece practices, não headcount).
- ❌ Custom chaos framework — usar Gremlin / chaos-mesh / Litmus (open source) ou existing tool.
- ❌ Chaos em produção GA — anti-scope estrito; chaos em staging-only at GA.
- ❌ FedRAMP-mandated continuous monitoring — pós-GA enterprise.
- ❌ Multi-vendor incident response orchestration (xMatters, FireHydrant) — PagerDuty only at GA.
- ❌ AI-powered RCA — pós-GA Q1+; manual blameless analysis at GA.
- ❌ Public status page customer-facing automation — manual at GA; automation post.

## 11. Dependencies

### Hard blockers

- **S-09 SEALED** (observability pra detectar chaos impact + multi-burn-rate alerts).

### Soft blockers

- **S-01..S-10 SEALED** (sistemas reais para gerar chaos contra).
- **S-13 SEALED** (admin plane permite chaos config flag).

### Outbound

- S-20 (GA exige all 42 runbooks dry-run em 90d + 4-week chaos test sustained + DR drill done; count atual repo Lote 9.5c, vs 26 baseline original).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S17-001** | Chaos scheduler + 8 chaos types staging weekly + chaos catalog | tooling integration (Gremlin/chaos-mesh); 8 experiments scripted; catalog; safe-mode | 16h | 24h | 38h | **25.0h** |
| **WI-S17-002** | DR drill tooling + semestral cadence + 1 cycle completed | drill scripts; semestral calendar; 1 cycle execution staging; report template | 14h | 22h | 36h | **23.0h** |
| **WI-S17-003** | Runbook dry-run tracker + 3 dry-runs P0/P1 + monthly cadence | tracker tool; 3 P0/P1 dry-runs; cadence calendar; FM-202 drift detection | 10h | 16h | 26h | **16.7h** |
| **WI-S17-004** | Incident + post-mortem templates + 1 sintético test + blameless training | incident template; post-mortem template; 1 sintético dry-run; engineering all-hands training | 8h | 14h | 22h | **14.3h** |
| **WI-S17-005** | Oncall PagerDuty schedule + fadigue tracking + dashboard | PD schedule; fadigue métricas; dashboard; rotation health alert | 10h | 14h | 22h | **14.7h** |
| **WI-S17-006** | Game day quarterly cadence start + 1 tabletop exercise + PRR | scenarios; 1 tabletop game day; action items tracking; PRR doc | 8h | 14h | 22h | **14.3h** |

**Total PERT:** ~108h ≈ 14 dias work × 1 eng. Buffer 5 dias confere com 4 semanas (chaos 4 weeks observation extends post-sprint to S-18+).

## 13. Duração + Timeline

- **Duração:** 4 semanas (15-20 dias úteis) + buffer 5 dias.
- **Note:** "4-weeks chaos test" requirement requires observation period that extends beyond sprint duration; chaos automation começa Day 1 e roda continuous para próximas 4-week observation period concurrent com S-18..S-20 sprints.
- **Marcos:**
  - **D+5:** WI-001 SEALED (chaos automation + 1st run).
  - **D+8:** WI-003 + WI-004 SEALED (runbook dry-runs + templates).
  - **D+12:** WI-002 SEALED (DR drill 1st cycle).
  - **D+15:** WI-005 SEALED (oncall + fadigue).
  - **D+18:** WI-006 SEALED (game day + PRR).
  - **D+20:** Sprint review + sign-offs.
  - **Post-sprint:** chaos automation continua weekly until S-20 GA gate (4 weeks total observation needed).

## 14. Critérios de promoção

- DoD complete + 4 weeks chaos test sustained (concurrent post-sprint observation).
- 1 DR drill cycle completed.
- 3 runbook dry-runs.
- Oncall rotation health dashboard live + 30d fadigue baseline.
- PRR STANDARD aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Chaos test quebra staging** → dev cycle interrompido | M | L | LOW (é staging) | L | LOW | Safe-mode auto-abort + scheduler off-hours staging + isolated tenant for chaos. |
| **Oncall fadigue fatal** (leak to prod via missed page) | L | M | HIGH | M | LOW | Fadigue tracking + > 2 SEV-1/shift alert + manager review + 2-week protection period. |
| **Runbook drift** (FM-202) não detectado | M | M | MEDIUM | M | LOW | Monthly dry-run cadence + drift detection (> 2× expected = flag) + RB-FM-202 dry-run. |
| **DR drill encontra unrecoverable failure** | L | M | HIGH (delay GA) | L | LOW | Drill em staging only + isolated tenant + report findings → fix em sprint subsequente. |
| **Post-mortem culture violated** (blame surfacing) | M | M | MEDIUM (psych safety) | M | LOW | SRE lead reviews; engineering all-hands training; iterate. |
| **Chaos test infra cost** (tooling overhead) | L | M | LOW | L | LOW | Open-source tooling (chaos-mesh free); CF Workers cron for orchestration. |
| **Game day quality variability** | M | L | LOW | L | LOW | Pre-defined scenario library; SRE lead facilitates; iterate quarterly. |
| **Chaos in prod inadvertent** | L | H | CRITICAL | M | LOW | Hard env check before chaos; staging-only enforcement at code level; alert on any prod hit. |
| **Oncall handoff issues** (knowledge transfer) | M | M | MEDIUM | M | LOW | Handoff template + 15-min sync per shift change; runbook URL standardized. |
| **Runbook URL not in alert payload** | L | L | LOW | L | LOW | S-09 R-S09-14 covers (PR fail if alert sem runbook URL). |

## 16. Benchmarks SOTA externos

| Critério | Netflix Chaos | Google SRE | Cloudflare Internal | Gremlin Saas | **CoreLink target S-17** |
|---|---|---|---|---|---|
| Chaos automation weekly | Yes | Yes | Yes | Yes | **Yes — 4 weeks pre-GA staging** |
| DR drill semestral | Yes | Yes | Yes | Manual | **Yes — semestral with full report** |
| Runbook dry-run cadence | Continuous | Monthly | Monthly | Manual | **Monthly + 90d coverage S-20 gate** |
| Post-mortem blameless | Yes | Yes (foundational) | Yes | Yes | **Yes — template + training + culture enforce** |
| Oncall fadigue tracking | Yes | Yes | Yes | Manual | **Yes — > 2 SEV-1/shift alert + protection period** |
| Game day quarterly | Yes | Yes | Yes | Yes | **Yes — quarterly cadence start** |
| Chaos catalog reproducible | Yes | Yes | Yes | Yes (proprietary) | **Yes — deterministic seed + 7y archive** |
| Safe-mode auto-abort | Yes | Yes | Yes | Yes | **Yes — error rate threshold + prod hit kill** |

**Veredito SOTA:** S-17 v1.1 atinge feature parity com Netflix / Google SRE em 8/8 dimensões.

## 17. References (RFCs, papers, standards)

- **Netflix Chaos Engineering Principles** <https://principlesofchaos.org/>.
- **Google SRE Workbook Ch 8** — On-Call.
- **Google SRE Workbook Ch 12** — Introducing Non-Abstract Large System Design (chaos foundations).
- **Google SRE Book Ch 15** — Postmortem Culture.
- **Gremlin Chaos Engineering Reliability Report 2024**.
- **NIST SP 800-61 Rev.2** — Computer Security Incident Handling Guide.
- **PagerDuty Incident Response Documentation** <https://response.pagerduty.com/>.
- **Atlassian Blameless Post-Mortem Template**.
- **chaos-mesh** <https://chaos-mesh.org/>.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Chaos test escapes safe-mode (impact prod) → CRITICAL post-mortem.
- DR drill fails recovery → 5-Why mandatório + runbook updates.
- Oncall fadigue alert (>2 SEV-1/shift) → manager review + system improvements.
- Runbook drift > 2× expected duration → 5-Why + runbook update.
- Post-mortem culture violation → engineering review + reinforcement training.
- Game day finding surfaces critical gap → action items prioritized + tracking.

## 19. Waiver policy

S-17 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ 4-week chaos test sustained — operational confidence baseline.
- ❌ DR drill 1 cycle completed — recovery capability validated.
- ❌ 3 runbook dry-runs — ops discipline baseline.
- ❌ PagerDuty schedule live — oncall responsiveness baseline.

Itens waivable com SRE lead + ADR:

- ⚠️ 8 chaos types → 6 chaos types GA (defer 2 to post-GA com plan).
- ⚠️ Game day quarterly → semestral (com plan to ramp to quarterly).
- ⚠️ DR drill semestral cycle 2 → annual at GA (acceptable risk).

---

**Fim spec contract S-17 v1.1.0 SOTA.**
