---
id: "DD-AWS-KMS-2026-05-15"
type: "vendor_due_diligence"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R5-3-GAP-14-VENDOR-RISK"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["VENDOR-RISK-REGISTER-2026-05-15", "VENDOR-RISK-METHODOLOGY-2026-05-15"]
tags: ["soc2", "cc9.2", "vendor-dd", "critical", "aws", "kms", "byok", "fips", "gap-14"]
---

# Vendor DD — Amazon Web Services (AWS KMS)

> **doc_status:** ACTIVE · Register row: 4 · Category: **Critical** · Residual risk: **2.0** (Inherent 20 × CEF 0.10).
>
> AWS KMS is one of four BYOK key backends (alongside GCP KMS, Azure Key Vault, HashiCorp Vault). Customer holds the CMK in **their own AWS account**; CoreLink only invokes `Encrypt`/`Decrypt` operations.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | Amazon Web Services, Inc. (AWS) — subsidiary of Amazon.com, Inc. |
| HQ | 410 Terry Avenue North, Seattle, WA 98109, USA |
| CoreLink relationship | (a) staging account `corelink-byok-staging` (AWS-side, CoreLink-owned, for BYOK matrix CI) + (b) per-customer accounts (customer-owned, customer-IAM-granted cross-account access to CoreLink role) |
| Contract effective | 2026-04-23 (AWS Customer Agreement + Service Terms accepted) |
| Account contacts | (recorded in `legal/vendor-contacts.md`) |

## 2. Service scope

- **KMS (Key Management Service)** — symmetric AES-256 CMKs in customer-managed key state
- **FIPS 140-3 validated endpoints** — `kms-fips.<region>.amazonaws.com` (toggled via `AWS_USE_FIPS_ENDPOINT=true` per `secrets-checklist.md` row 27)
- **IAM cross-account role assumption** — customer grants CoreLink role narrowly scoped `kms:Encrypt`/`kms:Decrypt`/`kms:GenerateDataKey` on a single CMK ARN
- **CloudTrail** — customer holds audit log of every CoreLink-initiated KMS call (this is **the customer's evidence trail**, not CoreLink's; intentional asymmetry)
- **Secrets Manager mirror** — defense-in-depth mirror of CoreLink staging access keys (`aws-sm-mirror` storage tier in `secrets-checklist.md`)

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Data encryption keys (DEKs) wrapped under CMK | CoreLink → AWS KMS (envelope encryption) | AWS-managed (HSM-backed for CMK material) | TLS 1.3 | **Customer** (CMK material never leaves AWS HSM) |
| Plaintext customer data | (never) | — | — | — |
| KMS API request metadata | CoreLink → AWS | AWS-managed | TLS 1.3 | AWS / Customer (via CloudTrail) |
| CoreLink staging IAM access keys | CoreLink-owned | `gha-secret` + `aws-sm-mirror` | TLS 1.3 | CoreLink |

The architectural invariant **INV-BYOK-CMK-NEVER-LEAVES-CUSTOMER (`compliance/byok-fips-matrix.md`)** means an AWS KMS compromise affects DEK wrapping but does not directly leak customer plaintext (which is decrypted only briefly in Workers RAM during request lifetime).

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 1 / SOC 2 / SOC 3 Type II | Attested | AWS Artifact → SOC reports (Drata-pulled annually) |
| ISO 27001 / 27017 / 27018 / 9001 | Certified | AWS Artifact |
| FedRAMP High | Authorized | AWS GovCloud + commercial regions |
| PCI-DSS Level 1 | Attested (Service Provider) | AWS Artifact |
| HIPAA | BAA available; CoreLink BAA not signed (no PHI) | — |
| FIPS 140-2 / 140-3 | KMS HSMs validated; FIPS endpoints available | NIST CMVP cert references in AWS docs |
| GDPR | Processor; SCCs Module 2; AWS European Sovereign Cloud roadmap | AWS DPA |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| AWS Customer Agreement | 2026-04-23 (click-through) | Standard commercial terms |
| AWS Service Terms | 2026-04-23 (incorporated by reference) | KMS-specific terms included |
| DPA | 2026-04-23 (unilateral acceptance) | SCCs Module 2 (Module 3 for customer-to-AWS chain) |
| AWS BAA | **Available, not signed** | No PHI in current scope |
| Enterprise Support | Not contracted (Business Support sufficient for staging) | Upgrade-on-demand path documented |

## 6. Control mapping

| CoreLink CTRL | Dependency on AWS KMS |
|---|---|
| CTRL-CRYPTO-002 | DEK generation + wrapping under customer CMK |
| CTRL-CRYPTO-003 | Envelope encryption full path |
| CTRL-ISO-001..005 | Per-tenant key isolation enforced by per-tenant CMK ARN |
| CTRL-PRIV-016 | Erasure attestation depends on CMK delete / disable (key-deletion satisfies cryptographic erasure) |
| CTRL-BYOK-KILL-SWITCH | Customer can `kms:DisableKey` to instantly revoke CoreLink's data-access capability |

## 7. Risk assessment narrative

**Inherent risk (20 = 5 × 4):** Impact 5 (KMS compromise = DEK-wrapping integrity loss across all AWS-BYOK tenants; mitigated only by short DEK rotation cadence + customer-side CloudTrail visibility); Likelihood 4 (AWS has had material public incidents — KMS-specific outages in 2021 us-east-1 — though no key-material compromise publicly disclosed).

**Residual risk (2.0):** CEF **0.10** — all four conditions met: (a) SOC 2 Type II current, (b) compensating controls documented (envelope encryption, FIPS endpoints, customer-held CMK, cross-vendor BYOK matrix), (c) tested failover (customer can switch to GCP/Azure/Vault — exercised weekly via `byok_matrix_weekly.yml`), (d) data shared is **only DEKs wrapped under customer-held CMKs** — i.e., CoreLink never sends plaintext customer data to AWS.

**Top risks monitored:**

1. AWS KMS regional outage → CoreLink workers cannot wrap DEKs in that region. Mitigation: customer-side CMK multi-region replicas; failover to alternate region per `compliance/byok-fips-matrix.md`.
2. IAM mis-configuration grants CoreLink role broader scope than `Encrypt`/`Decrypt` on the named ARN. Mitigation: customer-side IAM-policy linter shipped in `tooling/byok-iam-validator/`; flagged at customer-onboarding.
3. AWS regulatory event in customer jurisdiction (e.g., LGPD residency violation if customer mis-selects region). Mitigation: per-region attestation tracked under GAP-22.
4. CoreLink staging IAM access key leak → only affects staging key material; mirrored in `aws-sm-mirror` for break-glass rotation.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| AWS Premium Support | AWS Support Console — Severity "Critical / production system down" | 15-min response (Business Support tier) |
| AWS Trust & Safety | aws-security@amazon.com | 24h |
| Privacy / DPA | aws-EU-privacy@amazon.com | 5 business days |
| Breach notification | aws-security@amazon.com + customer-side AWS account holder + security@hugr.dev | Per AWS Customer Agreement Section 6.2 + DPA |

## 9. Termination / exit plan

AWS-KMS-specific exit (not full AWS exit, since CoreLink only uses KMS):

1. **Per-customer migration:** customer migrates CMK → GCP/Azure/Vault (CoreLink BYOK matrix supports this with zero data movement; only the envelope DEK is re-wrapped under the new CMK during a tenant-scope re-key operation).
2. **CoreLink staging account exit:** terminate staging IAM users; rotate any code-signing keys mirrored in `aws-sm-mirror`; delete staging KMS test keys after grace period.
3. **Total AWS exit (if AWS deplatforms us):** for AWS-BYOK customers, force-migrate to GCP/Azure/Vault within 30-day customer-notification window. RTO depends on customer cooperation — worst case 60 days per customer.

**RTO:** 60 days per BYOK-AWS customer worst case. **RPO:** zero data loss (BYOK rewrap is data-preserving).

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 2.0 | Initial DD; closes GAP-14. CEF 0.10 — strongest residual in register because envelope encryption + customer-held CMK + tested vendor-agnostic failover. |

Next quarterly review: **2026-08-15**.
