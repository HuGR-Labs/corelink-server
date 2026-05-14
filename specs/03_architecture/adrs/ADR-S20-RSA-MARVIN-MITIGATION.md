---
id: "ADR-S20-RSA-MARVIN-MITIGATION"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
wi: "R1-9-DEP-UPDATE"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - { role: "security_lead", name: "Gustavo Schneiter (interim until hire)" }
  - { role: "security", name: "Crypto SME — TBD (assign at next quarterly security review)" }
supersedes: null
superseded_by: null
tags: ["adr", "security", "supply-chain", "rsa", "marvin-attack", "rustsec-2023-0071", "decision-matrix", "r1-9"]
---

# ADR-S20-RSA-MARVIN-MITIGATION — `rsa` 0.9.x Marvin Timing-Sidechannel Decision

## Status

**DRAFT** — filed 2026-05-14 as part of R1-9 emergency dep update.
Pending review by security_lead + crypto_sme. Default-recommended path:
**Option B (Waiver + Mitigations)** pending sign-off; Option A retained as
the strict long-term replacement plan if upstream `rsa` 0.10 ships a
constant-time scalar.

## Context

### The advisory

`RUSTSEC-2023-0071` — Marvin Attack: potential key recovery through timing
sidechannels in the `rsa` crate (all versions ≤ 0.9.10). The Marvin attack
extends Bleichenbacher/Manger oracle attacks to recover RSA private keys
when the attacker can measure decryption latency with sub-microsecond
precision over many (10⁴–10⁶) handshakes.

CVSS 5.9 (Medium). No upstream fix is available as of 2026-05-14. The
`RustCrypto/RSA` maintainers have stated that a constant-time implementation
is on the 0.10 roadmap but no shipping date is committed.

### Where CoreLink uses `rsa`

| Crate                        | Cargo.toml line             | Feature gate     | Usage                                                                                  |
|------------------------------|-----------------------------|------------------|----------------------------------------------------------------------------------------|
| `corelink-clerk`             | `crates/corelink-clerk/Cargo.toml:62` | `test-utils`     | **Test-only** — generates ephemeral RSA keys in `fakes.rs` for JWT test fixtures.       |
| `corelink-dpa-acceptance`    | `crates/corelink-dpa-acceptance/Cargo.toml:40` | (always)         | DPA-acceptance receipts: RS256-sign DPA payload with operator key.                      |
| `corelink-worker`            | `crates/corelink-worker/Cargo.toml:105` | (always)         | RS256 signing of consent receipts + audit chain anchoring receipts.                     |

All three usages are **signing-side** (`pkcs1v15_sign` / `pss::Signer`). RSA
**decryption** (the classic Bleichenbacher/Marvin oracle surface) is **NOT**
in any CoreLink production codepath. KMS-wrap is AES-GCM (BYOK envelope);
TLS termination uses `rustls` with the modern `aws-lc-rs` provider; PAT/JWT
**verify** uses Ed25519 + HMAC-SHA256 — RSA is only present on the
**operator/signer** side for receipt generation.

### Threat model

The Marvin attack requires:

1. **Remote attacker** with high-precision latency observability against the
   RSA *decryption* primitive.
2. **Adaptive chosen-ciphertext oracle** — the victim must decrypt
   attacker-supplied ciphertexts and leak (via timing) whether padding parsed
   correctly.
3. **10⁴–10⁶ queries** with stable network jitter (typically same-datacenter
   or co-located VM).

For CoreLink, **none of these preconditions hold for the production signing
codepath**:

- The operator's RSA *signing* key never decrypts attacker-chosen
  ciphertexts. It signs canonical-JCS-serialized payloads that the operator
  itself constructs.
- The only consumer of the public key (`corelink-client-verify` via
  receipt-verify) performs **public-key verification only**, which has no
  secret-dependent timing.
- If an attacker has co-located timing observability against the
  operator's signing process, they already have a much larger compromise
  (process-level access, kernel-side-channel, or VM escape) — the Marvin
  exfiltration path is strictly weaker than what they already possess.

## Decision Matrix

| Dimension                                  | Option A — Replace `rsa` with `aws-lc-rs` / `ring`              | Option B — Waiver + ADR + operational mitigations              |
|--------------------------------------------|-----------------------------------------------------------------|----------------------------------------------------------------|
| **Constant-time guarantee**                | YES — `aws-lc-rs` is FIPS-validated, BoringSSL-derived          | NO — relies on threat-model exclusion                          |
| **Effort**                                 | HIGH — `ring` does not expose raw RSA sign API; `aws-lc-rs` exposes via `RsaKeyPair::sign` but key formats differ (PKCS#8 DER vs PEM). Rewrite required in `corelink-worker`, `corelink-dpa-acceptance`, `corelink-clerk::fakes`. Est. 2–3 days incl. tests. | LOW — write this ADR, document mitigations, add monitoring.    |
| **API surface affected**                   | `RsaPrivateKey::from_pkcs8_pem` → `RsaKeyPair::from_pkcs8` (DER); `pkcs1v15_sign` → `sign(&RSA_PKCS1_SHA256, ...)` | None.                                                          |
| **Test fixtures**                          | Must regenerate JWT test fixtures in `corelink-clerk::fakes`    | None.                                                          |
| **Risk of regression**                     | Medium — receipt-chain compatibility must survive (RS256 wire format is stable, but PEM/DER serialization differences are easy to typo) | Zero (no code change).                                         |
| **Audit-cleanliness post-fix**             | `cargo audit` reports 0 RSA advisories                          | `cargo audit` still reports RUSTSEC-2023-0071; deny.toml waiver required. |
| **Long-term posture**                      | Strong — eliminates the entire crate from the supply chain      | Carries supply-chain debt until `rsa` 0.10 ships               |
| **Aligns with charter (`#[non_exhaustive]`, no unsafe, deterministic)** | YES                                                | YES                                                            |
| **Mitigations available if accepted**      | N/A                                                             | (1) Quarterly RS256 key rotation. (2) Anomalous-signing-latency alert (P99 + 3σ → page). (3) Signing-process isolation (per-tenant VM, no co-located untrusted workloads). |

## Decision

**RECOMMENDED: Option B (Waiver + Mitigations).** Pending crypto_sme sign-off.

Rationale: the Marvin attack threat-model preconditions do not hold for
CoreLink's RSA usage (signing-only, no chosen-ciphertext oracle, no
co-located timing observability against the signing process). Option A's
effort cost (2–3 days + regression risk in the receipt chain that S-11/S-14
already SEALed) is not justified by the residual risk reduction. The
mitigations below restore defence-in-depth without rewriting battle-tested
signing code.

**Adopted iff** `rsa` 0.10 with documented constant-time guarantees ships
before R2 GA closes: revisit and switch (low-effort `cargo update` at
that point).

### Conditional auto-promotion to Option A

If **any** of the following triggers fire, Option B is **automatically void**
and Option A becomes mandatory within 30 days:

1. CoreLink adds an RSA *decryption* codepath (e.g. legacy enterprise SSO
   that ingests RSA-OAEP-wrapped tokens). Marvin's oracle surface re-opens.
2. A new RUSTSEC bumps the `rsa` advisory from Medium → High.
3. PoC tooling against `rsa` 0.9 becomes publicly available with documented
   recovery time < 24h on a same-region adversary.
4. FIPS 140-3 audit gate requires a constant-time signing primitive.

## Mitigations (in force from this ADR's ACTIVE date)

1. **RS256 key rotation: every 90 days, automated.** Existing rotation
   infrastructure (`corelink-rotation-worker`) gains an `rs256-receipt-key`
   schedule; old keys retained for verify-only for 12 months per audit chain
   replay window.
2. **Signing-latency anomaly alert.** New SLO: `rs256_sign_duration_seconds`
   P99 stays within `mean ± 3σ` over a 7-day rolling window. Breach pages
   security on-call. Baseline established within 30 days of ADR ACTIVE.
3. **Signing-process isolation.** Receipt signers run in dedicated
   single-tenant CF Workers (no shared-CPU oracle surface). Documented in
   the S-11 deployment topology.
4. **Quarterly review.** This ADR is re-evaluated at every quarterly
   security review; if `rsa` 0.10 has shipped, schedule the Option A
   migration in the next sprint.

## Consequences

- **`cargo audit` will continue to flag RUSTSEC-2023-0071** until either
  (a) `rsa` 0.10 ships or (b) Option A is executed. The `deny.toml`
  `advisories.ignore` list will carry an entry with this ADR ID as
  justification.
- **Supply-chain SBOM** (CycloneDX) will continue to include the `rsa` 0.9.x
  component with the advisory cross-referenced in the VEX statement (vector
  = "not affected" with justification = "vulnerable-code-not-in-execute-path"
  per CycloneDX VEX taxonomy).
- **No code changes required in this PR.** Mitigations 1-4 ship as
  follow-on WIs in R-2 W2.

## References

- `RUSTSEC-2023-0071`: https://rustsec.org/advisories/RUSTSEC-2023-0071
- Marvin Attack paper: Adamczyk et al., "Everlasting ROBOT: the Marvin
  Attack" (2023)
- `AUDIT-R1-8-DEPENDENCY-SECURITY` §2 (audit doc that surfaced this finding)
- CycloneDX VEX justification taxonomy:
  https://cyclonedx.org/docs/1.4/json/#vulnerabilities_items_analysis_justification

## Reviewer ack tracking

- [ ] security_lead — Gustavo Schneiter
- [ ] crypto_sme — TBD (assign at next security review)
