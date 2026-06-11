---
id: "ADR-0025"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s12", "supply-chain", "cosign", "sigstore", "fulcio", "rekor", "deploy-gate", "hard-gate", "high-risk", "ff-hr-005"]
---

# ADR-0025 — Deploy Gate: Hard Non-Bypassable Cosign Keyless OIDC Verify Gate

> **doc_status:** DRAFT · **audit_status:** ACTIVE
> **Parent:** WI-S12-003 · **Owner:** Gustavo Schneiter

---

## 1. Status

**DRAFT** — ratified at WI-S12-003 SEAL.

## 2. Context

CoreLink deploy pipeline uses Cloudflare Workers as the production runtime.
Without a cryptographic gate between build and deploy, supply chain attacks
materialise at the last mile: an attacker who compromises deploy credentials
(CF API token, GitHub Actions secrets) can push a tampered Worker bundle
without any detection.

The threat class is SolarWinds-class: build pipeline produces valid artifacts,
but a tampered binary is deployed post-build (post-build hijack).  Soft-fail
detection (logging, alerts after the fact) does not prevent customer impact.

Existing mitigations at the time of this ADR:

- **SLSA L3 build provenance** (WI-S12-001): proves the artifact was built by
  hermetic CI; does not prevent deployment of a different artifact.
- **SBOM CycloneDX** (WI-S12-002): CVE monitoring; does not gate deployment.
- **Cargo audit + deny** (WI-S12-004): dependency policy; does not gate deploy.

None of the above enforce that **the deployed artifact is the exact one produced
by the CI pipeline**.  A missing enforcement layer means the chain of trust
breaks at the deploy boundary.

## 3. Decision

Implement `corelink-deploy-verifier` as a Cloudflare Worker that serves as a
**hard gate non-bypassable** between the GitHub Actions release pipeline and
Cloudflare Workers rollout:

1. **Cosign sign (keyless OIDC)**: GitHub Actions identity (OIDC token) → Fulcio
   short-lived certificate (SAN URI = workflow ref) → Cosign signs OCI image
   digest → publishes signature to Rekor transparency log.

2. **CF deploy webhook**: release pipeline triggers `POST /webhook/deploy` to
   `corelink-deploy-verifier`; authenticated via HMAC-SHA256 (quarterly rotation).

3. **Verify pipeline** (all must pass; no bypass mode):
   a. Cosign signature present and cryptographically valid.
   b. Rekor inclusion proof fetched and validated (Merkle root verified).
   c. Fulcio certificate chain valid (TUF-pinned root; auto-rotation via sigstore-rs).
   d. SAN URI matches `^https://github\.com/humangr-labs/corelink-server/\.github/workflows/release-slsa3\.yml@refs/tags/v\d+\.\d+\.\d+$`.
   e. Image digest binding verified (signature binds SHA-256; prevents TOCTOU).

4. **On pass**: verifier calls CF API with pinned digest (not floating tag) →
   `wrangler deploy --version-id <sha256>`.

5. **On fail**: deploy rejected + audit event `dev.hugr.corelink.deploy.blocked.v1`
   emitted (fail-CLOSED) + SEV-2 alert fires.

6. **Audit emit failure**: fail-CLOSED — if audit emit fails, deploy is blocked
   and SEV-1 alert fires (INV-AUDIT-APPEND-ONLY).

**CF API token IAM scoping**: CF API token restricted to `Workers Scripts:Edit`
for `corelink-deploy-verifier` only.  Manual `wrangler deploy` blocked via IAM.

## 4. Alternatives Considered

### 4.1 GitHub Actions deploy step only (rejected)

A verify step in the GitHub Actions workflow catches unsigned deploys but does
not prevent an attacker with stolen CF API credentials from deploying directly
(bypass the pipeline entirely).  The CF Worker gate is the last line of defense.

**Rejected**: does not enforce at the CF API boundary; stolen token = bypass.

### 4.2 Long-lived Cosign signing keys (rejected)

Long-lived keys require secret management, rotation procedures, and are a
exfiltration target.  If compromised, an attacker can sign arbitrary artifacts.

**Rejected**: Cosign keyless OIDC (Fulcio short-lived certs) eliminates this
threat class entirely — no long-lived secret to steal.

### 4.3 Soft-fail / warn-and-allow mode (rejected)

Logging a warning when signature is missing but allowing the deploy would
make INV-SUPPLY-SIGNED-DEPLOY a suggestion, not an invariant.

**Rejected**: anti-pattern per §7; INV-SUPPLY-SIGNED-DEPLOY (CRITICAL) requires
hard enforcement — soft-fail = invariant violation possible.

### 4.4 "Emergency override" bypass mode (rejected)

An operator-triggered bypass during incidents was considered.  Rollback is
the correct recovery path: revert the PR, produce a new signed release, deploy.

**Rejected**: bypass mode = temporary policy downgrade that creates attack
windows; rollback via revert PR + signed release is operationally sound.

### 4.5 mTLS webhook authentication (not selected, HMAC preferred)

mTLS provides stronger guarantees but adds certificate lifecycle overhead
for the webhook caller (GitHub Actions environment).  HMAC-SHA256 with
quarterly rotation (CTRL-AUTH-014) is operationally simpler with equivalent
security for this trust boundary.

## 5. Consequences

### Positive

- **INV-SUPPLY-SIGNED-DEPLOY** (CRITICAL) is cryptographically enforced, not
  just monitored.
- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH) enforced at deploy boundary.
- Zero long-lived signing secrets (Cosign keyless OIDC = Fulcio short-lived cert
  per build).
- Rekor transparency log = publicly auditable supply chain evidence (SOC 2 CC6.7).
- Identity binding: SAN URI pattern prevents fork attacks (attacker's fork
  signature is rejected).
- TOCTOU mitigation: signature binds image digest; tag swap detected.
- Audit fail-CLOSED: tampered deploys cannot be invisible (no silent drop).

### Negative / Trade-offs

- **Rekor outage = deploy blocked** (intentional; no grace period).  Documented
  in runbook; operational runbook `RB-SUPPLY-REKOR-OUTAGE` covers incident
  response.  Post-GA mitigation: Sigstore enterprise instance for on-prem Rekor.
- **Latency budget**: verify pipeline adds ≤ 5s p99 latency to the deploy
  critical path (Cosign fetch + Rekor lookup + Fulcio chain).
- **CF Worker dependency**: verifier is itself a CF Worker; CF outage blocks
  deploys.  Mitigated by CF's 99.99% SLA and multi-region availability.
- **sigstore-rs version pinning**: `0.9.x` pinned; bump requires ADR and
  compatibility testing (R-001 in risk register).

## 6. Security Properties

| Threat | Mitigation | Enforced At |
|---|---|---|
| Post-build artifact swap | Image digest binding | Verify step (e) |
| Fork attack (SAN mismatch) | SAN URI regex check | Verify step (d) |
| Offline signature (no Rekor) | Rekor inclusion mandatory | Verify step (b) |
| Custom CA (non-Fulcio) | TUF-pinned Fulcio root | Verify step (c) |
| Replay attack (old sig) | Image digest binding | Verify step (e) |
| Stolen CF API token | IAM scoping + verifier gate | CF token IAM |
| Stolen signing key | N/A — keyless OIDC | Fulcio OIDC |
| Audit gap (tampering invisible) | Audit fail-CLOSED | emit step |
| Webhook DoS | Rate limit (10/hr/src) + CF DDoS | Worker rate limit |

## 7. Invariants Impacted

- **INV-SUPPLY-SIGNED-DEPLOY** (CRITICAL): PRIMARY implementation.
- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH): enforced at deploy boundary.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL): audit emit fail-CLOSED.

## 8. Compatibility

- `DeployVerifier` Rust trait: breaking changes require major version bump
  (per WI-S12-003 §14.s12.003.8).
- `POST /webhook/deploy` endpoint: wire schema versioned as `webhook-v1.json`
  in `specs/_schemas/`; backward-compatible additions only in v1.
- Cosign image tag format: `ghcr.io/humangr-labs/corelink-worker:vX.Y.Z`
  (semver tag mandatory; no `latest` or mutable tags).

## 9. Reusability

This deploy gate pattern is reusable for:

- **S-13 admin plane**: admin operation signing (admin op + Cosign
  signature verifier in admin plane Worker).
- Future Workers: any Worker with a release pipeline can adopt the
  same `corelink-deploy-verifier` webhook pattern.

## 10. References

- WI-S12-003 §1 Intent + §9 Design Decisions.
- Sigstore project: <https://www.sigstore.dev/>.
- Cosign documentation: <https://docs.sigstore.dev/cosign/overview/>.
- Fulcio (Cosign CA): <https://github.com/sigstore/fulcio>.
- Rekor (transparency log): <https://github.com/sigstore/rekor>.
- ADR-0043 (HMAC tenant prefix algorithm).
- SOC 2 CC6.7 (signed deploy mandatory) + CC8.1 (change management).
- NIST SP 800-218 SSDF PS.1 (produce well-secured software).

## 11. Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | WI-S12-003 implementation (ADR ratified at SEAL). |
