---
id: "S-17"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
# lane_forcing_factors omitted: STANDARD lane permite empty (REG-LANE-003 só obriga se lane=HIGH_RISK; schema minItems:1 rejeita empty array)
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
tags: ["sprint", "s17", "ops", "chaos-engineering", "dr-drills", "runbooks", "post-mortem", "sre", "standard"]
---

# Sprint S-17 — Ops Maturity (Chaos Engineering Automation Weekly Staging + Deterministic Seed Reproducible + Safe-mode Auto-abort + Chaos Catalog ≥ 8 FMs em `specs/05_quality/chaos/<experiment>.md`) + DR Drill Scheduler Semestral Cadence + 1 Cycle Completed em Staging (Simulate CF Region Outage + Failover + SLO Impact Measurement + Report 7y Archive) + Runbook Discipline (PAT-RUNBOOK-DRILL-001 Mensal Dry-run Cadence + EVT-017 Tracker + 3 P0/P1 Dry-runs + FM-202 Runbook Drift Detection > 2× Expected = Flag) + Incident Template + Post-Mortem Blameless Workflow (5-Why Analysis + Action Items Owner+Due + Lessons Learned Positive+Negative + Sin Nomeação de Blame; 1 Synthetic Incident Test + Engineering All-hands Training) + Oncall Rotation PagerDuty Schedule (Tier 1+2+3; 7d Shift Max + 2-week Protection Period; 24/7 Future-proof US/EU/APAC) + Fadigue Tracking Dashboard (SEV-1 > 2/shift = Alert + Manager Review; SEV-2 > 5/shift = Alert; > 10 Pages/month Non-rotation = Burnout Signal) + Game Day Quarterly Cadence Start + 1 Tabletop Exercise (4-hour Session Major Scenario Drills) + MTTA < 5min + MTTR < 30min Targets para SEV-1 + Closing PRR STANDARD 5-8 Sign-offs Canonical (7 Typical: SRE Lead + Engineer + Oncall Manager + Product + QA + Compliance + Privacy)

> **doc_status:** DRAFT · **lane:** STANDARD · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 12.S17.0 SOTA elevation; STANDARD lane no forcing factors)

> **Phase boundary:** Fase 5 — Operational maturity production-grade pré-GA (S-20 hard dependency: 4 weeks chaos sustained + DR drill done + monthly runbook drill cadence + game day quarterly cadence start).
> **STANDARD lane rationale (zero forcing factors):** ops sprint não introduz tenant data path novo; consome SLO/observability já validated em S-09 (multi-burn-rate alerts), audit events R2 (S-09 audit bucket), admin plane API (S-13 admin chaos config flag). Não há cripto-load-bearing controles novos (S-14 HIGH_RISK BYOK reuse only para BYOK key compromise drill scenarios). Chaos enforcement é staging-only at GA — staging-only enforcement é well-bounded surface (env check before chaos; staging-only enforcement at code level; alert on any prod hit). DR drill em staging only + isolated tenant. Post-mortem culture é process discipline (não cripto-load-bearing). Oncall rotation é process discipline. Game day é process discipline. PAT-RUNBOOK-DRILL-001 reuse pattern já em resilience_patterns.md.

> **Two-phase SEAL D+20/D+50 rationale:** S-17 DoD §6 explicitly requires "4 weeks chaos sustained" + "monthly runbook dry-run" + "1 game day quarterly cadence start" + "30d fadigue baseline" — these are 30d+ observation criteria that cannot be instant-verified at sprint close. Two-phase SEAL split:
> - **Implementation SEAL D+20** (Sprint Review ceremony): chaos automation tooling deployed + 1st chaos run executed + chaos catalog ≥ 8 FMs documented + DR drill 1 cycle completed em staging + 3 runbook dry-runs done + incident/post-mortem templates committed + 1 synthetic incident test + engineering all-hands training + PagerDuty schedule live + rotation iniciada + fadigue dashboard live + 1 game day exercise executed + closing PRR coletados.
> - **GA Evidence Gate D+50** (post-sprint observation window 30d): 4-week chaos sustained 30d staging green + monthly runbook drill cadence sustained 30d (3 dry-runs) + 30d fadigue baseline established + game day quarterly cadence began + chaos test reproducibility verified (deterministic seed + 7y archive) + post-mortem culture sustained (no blame surfacing reviewed pelo SRE lead) + report final assinado.

---

## 1. Objetivo

Operacionalizar a **disciplina SRE** que torna CoreLink production-resilient pré-GA: chaos engineering automation com **chaos experiments semanais em staging** (4 semanas mínimo de execution antes de promote para GA; staging-only at GA — chaos em prod é anti-scope estrito), **DR drills semestrais** (simular CF region outage + failover; semestral cadence calendar; 1 cycle completed em staging com SLO impact measurement + report; 7y archive for compliance), **runbook dry-runs mensais** (PAT-RUNBOOK-DRILL-001; oncall executa 1 P0/P1 runbook por mês; rotating which one; output em EVT-017 com timing + discrepancies + updates; runbook drift detection FM-202 mitigation via > 2× expected = flag for review), incident template + post-mortem blameless workflow (5-Why analysis + action items com owner+due date + lessons learned positive+negative + sin nomeação de blame; 1 synthetic incident test full flow + engineering all-hands training enforced), oncall rotation com PagerDuty schedule (Tier 1 primary + Tier 2 escalate + Tier 3 architect/security if needed; shift duration ≤ 7d com followed 2-week protection period no oncall; 24/7 rotation com 3 regions US/EU/APAC future-proof — APAC GA pós-S-20), fadigue tracking dashboard (SEV-1s per shift > 2 = alert + manager review; SEV-2s per shift > 5 = alert; total page count per month > 10 em non-rotation = burnout signal), game day quarterly cadence start com 1 tabletop exercise (4-hour session; pre-defined scenario library; SRE lead facilitates; outputs action items + runbook updates), MTTA < 5min + MTTR < 30min targets para SEV-1 (canonical per spec contract §9.8). Sem ops maturity, produção eventualmente falha **silenciosamente** ou **catastrófica em primeira crise**.

Decomposição em 6 WIs: (1) **WI-S17-001** chaos scheduler + 8 chaos types staging weekly + chaos catalog ≥ 8 FMs (latency injection R2/D1/Neon/KV; failure injection R2 5xx/D1 timeout/Neon connection drop; resource exhaustion DO storage/KV quota; network partition edge-to-origin/cross-region; deterministic seed reproducible; state captured pre/post; SLO impact measured; safe-mode auto-abort se prod SEV-1 OR staging error rate > 50%; tooling integration Gremlin / chaos-mesh / Litmus open-source; CF Workers cron orchestrator); (2) **WI-S17-002** DR drill tooling + semestral cadence calendar + 1 cycle completed (simulate CF region outage staging + failover to secondary region + SLO sustained measured; report includes pre-state + drill timeline + SLO impact + lessons learned + runbook updates needed; 7y archive for compliance); (3) **WI-S17-003** runbook discipline tracker + 3 P0/P1 dry-runs + monthly cadence calendar + FM-202 drift detection (PAT-RUNBOOK-DRILL-001 mensal; oncall executa 1 P0/P1 runbook rotating; output EVT-017 com timing + discrepancies; if dry-run > 2× expected = flag for review; RB-FM-202 runbook stale dry-run mensal); (4) **WI-S17-004** incident + post-mortem templates blameless + 1 sintético test + all-hands training (incident template `specs/_templates/incident.md` com header severity/timeline/resolution; post-mortem template `specs/_templates/post_mortem.md` com 5-Why + action items + lessons + sin blame; 1 synthetic incident test full flow; engineering all-hands training reinforce blameless culture; SRE lead reviews); (5) **WI-S17-005** oncall PagerDuty schedule + fadigue tracking + dashboard (PD schedule Tier 1/2/3; shift ≤ 7d + 2w protection; 24/7 rotation 3 regions future-proof; fadigue métricas SEV-1/shift + SEV-2/shift + total pages/month; rotation health dashboard; alerts thresholds); (6) **WI-S17-006** game day quarterly cadence start + 1 tabletop exercise + chaos catalog cleanup + closing PRR (quarterly tabletop exercise 4-hour session; pre-defined scenario library — region outage / BYOK key compromise / supply chain attack; SRE lead facilitates; action items tracked; chaos catalog cleanup post-1st run; PRR doc S-17 com 5-8 sign-offs canonical). Implementa **CAP-OPS-001..007** + reforça **PAT-RUNBOOK-DRILL-001** monthly cadence + **PAT-CORRELATION-ID-001** in all incidents + **PAT-DEGRADE-001** chaos test partial degradation. Não introduz novas INVs (sprint operational; per spec contract §8 mantidas only).

**Por que SOTA:** competitors lançam GA com chaos esporádico ou apenas em produção (Datadog Chaos Mode, AWS Resilience Hub manual). CoreLink S-17 entrega: (a) chaos weekly em staging por 4 semanas pré-GA com **deterministic seed reproducible** + state captured pre/post + SLO impact measured + safe-mode auto-abort prod hit kill (Netflix/Google SRE parity); (b) DR drill com semestral cadence calendar + SLO impact measurement + lessons learned + runbook updates archived 7y compliance (Google SRE parity; Cloudflare Internal parity); (c) runbook drill cadence mensal sustained 30d + 90d coverage S-20 gate (Google SRE Workbook Ch 8 + 12 reference); (d) post-mortem blameless culture template + 5-Why + sin nomeação de blame + 1 synthetic incident test + engineering all-hands training (Google SRE Book Ch 15 reference; Atlassian template); (e) oncall fadigue tracking SEV-1 > 2/shift alert + 7d shift max + 2w protection period (Google SRE Workbook Ch 8 reference); (f) game day quarterly cadence start (Netflix / Google SRE / Cloudflare Internal / Gremlin SaaS parity); (g) MTTA < 5min + MTTR < 30min targets SEV-1 (industry-leading targets). Reference: **Netflix Chaos Engineering Principles** <https://principlesofchaos.org/>, **Google SRE Workbook Ch 8 (On-Call) + Ch 12 (Chaos Engineering)**, **Google SRE Book Ch 15 (Postmortem Culture)**, **Gremlin Chaos Engineering Reliability Report 2024**, **NIST SP 800-61 Rev.2 (Computer Security Incident Handling Guide)**, **PagerDuty Incident Response Documentation** <https://response.pagerduty.com/>, **Atlassian Blameless Post-Mortem Template**, **chaos-mesh** <https://chaos-mesh.org/>.

## 2. Escopo

### 2.1 In-scope

- **WI-S17-001**: Chaos scheduler em staging weekly com 8 chaos types canonical (latency injection R2 GET +500ms / D1 query +200ms / Neon query +300ms / KV +100ms; failure injection R2 5xx 1% / D1 timeout 0.5% / Neon connection drop 0.1%; resource exhaustion DO storage near limit / KV quota near limit; network partition edge-to-origin 10s / cross-region 30s); deterministic seed por test run (reproducible; state captured pre/post + seed em chaos_run_state.json; SLO impact measured); safe-mode auto-abort (chaos test halts se prod SEV-1 OR staging error rate > 50%; hard env check before chaos via `process.env.CHAOS_TARGET === 'staging'` enforcement at code level; alert on any prod hit); chaos catalog em `specs/05_quality/chaos/<experiment>.md` per FM coverage ≥ 8 FMs (each chaos type documented com FM mapping + reproducible seed + safe-mode threshold + reviewer SRE); chaos automation tooling integration (Gremlin / chaos-mesh / Litmus open-source); CF Workers cron scheduler weekly orchestration; chaos test reproducibility verified (deterministic seed + state captured + 7y archive for compliance per Quality Standard 14.s17.1).

- **WI-S17-002**: DR drill scheduler semestral cadence calendar (every 6 months; cycle 1 = simulate CF region outage in staging + failover to secondary region + SLO sustained; cycle 2 = simulate D1 primary loss + restore from backup; cycle 3 = simulate BYOK key compromise + crypto-erase + customer notification — cycles 2/3 deferred annual at GA via waiver opt); 1 cycle completed em staging (cycle 1 CF region outage; full execution; full report); DR drill report includes pre-drill state + drill timeline + SLO impact measured + lessons learned + runbook updates needed (archived 7y for compliance per Quality Standard 14.s17.7); drill em staging only + isolated tenant for chaos (anti-prod hit guarantee); SRE lead facilitates drill execution; report committed em `specs/_audits/2026-XX-XX-dr-drill-cycle-1.md`.

- **WI-S17-003**: Runbook dry-run workflow tracker (PAT-RUNBOOK-DRILL-001 mensal cadence; oncall executa 1 P0/P1 runbook por mês; rotating which runbook per month per spec contract §5.3); 3 P0/P1 dry-runs executed em sprint (RB-FM-051 R2 bit rot + RB-FM-057 Neon failover + RB-FM-202 runbook stale meta-drill canonical mensal — covering critical paths); output em EVT-017 com timing + discrepancies + updates; runbook drift detection FM-202 mitigation (if dry-run duration > 2× expected → flag for review + post-mortem trigger per Quality Standard 14.s17.2); monthly cadence calendar published; tracker tool em `specs/04_runbooks/_dry_run_log.md` com runbook ID + ts + duration_actual + duration_expected + discrepancies + updates needed + reviewer; **P0/P1 priority subset (~25 of 47) dry-run em 90d** (S-20 GA gate REVISED Lote 10.17 codex P0 math fix; original "all 47/90d" era infeasible = ~16/month burdensome; subset definition: P0/P1 priority labels em runbook frontmatter; P2/P3 runbooks deferred pós-GA continuous coverage; sustainable cadence 8 dry-runs/month × 3 months = 24 covers ~25 P0/P1 subset).

- **WI-S17-004**: Incident template `specs/_templates/incident.md` (header com severity SEV-1/2/3 + start/end ts + services affected + customer impact estimate; timeline com events ts + actor; resolution com actions taken + verification); post-mortem template `specs/_templates/post_mortem.md` (blameless culture enforced — sin nomeação de blame; foco em system/process improvements; 5-Why analysis structured questions; action items com owner + due date + status tracking; lessons learned positive + negative; SRE lead reviews per Quality Standard 14.s17.3; reviewed Engineering + SRE); 1 synthetic incident test full flow (synthetic SEV-2 simulado em staging; team works through incident response per templates; post-mortem produced + reviewed + action items tracked); engineering all-hands training (1.5h session; blameless culture reinforce; 5-Why technique training; q&a; community reinforcement); post-mortems retroactively linked a sprint (sprint owner accepts/rejects action items per spec contract §5.4 R-S17-11).

- **WI-S17-005**: Oncall PagerDuty schedule (Tier 1 primary responder + Tier 2 escalate + Tier 3 architect/security if needed; shift duration 7 dias máximo per Google SRE best practice; followed by 2-week protection period — no oncall in protection per spec contract §5.5 R-S17-12; 24/7 rotation com 3 regions US/EU/APAC future-proof — APAC GA pós-S-20; rotation health published em `apps/web/admin/oncall` ou equivalent); fadigue metric (SEV-1s per shift counter; > 2 SEV-1/shift = alert + manager review; SEV-2s per shift counter; > 5 SEV-2/shift = alert; total page count per month per oncall; > 10 alerts in non-rotation = burnout signal per spec contract §5.5 R-S17-13); rotation health dashboard `corelink_oncall_*` métricas Prometheus emitting (snake_case canonical per observability_model §3.1; `plan` label NÃO aplicável — internal métricas; INV-OBS-CARDINALITY-BUDGET respeitado).

- **WI-S17-006**: Game day quarterly cadence start (4-hour session per quarter; quarterly cadence sustained pós-GA per Quality Standard 14.s17.5); 1 tabletop exercise executed (1 cycle initial; pre-defined scenario library: scenario A region outage CF / scenario B BYOK key compromise simulated / scenario C supply chain attack typosquat / scenario D insider exfil — 4 scenarios; SRE lead facilitates; team works through response per runbooks; observe gaps em runbooks/process; iterate; outputs action items + runbook updates per spec contract §5.6 R-S17-15); chaos catalog cleanup post-1st run (refine seed determinism; refine safe-mode thresholds; refine FM mappings; commit refined catalog); closing PRR doc S-17 com 5-8 sign-offs canonical (7 typical: SRE lead + Engineer + Oncall manager + Product + QA + Compliance officer + Privacy officer per sprint contract §14); evidence pack: chaos test reports 4 weeks (sustained observation per DoD §6) + DR drill report cycle 1 + 3 runbook dry-run EVT-017s + incident/post-mortem templates committed + 1 synthetic incident report + PagerDuty schedule live screenshot + fadigue dashboard live screenshot + 1 game day report + adversarial summary 30+ scenarios cross-WI.

### 2.2 Anti-scope

- Full SRE team hiring (post-GA demand-driven; S-17 estabelece practices, não headcount).
- Custom chaos framework — usar Gremlin / chaos-mesh / Litmus (open-source) ou existing tool; never custom from scratch.
- Chaos em produção GA — anti-scope estrito; chaos em staging-only at GA; hard rule per spec contract §10.
- FedRAMP-mandated continuous monitoring — pós-GA enterprise.
- Multi-vendor incident response orchestration (xMatters, FireHydrant) — PagerDuty only at GA.
- AI-powered RCA — pós-GA Q1+; manual blameless analysis at GA.
- Public status page customer-facing automation — manual at GA; automation post.
- Bug bounty program for chaos testing — pós-GA Q1.
- Chaos in customer environments (chaos-as-a-service) — anti-scope GA; staging-only.

## 3. Customer Impact & Journey

**JTBD:** "Como SRE / Internal Engineer, preciso confiança production-grade para promote CoreLink GA: (a) chaos engineering automation com 4 weeks staging chaos sustained verificando 8 FMs covered + safe-mode auto-abort + deterministic seed reproducible — sin chaos production GA; (b) DR drill semestral cadence + 1 cycle completed staging com SLO impact measurement + lessons learned archived 7y for compliance; (c) runbook dry-run mensal (PAT-RUNBOOK-DRILL-001) com EVT-017 tracker + drift detection FM-202; (d) incident response template + post-mortem blameless workflow + 5-Why + sin blame culture; (e) oncall PagerDuty schedule + Tier 1/2/3 + shift ≤ 7d + 2w protection + fadigue tracking SEV-1 > 2/shift alert + manager review; (f) game day quarterly cadence start + 4-hour tabletop exercise. Como Compliance auditor, preciso evidence pack chaos report 4 weeks sustained + DR drill report + 3 runbook dry-run EVT-017s + 1 post-mortem synthetic test + PagerDuty schedule live + fadigue dashboard live + 1 game day report archived 7y for SOC 2 + ISO 27001 readiness."

**CAPs entregues:** CAP-OPS-001 (chaos engineering automation) + CAP-OPS-002 (DR drill scheduler) + CAP-OPS-003 (runbook dry-run tracker + EVT-017) + CAP-OPS-004 (incident template + post-mortem workflow) + CAP-OPS-005 (oncall rotation + fadigue tracking) + CAP-OPS-006 (chaos catalog + taxonomy) + CAP-OPS-007 (game day exercises).

**Persona 1 — SRE / Internal Engineer**:
- Confidence: 4 weeks chaos sustained staging green sustained = production confidence promote GA.
- Operational: PAT-RUNBOOK-DRILL-001 mensal cadence (RB-FM-202 stale runbook drill canonical) + EVT-017 tracker = ops discipline.
- Sustainability: shift ≤ 7d + 2w protection period + fadigue tracking SEV-1 > 2/shift alert + manager review = burnout prevention.
- Diferenciador competitivo vs Datadog Chaos Mode: deterministic seed reproducible + 4 weeks sustained baseline + chaos catalog ≥ 8 FMs.

**Persona 2 — Compliance auditor (SOC 2 + ISO 27001)**:
- Audit evidence pack: chaos test reports 4 weeks (EVT-023) + DR drill report (EVT-023 + EVT-017) + 3 runbook dry-run EVT-017s + 1 post-mortem synthetic test + PagerDuty schedule (EVT-026 ou EVT-018 alternative) + fadigue dashboard (EVT-021) + 1 game day report (EVT-023) — all 7y retention.
- Recovery capability: DR drill 1 cycle completed em staging com SLO impact measurement = recovery validated.
- Operational discipline: 8 dry-runs/month sustained 30d post-sprint (per Completeness Criteria 10.s17.4 + Lote 10.17 codex P0 canonical math fix) + 90d coverage S-20 gate (P0/P1 priority subset ~25 of 47; sustainable rate 8/month × 3 months = 24 covers subset).
- Post-mortem culture: blameless template + 5-Why + sin nomeação de blame + engineering all-hands training = psychological safety baseline.

**Persona 3 — Oncall Manager**:
- Rotation health dashboard: SEV-1/shift + SEV-2/shift + total pages/month = burnout prevention.
- Fadigue thresholds: > 2 SEV-1/shift = alert + manager review; > 10 pages/month em non-rotation = burnout signal.
- Shift discipline: 7d max + 2w protection period (Google SRE Workbook Ch 8 reference).
- MTTA < 5min + MTTR < 30min targets para SEV-1 = response time accountability.

**SLA addendum**:
- Chaos test 4 weeks sustained: 30d staging green sin incidents não-detectados (concurrent post-sprint observation per S-17 §13).
- DR drill cycle 1: completed em staging + full report archived 7y.
- Runbook dry-run mensal cadence: 1 P0/P1 runbook per month rotating; sustained 90d for S-20 gate.
- Post-mortem culture: blameless reviewed pelo SRE lead; sin name shaming; iterate.
- Oncall fadigue: shift ≤ 7d + 2w protection period; SEV-1 > 2/shift = alert.
- Game day quarterly cadence: 1 tabletop exercise per quarter sustained.
- MTTA < 5min + MTTR < 30min para SEV-1 (canonical per spec contract §9.8).
- Chaos test reproducibility: deterministic seed + 7y archive for compliance.

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `resilience_patterns.md` (PAT-RUNBOOK-DRILL-001 monthly cadence canonical + PAT-CORRELATION-ID-001 in all incidents + PAT-DEGRADE-001 chaos test partial degradation) + `failure_modes.md` (FM-202 runbook stale; 26 FMs P0/P1 inventory; ≥ 8 FMs covered by chaos catalog) + `observability_model.md §3.1` (Prometheus snake_case + `plan` label NÃO aplicável em internal ops métricas; INV-OBS-CARDINALITY-BUDGET respeitado) + `slo_catalog.md` (multi-burn-rate alerts already from S-09 — chaos impact measured; MTTA/MTTR tracking) + `security_model.md` (post-mortem para security incidents; chaos data sanitization no PII em chaos reports) + `privacy_model.md` (post-mortem privacy incidents; chaos test never expose PII; CTRL-PRIV-001 zero PII em chaos logs).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S17-D1 | Chaos scheduler + 8 chaos types + chaos catalog ≥ 8 FMs + safe-mode auto-abort | `infra/chaos/` + `specs/05_quality/chaos/<experiment>.md` × 8 | Chaos scheduler weekly em staging; deterministic seed reproducible; safe-mode auto-abort prod hit kill; 8 chaos types canonical; catalog ≥ 8 FMs documented |
| S17-D2 | DR drill scheduler + semestral cadence + 1 cycle completed | `infra/dr_drills/` + `specs/_audits/2026-XX-XX-dr-drill-cycle-1.md` | DR drill cycle 1 completed em staging (CF region outage simulate + failover + SLO sustained); report archived 7y |
| S17-D3 | Runbook dry-run tracker + 3 P0/P1 dry-runs + monthly cadence + FM-202 drift detection | `specs/04_runbooks/_dry_run_log.md` + 3 EVT-017 entries | 3 P0/P1 dry-runs executed (RB-FM-051 + RB-FM-057 + RB-FM-202); EVT-017 tracker; monthly cadence calendar published; FM-202 drift detection > 2× expected = flag |
| S17-D4 | Incident + post-mortem templates blameless + 1 synthetic incident test + all-hands training | `specs/_templates/incident.md` + `specs/_templates/post_mortem.md` + `specs/_audits/2026-XX-XX-synthetic-incident-test.md` | Templates committed reviewed Engineering + SRE; 1 synthetic SEV-2 incident test full flow + post-mortem produced; engineering all-hands training delivered |
| S17-D5 | Oncall PagerDuty schedule + fadigue tracking dashboard + alerts | PagerDuty config + `infra/dashboards/oncall_health.json` + Prometheus métricas `corelink_oncall_*` | PD schedule Tier 1/2/3 published; rotation iniciada; fadigue dashboard live; SEV-1 > 2/shift alert; SEV-2 > 5/shift alert; > 10 pages/month non-rotation alert |
| S17-D6 | Game day quarterly cadence start + 1 tabletop exercise + chaos catalog cleanup + closing PRR | `specs/_audits/2026-XX-XX-game-day-q1.md` + `specs/04_sprints/S17/PRR-S17.md` | 1 game day 4-hour tabletop exercise executed (4 scenarios library); chaos catalog refined post-1st run; PRR doc S-17 com 5-8 sign-offs canonical |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Resilience Patterns (herda `resilience_patterns.md`)

- **PAT-RUNBOOK-DRILL-001** (monthly cadence canonical) — IMPLEMENTA reflection cumulative em runbook discipline tracker WI-S17-003; EVT-017 per dry-run; FM-202 drift detection > 2× expected = flag; oncall executa 1 P0/P1 runbook por mês rotating; sustained 30d post-sprint (per Completeness Criteria 10.s17.4); 90d coverage S-20 gate.
- **PAT-CORRELATION-ID-001** (in all incidents) — IMPLEMENTA reflection cumulative em incident template + post-mortem template em WI-S17-004; correlation_id captured em incident header + propagated em post-mortem timeline; facilita debugging + audit trail forensic-grade.
- **PAT-DEGRADE-001** (chaos test partial degradation) — IMPLEMENTA reflection em chaos scheduler WI-S17-001; chaos test simula partial degradation com graceful degradation expected; SLO impact measured + monitored.

### 6.2 Failure Modes (herda `failure_modes.md`)

- **FM-202** (runbook desatualizado em incident — P1 RPN 36) — IMPLEMENTA mitigation cumulative em PAT-RUNBOOK-DRILL-001 mensal cadence + drift detection > 2× expected = flag; RB-FM-202 dry-run mensal canonical (meta-drill).
- **26 FMs P0/P1 inventory** — IMPLEMENTA chaos catalog ≥ 8 FMs covered (subset of 26; remaining FMs covered post-GA per Completeness Criteria 10.s17.2); each chaos type maps a 1+ FM; reproducible seed + safe-mode threshold + reviewer SRE.
- **FM-150** (transient API): chaos test simulates transient API errors + retry verification.
- **FM-160** (auth invalid storm): chaos test simulates auth invalid storm + retry verification.
- **FM-203** (oncall sobrecarregado / fadiga → missed alert): IMPLEMENTA mitigation via fadigue tracking SEV-1 > 2/shift alert + manager review + 7d shift max + 2w protection period em WI-S17-005.

### 6.3 Observability Model (herda `observability_model.md §3.1`)

Chaos + DR drill + runbook + oncall métricas Prometheus snake_case (NÃO `plan` label aplicável em internal ops métricas; INV-OBS-CARDINALITY-BUDGET respeitado):

- `corelink_chaos_run_total{experiment, outcome, env}` (counter; experiment ∈ enum 8 chaos types; outcome ∈ pass|fail|aborted; env ∈ staging|prod).
- `corelink_chaos_safe_mode_abort_total{trigger, env}` (counter; trigger ∈ prod_sev1|staging_error_rate_high; env should always be staging).
- `corelink_dr_drill_total{cycle, outcome}` (counter; cycle ∈ 1|2|3; outcome ∈ pass|fail).
- `corelink_dr_drill_slo_impact_seconds{cycle, slo}` (gauge; SLO impact measured per cycle).
- `corelink_runbook_dry_run_total{runbook_id, outcome}` (counter; runbook_id ∈ enum RB-* IDs; outcome ∈ pass|drift_flagged|fail).
- `corelink_runbook_dry_run_duration_ratio{runbook_id}` (gauge; actual / expected duration; > 2.0 = drift flag).
- `corelink_oncall_sev1_per_shift{tier}` (gauge; tier ∈ 1|2|3; > 2 = alert).
- `corelink_oncall_sev2_per_shift{tier}` (gauge; > 5 = alert).
- `corelink_oncall_pages_per_month_total{tier, in_rotation}` (counter; > 10 in non-rotation = burnout signal).
- `corelink_incident_mtta_seconds{severity}` (gauge; severity ∈ sev1|sev2|sev3; SEV-1 target < 300s).
- `corelink_incident_mttr_seconds{severity}` (gauge; SEV-1 target < 1800s).
- `corelink_postmortem_total{severity, blameless}` (counter; blameless ∈ true|false — should always be true per culture enforcement).
- `corelink_game_day_total{quarter, outcome}` (counter; quarter ∈ q1|q2|q3|q4; outcome ∈ pass|fail).

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels** — these são ops internal métricas; tenant-agnostic).

### 6.4 SLO Catalog (herda `slo_catalog.md`)

- Chaos test **MEASURES** SLO impact via existing multi-burn-rate alerts (S-09 SEALED); chaos run report includes SLO error budget consumed.
- DR drill **MEASURES** SLO sustained durante failover; report includes SLO impact per cycle.
- MTTA/MTTR tracking **REPORTED** vs SLO targets (SEV-1 MTTA < 5min, MTTR < 30min canonical per spec contract §9.8).

### 6.5 Security Model (herda `security_model.md`)

- **Post-mortem para security incidents**: post-mortem template covers security category + 5-Why + action items; SRE lead + Security review.
- **Chaos data sanitization**: chaos test NUNCA expose PII em reports; redaction allowlist enforced; PAT-PRIV-001 reuse.
- **DR drill cycle 3** (BYOK key compromise simulated; deferred annual at GA via waiver) reuses S-14 BYOK + crypto-erase patterns; DR drill report sanitized.
- **Game day scenarios** include security drills (BYOK key compromise; supply chain attack typosquat; insider exfil) — manual tabletop only at GA.

### 6.6 Privacy Model (herda `privacy_model.md`)

- **Post-mortem privacy incidents**: post-mortem template covers privacy category + 5-Why + action items; Privacy officer review.
- **Chaos test never expose PII**: chaos report sanitized; redaction allowlist explicit fields only; never raw PII em chaos reports.
- **DR drill report sanitized**: PII-free; aggregate metrics only.
- **CTRL-PRIV-001** (zero PII em logs) enforced em chaos + DR + runbook + post-mortem reports.

## 7. Definition of Done (lane STANDARD; **two-phase SEAL D+20/D+50**)

> **Two-phase SEAL D+20 (Implementation) + D+50 (GA Evidence Gate)**: STANDARD lane base mas DoD §6 explicitly requires "4 weeks chaos sustained" + "monthly runbook dry-run" + "1 game day quarterly cadence start" + "30d fadigue baseline" — these are 30d+ observation criteria that cannot be instant-verified at sprint close. Split documented em `_spec_contract.md §13`.

### 7.1 Implementation SEAL D+20 (Sprint Review ceremony)

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **Chaos automation tooling** deployed em staging (Gremlin / chaos-mesh / Litmus integrated; CF Workers cron scheduler weekly orchestrator).
- [ ] **8 chaos types** scripted + safe-mode auto-abort enforced (env check `process.env.CHAOS_TARGET === 'staging'`; alert on any prod hit).
- [ ] **Chaos catalog** ≥ 8 FMs covered em `specs/05_quality/chaos/<experiment>.md` per FM coverage (EVT-018).
- [ ] **1st chaos run executed** em staging (deterministic seed; state captured pre/post; SLO impact measured) (EVT-023).
- [ ] **DR drill cycle 1 completed** em staging (simulate CF region outage + failover + SLO sustained; full report archived 7y) (EVT-023 + EVT-017).
- [ ] **3 runbook dry-runs** executados (RB-FM-051 + RB-FM-057 + RB-FM-202) com EVT-017 coletados (P0/P1) (EVT-017).
- [ ] **Incident template** + **post-mortem template** committed em `specs/_templates/` reviewed Engineering + SRE (EVT-016).
- [ ] **Post-mortem template tested** em 1 incident sintético (full flow; SEV-2 simulado em staging) (EVT-016).
- [ ] **Engineering all-hands training** delivered (1.5h session; blameless culture reinforce; 5-Why technique training).
- [ ] **PagerDuty schedule** publicado + rotation iniciada (Tier 1/2/3; shift ≤ 7d + 2w protection; 24/7 future-proof) (EVT-026 if exists; EVT-018 alternative).
- [ ] **Fadigue dashboard live**: SEV-1/shift, SEV-2/shift, total pages/month per oncall (EVT-021).
- [ ] **1 game day exercise** executed (4-hour tabletop session; 4 scenarios library) (EVT-023).
- [ ] **MTTA/MTTR tracking** instrumented (chaos test SEV-1 simulado verifies tracking baseline).
- [ ] **PRR STANDARD**: 5-8 sign-offs canonical (7 typical: SRE lead + Engineer + Oncall manager + Product + QA + Compliance officer + Privacy officer).
- [ ] **Métricas snake_case Prometheus**: 13+ ops métricas emitting em staging (`corelink_chaos_*` + `corelink_dr_drill_*` + `corelink_runbook_*` + `corelink_oncall_*` + `corelink_incident_*` + `corelink_postmortem_*` + `corelink_game_day_*`); INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels) (EVT-013).
- [ ] **PAT-RUNBOOK-DRILL-001** monthly cadence implementada.
- [ ] **PAT-CORRELATION-ID-001** in all incidents reflected em template.
- [ ] **PAT-DEGRADE-001** chaos test partial degradation reflected em scheduler.
- [ ] **Não introduz INVs novas** (sprint operational; per spec contract §8 mantidas only).
- [ ] **Cost regression gate**: ops infra ≤ $200/mês (chaos-mesh free + Grafana Cloud free tier amortized + PagerDuty free tier; UX workshop external developers $0 — internal team only).

### 7.2 GA Evidence Gate D+50 (post-sprint observation window 30d)

- [ ] **4 weeks chaos sustained** em staging sem incidents não-detectados (concurrent post-sprint observation period required for promotion gate per spec contract §13) (EVT-023 sustained).
- [ ] **Monthly runbook drill cadence sustained 30d** (3 dry-runs minimum em 30d; per Completeness Criteria 10.s17.4 — S-20 gate exige 90d).
- [ ] **30d fadigue baseline** established (SEV-1/shift + SEV-2/shift + pages/month metrics validated).
- [ ] **Game day quarterly cadence began** (1 game day executado; quarterly cadence sustained pós-GA per Quality Standard 14.s17.5).
- [ ] **Chaos test reproducibility verified** (deterministic seed; state captured pre/post; report archived 7y for compliance per Quality Standard 14.s17.1).
- [ ] **Post-mortem culture sustained** (no blame surfacing reviewed pelo SRE lead; sin name shaming; iterate per Quality Standard 14.s17.3).
- [ ] **Report final** assinado SRE lead + Final Approver + Compliance officer.

### 7.3 Completeness Criteria (delta local; per spec contract §7)

- [ ] **10.s17.1** Oncall schedule published; rotation começou (sustained 30d minimum to validate fadigue metrics) [GA Evidence Gate].
- [ ] **10.s17.2** Chaos test coverage ≥ 8 FMs do failure_modes §3 (cobertura crítica; remaining FMs covered post-GA) [Implementation SEAL].
- [ ] **10.s17.3** **DR drill semestral** scheduled + 1 cycle completed em staging [Implementation SEAL].
- [ ] **10.s17.4** **Runbook dry-run cadence**: monthly sustained 3 months minimum (S-20 gate exige 90d) [GA Evidence Gate].
- [ ] **10.s17.5** **Post-mortem blameless culture** documented + trained (engineering all-hands) [Implementation SEAL].
- [ ] **10.s17.6** **Game day exercise** quarterly cadence start [Implementation SEAL].
- [ ] **10.s17.7** **Chaos test reproducible** com deterministic seed; report archive 7y [GA Evidence Gate].

## 8. Dependencies

### Hard blockers

- **S-09 SEALED** (observability pra detectar chaos impact + multi-burn-rate alerts + audit events R2 bucket).

### Soft blockers

- **S-01..S-10 SEALED** (sistemas reais para gerar chaos contra).
- **S-13 SEALED** (admin plane permite chaos config flag — staging-only enforcement).
- **S-14 SEALED** (BYOK setup; required only for DR drill cycle 3 — deferred annual at GA via waiver opt).

### Outbound

- S-20 (GA exige P0/P1 priority subset ~25 of 47 dry-run em 90d + 4-week chaos test sustained + DR drill done + game day quarterly cadence; Lote 10.17 codex P0 canonical math fix — original "all 47/90d" era infeasible = ~16/month burdensome; revised subset = sustainable 8/month × 3 months covers ~25 P0/P1).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-09 SEALED hard blocker; ideally após S-13 SEALED para admin chaos config flag).
- **D+5**: WI-S17-001 SEALED (chaos automation + 1st run + chaos catalog ≥ 8 FMs).
- **D+8**: WI-S17-003 + WI-S17-004 SEALED (runbook dry-runs + templates + synthetic incident test).
- **D+12**: WI-S17-002 SEALED (DR drill 1st cycle completed).
- **D+15**: WI-S17-005 SEALED (oncall PagerDuty + fadigue tracking + dashboard).
- **D+18**: WI-S17-006 SEALED (game day + chaos catalog cleanup + closing PRR).
- **D+20**: **Implementation SEAL ceremony** (Sprint Review; PRR coletados; sprint review).
- **Post-sprint observation**: chaos automation continua weekly until S-20 GA gate (4 weeks total observation needed); monthly runbook drill cadence sustained; 30d fadigue baseline; game day quarterly cadence began.
- **D+50**: **GA Evidence Gate ceremony** (4-week chaos sustained + monthly runbook drill cadence sustained 30d + game day quarterly cadence began + chaos test reproducibility verified + post-mortem culture sustained + report final assinado).
- **Total**: 4 semanas (15-20 dias úteis) Implementation SEAL + 30d post-sprint observation = 50 dias total to GA Evidence Gate.

## 10. Risk Register

Ver `_spec_contract.md §15` (10 riscos: chaos test quebra staging dev cycle interrompido, oncall fadigue fatal leak to prod via missed page, runbook drift FM-202 não detectado, DR drill encontra unrecoverable failure, post-mortem culture violated blame surfacing, chaos test infra cost tooling overhead, game day quality variability, chaos in prod inadvertent CRITICAL, oncall handoff issues knowledge transfer, runbook URL not in alert payload).

## 11. Observability Plan

DASH-OPS-MATURITY (novo dashboard, internal SRE-only):
- Chaos run rate per experiment (`corelink_chaos_run_total{experiment}`); alert se 0 runs em 7d.
- Chaos safe-mode abort rate (`corelink_chaos_safe_mode_abort_total{trigger}`); CRITICAL alert se trigger=prod_sev1 (zero tolerance).
- DR drill outcome (`corelink_dr_drill_total{cycle, outcome}`); alert se outcome=fail.
- DR drill SLO impact (`corelink_dr_drill_slo_impact_seconds`); review per cycle.
- Runbook dry-run rate per runbook (`corelink_runbook_dry_run_total{runbook_id}`); alert se 8/month cadence missed (Lote 10.17 codex P0 canonical math fix; prior 1/month inconsistent com 3 dry-runs/30d sprint window).
- Runbook drift detection (`corelink_runbook_dry_run_duration_ratio`); flag se > 2.0 (FM-202 mitigation).
- Oncall SEV-1/shift (`corelink_oncall_sev1_per_shift`); alert > 2.
- Oncall SEV-2/shift (`corelink_oncall_sev2_per_shift`); alert > 5.
- Oncall pages/month (`corelink_oncall_pages_per_month_total{in_rotation}`); alert > 10 non-rotation = burnout signal.
- Incident MTTA (`corelink_incident_mtta_seconds{severity=sev1}`); alert > 300s.
- Incident MTTR (`corelink_incident_mttr_seconds{severity=sev1}`); alert > 1800s.
- Post-mortem rate (`corelink_postmortem_total{blameless}`); alert se blameless=false (culture enforcement).
- Game day rate (`corelink_game_day_total{quarter}`); alert se quarterly cadence missed.

Métricas listadas em §6.3 (13+); todas snake_case canonical; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels — internal ops métricas tenant-agnostic).

## 12. Security & Privacy

**STRIDE delta** (vs S-13/S-14/S-15/S-16 baseline):
- **Spoofing**: PagerDuty schedule auth via SSO + MFA; oncall handoff template + 15-min sync per shift change (knowledge transfer canonical); incident commander identified com auth.
- **Tampering**: chaos catalog em `specs/05_quality/chaos/` git-tracked + reviewer SRE; runbook dry-run logs em `_dry_run_log.md` git-tracked + EVT-017 immutable; post-mortem templates git-tracked.
- **Repudiation**: EVT-017 (RUNBOOK_EXECUTION) audit trail forensic-grade; EVT-023 (CHAOS_EXPERIMENT_REPORT) archived 7y for compliance; post-mortem committed git + reviewed SRE lead.
- **Information disclosure**: chaos data sanitization (no PII em chaos reports; CTRL-PRIV-001 reuse); DR drill report sanitized (aggregate metrics only); post-mortem privacy review pelo Privacy officer.
- **DoS**: chaos test safe-mode auto-abort se staging error rate > 50%; staging-only enforcement at code level; alert on any prod hit (CRITICAL post-mortem trigger).
- **Elevation of privilege**: chaos config flag em admin plane (S-13 reuse) requires admin auth + dual-approval (S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window); never chaos config via URL params.

**LINDDUN delta**:
- **Linkability**: chaos test runs internal ops métricas (no per-tenant labels; INV-OBS-CARDINALITY-BUDGET respeitado).
- **Identifiability**: chaos report sanitized; oncall manager review fadigue metrics aggregate (no individual targeting beyond manager review for burnout prevention).
- **Non-repudiation**: EVT-017 + EVT-023 forensic-grade audit trail; post-mortem committed git.
- **Detectability**: chaos safe-mode auto-abort + alerts; runbook drift detection > 2× expected = flag; oncall fadigue alerts.
- **Disclosure**: chaos config flag scope-limited admin plane S-13 reuse; never customer environments at GA.
- **Unawareness**: post-mortem blameless culture enforced + engineering all-hands training; oncall manager review fadigue + 1:1 conversation if burnout signal.
- **Non-compliance**: chaos test reproducibility 7y archive (SOC 2 + ISO 27001 baseline); DR drill report 7y archive; post-mortem culture (Atlassian / Google SRE Book Ch 15 reference); LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-ops-maturity.md`.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- Chaos test escapes safe-mode (impact prod) → CRITICAL post-mortem (zero tolerance; staging-only enforcement violated).
- DR drill fails recovery → 5-Why mandatório + runbook updates required.
- Oncall fadigue alert (> 2 SEV-1/shift) → manager review + system improvements.
- Runbook drift > 2× expected duration → 5-Why + runbook update.
- Post-mortem culture violation → engineering review + reinforcement training.
- Game day finding surfaces critical gap → action items prioritized + tracking.
- Chaos in prod inadvertent → CRITICAL post-mortem + Security review + staging-only enforcement reinforce.

## 14. Sign-off (STANDARD 5-8 canonical; 7 typical)

7 roles canonical para STANDARD lane (per framework §33.5.4 + spec contract §6 sign-off line): SRE lead + Engineer + Oncall manager + Product + QA + Compliance officer + Privacy officer.

**Lane rationale (STANDARD vs HIGH_RISK 11)**:
- Ops sprint não introduz tenant data path novo; consume SLO/observability já validated em S-09.
- Chaos enforcement é staging-only at GA — well-bounded surface (env check before chaos; staging-only enforcement at code level).
- Não há cripto-load-bearing controles novos (S-14 HIGH_RISK BYOK reuse only para DR drill cycle 3 deferred).
- Post-mortem culture é process discipline (não cripto-load-bearing).
- Oncall rotation é process discipline.
- Game day é process discipline.
- PAT-RUNBOOK-DRILL-001 reuse pattern já em resilience_patterns.md.
- Não há regulatory residency exposure (S-14 cobre WNAM/ENAM/WEUR/SAM; S-17 é internal ops discipline).

Compliance/AppSec/Architect com Crypto SME specialization são **NÃO mandatory canonical** em STANDARD lane (folded em PR review se applicable; SRE lead + Oncall manager canonical em S-17 dado scope chaos engineering + DR drills + runbook discipline + oncall rotation; Compliance + Privacy canonical dado scope post-mortem privacy/security incidents + chaos data sanitization + DR drill 7y archive for compliance).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-17 (cycle 12.S17.0; spec contract v1.1.0 STANDARD lane base; Ops Maturity Chaos Automation + DR Drills + Runbook Discipline + Oncall; 6 WIs; two-phase SEAL D+20 Implementation + D+50 GA Evidence Gate). |

---

**Fim de S-17 sprint contract.**
