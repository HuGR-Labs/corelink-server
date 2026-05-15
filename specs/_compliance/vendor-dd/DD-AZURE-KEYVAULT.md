---
id: "DD-AZURE-KEYVAULT-2026-05-15"
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
tags: ["soc2", "cc9.2", "vendor-dd", "critical", "azure", "key-vault", "byok", "fips", "gap-14"]
---

# Vendor DD — Microsoft Corporation (Azure Key Vault)

> **doc_status:** ACTIVE · Register row: 6 · Category: **Critical** · Residual risk: **2.0** (Inherent 20 × CEF 0.10).
>
> Azure Key Vault is one of four BYOK key backends (alongside AWS KMS, GCP KMS, HashiCorp Vault). Customer holds the CMK in **their own Azure tenant**; CoreLink only invokes `wrapKey`/`unwrapKey` operations via Managed HSM or Standard/Premium Key Vault under a federated workload identity.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | Microsoft Corporation (Washington, USA) |
| HQ | One Microsoft Way, Redmond, WA 98052, USA |
| EEA acquiring | Microsoft Ireland Operations Limited (Dublin) — for EEA data-processing |
| CoreLink relationship | (a) staging tenant `corelink-byok-staging-azure` (CoreLink-owned, for BYOK matrix CI) + (b) per-customer tenants (customer-owned; customer grants CoreLink federated identity narrow scope on a single Key Vault or Managed HSM pool) |
| Contract effective | 2026-04-23 (Microsoft Customer Agreement + Online Service Terms accepted) |
| Account contacts | (recorded in `legal/vendor-contacts.md`) |

## 2. Service scope

- **Azure Key Vault Standard** — software-protected symmetric AES-256 CMKs (FIPS 140-2 Level 2)
- **Azure Key Vault Premium / Managed HSM** — HSM-protected CMKs (FIPS 140-2 Level 3) — recommended tier for CoreLink BYOK customers
- **Workload Identity Federation** — short-lived OIDC-exchanged tokens replace long-lived service principal secrets (per `secrets-checklist.md` row 32)
- **Azure Monitor / Activity Log** — customer holds audit log of every CoreLink-initiated Key Vault call (the customer's evidence trail, not CoreLink's)
- **Azure Key Vault for Secrets** — defense-in-depth mirror of CoreLink staging credentials (`azure-kv-mirror` storage tier in `secrets-checklist.md`)

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Data encryption keys (DEKs) wrapped under CMK | CoreLink → Azure Key Vault (envelope encryption) | Microsoft-managed (HSM-backed for Premium / Managed HSM tier) | TLS 1.3 | **Customer** (CMK material never leaves Azure HSM in Premium / Managed HSM tier) |
| Plaintext customer data | (never) | — | — | — |
| Key Vault API request metadata | CoreLink → Microsoft | Microsoft-managed | TLS 1.3 | Microsoft / Customer (via Azure Activity Log) |
| Workload-identity federation tokens (short-lived, < 1h) | CoreLink Workers → Entra ID | Ephemeral; never persisted | TLS 1.3 | CoreLink (issuance only) |

The architectural invariant **INV-BYOK-CMK-NEVER-LEAVES-CUSTOMER (`compliance/byok-fips-matrix.md`)** means an Azure Key Vault compromise affects DEK wrapping but does not directly leak customer plaintext.

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 1 / SOC 2 / SOC 3 Type II | Attested | Microsoft Service Trust Portal (Drata-pulled annually) |
| ISO 27001 / 27017 / 27018 / 27701 / 9001 | Certified | Service Trust Portal |
| FedRAMP High | Authorized | Azure Government + commercial regions |
| PCI-DSS Level 1 | Attested (Service Provider) | Service Trust Portal |
| HIPAA / HITRUST | BAA available; CoreLink BAA not signed (no PHI) | — |
| FIPS 140-2 Level 2 (Standard) / Level 3 (Premium, Managed HSM) | Validated | NIST CMVP cert references in Azure docs |
| GDPR | Processor; SCCs Module 2; EU Data Boundary offered | Microsoft DPA / Online Services DPA |
| LGPD | Processor; Brazilian regions (Brazil South `brazilsouth`) | DPA |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| Microsoft Customer Agreement (MCA) | 2026-04-23 (click-through) | Standard commercial terms |
| Microsoft Online Services Terms / Product Terms | 2026-04-23 (incorporated by reference) | Key Vault service-specific terms included |
| Microsoft DPA (Online Services DPA) | 2026-04-23 (unilateral acceptance) | SCCs Module 2 (Module 3 for customer-to-Microsoft chain) |
| Microsoft BAA | **Available, not signed** | No PHI in current scope |
| Premier / Unified Support | Not contracted (Standard sufficient for staging) | Upgrade-on-demand path documented |

## 6. Control mapping

| CoreLink CTRL | Dependency on Azure Key Vault |
|---|---|
| CTRL-CRYPTO-002 | DEK generation + wrapping under customer CMK (alternate backend) |
| CTRL-CRYPTO-003 | Envelope encryption full path |
| CTRL-ISO-001..005 | Per-tenant key isolation enforced by per-tenant Key Vault resource ID |
| CTRL-PRIV-016 | Erasure attestation depends on `Disable` + soft-delete purge (90-day soft-delete is Azure-side; documented in customer-onboarding) |
| CTRL-BYOK-KILL-SWITCH | Customer can `Disable` the key version to instantly revoke CoreLink's data-access capability |

## 7. Risk assessment narrative

**Inherent risk (20 = 5 × 4):** Impact 5 (Key Vault compromise = DEK-wrapping integrity loss across all Azure-BYOK tenants; mitigated only by short DEK rotation cadence + customer-side Azure Monitor visibility); Likelihood 4 (Azure has had material public regional incidents — including the 2024 Azure Front Door global outage and recurrent Entra ID auth incidents in 2023 — though no Key Vault key-material compromise publicly disclosed).

**Residual risk (2.0):** CEF **0.10** — all four conditions met: (a) SOC 2 Type II current, (b) compensating controls documented (envelope encryption, Premium / Managed HSM tier CMKs, customer-held CMK, workload identity federation eliminates long-lived secrets, cross-vendor BYOK matrix), (c) tested failover (customer can switch to AWS/GCP/Vault — exercised weekly via `byok_matrix_weekly.yml`), (d) data shared is **only DEKs wrapped under customer-held CMKs** — i.e., CoreLink never sends plaintext customer data to Microsoft.

**Top risks monitored:**

1. Azure Key Vault regional outage → CoreLink workers cannot wrap DEKs in that region. Mitigation: customer-side multi-region replication (Managed HSM supports paired regions); failover to alternate region per `compliance/byok-fips-matrix.md`.
2. Entra ID auth incident propagates to Key Vault data-plane access. Mitigation: cross-vendor BYOK matrix permits temporary failover to AWS/GCP/Vault for affected tenants.
3. RBAC/Access Policy mis-configuration grants CoreLink federated identity broader scope than `wrapKey`/`unwrapKey` on the named Key Vault. Mitigation: customer-side IAM-policy linter shipped in `tooling/byok-iam-validator/`; flagged at customer-onboarding.
4. Azure regulatory event in customer jurisdiction (e.g., EU Data Boundary scope change). Mitigation: per-region attestation tracked under GAP-22 LGPD residency.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| Azure Support (Standard) | Azure Portal → New Support Request → Severity A (Production system down) | 1h business hours (Standard); upgrade to Professional Direct for 1h 24/7 |
| Microsoft Security Response Center | secure@microsoft.com | 24h |
| Privacy / DPA | dpoeurope@microsoft.com (EEA) / privacy@microsoft.com | 5 business days |
| Breach notification | secure@microsoft.com + customer-side Azure tenant owner + security@hugr.dev | Per Microsoft DPA Section "Notification of Security Incident" |

## 9. Termination / exit plan

Azure-Key-Vault-specific exit (not full Azure exit, since CoreLink only uses Key Vault):

1. **Per-customer migration:** customer migrates CMK → AWS/GCP/Vault (CoreLink BYOK matrix supports this with zero data movement; only the envelope DEK is re-wrapped under the new CMK during a tenant-scope re-key operation).
2. **CoreLink staging tenant exit:** terminate staging workload-identity federations; rotate any code-signing keys mirrored in `azure-kv-mirror`; trigger soft-delete + 90-day purge window on staging keys.
3. **Total Azure exit (if Microsoft deplatforms us):** for Azure-BYOK customers, force-migrate to AWS/GCP/Vault within 30-day customer-notification window. RTO depends on customer cooperation — worst case 60 days per customer.

**RTO:** 60 days per BYOK-Azure customer worst case. **RPO:** zero data loss (BYOK rewrap is data-preserving).

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 2.0 | Initial DD; closes GAP-14 Important-tier batch. CEF 0.10 — envelope encryption + customer-held HSM-tier CMK + tested vendor-agnostic failover. |

Next quarterly review: **2026-08-15**.
