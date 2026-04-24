---
id: "RB-HSM-UNAVAILABLE"
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
tags: ["runbook", "p1", "crypto", "availability"]
---

# RB-HSM-UNAVAILABLE — HSM / Cloudflare Secrets Outage

> **Trigger:** Root HSM (CF Workers Secrets) unavailable; key operations failing.
>
> **SLA:** read-only mode ≤ 1 min; recovery depende do vendor.

## Detecção

- Métrica `corelink_hsm_errors_total` spike.
- `corelink_key_unwrap_duration_seconds` p99 > 5s sustained.
- CF status page reporta issues em Workers Secrets / KMS region.

## Comunicação

- SEV-1 (blast radius = todo CoreLink).
- Page SRE Lead + Security Lead.
- Status page: `major degradation` ("authentication & encryption temporarily unavailable").
- Expor ETA se CF status page tem.

## Mitigação imediata (≤ 1 min)

1. **Automatic read-only mode** (PAT-DEGRADE-001 `degrade_mode=read-only`).
   - Reads continuam servindo cache (keys já unwrapped em memory por TTL curto).
   - Writes retornam 503 `Retry-After: 60`.
2. Alert customers via webhook + status page.
3. Escalate para Cloudflare support se outage > 5 min.

## Mitigação completa (depende do vendor)

1. Aguardar recovery do CF HSM.
2. Após recovery:
   - Probe key unwrap em 3 regiões para confirm.
   - Reactivate writes gradualmente.
   - Monitor `key_unwrap_duration` por 30 min antes de claim "OK".
3. Post-outage scrub: verificar que nenhum key cache em memória expirou sem refresh.

## Forensics

- Cloudflare support ticket: root cause + duração total.
- Métricas de impact: quantos writes negados? Quantos customers afetados?
- Preservar evidence: CF dashboard screenshots + status page history.

## Notificação

- Comunicação pública em status page (real-time).
- Post-mortem em ≤ 14d se outage > 1h.
- SLA credits para enterprise customers conforme contrato.

## Prevenção & resilience

- Key cache em memory (TTL 60s) permite reads continuarem durante outage curto.
- Multi-region HSM disponível via CF (verify with vendor).
- Tabletop exercise semestral: simular HSM outage.
- Contract SLA com Cloudflare para HSM availability ≥ 99.99%.
