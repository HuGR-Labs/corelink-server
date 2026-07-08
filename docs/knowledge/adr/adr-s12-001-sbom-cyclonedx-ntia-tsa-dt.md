---
type: "ADR"
title: "ADR-S12-001 — SBOM CycloneDX 1.5+ NTIA strict + RFC 3161 TSA + Dependency-Track"
description: "Why CoreLink generates a CycloneDX 1.5 SBOM per release, gates it in NTIA strict mode, timestamps it via the Sigstore RFC 3161 TSA, and ingests it into self-hosted Dependency-Track."
source_files:
  - "specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md"
checkpoint_sha: "b5ce2bff384a09047f027082dcf4355136822242"
provenance: "AUTHORED"
tags: ["adr", "s12", "sbom", "cyclonedx", "supply-chain", "ntia"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S12-001 — SBOM CycloneDX 1.5+ NTIA strict + RFC 3161 TSA + Dependency-Track

Enterprise and federal prospects require a public, verifiable software bill of materials with continuous CVE matching, or CoreLink is not compliance-grade. This ADR fixes the whole SBOM stack: CycloneDX 1.5 as the format (first-class Rust + Sigstore support), NTIA strict mode as the CI gate, the free public Sigstore TSA for RFC 3161 timestamping, and self-hosted Dependency-Track for CVE matching — each chosen against named alternatives.

# Context

CoreLink needs an SBOM per release to satisfy US EO 14028, the NTIA minimum elements (7 mandatory fields), SOC 2 CC7.1 + ISO 27001 A.8.30, and LGPD Art. 38. Without a public verifiable SBOM + CVE matching, it is not compliance-grade for enterprise/federal prospects (`specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:27-35`).

# Decision

Six binding choices: (1) **CycloneDX 1.5** over SPDX — `cargo-cyclonedx` is first-class Rust, Cosign attaches it natively, it is more expressive for deps/licenses; (2) **NTIA strict mode** as the CI gate (100% threshold per field, placeholder `"UNKNOWN"` rejected), auditor mode opt-in only; (3) **Sigstore TSA** (`tsa.sigstore.dev`) for RFC 3161 timestamps — public, free, replay-defeated by SHA-256 hash binding, self-hosted TSA rejected; (4) **PURL normalisation** with a `pkg:cargo/...` canonical + DT alias + workspace `vcs_url` qualifier (`github.com/HumanGuardrail/corelink-server`) against PURL-confusion attacks; (5) **self-hosted Dependency-Track v4.11+** over Snyk/GHAS — self-hostable, open-source, native ingestion, $0; (6) **SBOM regenerated every release** (not cached), since `Cargo.lock` drifts via Dependabot transitive updates (`specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:37-83`). TSA and DT outages degrade gracefully (ship without TSR / fallback queue + SEV-3) rather than blocking the release.

# Consequences

The SBOM is a publicly downloadable release asset; the RFC 3161 timestamp gives tamper-detectable proof of generation time; DT provides continuous CVE matching with ≤15min p99 alerts; NTIA strict mode blocks incomplete SBOMs; and the workspace PURL discriminator prevents confusion attacks. Trade-offs: a transient TSA outage leaves a tampering window until async retry, DT outages delay (not block) CVE matching, and `cargo-cyclonedx` is pinned (bumps need an ADR) (`specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:85-99`). Rejected: SPDX, NTIA auditor-mode-as-gate, self-hosted TSA, Snyk/GHAS, blocking on DT outage, and an SBOM optional flag (`specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:101-110`).

# Citations

1. `specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:27-35` — Context: EO 14028 / NTIA / SOC2 / LGPD compliance drivers.
2. `specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:37-83` — Decision: the six binding choices (format, NTIA strict, TSA, PURL, DT, regen).
3. `specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:85-99` — Consequences and trade-offs.
4. `specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md:101-110` — Rejected alternatives table.
