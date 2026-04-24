---
id: "RB-FM-206"
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
tags: ["runbook", "p1", "operational", "iac"]
---

# RB-FM-206 — Terraform Drift (estado real ≠ definido)

> **FM:** FM-206 (RPN=36, P1) | **PAT:** PAT-DRIFT-DETECTION-001 | **SLA:** reconciliate < 24h

## Detecção

- Drift detection daily job emite `corelink_terraform_drift_count > 0`.
- Manual: `terraform plan` em staging mostra diff inesperado.

## Comunicação

- Slack `#oncall` SEV-3 (não bloqueia produto).
- Tag dev que aplicou mudança recente (via git blame em mudanças .tf).

## Mitigação

### Caso 1: Drift acidental (config mudou em CF dashboard fora do TF)

1. Snapshot do estado real (`terraform refresh`).
2. Decidir: revert para state TF (preferred) ou import do drift para state TF.
3. PR aplicando decisão com comment do contexto.

### Caso 2: Drift intencional não-documentado (alguém mudou via dashboard pra fix urgente)

1. Confirmar com author via Slack/PR comment.
2. Documentar mudança no .tf + import.
3. Post-mortem leve: por que a mudança não foi via PR?

### Caso 3: Drift malicioso

1. Escalate para Security Lead imediatamente.
2. Audit log do CF dashboard: who + when.
3. Se confirmado adversarial: incident SEV-1.

## Prevenção

- CTRL-AUDIT-002: todas mudanças de config (CF dashboard) logged.
- CI roda `terraform plan` em PRs touching .tf.
- Daily diff job alerta no canal.
