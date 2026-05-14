---
id: "ADR-S12-001"
title: "SBOM CycloneDX 1.5+ NTIA strict + RFC 3161 TSA + Dependency-Track ingestion"
type: "adr"
doc_status: "SEALED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - { role: "architect", name: "Architect" }
  - { role: "compliance", name: "Compliance Officer (NTIA + EO 14028)" }
  - { role: "security", name: "AppSec advisor (TSA replay + DT API key threat model)" }
supersedes: null
superseded_by: null
tags: ["adr", "s12", "sbom", "cyclonedx", "ntia", "tsa", "dependency-track", "supply-chain"]
---

# ADR-S12-001 — SBOM CycloneDX 1.5+ NTIA strict + RFC 3161 TSA + Dependency-Track ingestion

## Status

ACCEPTED (WI-S12-002 SEALED 2026-05-13)

## Context

CoreLink requires a software bill of materials (SBOM) for every release to satisfy:
- US Executive Order 14028 (SBOM mandatory for federal contracts).
- NTIA minimum elements (Jul 2021 spec, 7 mandatory fields).
- SOC 2 CC7.1 (vulnerability detection) + ISO 27001 A.8.30.
- LGPD Art. 38 (registro de operações).

Without a public, verifiable SBOM + CVE matching system, CoreLink is not compliance-grade for enterprise or federal prospects.

## Decision

### 1. Format: CycloneDX 1.5+ (not SPDX)

CycloneDX 1.5 is chosen over SPDX 2.3 because:
- `cargo-cyclonedx` (first-class Rust support) targets CycloneDX natively.
- Sigstore Cosign attaches CycloneDX bundles natively.
- CycloneDX is more expressive for dependency relationships + license metadata.
- Industry pattern: Chainguard / Distroless / Wolfi all use CycloneDX.

**Future:** SPDX dual-format if enterprise demand materialises (S-20 GA hardening, requires ADR bump).

### 2. NTIA validation: strict mode (not auditor mode) as CI gate

Strict mode enforces 100 % threshold per NTIA field:
- Author present, timestamp present, component name 100 %, version 100 %, supplier ≥ 95 %, unique ID (PURL or CPE) 100 %, dependency relationships populated.
- Auditor mode (soft warnings) is opt-in (`--auditor`) for debugging; never the production gate.
- Placeholder values (`"UNKNOWN"` supplier) are detected and rejected.

### 3. RFC 3161 timestamp: Sigstore TSA (`tsa.sigstore.dev`) (not self-hosted)

- Sigstore TSA is public, free, and RFC 3161 compliant.
- Customer-side verification: `openssl ts -verify -in sbom.cdx.json.tsr -data sbom.cdx.json -CAfile sigstore-tsa-root.pem`.
- Self-hosted TSA rejected: operational overhead + trust anchor management not justified pre-GA.
- TSR binds the SHA-256 hash of the SBOM + nonce. Replay attacks are defeated by hash binding verification.
- TSA outage policy: publish SBOM without TSR + SEV-3 alert; release continues.

### 4. PURL normalisation

- Primary PURL: `pkg:cargo/<name>@<version>` (canonical).
- DT alias: `pkg:crates/<name>@<version>` (stored as `dt:purl_alias` property for DT compatibility).
- Workspace members: `?vcs_url=https://github.com/humangr-labs/corelink-server` qualifier to prevent PURL confusion attacks.
- Patched crates (`[patch.crates-io]`): `cdx:patched_locally=true` annotation + auditor mode warning.

### 5. Dependency-Track: self-hosted v4.11+ (not Snyk / GitHub Advanced Security)

- Self-hostable → data residency control.
- Open-source → no vendor lock-in.
- CycloneDX ingestion native.
- Mature CVE matching (NVD + OSV + GHSA).
- Cost: $0 vs Snyk $$$/dev.

DT ingestion uses `POST /api/v1/bom` (multipart, `autoCreate=true`) with exponential-backoff retry (1 s → 4 s → 16 s, max 4 attempts). On exhaustion: fallback queue + SEV-3 alert. Release publication continues.

### 6. SBOM regenerated on every release (not cached)

`Cargo.lock` changes between releases via Dependabot transitive updates. Stale SBOM → false-negative CVE matching. Cost: ~20 s per release (negligible).

## Consequences

### Positive

- CoreLink SBOM is publicly downloadable as a GitHub release asset (compliance evidence).
- RFC 3161 TSA timestamp provides cryptographic evidence of generation time (tamper-detectable).
- Dependency-Track provides continuous CVE matching with webhook alerts ≤ 15 min p99.
- NTIA strict mode gate in CI prevents incomplete SBOMs from shipping.
- Workspace member PURL discriminator prevents PURL confusion supply-chain attacks.

### Negative / Trade-offs

- TSA outage window (transient): SBOM shipped without TSR; tampering window until async retry.
- DT outage: CVE matching delayed (fallback queue); accepted as non-critical degradation.
- `cargo-cyclonedx` pinned at 0.5.x; bumps require ADR + regression testing.

## Rejected alternatives

| Alternative | Reason rejected |
|---|---|
| SPDX format | Less expressive for deps; `cargo-spdx` tooling immature |
| NTIA auditor mode as gate | Soft warnings = compliance evidence gap |
| Self-hosted TSA | Operational overhead pre-GA |
| Snyk / GitHub Advanced Security | Vendor lock-in + cost |
| Block release on DT outage | Operational drag; CVE matching delayed ≠ bypass |
| SBOM optional flag | Anti-pattern (INV-SUPPLY-SBOM-PRESENT mandatory) |

## References

- [NTIA Minimum Elements for SBOM](https://www.ntia.doc.gov/files/ntia/publications/sbom_minimum_elements_report.pdf)
- [Executive Order 14028](https://www.whitehouse.gov/briefing-room/presidential-actions/2021/05/12/executive-order-on-improving-the-nations-cybersecurity/)
- [CycloneDX 1.5 Specification](https://cyclonedx.org/specification/overview/)
- [Sigstore TSA](https://www.sigstore.dev/)
- [RFC 3161 — Time-Stamp Protocol](https://www.rfc-editor.org/rfc/rfc3161)
- [WI-S12-002 spec](../../04_sprints/S12/work_items/WI-S12-002-sbom-cyclonedx-dependency-track.md)

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-13 | Gustavo Schneiter (via Claude Sonnet 4.6) | Initial ADR for WI-S12-002 SEALED. |
