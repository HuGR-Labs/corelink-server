---
id: "SPEC-CONTRACT-S17"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s17", "ops", "chaos", "runbooks", "standard"]
---

# Spec Contract — S-17: Runbooks Automation + Chaos + DR Drills

## 0. Metadata

| Sprint ID | S-17 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2.5 semanas | WIs | 5 |

## 1. Objetivo

Operacionalizar a disciplina SRE: automation de chaos experiments semanais em staging, DR drills semestrais, runbook dry-runs mensais (PAT-RUNBOOK-DRILL-001), incident template + retrospective machinery. Sem ops maturity, produção eventualmente falha silenciosamente.

## 2. Lane + forcing factors

- **Lane:** STANDARD.

## 3. Inherits_from

```yaml
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-OPS-001**: Chaos engineering automation (chaos-mesh ou custom).
- **CAP-OPS-002**: DR drill scheduler (semestral) + checklist runbook.
- **CAP-OPS-003**: Runbook dry-run tracker + evidence EVT-017.
- **CAP-OPS-004**: Incident template + post-mortem workflow.
- **CAP-OPS-005**: Oncall rotation + PagerDuty schedule + fadigue tracking.

## 5. Requirements específicos

- **R-S17-1**: Chaos scheduler em staging: inject latency/failures weekly por componente (R2, D1, Neon, edge).
- **R-S17-2**: DR drill semestral calendar + runbook execution tracker.
- **R-S17-3**: Runbook dry-run workflow (oncall executa 1 runbook P0/P1 por mês; output em EVT-017).
- **R-S17-4**: Incident template `specs/_templates/incident.md` (a criar); post-mortem template + retroactive sprint linking.
- **R-S17-5**: Oncall fadigue metric: SEV-1s por shift; alert se > 2.

## 6. DoD

- [ ] 5 WIs SEALED.
- [ ] 4 semanas de chaos tests executados em staging sem incidents não-detectados.
- [ ] 1 DR drill completo em staging (simulate CF outage).
- [ ] 3 runbook dry-runs com EVT-017 coletados.
- [ ] Post-mortem template testado em incident sintético.

## 7. Completeness (delta)

- [ ] **10.s17.1** Oncall schedule published; rotation começou.
- [ ] **10.s17.2** Chaos test coverage ≥ 5 FMs do failure_modes §3.

## 8. Invariants

- PAT-RUNBOOK-DRILL-001 executado monthly.
- PAT-CORRELATION-ID-001 in all incidents (facilita debugging).

## 9. Quality Standards

- Chaos tests reproducible (deterministic seed).
- Runbooks atualizados após dry-run se discrepância.
- Post-mortem blameless culture enforced.

## 10. Anti-scope

- ❌ Full SRE team hiring (post-GA demand-driven).
- ❌ Custom chaos framework — usar chaos-mesh ou existing tool.

## 11. Dependencies

- Blocker: S-09 (observability pra detectar chaos impact).
- Soft: S-01..S-10 (sistemas pra gerar caos contra).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S17-001 | Chaos scheduler + weekly tests staging |
| WI-S17-002 | DR drill tooling + semestral cadence |
| WI-S17-003 | Runbook dry-run tracker |
| WI-S17-004 | Incident + post-mortem templates |
| WI-S17-005 | Oncall schedule + fadigue metrics |

## 13. Duração

2.5 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + 4 weeks ops rhythm estabelecido.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Chaos test quebra staging → dev ciclo interrompido | M | LOW (é staging) |
| Oncall fadigue fatal (leak to prod) | L | HIGH |
| Runbook drift (FM-202) não detectado | M | MEDIUM |

---
