---
id: "LFPDPPP-MX-ENGAGEMENT-LETTER-TEMPLATE"
type: "engagement_template"
doc_status: "DRAFT"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
wi: "WI-S11-004"
tags:
  - "legal-externo"
  - "engagement"
  - "lfpdppp"
  - "mexico"
  - "mx-attorney"
  - "arco"
  - "s11"
  - "wave-23"
reviewers:
  - "Owner"
  - "Privacy Officer"
supersedes: null
superseded_by: null
related_audit: "specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md"
---

# LFPDPPP MX Attorney Review — Engagement Letter Template
## CoreLink / HuGR Labs — WI-S11-004 §6.1.4

> **Purpose:** Template engagement letter for retaining a Mexican attorney to review the CoreLink LFPDPPP compliance package consolidated in `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md`. Customize fields between `[BRACKETS]` and send under HuGR Labs letterhead.

---

## 1. Purpose

This document defines the engagement plan for retaining a Mexican attorney with LFPDPPP (Ley Federal de Protección de Datos Personales en Posesión de los Particulares) experience to review and render a written legal opinion on the following CoreLink legal artifacts produced under WI-S11-004 §6.1.4 ("legal local review per locale" SLA):

1. **`legal/privacy-notice/v1.0.0/es-MX.md`** — es-MX privacy notice (Aviso de Privacidad), 13 sections + DPO contact.
2. **`docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md`** — es breach-notification template (audit-chain integrity incident).
3. **`docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md`** — es breach-notification template (DSR pipeline degradation).

The engagement scoping packet is `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md` (§1-§8), which includes statutory citation inventory, breach template anchors, privacy notice metadata, DSR erasure flow, data residency posture, sub-processor disclosure, and 4 open questions for attorney opinion (Q1-Q4).

CoreLink has no in-house Mexican counsel. External legal review is mandatory before WI-S11-004 §6.1.4 can be marked CLOSED, and before any Mexican tenant onboards to CoreLink at GA.

---

## 2. Scope of Engagement

### 2.1 In-Scope

The attorney shall:

1. **Verify the statutory citations inventory** (§1 of the scoping packet — 11 citation sites covering LFPDPPP Arts. 8, 10, 20, 21, 22-26, 32, 36 + Reglamento Capítulo VII PPD). Confirm completeness or supply missing citations.

2. **Review the es-MX privacy notice** (`legal/privacy-notice/v1.0.0/es-MX.md`) and:
   - Verify Art. 8 + Art. 10 I, III, VI base-legal mapping in §4.
   - Verify the Arts. 22-36 ARCO header in §8 (including portabilidad bundling).
   - Verify retention table in §7 (Art. 26 exception doctrine).
   - Verify cross-border transfer disclosure in §6 (Reglamento Art. 67 form requirements).
   - Verify sub-processor disclosure in §5 (LFPDPPP Art. 36 Encargado disclosure).
   - Verify INAI contact in §13.
   - **Deliver a signed EVT-044 PDF** (per `legal/privacy-notice/REVIEW_PROCESS.md §2.3`) for upload to R2 `evidence-legal/v1.0.0-es-MX.pdf`.

3. **Review the 2 es breach-notification templates** (`docs/customer-comm/breach-notification/v1.0.0/es/{audit-chain-integrity-incident,dsr-pipeline-temporary-degradation}.md`) and:
   - Verify the joint LFPDPPP Art. 20-21 anchor with Art. 21 as primary (or correct).
   - Verify the Arts. 22-26 ARCO references in the customer-rights body.
   - Verify the Art. 32 ("plazo 20 días hábiles") response-time citation in the DSR-degradation template.
   - **Deliver signed EVT-044 PDFs** for each template (2 PDFs total).

4. **Verify the DSR erasure flow** (per §4 of the scoping packet) and confirm:
   - Arts. 22-26 ARCO coverage (A/R/C/O) plus Art. 27 (Oposición) plus Art. 8 (revocación del consentimiento) coverage.
   - Art. 32 SLA (20 / 15 días hábiles depending on right) is correctly applied per endpoint.
   - Art. 26 retention exception is correctly surfaced to titular in erasure response.

5. **Confirm data residency posture** (per §5 of the scoping packet) — that no MX data residency is required and that the SAM/ENAM/WNAM/WEUR regions with SCCs are LFPDPPP-compliant per Reglamento Arts. 66-70.

6. **Confirm sub-processor disclosure** (per §6 of the scoping packet) satisfies LFPDPPP Art. 36 Encargado disclosure obligation.

7. **Render written answers to 4 open questions** (per §7 of the scoping packet):
   - **Q1**: Joint Art. 20-21 anchor with Art. 21 primary, or different framing?
   - **Q2**: Is LFPDPPP Art. 10 VI defensible for "legitimate interest" purposes (abuse monitoring, fraud detection, platform security), or must this be re-anchored? (Resolves wave-17-ter audit D-8.)
   - **Q3**: Does the §6 cross-border transfer disclosure satisfy Reglamento Art. 67 form requirements, or must per-recipient itemization be added?
   - **Q4 (optional)**: Should privacy notice §13 cite Capítulo VII PPD + Capítulo VIII verificación explicitly?

### 2.2 Out-of-Scope

- Authoring final executed DPA for any specific Mexican customer (per-customer engagement separate).
- Mexican tax / fiscal record retention opinion (covered by Código Fiscal de la Federación; separate engagement if needed).
- Litigation advice / PPD defense (separate engagement if INAI proceeding initiated).
- Other locales (pt-BR LGPD review and en-US GDPR/CCPA review are separate engagements with BR / EU attorneys per WI-S11-004 §6.1.4).
- Implementation review (the attorney reviews legal artifacts; engineering implementation of ARCO endpoints is reviewed separately by the Architect + DPO).

---

## 3. Candidate Firms / Attorneys

CoreLink is evaluating the following types of Mexican LFPDPPP-experienced practitioners:

| Profile | Indicative rate (USD/hr) | Indicative turnaround | Notes |
|---|---|---|---|
| Mid-tier MX boutique data protection firm (DLA Piper México, Basham Ringe Correa, Sánchez Devanny equivalent) | 280-450 | 2-3 weeks | First choice; LFPDPPP + INAI proceedings depth |
| Solo practitioner with INAI procedural experience (ex-INAI staff or LFPDPPP author/commenter) | 180-280 | 2-4 weeks | Cost-effective; verify CV + recent ARCO precedent |
| Large international firm Mexican office (Baker McKenzie México, Hogan Lovells BSTL) | 450-650 | 1-2 weeks | Premium; only if cross-border GDPR coordination is needed |

**Selection criteria** (in priority order):

1. **LFPDPPP statutory + Reglamento depth** — must have authored at least one LFPDPPP article in a peer-reviewed journal or have at least one published INAI PPD/verificación case under their belt.
2. **INAI procedural experience** — has filed at least 3 ARCO petitions or 1 verificación procedure with INAI.
3. **SaaS / cross-border data flow familiarity** — has reviewed at least one cross-border SaaS privacy notice in the last 3 years (Cloudflare-grade infrastructure).
4. **Written-output discipline** — delivers redlined documents + structured opinion (not verbal Q&A only).
5. **Spanish-native** — opinion must be deliverable in Spanish (Mexican register, formal-legal); attorney may also be English-bilingual for cross-team coordination.

---

## 4. Deliverables

The attorney shall deliver, by the date set in §6 below:

1. **Written legal opinion** (PDF, ≥ 4 pages, Spanish, formal-legal register) addressing §1-§7 of the scoping packet, with explicit "compliant" / "compliant with redlines" / "non-compliant" verdict per section.

2. **Redlined es-MX privacy notice** (`legal/privacy-notice/v1.0.0/es-MX.md`) with tracked changes (Word .docx or markdown diff acceptable). If no changes, an explicit "no changes required" sign-off paragraph in the opinion.

3. **Redlined 2 es breach-notification templates** with tracked changes. If no changes, an explicit "no changes required" sign-off paragraph.

4. **Signed EVT-044 PDFs** (3 total: 1 per artifact) per `legal/privacy-notice/REVIEW_PROCESS.md §2.3`. Each PDF carries:
   - Attorney name + cédula profesional number.
   - Date of review.
   - Hash (BLAKE3 or SHA-256) of the reviewed file as it stood at review time.
   - Explicit attestation: *"He revisado este documento bajo la LFPDPPP, su Reglamento, los Lineamientos de Aviso de Privacidad (INAI), y los criterios INAI vigentes. El documento es [conforme / conforme con las redlines adjuntas / no conforme — ver opinión]."*
   - Attorney signature (electronic or wet) + date.

5. **List of material issues** (if any) requiring Owner attention with priority ranking (P0 must-fix-before-GA / P1 fix-pre-launch / P2 fix-within-90d-post-GA).

6. **Q&A reserve**: Up to 2 hours follow-up Q&A within 30 days of opinion delivery, billed at the same hourly rate, for clarification questions on the opinion.

---

## 5. Engagement Terms

### 5.1 Retention budget

| Component | Amount (USD) | Notes |
|---|---|---|
| Primary review (8-12 hours @ $[RATE]/hr) | $[2,500 - 4,500] | Per §8.2 of scoping packet |
| Q&A reserve (up to 2 hours @ $[RATE]/hr) | $[600 - 900] | Used only if needed |
| **Estimated total** | **$[3,100 - 5,400]** | Cap set in §5.4 below |

Owner sets the actual cap per `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md §8.3`.

### 5.2 Payment terms

50% deposit upon signed engagement letter; 50% upon delivery of the written opinion + EVT-044 PDFs.

### 5.3 Confidentiality

- All CoreLink artifacts shared with the attorney are **HuGR Labs Confidential — Attorney Work Product** under Mexican Código Federal de Procedimientos Civiles privileges.
- Attorney shall not disclose CoreLink artifacts or the engagement existence to any third party without HuGR Labs prior written consent, except as required by law or INAI proceeding.
- Attorney engagement letter shall include a standard NDA clause (or HuGR Labs shall provide one separately at attorney's preference).

### 5.4 Hard cap

Total engagement cost shall not exceed USD **$[CAP — Owner to set; suggested USD 5,500 ceiling per scoping packet §8.3]** without HuGR Labs written approval for overage. Any additional scope (e.g. PPD defense, second-locale review, multi-jurisdiction coordination) is a separate engagement.

### 5.5 Governing law + dispute resolution

This engagement is governed by the laws of [JURISDICTION — Owner choice; suggested: Mexican federal law if attorney is Mexico-based; or Delaware/California if HuGR Labs USA entity is the contracting party]. Disputes resolved by [VENUE — Owner choice; suggested: AAA-ICDR arbitration in Mexico City or San Francisco].

---

## 6. Timeline

| Step | Owner | Target date |
|---|---|---|
| Engagement letter sent to attorney | HuGR Labs (Gustavo Schneiter) | T+0 (this letter date) |
| Attorney accepts engagement; deposit paid | HuGR Labs + attorney | T+7d |
| Scoping packet + 3 review artifacts shared (BLAKE3 hashes recorded) | HuGR Labs | T+7d |
| Attorney review (8-12 hours spread over) | Attorney | T+21d |
| Written opinion + redlines + 3 EVT-044 PDFs delivered | Attorney | T+21d |
| Final payment | HuGR Labs | T+24d |
| Q&A reserve window (2 hours) | Attorney | T+24d to T+54d |
| Opinion absorbed into spec corpus (privacy notice corrections + metadata.yaml `legal_review.mx_attorney` update + `notice_text_hash` re-compute on any text change + DEBT-025 → CLOSED) | HuGR Labs orchestrator | wave-26 (T+28d) |

**Critical path constraint**: opinion must be delivered before the S-20 GA gate target `2026-10-01` (per `specs/04_sprints/S11/_review_R5_sonnet_round_2.md §3.5`). This engagement is independent of the S-20 GA gate timing — the attorney review can proceed in parallel with other GA-readiness streams.

---

## 7. Communication

| Party | Name | Email | Phone |
|---|---|---|---|
| HuGR Labs (Owner / Final Approver) | Gustavo Schneiter | gustavo@humangr.com | [TBD if requested] |
| HuGR Labs (Privacy Officer) | Gustavo Schneiter (interim) | privacy@hugr.dev | — |
| Attorney | [ATTORNEY NAME] | [ATTORNEY EMAIL] | [ATTORNEY PHONE] |
| Attorney's firm | [FIRM NAME] | [FIRM ADDRESS] | — |

Communication cadence: weekly written status check from attorney (≤ 2 paragraphs) for the duration of the review; ad-hoc Slack / email for clarification questions.

---

## 8. Acceptance + signature

**HuGR Labs side**: signed engagement letter under HuGR Labs letterhead.

**Attorney side**: signed acceptance returned to gustavo@humangr.com within 7 days, with:
- Confirmation of LFPDPPP / Reglamento / INAI Lineamientos credentials per §3 criteria.
- Cédula profesional number for inclusion on the EVT-044 PDFs.
- Estimated calendar dates per the §6 timeline.
- Preferred mode of redline delivery (Word .docx / markdown diff / inline PDF).

---

## 9. Cross-references

- **Scoping packet**: `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md` (§1-§8 — read before signing this engagement).
- **Review process SOP**: `legal/privacy-notice/REVIEW_PROCESS.md` (EVT-044 PDF SOP).
- **Privacy notice metadata**: `legal/privacy-notice/v1.0.0/metadata.yaml` (where attorney name + EVT-044 path will be recorded post-review).
- **Work item**: `specs/04_sprints/S11/work_items/WI-S11-004-privacy-notice-versioning-3-locales-diff-publication.md` §6.1.4 (legal local review SLA).
- **Debt-register row**: `specs/_audits/2026-05-15-debt-register.md DEBT-025` (this engagement; target wave-26 close).
- **Prior LGPD/GDPR engagement letter template (model precedent)**: `legal/legal-externo-engagement-contract.md` (WI-S14-008; DPA + TIA review engagement structure).
- **Wave-17-ter LFPDPPP audit (origin of D-8 deferral)**: `specs/_audits/2026-05-15-s11-legal-citation-revalidation.md §3 D-8`.
- **Wave-18 breach-notification templates audit**: `specs/_audits/2026-05-15-customer-breach-notification-templates.md §2.1` (Art. 21 anchor design rationale).

---

**End LFPDPPP MX Engagement Letter Template** — fill `[BRACKETS]`, attach scoping packet, send under HuGR Labs letterhead.
