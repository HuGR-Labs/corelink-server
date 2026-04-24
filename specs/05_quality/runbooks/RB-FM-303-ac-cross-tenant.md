---
id: "RB-FM-303"
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
tags: ["runbook", "p1", "tenant-isolation", "data-integrity"]
---

# RB-FM-303 — AC Entry Aponta Para Blob de Outro Tenant

> **FM:** FM-303 (S=5, P1 S=5→upgrade) | **CTRLs:** INV-TENANT-ISOLATION + integration test | **SLA:** mitigate < 15 min (cross-tenant!)

## Detecção

- Property test em CI falha cross-tenant assertion.
- Métrica `corelink_isolation_assertion_total{outcome="violation"} > 0` (alert SEV-1 imediato).
- Customer report (raríssimo, mas possível em pre-prod).

## Comunicação

- **SEV-1 imediato.** Page Security Lead + Architect + SRE.
- Status page: degraded (sem detalhe sobre isolation).
- Comms preparados para customer notification se confirmado em prod.

## Mitigação imediata (≤ 15 min)

1. **Disable AC writes globalmente** via degrade_mode flag (`PAT-DEGRADE-001` cache-only).
2. Snapshot dos AC entries afetados (D1 query); preserve evidence.
3. Identificar tenant_id afetado (vítima + contaminador).
4. Quarantine tenant contaminador (suspend writes).

## Mitigação completa (≤ 4h)

1. Patch hot-fix do bug específico (path validation, HMAC derivação).
2. Re-validar TODOS AC entries via batch job: `expected_tenant_hmac == observed`.
3. Quarantine entries com mismatch.
4. Re-enable AC writes apenas após patch verificado em staging.

## Forensics

1. Audit log: quem escreveu o AC entry inválido? Quando?
2. Se PAT comprometido: revoke + investigar uso.
3. Se bug de código: git blame + revisão de PR responsável.

## Notificação obrigatória

- **Tenant vítima**: dado pode ter sido exposto. Email formal + DPA reference.
- **ANPD/DPA** (se confirmado data exposure): conforme `RB-BREACH-NOTIF`.
- **Tenant contaminador**: provável bug, não maliciosidade; informe.

## Post-mortem

- TLA+ spec INV-TENANT-ISOLATION precisa cobrir o cenário que foi violado.
- Adicionar regression test ao property test suite.
- Considerar promoção para FF-HR-002 + revisar code review process.
