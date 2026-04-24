---
id: "RB-SLO-AVAIL-CP"
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
tags: ["runbook", "slo", "availability", "control-plane"]
---

# RB-SLO-AVAIL-CP — Control Plane Availability SLO Burn

> **SLO:** SLO-AVAIL-CP (slo_catalog §4.1); target `team` = 99.9%, `enterprise` = 99.95%.
>
> **Alert:** multi-burn-rate 5m/1h/6h/24h (observability_model §9.3).

## Detecção

- SEV-1: burn 5m > 14.4 sustained (fast-burn).
- SEV-2: burn 1h > 6 sustained.
- SEV-3: burn 24h > 2 sustained.

## Dashboard

- `DASH-GLOBAL-HEALTH` (observability_model §8): burn rates + error budget remaining.

## Step-by-step

### SEV-1 fast-burn (≤ 5 min response)

1. **Confirma sinal**: `corelink_cp_requests_total{status=~"5.."} / total` > threshold em múltiplas regiões? Ou isolado?
2. Identificar componente afetado:
   - Worker CPU timeouts (FM-001)? → `corelink_handler_cpu_ms` p99
   - DO migration (FM-005)? → `corelink_do_state_migration`
   - Neon outage (FM-057)? → `corelink_neon_errors`
   - CF edge outage (FM-101)? → `CF status page`
3. Acionar runbook FM-específico.
4. Se desconhecido: `degrade_mode=read-only` enquanto investiga.

### SEV-2 slow-burn (≤ 30 min response)

1. Error budget burn sustained — não é flash mas persistente.
2. Check rollout recente: houve deploy nas últimas 24h? Rollback candidate?
3. PAT-PROGRESSIVE-ROLLOUT-001 auto-rollback deveria ter disparado — verificar.
4. Se não é deploy-related: abrir investigation parallel + degrade mode preventivo.

### SEV-3 slow burn (1 business day response)

1. Budget burn constante mas baixo.
2. Investigar root cause sem pressão:
   - Dependency drift (CVE HIGH em dep)?
   - Customer com PAT abusive (CTRL-RATE-001)?
   - Regional capacity issue?

## Forensics

- Post-mortem automático se burn > 10% em 24h (consome > 2.5 dias de budget).
- Link SEV-1 incidents a FMs específicos.

## Comunicação

- SEV-1 → status page `major`; customers enterprise notificados.
- SEV-2 → status page `minor degradation`; Slack `#oncall`.
- SEV-3 → ticket interno + Slack heads-up.

## Prevenção

- Chaos test mensal em staging para cada componente do CP.
- Error budget policy (slo_catalog §5): freeze se budget esgotado antes do fim do window.
- Progressive rollout cobre deploy risk (PAT-PROGRESSIVE-ROLLOUT-001).
