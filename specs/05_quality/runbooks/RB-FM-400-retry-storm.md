---
id: "RB-FM-400"
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
tags: ["runbook", "p1", "emergent", "circuit-breaker"]
---

# RB-FM-400 — Retry Storm Amplifica Outage Downstream

> **FM:** FM-400 (RPN=36, P1) | **PATs:** PAT-CIRCUIT-001 + PAT-JITTER-001 | **SLA:** mitigate < 30 min

## Detecção

- Métrica `corelink_retry_attempts_total` rate > 5× baseline.
- Downstream error rate sustained > 50% por 5+ min.
- Circuit breaker `corelink_circuit_state{target} = open` (gauge=1).

## Mitigação imediata

1. **Confirmar circuit breaker tripped** (deve ter aberto automaticamente).
2. Se não abriu (bug): manualmente trip via DO `circuit-<target>::open()`.
3. Reduzir rate limit global temporariamente (reduce burst).
4. Cliente recebe 503 com `Retry-After: 30` + jitter.

## Mitigação completa

1. Identificar root cause downstream (R2? D1? Neon? Stripe?).
2. Esperar recovery do downstream OU degrade-mode (PAT-DEGRADE-001).
3. Probe gradual via half-open state.
4. Re-enable após error_rate < 5% sustained 10 min.

## Forensics

- Por que retry attempts subiram tão rápido?
- Faltou backoff/jitter em algum caller?
- PAT-CIRCUIT-001 está configurado para target afetado?

## Prevenção

- Audit semestral de retry policies em todos os call sites.
- Chaos test mensal injeta latência downstream.
