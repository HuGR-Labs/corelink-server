---
id: "RB-DATA-RESIDENCY-LEAK"
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
tags: ["runbook", "p1", "privacy", "residency", "schrems-ii", "stub"]
---

# RB-DATA-RESIDENCY-LEAK — Residency Leak (Cross-Region Tenant Data Exposure)

> **INV:** INV-DATA-RESIDENCY HIGH + INV-REGION-NO-CROSS-LEAK CRITICAL | **CTRL:** CTRL-PRIV-031 | **SLA:** detect ≤ 1h, mitigate ≤ 6h

## Detecção

- Property test detects cross-region blob.
- Customer report: "I'm EU and my data is in US bucket".
- Audit reveals routing bug.

## Comunicação

- **SEV-1** (Schrems II / GDPR Art. 44 exposure).
- Page Privacy Officer + Legal + Security + SRE.
- Customer notification within 72h (GDPR Art. 33).
- Regulator notification (DPC) consideration.

## Mitigação imediata

1. Stop all writes to wrong region.
2. Identify scope: which tenant + how many records.
3. Migrate data to correct region; verify hash.
4. Audit emission `corelink.privacy.residency_leak` event.

## Resolução

- Hot fix: migrate data; verify; close routing bug.
- Cold fix: insert checks reinforced; property test 30k expand; TLA+ region_residency.tla extended.
- Post-mortem CRITICAL within 7d.
- Customer trust review.

## References

- `invariant_registry.md` INV-DATA-RESIDENCY + INV-REGION-NO-CROSS-LEAK.
- `specs/04_sprints/S14/_spec_contract.md`.
- Schrems II + GDPR Art. 44.
