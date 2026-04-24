---
id: "RB-FM-SIGNUP-FAILED"
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
tags: ["runbook", "p2", "onboarding", "signup", "atomicity", "stub"]
---

# RB-FM-SIGNUP-FAILED — Signup Atomicity Failure (Orphan Tenant)

> **INV:** INV-ONBOARD-ATOMIC-PROVISIONING + INV-ONBOARD-DPA-FIRST | **SLA:** detect ≤ 24h, remediate ≤ 7d

## Detecção

- D1 query: tenant rows sem corresponding Stripe customer ou DPA acceptance.
- Customer complains: "I signed up but billing fails" or "I'm billed but didn't sign DPA".
- Conversion funnel anomaly: spike in signup_completed sem first_pat_created.

## Comunicação

- **SEV-2** (data consistency + customer trust).
- Page SRE + Engineer onboarding.
- Customer success outreach.

## Mitigação imediata

1. Query orphan tenants D1.
2. Manual cleanup: rollback partial state; refund se billing inadvertent.
3. Customer outreach via support.
4. Audit emission.

## Resolução

- Hot fix: cleanup orphans; verify atomicity invariant in code path.
- Cold fix: chaos test Stripe outage during signup; property test 10k concurrent.
- Post-mortem if > 5 orphans/month.

## References

- `invariant_registry.md` INV-ONBOARD-ATOMIC-PROVISIONING.
- `specs/04_sprints/S19/_spec_contract.md`.
