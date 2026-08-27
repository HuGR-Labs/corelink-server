---
id: "RB-DEVENV-BILLING-DISCREPANCY"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "devenv", "p2", "sre", "billing", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-BILLING-DISCREPANCY — vCPU Metering & Ledger Reconciliation

> **Severity floor:** P2
> **Detect → Acknowledge → Engage signers:** FinOps & Billing on-call.
> **Companion docs:** `RB-INCIDENT-RESPONSE.md`.

---

## 1. Symptoms
- Monthly aggregated vCPU seconds in `devenv_monthly_vcpu` mismatch telemetry push events.
- Customer dispute on runner overage charges.

## 2. Diagnosis
- Query D1 raw telemetry ledger against Stripe invoiced usage:
  ```sql
  SELECT tenant_id, year_month, total_vcpu_seconds, last_recorded_at 
  FROM devenv_monthly_vcpu WHERE tenant_id = ?;
  ```

## 3. Resolution
- Replay telemetry journal events from `pushUsageEvent` logs to reconcile D1 totals.
