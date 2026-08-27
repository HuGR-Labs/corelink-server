---
id: "RB-DEVENV-WONT-STOP"
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
tags: ["runbook", "devenv", "p1", "sre", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-WONT-STOP — DevEnv Container Teardown Failure

> **Severity floor:** P1
> **Detect → Acknowledge → Engage signers:** SRE on-call (pager duty alerts on `devenv_stop_failure_total > 3`).
> **Companion docs:** `RB-INCIDENT-RESPONSE.md`.

---

## 1. Symptoms
- User issues `DELETE /v1/customer/devenv` but state remains `stopping` for > 30s.
- Background snapshot pass hanging on large uncommitted workspace directory.

## 2. Diagnosis
- Inspect active snapshot processes via exec-server `/port-check/9090`.
- Verify if CAS R2 uploads are experiencing throttling.

## 3. Resolution
- Issue emergency kill and transition to `stopped`:
  ```bash
  curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" \
    https://api.corelink.humangr.com/internal/v1/devenv/tenant/<TENANT_ID>/kill
  ```
