---
id: "ADR-S11-012"
title: "TLA+ Scope Discipline S-11: dsr_erasure_atomicity covers erasure + consent + PARTIAL residency; FULL region_residency.tla deferred to S-14"
status: "ACCEPTED"
date: "2026-05-13"
tags: ["adr", "s-11", "tla-plus", "formal-verification", "scope-discipline", "residency"]
deciders:
  - "Gustavo Schneiter (owner / final approver)"
  - "Architect"
  - "Privacy Officer (interim Gustavo até hire)"
supersedes: null
superseded_by: null
parent: "S-11"
---

# ADR-S11-012 — TLA+ Scope Discipline: S-11 `dsr_erasure_atomicity.tla` (REVISED Lote 10.11.0-bis-prime cycle 3)

> **Status:** ACCEPTED
> **Date:** 2026-05-13
> **Deciders:** Gustavo Schneiter (Owner/Architect/Privacy Officer interim)
> **Honest-flag:** S-11 TLA+ coverage is PARTIAL for residency — FULL coverage deferred to S-14. This ADR documents the scope boundary and rationale explicitly.

---

## 1. Context

S-11 has 8 WIs in HIGH_RISK lane, a sprint budget of ~20.7h for WI-S11-008 alone, and three competing TLA+ spec requirements:

1. **INV-DATA-ERASURE-COMPLETE** (CRITICAL) — cross-backend erasure atomicity. Direct output of WI-S11-002 erasure worker.
2. **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL) — consent grant/revoke schema parity (Lote 9.4 H-05). Direct output of WI-S11-003 consent ledger.
3. **INV-DATA-RESIDENCY** (CRITICAL) — tenant.primary_region pinning. Directly tied to WI-S11-007 residency enforcement, which itself overlaps with S-14 BYOK and cross-region routing semantics.

Additionally, S-14 has a planned `region_residency.tla` and `byok_sovereignty.tla` covering the full cross-region routing action semantics that S-11 residency enforcement sets up but does not fully model (backend region dimension, cross-region write impossibility proof, BYOK multi-region key hierarchy).

---

## 2. Decision

**`specs/tla/dsr_erasure_atomicity.tla` (S-11 WI-S11-008) covers:**

1. **INV-DATA-ERASURE-COMPLETE** (CRITICAL): state invariant `InvErasureComplete` — 12-backend atomic completion gate. All 12 backends (8 effective + 4 pseudonymized) must ack before `completed` state reachable.

2. **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL): state invariant `InvConsentSymmetry` — consent_ledger and consent_revocation share identical 6-field `ConsentProofFields` schema. `GrantConsent` and `RevokeConsent` actions both require `ValidProof(p)` with same field schema.

3. **INV-DATA-RESIDENCY (PARTIAL — S-11 sub-property)**: two properties in `dsr_erasure_atomicity.tla`:
   - **State invariant:** `InvResidencyPinned` — every active ticket (status ≥ received) has a pinned region in `ticket_region`; region is canonical 6-region enum.
   - **Temporal property:** `InvResidencyMonotonic` — once a ticket's region is set, it never changes (no cross-region migration).

   These two properties prove the **pinning + monotonic** sub-properties of INV-DATA-RESIDENCY. They do NOT prove:
   - Cross-region write impossibility (requires backend region dimension — S-14 scope).
   - Multi-region routing action semantics (requires separate `region_residency.tla` — S-14 scope).
   - BYOK key hierarchy sovereignty (requires `byok_sovereignty.tla` — S-14 scope).

4. **INV-AUDIT-APPEND-ONLY** (CRITICAL): temporal property `InvAuditAppendOnly` — audit_chain length monotonic non-decreasing + prefix preserved. Box-prime formula over vars.

5. **InvBackendAckIdempotent**: same (ticket, backend) outcome overwrite-idempotent under retry.

6. **Liveness:** `EventualTermination` — in-flight tickets eventually reach terminal state (`completed`/`denied`/`failed`). WF fairness conditions on all transition actions.

**Deferred to S-14:**

- `region_residency.tla` — FULL residency proof covering: backend region dimension (each backend has a region; writes only to backend in tenant's region); cross-region write impossibility; custom domain routing fail-CLOSED proof; BYOK multi-region key hierarchy.
- `byok_sovereignty.tla` — BYOK key sovereignty + cross-region key isolation.

**Reference:** `invariant_registry.md §4.2` — L414 (INV-DATA-ERASURE-COMPLETE), L419 (INV-CONSENT-PROOF-VERIFIABLE), L420 (INV-DATA-RESIDENCY partial), L450 (residency full S-14 deferral).

---

## 3. Rationale

### 3.1 Sprint scope discipline

S-11 sprint contract (§19 R-S11-19a) explicitly acknowledges the residency TLA+ split. Adding full cross-region routing semantics to `dsr_erasure_atomicity.tla` would:
- Require adding a `backend_region` dimension to the state space (12 backends × 6 regions = 72 new CONSTANT combinations).
- Require new actions: `RouteRequest`, `CrossRegionWriteAttempt`, `FailCrossRegionWrite`.
- Increase state space by an estimated 10–100x (from ~50k to potentially millions of states), violating the CI ≤ 30min constraint.
- Blow WI-S11-008's 32h time budget.

### 3.2 S-11 residency coverage is sufficient for S-11 invariants

The partial residency coverage (pinning + monotonic) in `dsr_erasure_atomicity.tla` directly supports the S-11 deployment guarantee:
- WI-S11-007 runtime residency pinning (property tests 20k iterations) + custom domain routing fail-CLOSED (PAT-ROUTING-PINNED-001) provide the practical runtime guarantee.
- TLA+ `InvResidencyPinned` + `InvResidencyMonotonic` provide formal proof of the state machine invariant (no ticket ever in wrong region).
- The "cross-region write impossibility" proof requires backend infrastructure that is fully specified in S-14 (BYOK + multi-region storage).

### 3.3 Honest-flag principle (Lote 10.11.0-bis-prime cycle 4)

The spec must not claim full residency formal coverage when only partial coverage is delivered. `invariant_registry.md §4.2 L450` explicitly marks INV-DATA-RESIDENCY as: "PARTIAL coverage S-11 via dsr_erasure_atomicity.tla (InvResidencyPinned + temporal InvResidencyMonotonic); FULL coverage S-14 via region_residency.tla."

---

## 4. Consequences

### Positive
- S-11 TLA+ spec remains tractable for CI (state space target 50k–500k; ≤ 30min TLC run).
- INV-DATA-ERASURE-COMPLETE and INV-CONSENT-PROOF-VERIFIABLE are formally proven in S-11.
- INV-DATA-RESIDENCY gets meaningful partial formal coverage (pinning + monotonic) in S-11.
- Sprint budget respected; WI-S11-008 delivers within 32h limit.

### Negative
- INV-DATA-RESIDENCY CRITICAL status requires S-14 delivery to be fully covered. S-14 scope risk.
- Regulatory auditors may ask for full cross-region routing proof; response: partial TLA+ + 20k property tests + custom domain routing fail-CLOSED PAT-ROUTING-PINNED-001 cover S-11 scope. Full formal coverage shipped S-14.

### Neutral
- `region_residency.tla` scaffold in S-14 sprint contract (confirmed in _spec_contract.md §19a).

---

## 5. Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Owner / Architect | Gustavo Schneiter | 2026-05-13 | Approved |
| Privacy Officer | Gustavo Schneiter (interim) | 2026-05-13 | Approved |
| Compliance | TBD | TBD | Pending |

---

*ADR-S11-012 REVISED Lote 10.11.0-bis-prime cycle 3. Supersedes informal sprint contract discussion. Formally documents TLA+ scope split between S-11 and S-14.*
