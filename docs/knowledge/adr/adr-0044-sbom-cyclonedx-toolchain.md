---
type: "ADR"
title: "ADR-0044 (sbom) — SBOM CycloneDX toolchain: cargo-cyclonedx + sbomqs + cosign keyless"
description: "Pins the canonical SBOM generation/validation/signing stack with NTIA 10/10 gate, cosign keyless OIDC via Fulcio+Rekor, a release-only signing guard, and a fail-closed Sigstore-outage policy."
source_files:
  - "specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "sbom", "supply-chain", "cosign", "ci"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0044 (sbom) — SBOM CycloneDX toolchain: cargo-cyclonedx + sbomqs + cosign keyless

ADR-0014 chose CycloneDX 1.5 as the SBOM format but left the concrete CI pipeline unspecified; this
ADR fills that gap by pinning the exact generation/validation/signing toolchain and the operational
policy around it. It is the supply-chain decision record for how every CoreLink build produces a
signed, transparency-logged SBOM. (Note: this is the SBOM-toolchain ADR; two unrelated `ADR-0044-*`
files exist, disambiguated by slug.)

# Context

ADR-0014 decided CycloneDX 1.5+ JSON as the preferred format but left four implementation items open —
generate per build, NTIA-validate, cosign keyless sign, publish as a release asset — which WI-S01-007
closes; this ADR documents the canonical toolchain stack with pinned versions, the Sigstore-outage
failure-mode policy, and the release-only signing guard that limits OIDC `id-token` exposure.

# Decision

The canonical stack is `cargo-cyclonedx` =0.5.9 for generation, `sbomqs` v1.0.5 for NTIA validation
(gate: `avg_score >= 10.0` — all 7 NTIA elements present, zero warning-only), and `cosign` v2.4.1 for
keyless OIDC signing (Fulcio short-lived ≤10min cert + Rekor inclusion proof). Signing runs **only** on
pushes to `main` (`if: github.event_name == 'push' && github.ref == 'refs/heads/main'`) so PRs never
request the OIDC `id-token`, minimizing the OIDC surface; forks and same-repo PRs still run every other
gate. The Sigstore-outage policy is fail-closed: if Fulcio or Rekor is offline the release is blocked
with no silent grace period, because a short-lived cert cannot be cached and "skip + retry" is a
supply-chain anti-pattern.

# Consequences

Supply-chain provenance is publicly verifiable via Rekor, there is zero long-lived key to rotate or
leak, the SBOM is NTIA-compliant out of the box, and the flow is fully automated; the accepted costs
are a hard Sigstore dependency (outage blocks releases, mitigated by a runbook), a GitHub-Actions OIDC
dependency, and the fork-PR limitation that external contributors cannot emit signatures.

# Citations

1. `specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md:30-37` — Context: ADR-0014's four open pipeline items that WI-S01-007 closes.
2. `specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md:43-52` — Decision: the canonical toolchain table (cargo-cyclonedx / sbomqs / cosign with pinned versions).
3. `specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md:67-77` — the release-only signing guard (`if:` on push to `main`) that minimizes OIDC exposure.
4. `specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md:79-88` — the fail-closed Sigstore-outage policy (release blocked, no silent grace period).
5. `specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md:106-118` — Consequences: public/zero-key/automated provenance vs the Sigstore + OIDC + fork-PR costs.
