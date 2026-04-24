---
id: "RB-FM-105"
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
tags: ["runbook", "p2", "region", "replication", "stub"]
---

# RB-FM-105 — Region Replication Diverge

> **FM:** FM-105 (S=3, O=3, D=3, RPN=27, P2) | **INV:** INV-CAS-INTEGRITY HIGH + INV-REGION-NO-CROSS-LEAK | **SLA:** detect ≤ 1h, mitigate ≤ 6h

## Detecção

- Reconcile cross-region: hash mismatch entre primary e replica.
- Failover read returns content diferente.
- Propagation lag > 60s p99 sustained.

## Comunicação

- **SEV-2.** Page SRE + Architect.
- Internal channel.
- Customer notification se serving stale data > 5 min.

## Mitigação imediata

1. Identificar blob digest divergente; quarantine ambas as cópias.
2. Re-replicate from authoritative primary.
3. Hash verify (INV-CAS-INTEGRITY).
4. Audit emit `corelink.region.replication_diverge` event.

## Resolução

- Hot fix: re-replicate; verify; clear cache.
- Cold fix: investigate root cause (network partition? clock skew? bug in replicator?).
- Long-term: TLA+ scope expand to model replication.

## References

- `failure_modes.md` FM-105.
- `invariant_registry.md` INV-CAS-INTEGRITY.
- `specs/04_sprints/S14/_spec_contract.md`.
