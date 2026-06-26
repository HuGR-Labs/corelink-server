---
type: "ADR"
title: "ADR-0045 — SLSA L3 + Rekor mandatory build provenance"
description: "Adopts SLSA Level 3 provenance for all releases via the SLSA generator + Fulcio keyless OIDC + mandatory Rekor inclusion (INV-SUPPLY-PROVENANCE-IN-REKOR) + in-toto v1.0 + a customer-side verify CLI."
source_files:
  - "specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s12", "supply-chain", "slsa", "sigstore", "rekor"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0045 — SLSA L3 + Rekor mandatory build provenance

CoreLink is a multi-tenant cache wired into 100+ customers' CI/CD pipelines, which makes it a
high-value supply-chain target — so the build pipeline must emit provenance that customers can verify
without trusting the vendor. This ADR is the decision record that sets SLSA Level 3 with mandatory
Rekor as the build-provenance standard, the supply-chain posture that the whole release process is
built around.

# Context

As a cache substrate in many customers' CI/CD, CoreLink is a high-value supply-chain target (SolarWinds,
event-stream, XZ Utils show the class is actively exploited), so the build pipeline must produce
cryptographically verifiable provenance that cannot be forged without the canonical workflow, is
independently verifiable without trusting the vendor, detects offline tampering, and satisfies SOC 2
CC6.7 / EO 14028 / NIST SSDF — while competitors sit at SLSA L1/L2.

# Decision

CoreLink adopts **SLSA Level 3** as the mandatory standard for all releases via: the SHA-pinned
`slsa-github-generator` (reuse audited upstream, not hand-rolled); Fulcio keyless OIDC signing with
short-lived (≤10min) certs bound to the workflow ref + commit SHA (zero long-lived secrets); and a
**mandatory Rekor inclusion proof** (`INV-SUPPLY-PROVENANCE-IN-REKOR`) enforced at the release
pipeline, the deploy webhook, and the customer CLI — with no graceful fallback, because attestation
without Rekor is effectively SLSA L1 (offline tampering undetectable). Only in-toto v1.0
`predicateType` is accepted, and a `corelink-supply-verify` CLI lets customers verify independently of
vendor infrastructure.

# Consequences

The posture is industry-leading versus L1/L2 competitors, externally auditable via Rekor, eliminates
the secret-exfiltration threat class, and gives defense-in-depth through customer-side verify; the
accepted costs are that a Rekor outage blocks release publication (rare; documented runbook), the SLSA
generator adds ~3 min/run, and the verify CLI must be distributed to customers.

# Citations

1. `specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md:24-38` — Context: high-value supply-chain target + the four provenance requirements + competitors at L1/L2.
2. `specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md:42-43` — Decision: adopt SLSA Level 3 as the mandatory standard for all releases.
3. `specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md:66-79` — Rekor inclusion is MANDATORY (INV-SUPPLY-PROVENANCE-IN-REKOR) with no graceful fallback.
4. `specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md:122-131` — Consequences: industry-leading + externally auditable + no long-lived secrets vs the Rekor-outage and CLI-distribution costs.
