---
id: "LEGAL-EXTERNO-ENGAGEMENT-CONTRACT"
type: "engagement_template"
doc_status: "DRAFT"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
wi: "WI-S14-008"
tags:
  - "legal-externo"
  - "engagement"
  - "gdpr"
  - "schrems-ii"
  - "tia"
  - "dpa"
  - "s14"
reviewers:
  - "Owner"
  - "Compliance Officer"
supersedes: null
superseded_by: null
---

# Legal Externo Review Engagement Plan
## CoreLink / HuGR Labs — WI-S14-008

---

## 1. Purpose

This document defines the engagement plan for retaining an external GDPR-experienced law firm to review and redline the following CoreLink legal templates produced under WI-S14-008:

1. `legal/dpa-residency-amendment.md` — DPA Amendment Template (15 sections + 3 appendices)
2. `legal/tia-template.md` — Schrems II Transfer Impact Assessment (TIA), EDPB Recommendations 01/2020

CoreLink has no in-house counsel (per ADR-0034). External legal review is mandatory before any enterprise customer DPA is signed.

---

## 2. Scope of Engagement

### 2.1 In-Scope

- Review and redline of DPA Amendment Template (`legal/dpa-residency-amendment.md`):
  - Verify GDPR Art. 28 compliance (data processing agreement requirements).
  - Verify GDPR Art. 46 transfer mechanism adequacy.
  - Verify LGPD Art. 33 §1 compliance for SAM region.
  - Verify residency commitment language (Section 7, 4 regions).
  - Verify sub-processor disclosure (Section 8, Cloudflare).
  - Verify breach notification SLA 72h (Section 11, GDPR Art. 33 / LGPD Art. 48).
  - Verify governing law + jurisdiction clauses (Section 15).
  - Verify data subject rights coverage (Section 10).

- Review and redline of Schrems II TIA Template (`legal/tia-template.md`):
  - Verify EDPB Recommendations 01/2020 framework adherence (Steps 1-6).
  - Verify third-country law assessment (Section 3: FISA 702, EO 12333, CLOUD Act).
  - Verify supplementary measures adequacy (Section 4: technical + organisational + contractual).
  - Verify BYOK effectiveness argument (Section 5.1).
  - Assess EDPB §83 Use Case 6 applicability.

- Delivery of:
  - Redlined DPA Amendment Template (tracked changes).
  - Redlined TIA Template (tracked changes).
  - Sign-off letter confirming Legal externo review completion.
  - List of material issues requiring owner attention.

### 2.2 Out-of-Scope

- Authoring final executed DPA for any specific customer (per-customer engagement is separate).
- LGPD DPO appointment opinion.
- Customer-specific addenda.
- Litigation advice.

---

## 3. Candidate Firms

CoreLink is evaluating the following GDPR-experienced firms with international data protection capability:

| Firm | Strengths | Notes |
|---|---|---|
| **Schellman Legal** | Privacy + compliance + SOC 2 ecosystem | Already engaged for SOC 2 Type II (synergy) |
| **Cooley LLP** | Tech + startup + GDPR + Schrems II | Strong transatlantic capability |
| **DLA Piper** | Global; GDPR + LGPD + cross-border transfers | SAM region expertise |
| **Bird & Bird** | EU-focused; GDPR + Schrems II specialist | Recommended by EDPB practitioners |
| **Fenwick & West** | Tech + startup + privacy | US base + GDPR capability |
| **Latham & Watkins** | Global; enterprise + data protection | Broader cost |

**Selection criteria**: GDPR Schrems II TIA experience, LGPD capability, startup-friendly engagement model, availability within 2-week engagement start.

**Selected firm**: `[TO BE FILLED UPON SELECTION]`

---

## 4. Budget

| Item | Estimated Cost |
|---|---|
| DPA Amendment review + redline | $7,500 – $15,000 |
| TIA review + redline | $5,000 – $10,000 |
| Sign-off letter + coordination | $2,500 – $5,000 |
| **Total** | **$15,000 – $30,000** |

Budget approved per WI-S14-008 §22. Cost regression gate in CI: total ≤ $30,000.

---

## 5. Timeline

| Milestone | Target Date | Notes |
|---|---|---|
| D+0 | Firm selected + engagement contract signed | Parallel to WI-S14-008 implementation start |
| D+0 | Templates v1.0.0 shared with firm | `legal/dpa-residency-amendment.md` + `legal/tia-template.md` |
| D+14 | First round of feedback / redlines from firm | Kickoff + firm review period |
| D+21 | CoreLink response to redlines | Owner + Compliance Officer review |
| D+28 | Second round (if needed) | Iteration cycle |
| D+30 | Sign-off letter issued (target — sprint window) | If achieved → WAIVER-S14-001 not needed |
| D+60 | Final sign-off letter committed (GA Evidence Gate) | Hard deadline for WAIVER-S14-001 expiry |

If Legal externo review is not completed by D+30, WAIVER-S14-001 is activated (`specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md`).

---

## 6. Deliverables

| # | Deliverable | Format | Committed To |
|---|---|---|---|
| 1 | Redlined DPA Amendment Template | `.docx` + `.md` (tracked changes) | `legal/dpa-residency-amendment-redlined-v[N].md` |
| 2 | Redlined TIA Template | `.docx` + `.md` (tracked changes) | `legal/tia-template-redlined-v[N].md` |
| 3 | Legal externo sign-off letter | PDF + Markdown | `specs/_audits/2026-XX-XX-legal-externo-review-s14.md` |
| 4 | Material issues summary | Markdown | Included in sign-off letter |

---

## 7. Engagement Contract Terms (Placeholder — Legal Review Required)

> The following is a structural placeholder for engagement contract terms. Legal externo firm will provide their standard engagement letter / retainer agreement. Owner reviews + signs.

- **Confidentiality**: NDA to be signed by firm prior to document sharing.
- **Privilege**: Engagement under attorney-client privilege where applicable.
- **Scope**: Limited to items in Section 2.1 above.
- **Fee structure**: Fixed fee preferred; time-and-materials fallback.
- **Deliverables**: Per Section 6 above.
- **Governing law**: `[TO BE DETERMINED WITH FIRM]`
- **Cancellation**: 14-day written notice; pro-rated fee for work completed.

---

## 8. Post-Engagement

After Legal externo review completion:

1. Commit redlined templates to `legal/`.
2. Update `doc_status` in `legal/dpa-residency-amendment.md` and `legal/tia-template.md` from `PENDING_LEGAL_REVIEW` to `LEGAL_REVIEWED`.
3. Commit sign-off letter to `specs/_audits/`.
4. Trigger WI-S14-008 12th sign-off Legal Counsel (§30 of WI-S14-008 spec).
5. Emit CloudEvent: `corelink.legal.legal_externo_review.completed`.
6. Update WAIVER-S14-001 status if active (mark resolved).

---

*Version 1.0.0 · 2026-05-14 · WI-S14-008.*
