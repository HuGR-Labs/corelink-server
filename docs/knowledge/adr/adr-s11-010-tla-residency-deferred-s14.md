---
type: "ADR"
title: "ADR-S11-010 — TLA+ Residency Formal Proof Deferred to S-14"
description: "Why S-11 ships residency as runtime enforcement + a 20k property test rather than a full TLA+ region_residency.tla proof, which is deferred to S-14 alongside BYOK sovereignty."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "tla-plus", "residency", "formal-verification", "deferred"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-010 — TLA+ Residency Formal Proof Deferred to S-14

S-11 delivers residency enforcement as a runtime layer (custom-domain routing, D1 trigger checks, Worker pre-flight assertions) plus a 20k-iteration property test proving zero cross-region leaks. This ADR records the deliberate scope call to ship that as S-11's primary residency mechanism and **defer** the full `region_residency.tla` formal proof to S-14, where it overlaps with BYOK sovereignty — while noting that S-11 already carries the safety-critical sub-properties formally.

# Context

WI-S11-007 delivers the residency runtime layer + a 20k property test (10k weur + 10k enam × 5 ops → 0 leaks). The question is whether a full TLA+ proof of `INV-DATA-RESIDENCY` must also ship in S-11. The invariant registry already marks the full proof as "S-14 PLANNED," and `dsr_erasure_atomicity.tla` already provides partial formal coverage via `InvResidencyPinned` + `InvResidencyMonotonic` (`specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:23-36`).

# Decision

The TLA+ proof of cross-region routing actions (`region_residency.tla`) is **deferred to S-14**; S-11 ships runtime enforcement + property tests as the primary mechanism (`specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:38-42`). Rationale: runtime enforcement + 20k property test meets the industry-standard regulatory bar (Schrems II / LGPD Art. 33 §1º / GDPR Art. 44 require runtime blocking, not necessarily a formal proof); `region_residency.tla` overlaps with S-14's `byok_sovereignty.tla`, so combining them avoids duplication; S-11's HIGH_RISK lane has no budget for ~16+ extra hours; and `dsr_erasure_atomicity.tla` already formally covers the safety-critical pinning + monotonic subset (`specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:44-60`).

# Consequences

S-11 ships without the full cross-region routing proof; the invariant registry keeps `INV-DATA-RESIDENCY` at "S-14 PLANNED"; the S-14 sprint contract must include `region_residency.tla` + `byok_sovereignty.tla`; and this ADR is cited as the deferral rationale in residency audit reports (`specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:68-73`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:23-36` — Context: runtime layer + property test + existing partial TLA+ coverage.
2. `specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:38-42` — Decision: defer `region_residency.tla` to S-14.
3. `specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:44-60` — Rationale: industry sufficiency, S-14 BYOK overlap, sprint budget, partial coverage.
4. `specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md:68-73` — Consequences: S-14 contract obligation + audit-report citation.
