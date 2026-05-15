---
id: "DD-HASHICORP-VAULT-2026-05-15"
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
tags: ["soc2", "cc9.2", "vendor-dd", "important", "hashicorp", "vault", "byok", "gap-14"]
---

# Vendor DD — HashiCorp, Inc. (Vault — customer-managed)

> **doc_status:** ACTIVE · Register row: 8 · Category: **Important** · Residual risk: **3.0** (Inherent 12 × CEF 0.25).
>
> HashiCorp Vault is one of four BYOK key backends (alongside AWS KMS, GCP KMS, Azure Key Vault) — but unlike the cloud-KMS options, **Vault is customer-self-hosted or HCP-Vault-Dedicated**: CoreLink only exchanges short-lived AppRole `role_id`/`secret_id` credentials and invokes the Transit secrets engine. CoreLink **never holds plaintext keys** and never operates the Vault control plane.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | HashiCorp, Inc. (Delaware) — acquired by IBM Corporation 2024-04-24, closed 2025-02-27; operates as IBM subsidiary |
| HQ | 101 Second Street, Suite 700, San Francisco, CA 94105, USA |
| Public ticker | Subsidiary of IBM (NYSE: IBM) |
| Product modes used | (a) Customer-self-hosted Vault OSS / Enterprise (most common for CoreLink BYOK customers) + (b) HCP Vault Dedicated (HashiCorp-hosted) for customers who opt in |
| CoreLink relationship | Per-customer addendum to customer's existing Vault Enterprise license OR per-customer HCP Vault Dedicated tenant — CoreLink is a **client** of the customer's Vault, not a HashiCorp customer in the BYOK path |
| Contract effective | Per-customer; CoreLink's own HashiCorp commercial relationship is limited to a non-production HCP Vault Dedicated dev-tier instance for BYOK matrix CI (2026-04-23) |
| Account contacts | (recorded in `legal/vendor-contacts.md`) |

## 2. Service scope

- **Vault Transit secrets engine** — symmetric AES-256-GCM CMK wrapping (`encrypt`/`decrypt` endpoints), customer-controlled key rotation
- **AppRole auth method** — short-lived `role_id`/`secret_id` issued by customer to CoreLink Workers (per `secrets-checklist.md` row 33); response-wrapping recommended
- **Vault audit devices** — customer holds audit log of every CoreLink-initiated Transit call (the customer's evidence trail)
- **HCP Vault Dedicated** (CoreLink staging only) — HashiCorp-hosted Vault cluster used exclusively for BYOK matrix CI; no production tenant data ever flows through this instance

CoreLink does **not** consume: Vault Enterprise replication / DR, Vault Agent, Vault PKI engine, Vault SSH-secrets engine, Boundary, Consul, Nomad, Terraform Cloud — these are explicitly out of scope.

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| DEKs wrapped via Transit `encrypt` | CoreLink → customer Vault (envelope encryption) | Customer-managed (Vault seal-wraps the CMK under unseal keys / cloud-KMS auto-unseal) | TLS 1.3 (mTLS recommended; customer-issued certs) | **Customer** (CMK material lives inside customer's Vault barrier) |
| Plaintext customer data | (never) | — | — | — |
| AppRole `secret_id` (short-lived, response-wrapped) | Customer Vault → CoreLink Workers | `cf-wrangler` secret store; rotated per customer policy (typical 24h TTL) | TLS 1.3 | CoreLink (transient) |
| HCP Vault Dedicated staging telemetry | CoreLink staging → HashiCorp | HashiCorp-managed | TLS 1.3 | HashiCorp (staging only; no customer data) |

Register row 8 documents data-sharing as **`none` (customer-side; we exchange role-id/secret-id only)** — the only data CoreLink shares with HashiCorp **the company** is staging-only telemetry from the dev-tier HCP instance. Production customer data never touches HashiCorp-operated infrastructure unless the customer explicitly opts into HCP Vault Dedicated.

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 2 Type II (HashiCorp Cloud Platform) | Attested | HashiCorp Trust Portal (Drata vendor module, manual link) |
| ISO 27001 (HCP) | Certified | HashiCorp Trust Portal |
| FedRAMP Moderate (HCP) | In Process (as of 2026-05-15) | HashiCorp Trust Portal |
| GDPR | Processor (HCP only); customer-self-hosted Vault inherits customer attestations | HashiCorp DPA |
| LGPD | Processor (HCP only); customer-self-hosted Vault inherits customer attestations | HashiCorp DPA |
| Customer-self-hosted Vault attestations | **Inherited from the customer** — CoreLink does not separately re-attest a customer's Vault deployment | Per-customer addendum |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| HCP Subscription Agreement (CoreLink staging dev-tier) | 2026-04-23 (click-through) | Staging only; no production tenant data |
| HashiCorp DPA | 2026-04-23 (HCP staging only) | SCCs Module 2; not applicable when customer self-hosts |
| Customer addendum (BYOK-Vault customer) | Per-customer | Defines: scope of AppRole, audit-log delivery to customer SIEM, response-wrapping requirement, incident-notification SLA back to CoreLink |
| HashiCorp Enterprise License | N/A | CoreLink does not hold a HashiCorp Enterprise license; the customer's license covers the BYOK use case |

## 6. Control mapping

| CoreLink CTRL | Dependency on HashiCorp Vault |
|---|---|
| CTRL-CRYPTO-002 | DEK generation + wrapping via customer Transit engine (alternate backend) |
| CTRL-CRYPTO-003 | Envelope encryption full path |
| CTRL-ISO-001..005 | Per-tenant key isolation enforced by per-tenant Vault namespace + AppRole |
| CTRL-PRIV-016 | Erasure attestation depends on customer-side `transit/keys/<name>` deletion + version rotation |
| CTRL-BYOK-KILL-SWITCH | Customer can revoke the AppRole `role_id` or disable the Transit key to instantly cut CoreLink's access |

## 7. Risk assessment narrative

**Inherent risk (12 = 4 × 3):** Impact 4 (Vault compromise = DEK-wrapping integrity loss for HashiCorp-BYOK tenants; mitigated by the fact that **customer operates Vault**, so the blast radius is one customer); Likelihood 3 (HashiCorp-side: SaaS HCP has had no public material incidents in last 12 months; customer-self-hosted: blast radius depends on customer operational maturity — captured per-customer in addendum).

**Residual risk (3.0):** CEF **0.25** — three of four conditions met: (a) SOC 2 Type II current (HCP), (b) compensating controls documented (envelope encryption, response-wrapped AppRole, customer-held barrier keys, cross-vendor BYOK matrix), (c) tested failover (customer can switch to AWS/GCP/Azure — exercised weekly via `byok_matrix_weekly.yml`). CEF stops at 0.25 (not 0.10) because **condition (d) is weakened**: while CoreLink never sees plaintext, the **customer-self-hosted operational posture is heterogeneous** — a customer running an out-of-date Vault binary on un-patched infra is a real exposure even though attestation is inherited.

**Top risks monitored:**

1. Customer Vault outage → CoreLink workers cannot wrap DEKs for that tenant. Mitigation: per-customer documented degraded-mode (queue + retry); cross-vendor BYOK matrix permits failover to AWS/GCP/Azure as alternate backend if the customer pre-configures dual backends.
2. AppRole `secret_id` leak → CoreLink-side compromise window bounded by TTL (typically 24h). Mitigation: response-wrapping mandatory for new tenants; periodic rotation enforced.
3. HashiCorp Cloud Platform (HCP-Dedicated customer mode) regional outage → customer's HCP cluster down. Mitigation: customer-side cross-region replication if Vault Enterprise; advice in customer-onboarding playbook.
4. IBM corporate-control change (post-acquisition) → roadmap or licensing surprise. Mitigation: contract renewal review at staging-tier annual checkpoint; cross-vendor BYOK matrix means CoreLink is never single-vendor-locked.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| HashiCorp Support (HCP staging tier) | HCP Portal → Support → "Customer Issue" | Best-effort (dev-tier); upgrade to Plus / Premium for SLAs |
| HashiCorp Security | security@hashicorp.com | 5 business days |
| Privacy / DPA | privacy@hashicorp.com | 5 business days |
| Breach notification (HCP customers) | security@hashicorp.com + customer-side Vault operator + security@hugr.dev | Per HashiCorp DPA |
| **Customer-self-hosted Vault escalation** | **Customer's own ops on-call** — defined in per-customer addendum, mirrored into PagerDuty `byok-vault-customers` schedule | Per customer SLA |

## 9. Termination / exit plan

HashiCorp-Vault-specific exit:

1. **Per-customer migration:** customer migrates CMK → AWS/GCP/Azure (CoreLink BYOK matrix supports this with zero data movement; only the envelope DEK is re-wrapped under the new CMK during a tenant-scope re-key operation).
2. **CoreLink staging HCP exit:** terminate HCP Vault Dedicated dev-tier subscription; export staging Transit test keys to an alternate backend (or destroy outright); rotate any staging AppRole credentials mirrored in `cf-wrangler`.
3. **IBM/HashiCorp deplatforming or license change:** for Vault-BYOK customers (whether HCP or self-hosted), force-migrate to AWS/GCP/Azure within 30-day customer-notification window. Customer-self-hosted customers may keep running their own Vault binary indefinitely under existing license — CoreLink simply removes the HashiCorp branding from its BYOK matrix and treats it as a generic OSS Vault target.

**RTO:** 60 days per BYOK-Vault customer worst case. **RPO:** zero data loss (BYOK rewrap is data-preserving).

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 3.0 | Initial DD; closes GAP-14 Important-tier batch. CEF 0.25 — strong vendor controls, but customer-self-hosted heterogeneity weakens condition (d). |

Next bi-annual review: **2026-11-15**.
