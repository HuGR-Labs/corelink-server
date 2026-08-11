# SLSA L3 Build Provenance Pipeline (WI-S12-001)

Internal documentation for the CoreLink SLSA L3 supply chain hardening pipeline.

> **Note on `HumanGuardrail/corelink-server` below:** the GitHub org moved to
> `HuGR-Labs` (2026-08-01), but the provenance/builder-identity strings on
> this page are deliberately left as `HumanGuardrail/corelink-server` because
> the code that emits and verifies them (Sigstore builder ID, `--expected-builder`
> default, cosign/provenance config) still hardcodes the old org. Doc and code
> must move together — this is out of scope for a docs-only fix and is pending
> the org-rename landing in code.

## Overview

CoreLink generates **SLSA Level 3 build provenance** for every release (tagged + nightly canary)
via GitHub Actions + sigstore/Fulcio + Rekor transparency log. This satisfies:
- SOC 2 CC6.7 (change management with signed deploy + provenance)
- EO 14028 (US federal software supply chain requirements)
- NIST SSDF PS.1 (cryptographic integrity)
- INV-SUPPLY-PROVENANCE-IN-REKOR (Rekor inclusion mandatory)

## Architecture Sequence

```
Release tag published
        │
        ▼
┌──────────────────────────────────────┐
│  Job 1: build-artifacts              │
│  (ubuntu-22.04, GitHub-managed)      │
│                                      │
│  cargo build --release               │
│    --target wasm32-unknown-unknown   │
│    -p corelink-worker                │
│                                      │
│  sha256sum → artifact-digest         │
│  base64-encode subjects              │
└───────────────┬──────────────────────┘
                │ outputs: base64-subjects
                ▼
┌──────────────────────────────────────────────────────┐
│  Job 2: slsa-provenance                              │
│  (slsa-github-generator isolated VM — L3 hermetic)   │
│                                                      │
│  generator_generic_slsa3.yml@v1.10.0                 │
│                                                      │
│  1. Read artifact subjects (base64-encoded)          │
│  2. Generate in-toto v1.0 Statement                  │
│     predicateType: https://slsa.dev/provenance/v1    │
│  3. Request Fulcio OIDC cert                         │
│     → GitHub Actions identity bound to workflow ref  │
│     → 10 min validity (keyless; no long-lived keys)  │
│  4. Sign DSSE envelope with Fulcio cert              │
│  5. Publish to Rekor transparency log                │
│     → Merkle inclusion proof embedded in bundle      │
│  6. Output: provenance.intoto.jsonl                  │
│             provenance.intoto.bundle                  │
└───────────────┬──────────────────────────────────────┘
                │
                ▼
┌──────────────────────────────────────┐
│  Job 3: attach-release-assets        │
│                                      │
│  Verify SHA-256 digest matches       │
│  Upload WASM + bundle to release     │
│  Emit SLSA attestation metric        │
└──────────────────────────────────────┘

                       ┌──────────────┐
    Rekor Merkle tree: │ Public log   │
    (append-only)      │ entry visible│
                       │ via:         │
                       │ rekor-cli    │
                       │ cURL         │
                       └──────────────┘
```

## Verification Flow (Customer-Side)

```
Customer downloads:
  - corelink-worker.wasm (release asset)
  - provenance.intoto.bundle (release asset)

Customer installs CLI:
  cargo install corelink-supply-verify

Customer runs:
  corelink-supply-verify verify \
    --bundle provenance.intoto.bundle \
    --release v0.X.Y \
    --expected-builder HumanGuardrail/corelink-server

CLI performs:
  1. Parse DSSE envelope → reject if alg=none or no signatures
  2. Decode payload → parse in-toto v1.0 Statement
  3. Validate predicateType == "https://slsa.dev/provenance/v1"
  4. Extract Fulcio cert → validate PEM format + SAN URI extraction
  5. Match builder identity → reject if not "HumanGuardrail/corelink-server"
  6. Validate Rekor inclusion proof (MANDATORY; no bypass)
     → Merkle root hash 64-char hex
     → log_index < tree_size
     → inclusion hashes non-empty
  7. Return VerifiedProvenance on success

Output (success):
  SLSA L3 provenance verification PASSED for release v0.X.Y
    builder_id:    https://github.com/HumanGuardrail/corelink-server/...
    commit_sha:    <40-char SHA>
    workflow_ref:  refs/tags/v0.X.Y
    rekor_index:   <log index>
    rekor_url:     https://rekor.sigstore.dev/api/v1/log/entries?logIndex=...
```

## Security Properties

| Property | Implementation | Invariant |
|----------|---------------|-----------|
| Hermetic build | GitHub-managed isolated VM (L3 requirement) | Builder cannot exfiltrate deps mid-build |
| No long-lived secrets | Fulcio keyless OIDC (10 min cert) | No secret to exfiltrate from CI |
| Tamper-evident | Rekor Merkle tree inclusion (mandatory) | INV-SUPPLY-PROVENANCE-IN-REKOR |
| Identity binding | Fulcio cert SAN URI = workflow ref + commit SHA | Forge from fork detectable |
| Schema pinned | predicateType = "https://slsa.dev/provenance/v1" only | XZ Utils 2024 class mitigated |

## Failure Modes + Escalation

| Failure | Behavior | Alert | Escalation |
|---------|----------|-------|-----------|
| Rekor outage (503) | Release publication BLOCKED | SEV-2 | Ops on-call; wait recovery |
| Fulcio cert issuance fail | Release BLOCKED | SEV-2 | Ops on-call; re-trigger |
| Builder identity mismatch | CLI exit 1 | Customer alert | Security team review |
| DSSE alg=none detected | CLI exit 1 | Immediate | CRITICAL post-mortem |
| Rekor inclusion proof absent | CLI exit 1; deploy blocked | SEV-1 | Security incident |
| in-toto schema drift | CLI exit 1 | SEV-2 | Schema migration review |

## Audit Evidence Collection

For SOC 2 / ISO 27001 audits, generate evidence:

```bash
#!/usr/bin/env bash
# audit-evidence-collect.sh — collect Rekor evidence for all releases in last 30d
set -euo pipefail

RELEASES=$(gh release list --limit 30 --json tagName -q '.[].tagName')
for TAG in $RELEASES; do
    echo "=== $TAG ==="
    gh release download "$TAG" --pattern "provenance.intoto.bundle" -O "/tmp/provenance-$TAG.bundle" 2>/dev/null || continue
    corelink-supply-verify verify \
        --bundle "/tmp/provenance-$TAG.bundle" \
        --release "$TAG" \
        --expected-builder "HumanGuardrail/corelink-server" \
        --format json
done
```

## Nightly Canary

Nightly builds also generate SLSA L3 attestation (provenance attestation only for tagged releases
per current workflow; nightly canary provenance added when canary pipeline is extended).

## ADR Reference

ADR-0045: SLSA L3 + Rekor mandatory ratification (specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md)

## Metrics

| Metric | Labels | Target |
|--------|--------|--------|
| `corelink_supply_slsa_attestations_total` | `outcome`, `plan` | outcome=ok ≥ 99.9% of releases |
| `corelink_supply_rekor_inclusion_proof_verify_total` | `outcome` | outcome=ok ≥ 99.9% |
| `corelink_supply_fulcio_chain_validate_total` | `outcome` | outcome=ok ≥ 99.9% |
| `corelink_supply_slsa_attestation_generation_duration_seconds_bucket` | `plan` | p99 ≤ 180s |

## WI References

- WI-S12-001: this document (SLSA L3 + Rekor workflow + CLI)
- WI-S12-003: Cosign + CF deploy webhook verifier (consumes provenance attestation)
- WI-S12-002: SBOM CycloneDX (parallel release asset)
- WI-S12-007: PRR ship gate
