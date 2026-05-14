---
id: "LEGAL-REVIEW-PROCESS"
type: "internal_doc"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
audience: "internal"
wi: "WI-S14-008"
tags:
  - "legal"
  - "review-process"
  - "dpa"
  - "tia"
  - "legal-externo"
  - "playbook"
  - "s14"
supersedes: null
superseded_by: null
---

# Internal Legal Review Process
## CoreLink / HuGR Labs — Legal Externo Engagement Playbook

> **Audience**: CoreLink Owner, Compliance Officer, Privacy Officer, Sales Engineer. Internal use only.

---

## 1. Background

CoreLink has no in-house counsel (per ADR-0034 Tier-1 staffing constraint). All legal document review is performed by an external GDPR-experienced law firm (Legal externo). This playbook governs:

1. Initial Legal externo engagement (template DPA + TIA review).
2. Ongoing quarterly Legal review cycle.
3. Per-customer DPA engagement for enterprise customers.
4. WAIVER-S14-001 process if Legal externo review misses sprint timeline.
5. Lighthouse customer DPA signing process.

---

## 2. Initial Legal Externo Engagement (WI-S14-008)

### 2.1 Trigger

WI-S14-008 implementation complete; DPA Amendment template and TIA template published at:
- `legal/dpa-residency-amendment.md`
- `legal/tia-template.md`

### 2.2 Steps

```
D+0  Owner selects firm (per legal/legal-externo-engagement-contract.md §3)
D+0  NDA signed with selected firm
D+0  Documents shared: DPA template + TIA template + S-14 technical context
     (WI-S14-001..007 evidence pack summary)
D+0  Engagement contract signed (legal/legal-externo-engagement-contract.md)
D+0  Emit CloudEvent: corelink.legal.legal_externo_review.engaged

D+7  Kickoff call: CoreLink Owner + Compliance Officer + firm partner
D+14 Firm delivers initial redlines (DPA + TIA)
D+17 CoreLink reviews redlines (Owner + Compliance Officer + Privacy Officer)
D+21 CoreLink response sent to firm
D+28 Final redlines / sign-off letter (if no major issues)
D+30 TARGET: Legal externo sign-off letter committed

If D+30 missed → activate WAIVER-S14-001
     (specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md)

D+60 HARD DEADLINE: GA Evidence Gate — Legal externo sign-off must be complete
```

### 2.3 Deliverables Committed

On sign-off completion:
1. Commit redlined templates to `legal/dpa-residency-amendment-redlined-vN.md` and `legal/tia-template-redlined-vN.md`.
2. Commit sign-off letter to `specs/_audits/[DATE]-legal-externo-review-s14.md`.
3. Update `doc_status` in DPA + TIA to `LEGAL_REVIEWED`.
4. Emit CloudEvent: `corelink.legal.legal_externo_review.completed`.
5. Trigger WI-S14-008 §30 12th sign-off (Legal Counsel) — append to sign-off table.
6. If WAIVER-S14-001 active: update waiver status to `RESOLVED`.

---

## 3. Quarterly Legal Review Cycle

**Owner**: Privacy Officer (interim: Gustavo Schneiter).
**Trigger**: 3 months post-DPA + TIA Legal externo sign-off.
**Template**: `legal/quarterly-legal-review-template.md`.

### 3.1 Process

1. Privacy Officer initiates review per template checklist.
2. EDPB + regulatory monitoring (checklist §3.1–3.4).
3. DPA + TIA assessment (checklist §3.5–3.6).
4. Report drafted using template §4.
5. Sign-off: Privacy Officer + Compliance Officer.
6. If material changes: escalate to Legal externo (see §2 above, expedited engagement ~$5k-$10k / 2-3 weeks).
7. Commit report to `specs/_audits/[DATE]-quarterly-legal-review-q[N]-[YEAR].md`.
8. Emit CloudEvents.

**Missed review SLA**: Privacy Officer notified → SEV-3 alert → Owner escalation within 48h.

---

## 4. Per-Customer Enterprise DPA Process

When a prospect enterprise customer requests DPA signing:

```
Sales → Owner notification → DPA package assembled:
  - legal/dpa-residency-amendment.md (Legal externo reviewed version)
  - legal/tia-template.md (Legal externo reviewed version)
  - docs/customer/dpa-onboarding.md
  - Technical evidence pack (byok-fips-evidence.md + SOC 2 Type II under NDA)

NDA signed by customer → DPA package shared

Customer Legal review cycle (2-4 weeks typical)
  → Customer issues comments → CoreLink + Legal externo review
  → Iteration (standard clauses: Owner resolves; custom clauses: Legal externo re-review)

DPA signed by:
  - CoreLink: Gustavo Schneiter (Owner + Final Approver)
  - Customer: Authorised representative

Post-signing:
  - Commit signed DPA evidence to specs/_audits/[DATE]-[CUSTOMER-ID]-dpa-signed.md
  - Emit: corelink.legal.lighthouse_customer.dpa_signed (first customer)
  - Update corelink_legal_dpa_signed_total metric
```

### 4.1 Custom Clause Handling

If customer requests non-standard clauses:
1. Owner reviews — can Owner accept without Legal externo? Only for minor clarifications (e.g., DPO email update, contact details).
2. Material deviations → Legal externo re-review required. Timeline: +2-4 weeks. Cost: $3k-$8k depending on scope.
3. Document deviation as customer-specific addendum (committed separately; not merged into base template).
4. If deviation creates compliance gap → Compliance Officer + Privacy Officer approval required.

---

## 5. WAIVER-S14-001 Process

**Trigger**: Legal externo review not completed by D+30 (sprint window).

### 5.1 Activation

1. Owner confirms Legal externo review will miss D+30.
2. Owner creates WAIVER-S14-001 document: `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md`.
3. Sign-off: Owner + Final Approver + Compliance Officer + Privacy Officer + Legal Counsel (in-progress firm; partial sign-off).
4. Emit: `corelink.legal.waiver.activated{waiver_id: "WAIVER-S14-001"}`.

### 5.2 Compensating Controls During Waiver Period

- No enterprise customer DPA signed while waiver is active (unless Legal externo provides partial sign-off on specific sections).
- DPA + TIA templates marked `PENDING_LEGAL_REVIEW` in metadata.
- Sales to inform prospects of review-in-progress status.

### 5.3 Waiver Resolution

- **Expiry**: 90 days from waiver issuance.
- **Revalidation triggers**: (a) Legal externo sign-off letter received, OR (b) D+60 GA Evidence Gate.
- On resolution: update WAIVER-S14-001 `status` to `RESOLVED`; update DPA + TIA `doc_status` to `LEGAL_REVIEWED`.

---

## 6. Lighthouse Customer Engagement

**Objective**: 1 enterprise customer beta DPA signed demonstrating real-world contract closure capability.

### 6.1 Candidate Selection

Identify from current enterprise prospect pipeline:
- Candidate must be in active RFP cycle with DPA requirement.
- Candidate must have Legal team available for review cycle.
- Preferred: EU/EEA customer (GDPR/Schrems II validation) or Brazilian customer (LGPD validation).

### 6.2 Parallel Track

Lighthouse customer engagement runs parallel to Legal externo template review:
- D+5: Identify lighthouse customer candidate.
- D+7: Initial DPA package shared (under NDA).
- D+14–D+30: Customer Legal review cycle.
- D+30–D+60: DPA signed (target: GA Evidence Gate).

### 6.3 Success Criteria

- DPA signed by lighthouse customer (authorised representative).
- Committed report: `specs/_audits/[DATE]-lighthouse-customer-dpa-signed.md`.
- CloudEvent emitted: `corelink.legal.lighthouse_customer.dpa_signed`.
- Reference customer available for future prospect evidence (under NDA, with customer consent).

---

## 7. Contacts

| Role | Name | Contact |
|---|---|---|
| Owner / DPO | Gustavo Schneiter | `dpo@corelink.io` |
| Security Emergency | CoreLink Security | `security@corelink.io` |
| Legal externo | `[FIRM NAME]` | `[FIRM CONTACT — to be filled post-engagement]` |

---

## 8. Audit Emission Reference

| Event | Trigger |
|---|---|
| `corelink.legal.dpa.template.published` | DPA template v1.0.0 committed |
| `corelink.legal.tia.template.published` | TIA template v1.0.0 committed |
| `corelink.legal.legal_externo_review.engaged` | Engagement contract signed |
| `corelink.legal.legal_externo_review.completed` | Sign-off letter committed |
| `corelink.legal.lighthouse_customer.dpa_signed` | Lighthouse customer DPA signed |
| `corelink.legal.quarterly_review.completed` | Quarterly review report committed |
| `corelink.legal.waiver.activated` | WAIVER-S14-001 activated |
| `corelink.legal.waiver.resolved` | WAIVER-S14-001 resolved |

All events: 7-year retention (CTRL-AUDIT-005).

---

*Version 1.0.0 · 2026-05-14 · WI-S14-008 · Internal use only.*
