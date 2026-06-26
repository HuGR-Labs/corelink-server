---
type: "ADR"
title: "ADR-0014 — SBOM in CycloneDX 1.5+ (preferred), SPDX 2.3+ accepted"
description: "Resolves a spec contradiction by standardizing SBOM output on CycloneDX 1.5+ JSON for the Rust toolchain while still accepting SPDX 2.3+ for external stakeholders."
source_files:
  - "specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "supply-chain", "sbom", "cyclonedx", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0014 — SBOM in CycloneDX 1.5+ (preferred), SPDX 2.3+ accepted

A Software Bill of Materials is CoreLink's supply-chain evidence artifact, but the specs disagreed on its format — the framework said SPDX while the security model said CycloneDX — which would block any automated SBOM pipeline from being authoritative. This ADR settles it on CycloneDX 1.5+ JSON as the preferred format (mature Rust tooling) while keeping SPDX 2.3+ accepted for stakeholders that demand it. It matters as the format anchor for SOC 2 / NTIA SBOM compliance and Dependency-Track ingestion.

# Context

An audit found a three-way contradiction: the framework's `EVT-010` specified SPDX 2.3+ while the security model specified CycloneDX 1.5 generated via `cargo-cyclonedx` (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:30`). The Rust ecosystem reality is that `cargo-cyclonedx` is mature while `cargo-spdx` is less so, and downstream tools (Dependency-Track, Snyk, JFrog Xray) and compliance regimes accept either (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:36-40`).

# Decision

Adopt CycloneDX 1.5+ JSON as the preferred SBOM format, keeping SPDX 2.3+ accepted for external requirements (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:42-48`). Rejected: SPDX-only (immature Rust tooling produces incomplete SBOMs), CycloneDX-only (some enterprise customers ask for SPDX), and a custom format (anti-pattern, breaks interop) (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:54-57`).

# Consequences

- `cargo-cyclonedx` works out-of-the-box in CI and SOC 2 auditors get an industry-standard SBOM (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:62-64`).
- Supporting two formats costs a conversion path and a second CI test path — a small overhead (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:66-67`).
- The decision is governance-only: the implementation note records the CI pipeline to generate + cosign-sign the SBOM was not yet implemented, tracked as an open compliance gap (`specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:74`).
- Related supply-chain decision: [ADR-0015](/adr/adr-0015-reproducible-build-best-effort.md). Sibling lote ADRs: [ADR-0012](/adr/adr-0012-ff-hr-011-gc-reachability.md), [ADR-0013](/adr/adr-0013-promote-remote-cache-canonical.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:30` — the SPDX-vs-CycloneDX contradiction across specs.
2. `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:36-40` — Rust tooling maturity + downstream acceptance.
3. `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:42-48` — the decision: CycloneDX preferred, SPDX accepted.
4. `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:54-57` — rejected alternatives.
5. `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:62-67` — positive + negative consequences.
6. `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:74` — CI pipeline not yet implemented (open gap).
