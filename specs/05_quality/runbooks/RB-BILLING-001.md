---
id: "RB-BILLING-001"
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
tags: ["runbook", "p2", "billing", "audit-replay", "stub"]
---

# RB-BILLING-001 — Audit-Grade Invoice Replay (Forensic Reconstruction)

> **INV:** INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH | **SLA:** reconstruction ≤ 5 min

## Quando usar

- Auditor requests invoice reconstruction.
- Customer disputes invoice line item.
- Internal compliance review (SOC 2).
- Finance close-of-month verification.

## Procedure

1. Identify invoice_id + tenant_id + period.
2. Query R2 `billing-events` bucket per tenant + ts range.
3. Run replay endpoint: `POST /v1/billing/replay?invoice_id=X&dry_run=true`.
4. Compare output JSON com Stripe invoice.
5. Document discrepancies (if any) → escalate.

## Outputs

- Reconstructed invoice JSON.
- Stripe invoice JSON.
- Diff report.
- Audit emission `corelink.billing.replay_executed`.

## Acceptance criteria

- Reconstructed = Stripe (modulo Stripe metadata).
- Diff < 0.01% — accept; > 0.01% → SEV-2 reconciliation.

## References

- `invariant_registry.md` INV-BILLING-REPLAYABLE-FROM-EVENTS.
- `specs/04_sprints/S10/_spec_contract.md`.
