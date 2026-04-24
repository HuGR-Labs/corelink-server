---
id: "RB-FM-403"
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
tags: ["runbook", "p1", "container", "memory"]
---

# RB-FM-403 — Memory Leak Latente em Long-running Container

> **FM:** FM-403 (RPN=36, P1) | **PAT:** PAT-RESTART-JIT-001 | **SLA:** mitigate < 1h

## Detecção

- Métrica `container_memory_rss_bytes` cresce monotonicamente sem release.
- OOM kill counter > 0.
- p99 latência sobe gradativamente em uma região.

## Mitigação imediata

1. Restart graceful do container afetado (PAT-RESTART-JIT-001 com jitter).
2. Verificar `container_uptime_seconds` — se > 24h, candidato.
3. Cap restart rate: max 1 container por região por minuto (não derrubar todos).

## Mitigação completa

1. Heap dump + analyse (se possível em CF Containers; pode requerer attach).
2. Identificar leak source: novo dep? código novo recente?
3. Patch + deploy progressive rollout.

## Prevenção

- PAT-RESTART-JIT-001 default: restart após 1000 requests OU 4 horas (com jitter ±20%).
- Métrica de heap watermark em dashboard semanal.
- Profile guided benchmarks em release pipeline.
