---
id: "RB-FM-100"
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
tags: ["runbook", "p1", "network", "dns"]
---

# RB-FM-100 — DNS Outage (Registrar ou CF DNS)

> **FM:** FM-100 (S=5, P1 S=5→upgrade) | **PATs:** PAT-DNS-TTL-001 | **SLA:** comms ≤ 5 min; mitigate depende vendor

## Detecção

- Synthetic monitor externa (Pingdom/Better Uptime) reporta `NXDOMAIN` ou `SERVFAIL` em `cache.corelink.humangr.com`.
- Métrica interna `corelink_synthetic_probe_success_ratio` cai a 0 de múltiplas regiões.
- CF status page reporta DNS issue OU registrar reporta outage.

## Comunicação

- **SEV-1.** Page SRE Lead + Communications.
- Status page IMEDIATO: "Customers can't reach cache.corelink.humangr.com due to DNS outage at [vendor]".
- Twitter/X, Slack (tenants enterprise), email blast.
- Se registrar próprio (ex: customer domains apontando para nosso CNAME): customer success notify.

## Triage (≤ 5 min)

1. `dig @8.8.8.8 cache.corelink.humangr.com` — funciona?
2. `dig @1.1.1.1 cache.corelink.humangr.com` — funciona?
3. `whois cache.corelink.humangr.com` — registrar respondendo?
4. Identificar: é CF DNS? É registrar? É nosso?

## Mitigação imediata

### Se CF DNS outage

- Aguardar CF resolver; nada pra fazer do nosso lado.
- Comms contínuo.

### Se registrar outage

- CloudFlare tem secondary DNS? (verificar config).
- Se sim: activate failover.
- Se não: criar ticket emergência com registrar + escalate.

### Se config mudou (TTL, CNAME errado)

- Revert last terraform apply (provavelmente origem).
- PAT-DRIFT-DETECTION-001 deveria ter alertado antes.

## Mitigação completa

- Aguardar recovery.
- Pós recovery: probar de múltiplas regiões + TTL warming via traffic.

## Forensics

- Vendor post-mortem (CF status page).
- Análise: quanto do nosso traffic foi afetado? TTL cache mitigou?

## Prevenção

- PAT-DNS-TTL-001: TTL 300s em records críticos (não 86400s default).
- Secondary DNS provider configurado (via CF multi-provider feature OR manual).
- CAA record pinned (CTRL-NET-002).
- Synthetic monitor 24/7 de múltiplas regiões.
- Semestral DR drill: simular DNS outage.
