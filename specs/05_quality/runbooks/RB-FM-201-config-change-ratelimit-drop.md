---
id: "RB-FM-201"
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
tags: ["runbook", "p2", "config", "rate-limit", "rollback"]
---

# RB-FM-201 — Config Change Causa Rate-Limit Drop

> **FM:** FM-201 (S=3, O=3, D=2, RPN=18, P2) | **CTRL:** CTRL-RATE-001, PAT-DUAL-APPROVAL-001 | **SLA:** rollback ≤ 5 min

## Detecção

- Alert `corelink_rate_limit_rejects_total` spike imediatamente pós-deploy.
- Customer reports: "everything is being throttled".
- Metric `corelink_rate_limit_config_version` mudou nos últimos 5 min.
- SLO breach `SLO-AVAIL-CAS-PUT` ou `SLO-AVAIL-CAS-GET`.

## Comunicação

- **SEV-2** (não SEV-1: causa conhecida + reversível).
- Page SRE on-call + Engineer responsável pelo deploy.
- Internal channel `#incidents-corelink`.
- Customer notification se impact > 5 min.

## Mitigação imediata (≤ 5 min)

1. **Identificar config change**: query `config_change_log` D1 últimos 30 min.
2. **Auto-rollback** (PAT-AUTO-ROLLBACK-001 já configurado):
   - Se rate-limit rejects spike > 10× baseline em < 2 min pós-deploy → auto-rollback.
   - Manual override `wrangler rollback --to-config-version <prev>`.
3. **Verificar dual-approval** (PAT-DUAL-APPROVAL-001): config change passou pelos 2 approvers obrigatórios?
4. **Customer-facing**: status page degraded; notify customers afetados.

## Diagnóstico

1. Diff config: novo vs anterior; identify dropped/lowered limits.
2. Cross-reference: dual-approval workflow violation? Code review missed limit change?
3. Test in staging com mesmo config → reproduzir.

## Resolução

- Hot fix: rollback completed; deploy fix com correct limits.
- Cold fix:
  - Reforçar PAT-DUAL-APPROVAL-001 com schema validation rejecting limit drops > 50% sem ADR.
  - Add canary deploy: 5% traffic → wait 10 min → measure rejects → progress se ok.
- Post-mortem dentro de 7d.

## Post-incident

- Update CTRL-RATE-001 deployment safety checks.
- Review process: como dual-approval falhou? Reviewer treinou?
- Adicionar property test cobrindo "config diff > X% em limits → require explicit ADR".

## Evidence

- Config diff (previous vs new).
- Rate-limit rejects timeline.
- Dual-approval audit log.

## References

- `failure_modes.md` FM-201 entry.
- `resilience_patterns.md` PAT-DUAL-APPROVAL-001, PAT-AUTO-ROLLBACK-001.
- `specs/04_sprints/S08/_spec_contract.md`.
