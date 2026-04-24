---
id: "SPEC-CONTRACT-S10"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s10", "billing", "stripe", "high-risk"]
---

# Spec Contract — S-10: Billing Pipeline

## 0. Metadata

| Sprint ID | S-10 | Lane | HIGH_RISK |
|---|---|---|---|
| Duração | 3 semanas | WIs | 7 |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001), FF-HR-009 (contratos com customer via Stripe) |

## 1. Objetivo

Implementar pipeline de billing: usage events → counters → Stripe invoicing → reconciliation. Integridade financeira é CRITICAL — under-count = revenue leak; over-count = customer trust loss. INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP são enforced via reconciliation diária.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-005**: CTRL-BILLING-001 (append-only events + reconciliation) é controle de segurança financeira.
- **FF-HR-009**: integração Stripe = contrato com customer.

## 3. Inherits_from

```yaml
inherits_from:
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "SECURITY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-BILLING-001**: Usage events append-only em R2 + counter aggregation D1.
- **CAP-BILLING-002**: Stripe integration (customer creation + subscription + invoicing).
- **CAP-BILLING-003**: Daily reconciliation job (CTRL-BILLING-001).
- **CAP-BILLING-004**: Customer-facing usage dashboard (dados pra admin UI S-16).
- **CAP-BILLING-005**: Overage handling (95% quota alert → hard-block 100%).

## 5. Requirements específicos

- **R-S10-1**: Usage event emitter em CAS write/read paths (EVT-047 CloudEvents).
- **R-S10-2**: Counter aggregator cron DO (hourly rollup → D1 `usage_counter`).
- **R-S10-3**: Stripe adapter (`crates/corelink-billing`) com idempotency keys.
- **R-S10-4**: Reconciliation worker daily: Σ(events) vs counters; drift alert > 0.1%.
- **R-S10-5**: SLO-FRESH-BILLING enforcement (events processados ≤ 15 min).
- **R-S10-6**: Schema Neon `plan`, `subscription`, `invoice_line_item`.
- **R-S10-7**: RB-FM-302 (billing leak) dry-run obrigatório.

## 6. DoD

- [ ] 7 WIs SEALED.
- [ ] E2E: tenant signup → uso → monthly invoice → Stripe billed → reconciliation green.
- [ ] Chaos: Stripe outage → events queued → retry sucede (PAT-QUEUE-EVENTS-001).
- [ ] Reconciliation drift simulado → detected + alerted.
- [ ] Property test: replay de events não duplica counters (idempotency).
- [ ] PRR com Finance reviewer + Legal (DPA reference).

## 7. Completeness (delta)

- [ ] **10.s10.1** SLO-FRESH-BILLING ≥ 99.9% sustained 30d em staging.
- [ ] **10.s10.2** Stripe test mode: $1000 de usage simulado → invoice correto.
- [ ] **10.s10.3** Refund flow testado (customer contested).

## 8. Invariants

- INV-BILLING-NO-LOSS (HIGH): Σ(events) ≈ Σ(invoiced).
- INV-BILLING-NO-DUP (HIGH): idempotency via Idempotency-Key header.
- INV-AUDIT-APPEND-ONLY: usage events are immutable.

## 9. Quality Standards

- Zero tolerance pra drift > 0.1% em reconciliation.
- Stripe API retry com backoff (PAT-BACKOFF-001) + queue fallback.
- Audit-grade: Finance pode reconstruir qualquer invoice de events raw.

## 10. Anti-scope

- ❌ Tax calculation (Stripe Tax handles; não implementar).
- ❌ Multi-currency (USD only em S-10; EUR/BRL pós-GA).
- ❌ Enterprise custom pricing (S-19).

## 11. Dependencies

- Blocker: S-01+S-02+S-04 (usage points existem).
- Blocker: S-03 (auth tenant context).
- Blocker: S-09 (observability pra monitoring).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S10-001 | Usage event emitter em CAS hot paths |
| WI-S10-002 | Counter aggregator cron DO |
| WI-S10-003 | Crate corelink-billing (Stripe adapter) |
| WI-S10-004 | Reconciliation worker + drift alerts |
| WI-S10-005 | Neon schema billing (plan/subscription/invoice) |
| WI-S10-006 | SLO-FRESH-BILLING instrumentation |
| WI-S10-007 | RB-FM-302 dry-run + docs |

## 13. Duração

3 semanas; buffer 7 dias (Stripe integration é unpredictable).

## 14. Critérios de promoção

- DoD + 30d staging sem drift.
- Legal sign-off em Terms de billing.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Stripe outage (FM-151) | L | MEDIUM (queue mitiga) |
| Billing drift > 0.1% (FM-302) | M | HIGH (revenue) |
| Idempotency key collision | L | CRITICAL (double-charge) |
| Reconciliation lento (> 1h) → stale billing | M | MEDIUM |

---
