---
id: "ASVS-CHECKLIST-S12"
type: "audit"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "S-12"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["checklist", "owasp-asvs", "ssdf", "eo14028", "compliance", "s12", "supply-chain", "wi-s12-007"]
references:
  - "specs/04_sprints/_sealed/S12/_spec_contract.md"
  - "specs/04_sprints/_sealed/S12/work_items/WI-S12-007-rb-fm-156-rb-fm-157-prr-ship-gate.md"
---

# OWASP ASVS V14 + V11.1 + SSDF + EO 14028 Compliance Checklist — S-12

> **Sprint:** S-12 Supply Chain Hardening | **Date:** 2026-05-14
> **Scope:** S-12 supply chain surface (SLSA L3 + SBOM + Cosign + cargo-audit/deny + Dependabot + DT + Reproducible Builds)

---

## OWASP ASVS V14 — Build and Deploy Pipeline

| # | Control | Requirement | Status | Evidence |
|---|---|---|---|---|
| V14.1.1 | Build pipeline reproducibility | Build artifacts are reproducible from source | ✅ PASS | WI-S12-006: 2-runner diff ≤ 5%; SOURCE_DATE_EPOCH; rust-toolchain.toml pinned |
| V14.1.2 | Dependency pinning | All dependencies pinned to specific versions | ✅ PASS | `Cargo.lock` committed at root; cargo-deny `yanked = "deny"` |
| V14.1.3 | Dependency vulnerability scanning | Automated vulnerability scanning of all dependencies | ✅ PASS | cargo-audit daily cron + CI PR gate; DT continuous CVE matching |
| V14.1.4 | Build integrity verification | Build artifacts verified before deployment | ✅ PASS | Cosign keyless OIDC sign + CF deploy webhook verify (ADR-0044 fail-closed) |
| V14.2.1 | Artifact signing | Release artifacts signed with cryptographic key | ✅ PASS | Cosign keyless OIDC (Fulcio short-lived cert per build); SLSA L3 provenance signed via sigstore |
| V14.2.2 | Signature verification before deploy | Signature verified before artifact activation | ✅ PASS | `corelink-deploy-verifier` Cosign verify + Rekor inclusion proof mandatory pre-rollout |
| V14.2.3 | Provenance transparency | Build provenance publicly verifiable | ✅ PASS | Rekor transparency log; SLSA L3 attestation public (INV-SUPPLY-PROVENANCE-IN-REKOR) |
| V14.2.4 | License compliance | No copyleft-incompatible licenses in transitive deps | ✅ PASS | cargo-deny `[licenses]` 7-OSI allowlist; GPL/AGPL/SSPL banned; CI gate enforced |
| V14.3.1 | Supply chain bill of materials | SBOM generated for every release | ✅ PASS | SBOM CycloneDX 1.5+ via `sbom.yml` workflow; GitHub release asset + DT ingestion |
| V14.3.2 | SBOM minimum elements | SBOM meets regulatory minimum elements | ✅ PASS | NTIA minimum elements validated via `cyclonedx-cli validate --minimum-required-fields ntia` |
| V14.3.3 | SBOM timestamp integrity | SBOM timestamp attested | ✅ PASS | RFC 3161 TSA timestamp via Sigstore TSA |
| V14.4.1 | Security advisory monitoring | Monitor for security advisories affecting deps | ✅ PASS | RustSec advisory DB via cargo-audit daily + Dependabot security PRs; DT CVE continuous |
| V14.4.2 | Vulnerability remediation SLA | SLA for vulnerability remediation defined | ✅ PASS | HIGH: ≤ 7d; MEDIUM: ≤ 30d; per sprint contract §9 + RB-FM-156 |
| V14.5.1 | CI/CD pipeline integrity | CI/CD pipeline protected from modification | ✅ PASS | SLSA L3 GitHub Actions isolated VM; minimal permissions; no self-hosted runners in critical path |
| V14.5.2 | Secrets management in CI/CD | No long-lived secrets in CI/CD | ✅ PASS | Cosign keyless OIDC = no long-lived signing keys; CF API tokens scoped minimally |

**V14 total: 15 / 15 PASS (100%).**

---

## OWASP ASVS V11.1 — Business Logic

| # | Control | Requirement | Status | Evidence |
|---|---|---|---|---|
| V11.1.1 | Prevent automated attacks | Supply chain automation attacks prevented | ✅ PASS | Cosign verify gate prevents automated unsigned deploy; cargo-deny prevents automated GPL dep merge |
| V11.1.2 | Workflow integrity | Business workflows cannot be bypassed | ✅ PASS | ADR-0044 + ADR-0045: no operator override for Cosign/Rekor gates; fail-closed canonical |
| V11.1.3 | Validate high-value transactions | High-risk operations require elevated verification | ✅ PASS | Deploy = high-risk; Cosign + Rekor + SLSA provenance all required before activation |
| V11.1.4 | Prevent unauthorized state transitions | Supply chain state transitions authorized | ✅ PASS | INV-SUPPLY-SIGNED-DEPLOY enforced at CF webhook layer; unsigned artifact cannot transition to "deployed" |
| V11.1.5 | Audit trail for high-risk operations | All supply chain operations audited | ✅ PASS | Cosign sign + deploy verify → audit chain event `corelink.supply.deploy_verified`; SLSA provenance in Rekor public log |
| V11.1.6 | Limit automated bulk operations | Dependabot auto-merge limited to safe subset | ✅ PASS | `dependabot-auto-merge.yml`: minor patches only; security updates require human review; no bulk force-merge |
| V11.1.7 | Prevent timing attacks | Constant-time comparisons where applicable | ✅ PASS | DT HMAC verify uses `subtle::ConstantTimeEq`; Cosign keyless OIDC token compare in verifier |
| V11.1.8 | Alert on anomalous conditions | Supply chain anomalies trigger alerts | ✅ PASS | DT CVE alert ≤ 15 min p99; cargo-audit advisory alert via Slack + PagerDuty; lockfile diff PR comment |

**V11.1 total: 8 / 8 PASS (100%).**

---

## NIST SP 800-218 SSDF (Secure Software Development Framework)

### PS.1 — Protect all forms of code from unauthorized access and tampering

| Requirement | Status | Evidence |
|---|---|---|
| PS.1.1: All code stored in protected repos | ✅ PASS | GitHub private repo; branch protection + CODEOWNERS required reviews |
| PS.1.2: Integrity of code verified when used | ✅ PASS | SLSA L3 provenance + Cosign signature + Rekor inclusion proof on every release |
| PS.1.3: Code signing applied | ✅ PASS | Cosign keyless OIDC (Fulcio short-lived cert per GitHub Actions OIDC identity) |

### PW.4 — Reuse existing, well-secured software when feasible

| Requirement | Status | Evidence |
|---|---|---|
| PW.4.1: Inventory third-party software components | ✅ PASS | SBOM CycloneDX 1.5+ on every release; DT continuous inventory |
| PW.4.2: Verify integrity of acquired software | ✅ PASS | `Cargo.lock` cryptographic hash of every dep; cargo-deny sources allowlist (crates.io only) |
| PW.4.3: Identify security requirements for third-party components | ✅ PASS | cargo-audit RustSec advisory monitoring; DT CVE matching; license allowlist |
| PW.4.4: Assess and remediate vulnerabilities in third-party components | ✅ PASS | cargo-audit daily + HIGH ≤ 7d SLA; DT alert ≤ 15 min; Dependabot auto-merge minor |

**SSDF total: 7 / 7 PASS (100%).**

---

## Executive Order 14028 — Improving the Nation's Cybersecurity (SBOM mandatory)

| Requirement | Status | Evidence |
|---|---|---|
| SBOM for all software sold to federal agencies | ✅ PASS | SBOM CycloneDX 1.5+ generated on every release via `sbom.yml`; GitHub release asset |
| SBOM minimum elements (NTIA) | ✅ PASS | NTIA minimum elements validated in CI (`cyclonedx-cli validate --minimum-required-fields ntia`) |
| SBOM accessible format (CycloneDX or SPDX) | ✅ PASS | CycloneDX 1.5+ JSON format (industry-standard; spec contract §5.2 R-S12-3) |
| Supplier name, component name, version, unique identifier | ✅ PASS | All 4 NTIA mandatory fields present + validated; PURL `pkg:cargo/<name>@<version>` as unique ID |
| Dependency relationships | ✅ PASS | CycloneDX `dependencies[]` array includes full transitive dep graph from `cargo-cyclonedx` |
| Author of SBOM | ✅ PASS | CycloneDX `metadata.tools[]` includes `cargo-cyclonedx` as SBOM author tool |
| Timestamp | ✅ PASS | RFC 3161 TSA timestamp via Sigstore TSA; cryptographically attested creation time |

**EO 14028 total: 7 / 7 PASS (100%).**

---

## SOC 2 Controls (Supply Chain-related)

| Control | Requirement | Status | Evidence |
|---|---|---|---|
| CC6.7 | Signed artifacts only deployed | ✅ PASS | Cosign verify gate (ADR-0044); unsigned deploy = rollout blocked |
| CC7.1 | Vulnerability detection continuous | ✅ PASS | DT CVE alerts ≤ 15 min p99; cargo-audit daily; Dependabot weekly |
| CC8.1 | System change management with integrity verification | ✅ PASS | SLSA L3 provenance on every release; lockfile diff review required; CODEOWNERS |

---

## Summary

| Framework | Controls | Pass | Fail |
|---|---|---|---|
| OWASP ASVS V14 | 15 | 15 | 0 |
| OWASP ASVS V11.1 | 8 | 8 | 0 |
| NIST SSDF PS.1 + PW.4 | 7 | 7 | 0 |
| EO 14028 | 7 | 7 | 0 |
| SOC 2 (supply chain) | 3 | 3 | 0 |
| **TOTAL** | **40** | **40** | **0** |

**Overall: 40 / 40 PASS (100%).**

All supply chain compliance requirements satisfied for S-12 scope. Deferred
items (SOC 2 Type II formal audit engagement, FedRAMP Moderate, ISO/IEC 27036)
per sprint contract §10 anti-scope — not applicable at solo-tier launch phase.
