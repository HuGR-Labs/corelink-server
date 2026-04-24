---
id: "RB-FM-205"
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
tags: ["runbook", "p1", "operational", "data-integrity"]
---

# RB-FM-205 — Manual Intervention Apaga Dado (Admin Mistake)

> **FM:** FM-205 (S=5, RPN=30, P1) | **PATs:** PAT-DUAL-APPROVAL-001 + PAT-SOFT-DELETE-001 | **SLA:** recover ≤ 1h

## Detecção

- Alert `corelink_admin_unexpected_delete_total > 0`.
- Admin auto-reporta (audit log com `op=admin.delete`).
- Customer reporta "meus dados sumiram".

## Comunicação

- **SEV-1.** Page Architect + SRE Lead + Privacy Officer (se PII envolvido).
- Comms preparados: customer notification + internal post-mortem candidate.

## Mitigação imediata (≤ 15 min)

1. Pausar admin destructive operations globalmente via kill-switch.
2. Verificar soft-delete grace (24h default para metadata, 72h para CAS): se ainda dentro, **undelete via tombstone reversão**.
3. Se já physical delete: iniciar recovery de backup (R2 versioning / Object Lock).

## Mitigação completa (≤ 1h)

1. Restore dos objetos afetados (R2 versioning GET old version).
2. Rebuild metadata em D1/Neon a partir de audit log + event sourcing.
3. Validar integridade: `hash(restored_body) == expected_digest` (CTRL-CAS-001).
4. Reativar admin ops APENAS após root cause conhecido.

## Forensics

1. Audit log: quem executou? Quando? Justificativa?
2. PR/ticket que autorizou a operação existe? Dual-approval foi cumprido?
3. Se dual-approval falhou: revisar CTRL-AUDIT-003 + RB-FM-206 (drift).

## Comunicação ao customer

- Se dado customer afetado: email formal em ≤ 24h com:
  - O que aconteceu
  - Dados afetados
  - Status da recuperação
  - Ações tomadas para prevenção

## Prevenção

- PAT-DUAL-APPROVAL-001 mandatório para destructive ops; zero exceções.
- Tabletop exercise anual: simular admin mistake + recovery.
- CTRL-AUDIT-003: MFA attestation + session recording para admin ops.
