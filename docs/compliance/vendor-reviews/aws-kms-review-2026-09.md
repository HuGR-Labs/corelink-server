# Vendor Legal-Review Record — Amazon Web Services, Inc.

> STATUS: TEMPLATE — formal Legal review pending; dated public-source assessment below is not a completed review.

| Field | Value |
|---|---|
| Vendor | Amazon Web Services, Inc. |
| Sub-processor id | `aws-kms` |
| Review date | `TBD (formal Legal review not completed)` |
| Public-source assessment date | 2026-09-22 (does not replace the formal review) |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)`; no formal reviewer or decision is recorded. |
| DPA reference | <https://d1.awsstatic.com/legal/aws-gdpr/aws-gdpr-dpa-online.pdf> |
| DPA status | Evidence insufficient — current CoreLink customer agreement, accepted DPA, and effective version are not in this packet. |
| SCC / transfer mechanism | Evidence insufficient — applicable account agreement and transfer mechanism are not in this packet. |
| Schrems II TIA | Evidence insufficient — no account-specific transfer assessment is in this packet. |
| Data categories processed | encrypted-blobs (envelope keys only; no plaintext content) |
| Data residency / region | Customer AWS account / selected AWS Region; actual account configuration not verified |
| Sub-processor flow-down | Evidence insufficient — no applicable subprocessor terms or dated change evidence were retrieved. |
| Certifications verified | Not verified for CoreLink — register lists SOC 2 Type II, ISO 27001/17/18, FedRAMP High, PCI-DSS L1, HIPAA, and FIPS endpoints; no CoreLink AWS Artifact package was retrieved. |
| Review outcome | `TBD (formal Legal outcome pending)`; public-source assessment: **Evidence insufficient — not approved**. |
| Human decision | Pending formal human review; no approval decision is recorded. |
| Signer | `TBD (named human signer pending)`; no signature is recorded. |
| Conditions / follow-ups | VP-Sec and Legal/Privacy must obtain current customer/account evidence, complete the refresh fields, decide, and record the named reviewer and signer. |
| Next review due | `2026-08-15` (formal due date; overdue 38 calendar days at assessment capture; not reset). |

## Public-source assessment (2026-09-22)

**Public statements observed.** AWS publishes a Data Processing Addendum. AWS states customer SOC 1/2 reports and ISO/PCI materials are available through AWS Artifact, and that SOC 2 reports are not public. AWS KMS documentation describes customer key ownership, HSM protection, TLS, and shared responsibility.

| Refresh item | Current public evidence and remaining gap |
|---|---|
| Scope | Public documentation describes AWS KMS. The CoreLink-owned staging account, customer-owned accounts, regions, key policies, connected services, and report scope were not reconciled. |
| Subprocessors | No dated AWS Artifact or account-specific subprocessor evidence was retrieved; applicable vendors and changes remain unverified. |
| Residency | AWS describes service and Region options. Actual CoreLink/customer account Regions, key locations, data flows, and transfer posture are unverified. |
| Security changes / current assurance | AWS states customer reports are retrievable through Artifact. No CoreLink-accessible report, auditor period, KMS service scope confirmation, or exceptions were retrieved. |
| DPA changes | No executed CoreLink agreement or prior DPA version was available for comparison. |
| Renewal date | CoreLink AWS agreement, renewal date, notice window, and BAA disposition remain unverified. |
| Human decision / signer | No human approval or signature is recorded. Evidence is insufficient; the outcome remains not approved and pending Legal review. |

### Official vendor sources consulted on 2026-09-22

- [AWS Data Processing Addendum](https://d1.awsstatic.com/legal/aws-gdpr/aws-gdpr-dpa-online.pdf)
- [AWS SOC FAQs (report availability and period)](https://aws.amazon.com/compliance/soc-faqs/)
- [AWS Artifact FAQ](https://aws.amazon.com/artifact/faq/)
- [AWS KMS data protection](https://docs.aws.amazon.com/kms/latest/developerguide/data-protection.html)

## Formal review cadence

The source register records the last formal review as **2026-05-15** and the formal next-review due date as **2026-08-15**. On the assessment capture date, 2026-09-22, the recorded due date is **38 calendar days overdue**. This public-source assessment does not satisfy the formal review, alter its due date, or start a new cadence. The register’s quarterly cadence, 90-day threshold, and explicit scheduled date are left as recorded; no replacement due date is inferred.

Public vendor material does not establish CoreLink’s executed terms, account settings, customer-restricted reports, applicable subprocessor snapshot, current data flow, completed transfer assessment, renewal terms, human disposition, or Drata workspace export. Any assertions in repository registers or commitments remain repository-recorded assertions until the underlying dated evidence is examined by the accountable reviewers.

*This file remains a template and is not a signed legal review. Referenced by `legal/sub-processors.md` and/or `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`; existence is enforced by `scripts/validate_sub_processors.py`.*
