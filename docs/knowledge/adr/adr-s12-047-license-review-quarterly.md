---
type: "ADR"
title: "ADR-S12-047 — Quarterly license review process"
description: "Why CoreLink runs a manual quarterly license review on top of the automated cargo-deny SPDX gate, and what that review covers."
source_files:
  - "specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s12", "supply-chain", "license", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S12-047 — Quarterly license review process

Automated license enforcement (cargo-deny + `deny.toml`) only validates the SPDX expression a crate *declares* in its `Cargo.toml`; it cannot tell whether that declaration matches the crate's actual license. This ADR records the decision to close that gap with a structured manual review on a quarterly cadence, the cost-effective midpoint between continuous (too expensive) and annual (too infrequent) cycles. It is the dependency-license compliance backbone for SOC 2 CC7.1 / OWASP ASVS V14.

# Context

The automated gate covers 100% of declared SPDX expressions but is blind to **SPDX drift** — a crate that declares `MIT` while shipping GPL-derived code, or that silently changes its effective license between releases (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:24-32`). The Rust ecosystem also introduces novel license expressions that may be business-compatible but absent from the CoreLink allowlist. Quarterly was chosen as the cost/coverage balance between continuous and annual review (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:34-36`).

# Decision

- **Cadence:** every 3 months the Compliance Officer + Legal conduct a structured review of the dependency license posture (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:39-42`).
- **Process (6 steps):** sample 5% of `Cargo.lock` deps (prioritising new additions + high-download crates), verify SPDX drift against actual license files, evaluate new license expressions for allowlist addition, review unmaintained deps, document findings as a `ADR-S12-XXXX-license-review-YYYYQQ.md`, and require Compliance + Legal sign-off (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:44-58`).
- **Output artefacts:** the per-quarter review ADR, any `deny.toml` allowlist update, and backlog items for unmaintained-dep replacements (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:60-67`).
- **Why 5% sample, not 100%:** 100% audit of 200+ transitive deps per quarter is not cost-effective; the automated gate already enforces 100% SPDX expressions continuously, so the manual review targets only the drift gap automation cannot close (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:83-89`).

# Consequences

- Positive: a repeatable process + a SOC 2 CC7.1 evidence trail (quarterly review + cargo-deny CI gate); unmaintained deps tracked and replaced on cadence (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:99-102`).
- Negative / mitigated: 4–8 hours/quarter of reviewer time, and probabilistic drift detection (5% sample misses 95% of crates) — bounded by continuous automated SPDX enforcement on every PR (`specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:104-108`).

# Citations

1. `specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:24-36` — the SPDX-drift gap automation cannot see, and the quarterly-cadence rationale.
2. `specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:39-67` — cadence, 6-step review process, and output artefacts.
3. `specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:83-89` — the 5%-sample-vs-100% rationale.
4. `specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md:99-108` — positive and mitigated-negative consequences.
