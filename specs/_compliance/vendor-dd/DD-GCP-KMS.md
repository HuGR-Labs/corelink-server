---
id: "DD-GCP-KMS-2026-05-15"
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
tags: ["soc2", "cc9.2", "vendor-dd", "critical", "gcp", "kms", "byok", "fips", "gap-14"]
---

# Vendor DD — Google LLC (Google Cloud KMS)

> **doc_status:** ACTIVE · Register row: 5 · Category: **Critical** · Residual risk: **2.0** (Inherent 20 × CEF 0.10).
>
> Google Cloud KMS is one of four BYOK key backends (alongside AWS KMS, Azure Key Vault, HashiCorp Vault). Customer holds the CMK in **their own GCP project**; CoreLink only invokes `Encrypt`/`Decrypt`/`GenerateRandomBytes` operations under a workload-identity-federated service account.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | Google LLC — subsidiary of Alphabet Inc. (Delaware) |
| HQ | 1600 Amphitheatre Parkway, Mountain View, CA 94043, USA |
| EEA acquiring | Google Ireland Limited (Dublin) — for EEA data-processing |
| CoreLink relationship | (a) staging project `corelink-byok-staging-gcp` (CoreLink-owned, for BYOK matrix CI) + (b) per-customer projects (customer-owned; customer grants CoreLink workload-identity-federated SA narrow scope on a single keyring) |
| Contract effective | 2026-04-23 (Google Cloud Platform Terms of Service accepted) |
| Account contacts | (recorded in `legal/vendor-contacts.md`) |

## 2. Service scope

- **Cloud KMS** — symmetric AES-256 CMKs (Software tier) and HSM-backed CMKs (HSM tier, FIPS 140-2 Level 3) in customer-managed key state
- **External Key Manager (EKM)** — optional path where customer's CMK lives in an external HSM and Google KMS is the proxy (treated as a sub-mode of the BYOK matrix)
- **Workload Identity Federation** — short-lived OIDC-exchanged tokens replace long-lived service-account JSON keys (per `secrets-checklist.md` row 31)
- **Cloud Audit Logs (Data Access)** — customer holds audit log of every CoreLink-initiated KMS call (the customer's evidence trail, not CoreLink's; intentional asymmetry)
- **Secret Manager mirror** — defense-in-depth mirror of CoreLink staging credentials (`gcp-sm-mirror` storage tier in `secrets-checklist.md`)

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Data encryption keys (DEKs) wrapped under CMK | CoreLink → GCP Cloud KMS (envelope encryption) | Google-managed (HSM-backed for HSM tier CMKs) | TLS 1.3 | **Customer** (CMK material never leaves Google HSM in HSM tier) |
| Plaintext customer data | (never) | — | — | — |
| KMS API request metadata | CoreLink → Google | Google-managed | TLS 1.3 | Google / Customer (via Cloud Audit Logs) |
| Workload-identity-federation OIDC tokens (short-lived, < 1h) | CoreLink Workers → Google STS | Ephemeral; never persisted | TLS 1.3 | CoreLink (issuance only) |

The architectural invariant **INV-BYOK-CMK-NEVER-LEAVES-CUSTOMER (`compliance/byok-fips-matrix.md`)** means a Google KMS compromise affects DEK wrapping but does not directly leak customer plaintext.

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 1 / SOC 2 / SOC 3 Type II | Attested | Google Cloud Compliance Reports Manager (Drata-pulled annually) |
| ISO 27001 / 27017 / 27018 / 27701 / 9001 | Certified | Compliance Reports Manager |
| FedRAMP High | Authorized | GCP Assured Workloads + commercial regions |
| PCI-DSS Level 1 | Attested (Service Provider) | Compliance Reports Manager |
| HIPAA | BAA available; CoreLink BAA not signed (no PHI) | — |
| FIPS 140-2 Level 3 | HSM tier CMKs validated | NIST CMVP cert references in GCP docs |
| GDPR | Processor; SCCs Module 2; EU data residency offered | Google Cloud DPA |
| LGPD | Processor; Brazilian regions (São Paulo `southamerica-east1`) | DPA |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| Google Cloud Platform Terms of Service | 2026-04-23 (click-through) | Standard commercial terms |
| Google Cloud Service-Specific Terms | 2026-04-23 (incorporated by reference) | Cloud KMS terms included |
| Cloud Data Processing Addendum | 2026-04-23 (unilateral acceptance) | SCCs Module 2 (Module 3 for customer-to-Google chain) |
| Google Cloud BAA | **Available, not signed** | No PHI in current scope |
| Premium / Enhanced Support | Not contracted (Standard sufficient for staging) | Upgrade-on-demand path documented |

## 6. Control mapping

| CoreLink CTRL | Dependency on GCP KMS |
|---|---|
| CTRL-CRYPTO-002 | DEK generation + wrapping under customer CMK (alternate backend) |
| CTRL-CRYPTO-003 | Envelope encryption full path |
| CTRL-ISO-001..005 | Per-tenant key isolation enforced by per-tenant CMK resource name |
| CTRL-PRIV-016 | Erasure attestation depends on CMK `destroyVersion` (90-day soft-delete is GCP-side; documented in customer-onboarding) |
| CTRL-BYOK-KILL-SWITCH | Customer can `disable` the key version to instantly revoke CoreLink's data-access capability |

## 7. Risk assessment narrative

**Inherent risk (20 = 5 × 4):** Impact 5 (KMS compromise = DEK-wrapping integrity loss across all GCP-BYOK tenants; mitigated only by short DEK rotation cadence + customer-side Cloud Audit Logs visibility); Likelihood 4 (Google Cloud has had material public regional incidents — most notably the 2019 us-central1 networking event and 2024 europe-west1 storage degradation — though no key-material compromise publicly disclosed).

**Residual risk (2.0):** CEF **0.10** — all four conditions met: (a) SOC 2 Type II current, (b) compensating controls documented (envelope encryption, HSM-tier CMKs, customer-held CMK, workload identity federation eliminates long-lived SA JSON keys, cross-vendor BYOK matrix), (c) tested failover (customer can switch to AWS/Azure/Vault — exercised weekly via `byok_matrix_weekly.yml`), (d) data shared is **only DEKs wrapped under customer-held CMKs** — i.e., CoreLink never sends plaintext customer data to Google.

**Top risks monitored:**

1. GCP Cloud KMS regional outage → CoreLink workers cannot wrap DEKs in that region. Mitigation: customer-side CMK multi-region rings; failover to alternate region per `compliance/byok-fips-matrix.md`.
2. IAM mis-configuration grants CoreLink workload-identity SA broader scope than `cloudkms.cryptoKeyEncrypterDecrypter` on the named keyring. Mitigation: customer-side IAM-policy linter shipped in `tooling/byok-iam-validator/`; flagged at customer-onboarding.
3. GCP regulatory event in customer jurisdiction (e.g., data-export-control change). Mitigation: per-region attestation tracked under GAP-22 LGPD residency.
4. Service-account JSON key leakage (legacy auth mode) — fully mitigated by mandating workload identity federation for new tenants; legacy SA-JSON path is deprecated.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| Google Cloud Support (Standard) | Cloud Console → Support → Severity "P1: Production system down" | 1h business hours (Standard); upgrade to Premium for 15-min |
| Cloud Trust & Safety | cloud-security@google.com | 24h |
| Privacy / DPA | cloud-DPO@google.com | 5 business days |
| Breach notification | cloud-security@google.com + customer-side GCP project owner + security@hugr.dev | Per GCP DPA Section 7 |

## 9. Termination / exit plan

GCP-KMS-specific exit (not full GCP exit, since CoreLink only uses Cloud KMS):

1. **Per-customer migration:** customer migrates CMK → AWS/Azure/Vault (CoreLink BYOK matrix supports this with zero data movement; only the envelope DEK is re-wrapped under the new CMK during a tenant-scope re-key operation).
2. **CoreLink staging project exit:** terminate staging workload-identity pools; rotate any code-signing keys mirrored in `gcp-sm-mirror`; schedule key version destruction with 90-day soft-delete window.
3. **Total GCP exit (if Google deplatforms us):** for GCP-BYOK customers, force-migrate to AWS/Azure/Vault within 30-day customer-notification window. RTO depends on customer cooperation — worst case 60 days per customer.

**RTO:** 60 days per BYOK-GCP customer worst case. **RPO:** zero data loss (BYOK rewrap is data-preserving).

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 2.0 | Initial DD; closes GAP-14 Important-tier batch. CEF 0.10 — envelope encryption + customer-held HSM-tier CMK + tested vendor-agnostic failover. |

Next quarterly review: **2026-08-15**.
