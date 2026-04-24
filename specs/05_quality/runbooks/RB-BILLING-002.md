---
id: "RB-BILLING-002"
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
tags: ["runbook", "p2", "billing", "late-events", "stub"]
---

# RB-BILLING-002 — Late-Arriving Events Triage

> **INV:** INV-BILLING-RECONCILE-3-LAYER | **SLA:** triage ≤ 24h

## Detecção

- Métrica `corelink.billing.late_events_total` > 5% mensal.
- usage_counter_late D1 table grows abnormalmente.
- Reconciliation drift detected em layer 1.

## Triage

1. Identify root cause:
   - Network partition? Region failover lag?
   - Bug em event emitter (CAS hot path)?
   - Stripe outage backlog drain?
2. Assess revenue impact: late events that fall em billing cycle anterior?
3. Customer-facing: should bill late events em next cycle ou waive?

## Resolução

- Hot fix: process late_events_table; audit emission; reconciliation re-run.
- Cold fix: address root cause; tighten emit window from 6h → 4h se needed.
- Post-mortem se > 1% mensal sustained 3 months.

## References

- `invariant_registry.md` INV-BILLING-RECONCILE-3-LAYER.
- `specs/04_sprints/S10/_spec_contract.md`.
