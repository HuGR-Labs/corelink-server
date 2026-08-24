---
id: "DPA-RESIDENCY-AMENDMENT"
type: "legal_template"
doc_status: "PENDING_LEGAL_REVIEW"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
legal_review_status: "PENDING"
legal_review_firm: "TBD (Schellman Legal / Cooley / DLA Piper / Bird & Bird / Fenwick & West / Latham & Watkins)"
legal_review_budget: "$15,000–$30,000"
legal_review_lead_time: "6 weeks"
wi: "WI-S14-008"
tags:
  - "dpa"
  - "gdpr-art-28"
  - "gdpr-art-46"
  - "lgpd-art-33"
  - "schrems-ii"
  - "residency"
  - "4-regions"
  - "data-processing-agreement"
  - "s14"
reviewers:
  - "Compliance Officer"
  - "Privacy Officer"
  - "Legal Counsel (Legal externo)"
supersedes: null
superseded_by: null
---

# Data Processing Agreement — Residency Amendment
## CoreLink by HuGR Labs — Template v1.0.0

> **IMPORTANT LEGAL NOTICE**: This document is a **structural template** prepared to facilitate external Legal counsel review. It is **NOT a finalised legal instrument**. The redlined, legally verified version will be produced by a qualified GDPR-experienced external law firm engaged per `legal/legal-externo-engagement-contract.md`. **Legal review status: PENDING.**
>
> **References**: GDPR Art. 28, Art. 33, Art. 37, Art. 46 · LGPD Art. 33 §1, Art. 46, Art. 48 · Schrems II (C-311/18) · EDPB Recommendations 01/2020 · NIST SP 800-88 Rev.1 §2.4 · INV-DATA-RESIDENCY · INV-REGION-NO-CROSS-LEAK · INV-BYOK-CRYPTO-SOVEREIGNTY · INV-ERASURE-ATTESTATION-SIGNED

---

## Section 1 — Parties

**Data Controller ("Customer"):**
- Legal name: `[CUSTOMER_LEGAL_NAME]`
- Registration number: `[CUSTOMER_REG_NUMBER]`
- Registered address: `[CUSTOMER_ADDRESS]`
- DPO / Privacy contact: `[CUSTOMER_DPO_NAME]` · `[CUSTOMER_DPO_EMAIL]`

**Data Processor ("CoreLink" / "HuGR Labs"):**
- Legal name: HuGR Labs Ltda. (operating as CoreLink)
- Registered address: `[HUGR_REGISTERED_ADDRESS]`
- DPO contact: Gustavo Schneiter (interim, sole-founder dual-hat) · `dpo@corelink.io`
- Emergency contact: `security@corelink.io`

**Relationship:** The parties have entered into a Master Service Agreement ("MSA") or equivalent order form ("Service Agreement") under which CoreLink acts as Processor and Customer acts as Controller in respect of Personal Data processed through the CoreLink content-addressable cache service.

---

## Section 2 — Definitions

| Term | Definition |
|---|---|
| **Personal Data** | Any information relating to an identified or identifiable natural person as defined in GDPR Art. 4(1) and LGPD Art. 5(I). |
| **Processing** | Any operation performed on Personal Data as defined in GDPR Art. 4(2) and LGPD Art. 5(X). |
| **Data Subject** | The natural person to whom Personal Data relates. |
| **Sub-processor** | Any third party engaged by CoreLink to process Personal Data on behalf of Customer. |
| **CMK** | Customer-Managed Key — cryptographic key held exclusively by Customer, used to wrap the DEK. |
| **DEK** | Data Encryption Key — AES-256-GCM key generated per-blob, wrapped by the CMK. |
| **Region** | A named geographic grouping corresponding to CoreLink's Cloudflare infrastructure footprint. See Section 7 for enumeration. |
| **Tenant** | A Customer's isolated workspace within the CoreLink service, identified by `tenant_id`. |
| **Erasure Attestation** | An Ed25519-signed cryptographic receipt confirming irreversible data erasure per NIST SP 800-88 Rev.1 §2.4. |
| **BYOK** | Bring Your Own Key — cryptographic architecture where Customer supplies and exclusively controls the CMK. |
| **Breach** | A personal data breach as defined in GDPR Art. 4(12). |
| **EDPB Recommendations 01/2020** | European Data Protection Board Recommendations 01/2020 on measures that supplement transfer tools, as adopted 18 June 2021. |

---

## Section 3 — Subject-Matter and Duration

**Subject-matter:** CoreLink processes Personal Data solely to provide the content-addressable cache service described in the Service Agreement, including: blob storage, retrieval, deduplication, and associated access-control and audit services.

**Duration:** This DPA Amendment remains in force for the duration of the Service Agreement. Upon termination or expiry, Section 14 (Termination) applies.

---

## Section 4 — Nature and Purpose of Processing

CoreLink processes Personal Data on documented instruction from Customer solely for the following purposes:
1. Storage and retrieval of cache blobs uploaded by Customer or Customer's authorised users.
2. Access control enforcement (tenant isolation, RBAC, BYOK key operations).
3. Audit log generation for compliance, integrity, and forensic purposes (7-year retention per CTRL-AUDIT-005).
4. Breach detection, incident response, and security operations.

CoreLink shall not process Personal Data for its own purposes, use it for training machine-learning models, or disclose it to third parties except as required by applicable law or as necessary to deliver the service via authorised Sub-processors (Section 8).

---

## Section 5 — Categories of Data Subjects

- Customer's end-users (individuals whose data is contained in cache blobs uploaded by Customer).
- Customer's tenant organisation members (individuals who access the CoreLink service using Customer's credentials).

---

## Section 6 — Categories of Personal Data

Categories of Personal Data that may be processed are determined by Customer. CoreLink's data model (per `privacy_model.md` CTRL-PRIV-001 through CTRL-PRIV-030) supports but does not mandate the following categories:

| Category | Examples | CTRL Reference |
|---|---|---|
| Identifiers | User IDs, email addresses (hashed in audit chain) | CTRL-PRIV-001 |
| Audit metadata | Timestamps, operation type, tenant\_id, region | CTRL-PRIV-002 |
| Blob content | Customer-uploaded cache blobs (encrypted at rest; CoreLink cannot access plaintext) | CTRL-PRIV-010 |
| Access logs | IP addresses (pseudonymised), request paths, response codes | CTRL-PRIV-020 |

CoreLink does not knowingly process special-category data (GDPR Art. 9) or data concerning children. Customer is responsible for ensuring such data is not uploaded without appropriate safeguards.

---

## Section 7 — Residency Commitment per Region

CoreLink commits to storing and processing Customer's Tenant data within the following enumerated regions. The Tenant's `primary_region` is set at provisioning and may only be changed via Customer-initiated migration with 30-day advance notice.

### 7.1 Enumerated Regions

| Region Code | Geographic Area | Cloudflare Infrastructure | Data Localization Commitment |
|---|---|---|---|
| **WNAM** | Western North America | Cloudflare us-west infrastructure | Tenant data stored and processed in us-west facilities. |
| **ENAM** | Eastern North America | Cloudflare us-east infrastructure | Tenant data stored and processed in us-east facilities. |
| **WEUR** | Western Europe | Cloudflare eu-west infrastructure | Tenant data stored and processed in eu-west facilities. `jurisdictional_restriction = "eu"` enforced (WI-S14-001). |
| **SAM** | South America | Cloudflare sa-east infrastructure | Tenant data stored and processed in sa-east facilities. |

### 7.2 Failover Restrictions

CoreLink may replicate data to a secondary region solely for high-availability purposes, subject to the following hard restrictions (enforced by INV-DATA-RESIDENCY and INV-REGION-NO-CROSS-LEAK):

| Primary Region | Permitted Failover Destinations |
|---|---|
| WNAM | ENAM (US sibling pair only) |
| ENAM | WNAM (US sibling pair only) |
| WEUR | WEUR read-replica only; no cross-jurisdiction transfer |
| SAM | SAM read-replica only |

**WEUR data NEVER replicates outside the EU jurisdiction.** This restriction is enforced at the infrastructure level (Cloudflare DO `jurisdictional_restriction`) and validated by `PAT-REGION-FAILOVER-001` (WI-S14-003).

### 7.3 International Transfer Mechanism

For Customer tenants in WEUR: data transfers, if any, are governed by:
- EU Standard Contractual Clauses (SCCs) per GDPR Art. 46(2)(c).
- Schrems II Transfer Impact Assessment (`legal/tia-template.md`).
- EDPB Recommendations 01/2020 supplementary measures (Section 12 and Appendix A).

For Customer tenants in SAM (Brazil): transfers are governed by:
- LGPD Art. 33 §1 and its permitted mechanisms.
- Adequacy decision or appropriate safeguards per LGPD Art. 33.

---

## Section 8 — Sub-processors

CoreLink engages the following Sub-processors. Customer authorises their engagement. CoreLink shall: (a) impose equivalent data protection obligations on each Sub-processor; (b) notify Customer at least 30 days before engaging a new Sub-processor or making material changes; and (c) remain fully liable for Sub-processor acts and omissions.

### 8.1 Authorised Sub-processors

| Sub-processor | Role | Data Categories | DPA Reference | Region Scope |
|---|---|---|---|---|
| **Cloudflare, Inc.** | Infrastructure: Workers, R2, D1, KV, Durable Objects, Custom Domains | Blob content (encrypted), audit metadata, access logs | [cf-dpa.cloudflare.com](https://www.cloudflare.com/cloudflare-customer-dpa/) | Tenant-pinned (Section 7) |
| **Customer KMS Provider** | BYOK CMK storage and key operations (Customer-controlled) | CMK (customer-held; CoreLink cannot access) | Customer's agreement with their chosen KMS provider | N/A (customer-controlled) |

### 8.2 Customer KMS Provider Options

Customer may choose any of the following FIPS 140-2 / FIPS 140-3 validated KMS providers for BYOK:
- AWS Key Management Service (KMS)
- Google Cloud Key Management Service
- Azure Key Vault (Premium tier, HSM-backed)
- HashiCorp Vault (Enterprise, transit secrets engine)

### 8.3 Sub-processor Audit Rights

Customer has the right to request information about Cloudflare's compliance posture, including access to Cloudflare's SOC 2 Type II report (available at [trust.cloudflare.com](https://www.cloudflare.com/trust-hub/compliance-resources/)). CoreLink shall facilitate such requests within 30 days.

---

## Section 9 — Security Measures

CoreLink implements and maintains the following technical and organisational security measures:

### 9.1 Encryption at Rest

- **Envelope Encryption**: Each blob encrypted with a unique DEK (AES-256-GCM). DEK wrapped by Customer CMK via BYOK (WI-S14-004, WI-S14-005). CoreLink cannot decrypt blob content without Customer-provided CMK.
- **BYOK FIPS Verification**: CMK operations performed via FIPS 140-2 or FIPS 140-3 validated KMS endpoints (per provider). FIPS compliance documented in `docs/compliance/byok-fips-evidence.md`.
- **Per-region Key Isolation**: Audit chain signing keys and attestation keys are per-region; no cross-region key sharing.

### 9.2 Encryption in Transit

- TLS 1.2 floor, 1.3 negotiated, for all client-to-CoreLink and CoreLink-to-Cloudflare communications (ADR-0072).
- Mutual TLS (mTLS) for HashiCorp Vault key operations.
- Certificate management via Cloudflare-managed certificates.

### 9.3 Access Controls

- Least-privilege IAM with role-based access control (RBAC) enforced at tenant level.
- Admin role requires dual-approval for sensitive operations (WI-S13-002).
- Multi-factor authentication (MFA) mandatory for all CoreLink personnel with production access.
- Personnel access log retained 7 years (CTRL-AUDIT-005).

### 9.4 Audit Chain Integrity

- Append-only audit chain with tamper-evident Ed25519 signatures (S-09).
- Daily integrity verification (continuous auditor job).
- Audit logs retained 7 years (CTRL-AUDIT-005).

### 9.5 Erasure Attestation

- Cryptographic erasure via BYOK CMK revocation (NIST SP 800-88 Rev.1 §2.4 crypto-erase mode): Customer revokes CMK; all DEKs become permanently inaccessible; blobs irrecoverably inaccessible ≤ 5 minutes globally (INV-BYOK-CRYPTO-SOVEREIGNTY).
- Ed25519-signed erasure attestation issued post-erasure (INV-ERASURE-ATTESTATION-SIGNED, WI-S14-007).
- Attestation document retained by Customer as forensic evidence.

---

## Section 10 — Data Subject Rights

CoreLink shall assist Customer in fulfilling Data Subject rights requests within the following SLAs, as required by GDPR Chapter III and LGPD Chapter III:

| Right | Mechanism | SLA |
|---|---|---|
| **Right of Access** | Customer exports tenant data via CoreLink admin API (S-11) | 30 days (GDPR Art. 12) |
| **Right to Portability** | Customer exports blobs in standard format via admin API (S-11) | 30 days |
| **Right to Correction** | Customer updates metadata via standard write operations | Immediate |
| **Right to Erasure** | Customer revokes CMK (BYOK kill switch) → crypto-erase ≤ 5 min globally (INV-BYOK-CRYPTO-SOVEREIGNTY) + Ed25519 attestation (INV-ERASURE-ATTESTATION-SIGNED) | ≤ 5 min (technical); 7-day cooling-off period (S-11) |
| **Right to Restriction** | Customer disables tenant via admin API | Immediate |

**Erasure Note**: The crypto-erase mechanism satisfies NIST SP 800-88 Rev.1 §2.4 and constitutes effective erasure — all DEKs wrapped by the revoked CMK are permanently inaccessible. The Ed25519-signed attestation provides documentary evidence of erasure for regulatory purposes.

**Cooling-off period**: 7 calendar days between erasure request and irreversible execution (S-11), during which Customer may cancel the request.

---

## Section 11 — Breach Notification

### 11.1 CoreLink Obligations

Upon becoming aware of a Breach affecting Customer Personal Data, CoreLink shall:

1. Notify Customer DPO (Section 1) **within 72 hours** of becoming aware of the Breach. (GDPR Art. 33 / LGPD Art. 48.)
2. Notification shall include (to the extent then known):
   - Nature of the Breach (categories and approximate number of Data Subjects affected).
   - Likely consequences of the Breach.
   - Measures taken or proposed to address the Breach.
   - Name and contact details of the CoreLink DPO.
3. Initial notification may be made in stages; CoreLink shall provide complete information as soon as reasonably practicable.
4. CoreLink shall cooperate with Customer to facilitate Customer's own notifications to supervisory authorities and Data Subjects.

### 11.2 Contact Details

- **CoreLink DPO**: Gustavo Schneiter · `dpo@corelink.io`
- **Emergency security hotline**: `security@corelink.io` (monitored 24/7)
- **Incident runbook**: `RB-breach-notification` (internal reference)

### 11.3 Breach Response SLA

| Step | SLA |
|---|---|
| Initial Customer notification | ≤ 72 hours of CoreLink awareness |
| Incident containment | Per `RB-breach-notification` severity levels |
| Root-cause analysis | ≤ 14 days post-containment |
| Post-mortem report to Customer | ≤ 30 days post-containment |

---

## Section 12 — International Transfers

### 12.1 Transfer Mechanism

Where Personal Data is transferred from the European Economic Area (EEA) to a third country (including the United States, where Cloudflare is headquartered), such transfers are made on the basis of:
- EU Standard Contractual Clauses (Module 1: Controller to Controller; Module 2: Controller to Processor) per GDPR Art. 46(2)(c).
- Cloudflare's SCCs with CoreLink (incorporated by reference in Cloudflare DPA).

### 12.2 Schrems II Transfer Impact Assessment

CoreLink has conducted a Transfer Impact Assessment (TIA) per EDPB Recommendations 01/2020. The TIA:
- Assesses US surveillance law (FISA 702, EO 12333, CLOUD Act) risk.
- Documents supplementary measures (technical, organisational, contractual) rendering the transfer compliant.
- Concludes that BYOK customer-controlled CMK (Customer-held key; CoreLink and Cloudflare cannot access plaintext) renders the transfer compliant notwithstanding US surveillance risk.

Full TIA available at `legal/tia-template.md`.

### 12.3 LGPD Art. 33 §1 (Brazil — SAM Region)

For Customer tenants in SAM region (sa-east / Brazil):
- Transfers governed by LGPD Art. 33 §1 and applicable ANPD decisions.
- CoreLink commits to equivalent supplementary measures as documented in `legal/tia-template.md`.
- SAM data stored in Cloudflare sa-east infrastructure; no transfer outside Brazil without Customer consent.

---

## Section 13 — Audit Rights

### 13.1 Customer Audit Rights

Customer may, on reasonable notice (minimum 30 days) and no more than once per calendar year, request an audit of CoreLink's data processing activities, limited to matters relevant to this DPA. CoreLink may satisfy this right by:
- Providing its current SOC 2 Type II report (under NDA).
- Providing written responses to a standard security questionnaire (CAIQ/SIG).
- Facilitating a third-party audit (at Customer's cost, subject to scheduling constraints).

### 13.2 Regulatory Audit Cooperation

CoreLink shall cooperate with supervisory authorities (including ANPD, European DPAs) conducting audits or investigations related to Customer Personal Data processing.

---

## Section 14 — Termination and Data Deletion

### 14.1 Data Deletion

Upon termination or expiry of the Service Agreement for any reason:
- CoreLink shall, within **30 days**, delete or return (at Customer's option) all Customer Personal Data.
- Deletion shall use the BYOK crypto-erase mechanism (Section 9.5 / Section 10).
- CoreLink shall issue an **Ed25519-signed erasure attestation** (INV-ERASURE-ATTESTATION-SIGNED) within 30 days of termination as documentary evidence of deletion.

### 14.2 Audit Log Retention Post-Termination

Anonymised or pseudonymised audit logs (from which Personal Data has been removed) may be retained for up to 7 years from the date of the event, solely for CoreLink's internal audit, legal compliance, and integrity-verification purposes.

### 14.3 Return of Data

At Customer's written request (within 30 days of termination notice), CoreLink shall export Customer blobs in a standard format for Customer retrieval, prior to deletion.

---

## Section 15 — Governing Law and Jurisdiction

| Customer Tenant Region | Governing Law | Jurisdiction |
|---|---|---|
| WEUR (EU/EEA customers) | Laws of Ireland (EU Member State) | Courts of Ireland; GDPR supervisory authority: Data Protection Commission (Ireland) |
| WNAM / ENAM (US customers) | Laws of the State of Delaware, USA | Courts of Delaware, USA |
| SAM (Brazil customers) | Laws of the Federative Republic of Brazil (LGPD) | Courts of São Paulo, Brazil; ANPD (regulatory) |
| Default (unspecified) | `[TO BE DETERMINED BY LEGAL EXTERNO REVIEW]` | `[TO BE DETERMINED]` |

> Note: Governing law and jurisdiction clauses require Legal externo review and finalisation. The above is a structural placeholder.

---

## Appendix A — Technical Measures Evidence Pack

The following CoreLink implementation artefacts evidence the technical security measures committed in Section 9:

| Measure | Evidence Reference | Sprint WI |
|---|---|---|
| BYOK envelope encryption (AES-256-GCM + DEK + CMK) | `docs/compliance/byok-fips-evidence.md` | WI-S14-004, WI-S14-005 |
| BYOK kill switch ≤ 5 min global revocation | `specs/04_sprints/S14/work_items/WI-S14-006-*.md` | WI-S14-006 |
| Erasure attestation Ed25519-signed | `specs/04_sprints/S14/work_items/WI-S14-007-*.md` | WI-S14-007 |
| Region pinning + INV-REGION-NO-CROSS-LEAK | `specs/04_sprints/S14/work_items/WI-S14-002-*.md` | WI-S14-002 |
| Failover restriction PAT-REGION-FAILOVER-001 | `specs/04_sprints/S14/work_items/WI-S14-003-*.md` | WI-S14-003 |
| Audit chain integrity (Ed25519 + 7y retention) | S-09 audit chain | S-09 |
| FIPS-validated BYOK (4 providers) | `docs/compliance/byok-fips-evidence.md` | WI-S14-005 |
| TLS 1.2 floor / 1.3 negotiated + mTLS | Cloudflare certificate management | S-06 |

---

## Appendix B — Organisational Measures

| Measure | Description |
|---|---|
| DPO designation | Gustavo Schneiter (interim dual-hat); external DPO to be contracted per ADR-0034 Option C |
| Personnel training | Annual data protection training; GDPR + LGPD awareness |
| Access control policy | Least-privilege; MFA mandatory; dual-approval for admin operations |
| Incident response | `RB-breach-notification` runbook; 72h notification SLA; quarterly breach-response drills |
| Vendor management | Sub-processor disclosure (Section 8); 30-day advance notice of changes |
| Quarterly Legal review | EDPB monitoring; Schrems II landscape; DPA + TIA updates if needed |
| Sub-processor audit | Annual Cloudflare DPA review; SOC 2 Type II verification |
| 4 regions enumerated | WNAM / ENAM / WEUR / SAM — explicit residency commitments (Section 7) |

---

## Appendix C — Contractual Measures (Sub-processor Agreements)

| Sub-processor | Agreement | Status |
|---|---|---|
| Cloudflare, Inc. | [Cloudflare Customer DPA](https://www.cloudflare.com/cloudflare-customer-dpa/) (incorporating SCCs) | Signed 2026-04-23 (see `legal/sub-processors.md`) |
| Customer KMS Provider | Customer's own agreement with chosen KMS provider (AWS / GCP / Azure / Vault) | Customer responsibility |

**Customer right to audit Sub-processors**: Customer may request CoreLink to exercise its audit rights under the Cloudflare DPA on Customer's behalf, subject to Cloudflare's audit procedures and reasonable scheduling constraints.

---

*Document status: PENDING LEGAL REVIEW — not a finalised legal instrument. Version 1.0.0 · 2026-05-14 · WI-S14-008.*
