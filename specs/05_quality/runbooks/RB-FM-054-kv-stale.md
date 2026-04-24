---
id: "RB-FM-054"
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
tags: ["runbook", "p1", "consistency", "kv"]
---

# RB-FM-054 — KV Eventual Consistency Stale > 60s

> **FM:** FM-054 (RPN=30, P1) | **CTRL/PAT:** PAT-KV-TTL-001 | **SLA:** mitigate < 30 min

## Detecção

- Métrica `corelink_kv_stale_age_seconds{key_prefix}` p99 > 60s.
- Customer reporta "deletei meu PAT mas ainda funciona".

## Comunicação

- Slack `#oncall`; SEV-2 (não bloqueia produto).

## Mitigação imediata

1. Forçar invalidação via `KV.delete()` em todas regiões via DO broadcast.
2. Se PAT revocation: também atualizar D1 `pat.revoked_at` (source of truth).
3. Cliente reportando outubro: dizer "tente em 60s; se persistir, escalate".

## Mitigação completa

1. Verificar TTL configurado < 300s (PAT-KV-TTL-001).
2. Verificar que KV não é source of truth para nada crítico (D1/Neon são).
3. Se houve uso indevido (KV como source of truth): WI urgente para mover.

## Post-mortem

- Se recorrente: avaliar mover dados afetados para DO ou D1.
