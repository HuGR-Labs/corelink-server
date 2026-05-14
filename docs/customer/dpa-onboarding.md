---
id: "DPA-ONBOARDING"
type: "customer_doc"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
audience: "enterprise_customer"
distribution: "post-NDA"
wi: "WI-S14-008"
tags:
  - "dpa"
  - "onboarding"
  - "enterprise"
  - "gdpr"
  - "schrems-ii"
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

CoreLink stores and processes data exclusively in your chosen region. Available regions:

| Region Code | Geography | Data Location |
|---|---|---|
| **WNAM** | Western North America | Cloudflare us-west infrastructure |
| **ENAM** | Eastern North America | Cloudflare us-east infrastructure |
| **WEUR** | Western Europe | Cloudflare eu-west infrastructure (EU jurisdictional restriction enforced) |
| **SAM** | South America | Cloudflare sa-east infrastructure (Brazil data stays in Brazil) |

**WEUR note**: For EU/EEA customers, your data is stored exclusively in EU infrastructure with Cloudflare's `jurisdictional_restriction = "eu"` enforcement. Data **never** replicates outside the EU.

**SAM note**: For Brazilian customers, your data is stored in sa-east and governed by LGPD Art. 33 §1.

---

## Step 2 — Review the DPA Package

Your DPA package includes:

1. **DPA Amendment Template** (`legal/dpa-residency-amendment.md`) — reviewed by an external GDPR-experienced law firm. Covers:
   - 15 sections: parties, definitions, data categories, residency commitment, sub-processors, security measures, data subject rights, breach notification, international transfers, audit rights, termination.
   - 3 appendices: technical measures evidence pack, organisational measures, contractual measures.

2. **Schrems II TIA** (`legal/tia-template.md`) — EDPB Recommendations 01/2020 framework. Documents why CoreLink's BYOK architecture renders US sub-processor jurisdiction risk substantively mitigated.

3. **Technical Evidence Pack** — available under NDA:
   - BYOK FIPS compliance documentation (`docs/compliance/byok-fips-evidence.md`)
   - Erasure attestation mechanism (Ed25519-signed; WI-S14-007)
   - SOC 2 Type II report (available on request)
   - Cloudflare sub-processor DPA (`https://www.cloudflare.com/cloudflare-customer-dpa/`)

---

## Step 3 — Key DPA Commitments

### Data Residency
- Your data is stored exclusively in your chosen region (Section 7 of DPA).
- Failover is restricted to jurisdictionally compatible sibling regions only.
- CoreLink does not transfer your data outside your region without your consent.

### BYOK Encryption — You Hold the Keys
- Every blob stored by CoreLink is encrypted with AES-256-GCM using a Data Encryption Key (DEK).
- Your DEK is wrapped by your Customer-Managed Key (CMK), which you exclusively hold.
- CoreLink **cannot decrypt your data** without your CMK.
- You can revoke your CMK at any time — your data becomes irrecoverably inaccessible within **≤ 5 minutes globally**.

### Supported KMS Providers
- AWS Key Management Service
- Google Cloud Key Management Service
- Azure Key Vault (Premium / HSM)
- HashiCorp Vault (Enterprise)

All FIPS 140-2 or FIPS 140-3 validated.

### Erasure — Cryptographic Proof
When you request erasure or terminate service:
- CMK revocation triggers immediate DEK inaccessibility (crypto-erase per NIST SP 800-88 Rev.1 §2.4).
- CoreLink issues an **Ed25519-signed erasure attestation** — a cryptographic receipt proving data erasure, suitable for regulatory submissions.

### Breach Notification
- CoreLink will notify your DPO within **72 hours** of becoming aware of any breach affecting your Personal Data (GDPR Art. 33 / LGPD Art. 48).

### Sub-processors
CoreLink uses only two categories of sub-processors:
1. **Cloudflare, Inc.** — infrastructure (Workers, R2, D1, KV, Durable Objects). Cloudflare DPA: `https://www.cloudflare.com/cloudflare-customer-dpa/`
2. **Your chosen KMS provider** — key management (you control this relationship).

No other sub-processors access your Personal Data.

---

## Step 4 — DPA Signing Process

```
1. CoreLink Sales provides DPA package + TIA + evidence pack (under NDA)
2. Your Legal reviews DPA + TIA
3. Iteration cycle (comments → CoreLink + Legal externo review)
4. DPA signed by authorised representatives of both parties
5. Onboarding proceeds: region selected → BYOK configured → CoreLink operational
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
- Delete all tenant data (right of erasure — crypto-erase + attestation).
- Restrict processing (disable tenant).

### Termination
Upon termination:
- Your data is deleted within 30 days via crypto-erase.
- You receive an Ed25519-signed erasure attestation.
- Anonymised audit logs may be retained up to 7 years for CoreLink's internal compliance obligations.

---

## Frequently Asked Questions

**Q: What happens if Cloudflare receives a US government data request (FISA 702 / CLOUD Act)?**
A: Cloudflare holds only ciphertext (AES-256-GCM encrypted blobs with DEKs wrapped by your CMK). Without your CMK, which Cloudflare does not hold, any data produced in response to a government order is unreadable. Our TIA documents this effectiveness argument per EDPB Recommendations 01/2020 §83.

**Q: Is CoreLink compliant with GDPR Art. 28?**
A: Yes. Our DPA is drafted to satisfy GDPR Art. 28 requirements and has been reviewed by an external GDPR-experienced law firm.

**Q: What about LGPD compliance (for Brazil)?**
A: SAM region data is stored in Cloudflare sa-east (Brazil). Our DPA covers LGPD Art. 33 §1 (international transfers), Art. 46 (security measures), and Art. 48 (breach notification). ANPD audit cooperation is documented.

**Q: Can we request custom DPA clauses?**
A: Yes, for enterprise customers. Custom clauses require a review cycle with our external legal counsel. Timeline: +2-4 weeks; potential additional cost.

**Q: Is there a lighthouse customer reference?**
A: Yes. CoreLink has executed its DPA with a lighthouse enterprise customer (available as reference under NDA upon request).

---

*Version 1.0.0 · 2026-05-14 · WI-S14-008 · Distribution: post-NDA enterprise customers.*
