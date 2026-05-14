---
id: "QUARTERLY-LEGAL-REVIEW-TEMPLATE"
type: "process_template"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
cadence: "Quarterly"
wi: "WI-S14-008"
tags:
  - "quarterly-review"
  - "legal"
  - "edpb"
  - "schrems-ii"
  - "gdpr"
  - "lgpd"
  - "s14"
reviewers:
  - "Compliance Officer"
  - "Privacy Officer"
  - "Legal Counsel (Legal externo)"
supersedes: null
superseded_by: null
---

# Quarterly Legal Review Template
## CoreLink DPA + Schrems II TIA Review — WI-S14-008

---

## 1. Overview

**Cadence**: Every 3 months post-DPA + TIA template ratification.
**Owner**: Privacy Officer (interim: Gustavo Schneiter).
**SLA**: Review completed within 30 days of quarter start. Missed review → SEV-3 alert.
**Scope**: Review EDPB guidance updates, Schrems II legal landscape, LGPD / GDPR enforcement, and assess whether DPA Amendment or TIA requires update.

---

## 2. Review Trigger Schedule

| Quarter | Start Date | Completion Deadline |
|---|---|---|
| Q3 2026 | 2026-07-01 | 2026-07-31 |
| Q4 2026 | 2026-10-01 | 2026-10-31 |
| Q1 2027 | 2027-01-01 | 2027-01-31 |
| Q2 2027 | 2027-04-01 | 2027-04-30 |
| *(Subsequent quarters)* | *(+ 3 months)* | *(30 days from start)* |

First review: 3 months after DPA + TIA Legal externo sign-off date.

---

## 3. Review Checklist

### 3.1 EDPB Guidance Monitoring

- [ ] Check EDPB website for new opinions, guidelines, or recommendations since last review.
- [ ] Assess applicability to CoreLink TIA supplementary measures (specifically EDPB Recommendations 01/2020 and any updates/revisions).
- [ ] Check for EDPB guidance on BYOK / encryption as Schrems II supplementary measure.
- [ ] Check for EDPB guidance on US surveillance law assessment updates.

### 3.2 Schrems II Legal Landscape

- [ ] Review CJEU / national DPA rulings since last review affecting Schrems II TIA validity.
- [ ] Assess EU-US Data Privacy Framework (DPF) status — any Schrems III challenge filed?
- [ ] Review FISA 702 reauthorisation status (US Congress).
- [ ] Review any CLOUD Act bilateral agreements relevant to EU-US data transfers.
- [ ] Review Cloudflare's DPA + sub-processor status for material changes.

### 3.3 GDPR Enforcement Review

- [ ] Review material GDPR enforcement decisions affecting DPA templates or TIA requirements.
- [ ] Review Irish DPC + other EU DPA decisions on data transfers (especially US transfer decisions).
- [ ] Assess whether any enforcement decision requires DPA Amendment update.

### 3.4 LGPD / ANPD Review (SAM Region)

- [ ] Review ANPD resolutions or opinions affecting LGPD Art. 33 §1 international transfers.
- [ ] Review LGPD Art. 48 breach notification enforcement decisions.
- [ ] Assess whether any ANPD guidance affects SAM region DPA commitments.

### 3.5 DPA Amendment Review (`legal/dpa-residency-amendment.md`)

- [ ] Verify residency commitments (Section 7 — 4 regions) still accurate vs Cloudflare infrastructure.
- [ ] Verify Sub-processor list (Section 8) — any new sub-processors added since last review?
- [ ] Verify breach notification SLA (Section 11) — 72h still compliant with current regulatory requirements?
- [ ] Verify governing law / jurisdiction (Section 15) — any legal changes?
- [ ] Verify technical security measures (Section 9) — any cryptographic standard changes?

### 3.6 TIA Review (`legal/tia-template.md`)

- [ ] Verify third-country law assessment (Section 3) still accurate.
- [ ] Verify supplementary measures effectiveness (Section 4) — any EDPB guidance affecting BYOK Use Case 6?
- [ ] Verify effectiveness assessment conclusion (Section 5) still valid.
- [ ] Verify EDPB compliance matrix (Appendix) complete.

### 3.7 Lighthouse Customer DPA Review

- [ ] If lighthouse customer DPA signed: verify no material deviations from template.
- [ ] If customer raised issues during review period: assess template update need.

---

## 4. Report Template

```markdown
# Quarterly Legal Review Report — Q[N] [YEAR]
**Review period**: [START] – [END]
**Completed**: [DATE]
**Reviewer**: [NAME, ROLE]
**WI reference**: WI-S14-008

## Summary
[1-3 sentence executive summary. Material changes required: YES/NO]

## EDPB Guidance
- New guidance: [YES/NO — list if yes]
- Impact on TIA: [NONE / MINOR / MATERIAL]
- Action required: [NONE / UPDATE TIA §X / ESCALATE TO LEGAL EXTERNO]

## Schrems II Landscape
- DPF status: [ACTIVE / CHALLENGED — details]
- FISA 702 status: [details if changed]
- Impact: [NONE / MINOR / MATERIAL]

## GDPR Enforcement
- Material decisions: [list or NONE]
- Impact on DPA template: [NONE / UPDATE §X]

## LGPD / ANPD (SAM region)
- Material ANPD resolutions: [list or NONE]
- Impact: [NONE / UPDATE §X]

## DPA Amendment Status
- Material changes required: [YES/NO]
- Sections affected: [list or N/A]
- Action: [NONE / MINOR EDIT (Owner) / LEGAL EXTERNO RE-REVIEW REQUIRED]

## TIA Status
- Material changes required: [YES/NO]
- Sections affected: [list or N/A]
- Action: [NONE / MINOR EDIT (Owner) / LEGAL EXTERNO RE-REVIEW REQUIRED]

## Sub-processor Changes
- New sub-processors since last review: [YES/NO — list if yes]
- DPA Section 8 update required: [YES/NO]
- Customer 30-day advance notice sent: [YES/NO/N/A]

## Actions
| Action | Owner | Deadline |
|---|---|---|
| [action] | [owner] | [date] |

## Next Review
Scheduled: [DATE]

## Sign-off
- [ ] Privacy Officer: [NAME] · [DATE]
- [ ] Compliance Officer: [NAME] · [DATE]
- [ ] Legal Counsel (if material changes): [NAME] · [DATE]
```

---

## 5. Escalation

If the quarterly review identifies **material changes** requiring DPA or TIA update:

1. **Minor edit** (clarification, non-substantive): Owner edits template + Privacy Officer sign-off.
2. **Moderate change** (substantive clause update): Privacy Officer + Compliance Officer review + commit.
3. **Material change** (core supplementary measure affected, governing law change, major regulatory shift): Re-engage Legal externo firm for expedited review. Estimated timeline: 2-3 weeks. Budget: $5k-$10k from quarterly review budget.

Trigger for re-engagement: EDPB guidance explicitly invalidates BYOK Use Case 6 approach, or EU-US DPF invalidated (Schrems III).

---

## 6. Audit Emission

Per legal milestone, emit CloudEvents to audit chain (7-year retention, CTRL-AUDIT-005):

- `corelink.legal.quarterly_review.started` — quarter start
- `corelink.legal.quarterly_review.completed` — review completion
- `corelink.legal.dpa.template.updated` — if DPA template updated
- `corelink.legal.tia.template.updated` — if TIA template updated

---

*Version 1.0.0 · 2026-05-14 · WI-S14-008.*
