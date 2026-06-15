---
id: "DPA-ONBOARDING"
type: "customer_doc"
doc_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-06-15"
owner: "Gustavo Schneiter"
audience: "enterprise_customer"
distribution: "post-NDA"
wi: "WI-S14-008"
tags:
  - "dpa"
  - "onboarding"
  - "enterprise"
  - "gdpr"
  - "residency"
  - "customer-facing"
  - "s14"
supersedes: null
superseded_by: null
---

# CoreLink DPA Onboarding Guide
## Enterprise Customer — Data Processing Agreement

> **Distribution**: This document is shared with enterprise customers under NDA, as part of the DPA onboarding process.

---

## Overview

CoreLink is a content-addressable shared cache service by HuGR Labs. For enterprise customers, CoreLink provides:

- A **Data Processing Agreement (DPA)** covering data residency commitments, security measures, and GDPR/LGPD compliance obligations.
- A **Schrems II Transfer Impact Assessment (TIA)** documenting supplementary measures per EDPB Recommendations 01/2020.
- A **technical evidence pack** providing cryptographic evidence of data security and erasure.

This guide walks through the DPA onboarding process.

---

## Step 1 — Select Your Data Residency Region

CoreLink stores and processes data exclusively in your chosen region. Available regions at launch:

| Region Code | Geography | Data Location | Status |
|---|---|---|---|
| **WNAM** | Western North America | Cloudflare us-west infrastructure | **Available** |
| **ENAM** | Eastern North America | Cloudflare us-east infrastructure | **Available** |
| **WEUR** | Western Europe | Cloudflare eu-west infrastructure | Phase 2 — not yet available |
| **SAM** | South America | Cloudflare sa-east infrastructure | Phase 2 — not yet available |

> **Launch posture (US-only):** At launch, CoreLink provisions tenants exclusively in US regions (WNAM / ENAM). New signup requests selecting WEUR or SAM are rejected during this phase. EU/EEA and Brazil residency support (including the enforcement commitments described below for those regions) is on the roadmap for Phase 2 and will be communicated when available. DPA signers requiring EU or SAM residency should confirm availability with CoreLink before signing.

**WEUR (Phase 2)**: When enabled, EU/EEA customer data will be stored exclusively in EU infrastructure with Cloudflare's `jurisdictional_restriction = "eu"` enforcement. Data will not replicate outside the EU.

**SAM (Phase 2)**: When enabled, Brazilian customer data will be stored in sa-east and governed by LGPD Art. 33 §1.

---

## Step 2 — Review the DPA Package

Your DPA package includes:

1. **DPA Amendment Template** (`legal/dpa-residency-amendment.md`) — reviewed by an external GDPR-experienced law firm. Covers:
   - 15 sections: parties, definitions, data categories, residency commitment, sub-processors, security measures, data subject rights, breach notification, international transfers, audit rights, termination.
   - 3 appendices: technical measures evidence pack, organisational measures, contractual measures.

2. **Schrems II TIA** (`legal/tia-template.md`) — EDPB Recommendations 01/2020 framework. Documents CoreLink's supplementary measures for US sub-processor jurisdiction risk. **Note:** The TIA's encryption effectiveness argument assumes customer-managed key (BYOK) encryption of stored blobs. BYOK is not yet enabled for the launched data plane (see Section 3 below). Until BYOK is enabled, the TIA's Schrems-II mitigations should be assessed against the current posture: tenant-isolated, content-addressed storage on Cloudflare R2 with Cloudflare-managed encryption at rest, protected by Cloudflare's SCCs and EU-US DPF commitments.

3. **Technical Evidence Pack** — available under NDA:
   - BYOK FIPS compliance documentation (`docs/compliance/byok-fips-evidence.md`) — available when BYOK is provisioned
   - Erasure attestation mechanism (Ed25519-signed; WI-S14-007)
   - SOC 2 Type II report (available on request)
   - Cloudflare sub-processor DPA (`https://www.cloudflare.com/cloudflare-customer-dpa/`)

---

## Step 3 — Key DPA Commitments

### Data Residency
- Your data is stored exclusively in your chosen region (Section 7 of DPA).
- Failover is restricted to jurisdictionally compatible sibling regions only.
- CoreLink does not transfer your data outside your region without your consent.

### Encryption and Storage Security

**Current launch posture:** CoreLink stores data as tenant-isolated, content-addressed objects in Cloudflare R2. All data is encrypted in transit (TLS) and at rest using Cloudflare-managed AES-256 encryption. Tenant isolation is enforced via HMAC-derived key prefixes — no cross-tenant access is possible at the storage layer, even for CoreLink operators. Cloudflare's encryption at rest is covered by Cloudflare's sub-processor DPA and SOC 2 Type II.

**BYOK (Bring Your Own Key) — planned, not yet available:** CoreLink's roadmap includes customer-managed key (BYOK) envelope encryption, in which every blob is wrapped by a Data Encryption Key (DEK) that is itself wrapped by the customer's Customer-Managed Key (CMK) held in an external KMS. Under this model, CoreLink cannot decrypt stored data without the customer CMK. BYOK is a planned Enterprise-tier feature and is **not yet wired into the CAS storage path in the launched data plane.** It will be communicated explicitly when available, along with the supported KMS providers and FIPS validation documentation.

**Kill-switch and CMK revocation:** When BYOK is active, revoking the CMK renders stored data irrecoverably inaccessible within ≤ 5 minutes globally. This control is **not available** until BYOK is provisioned for your tenant.

### Erasure — Cryptographic Proof
When you request erasure or terminate service:
- CoreLink issues an **Ed25519-signed erasure attestation** — a cryptographic receipt proving data erasure, suitable for regulatory submissions.

### Breach Notification
- CoreLink will notify your DPO within **72 hours** of becoming aware of any breach affecting your Personal Data (GDPR Art. 33 / LGPD Art. 48).

### Sub-processors
CoreLink uses the following sub-processors at launch:
1. **Cloudflare, Inc.** — infrastructure (Workers, R2, D1, KV, Durable Objects). Cloudflare DPA: `https://www.cloudflare.com/cloudflare-customer-dpa/`

When BYOK is provisioned, your chosen KMS provider (AWS KMS, Google Cloud KMS, Azure Key Vault, or HashiCorp Vault) will also be engaged as a sub-processor for key management. CoreLink will provide 30-day advance notice prior to adding that sub-processor.

No other sub-processors access your Personal Data.

---

## Step 4 — DPA Signing Process

```
1. CoreLink Sales provides DPA package + TIA + evidence pack (under NDA)
2. Your Legal reviews DPA + TIA
3. Iteration cycle (comments → CoreLink + Legal externo review)
4. DPA signed by authorised representatives of both parties
5. Onboarding proceeds: region selected → CoreLink operational (BYOK configuration follows when that feature is provisioned for your tier)
```

**Typical timeline**: 2-4 weeks from package delivery to DPA signing (dependent on your Legal review cycle).

**CoreLink DPO contact**: `dpo@corelink.io`

---

## Step 5 — Ongoing Rights and Controls

### Annual Audit
You may request an annual audit of CoreLink's data processing activities. CoreLink will provide:
- SOC 2 Type II report (under NDA).
- Written responses to security questionnaire (CAIQ/SIG).

### Sub-processor Changes
CoreLink will notify you at least **30 days** before engaging a new sub-processor or making material changes to existing sub-processors. You have the right to object.

### Data Subject Rights
CoreLink's admin API enables you to:
- Export all tenant data (right of access / portability).
- Delete all tenant data (right of erasure — secure deletion + Ed25519-signed attestation; CMK revocation accelerated crypto-erase is available when BYOK is provisioned).
- Restrict processing (disable tenant).

### Termination
Upon termination:
- Your data is deleted within 30 days. Where BYOK is active, deletion is accelerated via CMK revocation (crypto-erase per NIST SP 800-88 Rev.1 §2.4); otherwise data is deleted by secure R2 object deletion.
- You receive an Ed25519-signed erasure attestation.
- Anonymised audit logs may be retained up to 7 years for CoreLink's internal compliance obligations.

---

## Frequently Asked Questions

**Q: What happens if Cloudflare receives a US government data request (FISA 702 / CLOUD Act)?**
A: This depends on whether BYOK is active for your tenant. **With BYOK (planned, not yet available at launch):** Cloudflare holds only ciphertext encrypted with DEKs wrapped by your CMK; without your CMK — which Cloudflare does not hold — any data produced in response to a government order is unreadable. Our TIA documents this effectiveness argument per EDPB Recommendations 01/2020 §83. **Without BYOK (current launch posture):** Cloudflare holds data encrypted with Cloudflare-managed keys, subject to Cloudflare's own legal challenge process and SCCs/DPF commitments. Customers with heightened Schrems-II exposure should await BYOK availability or consult their legal counsel on the current posture before signing.

**Q: Is CoreLink compliant with GDPR Art. 28?**
A: Yes. Our DPA is drafted to satisfy GDPR Art. 28 requirements and has been reviewed by an external GDPR-experienced law firm.

**Q: What about LGPD compliance (for Brazil)?**
A: The SAM region (Cloudflare sa-east, Brazil) is on the roadmap and is **not yet available at launch.** Brazilian tenants cannot currently be provisioned; new signup attempts are rejected. When SAM is enabled, our DPA will cover LGPD Art. 33 §1 (international transfers), Art. 46 (security measures), and Art. 48 (breach notification), with ANPD audit cooperation documented. Brazilian customers should confirm SAM availability with CoreLink before signing a DPA.

**Q: Can we request custom DPA clauses?**
A: Yes, for enterprise customers. Custom clauses require a review cycle with our external legal counsel. Timeline: +2-4 weeks; potential additional cost.

**Q: Is there a lighthouse customer reference?**
A: Yes. CoreLink has executed its DPA with a lighthouse enterprise customer (available as reference under NDA upon request).

---

*Version 1.1.0 · 2026-06-15 · WI-S14-008 · Distribution: post-NDA enterprise customers.*
*Change note: Corrected residency table (WEUR/SAM marked Phase 2 — not yet available); corrected BYOK/Schrems-II section to reflect launched data plane posture (Cloudflare-managed encryption at rest; customer-managed BYOK is planned, not yet wired into CAS storage); updated FISA/LGPD FAQs accordingly.*
