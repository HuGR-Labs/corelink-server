---
id: "ADR-0045"
type: "adr"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s12", "supply-chain", "slsa", "slsa-l3", "sigstore", "fulcio", "rekor", "in-toto", "high-risk"]
---

# ADR-0045 — SLSA L3 + Rekor Mandatory as CoreLink Build Provenance Standard

## Status

FROZEN (ratified 2026-05-13)

## Context

CoreLink is a multi-tenant content-addressable cache substrate used in CI/CD pipelines for
100+ customers post-GA. It is a high-value target for supply chain attacks (nation-state ROI
elevated; SolarWinds 2020, event-stream 2018, XZ Utils 2024 demonstrate the attack class is
actively exploited in Rust/systems-language ecosystems).

The build pipeline must produce **cryptographically verifiable provenance** that:
1. Cannot be forged by an attacker who does not control the canonical GitHub Actions workflow.
2. Is independently verifiable by customers without trusting the vendor (CoreLink/HuGR Labs).
3. Detects offline tampering of the attestation artifact (attacker replaces attestation file on disk).
4. Satisfies SOC 2 CC6.7 (change management), EO 14028 (SBOM + provenance), and NIST SSDF PS.1.

Competitors operate at SLSA L1 (self-attestation, no hermetic builder, no transparency log) or
at best L2. L3 with Rekor mandatory is the industry-leading posture for OSS Rust SaaS at this
market stage.

## Decision

CoreLink adopts **SLSA Level 3** build provenance as the mandatory standard for all releases
(tagged releases + nightly canary builds) via the following architecture:

### 1. SLSA Generator

`slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@v1.10.0`
SHA-pinned (supply chain pin; bump via ADR amendment + regression testing).

Rationale: Reference implementation maintained by SLSA WG + OpenSSF; audited via OpenSSF
Scorecard; GitHub-managed isolated VM satisfies L3 hermetic builder requirement.

**Rejected alternative:** Hand-rolled generator — violates ADR-0014 (reuse audited upstream).

### 2. Signing: Fulcio Keyless OIDC (No Long-Lived Keys)

All provenance attestations signed via Fulcio keyless OIDC (sigstore.dev public instance).

- GitHub Actions identity bound to the specific workflow ref + commit SHA.
- Short-lived certificate (≤ 10 min validity) eliminates replay attack window.
- Zero long-lived secrets in GitHub Actions (eliminates secret exfiltration threat class).

**Rejected alternative:** Long-lived RSA key in GitHub Secrets — operationally fragile;
rotation overhead; secret exfiltration threat class not eliminated.

### 3. Rekor Transparency Log: MANDATORY (INV-SUPPLY-PROVENANCE-IN-REKOR)

All provenance attestations **must** have a Rekor inclusion proof. This is enforced at:

- **Release pipeline**: `release-slsa3.yml` fails if SLSA generator cannot publish to Rekor.
- **Deploy webhook** (WI-S12-003): Rekor inclusion checked before CF rollout activation.
- **Customer-side CLI** (`corelink-supply-verify`): Rekor bundle absence = hard error (exit 1).

**No graceful fallback**: Attestation without Rekor inclusion proof = SLSA L1 (offline tampering
undetectable). Intentional Rekor disruption + malicious release = attack window. Sigstore.dev
SLA historically 99.9%+; outage > 1h rare. Release publication blocked during outage is the
correct trade-off (operational delay > security bypass).

**Rejected alternative:** Optional Rekor / best-effort / 24h grace period — see Threat Analysis §2.

### 4. in-toto v1.0 Schema: MANDATORY

Only `predicateType: "https://slsa.dev/provenance/v1"` accepted. Old schemas (v0.0.1) rejected.

Rationale: v1.0 stable since Mar 2024; v0.0.1 deprecated. Schema drift = XZ Utils 2024
regression class (attacker generates attestation with unknown predicateType; verifier accepts
due to permissive schema check).

### 5. Customer-Side Verification CLI

`corelink-supply-verify` Rust CLI distributed via `cargo install`. Customers verify provenance
independently without trusting vendor infrastructure:

- `verify --bundle <path> --release <tag> --expected-builder <pattern>`
- `lookup --rekor-log-index <N>` (paranoid mode)
- `extract --bundle <path>` (debug)

## Threat Analysis

### T1: Attestation forge via fork
Attacker stages release from `attacker/corelink-server`; generates valid Fulcio cert
(OIDC bound to fork workflow). Mitigated: customer CLI enforces `--expected-builder HumanGuardrail/corelink-server`.

### T2: Rekor inclusion absent (offline tampering)
Attacker replaces `provenance.intoto.bundle` in release asset after generation.
Mitigated: Rekor mandatory + deploy webhook checks inclusion before rollout.

### T3: DSSE alg=none
Attacker generates DSSE envelope with no signatures (JWT alg=none class).
Mitigated: CLI and library reject empty signature list or empty `sig` field.

### T4: In-toto schema drift
Attacker uses old `predicateType: "https://slsa.dev/provenance/v0.0.1"`.
Mitigated: explicit `predicateType` equality check; only v1.0 accepted.

### T5: Workflow yaml tampering pre-trigger
Attacker with PR write modifies workflow + triggers release.
Mitigated: CODEOWNERS + tag protection + signed commits + Fulcio cert SAN includes workflow SHA1.

## Consequences

### Positive
- SLSA L3 = industry-leading supply chain posture vs OSS Rust SaaS competitors (L1/L2).
- Rekor mandatory = independently auditable (SOC 2 / ISO 27001 auditors can verify externally).
- No long-lived secrets = eliminates secret exfiltration threat class.
- Customer-side verify = defense in depth (server compromise does not break customer trust).

### Negative / Trade-offs
- Release publication blocked during Rekor outage (expected: rare; documented runbook).
- SLSA generator adds ~3 min per release pipeline run (~$0.024/release, negligible).
- `corelink-supply-verify` CLI must be distributed to customers (distribution via cargo install).

## Invariants Introduced

- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH): Every provenance attestation must have a Rekor
  inclusion proof. Release without proof = blocked. No bypass, no waiver.

## Forward Usage

This pattern is reused in:
- S-13 (admin plane): admin operation signing via same Fulcio keyless OIDC pattern.
- S-14 (BYOK): customer key attestation uses same in-toto envelope + Rekor publication.
- S-16 (customer-facing verifier as CF Worker): consumes `corelink-supply-verify` library.

## Version Bump Policy

SLSA generator version pinned at `v1.10.0`. Bumps require:
1. ADR-0045 amendment noting new SHA pin.
2. Adversarial regression test suite green with new version.
3. AppSec + Architect approval.

## References

- SLSA Specification v1.0: https://slsa.dev/spec/v1.0/
- slsa-github-generator v1.10.0: https://github.com/slsa-framework/slsa-github-generator/releases/tag/v1.10.0
- Sigstore Fulcio: https://github.com/sigstore/fulcio
- Rekor: https://github.com/sigstore/rekor
- in-toto Attestation Framework: https://in-toto.io/
- SOC 2 CC6.7, EO 14028, NIST SP 800-218 (SSDF)

## Change Log

| Version | Date | Author | Change |
|---------|------|--------|--------|
| 1.0.0 | 2026-05-13 | Gustavo Schneiter (via Claude) | Initial ratification (WI-S12-001). |
