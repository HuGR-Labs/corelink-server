---
id: "RB-DSR-ERASURE-INCOMPLETE"
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
tags: ["runbook", "p1", "privacy", "dsr", "erasure", "stub"]
---

# RB-DSR-ERASURE-INCOMPLETE — Erasure Incomplete (DSR Backend Coverage Gap)

> **INV:** INV-DATA-ERASURE-COMPLETE CRITICAL | **CTRL:** CTRL-PRIV-030 | **SLA:** detect ≤ 24h, remediate ≤ 30d

## Detecção

- Verification job 24h post-erasure encontra records remaining em algum dos 7 backends.
- Customer complains: "you still have my data".
- Quarterly internal compliance audit finds DSR backend gap.

## Comunicação

- **SEV-1** (regulatory exposure LGPD/GDPR).
- Page Privacy Officer + Legal + SRE + Engineer.
- Internal channel.
- Customer notification within 72h.
- Regulator notification consideration (depends on jurisdiction + scope).

## Mitigação imediata

1. Identify affected backend(s) + records.
2. Manual erasure execute imediato + audit emission.
3. Re-run verification job 24h post-fix.
4. Customer notification.

## Resolução

- Hot fix: complete erasure manually + audit; ensure backend integrated.
- Cold fix: backend coverage CI gate strengthen; integration test per backend.
- Post-mortem within 7d.

## References

- `invariant_registry.md` INV-DATA-ERASURE-COMPLETE.
- `specs/04_sprints/S11/_spec_contract.md`.
- LGPD Art. 18 + GDPR Art. 17.
