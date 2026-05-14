---
id: "ADR-S11-010"
type: "adr"
doc_status: "SEALED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s11", "tla+", "residency", "formal-verification", "s14", "deferred"]
---

# ADR-S11-010: TLA+ Residency Formal Proof Deferred to S-14

## Status

ACCEPTED

## Context

WI-S11-007 delivers residency enforcement runtime layer: custom domain routing,
D1 trigger insert checks, Worker pre-flight assertions, and a 20k property test
(10k weur + 10k enam × 5 ops → 0 cross-region leaks). The question is whether
a formal TLA+ proof of `INV-DATA-RESIDENCY` should also be delivered in S-11.

The invariant registry (`invariant_registry.md §4.2 L445`) already notes:
"Subsumed por INV-REGION-NO-CROSS-LEAK (S-14 PLANNED `region_residency.tla` /
`byok_sovereignty.tla`)".

`dsr_erasure_atomicity.tla` (WI-S11-001/002) covers `InvResidencyPinned` +
`InvResidencyMonotonic` (temporal: no cross-region migration), providing
PARTIAL formal coverage for S-11.

## Decision

TLA+ formal proof for cross-region routing actions (`region_residency.tla`) is
**deferred to S-14**. S-11 delivers runtime enforcement + property tests as
the primary residency coverage mechanism.

## Rationale

1. **Industry standard sufficiency**: Runtime enforcement + 20k property test
   (10k weur + 10k enam → 0 cross-region leaks) meets industry standard for
   regulatory defensibility (Schrems II + LGPD Art. 33 §1º + GDPR Art. 44
   compliance requires runtime blocking, not necessarily formal proof).

2. **S-14 BYOK overlap**: `region_residency.tla` overlaps with `byok_sovereignty.tla`
   (S-14 BYOK per-region key vault). Combining both in S-14 avoids duplication.

3. **Sprint scope discipline**: S-11 has 8 WIs with HIGH_RISK lane. Adding full
   TLA+ would add ~16+ hours beyond PERT budget.

4. **Partial coverage in S-11**: `dsr_erasure_atomicity.tla` already covers
   `InvResidencyPinned` (tenant pinning canonical) and `InvResidencyMonotonic`
   (no cross-region migration without formal request). This is the safety-critical
   subset; cross-region routing actions are the liveness subset (deferred).

## Alternatives Considered

- **TLA+ in S-11** (rejected): Scope blow, S-14 overlap, sprint budget exceeded.
- **Stub TLA+ in S-11, complete in S-14** (rejected): Stub without TLC verification
  provides false assurance; better to be explicit about deferral.

## Consequences

- S-11 ships without `region_residency.tla` (FULL cross-region routing proof).
- `invariant_registry.md §4.2 L445` maintains "S-14 PLANNED" status.
- S-14 sprint contract must include `region_residency.tla` + `byok_sovereignty.tla`.
- ADR-S11-010 referenced in all residency-related audit reports as deferral rationale.

## Sign-off

- Privacy Officer: _pending pre-merge_
- Architect: _pending pre-merge_
