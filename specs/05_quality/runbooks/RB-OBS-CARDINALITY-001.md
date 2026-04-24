---
id: "RB-OBS-CARDINALITY-001"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "observability", "cardinality", "cost-control"]
---

# RB-OBS-CARDINALITY-001 — Cardinality Explosion (Métrica → OOM Mimir / Cost Spike)

> **INV:** INV-OBS-CARDINALITY-BUDGET HIGH | **CTRL:** CTRL-OBS-001 | **SLA:** detect ≤ 1h, mitigate ≤ 6h

## Detecção

- Alert `metric_cardinality_budget_breach_pct > 80%` per métrica.
- Grafana Mimir tenant 429 (over-quota).
- Cost report: Grafana spending up > 30% MoM sem explicação proporcional.
- Worker analytics emit warnings: "cardinality limit reached".

## Comunicação

- **SEV-2** (não SEV-1: produção continua; observability quality degradada + cost explosion).
- Page SRE on-call + Engineer responsável pelo subsystem afetado.
- Internal channel `#incidents-corelink-observability`.
- Não requer customer notification.

## Mitigação imediata (≤ 1h)

1. **Identify offending metric**: top-N por unique series count em Mimir tenant.
2. **Identify offending labels**: frequently `tenant_id × region × op` cartesian explosion.
3. **Block ingestion** of high-cardinality label temporariamente:
   - Mimir tenant config: drop label via relabel rule.
   - Alternativa: workers emit menos detalhe (downgrade level).
4. **Validate cost impact estimated**: Grafana billing API → projected month cost.

## Diagnóstico (≤ 6h)

1. Root cause:
   - **Label novo adicionado sem CI check** (PR slipped past cardinality_check.py).
   - **Bug**: label conteúdo não-bounded (e.g., `error_message` literal in label vs metric attr).
   - **Tenant abuse**: sintético de tenant gerando millions de unique values.
   - **Schema migration**: enum field expandido sem awareness.
2. Cross-reference cardinality budget definido em `observability_model.md §11.2`.
3. Mimir queries: time-series retention vs ingest rate.

## Resolução

- **Hot fix** (≤ 24h):
  - Drop label via relabel rule (irreversible past data).
  - Fix code that emit unbounded label.
  - Roll back PR if recente.
- **Cold fix** (≤ 7d):
  - Strengthen `cardinality_check.py` CI gate (false positive em PR review missed).
  - Adicionar pre-deploy canary (10% traffic, monitor cardinality 1h, then rollout).
  - Update INV-OBS-CARDINALITY-BUDGET enforcement (Mimir tenant hard limit).
  - Adicionar `error_message` allowlist of generic patterns vs literals.

## Post-incident

- Post-mortem mandatório (Lote 9.1 post-mortem hook).
- 5-Why: por que CI cardinality_check missed? Por que budget budget set incorrectly?
- Update `observability_model.md §11.2` se budget revisitado.
- Treinar engineering: cardinality discipline em SRE workshop.

## Evidence

- Mimir cardinality dashboard pre/post fix.
- Cost report (Grafana billing).
- PR/commit que introduziu label problemático.
- CI cardinality_check.py logs.

## References

- `observability_model.md §11.2` cardinality budget.
- `invariant_registry.md` INV-OBS-CARDINALITY-BUDGET (S-09).
- `specs/04_sprints/S09/_spec_contract.md`.
- Grafana Mimir cardinality limits: <https://grafana.com/docs/mimir/latest/configure/about-tenant-ids/>.
- PromCon 2022 — "Cardinality is your enemy" talk.
