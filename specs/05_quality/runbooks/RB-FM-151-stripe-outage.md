---
id: "RB-FM-151"
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
tags: ["runbook", "p2", "billing", "stripe", "vendor-outage", "stub"]
---

# RB-FM-151 — Stripe Outage (Billing Vendor Down)

> **FM:** FM-151 (S=3, O=2, D=2, RPN=12, P2) | **CTRL:** CTRL-BILLING-001, PAT-QUEUE-EVENTS-001 | **SLA:** mitigate ≤ 30 min

## Detecção

- Stripe status page reports incident <https://status.stripe.com>.
- Métrica `corelink.billing.stripe_api_calls_total{status="5xx"}` spike.
- Webhook delivery failures.

## Comunicação

- **SEV-2** (não SEV-1: events queued; eventual consistency).
- Page SRE on-call + Engineer billing.
- Internal channel; não customer notification (transparency baixa).

## Mitigação imediata

1. Confirmar Stripe outage via status page.
2. PAT-QUEUE-EVENTS-001 ativa: events queued em R2 events bucket.
3. PAT-BACKOFF-001 aplicado em retries Stripe API: exponential 1s/2s/4s/8s/16s.
4. Verificar no customer charges/refunds/disputes pendentes que afetem trust.

## Resolução

- Aguardar Stripe recovery.
- Verificar reconciliation post-recovery: queue drained 100% sem dup.
- Audit chain integrity preserved.

## Post-incident

- Post-mortem se outage > 1h.
- Review reconciliation drift.
- Stripe SLA tier review.

## References

- `failure_modes.md` FM-151.
- `specs/04_sprints/S10/_spec_contract.md` (billing).
- Stripe status: <https://status.stripe.com>.
