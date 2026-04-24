---
id: "RB-FM-202"
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
tags: ["runbook", "p1", "operational", "meta"]
---

# RB-FM-202 — Runbook Desatualizado em Incident

> **FM:** FM-202 (RPN=36, P1) | **PAT:** PAT-RUNBOOK-DRILL-001 | **SLA:** atualizar runbook ≤ 24h pós incident

## Detecção (durante incident)

- Oncall reporta "este passo do runbook não funciona / API mudou / serviço renomeado".

## Mitigação imediata

1. Marcar runbook como `STALE` (header) + abrir issue/ticket.
2. Improvisar mitigação documentando passos que funcionaram.
3. Capturar screenshots/output do que mudou.

## Mitigação completa (≤ 24h)

1. PR atualizando runbook com correções.
2. Bump version (patch).
3. Re-validate executando dry-run em staging (EVT-017).
4. Se mudança fundamental: ADR + revisão de FMs relacionados.

## Prevenção

- PAT-RUNBOOK-DRILL-001: oncall faz 1 dry-run de runbook P0/P1 por mês.
- Output do dry-run em EVT-017 obrigatório.
- Runbook sem dry-run > 6 meses → marcar `audit_status: AUDIT_PENDING`.
