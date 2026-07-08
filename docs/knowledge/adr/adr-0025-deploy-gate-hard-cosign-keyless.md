---
type: "ADR"
title: "ADR-0025 — Hard non-bypassable cosign keyless deploy gate"
description: "Why a Cloudflare-Worker deploy verifier cryptographically gates every Worker rollout on a cosign keyless-OIDC signature with mandatory Rekor inclusion, with no soft-fail or override mode."
source_files:
  - "specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md"
checkpoint_sha: "b5ce2bff384a09047f027082dcf4355136822242"
provenance: "AUTHORED"
tags: ["adr", "supply-chain", "cosign", "sigstore", "rekor", "deploy-gate", "s12"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0025 — Hard non-bypassable cosign keyless deploy gate

The deploy boundary is the last mile where a SolarWinds-class supply-chain attack lands: a tampered Worker bundle pushed with stolen deploy credentials would otherwise reach production undetected, because SLSA provenance, SBOMs, and cargo-audit all prove things about the *build* but none of them enforce that the *deployed* artifact is the one CI produced. This ADR records the decision (a DRAFT, ratified at WI-S12-003 SEAL) to put a hard cryptographic gate at the CF API boundary itself.

# Context

SLSA L3 provenance, CycloneDX SBOMs, and cargo-audit+deny each harden the build but do not gate deployment, so an attacker who steals a CF API token or GitHub Actions secret can deploy a different artifact and break the chain of trust at the deploy boundary; soft-fail detection after the fact does not prevent customer impact (ADR-0025:30-50).

# Decision

`corelink-deploy-verifier` is a Cloudflare Worker that hard-gates rollout: GitHub Actions OIDC identity → Fulcio short-lived cert → cosign signs the OCI digest → signature published to Rekor; a HMAC-authenticated deploy webhook then runs a verify pipeline where *all* checks must pass — valid signature, fetched Rekor inclusion proof, TUF-pinned Fulcio chain, an exact SAN-URI regex binding the release workflow ref (the `HumanGuardrail/corelink-server` release-slsa3 workflow), and image-digest binding against TOCTOU — and on failure the deploy is rejected fail-CLOSED with an audit event, with no soft-fail and no emergency-override mode (rollback via a fresh signed release is the recovery path) (ADR-0025:52-82, ADR-0025:101-115).

# Consequences

`INV-SUPPLY-SIGNED-DEPLOY` becomes cryptographically enforced rather than monitored, with zero long-lived signing secrets and a publicly auditable Rekor trail, traded against the deliberate operational cost that a Rekor outage blocks deploys (no grace period), the verifier is itself a CF-Worker dependency, and the pipeline adds ≤ 5 s p99 to the deploy critical path (ADR-0025:124-150).

# Citations

1. `specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md:30-50` — the post-build hijack threat and why existing mitigations don't gate deploy (Context).
2. `specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md:52-82` — the hard non-bypassable verifier: keyless OIDC, Rekor inclusion, SAN regex, digest binding, fail-closed (Decision).
3. `specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md:101-115` — soft-fail and emergency-override modes explicitly rejected (Decision).
4. `specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md:124-150` — Rekor-outage-blocks-deploy and the latency/dependency trade-offs (Consequences).
5. `specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md:26` — DRAFT status, ratified at WI-S12-003 SEAL (Status).
