---
id: "RB-FM-302"
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
tags: ["runbook", "p1", "billing", "financial", "integrity"]
---

# RB-FM-302 — Billing Counter Não Incrementa (Silent Revenue Leak)

> **FM:** FM-302 (RPN=30, P1) | **CTRLs:** CTRL-BILLING-001 + PAT-RECONCILE-001 | **SLA:** reconcile < 24h

## Detecção

- Reconcile diário emite `corelink_reconcile_drift{counter="usage"} > 0.1%`.
- Customer report: "minha fatura está menor/maior do que esperado".
- Métrica `corelink_billing_event_age_seconds` p99 > 15 min (SLO-FRESH-BILLING breach).
- CF Analytics vs nosso usage_counter diff > 1%.

## Comunicação

- **SEV-1** (revenue + customer trust impact).
- Page Finance + SRE + CEO (se drift > $1k/dia impacto).
- Legal + Privacy on standby (se precisa refund corrections).

## Mitigação imediata (≤ 1h)

1. **Pausar billing posting para Stripe** via config flag (evita double-charge no fix).
2. Identificar scope:
   - `SELECT tenant_id, period, metric, value FROM usage_counter WHERE updated_at > since_drift;`
   - Compare com events append-only em R2 `events/` bucket.
3. Se counter < events (under-counting): revenue leak — reconstruir counter.
4. Se counter > events (over-counting): customer over-charged — preparar refund.

## Mitigação completa (≤ 24h)

1. **Rebuild counters a partir de events** (R2 é source of truth para billing):
   ```sql
   WITH recomputed AS (
     SELECT tenant_id, period, metric, SUM(value) as real_value
     FROM events_unrolled
     WHERE period >= :affected_since
     GROUP BY tenant_id, period, metric
   )
   UPDATE usage_counter SET value = recomputed.real_value
   FROM recomputed WHERE (tenant_id, period, metric) MATCH;
   ```
2. Re-enable Stripe posting com retention de eventos não-postados.
3. Dry-run dos posts reconciliados em staging antes de prod.
4. Comunicar customers afetados (se billing mudou material).

## Forensics

1. Root cause:
   - Event loss (queue overflow)? Check `corelink_billing_queue_depth`.
   - Counter update fails silently? Check WAL/transactions.
   - Idempotency key collision? PAT-IDEMPOTENCY-001.
   - Clock drift afetou windowing? PAT-MONOTONIC-001.
2. Preservar evidence: D1 snapshot + R2 events + Stripe postings log.

## Notificação ao customer

- Se customer foi under-charged < $10: absorver internamente (sem notificação).
- Se > $10: notificar + oferecer pagamento ou crédito.
- Se over-charged: refund + crédito "trust bonus" (comms template).

## Post-mortem

- Public incident report se impact > $10k total ou > 100 customers.
- Reconcile tornado contínuo (não-diário) se rincidência.

## Prevenção

- INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP (invariant_registry §3.9).
- Events append-only em R2 Object Lock (immutable source of truth).
- Reconcile diário com threshold 0.1%; weekly 0.01%.
- Counter reset periódico em staging para validar reconstruction path.
