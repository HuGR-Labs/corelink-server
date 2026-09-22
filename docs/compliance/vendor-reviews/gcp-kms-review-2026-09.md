# Vendor Legal-Review Record — Google LLC (Google Cloud)

> STATUS: TEMPLATE — formal Legal review pending; dated public-source assessment below is not a completed review.

| Field | Value |
|---|---|
| Vendor | Google LLC (Google Cloud) |
| Sub-processor id | `gcp-kms` |
| Review date | `TBD (formal Legal review not completed)` |
| Public-source assessment date | 2026-09-22 (does not replace the formal review) |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)`; no formal reviewer or decision is recorded. |
| DPA reference | <https://cloud.google.com/terms/data-processing-addendum> |
| DPA status | Evidence insufficient — current CoreLink customer agreement, accepted addendum, and effective version are not in this packet. |
| SCC / transfer mechanism | Evidence insufficient — applicable account agreement and transfer mechanism are not in this packet. |
| Schrems II TIA | Evidence insufficient — no account-specific transfer assessment is in this packet. |
| Data categories processed | encrypted-blobs (envelope keys only) |
| Data residency / region | Customer Google Cloud project / selected location; actual location not verified |
| Sub-processor flow-down | Evidence insufficient — no applicable subprocessor terms or dated change evidence were retrieved. |
| Certifications verified | Not verified for CoreLink — register lists SOC 2 Type II, ISO 27001/17/18, FedRAMP High, PCI-DSS L1 and FIPS HSM scope; no CoreLink report export was retrieved. |
| Review outcome | `TBD (formal Legal outcome pending)`; public-source assessment: **Evidence insufficient — not approved**. |
| Human decision | Pending formal human review; no approval decision is recorded. |
| Signer | `TBD (named human signer pending)`; no signature is recorded. |
| Conditions / follow-ups | VP-Sec and Legal/Privacy must obtain current customer/account evidence, complete the refresh fields, decide, and record the named reviewer and signer. |
| Next review due | `2026-08-15` (formal due date; overdue 38 calendar days at assessment capture; not reset). |

## Public-source assessment (2026-09-22)

**Public statements observed.** Google publishes the Cloud Data Processing Addendum. Its Compliance Reports Manager offers SOC reports, ISO certificates, and self-assessments; some resources require sign-in or are confidential. Current Cloud KMS documentation describes location-dependent key residency and says the global location has no geographic residency guarantee.

| Refresh item | Current public evidence and remaining gap |
|---|---|
| Scope | Public documentation describes Google Cloud and Cloud KMS. CoreLink projects, connected services, customer keys, enabled controls, and assurance scope have not been reconciled. |
| Subprocessors | No dated, account-applicable subprocessor list or change comparison since the prior review was captured. |
| Residency | Public Cloud KMS documentation describes location-based residency and limitations of the global location. CoreLink key locations and data flows remain unverified. |
| Security changes / current assurance | Compliance Reports Manager lists SOC/ISO artifacts, some gated by authentication/confidentiality. No current CoreLink report, period, service scope, or exceptions were retrieved. |
| DPA changes | No executed CoreLink addendum or prior version was available for comparison. |
| Renewal date | CoreLink Google Cloud agreement/order, renewal date, and notice window remain unverified. |
| Human decision / signer | No human approval or signature is recorded. Evidence is insufficient; the outcome remains not approved and pending Legal review. |

### Official vendor sources consulted on 2026-09-22

- [Google Cloud Data Processing Addendum](https://cloud.google.com/terms/data-processing-addendum)
- [Google Cloud Compliance Reports Manager](https://cloud.google.com/security/compliance/compliance-reports-manager)
- [Cloud KMS encryption and key management, updated September 2026](https://docs.cloud.google.com/docs/security/key-management-deep-dive)

## Formal review cadence

The source register records the last formal review as **2026-05-15** and the formal next-review due date as **2026-08-15**. On the assessment capture date, 2026-09-22, the recorded due date is **38 calendar days overdue**. This public-source assessment does not satisfy the formal review, alter its due date, or start a new cadence. The register’s quarterly cadence, 90-day threshold, and explicit scheduled date are left as recorded; no replacement due date is inferred.

Public vendor material does not establish CoreLink’s executed terms, account settings, customer-restricted reports, applicable subprocessor snapshot, current data flow, completed transfer assessment, renewal terms, human disposition, or Drata workspace export. Any assertions in repository registers or commitments remain repository-recorded assertions until the underlying dated evidence is examined by the accountable reviewers.

*This file remains a template and is not a signed legal review. Referenced by `legal/sub-processors.md` and/or `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`; existence is enforced by `scripts/validate_sub_processors.py`.*
