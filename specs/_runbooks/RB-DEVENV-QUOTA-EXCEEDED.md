---
id: "RB-DEVENV-QUOTA-EXCEEDED"
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
tags: ["runbook", "devenv", "p2", "sre", "quota", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-QUOTA-EXCEEDED — Tenant vCPU Monthly Limit & Overdraft

> **Severity floor:** P2
> **Detect → Acknowledge → Engage signers:** SRE & Customer Support on-call.
> **Companion docs:** `RB-INCIDENT-RESPONSE.md`.

---

## 1. Symptoms
- API returns HTTP 402 `Payment Required: Monthly vCPU quota exceeded`.
- Customer requests emergency temporary quota bump for release deployment.

## 2. Diagnosis
- Inspect tenant quota tier in D1 `runners_entitlement`.

## 3. Resolution
- Apply manual quota expansion:
  ```sql
  UPDATE runners_entitlement SET max_vcpu_h = max_vcpu_h + 50 WHERE tenant_id = ?;
  ```
