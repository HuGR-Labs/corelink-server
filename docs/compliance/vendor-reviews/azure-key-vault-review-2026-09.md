# Vendor Legal-Review Record — Microsoft Corporation (Azure)

> STATUS: TEMPLATE — formal Legal review pending; dated public-source assessment below is not a completed review.

| Field | Value |
|---|---|
| Vendor | Microsoft Corporation (Azure) |
| Sub-processor id | `azure-key-vault` |
| Review date | `TBD (formal Legal review not completed)` |
| Public-source assessment date | 2026-09-22 (does not replace the formal review) |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)`; no formal reviewer or decision is recorded. |
| DPA reference | <https://www.microsoft.com/licensing/docs/view/Microsoft-Products-and-Services-Data-Protection-Addendum-DPA> |
| DPA status | Evidence insufficient — current CoreLink customer agreement, accepted DPA, and effective version are not in this packet. |
| SCC / transfer mechanism | Evidence insufficient — applicable account agreement and transfer mechanism are not in this packet. |
| Schrems II TIA | Evidence insufficient — no account-specific transfer assessment is in this packet. |
| Data categories processed | encrypted-blobs (envelope keys only) |
| Data residency / region | Customer Azure subscription / selected region; actual location not verified |
| Sub-processor flow-down | Evidence insufficient — no applicable subprocessor terms or dated change evidence were retrieved. |
| Certifications verified | Not verified for CoreLink — register lists SOC 2 Type II, ISO 27001/17/18, FedRAMP High, and FIPS scopes; no current STP report package was retrieved. |
| Review outcome | `TBD (formal Legal outcome pending)`; public-source assessment: **Evidence insufficient — not approved**. |
| Human decision | Pending formal human review; no approval decision is recorded. |
| Signer | `TBD (named human signer pending)`; no signature is recorded. |
| Conditions / follow-ups | VP-Sec and Legal/Privacy must obtain current customer/account evidence, complete the refresh fields, decide, and record the named reviewer and signer. |
| Next review due | `2026-08-15` (formal due date; overdue 38 calendar days at assessment capture; not reset). |

## Public-source assessment (2026-09-22)

**Public statements observed.** Microsoft’s Product Terms page identifies the May 2026 edition as the most recent public DPA version and says the DPA defines processing/security terms for subscribed products. Microsoft describes customer access to audit reports via its Service Trust Portal; some artifacts are access-gated.

| Refresh item | Current public evidence and remaining gap |
|---|---|
| Scope | Public materials describe Microsoft online services. CoreLink subscription, Key Vault configuration, service scope, and exclusions have not been reconciled. |
| Subprocessors | Microsoft publishes Online Services subprocessor information. A dated account-applicable snapshot and change disposition for CoreLink were not captured. |
| Residency | Public documentation is service-specific. CoreLink vault region, replication, key settings, and transfer assessment are unverified. |
| Security changes / current assurance | Microsoft describes audit materials in the Service Trust Portal. No current CoreLink-accessible report, coverage period, Azure Key Vault scope, or exceptions were retrieved. |
| DPA changes | The public page lists a May 2026 DPA edition. Whether it governs CoreLink or changed its terms cannot be established without the executed version. |
| Renewal date | CoreLink Azure agreement/subscription, renewal date, and notice window remain unverified. |
| Human decision / signer | No human approval or signature is recorded. Evidence is insufficient; the outcome remains not approved and pending Legal review. |

### Official vendor sources consulted on 2026-09-22

- [Microsoft Products and Services Data Protection Addendum](https://www.microsoft.com/licensing/docs/view/Microsoft-Products-and-Services-Data-Protection-Addendum-DPA)
- [Microsoft Compliance overview (includes Service Trust Portal)](https://learn.microsoft.com/en-us/compliance/)
- [Microsoft Online Services subprocessor information](https://www.microsoft.com/en-us/trust-center/privacy/data-access)

## Formal review cadence

The source register records the last formal review as **2026-05-15** and the formal next-review due date as **2026-08-15**. On the assessment capture date, 2026-09-22, the recorded due date is **38 calendar days overdue**. This public-source assessment does not satisfy the formal review, alter its due date, or start a new cadence. The register’s quarterly cadence, 90-day threshold, and explicit scheduled date are left as recorded; no replacement due date is inferred.

Public vendor material does not establish CoreLink’s executed terms, account settings, customer-restricted reports, applicable subprocessor snapshot, current data flow, completed transfer assessment, renewal terms, human disposition, or Drata workspace export. Any assertions in repository registers or commitments remain repository-recorded assertions until the underlying dated evidence is examined by the accountable reviewers.

*This file remains a template and is not a signed legal review. Referenced by `legal/sub-processors.md` and/or `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`; existence is enforced by `scripts/validate_sub_processors.py`.*
