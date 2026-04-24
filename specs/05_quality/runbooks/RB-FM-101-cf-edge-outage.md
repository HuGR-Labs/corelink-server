---
id: "RB-FM-101"
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
tags: ["runbook", "p1", "cloudflare", "outage"]
---

# RB-FM-101 — Cloudflare Edge Outage (Região ou Global)

> **FM:** FM-101 (S=5, P1 S=5→upgrade) | **CTRLs:** status page + comms plan | **SLA:** comms ≤ 5 min; recovery depende vendor

## Contexto

Cloudflare edge é a dependência mais crítica do CoreLink. Outage regional ou global = CoreLink degradado ou offline. **Nada fazemos direto**; resposta é orçamento de disponibilidade + comunicação + failover quando possível.

## Detecção

- Status page CF reporta incident.
- Synthetic monitor múltiplas regiões falha simultaneamente.
- Customer floods em support.
- Métrica `corelink_edge_5xx_rate` spike simultâneo em todas regiões.

## Comunicação

- **SEV-1 imediato.**
- Status page: reconhecer + link ao CF status.
- Twitter/X blast.
- Enterprise customer Slack/email: comms personalizado dentro de 15 min.

## Triage (≤ 5 min)

1. CF status page: qual serviço? Qual região?
2. Nosso synthetic monitor de outras regiões: quais ainda funcionam?
3. Workers / R2 / D1 afetados? (alguns podem estar isolados do edge).

## Mitigação imediata

### Outage regional CF

- PAT-REGION-FAILOVER-001 cobre reads de blobs hot (top 1%) em região secundária.
- Writes continuam na primary (mesmo com slower routing).
- Customers enterprise com BYOK podem ter latência aumentada devido à CMK roundtrip.

### Outage global CF

- Sem failover possível (somos 100% CF stack).
- `degrade_mode=emergency`: apenas health endpoint; tudo mais 503 `Retry-After: 600`.
- Aguardar recovery; sem recurso técnico.

## Mitigação completa

- Aguardar CF recovery.
- Pós recovery: probe de todas regiões; verificar consistency (R2 eventual + KV global).
- Reconcile diário fará catch-up em usage events (PAT-RECONCILE-001).

## Forensics

- CF post-mortem (normalmente publicado em 14d).
- Análise de nosso blast radius: quantos customers afetados? Quanto tempo? Quanto SLO budget consumido?
- SLA credits conforme contrato enterprise.

## Post-mortem interno

- Obrigatório se > 1h ou se > 5% budget de availability consumido.
- Public incident report em ≤ 14d.

## Prevenção & resilience

- CF é single-vendor risk. Aceitar ou diversificar (Fase 2+ roadmap).
- Monitor CF status page changes (automated).
- Contract SLA CF enterprise (99.99% uptime).
- Public transparency: publicar dependency disclosure em `/privacy/sub-processors`.
- Customer enterprise que precisam de multi-cloud: BYOK com secondary provider pode mitigar parte.
