---
type: "ADR"
title: "ADR-S11-012 — TLA+ Scope Discipline: S-11 dsr_erasure_atomicity.tla"
description: "What S-11's single TLA+ spec formally proves (erasure atomicity, consent symmetry, partial residency, audit append-only) and what it explicitly defers to S-14, with the honest partial-coverage flag."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md"
  - "specs/tla/region_residency.tla"
  - "specs/03_architecture/invariant_registry.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "tla-plus", "formal-verification", "scope-discipline"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-012 — TLA+ Scope Discipline: S-11 dsr_erasure_atomicity.tla

S-11 has three competing CRITICAL invariants demanding TLA+ coverage but a single tractable spec budget. This ADR draws the scope boundary: `dsr_erasure_atomicity.tla` formally proves erasure atomicity, consent symmetry, and the *pinning + monotonic* sub-properties of residency, while the full cross-region routing proof is explicitly deferred to S-14 — and it carries the honest flag that S-11 residency coverage is PARTIAL, never overclaiming full formal coverage.

# Context

S-11 has 8 WIs in a HIGH_RISK lane and three competing TLA+ requirements: INV-DATA-ERASURE-COMPLETE (cross-backend atomicity), INV-CONSENT-PROOF-VERIFIABLE (grant/revoke schema parity), and INV-DATA-RESIDENCY (region pinning, which overlaps S-14 BYOK + cross-region routing). S-14 has a planned `region_residency.tla` + `byok_sovereignty.tla` covering the full routing semantics S-11 sets up but does not fully model (`specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:29-39`).

# Decision

`dsr_erasure_atomicity.tla` covers: `InvErasureComplete` (12-backend atomic completion gate), `InvConsentSymmetry` (grant/revoke share the 6-field proof schema), the residency sub-properties `InvResidencyPinned` (every active ticket has a pinned canonical-6-region) + temporal `InvResidencyMonotonic` (region never changes), `InvAuditAppendOnly`, `InvBackendAckIdempotent`, and the `EventualTermination` liveness property. It does NOT prove cross-region write impossibility, multi-region routing action semantics, or BYOK key-hierarchy sovereignty — those are deferred to S-14's `region_residency.tla` + `byok_sovereignty.tla` (`specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:41-71`). Rationale: adding a backend-region dimension (12 backends × 6 regions) and new routing actions would 10–100× the state space, blowing the CI ≤30min constraint and WI-S11-008's budget; the partial coverage suffices for S-11's deployment guarantee; and the honest-flag principle forbids claiming full residency coverage when only partial is delivered (`specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:73-94`).

# Consequences

The S-11 spec stays CI-tractable (50k–500k states, ≤30min); the two erasure/consent invariants are fully proven and residency gets meaningful partial coverage; the WI-S11-008 budget holds. The original negative — that INV-DATA-RESIDENCY's CRITICAL status depended on S-14 delivery (scope risk) — and the interim answer to auditors (partial TLA+ + 20k property tests + fail-CLOSED custom-domain routing until the full proof shipped) are recorded at (`specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:96-111`).

# Status vs shipped

**That residency scope risk is RETIRED.** The deferred cross-region routing proof landed:
`region_residency.tla` is sealed (`specs/tla/region_residency.tla:1`) and the invariant registry's
`INV-DATA-RESIDENCY` row now reads **"FULL coverage NOW LANDED"** via `region_residency.tla`
(`specs/03_architecture/invariant_registry.md:931`). The scope-discipline boundary this ADR drew is
historical; INV-DATA-RESIDENCY no longer hangs on S-14 delivery.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:29-39` — Context: 3 competing CRITICAL invariants + S-14 overlap.
2. `specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:41-71` — Decision: what the spec covers + what is deferred to S-14.
3. `specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:73-94` — Rationale: state-space blowup, S-11 sufficiency, honest-flag principle.
4. `specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md:96-111` — Consequences: CI tractability, scope risk, auditor response.
5. `specs/tla/region_residency.tla:1` — the deferred cross-region routing proof, now LANDED (seal commit `9b4333db`) — retires the scope risk.
6. `specs/03_architecture/invariant_registry.md:931` — `INV-DATA-RESIDENCY` now reads "FULL coverage NOW LANDED" via `region_residency.tla`.
