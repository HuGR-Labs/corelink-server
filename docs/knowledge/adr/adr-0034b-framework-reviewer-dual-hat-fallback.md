---
type: "ADR"
title: "ADR-0034b — Framework reviewer dual-hat fallback policy"
description: "Why the Owner may dual-hat two of the four framework-freeze reviewer slots under named permitted pairings, eligibility conditions, and auto-expiration triggers."
source_files:
  - "specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "governance", "reviewers", "dual-hat", "framework-freeze", "small-org"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0034b — Framework reviewer dual-hat fallback policy

The framework v1.0.0 FROZEN cut needs four distinct senior reviewers (architecture, compliance/privacy, security, production-ops), which a pre-GA small org cannot contractually staff — leaving the cut indefinitely deferred even though the engineering side is ready. This ADR is the framework-freeze-tier analog of ADR-0034: it authorizes the Owner to dual-hat two slots under explicit boundaries so GA is viable without abandoning cross-domain coverage.

# Context

The wave-20 proposal defines four framework-freeze reviewer slots (FW-H-1..4), but hiring four distinct senior ICs is not contractually feasible pre-GA, so without a formal fallback the v1.0.0 FROZEN cut stays indefinitely DEFERRED despite engineering readiness; ADR-0034 establishes the pattern that such waivers deserve their own ADR, and this is the analog at the framework-freeze tier (ADR-0034b:36-62).

# Decision

The Owner MAY take two FW-H-* slots personally iff five conditions hold (headcount < 12, no contractual SOC2/ISO27001 separation-of-duty yet, ≥ 2 retained external advisors, Owner self-attestation, and this ADR referenced in the §42 change-log), using only the two permitted cross-domain pairings (Alpha: Architecture+Security; Beta: Compliance+Ops) while three pairings are explicitly forbidden as conflicts-of-interest, preserving cross-veto/BLOCK rights and a 3-of-3-effective-seats quorum under dual-hat (ADR-0034b:64-137). The mode auto-expires when any trigger fires — ≥ 10 engineers (a 10-12 hysteresis band against oscillation), a first contractual SOC2/ISO27001 kickoff, an 18-month ceiling, or an Owner role transition (ADR-0034b:118-129).

# Consequences

Small-org GA becomes viable with a SOC2-readable audit trail of who signed what under which waiver and a hysteresis band that allows a graceful transition to four-distinct staffing, traded against more single-points-of-failure on the Owner-held slots, a conflict-of-interest concentrated on the Owner (structural; mitigated by disclosure), retainer-incentive risk to external-advisor cross-veto courage, and possibly-late expiration at the 10-12 band (ADR-0034b:177-197).

# Citations

1. `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md:36-62` — four FW-H-* slots, infeasible four-distinct hire, ADR-0034 prior-art pattern (Context).
2. `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md:64-137` — five eligibility conditions, permitted Alpha/Beta vs forbidden pairings, cross-veto and quorum (Decision).
3. `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md:118-129` — the auto-expiration triggers and 10-12 hysteresis band (Decision).
4. `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md:177-197` — small-org-GA-viable + audit trail vs SPOF/COI/cross-veto/late-expiration risks (Consequences).
