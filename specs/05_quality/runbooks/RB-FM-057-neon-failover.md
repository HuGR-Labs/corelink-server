---
id: "RB-FM-057"
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
tags: ["runbook", "p2", "storage", "neon"]
---

# RB-FM-057 — Neon Primary Failover

> **FM:** FM-057 (RPN=16, P2) | **CTRLs/PATs:** PAT-CIRCUIT-001 + PAT-DEGRADE-001 | **SLA:** mitigate < 30 min

## Detecção

- `corelink_neon_query_errors_total{error_code="UPSTREAM_NEON"}` spike repentino.
- `corelink_neon_primary_latency_seconds` p99 > 3s sustained.
- Neon dashboard: primary em state `standby` promoting.
- Customer reports em billing/accounts (não em CAS hot path — esse é D1).

## Comunicação

- SEV-2 (billing/control plane impactado, cache hot path OK).
- Slack `#oncall`; status page: degradation em "account management".

## Mitigação imediata (≤ 5 min)

1. Circuit breaker `PAT-CIRCUIT-001` deve trip automaticamente para `neon`.
2. Worker entra em `degrade_mode=read-only` para ops que dependem de Neon (ex: signup, billing updates).
3. CAS/AC continuam normais (D1 é source de verdade para hot path).

## Mitigação completa (≤ 30 min)

1. Aguardar promoção completa (30-90s típico Neon).
2. Circuit breaker probe restaura tráfego gradualmente.
3. Validar consistency: queries recentes vs replicação concluída.
4. Re-enable degrade_mode quando Neon stable ≥ 5 min.

## Forensics

- Neon support ticket se failover > 5 min.
- Root cause Neon (hardware, planned, misconfiguration).
- Connection pool exhaustion (FM-058)? Check `corelink_neon_pool_inflight`.

## Prevenção

- Chaos test mensal: inject Neon primary failure via `kill-primary` game day.
- Read replicas em região secundária para queries não-crítical-path.
- Alert em `primary_latency > 1s p99` (early warning).
