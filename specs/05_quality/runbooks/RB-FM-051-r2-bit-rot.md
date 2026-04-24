---
id: "RB-FM-051"
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
tags: ["runbook", "p1", "data-integrity", "cas"]
---

# RB-FM-051 — R2 Bit Rot Detected (Hash Mismatch on Read)

> **FM:** FM-051 (S=5, O=1, D=4, RPN=20, P1 S=5→upgrade) | **CTRLs:** CTRL-CAS-002 + scrub | **SLA:** mitigate < 1h

## Detecção

- Métrica `corelink_cas_get_hash_mismatch_total{tenant_id}` > 0 (alert SEV-1).
- Client report via support: "blob downloaded mas hash diverge".
- Scrub job emite `corelink_scrub_mismatch_total` > 0.

## Comunicação imediata

- Page oncall + status page (degraded, sem detalhe técnico).
- Notificar Security Lead (potencial cache poisoning vs. genuíno bit rot).

## Mitigação imediata (≤ 15 min)

1. Quarantine o blob: marcar `blob_meta.quarantined_at = now`; reads retornam 503 com `Retry-After: 3600`.
2. Verificar checksum em backup (se houver replica via PAT-REGION-FAILOVER-001).
3. Se replica OK: re-write a partir da replica + remover quarantine.
4. Se sem replica: notificar tenant; flagar AC entries afetadas.

## Mitigação completa (≤ 1h)

1. Trigger scrub completo do prefix afetado (todos blobs do tenant ou da região).
2. Compare hash de cada blob contra `blob_meta.expected_digest`.
3. Quarantine + re-write para todos os mismatches.

## Root cause investigation

- R2 logs do bucket: corruption events via CF dashboard.
- Pattern temporal: bit rot é raro; cluster temporal sugere hardware ou supply chain.
- Verificar últimas mudanças em GC, scrub job, ou multipart upload.

## Post-mortem

- Trigger automático após resolução: `incident.md` com FM-051 ref.
- Revisitar O score se observed > predicted.
