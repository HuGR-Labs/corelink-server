---
id: "WALKTHROUGH-S12-2026-05-14"
type: "security_walkthrough"
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
tags: ["security-walkthrough", "adversarial", "s12", "supply-chain", "slsa-l3", "cosign", "sbom", "cargo-deny", "dependency-track", "wi-s12-007"]
---

# Security Walkthrough — S-12 Supply Chain Hardening · 2026-05-14

> **Session:** 2h adversarial review | **Date:** 2026-05-14 | **Sprint:** S-12
> **Pentester team:** Gustavo Schneiter (Security Lead + AppSec dual-hat per ADR-0034)
> **Scope:** Full S-12 supply chain surface (SLSA L3 + SBOM + Cosign + cargo-audit/deny + Dependabot + DT + Reproducible Builds)

---

## 0. Executive Summary

2-hour adversarial review of the S-12 supply chain surface conducted on 2026-05-14.
Five adversarial scenarios attempted against the integrated S-12 control stack.

**Results:**
- **P0 (blocker) findings:** 0
- **P1 (must-fix-sprint) findings:** 0
- **P2 (next-sprint) findings:** 1
- **Adversarial scenarios attempted:** 5
- **Scenarios blocked by controls:** 5 / 5 (100%)

**Promotion recommendation:** PROCEED with CONDITIONALLY_APPROVED (P2 deferred waiver).

---

## 1. Scope

| Domain | Coverage |
|---|---|
| SLSA L3 attestation | Provenance forge attempts; Rekor inclusion proof tampering; in-toto schema drift; algorithm confusion |
| SBOM | Tampering post-publish; NTIA placeholder injection; PURL confusion; TSA replay |
| Cosign + CF deploy verify gate | Unsigned deploy attempt; Rekor missing; identity confusion; replay; CF API IAM bypass |
| cargo-audit + cargo-deny + Dependabot | GPL leak; yanked auto-merge; typosquat dep; unmaintained; vendor patch without ADR |
| Dependency-Track | HMAC bypass; alert flood; DT compromise; PURL confusion |
| Reproducible builds | Compromised builder; non-determinism regression; build.rs lint; CPU heterogeneity; rustc upgrade |

---

## 2. Adversarial Scenarios Attempted

### Scenario A — Provenance forge via fork

**Attempt:** Stage a release from a fork with attacker-controlled builder; attempt
to produce a valid SLSA L3 provenance attestation that passes `cosign verify`
and Rekor lookup at the CoreLink deploy verifier.

**Control engaged:** `crates/corelink-deploy-verifier/src/verifier.rs`
`verify_builder_id()` — builder_id in SLSA provenance must match
`https://github.com/slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@refs/tags/v1.10.0`.
Fork produces a different builder_id; verification rejects.

**Result:** BLOCKED. `test_provenance_forge_rejected` adversarial test green.
**Severity:** N/A (blocked). **Finding:** None.

---

### Scenario B — Cosign signature bypass via captured CF API token

**Attempt:** Capture a Cloudflare API token with Workers deploy scope;
attempt direct `wrangler deploy` bypassing the `corelink-deploy-verifier`
Cosign verify step.

**Control engaged:** CF IAM scoping — deploy verifier is injected as a
pre-activation webhook. Workers cannot be deployed via `wrangler deploy`
in production without the webhook returning 200. CF IAM token scoped to
`workers:write` does not bypass the webhook activation gate.
ADR-0025: no operator override exists; fail-closed.

**Result:** BLOCKED. Architecture-level control (CF webhook mandatory path).
**Severity:** N/A (blocked). **Finding:** None.

---

### Scenario C — Dependency-Track fake alert injection

**Attempt:** Gain simulated read-only access to the self-hosted DT instance;
attempt to inject a fake CRITICAL CVE alert for a corelink crate by posting
directly to the DT alert webhook endpoint.

**Control engaged:** `crates/corelink-dt-webhook/src/hmac.rs`
`verify_webhook_hmac()` — HMAC-SHA256 verification of all incoming DT
webhooks using `subtle::ConstantTimeEq`. Forged alert (without valid
HMAC signature) returns 401. `crates/corelink-dt-reconcile/` reconciler
detects state divergence between DT API source-of-truth and webhook-delivered
alerts.

**Result:** BLOCKED. `test_hmac_bypass_rejected` adversarial test green.
**Severity:** N/A (blocked). **Finding:** None.

---

### Scenario D — Dependabot PR replay on closed/reverted branch

**Attempt:** Replay a closed Dependabot PR on a reverted branch to reintroduce
a previously-rejected dependency version bump.

**Control engaged:** GitHub native PR deduplication — Dependabot does not
re-open a PR for a version bump that was previously closed with "Not planned"
or "Close as not planned". Branch protection rules require `supply-chain / cargo-audit`
required status check on every PR; reopened PR would re-trigger CI.

**Result:** BLOCKED. GitHub deduplication + required status check prevents
silent reintroduction.
**Severity:** N/A (blocked). **Finding:** None.

---

### Scenario E — SBOM tampering post-publish

**Attempt:** Swap the `sbom.cdx.json` SBOM artifact on a GitHub Release mirror
with a modified version containing injected components; verify whether the
deploy verifier or DT detects the tampering.

**Control engaged:** SLSA L3 provenance attestation includes the SHA-256 hash
of all release artifacts (including `sbom.cdx.json`) in the `subject[]` array.
The `corelink-supply-verify` verifier checks `verify_subject_hash()` against the
attested material list. Swapped SBOM produces a different hash; verification fails.
Additionally, RFC 3161 TSA timestamp on the original SBOM is not re-issuable
for the modified SBOM.

**Result:** BLOCKED. `test_sbom_tampering_detected` adversarial test green.
**Severity:** N/A (blocked). **Finding:** None.

---

## 3. Findings

### P2 — cargo-vet integration absent

| Field | Value |
|---|---|
| ID | FINDING-S12-W-001 |
| Severity | P2 (next-sprint) |
| Title | `cargo-vet` / `cargo-crev` supply chain review layer not integrated |
| Description | S-12 supply chain controls validate provenance and CVE status but do not have a formal per-crate audit trail via `cargo-vet`. `cargo-vet` would allow designated engineers to vouch for specific crate versions, providing an additional layer of assurance beyond crates.io presence + cargo-audit advisory status. |
| Impact | Without `cargo-vet`, a sophisticated supply chain attack that bypasses RustSec advisory publication (e.g., malicious code not yet discovered) would not be caught by automation until post-publish advisory. CODEOWNERS + lockfile diff are the only human review gates. |
| Mitigating controls | CODEOWNERS mandatory supply-chain reviewer on all `Cargo.lock` changes; lockfile-diff PR comment Action; DT SBOM new-component anomaly; RB-FM-156 + RB-FM-157 operational runbooks. Risk residual: LOW given existing control density. |
| Recommendation | Integrate `cargo-vet` in S-13+ per spec anti-scope (WI-S12-007 §6.2 deferred). Budget: ~4h engineering. |
| Remediation ETA | S-13+ (deferred per anti-scope; waiver W-S12-003 in PRR-S12.md) |
| Status | WAIVED (ADR-0034; S-13+ deferred) |

---

## 4. Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Security Lead (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-14 | ✅ APPROVED |
| AppSec advisor (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-14 | ✅ APPROVED |

**Walkthrough conclusion:** S-12 supply chain surface is well-hardened for
solo-tier launch. P0=0, P1=0. P2 finding documented with waiver. Promotion
to staging-stable recommended as CONDITIONALLY_APPROVED per PRR-S12.md §4.

Next walkthrough: 6 months post-S-12 SEAL (November 2026) OR ad-hoc for
any HIGH_RISK changes in S-13+.
